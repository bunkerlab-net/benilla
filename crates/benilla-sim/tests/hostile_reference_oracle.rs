// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded single-reference projection of unchanged compiled source functions.
//! The C++ fixture owns actual online/offline lists and target reverse links.
use benilla_sim::creature_ai::hostile_reference::{HostileReferences, Membership, ThreatEvent};
use benilla_sim::creature_ai::Real;
use std::collections::BTreeSet;

const FIXTURE: &str = include_str!("fixtures/hostile_reference_cmangos.test-fixture");

#[test]
fn owned_reference_lifecycle_matches_exact_source_containers() {
    let mut manager = HostileReferences::new();
    let mut known = BTreeSet::new();
    let mut resolved = BTreeSet::new();
    let mut creations = 0_u64;
    let mut observations = 0;
    let mut predicates = 0;
    let mut saw_inaccessible_online = false;
    let mut saw_invalid_offline_with_threat = false;
    let mut saw_clear_preserve_dirty = false;
    for line in FIXTURE.lines().filter(|line| !line.starts_with('#')) {
        let (command, output) = line.split_once(" | ").unwrap();
        let (output, source_events) = output.split_once(" | ").unwrap();
        let args: Vec<&str> = command.split_whitespace().collect();
        let observed: Vec<&str> = output.split_whitespace().collect();
        assert!(observed.len() >= 6, "{line}");
        let scalar: Vec<u64> = observed[..6]
            .iter()
            .map(|value| value.parse().unwrap())
            .collect();
        let offline = scalar[0] != 0;
        let guid = || args[1].parse::<u64>().unwrap();
        let mut events = Vec::new();
        match args[0] {
            "N" => {
                manager = HostileReferences::new();
                known.clear();
                resolved.clear();
                creations = 0;
            }
            "A" => {
                let target = guid();
                if known.insert(target) {
                    resolved.insert(target);
                }
                creations += u64::from(manager.reference().is_none());
                let amount = Real::from_bits(args[2].parse().unwrap()).unwrap();
                events = manager.add_threat(target, amount, offline).unwrap();
            }
            "F" | "P" => {
                if known.insert(guid()) {
                    resolved.insert(guid());
                }
                // The oracle receives raw state/static-flag words and executes
                // source predicates. The Rust manager receives only their
                // observed result at A/U/T, never a test's recreated predicate.
                predicates += usize::from(args[0] == "P");
            }
            "G" | "X" => {}
            "U" | "T" => {
                let target = guid();
                events = manager
                    .update_online_status(
                        target,
                        resolved.contains(&target).then_some(target),
                        offline,
                    )
                    .unwrap();
            }
            "V" => {
                events = manager.invalidate_target(guid()).unwrap();
                resolved.remove(&guid());
            }
            "L" => {
                resolved.insert(guid());
            }
            "R" => {
                events = manager.remove(guid()).unwrap();
            }
            "D" if guid() != 1000 => {
                events = manager.remove(guid()).unwrap();
            }
            "C" | "D" => manager.clear(),
            "S" => {
                assert!(manager.reference().is_some_and(|r| r.online()));
                manager.select_current_victim();
            }
            other => panic!("unknown exact-source command {other}"),
        }
        let expected_events: Vec<ThreatEvent> = source_events
            .split(',')
            .filter_map(|event| match event {
                "-" => None,
                "T" => Some(ThreatEvent::ThreatChanged),
                "O0" => Some(ThreatEvent::OnlineStatus(false)),
                "O1" => Some(ThreatEvent::OnlineStatus(true)),
                "R" => Some(ThreatEvent::Removed),
                "X0" | "X1" => {
                    assert_eq!(args[0], "X", "accessibility observation only: {line}");
                    None
                }
                other => panic!("unknown actual source event {other}"),
            })
            .collect();
        assert_eq!(
            events, expected_events,
            "ordered actual source dispatch: {line}"
        );
        assert_eq!(manager.current_victim().unwrap_or(0), scalar[1], "{line}");
        assert_eq!(u64::from(manager.dirty()), scalar[2], "{line}");
        assert_eq!(creations, scalar[5], "source aggro callbacks: {line}");
        let expected_count = usize::try_from(scalar[3] + scalar[4]).unwrap();
        assert!(expected_count <= 1, "single-target boundary: {line}");
        assert_eq!(observed.len() - 6, expected_count, "{line}");
        assert_eq!(
            usize::from(manager.reference().is_some()),
            expected_count,
            "{line}"
        );
        if let Some(reference) = manager.reference() {
            let expected: Vec<u64> = observed[6]
                .split(':')
                .map(|value| value.parse().unwrap())
                .collect();
            assert_eq!(expected.len(), 6, "{line}");
            assert_eq!(reference.target_guid(), expected[0], "{line}");
            assert_eq!(u64::from(reference.threat().bits()), expected[1], "{line}");
            assert_eq!(u64::from(reference.valid()), expected[2], "{line}");
            assert_eq!(u64::from(reference.online()), expected[3], "{line}");
            assert_eq!(
                reference.membership(),
                if scalar[3] == 1 {
                    Membership::Online
                } else {
                    Membership::Offline
                },
                "{line}"
            );
            // Source reverse-reference ownership, not a fabricated Rust provider.
            assert_eq!(
                expected[5], expected[2],
                "actual target reverse link: {line}"
            );
            saw_inaccessible_online |= expected[4] == 0 && reference.online();
            saw_invalid_offline_with_threat |=
                !reference.valid() && !reference.online() && reference.threat() != Real::ZERO;
        }
        saw_clear_preserve_dirty |= args[0] == "C" && manager.dirty();
        observations += 1;
    }
    assert_eq!(observations, 611);
    assert_eq!(predicates, 528);
    assert!(saw_inaccessible_online);
    assert!(saw_invalid_offline_with_threat);
    assert!(saw_clear_preserve_dirty);
}

#[test]
fn unsupported_identity_changes_do_not_mutate_owned_references() {
    let mut manager = HostileReferences::new();
    assert!(manager.add_threat(0, Real::ZERO, false).is_err());
    assert_eq!(manager, HostileReferences::new());
    manager.add_threat(17, Real::ONE, false).unwrap();
    manager.select_current_victim();
    let before = manager.clone();
    assert!(manager.add_threat(18, Real::ONE, false).is_err());
    assert!(manager.invalidate_target(18).is_err());
    assert!(manager.remove(18).is_err());
    assert!(manager.update_online_status(17, Some(18), false).is_err());
    assert_eq!(manager, before);
    manager.invalidate_target(17).unwrap();
    let invalid = manager.clone();
    assert!(manager.update_online_status(17, Some(18), false).is_err());
    assert_eq!(manager, invalid);
    // Safe Rust cleanup is a runtime contract. The C++ corpus deliberately
    // removes unresolved references before source clear/unlink (see README).
    manager.clear();
    assert!(manager.reference().is_none());
    assert_eq!(manager.current_victim(), None);
}
