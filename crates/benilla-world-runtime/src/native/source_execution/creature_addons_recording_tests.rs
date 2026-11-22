// SPDX-License-Identifier: GPL-3.0-or-later
//! World.cpp:1087, SQLStorageImpl.h:309-329. Recorded SQL is evidence, never loader output.
use super::*;
use crate::native::production_compiler::VerifiedCatalog;
use crate::native::source_execution::terrain_startup_census::sql_storage_field;
use std::io::{BufRead, BufReader};
fn compare(recording: &Path, catalog: &mut VerifiedCatalog) {
    let queries = [
        CreatureAddonTable::Template.queries(),
        CreatureAddonTable::Guid.queries(),
    ]
    .concat();
    let mut reader = BufReader::with_capacity(1 << 20, std::fs::File::open(recording).unwrap());
    let mut line = String::new();
    let mut seen = Vec::new();
    let mut pointer = String::new();
    let mut selected = None;
    let mut fields: [Vec<Vec<Vec<u8>>>; 6] = std::array::from_fn(|_| Vec::new());
    let mut line_number = 0;
    let mut query_lines = Vec::new();
    while {
        line.clear();
        reader.read_line(&mut line).unwrap() != 0
    } {
        line_number += 1;
        let mut parts = line.trim_end().splitn(4, '\t');
        if parts.next() != Some("1") {
            continue;
        }
        let _sequence = parts.next().unwrap();
        let kind = parts.next().unwrap();
        let data = parts.next().unwrap_or_default();
        if kind == "sql_query" {
            let p: Vec<_> = data.split_whitespace().collect();
            let query = sql_storage_field(&p);
            if seen.is_empty() && query != queries[0].as_bytes() {
                continue;
            }
            if seen.len() == 6 {
                break;
            }
            assert_eq!(
                query,
                queries[seen.len()].as_bytes(),
                "SQL statement order at {}:{line_number}",
                recording.display()
            );
            selected = Some(seen.len());
            pointer = p[0].to_owned();
            seen.push(query);
            query_lines.push(line_number);
        } else if let Some(slot) = selected {
            if kind == "sql_row" && data == pointer {
                fields[slot].push(Vec::new());
            }
            if kind == "sql_field" {
                let p: Vec<_> = data.split_whitespace().collect();
                if p[0] != pointer {
                    continue;
                }
                let column = p[1].parse::<usize>().unwrap();
                let row = fields[slot]
                    .last_mut()
                    .expect("recorded SQL row precedes its fields");
                assert_eq!(column, row.len());
                row.push(sql_storage_field(&p));
            }
        }
    }
    assert_eq!(
        seen,
        queries
            .iter()
            .map(|q| q.as_bytes().to_vec())
            .collect::<Vec<_>>()
    );
    for table in [CreatureAddonTable::Template, CreatureAddonTable::Guid] {
        let slot = table as usize * 3;
        let rows = catalog.execution_creature_addon_rows(table).unwrap();
        let maximum = rows.iter().map(|r| r.words[0]).max().unwrap_or(0);
        assert_eq!(fields[slot], [vec![maximum.to_string().into_bytes()]]);
        assert_eq!(
            fields[slot + 1],
            [vec![rows.len().to_string().into_bytes()]]
        );
        catalog
            .execution_check_creature_addon_trace(table, &fields[slot + 2])
            .unwrap();
    }
    println!("CREATURE_ADDON_SQL recording={} statements=6 exact_order_and_fields=true query_lines={query_lines:?}", recording.display());
}
#[test]
fn addon_loader_sql_and_primary_key_rows_match_seal8_and_seal16_recordings() {
    let (Some(root), Some(trace)) = (
        std::env::var_os("BENILLA_F2_CATALOG"),
        std::env::var_os("BENILLA_F3_STARTUP_TRACE"),
    ) else {
        println!("SKIP creature addon SQL recording: bind BENILLA_F2_CATALOG and BENILLA_F3_STARTUP_TRACE");
        return;
    };
    let mut catalog = VerifiedCatalog::open(Path::new(&root), false).unwrap();
    let trace = Path::new(&trace);
    compare(trace, &mut catalog);
    let seal16 = trace
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("seal16-full-01/run/trace.tsv");
    assert!(
        seal16.is_file(),
        "bound reference environment requires frozen seal16 recording"
    );
    if seal16 != trace {
        compare(&seal16, &mut catalog);
    }
}
