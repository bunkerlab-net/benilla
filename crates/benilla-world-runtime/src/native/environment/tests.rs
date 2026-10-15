// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::collections::BTreeSet;
pub(super) const STORE0: &[u8] =
    include_bytes!("../../../tools/fixtures/environment-state/store0/LiquidType.dbc.test-fixture");
pub(super) const STORE1: &[u8] =
    include_bytes!("../../../tools/fixtures/environment-state/store1/LiquidType.dbc.test-fixture");
pub(super) fn machine() -> EnvironmentMachine {
    let catalog = LiquidCatalog::from_wdbc(STORE0).unwrap();
    EnvironmentMachine::new(
        binding(&catalog),
        [1; 16],
        crate::PlayerId {
            owner: [2; 16],
            character: [3; 16],
        },
        catalog,
    )
    .unwrap()
}
pub(super) fn binding(catalog: &LiquidCatalog) -> EnvironmentSnapshotBinding {
    EnvironmentSnapshotBinding {
        map: 0,
        content: [1; 32],
        source_epoch: [2; 32],
        inventory: [3; 32],
        policy: [4; 32],
        liquid_catalog: catalog.digest(),
    }
}
pub(super) fn pose() -> EnvironmentPose {
    EnvironmentPose {
        position: [0, 0, 0],
        collision_height: 2f32.to_bits(),
    }
}
pub(super) fn reply(
    machine: &EnvironmentMachine,
    status: LiquidStatus,
    entry: u32,
    type_flags: u32,
) -> BoundLiquidResult {
    BoundLiquidResult {
        query: machine.query_for_position(pose()).unwrap().unwrap(),
        status,
        data: (status != LiquidStatus::NoWater).then_some(LiquidData {
            entry,
            type_flags,
            level: 8f32.to_bits(),
            depth_level: 0,
        }),
    }
}
fn reopen(
    machine: &EnvironmentMachine,
    bytes: &[u8],
) -> Result<EnvironmentMachine, EnvironmentError> {
    EnvironmentMachine::restore(
        machine.snapshot,
        machine.world,
        machine.player,
        machine.catalog.clone(),
        bytes,
    )
}

/// Entirely owned synthetic model. It exercises private callback order and atomic
/// staging, never claims to implement the game's aura, spell, threat or mirror engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TestHost {
    pub trace: Vec<String>,
    pub observed_flags: Vec<u8>,
    pub auras: BTreeSet<u32>,
    pub interval: u32,
    pub scripted_has_aura: Option<bool>,
    pub fail_at: Option<usize>,
    pub mirrors: [i32; 3],
    pub threats: u32,
    pub interrupt_masks: Vec<u32>,
}
impl TestHost {
    pub fn new(interval: u32) -> Self {
        Self {
            trace: vec![],
            observed_flags: vec![],
            auras: BTreeSet::new(),
            interval,
            scripted_has_aura: None,
            fail_at: None,
            mirrors: [10; 3],
            threats: 0,
            interrupt_masks: vec![],
        }
    }
    fn note(&mut self, text: String, intent: EnvironmentCallback) -> Result<(), EnvironmentError> {
        if self.fail_at == Some(self.trace.len()) {
            return Err(EnvironmentError::CallbacksUnavailable(intent));
        }
        self.trace.push(text);
        Ok(())
    }
}
impl Callbacks for TestHost {
    fn effect(&mut self, intent: EnvironmentCallback, flags: u8) -> Result<(), EnvironmentError> {
        let text = match intent {
            EnvironmentCallback::UpdateThreatTables => "threat".into(),
            EnvironmentCallback::RemoveInterruptAuras { mask } => format!("interrupt:{mask}"),
            EnvironmentCallback::SetMirrorScale { timer, scale } => format!(
                "{}:{scale}",
                match timer {
                    EnvironmentMirror::Fatigue => "fatigue",
                    EnvironmentMirror::Breath => "breath",
                    EnvironmentMirror::Environmental => "environmental",
                }
            ),
            EnvironmentCallback::RemoveSpellAuras { spell_id } => {
                format!("remove_spell:{spell_id}")
            }
            EnvironmentCallback::CastSelf {
                spell_id,
                triggered_flags,
            } => format!("cast_spell:{spell_id}:{triggered_flags}"),
            _ => panic!("query is not an effect"),
        };
        self.note(text, intent)?;
        self.observed_flags.push(flags);
        match intent {
            EnvironmentCallback::UpdateThreatTables => self.threats += 1,
            EnvironmentCallback::RemoveInterruptAuras { mask } => self.interrupt_masks.push(mask),
            EnvironmentCallback::SetMirrorScale { timer, scale } => {
                self.mirrors[match timer {
                    EnvironmentMirror::Fatigue => 0,
                    EnvironmentMirror::Breath => 1,
                    EnvironmentMirror::Environmental => 2,
                }] = scale
            }
            EnvironmentCallback::RemoveSpellAuras { spell_id } => {
                self.auras.remove(&spell_id);
            }
            EnvironmentCallback::CastSelf { spell_id, .. } => {
                self.auras.insert(spell_id);
            }
            _ => unreachable!(),
        }
        Ok(())
    }
    fn has_aura(&mut self, spell_id: u32) -> Result<bool, EnvironmentError> {
        let has = self
            .scripted_has_aura
            .unwrap_or_else(|| self.auras.contains(&spell_id));
        self.note(
            format!("has_aura:{spell_id}:{}", u8::from(has)),
            EnvironmentCallback::ReadHasAura { spell_id },
        )?;
        Ok(has)
    }
    fn breathing_interval(&mut self) -> Result<u32, EnvironmentError> {
        self.note(
            format!("interval:{}", self.interval),
            EnvironmentCallback::ReadWaterBreathingInterval,
        )?;
        Ok(self.interval)
    }
}
fn staged_refresh(
    state: &mut EnvironmentState,
    host: &mut TestHost,
    input: BoundLiquidResult,
    catalog: &LiquidCatalog,
) -> Result<(), EnvironmentError> {
    let mut next = state.clone();
    let mut staged = host.clone();
    next.refresh_if_due(input, catalog, &mut staged)?;
    *state = next;
    *host = staged;
    Ok(())
}

