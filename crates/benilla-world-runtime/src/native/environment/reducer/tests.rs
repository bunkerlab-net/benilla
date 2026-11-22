// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::native::environment::tests::{binding, machine, pose, reply, STORE1};

fn step(
    machine: &mut EnvironmentMachine,
    state: &mut EnvironmentState,
    sequence: &mut u64,
    phase: EnvironmentPhase,
) -> Result<(), EnvironmentError> {
    let before_state = state.clone();
    let before_checkpoint = machine.checkpoint();
    let before_records = machine.records_to_bytes().unwrap();
    let reduced = state.reduce(machine.snapshot, &machine.catalog, *sequence, &phase);
    assert_eq!(*state, before_state, "reduction never mutates its input");
    let wrapped = match phase {
        EnvironmentPhase::Update { diff_ms } => machine.update_phase(diff_ms),
        EnvironmentPhase::Position { pose, result } => {
            machine.set_position_phase(pose, result.map(|r| *r))
        }
    };
    match reduced {
        Ok((next, next_sequence)) => {
            wrapped.unwrap();
            assert_eq!(next_sequence, *sequence + 1);
            assert_eq!(next_sequence, machine.sequence);
            assert_eq!(next, machine.state);
            assert_eq!(
                next.snapshot()
                    .restore(machine.snapshot, &machine.catalog, next_sequence)
                    .unwrap(),
                next
            );
            *state = next;
            *sequence = next_sequence;
            Ok(())
        }
        Err(error) => {
            assert_eq!(wrapped, Err(error.clone()));
            assert_eq!(machine.checkpoint(), before_checkpoint);
            assert_eq!(machine.records_to_bytes().unwrap(), before_records);
            Err(error)
        }
    }
}

fn position(pose: EnvironmentPose, result: Option<BoundLiquidResult>) -> EnvironmentPhase {
    EnvironmentPhase::Position {
        pose,
        result: result.map(Box::new),
    }
}

#[test]
fn reducer_matches_wrapper_across_refresh_cache_restore_and_atomic_failures() {
    let mut machine = machine();
    let mut state = EnvironmentState::new(machine.snapshot, &machine.catalog).unwrap();
    let mut sequence = 0;
    assert_eq!(state, machine.state);
    assert_eq!(state.flags(), 0);
    assert_eq!(state.timer_ms(), 0);
    assert_eq!(state.last_liquid(), None);
    assert_eq!(state.last_position(), None);
    assert_eq!(state.cached_query(), None);

    let dry = reply(&machine, LiquidStatus::NoWater, 0, 0);
    step(
        &mut machine,
        &mut state,
        &mut sequence,
        position(pose(), Some(dry)),
    )
    .unwrap();
    let mut moved = pose();
    moved.position[0] = 5f32.to_bits();
    step(
        &mut machine,
        &mut state,
        &mut sequence,
        position(moved, None),
    )
    .unwrap();
    for diff_ms in [0, 99, 1, u32::MAX] {
        step(
            &mut machine,
            &mut state,
            &mut sequence,
            EnvironmentPhase::Update { diff_ms },
        )
        .unwrap();
    }
    assert_eq!(state.cached_query(), Some(dry.query));
    assert_eq!(state.last_position(), Some(moved));
    assert_eq!(
        state.query_for_position(machine.snapshot, sequence, pose()),
        machine.query_for_position(pose())
    );
    assert_eq!(
        step(
            &mut machine,
            &mut state,
            &mut sequence,
            position(pose(), None)
        ),
        Err(EnvironmentError::Binding)
    );
    for (status, entry, kind) in [
        (LiquidStatus::UnderWater, 11, 8),
        (LiquidStatus::UnderWater, 14, 2),
        (LiquidStatus::InWater, 14, 2),
    ] {
        let sample = reply(&machine, status, entry, kind);
        assert!(matches!(
            step(
                &mut machine,
                &mut state,
                &mut sequence,
                position(pose(), Some(sample))
            ),
            Err(EnvironmentError::CallbacksUnavailable(_))
        ));
    }
    // Above-water high-sea scaling is already supported by the standalone component.
    for kind in [0x10, 0] {
        let sample = reply(&machine, LiquidStatus::AboveWater, 14, kind);
        step(
            &mut machine,
            &mut state,
            &mut sequence,
            position(pose(), Some(sample)),
        )
        .unwrap();
        assert_eq!(
            step(
                &mut machine,
                &mut state,
                &mut sequence,
                position(pose(), Some(sample))
            ),
            Err(EnvironmentError::Binding)
        );
        step(
            &mut machine,
            &mut state,
            &mut sequence,
            EnvironmentPhase::Update { diff_ms: 100 },
        )
        .unwrap();
    }
    assert_eq!(state.mirror(EnvironmentMirror::Fatigue).scale(), 10);
    let later_aura = reply(&machine, LiquidStatus::InWater, 0, 0x10);
    assert_eq!(
        step(
            &mut machine,
            &mut state,
            &mut sequence,
            position(pose(), Some(later_aura))
        ),
        Err(EnvironmentError::CallbacksUnavailable(
            EnvironmentCallback::RemoveInterruptAuras { mask: 0x80 }
        ))
    );
    assert_eq!(state.mirror(EnvironmentMirror::Fatigue).scale(), 10);
    let dry_exit = reply(&machine, LiquidStatus::NoWater, 0, 0);
    assert_eq!(
        step(
            &mut machine,
            &mut state,
            &mut sequence,
            position(pose(), Some(dry_exit))
        ),
        Err(EnvironmentError::CallbacksUnavailable(
            EnvironmentCallback::UpdateThreatTables
        ))
    );
}

