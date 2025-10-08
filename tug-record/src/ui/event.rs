use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use super::input::TestingScreenshot;

#[allow(missing_docs)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Event {
    None,
    QuitAccept,
    QuitCancel,
    QuitInterrupt,
    QuitEscape,
    TakeScreenshot(TestingScreenshot),
    Redraw,
    EnsureSelectionInViewport,
    ScrollUp,
    ScrollDown,
    PageUp,
    PageDown,
    FocusPrev,
    /// Move focus to the previous item of the same kind (i.e. file, section, line).
    FocusPrevSameKind,
    FocusPrevPage,
    FocusNext,
    /// Move focus to the next item of the same kind.
    FocusNextSameKind,
    FocusNextPage,
    FocusInner,
    /// If `fold_section` is true, and the current section is expanded, the
    /// section should be collapsed without moving focus. Otherwise, move the
    /// focus outwards.
    FocusOuter {
        fold_section: bool,
    },
    ToggleItem,
    ToggleItemAndAdvance,
    ToggleAll,
    ToggleAllUniform,
    ExpandItem,
    ExpandAll,
    ToggleCommitViewMode, // no key binding currently
    EditCommitMessage,
    Help,
}

impl From<crossterm::event::Event> for Event {
    fn from(event: crossterm::event::Event) -> Self {
        use crossterm::event::Event;
        match event {
            Event::Key(KeyEvent {
                code: KeyCode::Char('q'),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: _,
            }) => Self::QuitCancel,

            Event::Key(KeyEvent {
                code: KeyCode::Esc,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: _,
            }) => Self::QuitEscape,

            Event::Key(KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                state: _,
            }) => Self::QuitInterrupt,

            Event::Key(KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: _,
            }) => Self::QuitAccept,

            Event::Key(KeyEvent {
                code: KeyCode::Char('?'),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: _,
            }) => Self::Help,

            Event::Key(KeyEvent {
                code: KeyCode::Up | KeyCode::Char('y'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                state: _,
            }) => Self::ScrollUp,
            Event::Key(KeyEvent {
                code: KeyCode::Down | KeyCode::Char('e'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                state: _,
            }) => Self::ScrollDown,

            Event::Key(KeyEvent {
                code: KeyCode::PageUp | KeyCode::Char('b'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                state: _,
            }) => Self::PageUp,
            Event::Key(KeyEvent {
                code: KeyCode::PageDown | KeyCode::Char('f'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                state: _,
            }) => Self::PageDown,

            Event::Key(KeyEvent {
                code: KeyCode::Up | KeyCode::Char('k'),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: _,
            }) => Self::FocusPrev,
            Event::Key(KeyEvent {
                code: KeyCode::Down | KeyCode::Char('j'),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: _,
            }) => Self::FocusNext,

            Event::Key(KeyEvent {
                code: KeyCode::PageUp,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: _,
            }) => Self::FocusPrevSameKind,
            Event::Key(KeyEvent {
                code: KeyCode::PageDown,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: _,
            }) => Self::FocusNextSameKind,

            Event::Key(KeyEvent {
                code: KeyCode::Left | KeyCode::Char('h'),
                modifiers: KeyModifiers::SHIFT,
                kind: KeyEventKind::Press,
                state: _,
            }) => Self::FocusOuter {
                fold_section: false,
            },
            Event::Key(KeyEvent {
                code: KeyCode::Left | KeyCode::Char('h'),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: _,
            }) => Self::FocusOuter { fold_section: true },
            Event::Key(KeyEvent {
                code: KeyCode::Right | KeyCode::Char('l'),
                // The shift modifier is accepted for continuity with FocusOuter.
                modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
                kind: KeyEventKind::Press,
                state: _,
            }) => Self::FocusInner,

            Event::Key(KeyEvent {
                code: KeyCode::Char('u'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                state: _,
            }) => Self::FocusPrevPage,
            Event::Key(KeyEvent {
                code: KeyCode::Char('d'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                state: _,
            }) => Self::FocusNextPage,

            Event::Key(KeyEvent {
                code: KeyCode::Char(' '),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: _,
            }) => Self::ToggleItem,

            Event::Key(KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: _,
            }) => Self::ToggleItemAndAdvance,

            Event::Key(KeyEvent {
                code: KeyCode::Char('a'),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: _,
            }) => Self::ToggleAll,
            Event::Key(KeyEvent {
                code: KeyCode::Char('A'),
                modifiers: KeyModifiers::SHIFT,
                kind: KeyEventKind::Press,
                state: _,
            }) => Self::ToggleAllUniform,

            Event::Key(KeyEvent {
                code: KeyCode::Char('f'),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: _,
            }) => Self::ExpandItem,
            Event::Key(KeyEvent {
                code: KeyCode::Char('F'),
                modifiers: KeyModifiers::SHIFT,
                kind: KeyEventKind::Press,
                state: _,
            }) => Self::ExpandAll,

            Event::Key(KeyEvent {
                code: KeyCode::Char('e'),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: _event,
            }) => Self::EditCommitMessage,

            _event => Self::None,
        }
    }
}

