use crate::render::{Component, Rect, Viewport};
use crate::types::Tristate;
use crate::ui::components::app::SelectionKey;
use crate::ui::components::widgets::{highlight_rect, TristateBox};
use crate::ui::components::{section, ComponentId};
use crate::util::UsizeExt;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use std::collections::HashSet;
use std::fmt::Debug;
use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct FileKey {
    pub commit_idx: usize,
    pub file_idx: usize,
}

#[derive(Clone, Debug)]
pub struct FileView<'a> {
    pub debug: bool,
    pub file_key: FileKey,
    pub toggle_box: TristateBox<ComponentId>,
    pub expand_box: TristateBox<ComponentId>,
    pub is_header_selected: bool,
    pub old_path: Option<&'a Path>,
    pub path: &'a Path,
    pub section_views: Vec<section::SectionView<'a>>,
}

impl FileView<'_> {
    fn is_expanded(&self) -> bool {
        match self.expand_box.tristate {
            Tristate::False => false,
            Tristate::Partial | Tristate::True => true,
        }
    }
}

impl Component for FileView<'_> {
    type Id = ComponentId;

    fn id(&self) -> Self::Id {
        ComponentId::SelectableItem(SelectionKey::File(self.file_key))
    }

    fn draw(&self, viewport: &mut Viewport<Self::Id>, x: isize, y: isize) {
        let Self {
            debug,
            file_key,
            toggle_box,
            expand_box,
            old_path,
            path,
            section_views,
            is_header_selected,
        } = self;

        let file_view_header_rect = viewport.draw_component(
            x,
            y,
            &FileViewHeader {
                file_key: *file_key,
                path,
                old_path: *old_path,
                is_selected: *is_header_selected,
                toggle_box: toggle_box.clone(),
                expand_box: expand_box.clone(),
            },
        );
        if self.is_expanded() {
            let x = x + 2;
            let mut section_y = y + file_view_header_rect.height.unwrap_isize();
            let expanded_sections: HashSet<usize> = section_views
                .iter()
                .enumerate()
                .filter_map(|(i, view)| {
                    if view.is_expanded() && view.section.is_editable() {
                        return Some(i);
                    }
                    None
                })
                .collect();
            for (i, section_view) in section_views.iter().enumerate() {
                // Skip this section if it is an un-editable context section and
                // none of the editable sections surrounding it are expanded.
                let context_section = !section_view.section.is_editable();
                let prev_is_collapsed = i == 0 || !expanded_sections.contains(&(i - 1));
                let next_is_collapsed = !expanded_sections.contains(&(i + 1));
                if context_section && prev_is_collapsed && next_is_collapsed {
                    continue;
                }

                let section_rect = viewport.draw_component(x, section_y, section_view);
                section_y += section_rect.height.unwrap_isize();

                if *debug {
                    viewport.debug(format!("section dims: {section_rect:?}",));
                }
            }
        }
    }
}

pub struct FileViewHeader<'a> {
    pub file_key: FileKey,
    pub path: &'a Path,
    pub old_path: Option<&'a Path>,
    pub is_selected: bool,
    pub toggle_box: TristateBox<ComponentId>,
    pub expand_box: TristateBox<ComponentId>,
}

impl Component for FileViewHeader<'_> {
    type Id = ComponentId;

    fn id(&self) -> Self::Id {
        let Self {
            file_key,
            path: _,
            old_path: _,
            is_selected: _,
            toggle_box: _,
            expand_box: _,
        } = self;
        ComponentId::FileViewHeader(*file_key)
    }

    // ANCHOR: updated_fileviewheader_draw
    fn draw(&self, viewport: &mut Viewport<Self::Id>, x: isize, y: isize) {
        let Self {
            file_key: _,
            path,
            old_path,
            is_selected,
            toggle_box,
            expand_box,
        } = self;

        // Draw components left-to-right: expand icon -> select checkbox -> file path
        let mut cursor_x = x;

        let expand_box_rect = viewport.draw_component(cursor_x, y, expand_box);
        cursor_x += expand_box_rect.width.unwrap_isize() + 1; // Add 1 for spacing

        let toggle_box_rect = viewport.draw_component(cursor_x, y, toggle_box);
        cursor_x += toggle_box_rect.width.unwrap_isize() + 1; // Add 1 for spacing

        viewport.draw_text(
            cursor_x,
            y,
            Span::styled(
                format!(
                    "{}{}",
                    match old_path {
                        Some(old_path) => format!("{} → ", old_path.to_string_lossy()),
                        None => String::new(),
                    },
                    path.to_string_lossy(),
                ),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
        );

        // 4. Highlight the entire line if it's selected.
        if *is_selected {
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
    // ANCHOR_END: updated_fileviewheader_draw
}