#[test]
fn reducer_does_not_own_or_inherit_standalone_journal_capacity() {
    let mut machine = machine();
    machine.update_phase(0).unwrap();
    machine.records = vec![machine.records[0].clone(); MAX_ENVIRONMENT_RECORDS];
    let phase = EnvironmentPhase::Update { diff_ms: u32::MAX };
    let (state, sequence) = machine
        .state
        .reduce(machine.snapshot, &machine.catalog, machine.sequence, &phase)
        .unwrap();
    assert_eq!(state, machine.state);
    assert_eq!(sequence, machine.sequence + 1);
    assert_eq!(
        machine.update_phase(u32::MAX),
        Err(EnvironmentError::JournalFull)
    );
    assert_eq!(
        machine
            .state
            .reduce(machine.snapshot, &machine.catalog, u64::MAX, &phase),
        Err(EnvironmentError::Limit)
    );
    assert_eq!(
        machine
            .state
            .query_for_position(machine.snapshot, u64::MAX, pose()),
        Err(EnvironmentError::Limit)
    );
}

#[test]
fn initialization_and_snapshot_restore_require_selected_complete_catalog() {
    let machine = machine();
    let other = LiquidCatalog::from_wdbc(STORE1).unwrap();
    assert_eq!(
        EnvironmentState::new(machine.snapshot, &other),
        Err(EnvironmentError::Binding)
    );
    assert_eq!(
        machine
            .state
            .snapshot()
            .restore(machine.snapshot, &other, 0),
        Err(EnvironmentError::Binding)
    );
    let mut bad = binding(&machine.catalog);
    bad.source_epoch = [0; 32];
    assert_eq!(
        EnvironmentState::new(bad, &machine.catalog),
        Err(EnvironmentError::Binding)
    );
}

#[test]
fn shared_restore_rejects_unbound_queries_unsupported_state_and_started_mirrors() {
    let mut machine = machine();
    let sample = reply(&machine, LiquidStatus::AboveWater, 14, 2);
    machine.set_position_phase(pose(), Some(sample)).unwrap();
    let valid = machine.state.snapshot();
    for change in 0..14 {
        let mut stored = valid.clone();
        match change {
            0 => stored.flags |= IN_WATER,
            1 => stored.timer_ms = REFRESH_MS + 1,
            2 => stored.last_liquid = Some(11),
            3 => stored.last_liquid = Some(0),
            4 => stored.last_liquid = Some(u32::MAX),
            5 => stored.last_position = None,
            6 => stored.last_position.as_mut().unwrap().position[0] = f32::NAN.to_bits(),
            7 => stored.cached_query = None,
            8 => stored.cached_query.as_mut().unwrap().ticket = 0,
            9 => stored.cached_query.as_mut().unwrap().ticket = machine.sequence + 1,
            10 => stored.cached_query.as_mut().unwrap().snapshot.content = [9; 32],
            11 => stored.cached_query.as_mut().unwrap().required_type = 2,
            12 => stored.cached_query.as_mut().unwrap().collision_height = f32::INFINITY.to_bits(),
            _ => stored.timer_ms -= 1,
        }
        assert_eq!(
            stored.restore(machine.snapshot, &machine.catalog, machine.sequence),
            Err(EnvironmentError::InvalidCheckpoint),
            "change {change}"
        );
    }
    for timer in 0..4 {
        let mut state = machine.state.clone();
        state.mirrors[timer].start(1000, 7);
        assert_eq!(
            state
                .snapshot()
                .restore(machine.snapshot, &machine.catalog, machine.sequence),
            Err(EnvironmentError::InvalidCheckpoint)
        );
    }
}

