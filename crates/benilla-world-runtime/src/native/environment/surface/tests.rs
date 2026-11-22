// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::native::environment::tests::{machine, pose, TestHost, STORE1};

/// Test-only callback probe, not the production threat/aura implementation.
#[derive(Default)]
struct Effects {
    calls: Vec<(EnvironmentCallback, u8)>,
    fail_at: Option<usize>,
}
impl NativeEnvironmentEffects for Effects {
    fn effect(&mut self, intent: EnvironmentCallback, flags: u8) -> Result<(), EnvironmentError> {
        if self.fail_at == Some(self.calls.len()) {
            return Err(EnvironmentError::CallbackFailed(intent));
        }
        self.calls.push((intent, flags));
        Ok(())
    }
}
struct Harness {
    machine: EnvironmentMachine,
    effects: Effects,
}
impl Harness {
    fn new() -> Self {
        Self {
            machine: machine(),
            effects: Effects::default(),
        }
    }
    fn apply(&mut self, phase: EnvironmentPhase) -> Result<(), EnvironmentError> {
        let m = &mut self.machine;
        let (state, sequence) = m.state.reduce_native_surface(
            m.snapshot,
            &m.catalog,
            m.sequence,
            &phase,
            &mut self.effects,
        )?;
        assert_eq!(
            state
                .snapshot()
                .restore_native_surface(m.snapshot, &m.catalog, sequence)
                .unwrap(),
            state
        );
        m.state = state;
        m.sequence = sequence;
        Ok(())
    }
    fn phase(&self, status: LiquidStatus, entry: u32, type_flags: u32) -> EnvironmentPhase {
        EnvironmentPhase::Position {
            pose: pose(),
            result: Some(Box::new(crate::native::environment::tests::reply(
                &self.machine,
                status,
                entry,
                type_flags,
            ))),
        }
    }
    fn position(&mut self, status: LiquidStatus, entry: u32, type_flags: u32) {
        self.apply(self.phase(status, entry, type_flags)).unwrap();
    }
    fn expire(&mut self) {
        self.apply(EnvironmentPhase::Update { diff_ms: 100 })
            .unwrap();
    }
}

#[test]
fn surface_effects_see_source_order_and_already_changed_flags() {
    let mut h = Harness::new();
    h.position(LiquidStatus::InWater, 14, 8);
    assert_eq!(h.machine.state.flags(), LIQUID | IN_WATER | HIGH_LIQUID);
    assert_eq!(
        h.effects.calls,
        [
            (EnvironmentCallback::UpdateThreatTables, LIQUID | IN_WATER),
            (
                EnvironmentCallback::RemoveInterruptAuras { mask: 0x80 },
                LIQUID | IN_WATER | HIGH_LIQUID,
            ),
        ]
    );
    assert_eq!(h.machine.state.mirrors, initial_mirrors());
    h.expire();
    h.position(LiquidStatus::InWater, 14, 8);
    assert_eq!(h.effects.calls.len(), 2, "requested-mask no-op guard");
    h.expire();
    h.position(LiquidStatus::WaterWalk, 14, 8);
    assert_eq!(
        &h.effects.calls[2..],
        [
            (
                EnvironmentCallback::UpdateThreatTables,
                LIQUID | HIGH_LIQUID
            ),
            (
                EnvironmentCallback::RemoveInterruptAuras { mask: 0x100 },
                LIQUID,
            ),
        ]
    );
    assert_eq!(h.machine.state.flags(), LIQUID);
}

#[test]
fn dry_combined_clear_executes_real_owned_mirror_mutations_even_from_liquid_only() {
    for status in [LiquidStatus::AboveWater, LiquidStatus::InWater] {
        let mut h = Harness::new();
        h.position(status, 14, 8);
        h.expire();
        h.effects.calls.clear();
        let phase = h.phase(LiquidStatus::NoWater, 0, 0);
        let mut source_state = h.machine.state.clone();
        let mut source_host = TestHost::new(0);
        let EnvironmentPhase::Position { result, .. } = &phase else {
            unreachable!()
        };
        source_state
            .refresh_if_due(
                *result.as_deref().unwrap(),
                &h.machine.catalog,
                &mut source_host,
            )
            .unwrap();
        assert_eq!(
            source_host.trace,
            [
                "threat",
                "interrupt:256",
                "fatigue:10",
                "breath:10",
                "environmental:10"
            ]
        );
        assert_eq!(source_host.observed_flags, [0; 5]);
        h.apply(phase).unwrap();
        assert_eq!(
            h.effects.calls,
            [
                (EnvironmentCallback::UpdateThreatTables, 0),
                (EnvironmentCallback::RemoveInterruptAuras { mask: 0x100 }, 0),
            ]
        );
        assert_eq!(h.machine.state.mirrors, cleared_mirrors());
        assert_eq!(h.machine.state.flags(), 0);
        assert_eq!(h.machine.state.last_liquid(), None);
        h.expire();
        h.position(LiquidStatus::NoWater, 0, 0);
        assert_eq!(h.effects.calls.len(), 2, "empty combined mask is a no-op");
        h.expire();
        h.position(LiquidStatus::InWater, 14, 2);
        assert_eq!(h.machine.state.mirrors, cleared_mirrors());
    }
}

