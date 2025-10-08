use crate::render::{centered_rect, Component, RectSize, Viewport};
use crate::ui::components::widgets::Button;
use crate::util::UsizeExt;
use ratatui::text::Text;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use std::borrow::Cow;
use std::fmt::Debug;
use std::hash::Hash;

pub struct Dialog<'a, Id> {
    pub id: Id,
    pub title: Cow<'a, str>,
    pub body: Cow<'a, Text<'a>>,
    pub buttons: &'a [Button<'a, Id>],
}

impl<Id: Clone + Debug + Eq + Hash> Component for Dialog<'_, Id> {
    type Id = Id;

    fn id(&self) -> Self::Id {
        self.id.clone()
    }

    fn draw(&self, viewport: &mut Viewport<Self::Id>, _x: isize, _y: isize) {
        let Self {
            id: _,
            title,
            body,
            buttons,
        } = self;
        let rect = {
            let border_size = 2;
            let body_lines = body.lines.len();
            let rect = centered_rect(
                viewport.rect(),
                RectSize {
                    // FIXME: we might want to limit the width of the text and
                    // let `Paragraph` wrap it.
                    width: body.width() + border_size,
                    height: body_lines + border_size,
                },
                60,
                20,
            );

            let paragraph = Paragraph::new((*body.as_ref()).clone()).block(
                Block::default()
                    .title(title.as_ref())
                    .borders(Borders::all()),
            );
            let tui_rect = viewport.translate_rect(rect);
            viewport.draw_widget(tui_rect, Clear);
            viewport.draw_widget(tui_rect, paragraph);

            rect
        };

        let mut bottom_x = rect.x + rect.width.unwrap_isize() - 1;
        let bottom_y = rect.y + rect.height.unwrap_isize() - 1;
        for button in buttons.iter() {
            bottom_x -= button.width().unwrap_isize();
            let button_rect = viewport.draw_component(bottom_x, bottom_y, button);
            bottom_x = button_rect.x - 1;
        }
    }
}
