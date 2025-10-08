use std::cmp::min;

use ratatui::{
    style::{Color, Modifier, Style},
    text::Span,
};

use crate::{
    render::{Component, Rect, Viewport},
    ui::components::{
        app::SelectionKey,
        line::{LineKey, SectionLineView, SectionLineViewInner},
        widgets::{highlight_rect, TristateBox, TristateIconStyle},
        ComponentId,
    },
    util::UsizeExt,
    FileMode, Section, SectionChangedLine, Tristate,
};

pub const NUM_CONTEXT_LINES: usize = 12;

#[derive(Clone, Debug)]
pub enum SectionSelection {
    SectionHeader,
    ChangedLine(usize),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct SectionKey {
    pub commit_idx: usize,
    pub file_idx: usize,
    pub section_idx: usize,
}

#[derive(Clone, Debug)]
pub struct SectionView<'a> {
    pub is_read_only: bool,
    pub section_key: SectionKey,
    pub toggle_box: TristateBox<ComponentId>,
    pub expand_box: TristateBox<ComponentId>,
    pub selection: Option<SectionSelection>,
    pub total_num_sections: usize,
    pub editable_section_num: usize,
    pub total_num_editable_sections: usize,
    pub section: &'a Section<'a>,
    pub line_start_num: usize,
}

impl SectionView<'_> {
    pub fn is_expanded(&self) -> bool {
        match self.expand_box.tristate {
            Tristate::False => false,
            Tristate::Partial => {
                // Shouldn't happen.
                true
            }
            Tristate::True => true,
        }
    }
}

// ... (imports and struct definitions remain the same) ...