#[test]
fn callback_failure_stages_reducer_state_only_outer_frame_owns_effect_rollback() {
    for initial in [LiquidStatus::NoWater, LiquidStatus::AboveWater] {
        let mut h = Harness::new();
        h.position(initial, 14, 8);
        h.expire();
        h.effects.calls.clear();
        h.effects.fail_at = Some(1);
        let phase = h.phase(
            if initial == LiquidStatus::NoWater {
                LiquidStatus::InWater
            } else {
                LiquidStatus::NoWater
            },
            14,
            8,
        );
        let before_state = h.machine.state.clone();
        let before_sequence = h.machine.sequence;
        assert!(matches!(
            h.apply(phase),
            Err(EnvironmentError::CallbackFailed(
                EnvironmentCallback::RemoveInterruptAuras { .. }
            ))
        ));
        assert_eq!(h.machine.state, before_state);
        assert_eq!(h.machine.sequence, before_sequence);
        assert_eq!(
            h.effects.calls.len(),
            1,
            "host is deliberately not rolled back"
        );
    }
}

#[test]
fn cached_pose_and_countdown_never_repeat_callbacks_or_tick_mirrors() {
    let mut h = Harness::new();
    h.position(LiquidStatus::InWater, 14, 8);
    let cached = h.machine.state.cached_query();
    let mut moved = pose();
    moved.position[0] = 9f32.to_bits();
    h.apply(EnvironmentPhase::Position {
        pose: moved,
        result: None,
    })
    .unwrap();
    assert_eq!(h.machine.state.cached_query(), cached);
    assert_eq!(h.machine.state.last_position(), Some(moved));
    h.apply(EnvironmentPhase::Update { diff_ms: 99 }).unwrap();
    assert_eq!(h.machine.state.timer_ms(), 1);
    h.apply(EnvironmentPhase::Update { diff_ms: 1 }).unwrap();
    h.apply(EnvironmentPhase::Update { diff_ms: u32::MAX })
        .unwrap();
    assert_eq!(h.machine.state.timer_ms(), 0);
    assert_eq!(h.machine.state.flags(), LIQUID | HIGH_LIQUID | IN_WATER);
    assert_eq!(h.machine.state.mirrors, initial_mirrors());
    assert_eq!(h.effects.calls.len(), 2);
}

#[test]
fn selective_kind_entry_zero_missing_rows_and_binary64_depth_stay_exact() {
    let mut h = Harness::new();
    h.position(LiquidStatus::InWater, 14, 8);
    h.expire();
    h.position(LiquidStatus::AboveWater, 0, 0x8000_0000);
    assert_eq!(h.machine.state.last_liquid(), Some(14));
    assert_eq!(h.machine.state.flags(), LIQUID | IN_WATER);
    h.expire();
    h.position(LiquidStatus::AboveWater, u32::MAX, 2);
    assert_eq!(h.machine.state.last_liquid(), None);
    assert_eq!(h.machine.state.flags(), LIQUID);
    h.expire();
    let mut p = pose();
    p.collision_height = 0x3fd5_5555;
    let query = h.machine.query_for_position(p).unwrap().unwrap();
    let level = 1.25f32;
    let height = f32::from_bits(p.collision_height);
    assert!(level <= height * 0.75f32);
    assert!(f64::from(level) > f64::from(height) * 0.75f64);
    h.apply(EnvironmentPhase::Position {
        pose: p,
        result: Some(Box::new(BoundLiquidResult {
            query,
            status: LiquidStatus::InWater,
            data: Some(LiquidData {
                entry: 0,
                type_flags: 0,
                level: level.to_bits(),
                depth_level: 0,
            }),
        })),
    })
    .unwrap();
    assert_eq!(h.machine.state.flags(), LIQUID | HIGH_LIQUID);
}

