use components::section;
use std::collections::HashSet;
use std::fmt::Debug;
use std::{iter, panic};
use tracing::warn;

pub mod components;
pub mod event;
pub mod input;
pub mod recorder;
pub mod terminal;

use crate::render::{DrawnRect, DrawnRects, Rect};
use crate::types::{ChangeType, Commit, RecordError, RecordState, Tristate};
use crate::ui::components::app::{AppDebugInfo, AppView, SelectionKey};
use crate::ui::components::commit_message_view::{CommitMessageView, CommitViewMode};
use crate::ui::components::commit_view::CommitView;
use crate::ui::components::file::{FileKey, FileView};
use crate::ui::components::help_dialog::HelpDialog;
use crate::ui::components::line::LineKey;
use crate::ui::components::widgets::{TristateBox, TristateIconStyle};
use crate::ui::components::{help_dialog, ComponentId};
use crate::ui::input::TestingScreenshot;
use crate::util::UsizeExt;
use crate::{File, FileMode, Section, SectionChangedLine};

#[derive(Clone, Debug, PartialEq, Eq)]
enum StateUpdate {
    None,
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

/// Holds the state of the UI, such as selection, expansion, and dialogs.
struct UiState {
    commit_view_mode: CommitViewMode,
    expanded_items: HashSet<SelectionKey>,
    selection_key: SelectionKey,
    focused_commit_idx: usize,
    help_dialog: Option<help_dialog::HelpDialog>,
    scroll_offset_y: isize,
}

/// Represents the application's state, combining the data model (`RecordState`)
/// and the UI state (`UiState`). It contains the core logic for updating the state
/// in response to events.
struct App<'state> {
    state: RecordState<'state>,
    ui: UiState,
}

impl<'state> App<'state> {
    fn new(mut state: RecordState<'state>) -> Self {
        // Ensure that there are at least two commits.
        state.commits.extend(
            iter::repeat_with(Commit::default).take(2_usize.saturating_sub(state.commits.len())),
        );
        if state.commits.len() > 2 {
            unimplemented!("more than two commits");
        }

        let mut app = Self {
            state,
            ui: UiState {
                commit_view_mode: CommitViewMode::Inline,
                expanded_items: Default::default(),
                selection_key: SelectionKey::None,
                focused_commit_idx: 0,
                help_dialog: None,
                scroll_offset_y: 0,
            },
        };
        app.ui.selection_key = app.first_selection_key();
        app.expand_initial_items();
        app
    }