#[test]
fn timer_boundary_same_pose_and_stationary_expiry_preserve_cached_state() {
    let mut m = machine();
    let first = reply(&m, LiquidStatus::AboveWater, 14, 2);
    m.set_position_phase(pose(), Some(first)).unwrap();
    assert_eq!(m.state.flags(), LIQUID);
    assert_eq!(m.state.last_liquid(), Some(14));
    assert_eq!(m.state.timer_ms(), 100);
    let mut moved = pose();
    moved.position[0] = 5f32.to_bits();
    m.set_position_phase(moved, None).unwrap();
    assert_eq!(m.state.cached_query(), Some(first.query));
    assert_eq!(m.state.last_position(), Some(moved));
    m.update_phase(99).unwrap();
    assert_eq!(m.state.timer_ms(), 1);
    m.set_position_phase(moved, None).unwrap();
    m.update_phase(1).unwrap();
    assert_eq!(m.state.timer_ms(), 0);
    assert_eq!(m.state.cached_query(), Some(first.query));
    m.update_phase(u32::MAX).unwrap();
    assert_eq!(m.state.flags(), LIQUID);
    let query = m.query_for_position(moved).unwrap().unwrap();
    assert_ne!(query.ticket, first.query.ticket);
    m.set_position_phase(moved, Some(BoundLiquidResult { query, ..first }))
        .unwrap();
    assert_eq!(m.state.timer_ms(), 100);
}

#[test]
fn unavailable_callback_rolls_back_timer_flags_last_liquid_sequence_and_journal() {
    let mut m = machine();
    let old = m.checkpoint().to_bytes().unwrap();
    let input = reply(&m, LiquidStatus::UnderWater, 11, 8);
    assert_eq!(
        m.set_position_phase(pose(), Some(input)),
        Err(EnvironmentError::CallbacksUnavailable(
            EnvironmentCallback::ReadHasAura { spell_id: 101 }
        ))
    );
    assert_eq!(m.checkpoint().to_bytes().unwrap(), old);
    assert!(m.records().is_empty());
    let first = reply(&m, LiquidStatus::AboveWater, 14, 2);
    m.set_position_phase(pose(), Some(first)).unwrap();
    m.update_phase(100).unwrap();
    let old = m.checkpoint().to_bytes().unwrap();
    let dry = reply(&m, LiquidStatus::NoWater, 0, 0);
    // Combined requested mask dispatches threat even though only LIQUID changed.
    assert_eq!(
        m.set_position_phase(pose(), Some(dry)),
        Err(EnvironmentError::CallbacksUnavailable(
            EnvironmentCallback::UpdateThreatTables
        ))
    );
    assert_eq!(m.checkpoint().to_bytes().unwrap(), old);
}

