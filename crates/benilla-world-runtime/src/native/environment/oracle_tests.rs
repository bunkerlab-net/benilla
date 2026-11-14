// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-observed reducer outputs and callback intents. Harness dependency answers
//! are authored, not proof that the production callback implementations exist.
use super::tests::{binding, TestHost, STORE0, STORE1};
use super::*;
const FIXTURE: &str = include_str!("../../../tools/fixtures/environment-state.tsv");
fn number(s: &str) -> u32 {
    s.parse().unwrap()
}
fn bits(s: &str) -> u32 {
    u32::from_str_radix(s, 16).unwrap()
}
fn last(s: &str) -> Option<u32> {
    if s == "-" {
        None
    } else {
        Some(number(s))
    }
}
fn status(s: &str) -> LiquidStatus {
    match number(s) {
        0 => LiquidStatus::NoWater,
        1 => LiquidStatus::AboveWater,
        2 => LiquidStatus::WaterWalk,
        4 => LiquidStatus::InWater,
        8 => LiquidStatus::UnderWater,
        _ => panic!("unexpected source status"),
    }
}
#[test]
fn exact_pinned_source_reducer_timer_and_callback_order_corpus() {
    assert!(FIXTURE.contains("8ec338a1704e7dcb1c0213eb7ed58f9231ade40f"));
    let stores = [
        LiquidCatalog::from_wdbc(STORE0).unwrap(),
        LiquidCatalog::from_wdbc(STORE1).unwrap(),
    ];
    let mut count = 0;
    for line in FIXTURE
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let row: Vec<_> = line.split('\t').collect();
        assert_eq!(row.len(), 22);
        let label = format!("{} step {}", row[0], row[1]);
        let catalog = &stores[number(row[2]) as usize];
        // Arbitrary uint8 flags and uint32 timers deliberately exercise the raw
        // source reducer, including states not admitted by production checkpoints.
        let mut state = EnvironmentState {
            mirrors: initial_mirrors(),
            flags: number(row[14]) as u8,
            timer_ms: number(row[15]),
            last_liquid: last(row[16]),
            last_position: None,
            cached_query: None,
        };
        if let Some(id) = state.last_liquid {
            assert!(catalog.get(id).is_some(), "{label}");
        }
        let liquid_status = status(row[6]);
        let reply = BoundLiquidResult {
            query: EnvironmentQuery {
                snapshot: binding(catalog),
                ticket: 1,
                position: [0; 3],
                required_type: ALL_LIQUIDS,
                collision_height: bits(row[11]),
            },
            status: liquid_status,
            data: (liquid_status != LiquidStatus::NoWater).then_some(LiquidData {
                entry: number(row[7]),
                type_flags: number(row[8]),
                level: bits(row[9]),
                depth_level: bits(row[10]),
            }),
        };
        let mut host = TestHost::new(number(row[12]));
        // Exact-source harness scripts this dependency; no claim of actual aura state.
        host.scripted_has_aura = Some(number(row[13]) != 0);
        let queried = match row[3] {
            "update" => {
                state.update_timer(number(row[5]));
                false
            }
            "position" => state.refresh_if_due(reply, catalog, &mut host).unwrap(),
            _ => panic!("unknown fixture phase"),
        };
        assert_eq!(state.flags, number(row[17]) as u8, "{label}: flags");
        assert_eq!(state.timer_ms, number(row[18]), "{label}: timer");
        assert_eq!(state.last_liquid, last(row[19]), "{label}: last liquid");
        assert_eq!(u32::from(queried), number(row[20]), "{label}: query count");
        let expected = if row[21] == "-" {
            vec![]
        } else {
            row[21].split(',').map(str::to_owned).collect::<Vec<_>>()
        };
        assert_eq!(host.trace, expected, "{label}: ordered callbacks");
        count += 1;
    }
    assert_eq!(count, 1324);
}

#[test]
fn complete_catalog_matches_original_wdbc_loader_observations() {
    let stores = [
        LiquidCatalog::from_wdbc(STORE0).unwrap(),
        LiquidCatalog::from_wdbc(STORE1).unwrap(),
    ];
    let fixture = include_str!("../../../tools/fixtures/environment-state-dbc.tsv");
    let mut count = 0;
    for line in fixture
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let row: Vec<_> = line.split('\t').collect();
        assert_eq!(row.len(), 7);
        let actual = stores[number(row[0]) as usize].get(number(row[1]));
        let expected = (row[2] == "1").then(|| LiquidTypeRow {
            id: number(row[3]),
            liquid_id: number(row[4]),
            kind: number(row[5]),
            spell_id: number(row[6]),
        });
        assert_eq!(actual, expected, "{line}");
        count += 1;
    }
    assert_eq!(count, 20);
}
