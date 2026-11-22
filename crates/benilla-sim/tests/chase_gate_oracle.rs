// SPDX-License-Identifier: GPL-3.0-or-later
//! Unchanged-source differential tests for numeric values and callback observations.
//! The ordered replay below is a test host, not a loaded geometry provider.
use benilla_sim::chase_gate::{self, NumericGate};

const FIXTURE: &str = include_str!("fixtures/chase_gate_cmangos.test-fixture");

struct Case<'a> {
    input: [u32; 21],
    output: [u32; 11],
    trace: &'a str,
}

fn cases() -> impl Iterator<Item = Case<'static>> {
    FIXTURE.lines().map(|line| {
        let columns: Vec<&str> = line.split(" | ").collect();
        assert_eq!(columns.len(), 3, "{line}");
        let numbers = |text: &str| {
            text.split_whitespace()
                .map(|value| value.parse::<u32>().unwrap())
                .collect::<Vec<_>>()
        };
        Case {
            input: numbers(columns[0]).try_into().unwrap(),
            output: numbers(columns[1]).try_into().unwrap(),
            trace: columns[2],
        }
    })
}

fn numeric(case: &Case<'_>) -> NumericGate {
    chase_gate::numeric_gate_bits(
        case.input[0..3].try_into().unwrap(),
        case.input[3..6].try_into().unwrap(),
        case.input[6],
    )
    .unwrap()
}

#[test]
fn chase_gate_numeric_and_goal_eye_bits_match_unchanged_source() {
    let mut counts = [0; 3];
    for case in cases() {
        let expected = if case.output[0] == 0 {
            counts[0] += 1;
            NumericGate::Distance
        } else if case.output[1] == 0 {
            counts[1] += 1;
            NumericGate::Vertical
        } else {
            counts[2] += 1;
            NumericGate::Pass
        };
        assert_eq!(numeric(&case), expected, "{:?}", case.input);
        assert_eq!(case.output[3], u32::from(expected == NumericGate::Pass));
        if expected == NumericGate::Pass {
            let endpoints = chase_gate::los_endpoints_bits(
                case.input[0..3].try_into().unwrap(),
                case.input[3..6].try_into().unwrap(),
                case.input[7],
                case.input[8],
            )
            .unwrap();
            assert_eq!(endpoints[0], case.output[4..7]);
            assert_eq!(endpoints[1], case.output[7..10]);
            assert_eq!(case.output[10], 0, "source default ignoreM2=false");
        } else {
            assert_eq!(case.output[4..11], [0; 7], "no LOS callback");
            assert_eq!(case.trace, "-");
        }
    }
    assert_eq!(counts.iter().sum::<usize>(), 2548);
    assert!(counts.iter().all(|count| *count > 50), "{counts:?}");
}

#[test]
fn chase_gate_source_callbacks_short_circuit_los_owner_water_target_water() {
    let mut source_traces = std::collections::BTreeSet::new();
    for case in cases() {
        source_traces.insert(case.trace);
        let mut trace = String::new();
        // Authored test-only replay. Actual loaded-provider provenance, stale
        // binding and error propagation are deliberately outside this crate.
        let result = (|| {
            if numeric(&case) != NumericGate::Pass {
                return false;
            }
            trace.push('L');
            if case.input[14] == 0 {
                return false;
            }
            trace.push('o'); // TerrainInfo::CanCheckLiquidLevel for owner
            if case.input[19] != 0 {
                trace.push('O'); // TerrainInfo::getLiquidStatus for owner
                if chase_gate::terrain_is_in_water(case.input[15] as u8) {
                    return false;
                }
            }
            if case.input[16] != 0 {
                trace.push('P'); // Original Player cached environment-bit read
                case.input[18] & 1 == 0
            } else {
                trace.push('t');
                if case.input[20] != 0 {
                    trace.push('T');
                    !chase_gate::terrain_is_in_water(case.input[17] as u8)
                } else {
                    // Source returns false water when CanCheckLiquidLevel is
                    // false. A native host must refuse missing loaded coverage;
                    // this control must never establish native dry admission.
                    true
                }
            }
        })();
        assert_eq!(u32::from(result), case.output[2], "{:?}", case.input);
        assert_eq!(
            if trace.is_empty() { "-" } else { &trace },
            case.trace,
            "{:?}",
            case.input
        );
    }
    for trace in ["-", "L", "LoO", "LoOP", "LoOtT", "Lot", "LotT"] {
        assert!(source_traces.contains(trace), "missing source case {trace}");
    }
}

#[test]
fn chase_gate_source_npc_raw_status_and_player_cached_flag_are_distinct() {
    let mut npc_above = false;
    let mut npc_walk = false;
    let mut cached_dry_over_wet_terrain = false;
    let mut cached_wet_over_dry_terrain = false;
    let mut cached_flags = std::collections::BTreeSet::new();
    for case in cases().filter(|case| numeric(case) == NumericGate::Pass) {
        if case.trace == "LoO" {
            assert!(chase_gate::terrain_is_in_water(case.input[15] as u8));
            assert_eq!(case.output[2], 0);
            npc_above |= case.input[15] == 1;
            npc_walk |= case.input[15] == 2;
        }
        if case.trace == "LoOP" {
            let cached = case.input[18] & 1 != 0;
            assert_eq!(case.output[2], u32::from(!cached));
            cached_dry_over_wet_terrain |= !cached && case.input[17] != 0;
            cached_wet_over_dry_terrain |= cached && case.input[17] == 0;
            if case.input[17] == 8 {
                cached_flags.insert(case.input[18]);
            }
        }
    }
    assert!(npc_above && npc_walk);
    assert!(cached_dry_over_wet_terrain && cached_wet_over_dry_terrain);
    assert_eq!(cached_flags.len(), 256);
}

#[test]
fn chase_gate_source_hashes_are_pinned_in_manifest_order() {
    let hashes: Vec<&str> = include_str!("../tools/chase_gate_source_hashes.sha256")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.split_whitespace().next().unwrap())
        .collect();
    assert_eq!(hashes, chase_gate::SOURCE_SHA256);
}
