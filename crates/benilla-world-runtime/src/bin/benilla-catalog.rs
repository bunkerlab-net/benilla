// SPDX-License-Identifier: GPL-3.0-or-later
//! Offline non-admitting catalog diagnostics.
use benilla_world_runtime::native::diagnose_catalog_schema;
use std::path::Path;
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 || args[0] != "schema" {
        eprintln!("usage: benilla-catalog schema PILOT");
        std::process::exit(2);
    }
    match diagnose_catalog_schema(Path::new(&args[1])) {
        Ok(report) => println!("{report}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}
