use crate::render::{centered_rect, Component, Mask, Rect, RectSize, Viewport};
use crate::ui::components::app::AppDebugInfo;
use crate::ui::components::commit_message_view::CommitMessageView;
use crate::ui::components::file::{FileView, FileViewHeader};
use crate::ui::components::ComponentId;
use crate::util::UsizeExt;
use ratatui::text::Span;
use std::fmt::Debug;

#[derive(Clone, Debug)]
pub struct CommitView<'a> {
    pub debug_info: Option<&'a AppDebugInfo>,
    pub commit_message_view: CommitMessageView<'a>,
    pub file_views: Vec<FileView<'a>>,
}

impl CommitView<'_> {
    pub const MARGIN: usize = 1;
}

impl Component for CommitView<'_> {
    type Id = ComponentId;

    fn id(&self) -> Self::Id {
        ComponentId::AppFiles
    }

    fn draw(&self, viewport: &mut Viewport<Self::Id>, x: isize, y: isize) {
        let Self {
            debug_info,
            commit_message_view,
            file_views,
        } = self;

        let commit_message_view_rect = viewport.draw_component(x, y, commit_message_view);
        if file_views.is_empty() {
            let message = "There are no changes to view.";
            let message_rect = centered_rect(
                Rect {
                    x,
                    y,
                    width: viewport.mask_rect().width,
                    height: viewport.mask_rect().height,
                },
                RectSize {
                    width: message.len(),
                    height: 1,
                },
                50,
                50,
            );
            viewport.draw_text(message_rect.x, message_rect.y, Span::raw(message));
            return;
        }

        let mut y = y;
        y += commit_message_view_rect.height.unwrap_isize();
        for file_view in file_views {
            let file_view_rect = {
                let file_view_mask = Mask {
                    x,
                    y,
                    width: viewport.mask().width,
                    height: None,
                };
                viewport.with_mask(file_view_mask, |viewport| {
                    viewport.draw_component(x, y, file_view)
                })
            };

            // Render a sticky header if necessary.
            let mask = viewport.mask();
            if file_view_rect.y < mask.y
                && mask.y < file_view_rect.y + file_view_rect.height.unwrap_isize()
            {
                viewport.with_mask(
                    Mask {
                        x,
                        y: mask.y,
                        width: Some(viewport.mask_rect().width),
                        height: Some(1),
                    },
                    |viewport| {
                        viewport.draw_component(
                            x,
                            mask.y,
                            &FileViewHeader {
                                file_key: file_view.file_key,
                                path: file_view.path,
                                old_path: file_view.old_path,
                                is_selected: file_view.is_header_selected,
                                toggle_box: file_view.toggle_box.clone(),
                                expand_box: file_view.expand_box.clone(),
                            },
                        );
                    },
                );
            }

            y += file_view_rect.height.unwrap_isize();

            if debug_info.is_some() {
                viewport.debug(format!(
                    "file {} dims: {file_view_rect:?}",
                    file_view.path.to_string_lossy()
                ));
            }
        }
    }
}
