use crate::render::{centered_rect, Component, RectSize, Viewport};
use crate::ui::components::widgets::Button;
use crate::ui::components::ComponentId;
use crate::util::UsizeExt;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use std::borrow::Cow;
use std::fmt::Debug;
use std::hash::Hash;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum QuitDialogButtonId {
    Quit,
    GoBack,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuitDialog {
    pub num_commit_messages: usize,
    pub num_changed_files: usize,
    pub focused_button: QuitDialogButtonId,
}

impl Component for QuitDialog {
    type Id = ComponentId;

    fn id(&self) -> Self::Id {
        ComponentId::QuitDialog
    }

    fn draw(&self, viewport: &mut Viewport<Self::Id>, _x: isize, _y: isize) {
        let Self {
            num_commit_messages,
            num_changed_files,
            focused_button,
        } = self;
        let title = "Quit";
        let alert_items = {
            let mut result = Vec::new();
            if *num_commit_messages > 0 {
                result.push(format!(
                    "{num_commit_messages} {}",
                    if *num_commit_messages == 1 {
                        "message"
                    } else {
                        "messages"
                    }
                ));
            }
            if *num_changed_files > 0 {
                result.push(format!(
                    "{num_changed_files} {}",
                    if *num_changed_files == 1 {
                        "file"
                    } else {
                        "files"
                    }
                ));
            }
            result
        };
        let alert = if alert_items.is_empty() {
            // Shouldn't happen.
            "".to_string()
        } else {
            format!("You have changes to {}. ", alert_items.join(" and "))
        };
        let body = Text::from(format!("{alert}Are you sure you want to quit?",));

        let quit_button = Button {
            id: ComponentId::QuitDialogButton(QuitDialogButtonId::Quit),
            label: Cow::Borrowed("Quit"),
            style: Style::default(),
            is_focused: match focused_button {
                QuitDialogButtonId::Quit => true,
                QuitDialogButtonId::GoBack => false,
            },
        };
        let go_back_button = Button {
            id: ComponentId::QuitDialogButton(QuitDialogButtonId::GoBack),
            label: Cow::Borrowed("Go Back"),
            style: Style::default(),
            is_focused: match focused_button {
                QuitDialogButtonId::GoBack => true,
                QuitDialogButtonId::Quit => false,
            },
        };
        let buttons = [quit_button, go_back_button];

        let dialog = Dialog {
            id: ComponentId::QuitDialog,
            title: Cow::Borrowed(title),
            body: Cow::Owned(body),
            buttons: &buttons,
        };
        viewport.draw_component(0, 0, &dialog);
    }
}

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

pub struct Dialog<'a, Id> {
    id: Id,
    title: Cow<'a, str>,
    body: Cow<'a, Text<'a>>,
    buttons: &'a [Button<'a, Id>],
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
