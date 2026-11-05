// SPDX-License-Identifier: GPL-3.0-or-later
//! Offline non-admitting catalog diagnostics.
use benilla_world_runtime::native::{diagnose_catalog, diagnose_catalog_schema};
use std::io::Write;
use std::path::Path;
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if !(2..=3).contains(&args.len()) {
        eprintln!(
            "usage: benilla-catalog schema PILOT | verify ROOT | candidate ROOT [NEW_RECEIPT]"
        );
        std::process::exit(2);
    }
    let result = match args[0].to_str() {
        Some("schema") => diagnose_catalog_schema(Path::new(&args[1])),
        Some("verify") => diagnose_catalog(Path::new(&args[1]), false),
        Some("candidate") => diagnose_catalog(Path::new(&args[1]), true),
        _ => {
            eprintln!("catalog.command");
            std::process::exit(2);
        }
    };
    match result {
        Ok(report) => {
            if let Some(path) = args.get(2) {
                let result = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)
                    .and_then(|mut file| {
                        file.write_all(report.to_string().as_bytes())?;
                        file.sync_all()
                    });
                if result.is_err() {
                    eprintln!("catalog.receipt");
                    std::process::exit(2);
                }
            }
            println!("{report}");
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}
