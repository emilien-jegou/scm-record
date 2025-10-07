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
    pub use_unicode: bool,
    pub id: Id,
    pub tristate: Tristate,
    pub icon_style: TristateIconStyle,
    pub is_focused: bool,
    pub is_read_only: bool,
}

impl<Id> TristateBox<Id> {
    pub fn text(&self) -> String {
        let Self {
            use_unicode,
            id: _,
            tristate,
            icon_style,
            is_focused,
            is_read_only,
        } = self;

        let (l, r) = match (is_read_only, is_focused) {
            (true, _) => ("<", ">"),
            (false, false) => ("[", "]"),
            (false, true) => ("(", ")"),
        };

        let inner = match (icon_style, tristate, use_unicode) {
            (TristateIconStyle::Expand, Tristate::False, _) => "+",
            (TristateIconStyle::Expand, Tristate::True, _) => "-",
            (TristateIconStyle::Expand, Tristate::Partial, false) => "~",
            (TristateIconStyle::Expand, Tristate::Partial, true) => "±",

            (TristateIconStyle::Check, Tristate::False, false) => " ",
            (TristateIconStyle::Check, Tristate::True, false) => "*",
            (TristateIconStyle::Check, Tristate::Partial, false) => "~",

            (TristateIconStyle::Check, Tristate::False, true) => " ",
            (TristateIconStyle::Check, Tristate::True, true) => "●",
            (TristateIconStyle::Check, Tristate::Partial, true) => "◐",
        };
        format!("{l}{inner}{r}")
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
            Style::default().add_modifier(Modifier::BOLD)
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
    viewport.set_style(rect, Style::default().add_modifier(Modifier::REVERSED));
}
