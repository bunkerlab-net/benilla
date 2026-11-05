// SPDX-License-Identifier: GPL-3.0-or-later
//! Offline non-admitting catalog diagnostics.
use benilla_world_runtime::native::diagnose_catalog_dbc;
use benilla_world_runtime::native::diagnose_catalog_events;
use benilla_world_runtime::native::diagnose_catalog_groups;
use benilla_world_runtime::native::diagnose_catalog_scripts;
use benilla_world_runtime::native::{
    diagnose_catalog, diagnose_catalog_pools, diagnose_catalog_schema, diagnose_catalog_spawns,
};
use std::io::Write;
use std::path::Path;
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let valid_count = if args
        .first()
        .is_some_and(|arg| arg == "dbc" || arg == "pools")
    {
        (3..=5).contains(&args.len())
    } else if args.first().is_some_and(|arg| arg == "scripts") {
        (4..=5).contains(&args.len())
    } else if args.first().is_some_and(|arg| arg == "events") {
        (6..=7).contains(&args.len())
    } else if args
        .first()
        .is_some_and(|arg| arg == "spawns" || arg == "groups")
    {
        (3..=4).contains(&args.len())
    } else {
        (2..=3).contains(&args.len())
    };
    if !valid_count {
        eprintln!(
            "usage: benilla-catalog schema PILOT | verify ROOT | candidate ROOT [NEW_RECEIPT] | spawns ROOT OCCURRENCES [NEW_RECEIPT] | groups ROOT OCCURRENCES [NEW_RECEIPT] | pools ROOT OCCURRENCES [NEW_RECEIPT [NEW_RECORDS_DIRECTORY]] | events ROOT OCCURRENCES SNAPSHOT ENVIRONMENT RECORDS [NEW_RECEIPT] | dbc ROOT OCCURRENCES [NEW_RECEIPT [NEW_RECORDS_DIRECTORY]] | scripts ROOT OCCURRENCES REGISTRATION_INPUT [NEW_RECEIPT]"
        );
        std::process::exit(2);
    }
    let result = match args[0].to_str() {
        Some("schema") => diagnose_catalog_schema(Path::new(&args[1])),
        Some("verify") => diagnose_catalog(Path::new(&args[1]), false),
        Some("candidate") => diagnose_catalog(Path::new(&args[1]), true),
        Some("dbc") => diagnose_catalog_dbc(
            Path::new(&args[1]),
            Path::new(&args[2]),
            args.get(4).map(Path::new),
        ),
        Some("spawns") if args.len() >= 3 => {
            diagnose_catalog_spawns(Path::new(&args[1]), Path::new(&args[2]))
        }
        Some("groups") if args.len() >= 3 => {
            diagnose_catalog_groups(Path::new(&args[1]), Path::new(&args[2]))
        }
        Some("pools") if args.len() >= 3 => diagnose_catalog_pools(
            Path::new(&args[1]),
            Path::new(&args[2]),
            args.get(4).map(Path::new),
        ),
        Some("scripts") if args.len() >= 4 => diagnose_catalog_scripts(
            Path::new(&args[1]),
            Path::new(&args[2]),
            Path::new(&args[3]),
        ),
        Some("events") if args.len() >= 6 => diagnose_catalog_events(
            Path::new(&args[1]),
            Path::new(&args[2]),
            Path::new(&args[3]),
            Path::new(&args[4]),
            Path::new(&args[5]),
        ),
        _ => {
            eprintln!("catalog.command");
            std::process::exit(2);
        }
    };
    match result {
        Ok(report) => {
            if let Some(path) = args.get(if args[0] == "events" {
                6
            } else if args[0] == "scripts" {
                4
            } else if args[0] == "spawns"
                || args[0] == "groups"
                || args[0] == "pools"
                || args[0] == "dbc"
            {
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
