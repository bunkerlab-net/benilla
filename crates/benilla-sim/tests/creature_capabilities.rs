// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::creature_capabilities::{
    evaluate, Error, InhabitType, MOVEFLAG_CAN_FLY, MOVEFLAG_FLYING, MOVEFLAG_HOVER,
    MOVEFLAG_LEVITATING, MOVEFLAG_SWIMMING, MOVEFLAG_WALK_MODE,
};
use std::collections::BTreeSet;

const FIXTURE: &str = include_str!("fixtures/creature_capabilities_cmangos.test-fixture");

#[test]
fn capabilities_match_unchanged_extracted_cpp_predicates() {
    let mut cases = BTreeSet::new();
    for line in FIXTURE.lines().filter(|line| line.starts_with("P ")) {
        let row: Vec<u32> = line
            .split_whitespace()
            .skip(1)
            .map(|word| word.parse().unwrap())
            .collect();
        assert_eq!(row.len(), 7, "{line}");
        assert_eq!(row[0], row[2], "predicate must not repair habitat: {line}");
        assert_eq!(row[6], 0, "predicate must not run loader repair: {line}");
        assert!(row[3..6].iter().all(|value| *value <= 1), "{line}");
        assert!(cases.insert((row[0], row[1])), "duplicate case: {line}");
        let inhabit_type = InhabitType::try_from_raw(row[0]).unwrap();
        assert_eq!(inhabit_type.raw(), row[0]);
        let result = evaluate(inhabit_type, row[1]);
        assert_eq!(result.can_walk(), row[3] != 0, "{line}");
        assert_eq!(result.can_swim(), row[4] != 0, "{line}");
        assert_eq!(result.can_fly(), row[5] != 0, "{line}");
    }
    assert_eq!(cases.len(), 889);
    for habitat in 1..=7 {
        assert!(cases.contains(&(habitat, 0)));
        assert!(cases.contains(&(habitat, u32::MAX)));
        for bit in 0..32 {
            assert!(cases.contains(&(habitat, 1 << bit)));
            assert!(cases.contains(&(habitat, u32::MAX ^ (1 << bit))));
        }
        let bits = [
            MOVEFLAG_WALK_MODE,
            MOVEFLAG_LEVITATING,
            MOVEFLAG_FLYING,
            MOVEFLAG_SWIMMING,
            MOVEFLAG_CAN_FLY,
            MOVEFLAG_HOVER,
        ];
        for subset in 0..(1 << bits.len()) {
            let flags = bits.iter().enumerate().fold(0, |flags, (index, bit)| {
                flags | if subset & (1 << index) != 0 { *bit } else { 0 }
            });
            assert!(cases.contains(&(habitat, flags)));
        }
    }
}

#[test]
fn raw_admission_rejects_cases_that_source_loader_repairs() {
    let mut cases = BTreeSet::new();
    for line in FIXTURE.lines().filter(|line| line.starts_with("R ")) {
        let row: Vec<u32> = line
            .split_whitespace()
            .skip(1)
            .map(|word| word.parse().unwrap())
            .collect();
        assert_eq!(row.len(), 7, "{line}");
        assert_eq!(row[1], 0, "{line}");
        assert!(cases.insert(row[0]), "duplicate repair case: {line}");
        match InhabitType::try_from_raw(row[0]) {
            Ok(habitat) => {
                assert_eq!(habitat.raw(), row[2], "{line}");
                assert_eq!(row[6], 0, "{line}");
            }
            Err(error) => {
                assert_eq!(error, Error::InvalidInhabitType(row[0]), "{line}");
                assert_eq!(row[2], 7, "source repair is separate: {line}");
                assert_eq!(row[6], 1, "{line}");
            }
        }
        let loaded = evaluate(InhabitType::try_from_raw(row[2]).unwrap(), row[1]);
        assert_eq!(loaded.can_walk(), row[3] != 0, "{line}");
        assert_eq!(loaded.can_swim(), row[4] != 0, "{line}");
        assert_eq!(loaded.can_fly(), row[5] != 0, "{line}");
    }
    assert_eq!(cases.len(), 262);
    for raw in 0..=256 {
        assert!(cases.contains(&raw));
        assert_eq!(
            InhabitType::try_from_raw(raw).is_ok(),
            (1..=7).contains(&raw)
        );
    }
    for raw in [65535, 65536, 0x7fff_ffff, 0x8000_0000, u32::MAX] {
        assert!(cases.contains(&raw));
        assert_eq!(
            InhabitType::try_from_raw(raw),
            Err(Error::InvalidInhabitType(raw))
        );
    }
}

#[test]
fn current_modes_do_not_replace_template_capabilities() {
    let ground = InhabitType::try_from_raw(1).unwrap();
    let current_modes = MOVEFLAG_WALK_MODE | MOVEFLAG_SWIMMING | MOVEFLAG_FLYING;
    let result = evaluate(ground, current_modes);
    assert!(result.can_walk());
    assert!(!result.can_swim());
    assert!(!result.can_fly());
    for flag in [MOVEFLAG_LEVITATING, MOVEFLAG_HOVER, MOVEFLAG_CAN_FLY] {
        let result = evaluate(ground, current_modes | flag);
        assert!(result.can_walk());
        assert!(!result.can_swim());
        assert!(result.can_fly());
    }
    let air = evaluate(InhabitType::try_from_raw(4).unwrap(), 0);
    assert!(!air.can_walk());
    assert!(!air.can_swim());
    assert!(air.can_fly());
    let water = evaluate(InhabitType::try_from_raw(2).unwrap(), MOVEFLAG_WALK_MODE);
    assert!(!water.can_walk());
    assert!(water.can_swim());
    assert!(!water.can_fly());
}

#[test]
fn derived_evidence_serializes_without_fabricated_defaults() {
    let habitat = InhabitType::try_from_raw(1).unwrap();
    let result = evaluate(habitat, MOVEFLAG_HOVER);
    assert_eq!(serde_json::to_value(habitat).unwrap(), serde_json::json!(1));
    assert_eq!(
        serde_json::to_value(result).unwrap(),
        serde_json::json!({"can_walk": true, "can_swim": false, "can_fly": true})
    );
}