// ANCHOR: updated_sectionview_component_impl
impl Component for SectionView<'_> {
    type Id = ComponentId;

    fn id(&self) -> Self::Id {
        ComponentId::SelectableItem(SelectionKey::Section(self.section_key))
    }

    fn draw(&self, viewport: &mut Viewport<Self::Id>, x: isize, y: isize) {
        let Self {
            is_read_only,
            section_key,
            toggle_box,
            expand_box,
            selection,
            total_num_sections,
            editable_section_num,
            total_num_editable_sections,
            section,
            line_start_num,
        } = self;
        viewport.draw_blank(Rect {
            x,
            y,
            width: viewport.mask_rect().width,
            height: 1,
        });

        let SectionKey {
            commit_idx,
            file_idx,
            section_idx,
        } = *section_key;
        match section {
            Section::Unchanged { lines } => {
                // ... (this entire block remains unchanged)
                if lines.is_empty() {
                    return;
                }

                let lines: Vec<_> = lines.iter().enumerate().collect();
                let is_first_section = section_idx == 0;
                let is_last_section = section_idx + 1 == *total_num_sections;
                let before_ellipsis_lines = &lines[..min(NUM_CONTEXT_LINES, lines.len())];
                let after_ellipsis_lines = &lines[lines.len().saturating_sub(NUM_CONTEXT_LINES)..];

                match (before_ellipsis_lines, after_ellipsis_lines) {
                    ([.., (last_before_idx, _)], [(first_after_idx, _), ..])
                        if *last_before_idx + 1 >= *first_after_idx
                            && !is_first_section
                            && !is_last_section =>
                    {
                        let first_before_idx = before_ellipsis_lines.first().unwrap().0;
                        let last_after_idx = after_ellipsis_lines.last().unwrap().0;
                        let overlapped_lines = &lines[first_before_idx..=last_after_idx];
                        let overlapped_lines = if is_first_section {
                            &overlapped_lines
                                [overlapped_lines.len().saturating_sub(NUM_CONTEXT_LINES)..]
                        } else if is_last_section {
                            &overlapped_lines[..lines.len().min(NUM_CONTEXT_LINES)]
                        } else {
                            overlapped_lines
                        };
                        for (dy, (line_idx, line)) in overlapped_lines.iter().enumerate() {
                            let line_view = SectionLineView {
                                line_key: LineKey {
                                    commit_idx,
                                    file_idx,
                                    section_idx,
                                    line_idx: *line_idx,
                                },
                                inner: SectionLineViewInner::Unchanged {
                                    line: line.as_ref(),
                                    line_num: line_start_num + line_idx,
                                },
                            };
                            viewport.draw_component(x + 2, y + dy.unwrap_isize(), &line_view);
                        }
                        return;
                    }
                    _ => {}
                };

                let mut dy = 0;
                if !is_first_section {
                    for (line_idx, line) in before_ellipsis_lines {
                        let line_view = SectionLineView {
                            line_key: LineKey {
                                commit_idx,
                                file_idx,
                                section_idx,
                                line_idx: *line_idx,
                            },
                            inner: SectionLineViewInner::Unchanged {
                                line: line.as_ref(),
                                line_num: line_start_num + line_idx,
                            },
                        };
                        viewport.draw_component(x + 2, y + dy, &line_view);
                        dy += 1;
                    }
                }

                let should_render_ellipsis = lines.len() > NUM_CONTEXT_LINES;
                if should_render_ellipsis {
                    let ellipsis = "\u{22EE}";
                    viewport.draw_span(
                        x + 6, // align with line numbering
                        y + dy,
                        &Span::styled(ellipsis, Style::default().add_modifier(Modifier::DIM)),
                    );
                    dy += 1;
                }

                if !is_last_section {
                    for (line_idx, line) in after_ellipsis_lines {
                        let line_view = SectionLineView {
                            line_key: LineKey {
                                commit_idx,
                                file_idx,
                                section_idx,
                                line_idx: *line_idx,
                            },
                            inner: SectionLineViewInner::Unchanged {
                                line: line.as_ref(),
                                line_num: line_start_num + line_idx,
                            },
                        };
                        viewport.draw_component(x + 2, y + dy, &line_view);
                        dy += 1;
                    }
                }
            }

            Section::Changed { lines } => {
                // Draw section header from left to right.
                let mut cursor_x = x;

                // 1. Draw the expand box.
                let expand_box_rect = viewport.draw_component(cursor_x, y, expand_box);
                cursor_x += expand_box_rect.width.unwrap_isize() + 1;

                // 2. Draw the toggle box.
                let toggle_box_rect = viewport.draw_component(cursor_x, y, toggle_box);
                cursor_x += toggle_box_rect.width.unwrap_isize() + 1;

                // 3. Draw the section description text.
                viewport.draw_text(
                    cursor_x,
                    y,
                    Span::styled(
                        format!("Section {editable_section_num}/{total_num_editable_sections}"),
                        // Use a distinct color for hunk headers.
                        Style::default().fg(Color::LightMagenta),
                    ),
                );

                match selection {
                    Some(SectionSelection::SectionHeader) => {
                        highlight_rect(
                            viewport,
                            Rect {
                                x: viewport.mask_rect().x,
                                y,
                                width: viewport.mask_rect().width,
                                height: 1,
                            },
                        );
                    }
                    Some(SectionSelection::ChangedLine(_)) | None => {}
                }

                if self.is_expanded() {
                    // Draw changed lines.
                    let y = y + 1;
                    for (line_idx, line) in lines.iter().enumerate() {
                        let SectionChangedLine {
                            is_checked,
                            change_type,
                            line,
                        } = line;
                        let is_focused = match selection {
                            Some(SectionSelection::ChangedLine(selected_line_idx)) => {
                                line_idx == *selected_line_idx
                            }
                            Some(SectionSelection::SectionHeader) | None => false,
                        };
                        let line_key = LineKey {
                            commit_idx,
                            file_idx,
                            section_idx,
                            line_idx,
                        };
                        let toggle_box = TristateBox {
                            id: ComponentId::ToggleBox(SelectionKey::Line(line_key)),
                            icon_style: TristateIconStyle::Check,
                            tristate: Tristate::from(*is_checked),
                            is_read_only: *is_read_only,
                        };
                        let line_view = SectionLineView {
                            line_key,
                            inner: SectionLineViewInner::Changed {
                                toggle_box,
                                change_type: *change_type,
                                line: line.as_ref(),
                            },
                        };
                        let y = y + line_idx.unwrap_isize();
                        viewport.draw_component(x + 2, y, &line_view);
                        if is_focused {
                            highlight_rect(
                                viewport,
                                Rect {
                                    x: viewport.mask_rect().x,
                                    y,
                                    width: viewport.mask_rect().width,
                                    height: 1,
                                },
                            );
                        }
                    }
                }
            }

            // ... (Section::FileMode and Section::Binary remain unchanged) ...
            Section::FileMode { is_checked, mode } => {
                let is_focused = match selection {
                    Some(SectionSelection::SectionHeader) => true,
                    Some(SectionSelection::ChangedLine(_)) | None => false,
                };
                let section_key = SectionKey {
                    commit_idx,
                    file_idx,
                    section_idx,
                };
                let selection_key = SelectionKey::Section(section_key);
                let toggle_box = TristateBox {
                    id: ComponentId::ToggleBox(selection_key),
                    icon_style: TristateIconStyle::Check,
                    tristate: Tristate::from(*is_checked),
                    is_read_only: *is_read_only,
                };
                let toggle_box_rect = viewport.draw_component(x, y, &toggle_box);
                let x = x + toggle_box_rect.width.unwrap_isize() + 1;

                let text = match mode {
                    // TODO: It would be nice to render this as 'file was created with mode x' but we don't have access
                    // to the file's mode to see if it was absent before here.
                    FileMode::Unix(mode) => format!("File mode set to {mode:o}"),
                    FileMode::Absent => "File deleted".to_owned(),
                };

                viewport.draw_text(x, y, Span::styled(text, Style::default().fg(Color::Magenta)));
                if is_focused {
                    highlight_rect(
                        viewport,
                        Rect {
                            x: viewport.mask_rect().x,
                            y,
                            width: viewport.mask_rect().width,
                            height: 1,
                        },
                    );
                }
            }

            Section::Binary {
                is_checked,
                old_description,
                new_description,
            } => {
                let is_focused = match selection {
                    Some(SectionSelection::SectionHeader) => true,
                    Some(SectionSelection::ChangedLine(_)) | None => false,
                };
                let section_key = SectionKey {
                    commit_idx,
                    file_idx,
                    section_idx,
                };
                let toggle_box = TristateBox {
                    id: ComponentId::ToggleBox(SelectionKey::Section(section_key)),
                    icon_style: TristateIconStyle::Check,
                    tristate: Tristate::from(*is_checked),
                    is_read_only: *is_read_only,
                };
                let toggle_box_rect = viewport.draw_component(x, y, &toggle_box);
                let x = x + toggle_box_rect.width.unwrap_isize() + 1;

                let text = {
                    let mut result =
                        vec![if old_description.is_some() || new_description.is_some() {
                            "binary contents:"
                        } else {
                            "binary contents"
                        }
                        .to_string()];
                    let description: Vec<_> = [old_description, new_description]
                        .iter()
                        .copied()
                        .flatten()
                        .map(|s| s.as_ref())
                        .collect();
                    result.push(description.join(" -> "));
                    format!("({})", result.join(" "))
                };
                viewport.draw_text(x, y, Span::styled(text, Style::default().fg(Color::Magenta)));

                if is_focused {
                    highlight_rect(
                        viewport,
                        Rect {
                            x: viewport.mask_rect().x,
                            y,
                            width: viewport.mask_rect().width,
                            height: 1,
                        },
                    );
                }
            }
        }
    }
}
