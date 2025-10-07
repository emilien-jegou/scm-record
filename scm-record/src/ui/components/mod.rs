use crate::ui::components::{app::SelectionKey, dialog::QuitDialogButtonId, file::FileKey};

pub mod app;
pub mod commit;
pub mod dialog;
pub mod file;
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
    QuitDialog,
    QuitDialogButton(QuitDialogButtonId),
    HelpDialog,
    HelpDialogQuitButton,
}
