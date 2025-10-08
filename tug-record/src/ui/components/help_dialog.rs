use crate::render::{Component, Viewport};
use crate::ui::components::dialog::Dialog;
use crate::ui::components::widgets::Button;
use crate::ui::components::ComponentId;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span, Text};
use std::borrow::Cow;
use std::fmt::Debug;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct HelpDialog();

impl Component for HelpDialog {
    type Id = ComponentId;

    fn id(&self) -> Self::Id {
        ComponentId::HelpDialog
    }

    fn draw(&self, viewport: &mut Viewport<Self::Id>, _: isize, _: isize) {
        let title = "Help";
        let body = Text::from(vec![
            Line::from("Use these keyboard shortcuts:"),
            Line::from(""),
            Line::from(vec![
                Span::raw("    "),
                Span::styled("General", Style::new().bold().underlined()),
                Span::raw("                             "),
                Span::styled("Navigation", Style::new().bold().underlined()),
            ]),
            Line::from(
                "    Quit/Cancel             q           Next/Prev               j/k or ↓/↑",
            ),
            Line::from("    Confirm changes         c           Next/Prev of same type  PgDn/PgUp"),
            Line::from("    Force quit              ^c          Move out & fold         h or ←"),
            Line::from(
                "                                        Move out & don't fold   H or Shift-←    ",
            ),
            Line::from(vec![
                Span::raw("    "),
                Span::styled("View controls", Style::new().bold().underlined()),
                Span::raw("                       Move in & unfold        l or →"),
            ]),
            Line::from("    Expand/Collapse         f"),
            Line::from(vec![
                Span::raw("    Expand/Collapse all     F           "),
                Span::styled("Scrolling", Style::new().bold().underlined()),
            ]),
            Line::from("    Edit commit message     e           Scroll up/down          ^y/^e"),
            Line::from("                                                             or ^↑/^↓"),
            Line::from(vec![
                Span::raw("    "),
                Span::styled("Selection", Style::new().bold().underlined()),
                Span::raw("                           Page up/down            ^b/^f"),
            ]),
            Line::from(
                "    Toggle current          Space                            or ^PgUp/^PgDn",
            ),
            Line::from("    Toggle and advance      Enter       Previous/Next page      ^u/^d"),
            Line::from("    Invert all              a"),
            Line::from("    Invert all uniformly    A"),
        ]);

        let quit_button = Button {
            id: ComponentId::HelpDialogQuitButton,
            label: Cow::Borrowed("Close"),
            style: Style::default(),
            is_focused: true,
        };

        let buttons = [quit_button];
        let dialog = Dialog {
            id: self.id(),
            title: Cow::Borrowed(title),
            body: Cow::Borrowed(&body),
            buttons: &buttons,
        };
        viewport.draw_component(0, 0, &dialog);
    }
}