#[test]
fn water_entry_callback_failures_leave_owned_synthetic_host_and_state_unchanged() {
    let m = machine();
    let input = reply(&m, LiquidStatus::UnderWater, 11, 8);
    let mut expected = m.state.clone();
    let mut host = TestHost::new(60_000);
    staged_refresh(&mut expected, &mut host, input, &m.catalog).unwrap();
    assert_eq!(
        host.trace,
        [
            "has_aura:101:0",
            "cast_spell:101:1",
            "interval:60000",
            "breath:-1",
            "threat",
            "interrupt:128"
        ]
    );
    assert_eq!(
        host.observed_flags,
        [
            LIQUID,
            LIQUID | UNDERWATER,
            LIQUID | UNDERWATER | IN_WATER,
            LIQUID | UNDERWATER | IN_WATER | HIGH_LIQUID
        ]
    );
    for fail_at in 0..host.trace.len() {
        let mut state = m.state.clone();
        let mut staged = TestHost::new(60_000);
        staged.fail_at = Some(fail_at);
        let before_host = staged.clone();
        assert!(staged_refresh(&mut state, &mut staged, input, &m.catalog).is_err());
        assert_eq!(state, m.state);
        assert_eq!(staged, before_host);
    }
}

#[test]
fn same_spell_new_liquid_removal_precedes_actual_synthetic_aura_query() {
    let m = machine();
    let mut state = m.state.clone();
    state.last_liquid = Some(11);
    let mut host = TestHost::new(0);
    host.auras.insert(101);
    let input = reply(&m, LiquidStatus::InWater, 15, 0);
    staged_refresh(&mut state, &mut host, input, &m.catalog).unwrap();
    assert_eq!(
        &host.trace[..3],
        ["remove_spell:101", "has_aura:101:0", "cast_spell:101:1"]
    );
    assert!(host.auras.contains(&101));
    assert_eq!(state.last_liquid, Some(15));
}

#[test]
fn row_zero_bypassed_and_prior_zero_spell_retained_without_callbacks() {
    let mut m = machine();
    assert_eq!(m.catalog.get(0).unwrap().spell_id, 909);
    let input = reply(&m, LiquidStatus::AboveWater, 14, 2);
    m.set_position_phase(pose(), Some(input)).unwrap();
    m.update_phase(100).unwrap();
    let input = reply(&m, LiquidStatus::AboveWater, 0, 0);
    m.set_position_phase(pose(), Some(input)).unwrap();
    assert_eq!(m.state.last_liquid, Some(14));
    assert_eq!(m.state.flags, LIQUID);
}

#[test]
fn exact_query_binding_and_status_data_presence_are_required_atomically() {
    let mut m = machine();
    let good = reply(&m, LiquidStatus::AboveWater, 14, 2);
    let before = m.checkpoint().to_bytes().unwrap();
    for kind in 0..12 {
        let mut bad = good;
        match kind {
            0 => bad.query.ticket += 1,
            1 => bad.query.position[0] = 1f32.to_bits(),
            2 => bad.query.collision_height = 3f32.to_bits(),
            3 => bad.query.required_type = 2,
            4 => bad.query.snapshot.map += 1,
            5 => bad.query.snapshot.content[0] ^= 1,
            6 => bad.query.snapshot.source_epoch[0] ^= 1,
            7 => bad.query.snapshot.inventory[0] ^= 1,
            8 => bad.query.snapshot.policy[0] ^= 1,
            9 => bad.query.snapshot.liquid_catalog[0] ^= 1,
            10 => bad.data = None,
            _ => bad.status = LiquidStatus::NoWater,
        }
        assert!(m.set_position_phase(pose(), Some(bad)).is_err());
        assert_eq!(m.checkpoint().to_bytes().unwrap(), before);
    }
    assert!(m.set_position_phase(pose(), None).is_err());
    m.set_position_phase(pose(), Some(good)).unwrap();
    assert!(m.set_position_phase(pose(), Some(good)).is_err());
}

