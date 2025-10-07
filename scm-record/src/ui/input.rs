use crate::RecordError;

use super::{event, terminal};
use std::cell::RefCell;
use std::fmt::Display;
use std::rc::Rc;

///
/// A copy of the contents of the screen at a certain point in time.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TestingScreenshot {
    contents: Rc<RefCell<Option<String>>>,
}

impl TestingScreenshot {
    pub fn set(&self, new_contents: String) {
        let Self { contents } = self;
        *contents.borrow_mut() = Some(new_contents);
    }

    /// Produce an `Event` which will record the screenshot when it's handled.
    pub fn event(&self) -> event::Event {
        event::Event::TakeScreenshot(self.clone())
    }
}

impl Display for TestingScreenshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self { contents } = self;
        match contents.borrow().as_ref() {
            Some(contents) => write!(f, "{contents}"),
            None => write!(f, "<this screenshot was never assigned>"),
        }
    }
}

/// Get user input.
pub trait RecordInput {
    /// Return the kind of terminal to use.
    fn terminal_kind(&self) -> terminal::TerminalKind;

    /// Get all available user events. This should block until there is at least
    /// one available event.
    fn next_events(&mut self) -> Result<Vec<event::Event>, RecordError>;

    /// Open a commit editor and interactively edit the given message.
    ///
    /// This function will only be invoked if one of the provided `Commit`s had
    /// a non-`None` commit message.
    fn edit_commit_message(&mut self, message: &str) -> Result<String, RecordError>;
}
