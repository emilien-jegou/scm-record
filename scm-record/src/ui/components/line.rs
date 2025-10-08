use crate::render::{Component, Rect, Viewport};
use crate::types::ChangeType;
use crate::ui::components::app::SelectionKey;
use crate::ui::components::widgets::TristateBox;
use crate::ui::components::ComponentId;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use std::fmt::Debug;
use unicode_width::UnicodeWidthChar;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct LineKey {
    pub commit_idx: usize,
    pub file_idx: usize,
    pub section_idx: usize,
    pub line_idx: usize,
}

#[derive(Clone, Debug)]
pub enum SectionLineViewInner<'a> {
    Unchanged {
        line: &'a str,
        line_num: usize,
    },
    Changed {
        toggle_box: TristateBox<ComponentId>,
        change_type: ChangeType,
        line: &'a str,
    },
}

fn replace_control_character(character: char) -> Option<&'static str> {
    match character {
        // Characters end up writing over each-other and end up
        // displaying incorrectly if ignored. Replacing tabs
        // with a known length string fixes the issue for now.
        '\t' => Some("→   "),
        '\n' => Some("⏎"),
        '\r' => Some("␍"),

        '\x00' => Some("␀"),
        '\x01' => Some("␁"),
        '\x02' => Some("␂"),
        '\x03' => Some("␃"),
        '\x04' => Some("␄"),
        '\x05' => Some("␅"),
        '\x06' => Some("␆"),
        '\x07' => Some("␇"),
        '\x08' => Some("␈"),
        // '\x09' ('\t') handled above
        // '\x0A' ('\n') handled above
        '\x0B' => Some("␋"),
        '\x0C' => Some("␌"),
        // '\x0D' ('\r') handled above
        '\x0E' => Some("␎"),
        '\x0F' => Some("␏"),
        '\x10' => Some("␐"),
        '\x11' => Some("␑"),
        '\x12' => Some("␒"),
        '\x13' => Some("␓"),
        '\x14' => Some("␔"),
        '\x15' => Some("␕"),
        '\x16' => Some("␖"),
        '\x17' => Some("␗"),
        '\x18' => Some("␘"),
        '\x19' => Some("␙"),
        '\x1A' => Some("␚"),
        '\x1B' => Some("␛"),
        '\x1C' => Some("␜"),
        '\x1D' => Some("␝"),
        '\x1E' => Some("␞"),
        '\x1F' => Some("␟"),

        '\x7F' => Some("␡"),

        c if c.width().unwrap_or_default() == 0 => Some("�"),

        _ => None,
    }
}

/// Split the line into a sequence of [`Span`]s where control characters are
/// replaced with styled [`Span`]'s and push them to the [`spans`] argument.
pub fn push_spans_from_line<'line>(line: &'line str, spans: &mut Vec<Span<'line>>) {
    const CONTROL_CHARACTER_STYLE: Style = Style::new().fg(Color::DarkGray);

    let mut last_index = 0;
    // Find index of the start of each character to replace
    for (idx, char) in line.match_indices(|char| replace_control_character(char).is_some()) {
        // Push the string leading up to the character and the styled replacement string
        if let Some(replacement_string) = char.chars().next().and_then(replace_control_character) {
            spans.push(Span::raw(&line[last_index..idx]));
            spans.push(Span::styled(replacement_string, CONTROL_CHARACTER_STYLE));
            // Move the "cursor" to just after the character we're replacing
            last_index = idx + char.len();
        }
    }
    // Append anything remaining after the last replacement
    let remaining_line = &line[last_index..];
    if !remaining_line.is_empty() {
        spans.push(Span::raw(remaining_line));
    }
}

#[derive(Clone, Debug)]
pub struct SectionLineView<'a> {
    pub line_key: LineKey,
    pub inner: SectionLineViewInner<'a>,
}

impl Component for SectionLineView<'_> {
    type Id = ComponentId;

    fn id(&self) -> Self::Id {
        ComponentId::SelectableItem(SelectionKey::Line(self.line_key))
    }

    fn draw(&self, viewport: &mut Viewport<Self::Id>, x: isize, y: isize) {
        viewport.draw_blank(Rect {
            x: viewport.mask_rect().x,
            y,
            width: viewport.mask_rect().width,
            height: 1,
        });

        match &self.inner {
            SectionLineViewInner::Unchanged { line, line_num } => {
                // Pad the number in 5 columns because that will align the
                // beginning of the actual text with the `+`/`-` of the changed
                // lines.
                let line_number = Span::raw(format!("{line_num:5} "));
                let mut spans = vec![line_number];
                push_spans_from_line(line, &mut spans);

                const UI_UNCHANGED_STYLE: Style = Style::new().fg(Color::Gray).add_modifier(Modifier::DIM);
                viewport.draw_text(x, y, Line::from(spans).style(UI_UNCHANGED_STYLE));
            }

            SectionLineViewInner::Changed {
                toggle_box,
                change_type,
                line,
            } => {
                let toggle_box_rect = viewport.draw_component(x, y, toggle_box);
                let x = toggle_box_rect.end_x() + 1;

                let (change_type_text, changed_line_style) = match change_type {
                    ChangeType::Added => ("+ ", Style::default().fg(Color::Green)),
                    ChangeType::Removed => ("- ", Style::default().fg(Color::Red)),
                };

                let mut spans = vec![Span::raw(change_type_text)];
                push_spans_from_line(line, &mut spans);

                viewport.draw_text(x, y, Line::from(spans).style(changed_line_style));
            }
        }
    }
}