#[test]
fn checkpoint_inside_cached_window_and_tail_replay_match_uninterrupted_component() {
    let mut m = machine();
    let input = reply(&m, LiquidStatus::AboveWater, 14, 2);
    m.set_position_phase(pose(), Some(input)).unwrap();
    m.update_phase(99).unwrap();
    let retained = m.checkpoint();
    let checkpoint = retained.to_bytes().unwrap();
    m.discard_records(&retained).unwrap();
    let mut restored = reopen(&m, &checkpoint).unwrap();
    assert_eq!(restored.state.timer_ms(), 1);
    assert_eq!(restored.state.last_liquid(), Some(14));
    assert_eq!(restored.state.cached_query(), Some(input.query));
    m.set_position_phase(pose(), None).unwrap();
    m.update_phase(1).unwrap();
    let input = reply(&m, LiquidStatus::AboveWater, 0, 0);
    m.set_position_phase(pose(), Some(input)).unwrap();
    restored
        .replay_bytes(&m.records_to_bytes().unwrap())
        .unwrap();
    assert_eq!(restored.checkpoint(), m.checkpoint());
}

#[test]
fn checkpoint_and_journal_tampering_or_wrong_catalog_never_commit() {
    let mut m = machine();
    let initial = m.checkpoint().to_bytes().unwrap();
    let input = reply(&m, LiquidStatus::AboveWater, 14, 2);
    m.set_position_phase(pose(), Some(input)).unwrap();
    m.update_phase(99).unwrap();
    let bytes = m.checkpoint().to_bytes().unwrap();
    let mut checkpoint: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    checkpoint["state"]["timer_ms"] = 100.into();
    assert!(reopen(&m, &serde_json::to_vec(&checkpoint).unwrap()).is_err());
    assert!(EnvironmentMachine::restore(
        m.snapshot,
        m.world,
        m.player,
        LiquidCatalog::from_wdbc(STORE1).unwrap(),
        &bytes
    )
    .is_err());
    let tail = m.records_to_bytes().unwrap();
    let mut records: serde_json::Value = serde_json::from_slice(&tail).unwrap();
    records[1]["phase"]["Update"]["diff_ms"] = 98.into();
    let mut restored = reopen(&m, &initial).unwrap();
    let before = restored.checkpoint();
    assert!(restored
        .replay_bytes(&serde_json::to_vec(&records).unwrap())
        .is_err());
    assert_eq!(restored.checkpoint(), before);
    let mut trailing = tail.clone();
    trailing.push(b' ');
    assert!(restored.replay_bytes(&trailing).is_err());
    restored.replay_bytes(&tail).unwrap();
    assert_eq!(restored.checkpoint(), m.checkpoint());
}

#[test]
fn cross_liquid_and_dry_clear_callback_failures_are_transactional() {
    let m = machine();
    for dry in [false, true] {
        let mut initial = m.state.clone();
        initial.flags = LIQUID_FLAGS;
        initial.last_liquid = Some(11);
        let input = if dry {
            reply(&m, LiquidStatus::NoWater, 0, 0)
        } else {
            reply(&m, LiquidStatus::UnderWater, 12, 0x1f)
        };
        let mut successful_state = initial.clone();
        let mut successful_host = TestHost::new(60000);
        successful_host.auras.insert(101);
        staged_refresh(
            &mut successful_state,
            &mut successful_host,
            input,
            &m.catalog,
        )
        .unwrap();
        if dry {
            assert_eq!(
                successful_host.trace,
                [
                    "threat",
                    "interrupt:256",
                    "fatigue:10",
                    "breath:10",
                    "environmental:10",
                    "remove_spell:101"
                ]
            );
        }
        for fail_at in 0..successful_host.trace.len() {
            let mut state = initial.clone();
            let mut host = TestHost::new(60000);
            host.auras.insert(101);
            host.fail_at = Some(fail_at);
            let before_host = host.clone();
            assert!(staged_refresh(&mut state, &mut host, input, &m.catalog).is_err());
            assert_eq!(state, initial);
            assert_eq!(host, before_host);
        }
    }
}

