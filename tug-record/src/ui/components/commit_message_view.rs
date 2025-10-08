use crate::render::{Component, Rect, Viewport};
use crate::types::Commit;
use crate::ui::components::widgets::Button;
use crate::ui::components::ComponentId;
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use std::borrow::Cow;
use std::fmt::Debug;

#[derive(Clone, Copy, Debug)]
pub enum CommitViewMode {
    Inline,
    Adjacent,
}

#[derive(Clone, Debug)]
pub struct CommitMessageView<'a> {
    pub commit_idx: usize,
    pub commit: &'a Commit,
}

impl Component for CommitMessageView<'_> {
    type Id = ComponentId;

    fn id(&self) -> Self::Id {
        ComponentId::CommitMessageView
    }

    fn draw(&self, viewport: &mut Viewport<Self::Id>, x: isize, y: isize) {
        let Self { commit_idx, commit } = self;
        match commit {
            Commit { message: None } => {}
            Commit {
                message: Some(message),
            } => {
                viewport.draw_blank(Rect {
                    x,
                    y,
                    width: viewport.mask_rect().width,
                    height: 1,
                });
                let y = y + 1;

                let style = Style::default();
                let button_rect = viewport.draw_component(
                    x,
                    y,
                    &Button {
                        id: ComponentId::CommitEditMessageButton(*commit_idx),
                        label: Cow::Borrowed("Edit message"),
                        style,
                        is_focused: false,
                    },
                );
                let divider_rect =
                    viewport.draw_span(button_rect.end_x() + 1, y, &Span::raw(" • "));
                viewport.draw_text(
                    divider_rect.end_x() + 1,
                    y,
                    Span::styled(
                        Cow::Borrowed({
                            let first_line = match message.split_once('\n') {
                                Some((before, _after)) => before,
                                None => message,
                            };
                            let first_line = first_line.trim();
                            if first_line.is_empty() {
                                "(no message)"
                            } else {
                                first_line
                            }
                        }),
                        style.add_modifier(Modifier::UNDERLINED),
                    ),
                );
                let y = y + 1;

                viewport.draw_blank(Rect {
                    x,
                    y,
                    width: viewport.mask_rect().width,
                    height: 1,
                });
            }
        }
    }
}
