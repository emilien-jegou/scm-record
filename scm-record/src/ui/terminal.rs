use std::{fmt::Write, io, panic};

use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, is_raw_mode_enabled, EnterAlternateScreen,
    LeaveAlternateScreen,
};
use ratatui::buffer::Buffer;
use unicode_width::UnicodeWidthStr;

use crate::RecordError;

/// The terminal backend to use.
pub enum TerminalKind {
    /// Use the `CrosstermBackend` backend.
    Crossterm,

    /// Use the `TestingBackend` backend.
    Testing {
        /// The width of the virtual terminal.
        width: usize,

        /// The height of the virtual terminal.
        height: usize,
    },
}

/// Copied from internal implementation of `tui`.
pub fn buffer_view(buffer: &Buffer) -> String {
    let mut view =
        String::with_capacity(buffer.content.len() + usize::from(buffer.area.height) * 3);
    for cells in buffer.content.chunks(buffer.area.width.into()) {
        let mut overwritten = vec![];
        let mut skip: usize = 0;
        view.push('"');
        for (x, c) in cells.iter().enumerate() {
            if skip == 0 {
                view.push_str(c.symbol());
            } else {
                overwritten.push((x, c.symbol()))
            }
            skip = std::cmp::max(skip, c.symbol().width()).saturating_sub(1);
        }
        view.push('"');
        if !overwritten.is_empty() {
            write!(&mut view, " Hidden by multi-width symbols: {overwritten:?}").unwrap();
        }
        view.push('\n');
    }
    view
}

pub fn install_panic_hook() {
    // HACK: installing a global hook here. This could be installed multiple
    // times, and there's no way to uninstall it once we return.
    //
    // The idea is
    // taken from
    // https://github.com/fdehau/tui-rs/blob/fafad6c96109610825aad89c4bba5253e01101ed/examples/panic.rs.
    //
    // For some reason, simply catching the panic, cleaning up, and
    // reraising the panic loses information about where the panic was
    // originally raised, which is frustrating.
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic| {
        clean_up_crossterm().unwrap();
        original_hook(panic);
    }));
}

pub fn set_up_crossterm() -> Result<(), RecordError> {
    if !is_raw_mode_enabled().map_err(RecordError::SetUpTerminal)? {
        crossterm::execute!(io::stdout(), EnterAlternateScreen)
            .map_err(RecordError::SetUpTerminal)?;
        enable_raw_mode().map_err(RecordError::SetUpTerminal)?;
    }
    Ok(())
}

pub fn clean_up_crossterm() -> Result<(), RecordError> {
    if is_raw_mode_enabled().map_err(RecordError::CleanUpTerminal)? {
        disable_raw_mode().map_err(RecordError::CleanUpTerminal)?;
        crossterm::execute!(io::stdout(), LeaveAlternateScreen)
            .map_err(RecordError::CleanUpTerminal)?;
    }
    Ok(())
}