#[test]
fn stale_checkpoint_cannot_discard_newer_records() {
    let mut m = machine();
    let stale = m.checkpoint();
    m.update_phase(0).unwrap();
    assert_eq!(
        m.discard_records(&stale),
        Err(EnvironmentError::InvalidCheckpoint)
    );
    assert_eq!(m.records().len(), 1);
    let retained = m.checkpoint();
    m.discard_records(&retained).unwrap();
    assert!(m.records().is_empty());
}

#[test]
fn selected_identity_full_catalog_profile_and_canonical_bounds_are_enforced() {
    let m = machine();
    let bytes = m.checkpoint().to_bytes().unwrap();
    let mut wrong_world = m.world;
    wrong_world[0] ^= 1;
    assert!(EnvironmentMachine::restore(
        m.snapshot,
        wrong_world,
        m.player,
        m.catalog.clone(),
        &bytes
    )
    .is_err());
    let mut wrong_player = m.player;
    wrong_player.character[0] ^= 1;
    assert!(EnvironmentMachine::restore(
        m.snapshot,
        m.world,
        wrong_player,
        m.catalog.clone(),
        &bytes
    )
    .is_err());
    let rows = u32::from_le_bytes(STORE0[4..8].try_into().unwrap()) as usize;
    let strings_at = 20 + rows * 16;
    let mut legacy_bytes = STORE0[..20].to_vec();
    let mut retained_rows = 0u32;
    for row in STORE0[20..strings_at].as_chunks::<16>().0 {
        let id = u32::from_le_bytes(row[..4].try_into().unwrap());
        let kind = u32::from_le_bytes(row[8..12].try_into().unwrap());
        if id <= u16::MAX as u32 && kind <= 3 {
            legacy_bytes.extend_from_slice(row);
            retained_rows += 1;
        }
    }
    assert!(retained_rows > 0 && (retained_rows as usize) < rows);
    legacy_bytes[4..8].copy_from_slice(&retained_rows.to_le_bytes());
    legacy_bytes.extend_from_slice(&STORE0[strings_at..]);
    let legacy = LiquidCatalog::from_legacy_wdbc(&legacy_bytes).unwrap();
    assert!(!legacy.is_complete());
    assert_eq!(
        EnvironmentMachine::new(binding(&legacy), m.world, m.player, legacy).unwrap_err(),
        EnvironmentError::Binding
    );
    let mut extra: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    extra["unexpected"] = true.into();
    assert!(reopen(&m, &serde_json::to_vec(&extra).unwrap()).is_err());
    let mut noncanonical = bytes.clone();
    noncanonical.push(b' ');
    assert!(reopen(&m, &noncanonical).is_err());
    assert_eq!(
        reopen(&m, &vec![b' '; MAX_ENVIRONMENT_BYTES + 1]).unwrap_err(),
        EnvironmentError::TooLarge
    );
    let mut target = machine();
    assert_eq!(
        target.replay_bytes(&vec![b' '; MAX_ENVIRONMENT_BYTES + 1]),
        Err(EnvironmentError::TooLarge)
    );
}

#[test]
fn invalid_numeric_observation_and_missing_interval_executor_do_not_commit() {
    let mut m = machine();
    let before = m.checkpoint();
    let mut malformed = reply(&m, LiquidStatus::AboveWater, 14, 2);
    malformed.data.as_mut().unwrap().level = f32::NAN.to_bits();
    assert_eq!(
        m.set_position_phase(pose(), Some(malformed)),
        Err(EnvironmentError::InvalidInput)
    );
    assert_eq!(m.checkpoint(), before);
    let mut invalid_pose = pose();
    invalid_pose.collision_height = f32::INFINITY.to_bits();
    assert_eq!(
        m.query_for_position(invalid_pose),
        Err(EnvironmentError::InvalidInput)
    );
    let input = reply(&m, LiquidStatus::UnderWater, 14, 2);
    assert_eq!(
        m.set_position_phase(pose(), Some(input)),
        Err(EnvironmentError::CallbacksUnavailable(
            EnvironmentCallback::ReadWaterBreathingInterval
        ))
    );
    assert_eq!(m.checkpoint(), before);
}
