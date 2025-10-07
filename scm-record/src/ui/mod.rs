use components::section;
use ratatui::backend::{Backend, TestBackend};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::any::Any;
use std::collections::HashSet;
use std::fmt::Debug;
use std::{io, iter, mem, panic};
use tracing::warn;

pub mod components;
pub mod event;
pub mod input;
pub mod terminal;
#[cfg(test)]
mod tests;

use crate::consts::ENV_VAR_DEBUG_UI;
use crate::render::{DrawnRect, DrawnRects, Rect, Viewport};
use crate::types::{ChangeType, Commit, RecordError, RecordState, Tristate};
use crate::ui::components::app::{AppDebugInfo, AppView, SelectionKey};
use crate::ui::components::commit::{CommitMessageView, CommitView, CommitViewMode};
use crate::ui::components::dialog::{self, HelpDialog, QuitDialog};
use crate::ui::components::file::{FileKey, FileView};
use crate::ui::components::line::LineKey;
use crate::ui::components::widgets::{TristateBox, TristateIconStyle};
use crate::ui::components::ComponentId;
use crate::ui::input::TestingScreenshot;
use crate::util::UsizeExt;
use crate::{File, FileMode, Section, SectionChangedLine};

#[derive(Clone, Debug, PartialEq, Eq)]
enum StateUpdate {
    None,
    SetQuitDialog(Option<QuitDialog>),
    QuitAccept,
    QuitCancel,
    SetHelpDialog(Option<HelpDialog>),
    TakeScreenshot(TestingScreenshot),
    Redraw,
    EnsureSelectionInViewport,
    ScrollTo(isize),
    SelectItem {
        selection_key: SelectionKey,
        ensure_in_viewport: bool,
    },
    ToggleItem(SelectionKey),
    ToggleItemAndAdvance(SelectionKey, SelectionKey),
    ToggleAll,
    ToggleAllUniform,
    SetExpandItem(SelectionKey, bool),
    ToggleExpandItem(SelectionKey),
    ToggleExpandAll,
    ToggleCommitViewMode,
    EditCommitMessage {
        commit_idx: usize,
    },
}

#[allow(clippy::enum_variant_names)]
enum ToggleSideEffects {
    ToggledModeChangeSection(section::SectionKey, FileMode, FileMode, bool),
    ToggledChangedSection(section::SectionKey, bool),
    ToggledChangedLine(LineKey, bool),
}

/// UI component to record the user's changes.
pub struct Recorder<'state, 'input> {
    state: RecordState<'state>,
    input: &'input mut dyn input::RecordInput,
    pending_events: Vec<event::Event>,
    use_unicode: bool,
    commit_view_mode: CommitViewMode,
    expanded_items: HashSet<SelectionKey>,
    selection_key: SelectionKey,
    focused_commit_idx: usize,
    quit_dialog: Option<dialog::QuitDialog>,
    help_dialog: Option<dialog::HelpDialog>,
    scroll_offset_y: isize,
}

impl<'state, 'input> Recorder<'state, 'input> {
    /// Constructor.
    pub fn new(mut state: RecordState<'state>, input: &'input mut dyn input::RecordInput) -> Self {
        // Ensure that there are at least two commits.
        state.commits.extend(
            iter::repeat_with(Commit::default).take(2_usize.saturating_sub(state.commits.len())),
        );
        if state.commits.len() > 2 {
            unimplemented!("more than two commits");
        }

        let mut recorder = Self {
            state,
            input,
            pending_events: Default::default(),
            use_unicode: true,
            commit_view_mode: CommitViewMode::Inline,
            expanded_items: Default::default(),
            selection_key: SelectionKey::None,
            focused_commit_idx: 0,
            quit_dialog: None,
            help_dialog: None,
            scroll_offset_y: 0,
        };
        recorder.expand_initial_items();
        recorder
    }

    /// Run the terminal user interface and have the user interactively select
    /// changes.
    pub fn run(self) -> Result<RecordState<'state>, RecordError> {
        #[cfg(feature = "debug")]
        if std::env::var_os(crate::consts::ENV_VAR_DUMP_UI_STATE).is_some() {
            let ui_state =
                serde_json::to_string_pretty(&self.state).map_err(RecordError::SerializeJson)?;
            std::fs::write(crate::consts::DUMP_UI_STATE_FILENAME, ui_state)
                .map_err(RecordError::WriteFile)?;
        }

