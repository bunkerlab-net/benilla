// SPDX-License-Identifier: GPL-3.0-or-later
//! Offline non-admitting catalog diagnostics.
use benilla_world_runtime::native::diagnose_catalog_groups;
use benilla_world_runtime::native::{
    diagnose_catalog, diagnose_catalog_schema, diagnose_catalog_spawns,
};
use std::io::Write;
use std::path::Path;
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let valid_count = if args
        .first()
        .is_some_and(|arg| arg == "spawns" || arg == "groups")
    {
        (3..=4).contains(&args.len())
    } else {
        (2..=3).contains(&args.len())
    };
    if !valid_count {
        eprintln!(
            "usage: benilla-catalog schema PILOT | verify ROOT | candidate ROOT [NEW_RECEIPT] | spawns ROOT OCCURRENCES [NEW_RECEIPT] | groups ROOT OCCURRENCES [NEW_RECEIPT]"
        );
        std::process::exit(2);
    }
    let result = match args[0].to_str() {
        Some("schema") => diagnose_catalog_schema(Path::new(&args[1])),
        Some("verify") => diagnose_catalog(Path::new(&args[1]), false),
        Some("candidate") => diagnose_catalog(Path::new(&args[1]), true),
        Some("spawns") if args.len() >= 3 => {
            diagnose_catalog_spawns(Path::new(&args[1]), Path::new(&args[2]))
        }
        Some("groups") if args.len() >= 3 => {
            diagnose_catalog_groups(Path::new(&args[1]), Path::new(&args[2]))
        }
        _ => {
            eprintln!("catalog.command");
            std::process::exit(2);
        }
    };
    match result {
        Ok(report) => {
            if let Some(path) = args.get(if args[0] == "spawns" || args[0] == "groups" {
                3
            } else {
                2
            }) {
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