#[test]
fn unsupported_surface_inputs_fail_before_callbacks_or_state_changes() {
    let mut h = Harness::new();
    for (status, entry, kind) in [
        (LiquidStatus::UnderWater, 14, 8),
        (LiquidStatus::AboveWater, 14, 0x01),
        (LiquidStatus::WaterWalk, 14, 0x04),
        (LiquidStatus::InWater, 14, 0x10),
        (LiquidStatus::AboveWater, 11, 8),
        (LiquidStatus::WaterWalk, 11, 8),
        (LiquidStatus::InWater, 11, 8),
    ] {
        let before = h.machine.state.clone();
        assert_eq!(
            h.apply(h.phase(status, entry, kind)),
            Err(EnvironmentError::InvalidInput)
        );
        assert_eq!(h.machine.state, before);
        assert_eq!(h.machine.sequence, 0);
        assert!(h.effects.calls.is_empty());
    }
    let mut timers = initial_mirrors();
    let mut callbacks = NativeSurfaceCallbacks {
        timers: &mut timers,
        effects: &mut h.effects,
    };
    for intent in [
        EnvironmentCallback::RemoveSpellAuras { spell_id: 123 },
        EnvironmentCallback::CastSelf {
            spell_id: 123,
            triggered_flags: 1,
        },
        EnvironmentCallback::ReadHasAura { spell_id: 123 },
        EnvironmentCallback::ReadWaterBreathingInterval,
    ] {
        assert_eq!(
            callbacks.effect(intent, 0),
            Err(EnvironmentError::CallbacksUnavailable(intent))
        );
    }
    assert_eq!(
        callbacks.has_aura(123),
        Err(EnvironmentError::CallbacksUnavailable(
            EnvironmentCallback::ReadHasAura { spell_id: 123 }
        ))
    );
    assert_eq!(
        callbacks.breathing_interval(),
        Err(EnvironmentError::CallbacksUnavailable(
            EnvironmentCallback::ReadWaterBreathingInterval
        ))
    );
    assert!(h.effects.calls.is_empty());
}

#[test]
fn native_restore_rejects_unbound_malformed_or_unreachable_state() {
    let mut h = Harness::new();
    h.position(LiquidStatus::InWater, 14, 8);
    let valid = h.machine.state.snapshot();
    for change in 0..20 {
        let mut stored = valid.clone();
        match change {
            0 => stored.flags |= UNDERWATER,
            1 => stored.flags |= HIGH_SEA,
            2 => stored.flags |= IN_MAGMA,
            3 => stored.flags |= IN_SLIME,
            4 => stored.flags &= !LIQUID,
            5 => stored.timer_ms = 101,
            6 => stored.last_liquid = Some(11),
            7 => stored.last_liquid = Some(0),
            8 => stored.last_liquid = Some(u32::MAX),
            9 => stored.last_position = None,
            10 => stored.last_position.as_mut().unwrap().position[0] = f32::NAN.to_bits(),
            11 => stored.cached_query = None,
            12 => stored.cached_query.as_mut().unwrap().ticket = 0,
            13 => stored.cached_query.as_mut().unwrap().ticket = 2,
            14 => stored.cached_query.as_mut().unwrap().snapshot.inventory = [9; 32],
            15 => stored.cached_query.as_mut().unwrap().required_type = 2,
            16 => stored.cached_query.as_mut().unwrap().collision_height = f32::INFINITY.to_bits(),
            17 => stored.timer_ms = 99,
            18 => stored.last_position.as_mut().unwrap().position[1] = 5f32.to_bits(),
            _ => stored.flags = 0,
        }
        assert_eq!(
            stored.restore_native_surface(
                h.machine.snapshot,
                &h.machine.catalog,
                h.machine.sequence
            ),
            Err(EnvironmentError::InvalidCheckpoint),
            "change {change}"
        );
    }
    assert_eq!(
        valid.clone().restore_native_surface(
            h.machine.snapshot,
            &LiquidCatalog::from_wdbc(STORE1).unwrap(),
            1
        ),
        Err(EnvironmentError::Binding)
    );
    let mut binding = h.machine.snapshot;
    binding.source_epoch = [0; 32];
    assert_eq!(
        valid.restore_native_surface(binding, &h.machine.catalog, 1),
        Err(EnvironmentError::Binding)
    );
}

