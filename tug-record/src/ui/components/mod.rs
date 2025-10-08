use crate::ui::components::{app::SelectionKey, file::FileKey};

pub mod app;
pub mod commit_message_view;
pub mod commit_view;
pub mod dialog;
pub mod file;
pub mod help_dialog;
pub mod line;
pub mod section;
pub mod widgets;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum ComponentId {
    App,
    AppFiles,
    CommitMessageView,
    CommitEditMessageButton(usize),
    FileViewHeader(FileKey),
    SelectableItem(SelectionKey),
    ToggleBox(SelectionKey),
    ExpandBox(SelectionKey),
    HelpDialog,
    HelpDialogQuitButton,
}
