use crate::render::{Component, DrawnRect, Mask, Viewport};
use crate::ui::components::commit_message_view::CommitViewMode;
use crate::ui::components::commit_view::CommitView;
use crate::ui::components::file::FileKey;
use crate::ui::components::help_dialog::HelpDialog;
use crate::ui::components::line::LineKey;
use crate::ui::components::section::SectionKey;
use crate::ui::components::ComponentId;
use crate::util::UsizeExt;
use std::collections::BTreeMap;
use std::fmt::Debug;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum SelectionKey {
    None,
    File(FileKey),
    Section(SectionKey),
    Line(LineKey),
}

impl Default for SelectionKey {
    fn default() -> Self {
        Self::None
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct AppDebugInfo {
    pub term_height: usize,
    pub scroll_offset_y: isize,
    pub selection_key: SelectionKey,
    pub selection_key_y: Option<isize>,
    pub drawn_rects: BTreeMap<ComponentId, DrawnRect>, // sorted for determinism
}

#[derive(Clone, Debug)]
pub struct AppView<'a> {
    pub debug_info: Option<AppDebugInfo>,
    pub commit_view_mode: CommitViewMode,
    pub commit_views: Vec<CommitView<'a>>,
    pub help_dialog: Option<HelpDialog>,
}

impl Component for AppView<'_> {
    type Id = ComponentId;

    fn id(&self) -> Self::Id {
        ComponentId::App
    }

    fn draw(&self, viewport: &mut Viewport<Self::Id>, _x: isize, _y: isize) {
        let Self {
            debug_info,
            commit_view_mode,
            commit_views,
            help_dialog,
        } = self;

        if let Some(debug_info) = debug_info {
            viewport.debug(format!("app debug info: {debug_info:#?}"));
        }

        let viewport_rect = viewport.mask_rect();

        let commit_view_width = match commit_view_mode {
            CommitViewMode::Inline => viewport.rect().width,
            CommitViewMode::Adjacent => {
                const MAX_COMMIT_VIEW_WIDTH: usize = 120;
                MAX_COMMIT_VIEW_WIDTH
                    .min(viewport.rect().width.saturating_sub(CommitView::MARGIN) / 2)
            }
        };
        let commit_views_mask = Mask {
            x: viewport_rect.x,
            y: viewport_rect.y,
            width: Some(viewport_rect.width),
            height: None,
        };
        viewport.with_mask(commit_views_mask, |viewport| {
            let mut commit_view_x = 0;
            for commit_view in commit_views {
                let commit_view_mask = Mask {
                    x: commit_views_mask.x + commit_view_x,
                    y: commit_views_mask.y,
                    width: Some(commit_view_width),
                    height: None,
                };
                let commit_view_rect = viewport.with_mask(commit_view_mask, |viewport| {
                    viewport.draw_component(commit_view_x, 0, commit_view)
                });
                commit_view_x += (CommitView::MARGIN
                    + commit_view_mask.apply(commit_view_rect).width)
                    .unwrap_isize();
            }
        });

        if let Some(help_dialog) = help_dialog {
            viewport.draw_component(0, 0, help_dialog);
        }
    }
}
