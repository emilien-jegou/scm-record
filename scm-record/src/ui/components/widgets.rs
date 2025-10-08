use crate::render::{Component, Rect, Viewport};
use crate::Tristate;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use std::borrow::Cow;
use std::fmt::Debug;
use std::hash::Hash;

#[derive(Clone, Debug)]
pub enum TristateIconStyle {
    Check,
    Expand,
}

#[derive(Clone, Debug)]
pub struct TristateBox<Id> {
    pub id: Id,
    pub tristate: Tristate,
    pub icon_style: TristateIconStyle,
    pub is_read_only: bool,
}

impl<Id> TristateBox<Id> {
    pub fn text(&self) -> String {
        let Self {
            tristate,
            icon_style,
            ..
        } = self;

        match icon_style {
            // Render expand/collapse icons: ▶ for collapsed, ▼ for expanded.
            // These icons do not have brackets.
            TristateIconStyle::Expand => match tristate {
                Tristate::False => "▶".to_string(),
                // A partially-selected container is still visually expanded.
                Tristate::True | Tristate::Partial => "▼".to_string(),
            },
            // Render selection state icons.
            TristateIconStyle::Check => match tristate {
                Tristate::False => "[ ]".to_string(),
                Tristate::True => "[*]".to_string(),
                Tristate::Partial => "[~]".to_string(),
            },
        }
    }

    pub fn color(&self) -> Color {
        let Self {
            tristate,
            icon_style,
            ..
        } = self;

        match icon_style {
            TristateIconStyle::Expand => Color::Magenta,
            // Render selection state icons.
            TristateIconStyle::Check => match tristate {
                Tristate::False => Color::DarkGray,
                Tristate::True => Color::Blue,
                Tristate::Partial => Color::Yellow,
            },
        }
    }
}

impl<Id: Clone + Debug + Eq + Hash> Component for TristateBox<Id> {
    type Id = Id;

    fn id(&self) -> Self::Id {
        self.id.clone()
    }

    fn draw(&self, viewport: &mut Viewport<Self::Id>, x: isize, y: isize) {
        let style = if self.is_read_only {
            Style::default().fg(Color::Gray).add_modifier(Modifier::DIM)
        } else {
            Style::default().fg(self.color()).add_modifier(Modifier::BOLD)
        };
        let span = Span::styled(self.text(), style);
        viewport.draw_span(x, y, &span);
    }
}

pub struct Button<'a, Id> {
    pub id: Id,
    pub label: Cow<'a, str>,
    pub style: Style,
    pub is_focused: bool,
}

impl<Id> Button<'_, Id> {
    pub fn span(&self) -> Span<'_> {
        let Self {
            id: _,
            label,
            style,
            is_focused,
        } = self;
        if *is_focused {
            Span::styled(format!("({label})"), style.add_modifier(Modifier::REVERSED))
        } else {
            Span::styled(format!("[{label}]"), *style)
        }
    }

    pub fn width(&self) -> usize {
        self.span().width()
    }
}

impl<Id: Clone + Debug + Eq + Hash> Component for Button<'_, Id> {
    type Id = Id;

    fn id(&self) -> Self::Id {
        self.id.clone()
    }

    fn draw(&self, viewport: &mut Viewport<Self::Id>, x: isize, y: isize) {
        let span = self.span();
        viewport.draw_span(x, y, &span);
    }
}

pub fn highlight_rect<Id: Clone + Debug + Eq + Hash>(viewport: &mut Viewport<Id>, rect: Rect) {
    viewport.set_style(rect, Style::default().bg(Color::Rgb(38, 38, 38)));
}