    /// Generates the `AppView` used for rendering.
    fn view(&'state self, debug_info: Option<AppDebugInfo>) -> AppView<'state> {
        let RecordState {
            is_read_only,
            commits,
            files,
        } = &self.state;
        let commit_views = match self.ui.commit_view_mode {
            CommitViewMode::Inline => {
                vec![CommitView {
                    debug_info: None,
                    commit_message_view: CommitMessageView {
                        commit_idx: self.ui.focused_commit_idx,
                        commit: &commits[self.ui.focused_commit_idx],
                    },
                    file_views: self.make_file_views(
                        self.ui.focused_commit_idx,
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
            commit_view_mode: self.ui.commit_view_mode,
            commit_views,
            help_dialog: self.ui.help_dialog.clone(),
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
                let is_focused = match self.ui.selection_key {
                    SelectionKey::None | SelectionKey::Section(_) | SelectionKey::Line(_) => false,
                    SelectionKey::File(selected_file_key) => file_key == selected_file_key,
                };
                FileView {
                    debug: debug_info.is_some(),
                    file_key,
                    toggle_box: TristateBox {
                        id: ComponentId::ToggleBox(SelectionKey::File(file_key)),
                        icon_style: TristateIconStyle::Check,
                        tristate: file_toggled,
                        is_read_only,
                    },
                    expand_box: TristateBox {
                        id: ComponentId::ExpandBox(SelectionKey::File(file_key)),
                        icon_style: TristateIconStyle::Expand,
                        tristate: file_expanded,
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
                                self.ui
                                    .expanded_items
                                    .contains(&SelectionKey::Section(section_key)),
                            );
                            if section.is_editable() {
                                editable_section_num += 1;
                            }
                            section_views.push(section::SectionView {
                                is_read_only,
                                section_key,
                                toggle_box: TristateBox {
                                    is_read_only,
                                    id: ComponentId::ToggleBox(SelectionKey::Section(section_key)),
                                    tristate: section_toggled,
                                    icon_style: TristateIconStyle::Check,
                                },
                                expand_box: TristateBox {
                                    is_read_only: false,
                                    id: ComponentId::ExpandBox(SelectionKey::Section(section_key)),
                                    tristate: section_expanded,
                                    icon_style: TristateIconStyle::Expand,
                                },
                                selection: match self.ui.selection_key {
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
        // If the help dialog is open, certain keys will close it.
        if self.ui.help_dialog.is_some()
            && matches!(
                event,
                event::Event::Help
                    | event::Event::QuitEscape
                    | event::Event::QuitCancel
                    | event::Event::ToggleItem
                    | event::Event::ToggleItemAndAdvance
            ) {
                return Ok(StateUpdate::SetHelpDialog(None));
            }

        let state_update = match event {
            event::Event::None => StateUpdate::None,
            event::Event::Redraw => StateUpdate::Redraw,
            event::Event::EnsureSelectionInViewport => StateUpdate::EnsureSelectionInViewport,

            event::Event::Help => StateUpdate::SetHelpDialog(Some(HelpDialog())),

            // Confirm changes and quit.
            event::Event::QuitAccept => StateUpdate::QuitAccept,
            // Cancel changes and quit immediately.
            event::Event::QuitCancel | event::Event::QuitInterrupt => StateUpdate::QuitCancel,

            event::Event::TakeScreenshot(screenshot) => StateUpdate::TakeScreenshot(screenshot),
            event::Event::ScrollUp => {
                StateUpdate::ScrollTo(self.ui.scroll_offset_y.saturating_sub(1))
            }
            event::Event::ScrollDown => {
                StateUpdate::ScrollTo(self.ui.scroll_offset_y.saturating_add(1))
            }
            event::Event::PageUp => StateUpdate::ScrollTo(
                self.ui
                    .scroll_offset_y
                    .saturating_sub(term_height.unwrap_isize()),
            ),
            event::Event::PageDown => StateUpdate::ScrollTo(
                self.ui
                    .scroll_offset_y
                    .saturating_add(term_height.unwrap_isize()),
            ),
            event::Event::FocusPrev => {
                let (keys, index) = self.find_selection();
                let selection_key = self.select_prev(&keys, index);
                StateUpdate::SelectItem {
                    selection_key,
                    ensure_in_viewport: true,
                }
            }
            event::Event::FocusNext => {
                let (keys, index) = self.find_selection();
                let selection_key = self.select_next(&keys, index);
                StateUpdate::SelectItem {
                    selection_key,
                    ensure_in_viewport: true,
                }
            }
            event::Event::FocusPrevSameKind => {
                let selection_key =
                    self.select_prev_or_next_of_same_kind(/*select_previous=*/ true);
                StateUpdate::SelectItem {
                    selection_key,
                    ensure_in_viewport: true,
                }
            }
            event::Event::FocusNextSameKind => {
                let selection_key =
                    self.select_prev_or_next_of_same_kind(/*select_previous=*/ false);
                StateUpdate::SelectItem {
                    selection_key,
                    ensure_in_viewport: true,
                }
            }
            event::Event::FocusPrevPage => {
                let selection_key = self.select_prev_page(term_height, drawn_rects);
                StateUpdate::SelectItem {
                    selection_key,
                    ensure_in_viewport: true,
                }
            }
            event::Event::FocusNextPage => {
                let selection_key = self.select_next_page(term_height, drawn_rects);
                StateUpdate::SelectItem {
                    selection_key,
                    ensure_in_viewport: true,
                }
            }
            event::Event::FocusOuter { fold_section } => self.select_outer(fold_section),
            event::Event::FocusInner => {
                let selection_key = self.select_inner();
                StateUpdate::SelectItem {
                    selection_key,
                    ensure_in_viewport: true,
                }
            }
            event::Event::ToggleItem => StateUpdate::ToggleItem(self.ui.selection_key),
            event::Event::ToggleItemAndAdvance => {
                let advanced_key = self.advance_to_next_of_kind();
                StateUpdate::ToggleItemAndAdvance(self.ui.selection_key, advanced_key)
            }
            event::Event::ToggleAll => StateUpdate::ToggleAll,
            event::Event::ToggleAllUniform => StateUpdate::ToggleAllUniform,
            event::Event::ExpandItem => StateUpdate::ToggleExpandItem(self.ui.selection_key),
            event::Event::ExpandAll => StateUpdate::ToggleExpandAll,
            event::Event::EditCommitMessage => StateUpdate::EditCommitMessage {
                commit_idx: self.ui.focused_commit_idx,
            },

            event::Event::ToggleCommitViewMode => StateUpdate::ToggleCommitViewMode,

            // generally ignore escape key
            event::Event::QuitEscape => StateUpdate::None,
        };
        Ok(state_update)
    }

    fn first_selection_key(&self) -> SelectionKey {
        match self.state.files.iter().enumerate().next() {
            Some((file_idx, _)) => SelectionKey::File(FileKey {
                commit_idx: self.ui.focused_commit_idx,
                file_idx,
            }),
            None => SelectionKey::None,
        }
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
                    self.ui
                        .expanded_items
                        .contains(&SelectionKey::File(file_key))
                        && self
                            .ui
                            .expanded_items
                            .contains(&SelectionKey::Section(section_key))
                }
            })
            .collect();
        let index = visible_keys.iter().enumerate().find_map(|(k, v)| {
            if v == &self.ui.selection_key {
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

        let original_y = match self.selection_key_y(drawn_rects, self.ui.selection_key) {
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

        let original_y = match self.selection_key_y(drawn_rects, self.ui.selection_key) {
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
            .skip_while(|selection_key| selection_key != &self.ui.selection_key)
            .skip(1)
            .find(|selection_key| {
                match (self.ui.selection_key, selection_key) {
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
            .unwrap_or(self.ui.selection_key)
    }

    fn select_outer(&self, fold_section: bool) -> StateUpdate {
        match self.ui.selection_key {
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
                if fold_section && self.ui.expanded_items.contains(&selection_key) {
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
            .find(|key| match (self.ui.selection_key, key) {
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
            .unwrap_or(self.ui.selection_key)
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

        let viewport_top_y = self.ui.scroll_offset_y + top_margin;
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
            self.ui.scroll_offset_y
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
                self.ui.expanded_items.insert(SelectionKey::File(FileKey {
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
                self.ui.expanded_items.insert(SelectionKey::File(FileKey {
                    commit_idx,
                    file_idx,
                }));
                self.ui
                    .expanded_items
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
            self.ui.expanded_items.insert(selection);
        } else {
            self.ui.expanded_items.remove(&selection);
        }
    }

    fn toggle_expand_item(&mut self, selection: SelectionKey) -> Result<(), RecordError> {
        match selection {
            SelectionKey::None => {}
            SelectionKey::File(file_key) => {
                if !self.ui.expanded_items.insert(SelectionKey::File(file_key)) {
                    self.ui.expanded_items.remove(&SelectionKey::File(file_key));
                }
            }
            SelectionKey::Section(section_key) => {
                if !self
                    .ui
                    .expanded_items
                    .insert(SelectionKey::Section(section_key))
                {
                    self.ui
                        .expanded_items
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
        self.ui.expanded_items = self
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
        self.ui.expanded_items = if self.ui.expanded_items == all_selection_keys {
            // Select an ancestor file key that will still be visible.
            self.ui.selection_key = match self.ui.selection_key {
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
        let is_expanded = self
            .ui
            .expanded_items
            .contains(&SelectionKey::File(file_key));
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
                                .ui
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
