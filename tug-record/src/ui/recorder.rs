use crate::consts::ENV_VAR_DEBUG_UI;
use crate::render::{DrawnRect, DrawnRects, Viewport};
use crate::types::{RecordError, RecordState};
use crate::ui::components::app::{AppDebugInfo, AppView};
use crate::ui::components::commit_message_view::CommitViewMode;
use crate::ui::components::ComponentId;
use crate::ui::{event, input, terminal, App, StateUpdate};
use crate::util::UsizeExt;
use ratatui::backend::{Backend, TestBackend};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::any::Any;
use std::{io, mem};

/// UI component to record the user's changes.
/// This struct is the main driver for the UI, handling the event loop,
/// terminal interaction, and I/O. The core application logic and state
/// are managed by the `App` struct.
pub struct Recorder<'state, 'input> {
    app: App<'state>,
    input: &'input mut dyn input::RecordInput,
    pending_events: Vec<event::Event>,
}

impl<'state, 'input> Recorder<'state, 'input> {
    /// Constructor.
    pub fn new(state: RecordState<'state>, input: &'input mut dyn input::RecordInput) -> Self {
        Self {
            app: App::new(state),
            input,
            pending_events: Default::default(),
        }
    }

    /// Run the terminal user interface and have the user interactively select
    /// changes.
    pub fn run(self) -> Result<RecordState<'state>, RecordError> {
        #[cfg(feature = "debug")]
        if std::env::var_os(crate::consts::ENV_VAR_DUMP_UI_STATE).is_some() {
            let ui_state = serde_json::to_string_pretty(&self.app.state)
                .map_err(RecordError::SerializeJson)?;
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
        let debug = if cfg!(feature = "debug") {
            std::env::var_os(ENV_VAR_DEBUG_UI).is_some()
        } else {
            false
        };

        'outer: loop {
            let app_view = self.app.view(None);
            let term_height = usize::from(term.get_frame().area().height);

            let mut drawn_rects: Option<DrawnRects<ComponentId>> = None;
            term.draw(|frame| {
                drawn_rects = Some(Viewport::<ComponentId>::render_top_level(
                    frame,
                    0,
                    self.app.ui.scroll_offset_y,
                    &app_view,
                ));
            })
            .map_err(RecordError::RenderFrame)?;
            let drawn_rects = drawn_rects.unwrap();

            // Dump debug info. We may need to use information about the
            // rendered app, so we perform a re-render here.
            if debug {
                let debug_info = AppDebugInfo {
                    term_height,
                    scroll_offset_y: self.app.ui.scroll_offset_y,
                    selection_key: self.app.ui.selection_key,
                    selection_key_y: self
                        .app
                        .selection_key_y(&drawn_rects, self.app.ui.selection_key),
                    drawn_rects: drawn_rects.clone().into_iter().collect(),
                };
                let debug_app = AppView {
                    debug_info: Some(debug_info),
                    ..app_view.clone()
                };
                term.draw(|frame| {
                    Viewport::<ComponentId>::render_top_level(
                        frame,
                        0,
                        self.app.ui.scroll_offset_y,
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
                match self.app.handle_event(event, term_height, &drawn_rects)? {
                    StateUpdate::None => {}
                    StateUpdate::SetHelpDialog(help_dialog) => {
                        self.app.ui.help_dialog = help_dialog;
                    }
                    StateUpdate::QuitAccept => {
                        if self.app.ui.help_dialog.is_some() {
                            self.app.ui.help_dialog = None;
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
                        if let Some(scroll_offset_y) = self.app.ensure_in_viewport(
                            term_height,
                            &drawn_rects,
                            self.app.ui.selection_key,
                        ) {
                            self.app.ui.scroll_offset_y = scroll_offset_y;
                        }
                    }
                    StateUpdate::ScrollTo(scroll_offset_y) => {
                        self.app.ui.scroll_offset_y = scroll_offset_y.clamp(0, {
                            let DrawnRect { rect, timestamp: _ } = drawn_rects[&ComponentId::App];
                            rect.height.unwrap_isize() - 1
                        });
                    }
                    StateUpdate::SelectItem {
                        selection_key,
                        ensure_in_viewport,
                    } => {
                        self.app.ui.selection_key = selection_key;
                        self.app.expand_item_ancestors(selection_key);
                        if ensure_in_viewport {
                            self.pending_events
                                .push(event::Event::EnsureSelectionInViewport);
                        }
                    }
                    StateUpdate::ToggleItem(selection_key) => {
                        self.app.toggle_item(selection_key)?;
                    }
                    StateUpdate::ToggleItemAndAdvance(selection_key, new_key) => {
                        self.app.toggle_item(selection_key)?;
                        self.app.ui.selection_key = new_key;
                        self.pending_events
                            .push(event::Event::EnsureSelectionInViewport);
                    }
                    StateUpdate::ToggleAll => {
                        self.app.toggle_all();
                    }
                    StateUpdate::ToggleAllUniform => {
                        self.app.toggle_all_uniform();
                    }
                    StateUpdate::SetExpandItem(selection_key, is_expanded) => {
                        self.app.set_expand_item(selection_key, is_expanded);
                        self.pending_events
                            .push(event::Event::EnsureSelectionInViewport);
                    }
                    StateUpdate::ToggleExpandItem(selection_key) => {
                        self.app.toggle_expand_item(selection_key)?;
                        self.pending_events
                            .push(event::Event::EnsureSelectionInViewport);
                    }
                    StateUpdate::ToggleExpandAll => {
                        self.app.toggle_expand_all()?;
                        self.pending_events
                            .push(event::Event::EnsureSelectionInViewport);
                    }
                    StateUpdate::ToggleCommitViewMode => {
                        self.app.ui.commit_view_mode = match self.app.ui.commit_view_mode {
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

        Ok(self.app.state)
    }

    fn edit_commit_message(&mut self, commit_idx: usize) -> Result<(), RecordError> {
        let message = &mut self.app.state.commits[commit_idx].message;
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
}