        match self.input.terminal_kind() {
            terminal::TerminalKind::Crossterm => self.run_crossterm(),
            terminal::TerminalKind::Testing { width, height } => self.run_testing(width, height),
        }
    }

    /// Run the recorder UI using `crossterm` as the backend connected to stdout.
    fn run_crossterm(self) -> Result<RecordState<'state>, RecordError> {
        terminal::set_up_crossterm()?;
        terminal::install_panic_hook();
        let backend = CrosstermBackend::new(io::stdout());
        let mut term = Terminal::new(backend).map_err(RecordError::SetUpTerminal)?;
        term.clear().map_err(RecordError::RenderFrame)?;
        let result = self.run_inner(&mut term);
        terminal::clean_up_crossterm()?;
        result
    }
    fn run_testing(self, width: usize, height: usize) -> Result<RecordState<'state>, RecordError> {
        let backend = TestBackend::new(width.clamp_into_u16(), height.clamp_into_u16());
        let mut term = Terminal::new(backend).map_err(RecordError::SetUpTerminal)?;
        self.run_inner(&mut term)
    }

    fn run_inner(
        mut self,
        term: &mut Terminal<impl Backend + Any>,
    ) -> Result<RecordState<'state>, RecordError> {
        self.selection_key = self.first_selection_key();
        let debug = if cfg!(feature = "debug") {
            std::env::var_os(ENV_VAR_DEBUG_UI).is_some()
        } else {
            false
        };

        'outer: loop {
            let app = self.make_app(None);
            let term_height = usize::from(term.get_frame().area().height);

            let mut drawn_rects: Option<DrawnRects<ComponentId>> = None;
            term.draw(|frame| {
                drawn_rects = Some(Viewport::<ComponentId>::render_top_level(
                    frame,
                    0,
                    self.scroll_offset_y,
                    &app,
                ));
            })
            .map_err(RecordError::RenderFrame)?;
            let drawn_rects = drawn_rects.unwrap();

            // Dump debug info. We may need to use information about the
            // rendered app, so we perform a re-render here.
            if debug {
                let debug_info = AppDebugInfo {
                    term_height,
                    scroll_offset_y: self.scroll_offset_y,
                    selection_key: self.selection_key,
                    selection_key_y: self.selection_key_y(&drawn_rects, self.selection_key),
                    drawn_rects: drawn_rects.clone().into_iter().collect(),
                };
                let debug_app = AppView {
                    debug_info: Some(debug_info),
                    ..app.clone()
                };
                term.draw(|frame| {
                    Viewport::<ComponentId>::render_top_level(
                        frame,
                        0,
                        self.scroll_offset_y,
                        &debug_app,
                    );
                })
                .map_err(RecordError::RenderFrame)?;
            }

            let events = if self.pending_events.is_empty() {
                self.input.next_events()?
            } else {
                // FIXME: the pending events should be applied without redrawing
                // the screen, as otherwise there may be a flash of content
                // containing the screen contents before the event is applied.
                mem::take(&mut self.pending_events)
            };
            for event in events {
                match self.handle_event(event, term_height, &drawn_rects)? {
                    StateUpdate::None => {}
                    StateUpdate::SetQuitDialog(quit_dialog) => {
                        self.quit_dialog = quit_dialog;
                    }
                    StateUpdate::SetHelpDialog(help_dialog) => {
                        self.help_dialog = help_dialog;
                    }
                    StateUpdate::QuitAccept => {
                        if self.help_dialog.is_some() {
                            self.help_dialog = None;
                        } else {
                            break 'outer;
                        }
                    }
                    StateUpdate::QuitCancel => return Err(RecordError::Cancelled),
                    StateUpdate::TakeScreenshot(screenshot) => {
                        let backend: &dyn Any = term.backend();
                        let test_backend = backend
                            .downcast_ref::<TestBackend>()
                            .expect("TakeScreenshot event generated for non-testing backend");
                        screenshot.set(terminal::buffer_view(test_backend.buffer()));
                    }
                    StateUpdate::Redraw => {
                        term.clear().map_err(RecordError::RenderFrame)?;
                    }
                    StateUpdate::EnsureSelectionInViewport => {
                        if let Some(scroll_offset_y) =
                            self.ensure_in_viewport(term_height, &drawn_rects, self.selection_key)
                        {
                            self.scroll_offset_y = scroll_offset_y;
                        }
                    }
                    StateUpdate::ScrollTo(scroll_offset_y) => {
                        self.scroll_offset_y = scroll_offset_y.clamp(0, {
                            let DrawnRect { rect, timestamp: _ } = drawn_rects[&ComponentId::App];
                            rect.height.unwrap_isize() - 1
                        });
                    }
                    StateUpdate::SelectItem {
                        selection_key,
                        ensure_in_viewport,
                    } => {
                        self.selection_key = selection_key;
                        self.expand_item_ancestors(selection_key);
                        if ensure_in_viewport {
                            self.pending_events
                                .push(event::Event::EnsureSelectionInViewport);
                        }
                    }
                    StateUpdate::ToggleItem(selection_key) => {
                        self.toggle_item(selection_key)?;
                    }
                    StateUpdate::ToggleItemAndAdvance(selection_key, new_key) => {
                        self.toggle_item(selection_key)?;
                        self.selection_key = new_key;
                        self.pending_events
                            .push(event::Event::EnsureSelectionInViewport);
                    }
                    StateUpdate::ToggleAll => {
                        self.toggle_all();
                    }
                    StateUpdate::ToggleAllUniform => {
                        self.toggle_all_uniform();
                    }
                    StateUpdate::SetExpandItem(selection_key, is_expanded) => {
                        self.set_expand_item(selection_key, is_expanded);
                        self.pending_events
                            .push(event::Event::EnsureSelectionInViewport);
                    }
                    StateUpdate::ToggleExpandItem(selection_key) => {
                        self.toggle_expand_item(selection_key)?;
                        self.pending_events
                            .push(event::Event::EnsureSelectionInViewport);
                    }
                    StateUpdate::ToggleExpandAll => {
                        self.toggle_expand_all()?;
                        self.pending_events
                            .push(event::Event::EnsureSelectionInViewport);
                    }
                    StateUpdate::ToggleCommitViewMode => {
                        self.commit_view_mode = match self.commit_view_mode {
                            CommitViewMode::Inline => CommitViewMode::Adjacent,
                            CommitViewMode::Adjacent => CommitViewMode::Inline,
                        };
                    }
                    StateUpdate::EditCommitMessage { commit_idx } => {
                        self.pending_events.push(event::Event::Redraw);
                        self.edit_commit_message(commit_idx)?;
                    }
                }
            }
        }

        Ok(self.state)
    }

    fn make_app(&'state self, debug_info: Option<AppDebugInfo>) -> AppView<'state> {
        let RecordState {
            is_read_only,
            commits,
            files,
        } = &self.state;
        let commit_views = match self.commit_view_mode {
            CommitViewMode::Inline => {
                vec![CommitView {
                    debug_info: None,
                    commit_message_view: CommitMessageView {
                        commit_idx: self.focused_commit_idx,
                        commit: &commits[self.focused_commit_idx],
                    },
                    file_views: self.make_file_views(
                        self.focused_commit_idx,
                        files,
                        &debug_info,
                        *is_read_only,
                    ),
                }]
            }

            CommitViewMode::Adjacent => commits
                .iter()
                .enumerate()
                .map(|(commit_idx, commit)| CommitView {
                    debug_info: None,
                    commit_message_view: CommitMessageView { commit_idx, commit },
                    file_views: self.make_file_views(commit_idx, files, &debug_info, *is_read_only),
                })
                .collect(),
        };
        AppView {
            debug_info: None,
            commit_view_mode: self.commit_view_mode,
            commit_views,
            quit_dialog: self.quit_dialog.clone(),
            help_dialog: self.help_dialog.clone(),
        }
    }

    fn make_file_views(
        &'state self,
        commit_idx: usize,
        files: &'state [File<'state>],
        debug_info: &Option<AppDebugInfo>,
        is_read_only: bool,
    ) -> Vec<FileView<'state>> {
        files
            .iter()
            .enumerate()
            .map(|(file_idx, file)| {
                let file_key = FileKey {
                    commit_idx,
                    file_idx,
                };
                let file_toggled = self.file_tristate(file_key).unwrap();
                let file_expanded = self.file_expanded(file_key);
                let is_focused = match self.selection_key {
                    SelectionKey::None | SelectionKey::Section(_) | SelectionKey::Line(_) => false,
                    SelectionKey::File(selected_file_key) => file_key == selected_file_key,
                };
                FileView {
                    debug: debug_info.is_some(),
                    file_key,
                    toggle_box: TristateBox {
                        use_unicode: self.use_unicode,
                        id: ComponentId::ToggleBox(SelectionKey::File(file_key)),
                        icon_style: TristateIconStyle::Check,
                        tristate: file_toggled,
                        is_focused,
                        is_read_only,
                    },
                    expand_box: TristateBox {
                        use_unicode: self.use_unicode,
                        id: ComponentId::ExpandBox(SelectionKey::File(file_key)),
                        icon_style: TristateIconStyle::Expand,
                        tristate: file_expanded,
                        is_focused,
                        is_read_only: false,
                    },
                    is_header_selected: is_focused,
                    old_path: file.old_path.as_deref(),
                    path: &file.path,
                    section_views: {
                        let mut section_views = Vec::new();
                        let total_num_sections = file.sections.len();
                        let total_num_editable_sections = file
                            .sections
                            .iter()
                            .filter(|section| section.is_editable())
                            .count();

                        let mut line_num = 1;
                        let mut editable_section_num = 0;
                        for (section_idx, section) in file.sections.iter().enumerate() {
                            let section_key = section::SectionKey {
                                commit_idx,
                                file_idx,
                                section_idx,
                            };
                            let section_toggled = self.section_tristate(section_key).unwrap();
                            let section_expanded = Tristate::from(
                                self.expanded_items
                                    .contains(&SelectionKey::Section(section_key)),
                            );
                            let is_focused = match self.selection_key {
                                SelectionKey::None
                                | SelectionKey::File(_)
                                | SelectionKey::Line(_) => false,
                                SelectionKey::Section(selection_section_key) => {
                                    selection_section_key == section_key
                                }
                            };
                            if section.is_editable() {
                                editable_section_num += 1;
                            }
                            section_views.push(section::SectionView {
                                use_unicode: self.use_unicode,
                                is_read_only,
                                section_key,
                                toggle_box: TristateBox {
                                    use_unicode: self.use_unicode,
                                    is_read_only,
                                    id: ComponentId::ToggleBox(SelectionKey::Section(section_key)),
                                    tristate: section_toggled,
                                    icon_style: TristateIconStyle::Check,
                                    is_focused,
                                },
                                expand_box: TristateBox {
                                    use_unicode: self.use_unicode,
                                    is_read_only: false,
                                    id: ComponentId::ExpandBox(SelectionKey::Section(section_key)),
                                    tristate: section_expanded,
                                    icon_style: TristateIconStyle::Expand,
                                    is_focused,
                                },
                                selection: match self.selection_key {
                                    SelectionKey::None | SelectionKey::File(_) => None,
                                    SelectionKey::Section(selected_section_key) => {
                                        if selected_section_key == section_key {
                                            Some(section::SectionSelection::SectionHeader)
                                        } else {
                                            None
                                        }
                                    }
                                    SelectionKey::Line(LineKey {
                                        commit_idx,
                                        file_idx,
                                        section_idx,
                                        line_idx,
                                    }) => {
                                        let selected_section_key = section::SectionKey {
                                            commit_idx,
                                            file_idx,
                                            section_idx,
                                        };
                                        if selected_section_key == section_key {
                                            Some(section::SectionSelection::ChangedLine(line_idx))
                                        } else {
                                            None
                                        }
                                    }
                                },
                                total_num_sections,
                                editable_section_num,
                                total_num_editable_sections,
                                section,
                                line_start_num: line_num,
                            });

                            line_num += match section {
                                Section::Unchanged { lines } => lines.len(),
                                Section::Changed { lines } => lines
                                    .iter()
                                    .filter(|changed_line| match changed_line.change_type {
                                        ChangeType::Added => false,
                                        ChangeType::Removed => true,
                                    })
                                    .count(),
                                Section::FileMode { .. } | Section::Binary { .. } => 0,
                            };
                        }
                        section_views
                    },
                }
            })
            .collect()
    }

    fn handle_event(
        &self,
        event: event::Event,
        term_height: usize,
        drawn_rects: &DrawnRects<ComponentId>,
    ) -> Result<StateUpdate, RecordError> {
        let state_update = match (&self.quit_dialog, event) {
            (_, event::Event::None) => StateUpdate::None,
            (_, event::Event::Redraw) => StateUpdate::Redraw,
            (_, event::Event::EnsureSelectionInViewport) => StateUpdate::EnsureSelectionInViewport,

            (
                _,
                event::Event::Help
                | event::Event::QuitEscape
                | event::Event::QuitCancel
                | event::Event::ToggleItem
                | event::Event::ToggleItemAndAdvance,
            ) if self.help_dialog.is_some() => StateUpdate::SetHelpDialog(None),
            (_, event::Event::Help) => StateUpdate::SetHelpDialog(Some(HelpDialog())),

            // Confirm the changes.
            (None, event::Event::QuitAccept) => StateUpdate::QuitAccept,
            // Ignore the confirm action if the quit dialog is open.
            (Some(_), event::Event::QuitAccept) => StateUpdate::None,

            // Render quit dialog if the user made changes.
            (None, event::Event::QuitCancel | event::Event::QuitInterrupt) => {
                let num_commit_messages = self.num_user_commit_messages()?;
                let num_changed_files = self.num_user_file_changes()?;
                if num_commit_messages > 0 || num_changed_files > 0 {
                    StateUpdate::SetQuitDialog(Some(QuitDialog {
                        num_commit_messages,
                        num_changed_files,
                        focused_button: dialog::QuitDialogButtonId::Quit,
                    }))
                } else {
                    StateUpdate::QuitCancel
                }
            }
            // If pressing quit again, or escape, while the dialog is open, close it.
            (Some(_), event::Event::QuitCancel | event::Event::QuitEscape) => {
                StateUpdate::SetQuitDialog(None)
            }
            // If pressing ctrl-c again wile the dialog is open, force quit.
            (Some(_), event::Event::QuitInterrupt) => StateUpdate::QuitCancel,
            // Select left quit dialog button.
            (Some(quit_dialog), event::Event::FocusOuter { .. }) => {
                StateUpdate::SetQuitDialog(Some(QuitDialog {
                    focused_button: dialog::QuitDialogButtonId::GoBack,
                    ..quit_dialog.clone()
                }))
            }
            // Select right quit dialog button.
            (Some(quit_dialog), event::Event::FocusInner) => {
                StateUpdate::SetQuitDialog(Some(QuitDialog {
                    focused_button: dialog::QuitDialogButtonId::Quit,
                    ..quit_dialog.clone()
                }))
            }
            // Press the appropriate dialog button.
            (Some(quit_dialog), event::Event::ToggleItem | event::Event::ToggleItemAndAdvance) => {
                let dialog::QuitDialog {
                    num_commit_messages: _,
                    num_changed_files: _,
                    focused_button,
                } = quit_dialog;
                match focused_button {
                    dialog::QuitDialogButtonId::Quit => StateUpdate::QuitCancel,
                    dialog::QuitDialogButtonId::GoBack => StateUpdate::SetQuitDialog(None),
                }
            }

            // Disable most keyboard shortcuts while the quit dialog is open.
            (
                Some(_),
                event::Event::ScrollUp
                | event::Event::ScrollDown
                | event::Event::PageUp
                | event::Event::PageDown
                | event::Event::FocusPrev
                | event::Event::FocusNext
                | event::Event::FocusPrevSameKind
                | event::Event::FocusNextSameKind
                | event::Event::FocusPrevPage
                | event::Event::FocusNextPage
                | event::Event::ToggleAll
                | event::Event::ToggleAllUniform
                | event::Event::ExpandItem
                | event::Event::ExpandAll
                | event::Event::EditCommitMessage,
            ) => StateUpdate::None,

            (Some(_) | None, event::Event::TakeScreenshot(screenshot)) => {
                StateUpdate::TakeScreenshot(screenshot)
            }
            (None, event::Event::ScrollUp) => {
                StateUpdate::ScrollTo(self.scroll_offset_y.saturating_sub(1))
            }
            (None, event::Event::ScrollDown) => {
                StateUpdate::ScrollTo(self.scroll_offset_y.saturating_add(1))
            }
            (None, event::Event::PageUp) => StateUpdate::ScrollTo(
                self.scroll_offset_y
                    .saturating_sub(term_height.unwrap_isize()),
            ),
            (None, event::Event::PageDown) => StateUpdate::ScrollTo(
                self.scroll_offset_y
                    .saturating_add(term_height.unwrap_isize()),
            ),
            (None, event::Event::FocusPrev) => {
                let (keys, index) = self.find_selection();
                let selection_key = self.select_prev(&keys, index);
                StateUpdate::SelectItem {
                    selection_key,
                    ensure_in_viewport: true,
                }
            }
            (None, event::Event::FocusNext) => {
                let (keys, index) = self.find_selection();
                let selection_key = self.select_next(&keys, index);
                StateUpdate::SelectItem {
                    selection_key,
                    ensure_in_viewport: true,
                }
            }
            (None, event::Event::FocusPrevSameKind) => {
                let selection_key =
                    self.select_prev_or_next_of_same_kind(/*select_previous=*/ true);
                StateUpdate::SelectItem {
                    selection_key,
                    ensure_in_viewport: true,
                }
            }
            (None, event::Event::FocusNextSameKind) => {
                let selection_key =
                    self.select_prev_or_next_of_same_kind(/*select_previous=*/ false);
                StateUpdate::SelectItem {
                    selection_key,
                    ensure_in_viewport: true,
                }
            }
            (None, event::Event::FocusPrevPage) => {
                let selection_key = self.select_prev_page(term_height, drawn_rects);
                StateUpdate::SelectItem {
                    selection_key,
                    ensure_in_viewport: true,
                }
            }
            (None, event::Event::FocusNextPage) => {
                let selection_key = self.select_next_page(term_height, drawn_rects);
                StateUpdate::SelectItem {
                    selection_key,
                    ensure_in_viewport: true,
                }
            }
            (None, event::Event::FocusOuter { fold_section }) => self.select_outer(fold_section),
            (None, event::Event::FocusInner) => {
                let selection_key = self.select_inner();
                StateUpdate::SelectItem {
                    selection_key,
                    ensure_in_viewport: true,
                }
            }
            (None, event::Event::ToggleItem) => StateUpdate::ToggleItem(self.selection_key),
            (None, event::Event::ToggleItemAndAdvance) => {
                let advanced_key = self.advance_to_next_of_kind();
                StateUpdate::ToggleItemAndAdvance(self.selection_key, advanced_key)
            }
            (None, event::Event::ToggleAll) => StateUpdate::ToggleAll,
            (None, event::Event::ToggleAllUniform) => StateUpdate::ToggleAllUniform,
            (None, event::Event::ExpandItem) => StateUpdate::ToggleExpandItem(self.selection_key),
            (None, event::Event::ExpandAll) => StateUpdate::ToggleExpandAll,
            (None, event::Event::EditCommitMessage) => StateUpdate::EditCommitMessage {
                commit_idx: self.focused_commit_idx,
            },

            (_, event::Event::ToggleCommitViewMode) => StateUpdate::ToggleCommitViewMode,

            // generally ignore escape key
            (_, event::Event::QuitEscape) => StateUpdate::None,
        };
        Ok(state_update)
    }

    fn first_selection_key(&self) -> SelectionKey {
        match self.state.files.iter().enumerate().next() {
            Some((file_idx, _)) => SelectionKey::File(FileKey {
                commit_idx: self.focused_commit_idx,
                file_idx,
            }),
            None => SelectionKey::None,
        }
    }

    fn num_user_commit_messages(&self) -> Result<usize, RecordError> {
        let RecordState {
            files: _,
            commits,
            is_read_only: _,
        } = &self.state;
        Ok(commits
            .iter()
            .map(|commit| {
                let Commit { message } = commit;
                match message {
                    Some(message) if !message.is_empty() => 1,
                    _ => 0,
                }
            })
            .sum())
    }

    fn num_user_file_changes(&self) -> Result<usize, RecordError> {
        let RecordState {
            files,
            commits: _,
            is_read_only: _,
        } = &self.state;
        let mut result = 0;
        for (file_idx, _file) in files.iter().enumerate() {
            match self.file_tristate(FileKey {
                commit_idx: self.focused_commit_idx,
                file_idx,
            })? {
                Tristate::False => {}
                Tristate::Partial | Tristate::True => {
                    result += 1;
                }
            }
        }
        Ok(result)
    }

    fn all_selection_keys(&self) -> Vec<SelectionKey> {
        let mut result = Vec::new();
        for (commit_idx, _) in self.state.commits.iter().enumerate() {
            if commit_idx > 0 {
                // TODO: implement adjacent `CommitView s.
                continue;
            }
            for (file_idx, file) in self.state.files.iter().enumerate() {
                result.push(SelectionKey::File(FileKey {
                    commit_idx,
                    file_idx,
                }));
                for (section_idx, section) in file.sections.iter().enumerate() {
                    match section {
                        Section::Unchanged { .. } => {}
                        Section::Changed { lines } => {
                            result.push(SelectionKey::Section(section::SectionKey {
                                commit_idx,
                                file_idx,
                                section_idx,
                            }));
                            for (line_idx, _line) in lines.iter().enumerate() {
                                result.push(SelectionKey::Line(LineKey {
                                    commit_idx,
                                    file_idx,
                                    section_idx,
                                    line_idx,
                                }));
                            }
                        }
                        Section::FileMode {
                            is_checked: _,
                            mode: _,
                        }
                        | Section::Binary { .. } => {
                            result.push(SelectionKey::Section(section::SectionKey {
                                commit_idx,
                                file_idx,
                                section_idx,
                            }));
                        }
                    }
                }
            }
        }
        result
    }

    fn find_selection(&self) -> (Vec<SelectionKey>, Option<usize>) {
        // FIXME: finding the selected key is an O(n) algorithm (instead of O(log(n)) or O(1)).
        let visible_keys: Vec<_> = self
            .all_selection_keys()
            .iter()
            .cloned()
            .filter(|key| match key {
                SelectionKey::None => false,
                SelectionKey::File(_) => true,
                SelectionKey::Section(section_key) => {
                    let file_key = FileKey {
                        commit_idx: section_key.commit_idx,
                        file_idx: section_key.file_idx,
                    };
                    match self.file_expanded(file_key) {
                        Tristate::False => false,
                        Tristate::Partial | Tristate::True => true,
                    }
                }
                SelectionKey::Line(line_key) => {
                    let file_key = FileKey {
                        commit_idx: line_key.commit_idx,
                        file_idx: line_key.file_idx,
                    };
                    let section_key = section::SectionKey {
                        commit_idx: line_key.commit_idx,
                        file_idx: line_key.file_idx,
                        section_idx: line_key.section_idx,
                    };
                    self.expanded_items.contains(&SelectionKey::File(file_key))
                        && self
                            .expanded_items
                            .contains(&SelectionKey::Section(section_key))
                }
            })
            .collect();
        let index = visible_keys.iter().enumerate().find_map(|(k, v)| {
            if v == &self.selection_key {
                Some(k)
            } else {
                None
            }
        });
        (visible_keys, index)
    }

    fn select_prev(&self, keys: &[SelectionKey], index: Option<usize>) -> SelectionKey {
        match index {
            None => self.first_selection_key(),
            Some(index) => match index.checked_sub(1) {
                Some(prev_index) => keys[prev_index],
                None => keys[index],
            },
        }
    }

    fn select_next(&self, keys: &[SelectionKey], index: Option<usize>) -> SelectionKey {
        match index {
            None => self.first_selection_key(),
            Some(index) => match keys.get(index + 1) {
                Some(key) => *key,
                None => keys[index],
            },
        }
    }

    // Returns the previous or next SelectionKey of the same kind as the current
    // selection key. If there are no other keys of the same kind, the current
    // key is returned instead. If `select_previous` is true, the previous key
    // is returned. Otherwise, the next key is returned.
    fn select_prev_or_next_of_same_kind(&self, select_previous: bool) -> SelectionKey {
        let (keys, index) = self.find_selection();
        match index {
            None => self.first_selection_key(),
            Some(index) => {
                let mut iterate_keys: Box<dyn DoubleEndedIterator<Item = _>> = match select_previous
                {
                    true => Box::new(keys[..index].iter().rev()),
                    false => Box::new(keys[index + 1..].iter()),
                };

                match iterate_keys
                    .find(|k| std::mem::discriminant(*k) == std::mem::discriminant(&keys[index]))
                {
                    None => keys[index],
                    Some(key) => *key,
                }
            }
        }
    }

    fn select_prev_page(
        &self,
        term_height: usize,
        drawn_rects: &DrawnRects<ComponentId>,
    ) -> SelectionKey {
        let (keys, index) = self.find_selection();
        let mut index = match index {
            Some(index) => index,
            None => return SelectionKey::None,
        };

        let original_y = match self.selection_key_y(drawn_rects, self.selection_key) {
            Some(original_y) => original_y,
            None => {
                return SelectionKey::None;
            }
        };
        let target_y = original_y.saturating_sub(term_height.unwrap_isize() / 2);
        while index > 0 {
            index -= 1;
            let selection_key_y = self.selection_key_y(drawn_rects, keys[index]);
            if let Some(selection_key_y) = selection_key_y {
                if selection_key_y <= target_y {
                    break;
                }
            }
        }
        keys[index]
    }

    fn select_next_page(
        &self,
        term_height: usize,
        drawn_rects: &DrawnRects<ComponentId>,
    ) -> SelectionKey {
        let (keys, index) = self.find_selection();
        let mut index = match index {
            Some(index) => index,
            None => return SelectionKey::None,
        };

        let original_y = match self.selection_key_y(drawn_rects, self.selection_key) {
            Some(original_y) => original_y,
            None => return SelectionKey::None,
        };
        let target_y = original_y.saturating_add(term_height.unwrap_isize() / 2);
        while index + 1 < keys.len() {
            index += 1;
            let selection_key_y = self.selection_key_y(drawn_rects, keys[index]);
            if let Some(selection_key_y) = selection_key_y {
                if selection_key_y >= target_y {
                    break;
                }
            }
        }
        keys[index]
    }

    fn select_inner(&self) -> SelectionKey {
        self.all_selection_keys()
            .into_iter()
            .skip_while(|selection_key| selection_key != &self.selection_key)
            .skip(1)
            .find(|selection_key| {
                match (self.selection_key, selection_key) {
                    (SelectionKey::None, _) => true,
                    (_, SelectionKey::None) => false, // shouldn't happen

                    (SelectionKey::File(_), SelectionKey::File(_)) => false,
                    (SelectionKey::File(_), SelectionKey::Section(_)) => true,
                    (SelectionKey::File(_), SelectionKey::Line(_)) => false, // shouldn't happen

                    (SelectionKey::Section(_), SelectionKey::File(_))
                    | (SelectionKey::Section(_), SelectionKey::Section(_)) => false,
                    (SelectionKey::Section(_), SelectionKey::Line(_)) => true,

                    (SelectionKey::Line(_), _) => false,
                }
            })
            .unwrap_or(self.selection_key)
    }

    fn select_outer(&self, fold_section: bool) -> StateUpdate {
        match self.selection_key {
            SelectionKey::None => StateUpdate::None,
            selection_key @ SelectionKey::File(_) => {
                StateUpdate::SetExpandItem(selection_key, false)
            }
            selection_key @ SelectionKey::Section(section::SectionKey {
                commit_idx,
                file_idx,
                section_idx: _,
            }) => {
                // If folding is requested and the selection is expanded,
                // collapse it. Otherwise, move the selection to the file.
                if fold_section && self.expanded_items.contains(&selection_key) {
                    StateUpdate::SetExpandItem(selection_key, false)
                } else {
                    StateUpdate::SelectItem {
                        selection_key: SelectionKey::File(FileKey {
                            commit_idx,
                            file_idx,
                        }),
                        ensure_in_viewport: true,
                    }
                }
            }
            SelectionKey::Line(LineKey {
                commit_idx,
                file_idx,
                section_idx,
                line_idx: _,
            }) => StateUpdate::SelectItem {
                selection_key: SelectionKey::Section(section::SectionKey {
                    commit_idx,
                    file_idx,
                    section_idx,
                }),
                ensure_in_viewport: true,
            },
        }
    }

    fn advance_to_next_of_kind(&self) -> SelectionKey {
        let (keys, index) = self.find_selection();
        let index = match index {
            Some(index) => index,
            None => return SelectionKey::None,
        };
        keys.iter()
            .skip(index + 1)
            .copied()
            .find(|key| match (self.selection_key, key) {
                (SelectionKey::None, _)
                | (SelectionKey::File(_), SelectionKey::File(_))
                | (SelectionKey::Section(_), SelectionKey::Section(_))
                | (SelectionKey::Line(_), SelectionKey::Line(_)) => true,
                (
                    SelectionKey::File(_),
                    SelectionKey::None | SelectionKey::Section(_) | SelectionKey::Line(_),
                )
                | (
                    SelectionKey::Section(_),
                    SelectionKey::None | SelectionKey::File(_) | SelectionKey::Line(_),
                )
                | (
                    SelectionKey::Line(_),
                    SelectionKey::None | SelectionKey::File(_) | SelectionKey::Section(_),
                ) => false,
            })
            .unwrap_or(self.selection_key)
    }

    fn selection_key_y(
        &self,
        drawn_rects: &DrawnRects<ComponentId>,
        selection_key: SelectionKey,
    ) -> Option<isize> {
        let rect = self.selection_rect(drawn_rects, selection_key)?;
        Some(rect.y)
    }

    fn selection_rect(
        &self,
        drawn_rects: &DrawnRects<ComponentId>,
        selection_key: SelectionKey,
    ) -> Option<Rect> {
        let id = match selection_key {
            SelectionKey::None => return None,
            SelectionKey::File(_) | SelectionKey::Section(_) | SelectionKey::Line(_) => {
                ComponentId::SelectableItem(selection_key)
            }
        };
        match drawn_rects.get(&id) {
            Some(DrawnRect { rect, timestamp: _ }) => Some(*rect),
            None => {
                if cfg!(debug_assertions) {
                    panic!(
                        "could not look up drawn rect for component with ID {id:?}; was it drawn?"
                    )
                } else {
                    warn!(component_id = ?id, "could not look up drawn rect for component; was it drawn?");
                    None
                }
            }
        }
    }

    fn ensure_in_viewport(
        &self,
        term_height: usize,
        drawn_rects: &DrawnRects<ComponentId>,
        selection_key: SelectionKey,
    ) -> Option<isize> {
        let sticky_file_header_height = match selection_key {
            SelectionKey::None | SelectionKey::File(_) => 0,
            SelectionKey::Section(_) | SelectionKey::Line(_) => 1,
        };
        let top_margin = sticky_file_header_height;

        let viewport_top_y = self.scroll_offset_y + top_margin;
        let viewport_height = term_height.unwrap_isize() - top_margin;
        let viewport_bottom_y = viewport_top_y + viewport_height;

        let selection_rect = self.selection_rect(drawn_rects, selection_key)?;
        let selection_top_y = selection_rect.y;
        let selection_height = selection_rect.height.unwrap_isize();
        let selection_bottom_y = selection_top_y + selection_height;

        // Idea: scroll the entire component into the viewport, not just the
        // first line, if possible. If the entire component is smaller than
        // the viewport, then we scroll only enough so that the entire
        // component becomes visible, i.e. align the component's bottom edge
        // with the viewport's bottom edge. Otherwise, we scroll such that
        // the component's top edge is aligned with the viewport's top edge.
        //
        // FIXME: if we scroll up from below, we would want to align the top
        // edge of the component, not the bottom edge. Thus, we should also
        // accept the previous `SelectionKey` and use that when making the
        // decision of where to scroll.
        let result = if viewport_top_y <= selection_top_y && selection_bottom_y < viewport_bottom_y
        {
            // Component is completely within the viewport, no need to scroll.
            self.scroll_offset_y
        } else if (
            // Component doesn't fit in the viewport; just render the top.
            selection_height >= viewport_height
        ) || (
            // Component is at least partially above the viewport.
            selection_top_y < viewport_top_y
        ) {
            selection_top_y - top_margin
        } else {
            // Component is at least partially below the viewport. Want to satisfy:
            // scroll_offset_y + term_height == rect_bottom_y
            selection_bottom_y - top_margin - viewport_height
        };
        Some(result)
    }

    fn toggle_item(&mut self, selection: SelectionKey) -> Result<(), RecordError> {
        if self.state.is_read_only {
            return Ok(());
        }

        let side_effects = match selection {
            SelectionKey::None => None,
            SelectionKey::File(file_key) => {
                let tristate = self.file_tristate(file_key)?;
                let is_checked_new = match tristate {
                    Tristate::False => true,
                    Tristate::Partial | Tristate::True => false,
                };
                self.visit_file(file_key, |file| {
                    file.set_checked(is_checked_new);
                })?;

                None
            }
            SelectionKey::Section(section_key) => {
                let tristate = self.section_tristate(section_key)?;
                let is_checked_new = match tristate {
                    Tristate::False => true,
                    Tristate::Partial | Tristate::True => false,
                };

                let old_file_mode = self.visit_file_for_section(section_key, |f| f.file_mode)?;

                self.visit_section(section_key, |section| {
                    section.set_checked(is_checked_new);

                    if let Section::FileMode { mode, .. } = section {
                        return Some(ToggleSideEffects::ToggledModeChangeSection(
                            section_key,
                            old_file_mode,
                            *mode,
                            is_checked_new,
                        ));
                    }

                    if let Section::Changed { .. } = section {
                        return Some(ToggleSideEffects::ToggledChangedSection(
                            section_key,
                            is_checked_new,
                        ));
                    }

                    None
                })?
            }
            SelectionKey::Line(line_key) => self.visit_line(line_key, |line| {
                line.is_checked = !line.is_checked;

                Some(ToggleSideEffects::ToggledChangedLine(
                    line_key,
                    line.is_checked,
                ))
            })?,
        };

        if let Some(side_effects) = side_effects {
            match side_effects {
                ToggleSideEffects::ToggledModeChangeSection(
                    section_key,
                    old_mode,
                    new_mode,
                    toggled_to,
                ) => {
                    // If we check a deletion, all lines in the file must be deleted
                    if toggled_to && new_mode == FileMode::Absent {
                        self.visit_file_for_section(section_key, |file| {
                            for section in &mut file.sections {
                                if matches!(section, Section::Changed { .. }) {
                                    section.set_checked(true);
                                }
                            }
                        })?;
                    }

                    // If we uncheck a creation, no lines in the file can be added
                    if !toggled_to && old_mode == FileMode::Absent {
                        self.visit_file_for_section(section_key, |file| {
                            for section in &mut file.sections {
                                section.set_checked(false);
                            }
                        })?;
                    }
                }
                ToggleSideEffects::ToggledChangedSection(section_key, toggled_to) => {
                    self.visit_file_for_section(section_key, |file| {
                        for section in &mut file.sections {
                            if let Section::FileMode { mode, is_checked } = section {
                                // If we removed a line and the file was being deleted, it can no longer
                                // be deleted as it needs to contain that line
                                if !toggled_to && *mode == FileMode::Absent {
                                    *is_checked = false;
                                }

                                // If we added a line and the file was not being created, it must be created
                                // in order to contain that line
                                if toggled_to && file.file_mode == FileMode::Absent {
                                    *is_checked = true;
                                }
                            }
                        }
                    })?;
                }
                ToggleSideEffects::ToggledChangedLine(line_key, toggled_to) => {
                    self.visit_file_for_line(line_key, |file| {
                        for section in &mut file.sections {
                            if let Section::FileMode { mode, is_checked } = section {
                                // If we removed a line and the file was being deleted, it can no longer
                                // be deleted as it needs to contain that line
                                if !toggled_to && *mode == FileMode::Absent {
                                    *is_checked = false;
                                }

                                // If we added a line and the file was not being created, it must be created
                                // in order to contain that line
                                if toggled_to && file.file_mode == FileMode::Absent {
                                    *is_checked = true;
                                }
                            }
                        }
                    })?;
                }
            }
        };

        Ok(())
    }

    fn toggle_all(&mut self) {
        if self.state.is_read_only {
            return;
        }

        for file in &mut self.state.files {
            file.toggle_all();
        }
    }

    fn toggle_all_uniform(&mut self) {
        if self.state.is_read_only {
            return;
        }

        let checked = {
            let tristate = self
                .state
                .files
                .iter()
                .map(|file| file.tristate())
                .fold(None, |acc, elem| match (acc, elem) {
                    (None, tristate) => Some(tristate),
                    (Some(acc_tristate), tristate) if acc_tristate == tristate => Some(tristate),
                    _ => Some(Tristate::Partial),
                })
                .unwrap_or(Tristate::False);
            match tristate {
                Tristate::False | Tristate::Partial => true,
                Tristate::True => false,
            }
        };
        for file in &mut self.state.files {
            file.set_checked(checked);
        }
    }

    fn expand_item_ancestors(&mut self, selection: SelectionKey) {
        match selection {
            SelectionKey::None | SelectionKey::File(_) => {}
            SelectionKey::Section(section::SectionKey {
                commit_idx,
                file_idx,
                section_idx: _,
            }) => {
                self.expanded_items.insert(SelectionKey::File(FileKey {
                    commit_idx,
                    file_idx,
                }));
            }
            SelectionKey::Line(LineKey {
                commit_idx,
                file_idx,
                section_idx,
                line_idx: _,
            }) => {
                self.expanded_items.insert(SelectionKey::File(FileKey {
                    commit_idx,
                    file_idx,
                }));
                self.expanded_items
                    .insert(SelectionKey::Section(section::SectionKey {
                        commit_idx,
                        file_idx,
                        section_idx,
                    }));
            }
        }
    }

    fn set_expand_item(&mut self, selection: SelectionKey, is_expanded: bool) {
        if is_expanded {
            self.expanded_items.insert(selection);
        } else {
            self.expanded_items.remove(&selection);
        }
    }

    fn toggle_expand_item(&mut self, selection: SelectionKey) -> Result<(), RecordError> {
        match selection {
            SelectionKey::None => {}
            SelectionKey::File(file_key) => {
                if !self.expanded_items.insert(SelectionKey::File(file_key)) {
                    self.expanded_items.remove(&SelectionKey::File(file_key));
                }
            }
            SelectionKey::Section(section_key) => {
                if !self
                    .expanded_items
                    .insert(SelectionKey::Section(section_key))
                {
                    self.expanded_items
                        .remove(&SelectionKey::Section(section_key));
                }
            }
            SelectionKey::Line(_) => {
                // Do nothing.
            }
        }
        Ok(())
    }

    fn expand_initial_items(&mut self) {
        self.expanded_items = self
            .all_selection_keys()
            .into_iter()
            .filter(|selection_key| match selection_key {
                SelectionKey::None | SelectionKey::File(_) | SelectionKey::Line(_) => false,
                SelectionKey::Section(_) => true,
            })
            .collect();
    }

    fn toggle_expand_all(&mut self) -> Result<(), RecordError> {
        let all_selection_keys: HashSet<_> = self.all_selection_keys().into_iter().collect();
        self.expanded_items = if self.expanded_items == all_selection_keys {
            // Select an ancestor file key that will still be visible.
            self.selection_key = match self.selection_key {
                selection_key @ (SelectionKey::None | SelectionKey::File(_)) => selection_key,
                SelectionKey::Section(section::SectionKey {
                    commit_idx,
                    file_idx,
                    section_idx: _,
                })
                | SelectionKey::Line(LineKey {
                    commit_idx,
                    file_idx,
                    section_idx: _,
                    line_idx: _,
                }) => SelectionKey::File(FileKey {
                    commit_idx,
                    file_idx,
                }),
            };
            Default::default()
        } else {
            all_selection_keys
        };
        Ok(())
    }

    fn edit_commit_message(&mut self, commit_idx: usize) -> Result<(), RecordError> {
        let message = &mut self.state.commits[commit_idx].message;
        let message_str = match message.as_ref() {
            Some(message) => message,
            None => return Ok(()),
        };
        let new_message = {
            match self.input.terminal_kind() {
                terminal::TerminalKind::Testing { .. } => {}
                terminal::TerminalKind::Crossterm => {
                    terminal::clean_up_crossterm()?;
                }
            }
            let result = self.input.edit_commit_message(message_str);
            match self.input.terminal_kind() {
                terminal::TerminalKind::Testing { .. } => {}
                terminal::TerminalKind::Crossterm => {
                    terminal::set_up_crossterm()?;
                }
            }
            result?
        };
        *message = Some(new_message);
        Ok(())
    }

    fn file(&self, file_key: FileKey) -> Result<&File<'_>, RecordError> {
        let FileKey {
            commit_idx: _,
            file_idx,
        } = file_key;
        match self.state.files.get(file_idx) {
            Some(file) => Ok(file),
            None => Err(RecordError::Bug(format!(
                "Out-of-bounds file key: {file_key:?}"
            ))),
        }
    }

    fn section(&self, section_key: section::SectionKey) -> Result<&Section<'_>, RecordError> {
        let section::SectionKey {
            commit_idx,
            file_idx,
            section_idx,
        } = section_key;
        let file = self.file(FileKey {
            commit_idx,
            file_idx,
        })?;
        match file.sections.get(section_idx) {
            Some(section) => Ok(section),
            None => Err(RecordError::Bug(format!(
                "Out-of-bounds section key: {section_key:?}"
            ))),
        }
    }

    fn visit_file_for_section<T>(
        &mut self,
        section_key: section::SectionKey,
        f: impl Fn(&mut File) -> T,
    ) -> Result<T, RecordError> {
        let section::SectionKey {
            commit_idx: _,
            file_idx,
            section_idx: _,
        } = section_key;

        match self.state.files.get_mut(file_idx) {
            Some(file) => Ok(f(file)),
            None => Err(RecordError::Bug(format!(
                "Out-of-bounds file key: {file_idx:?}"
            ))),
        }
    }

    fn visit_file_for_line<T>(
        &mut self,
        line_key: LineKey,
        f: impl Fn(&mut File) -> T,
    ) -> Result<T, RecordError> {
        let LineKey {
            commit_idx: _,
            file_idx,
            section_idx: _,
            line_idx: _,
        } = line_key;

        match self.state.files.get_mut(file_idx) {
            Some(file) => Ok(f(file)),
            None => Err(RecordError::Bug(format!(
                "Out-of-bounds file key: {file_idx:?}"
            ))),
        }
    }

    fn visit_file<T>(
        &mut self,
        file_key: FileKey,
        f: impl Fn(&mut File) -> T,
    ) -> Result<T, RecordError> {
        let FileKey {
            commit_idx: _,
            file_idx,
        } = file_key;
        match self.state.files.get_mut(file_idx) {
            Some(file) => Ok(f(file)),
            None => Err(RecordError::Bug(format!(
                "Out-of-bounds file key: {file_key:?}"
            ))),
        }
    }

    fn file_tristate(&self, file_key: FileKey) -> Result<Tristate, RecordError> {
        let file = self.file(file_key)?;
        Ok(file.tristate())
    }

    fn file_expanded(&self, file_key: FileKey) -> Tristate {
        let is_expanded = self.expanded_items.contains(&SelectionKey::File(file_key));
        if !is_expanded {
            Tristate::False
        } else {
            let any_section_unexpanded = self
                .file(file_key)
                .unwrap()
                .sections
                .iter()
                .enumerate()
                .any(|(section_idx, section)| {
                    match section {
                        Section::Unchanged { .. }
                        | Section::FileMode { .. }
                        | Section::Binary { .. } => {
                            // Not collapsible/expandable.
                            false
                        }
                        Section::Changed { .. } => {
                            let section_key = section::SectionKey {
                                commit_idx: file_key.commit_idx,
                                file_idx: file_key.file_idx,
                                section_idx,
                            };
                            !self
                                .expanded_items
                                .contains(&SelectionKey::Section(section_key))
                        }
                    }
                });
            if any_section_unexpanded {
                Tristate::Partial
            } else {
                Tristate::True
            }
        }
    }

    fn visit_section<T>(
        &mut self,
        section_key: section::SectionKey,
        f: impl Fn(&mut Section) -> T,
    ) -> Result<T, RecordError> {
        let section::SectionKey {
            commit_idx: _,
            file_idx,
            section_idx,
        } = section_key;
        let file = match self.state.files.get_mut(file_idx) {
            Some(file) => file,
            None => {
                return Err(RecordError::Bug(format!(
                    "Out-of-bounds file for section key: {section_key:?}"
                )));
            }
        };
        match file.sections.get_mut(section_idx) {
            Some(section) => Ok(f(section)),
            None => Err(RecordError::Bug(format!(
                "Out-of-bounds section key: {section_key:?}"
            ))),
        }
    }

    fn section_tristate(&self, section_key: section::SectionKey) -> Result<Tristate, RecordError> {
        let section = self.section(section_key)?;
        Ok(section.tristate())
    }

    fn visit_line<T>(
        &mut self,
        line_key: LineKey,
        f: impl FnOnce(&mut SectionChangedLine) -> Option<T>,
    ) -> Result<Option<T>, RecordError> {
        let LineKey {
            commit_idx: _,
            file_idx,
            section_idx,
            line_idx,
        } = line_key;
        let section = &mut self.state.files[file_idx].sections[section_idx];
        match section {
            Section::Changed { lines } => {
                let line = &mut lines[line_idx];
                Ok(f(line))
            }
            Section::Unchanged { .. } | Section::FileMode { .. } | Section::Binary { .. } => {
                // Do nothing.
                Ok(None)
            }
        }
    }
}
