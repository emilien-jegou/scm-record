use clap::Parser;
use tug_diff_editor::{run, Opts, Result};

pub fn main() -> Result<()> {
    let opts = Opts::parse();
    run(opts)?;
    Ok(())
}