#[test]
fn native_lifecycle_guards_do_not_expand_standalone_callback_support() {
    let mut machine = machine();
    assert_eq!(
        machine.state.validate_native_lifecycle(&machine.catalog),
        Ok(())
    );
    assert_eq!(
        EnvironmentPhase::Update { diff_ms: 100 }.validate_native_lifecycle(&machine.catalog),
        Ok(())
    );
    assert_eq!(
        position(pose(), None).validate_native_lifecycle(&machine.catalog),
        Ok(())
    );
    for (status, entry, kind) in [
        (LiquidStatus::UnderWater, 14, 0),
        (LiquidStatus::InWater, 14, 0),
        (LiquidStatus::AboveWater, 14, 0x01),
        (LiquidStatus::AboveWater, 14, 0x04),
        (LiquidStatus::AboveWater, 14, 0x10),
        (LiquidStatus::AboveWater, 11, 2),
    ] {
        let sample = reply(&machine, status, entry, kind);
        assert_eq!(
            position(pose(), Some(sample)).validate_native_lifecycle(&machine.catalog),
            Err(EnvironmentError::InvalidInput)
        );
    }
    let above = reply(&machine, LiquidStatus::AboveWater, 14, 2);
    assert_eq!(
        position(pose(), Some(above)).validate_native_lifecycle(&machine.catalog),
        Ok(())
    );
    machine.set_position_phase(pose(), Some(above)).unwrap();
    assert_eq!(
        machine.state.validate_native_lifecycle(&machine.catalog),
        Ok(())
    );
    machine.update_phase(100).unwrap();
    let dry = reply(&machine, LiquidStatus::NoWater, 0, 0);
    assert_eq!(
        position(pose(), Some(dry)).validate_native_lifecycle(&machine.catalog),
        Ok(())
    );
    // An acceptable native phase still must pass the fail-closed shared reducer.
    assert_eq!(
        machine.set_position_phase(pose(), Some(dry)),
        Err(EnvironmentError::CallbacksUnavailable(
            EnvironmentCallback::UpdateThreatTables
        ))
    );
    for kind in [0x10, 0] {
        let sample = reply(&machine, LiquidStatus::AboveWater, 14, kind);
        machine.set_position_phase(pose(), Some(sample)).unwrap();
        assert_eq!(
            machine.state.validate_native_lifecycle(&machine.catalog),
            Err(EnvironmentError::InvalidInput)
        );
        machine.update_phase(100).unwrap();
    }
}

#[test]
fn standalone_v2_checkpoint_and_journal_bytes_keep_the_preextraction_encoding() {
    use sha2::{Digest as _, Sha256};
    // Fixed hashes from the original v2 field order, enum encoding and digest
    // domains, independently serialized for this authored phase sequence.
    let mut machine = machine();
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(machine.checkpoint().to_bytes().unwrap())
        ),
        "639f9834d70c961f2251c15ea83640957c3334aa7a74760f564f58470abf226e"
    );
    let dry = reply(&machine, LiquidStatus::NoWater, 0, 0);
    machine.set_position_phase(pose(), Some(dry)).unwrap();
    machine.update_phase(99).unwrap();
    let mut moved = pose();
    moved.position[0] = 5f32.to_bits();
    machine.set_position_phase(moved, None).unwrap();
    machine.update_phase(1).unwrap();
    let sea = reply(&machine, LiquidStatus::AboveWater, 14, 0x10);
    machine.set_position_phase(pose(), Some(sea)).unwrap();
    machine.update_phase(100).unwrap();
    let above = reply(&machine, LiquidStatus::AboveWater, 0, 0);
    machine.set_position_phase(pose(), Some(above)).unwrap();
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(machine.checkpoint().to_bytes().unwrap())
        ),
        "08694c82a020b5e05f21d9dc98d0da5337c4652f69e1305eb4285ba6cd8146fb"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(machine.records_to_bytes().unwrap())),
        "4d0166b45d9e6e84b7cb0998605e1c12063d7e9934457a4f619448665b0bec2c"
    );
}
