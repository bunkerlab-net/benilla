// SPDX-License-Identifier: GPL-3.0-or-later
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    about = "Extract the fixed starter DBC inputs from your own WoW Data patch chain",
    version
)]
struct Args {
    /// Explicit read-only WoW 1.12.1 Data directory; no inferred install or downloads.
    #[arg(long)]
    data: PathBuf,
    /// New private directory outside Data. Parent must exist; never overwritten.
    #[arg(long)]
    output: PathBuf,
}
fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let manifest = benilla_native_dbc::extract(&args.data, &args.output)?;
    println!("Extracted {} fixed DBC inputs; hashes recorded; build authenticity and native gameplay not established", manifest.files.len());
    Ok(())
}