#[test]
fn native_restore_accepts_only_source_reachable_inactive_mirror_shapes() {
    let mut h = Harness::new();
    h.position(LiquidStatus::AboveWater, 14, 8);
    let valid = h.machine.state.clone();
    for timer in 0..4 {
        for operation in 0..4 {
            let mut state = valid.clone();
            match operation {
                0 => state.mirrors[timer].start(1000, 7),
                1 => state.mirrors[timer].set_scale(10),
                2 => state.mirrors[timer].set_frozen(true),
                _ => state.mirrors[timer].set_duration(123),
            }
            assert!(state
                .snapshot()
                .restore_native_surface(h.machine.snapshot, &h.machine.catalog, 1)
                .is_err());
            assert!(state.validate_native_surface(&h.machine.catalog).is_err());
        }
    }
    let mut stored = valid.snapshot();
    stored.mirrors = cleared_mirrors().each_ref().map(MirrorTimer::snapshot);
    assert!(stored
        .restore_native_surface(h.machine.snapshot, &h.machine.catalog, 1)
        .is_err());
    h.expire();
    h.position(LiquidStatus::NoWater, 0, 0);
    let mut stored = h.machine.state.snapshot();
    stored.flags = LIQUID;
    assert!(stored
        .restore_native_surface(h.machine.snapshot, &h.machine.catalog, 3)
        .is_err());
    h.expire();
    h.position(LiquidStatus::WaterWalk, 14, 8);
    assert_eq!(h.machine.state.mirrors, cleared_mirrors());
    for state in [valid, h.machine.state.clone()] {
        assert_eq!(
            state.mirrors[2],
            MirrorTimer::new(MirrorTimerType::FeignDeath)
        );
    }
}

#[test]
fn required_snapshot_fields_reject_omission_preserving_explicit_null_and_v2_encoding() {
    let h = Harness::new();
    let snapshot = h.machine.state.snapshot();
    let json = serde_json::to_value(&snapshot).unwrap();
    for field in json.as_object().unwrap().keys() {
        let mut changed = json.clone();
        changed.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<EnvironmentStateSnapshot>(changed).is_err(),
            "{field}"
        );
    }
    for field in ["last_liquid", "last_position", "cached_query"] {
        assert!(json[field].is_null());
    }
    let parsed: EnvironmentStateSnapshot = serde_json::from_value(json).unwrap();
    assert_eq!(parsed, snapshot);
    assert_eq!(
        parsed
            .clone()
            .restore_native_surface(h.machine.snapshot, &h.machine.catalog, 0)
            .unwrap(),
        h.machine.state
    );
    assert_eq!(
        parsed
            .restore(h.machine.snapshot, &h.machine.catalog, 0)
            .unwrap(),
        h.machine.state
    );
    let bytes = h.machine.checkpoint().to_bytes().unwrap();
    let restored = EnvironmentMachine::restore(
        h.machine.snapshot,
        h.machine.world,
        h.machine.player,
        h.machine.catalog.clone(),
        &bytes,
    )
    .unwrap();
    assert_eq!(restored.checkpoint().to_bytes().unwrap(), bytes);
}

#[test]
fn standalone_and_callback_free_native_domains_do_not_import_surface_state() {
    let mut h = Harness::new();
    h.position(LiquidStatus::InWater, 14, 8);
    for dry in [false, true] {
        if dry {
            h.expire();
            h.position(LiquidStatus::NoWater, 0, 0);
        }
        assert!(h
            .machine
            .state
            .validate_native_lifecycle(&h.machine.catalog)
            .is_err());
        assert_eq!(
            h.machine.state.snapshot().restore(
                h.machine.snapshot,
                &h.machine.catalog,
                h.machine.sequence
            ),
            Err(EnvironmentError::InvalidCheckpoint)
        );
        let bytes = h.machine.checkpoint().to_bytes().unwrap();
        assert!(EnvironmentMachine::restore(
            h.machine.snapshot,
            h.machine.world,
            h.machine.player,
            h.machine.catalog.clone(),
            &bytes
        )
        .is_err());
    }
}

#[test]
fn surface_query_binding_and_sequence_fail_before_executing_effects() {
    let mut h = Harness::new();
    let good = h.phase(LiquidStatus::InWater, 14, 8);
    for change in 0..5 {
        let mut bad = good.clone();
        let EnvironmentPhase::Position { pose, result } = &mut bad else {
            unreachable!()
        };
        match change {
            0 => result.as_mut().unwrap().query.ticket += 1,
            1 => result.as_mut().unwrap().query.snapshot.policy = [8; 32],
            2 => *result = None,
            3 => result.as_mut().unwrap().data = None,
            _ => pose.position[0] = 1f32.to_bits(),
        }
        assert!(h.apply(bad).is_err());
        assert!(h.effects.calls.is_empty());
        assert_eq!(h.machine.sequence, 0);
    }
    assert_eq!(
        h.machine.state.reduce_native_surface(
            h.machine.snapshot,
            &h.machine.catalog,
            u64::MAX,
            &good,
            &mut h.effects
        ),
        Err(EnvironmentError::Limit)
    );
    h.apply(good.clone()).unwrap();
    assert!(h.apply(good).is_err());
    assert_eq!(h.effects.calls.len(), 2);
}
