// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only original-prefix RNG projection. This does not execute a world.
use benilla_world_runtime::native::source_execution::compare_original_unsealed_rng;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = PathBuf::from(
        args.next()
            .ok_or("usage: source_execution <original-prefix.tsv>")?,
    );
    if args.next().is_some() {
        return Err("usage: source_execution <original-prefix.tsv>".into());
    }
    let report = compare_original_unsealed_rng(&path)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
