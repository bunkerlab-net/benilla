// SPDX-License-Identifier: GPL-3.0-or-later
// Authored slot/lifecycle operands, not F1 observations; SpawnGroup.cpp:739-1433.
use super::*;
use benilla_sim::source_spawn_groups::{Formation, GroupMember};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
enum Call {
    Motion(ObjectHandle, FormationMotion),
    Random(ObjectHandle, [f32; 4]),
    Positions(u32),
    Variation,
    Clock,
    Waypoint(ObjectHandle),
}
struct Host {
    units: BTreeMap<ObjectHandle, FormationUnit>,
    calls: Vec<Call>,
    combat: bool,
    now: i64,
    targets: BTreeMap<ObjectHandle, ObjectHandle>,
    reject_positions: bool,
    reject_motion: bool,
    reject_variation: bool,
}
impl Default for Host {
    fn default() -> Self {
        Self {
            units: BTreeMap::new(),
            calls: vec![],
            combat: false,
            now: 1000,
            targets: BTreeMap::new(),
            reject_positions: false,
            reject_motion: false,
            reject_variation: false,
        }
    }
}
impl FormationOwner for Host {
    fn formation_unit(&mut self, object: ObjectHandle) -> Result<FormationUnit> {
        self.units
            .get(&object)
            .copied()
            .ok_or_else(|| refuse("authored.unit_absent"))
    }
    fn formation_creature(&mut self, _map: MapIdentity, guid: u32) -> Result<Option<ObjectHandle>> {
        Ok(self
            .units
            .iter()
            .find(|(_, u)| !u.player && u.database_guid == guid)
            .map(|(id, _)| *id))
    }
    fn formation_out_of_combat(&mut self, _group: GroupIdentity) -> Result<bool> {
        Ok(!self.combat)
    }
    fn formation_world_millis(&mut self) -> Result<i64> {
        self.calls.push(Call::Clock);
        Ok(self.now)
    }
    fn formation_waypoint(&mut self, object: ObjectHandle) -> Result<(u32, u32)> {
        self.calls.push(Call::Waypoint(object));
        Ok((7, 99))
    }
    fn formation_follow_target(&mut self, object: ObjectHandle) -> Result<Option<ObjectHandle>> {
        Ok(self.targets.get(&object).copied())
    }
    fn formation_motion(&mut self, object: ObjectHandle, motion: FormationMotion) -> Result<()> {
        self.calls.push(Call::Motion(object, motion));
        if self.reject_motion {
            return Err(motion.missing_owner());
        }
        if let FormationMotion::Follow { master, .. } = motion {
            self.targets.insert(object, master);
        }
        Ok(())
    }
    fn formation_random_motion(&mut self, object: ObjectHandle, position: [f32; 4]) -> Result<()> {
        self.calls.push(Call::Random(object, position));
        Ok(())
    }
    fn formation_positions(
        &mut self,
        formations: &mut SourceFormations,
        group: GroupIdentity,
    ) -> Result<()> {
        self.calls
            .push(Call::Positions(formations.state(group)?.options));
        if self.reject_positions {
            Err(refuse("execution.formation.f4_13b_shape_numerics_owner"))
        } else {
            Ok(())
        }
    }
    fn formation_variation(
        &mut self,
        _formations: &mut SourceFormations,
        _group: GroupIdentity,
    ) -> Result<()> {
        self.calls.push(Call::Variation);
        if self.reject_variation {
            Err(refuse(
                "execution.formation.f4_13b_periodic_variation_owner",
            ))
        } else {
            Ok(())
        }
    }
}
fn group() -> GroupIdentity {
    GroupIdentity {
        map: MapIdentity {
            map: 0,
            instance: 0,
            creation_generation: 1,
        },
        handle: SpawnGroupHandle {
            id: 10,
            generation: 1,
        },
    }
}
fn object(id: u64) -> ObjectHandle {
    ObjectHandle {
        thread: 1,
        construction: id,
    }
}
fn unit(guid: u32, player: bool) -> FormationUnit {
    FormationUnit {
        database_guid: guid,
        player,
        alive: true,
        temporary_summon: false,
        moving: false,
        respawn_position: [1.0, 2.0, 3.0, 4.0],
    }
}
fn fixture(count: u32, dynamic: bool, motion: u32) -> (SourceFormations, Host) {
    let mut f = SourceFormations::default();
    let entry = Formation {
        group: 10,
        kind: 1,
        spread_bits: 3_f32.to_bits(),
        options: 0,
        path: 123,
        movement: motion,
        comment: vec![],
    };
    let members: Vec<_> = (0..count)
        .map(|slot| GroupMember {
            group: 10,
            guid: 100 + slot,
            slot: slot as i32,
            chance: 0,
            ordinal: u64::from(slot),
            own_entry: 1,
            random_entry: false,
        })
        .collect();
    f.create(group(), entry, &members, dynamic).unwrap();
    let mut h = Host::default();
    for id in 0..count {
        h.units
            .insert(object(u64::from(id + 1)), unit(100 + id, false));
    }
    (f, h)
}
fn run(f: &mut SourceFormations, h: &mut Host, c: FormationCommand) -> FormationOutcome {
    f.execute(group(), c, h).unwrap()
}
fn attach(f: &mut SourceFormations, h: &mut Host, id: u64) -> FormationSlotIdentity {
    match run(
        f,
        h,
        FormationCommand::Attach(object(id), FormationSlotType::Static),
    ) {
        FormationOutcome::Slot(Some(s)) => s,
        other => panic!("{other:?}"),
    }
}

#[test]
fn fresh_slots_have_no_master_and_missing_default_does_not_allocate() {
    // SpawnGroup.cpp:742-759,873-877,1094-1098,1408-1409.
    let (mut f, mut h) = fixture(2, false, 0);
    assert_eq!(f.master(group()).unwrap(), None);
    assert_eq!(f.state(group()).unwrap().slot_guid, 2);
    let slots: Vec<_> = f
        .state(group())
        .unwrap()
        .slots
        .values()
        .map(|s| f.slot(s.unwrap()).unwrap())
        .collect();
    assert!(slots
        .iter()
        .all(|s| s.owner.is_none() && s.can_follow && s.recompute_position && !s.kind.removable()));
    assert!(f.is_master(slots[1].identity).unwrap());
    h.units.insert(object(8), unit(999, false));
    assert_eq!(
        run(
            &mut f,
            &mut h,
            FormationCommand::Attach(object(8), FormationSlotType::Static)
        ),
        FormationOutcome::Slot(None)
    );
    assert_eq!(f.state(group()).unwrap().slots.len(), 2);
    assert!(h.calls.is_empty());
}
#[test]
fn follower_before_master_attaches_without_movement_or_activation() {
    // SpawnGroup.cpp:1408-1431 deliberately leaves SetActiveObjectState commented out.
    let (mut f, mut h) = fixture(2, false, 0);
    let s = attach(&mut f, &mut h, 2);
    assert_eq!(s.id, 1);
    assert_eq!(f.binding(object(2)), Some(s));
    assert!(h.calls.is_empty());
    assert_eq!(f.master(group()).unwrap(), None);
}
#[test]
fn repeated_attach_returns_same_slot_without_freeing_or_moving() {
    // SpawnGroup.cpp:1400-1405 precedes FreeSlot even when no empty static slot remains.
    let (mut f, mut h) = fixture(1, false, 0);
    let s = attach(&mut f, &mut h, 1);
    h.calls.clear();
    assert_eq!(attach(&mut f, &mut h, 1), s);
    run(&mut f, &mut h, FormationCommand::AddCreature(object(1)));
    assert!(h.calls.is_empty());
    assert_eq!(f.state(group()).unwrap().slot_guid, 1);
}
#[test]
fn dead_attach_and_dead_or_temporary_script_add_are_rejected() {
    // SpawnGroup.cpp:1216-1228,1390-1391.
    let (mut f, mut h) = fixture(2, false, 0);
    h.units.get_mut(&object(1)).unwrap().alive = false;
    assert_eq!(
        run(
            &mut f,
            &mut h,
            FormationCommand::Attach(object(1), FormationSlotType::Static)
        ),
        FormationOutcome::Slot(None)
    );
    run(&mut f, &mut h, FormationCommand::AddCreature(object(1)));
    h.units.get_mut(&object(2)).unwrap().temporary_summon = true;
    run(&mut f, &mut h, FormationCommand::AddCreature(object(2)));
    assert_eq!(f.state(group()).unwrap().slot_guid, 2);
    assert!(h.calls.is_empty());
}
#[test]
fn player_without_master_slot_restores_zero_counter() {
    // SpawnGroup.cpp:1254-1260.
    let (mut f, mut h) = fixture(0, false, 0);
    h.units.insert(object(9), unit(0, true));
    run(&mut f, &mut h, FormationCommand::AddPlayer(object(9)));
    assert_eq!(f.state(group()).unwrap().slot_guid, 0);
    assert_eq!(f.binding(object(9)), None);
    assert!(h.calls.is_empty());
}
#[test]
fn script_and_player_slots_remove_and_counter_continues() {
    // SpawnGroup.cpp:1234-1246,1254-1279,1324-1329,1373-1382.
    let (mut f, mut h) = fixture(1, false, 0);
    h.units.insert(object(8), unit(888, false));
    h.units.insert(object(9), unit(0, true));
    run(&mut f, &mut h, FormationCommand::AddCreature(object(8)));
    let script = f.binding(object(8)).unwrap();
    assert_eq!(script.id, 1);
    assert!(f.slot(script).unwrap().kind.removable());
    run(&mut f, &mut h, FormationCommand::Remove(object(8)));
    assert_eq!(f.binding(object(8)), None);
    assert!(h
        .calls
        .contains(&Call::Motion(object(8), FormationMotion::Initialize)));
    run(&mut f, &mut h, FormationCommand::AddPlayer(object(9)));
    assert_eq!(f.binding(object(9)).unwrap().id, 2);
    run(&mut f, &mut h, FormationCommand::Remove(object(9)));
    assert!(h
        .calls
        .contains(&Call::Motion(object(9), FormationMotion::Initialize)));
    assert_eq!(f.binding(object(9)), None);
    assert_eq!(f.state(group()).unwrap().slot_guid, 3);
    run(&mut f, &mut h, FormationCommand::AddCreature(object(8)));
    assert_eq!(f.binding(object(8)).unwrap().id, 3);
    assert!(f.binding(object(8)).unwrap().allocation > script.allocation);
}
#[test]
fn dead_player_is_not_rejected_by_add() {
    // SpawnGroup.cpp:1249-1279 has no IsAlive gate.
    let (mut f, mut h) = fixture(1, false, 0);
    let mut u = unit(0, true);
    u.alive = false;
    h.units.insert(object(9), u);
    run(&mut f, &mut h, FormationCommand::AddPlayer(object(9)));
    assert_eq!(f.binding(object(9)).unwrap().id, 1);
    assert_eq!(h.calls, vec![Call::Positions(0)]);
}
#[test]
fn master_death_out_of_combat_retains_waypoint_then_promotes_first_static() {
    // SpawnGroup.cpp:925-935,964-972,1042-1057,890-893.
    let (mut f, mut h) = fixture(3, false, 2);
    attach(&mut f, &mut h, 1);
    attach(&mut f, &mut h, 3);
    attach(&mut f, &mut h, 2);
    h.calls.clear();
    h.units.get_mut(&object(1)).unwrap().alive = false;
    run(&mut f, &mut h, FormationCommand::Death(object(1)));
    assert_eq!(f.master(group()).unwrap(), Some(object(2)));
    assert_eq!(f.binding(object(1)), None);
    assert_eq!(f.binding(object(2)).unwrap().id, 0);
    assert_eq!(f.state(group()).unwrap().last_waypoint, 0);
    assert_eq!(f.state(group()).unwrap().waypoint_path, 0);
    assert_eq!(
        &h.calls[..6],
        &[
            Call::Waypoint(object(1)),
            Call::Clock,
            Call::Positions(0),
            Call::Motion(
                object(2),
                FormationMotion::Clear {
                    reset: true,
                    all: true
                }
            ),
            Call::Motion(
                object(2),
                FormationMotion::Waypoint {
                    path: 123,
                    linear: false
                }
            ),
            Call::Motion(object(2), FormationMotion::NextWaypoint(8))
        ]
    );
    assert!(!f.slot(f.binding(object(2)).unwrap()).unwrap().can_follow);
}
#[test]
fn combat_master_death_defers_replacement_until_home() {
    // SpawnGroup.cpp:1054-1068.
    let (mut f, mut h) = fixture(2, false, 0);
    attach(&mut f, &mut h, 1);
    attach(&mut f, &mut h, 2);
    h.calls.clear();
    h.combat = true;
    run(&mut f, &mut h, FormationCommand::Death(object(1)));
    assert_eq!(f.master(group()).unwrap(), None);
    assert!(f.state(group()).unwrap().master_died);
    assert_eq!(h.calls, vec![Call::Waypoint(object(1))]);
    run(&mut f, &mut h, FormationCommand::Home);
    assert_eq!(f.master(group()).unwrap(), Some(object(2)));
    assert!(!f.state(group()).unwrap().master_died);
    assert_eq!(f.state(group()).unwrap().last_waypoint, 7);
    assert_eq!(f.state(group()).unwrap().waypoint_path, 99);
}
#[test]
fn removable_slots_are_not_fallback_master_candidates() {
    // SpawnGroup.cpp:929-930 excludes both SCRIPT and PLAYER slots.
    let (mut f, mut h) = fixture(1, false, 0);
    h.units.insert(object(8), unit(888, false));
    h.units.insert(object(9), unit(0, true));
    run(&mut f, &mut h, FormationCommand::AddCreature(object(8)));
    run(&mut f, &mut h, FormationCommand::AddPlayer(object(9)));
    assert_eq!(
        run(&mut f, &mut h, FormationCommand::TryMaster(None)),
        FormationOutcome::Changed(false)
    );
    assert_eq!(f.master(group()).unwrap(), None);
}
#[test]
fn static_replacement_moves_but_dynamic_replacement_does_not() {
    // SpawnGroup.cpp:969-970,1415-1421.
    for dynamic in [false, true] {
        let (mut f, mut h) = fixture(2, dynamic, 0);
        attach(&mut f, &mut h, 1);
        attach(&mut f, &mut h, 2);
        assert!(h
            .calls
            .contains(&Call::Motion(object(1), FormationMotion::Idle)));
        h.calls.clear();
        run(&mut f, &mut h, FormationCommand::TryMaster(Some(object(2))));
        assert_eq!(f.master(group()).unwrap(), Some(object(2)));
        assert_eq!(
            h.calls
                .contains(&Call::Motion(object(2), FormationMotion::Idle)),
            !dynamic
        );
    }
}
#[test]
fn reset_removes_removable_slots_restores_real_owners_and_keeps_counter() {
    // SpawnGroup.cpp:979-1031 never resets m_slotGuid or current options/spread.
    let (mut f, mut h) = fixture(2, false, 0);
    attach(&mut f, &mut h, 1);
    attach(&mut f, &mut h, 2);
    h.units.insert(object(8), unit(888, false));
    run(&mut f, &mut h, FormationCommand::AddCreature(object(8)));
    run(&mut f, &mut h, FormationCommand::TryMaster(Some(object(2))));
    run(&mut f, &mut h, FormationCommand::WaypointEnd);
    run(&mut f, &mut h, FormationCommand::Options(1));
    run(&mut f, &mut h, FormationCommand::Spread(9_f32.to_bits()));
    h.calls.clear();
    run(&mut f, &mut h, FormationCommand::Reset);
    assert_eq!(f.master(group()).unwrap(), Some(object(1)));
    assert_eq!(f.binding(object(2)).unwrap().id, 1);
    assert_eq!(f.binding(object(8)), None);
    assert!(h
        .calls
        .contains(&Call::Motion(object(8), FormationMotion::Initialize)));
    let state = f.state(group()).unwrap();
    assert_eq!(state.slot_guid, 3);
    assert_eq!(state.options, 1);
    assert_eq!(state.spread_bits, 9_f32.to_bits());
    assert!(!state.mirror && !state.followers_stopped);
}
#[test]
fn real_master_return_swaps_fallback_back_to_default_slot() {
    // SpawnGroup.cpp:1136-1155,1415-1431.
    let (mut f, mut h) = fixture(2, false, 0);
    attach(&mut f, &mut h, 1);
    attach(&mut f, &mut h, 2);
    run(&mut f, &mut h, FormationCommand::Death(object(1)));
    assert_eq!(f.master(group()).unwrap(), Some(object(2)));
    attach(&mut f, &mut h, 1);
    assert_eq!(f.master(group()).unwrap(), Some(object(1)));
    assert_eq!(f.binding(object(2)).unwrap().id, 1);
}
#[test]
fn remove_swapped_creature_restores_its_real_slot_before_death() {
    // SpawnGroup.cpp:1299-1326.
    let (mut f, mut h) = fixture(2, false, 0);
    attach(&mut f, &mut h, 1);
    attach(&mut f, &mut h, 2);
    run(&mut f, &mut h, FormationCommand::TryMaster(Some(object(2))));
    h.calls.clear();
    run(&mut f, &mut h, FormationCommand::Remove(object(2)));
    assert_eq!(f.master(group()).unwrap(), Some(object(1)));
    assert_eq!(f.binding(object(2)), None);
    assert_eq!(
        h.calls,
        vec![Call::Motion(object(2), FormationMotion::Initialize)]
    );
}
#[test]
fn disband_initializes_alive_units_but_dynamic_master_keeps_motion() {
    // SpawnGroup.cpp:832-869.
    for dynamic in [false, true] {
        let (mut f, mut h) = fixture(2, dynamic, 0);
        attach(&mut f, &mut h, 1);
        attach(&mut f, &mut h, 2);
        h.calls.clear();
        run(&mut f, &mut h, FormationCommand::Disband);
        assert_eq!(f.binding(object(1)), None);
        assert_eq!(f.binding(object(2)), None);
        assert!(f.state(group()).unwrap().slots.is_empty());
        assert_eq!(
            h.calls
                .contains(&Call::Motion(object(1), FormationMotion::Initialize)),
            !dynamic
        );
        assert!(h
            .calls
            .contains(&Call::Motion(object(2), FormationMotion::Initialize)));
        assert_eq!(
            f.state(group()).unwrap().last_waypoint,
            if dynamic { 0 } else { 7 }
        );
    }
}
#[test]
fn same_follow_target_is_retained_and_binding_returns_false() {
    // SpawnGroup.cpp:787-807.
    let (mut f, mut h) = fixture(2, false, 0);
    attach(&mut f, &mut h, 1);
    attach(&mut f, &mut h, 2);
    h.calls.clear();
    assert_eq!(
        run(&mut f, &mut h, FormationCommand::BindFollowers),
        FormationOutcome::Changed(false)
    );
    assert!(h.calls.is_empty());
    h.targets.insert(object(2), object(9));
    run(&mut f, &mut h, FormationCommand::BindFollowers);
    assert_eq!(
        h.calls,
        vec![
            Call::Motion(
                object(2),
                FormationMotion::Clear {
                    reset: false,
                    all: true
                }
            ),
            Call::Motion(
                object(2),
                FormationMotion::Follow {
                    slot: f.binding(object(2)).unwrap(),
                    master: object(1)
                }
            )
        ]
    );
}
#[test]
fn options_recompute_before_assignment_but_compact_mutates_before_recompute() {
    // SpawnGroup.cpp:821-829,1204-1211.
    let (mut f, mut h) = fixture(1, false, 0);
    run(&mut f, &mut h, FormationCommand::Options(2));
    assert_eq!(h.calls, vec![Call::Positions(0)]);
    run(&mut f, &mut h, FormationCommand::Compact(false));
    assert_eq!(h.calls, vec![Call::Positions(0), Call::Positions(0)]);
    run(&mut f, &mut h, FormationCommand::Options(1));
    assert_eq!(h.calls.len(), 2);
}
#[test]
fn shape_refusal_keeps_mutation_and_prevents_later_follow() {
    // SpawnGroup.cpp:815-817,1429-1430.
    let (mut f, mut h) = fixture(2, false, 0);
    attach(&mut f, &mut h, 1);
    h.calls.clear();
    h.reject_positions = true;
    assert_eq!(
        f.execute(group(), FormationCommand::Shape(3), &mut h)
            .unwrap_err()
            .id,
        "execution.formation.f4_13b_shape_numerics_owner"
    );
    assert_eq!(f.state(group()).unwrap().shape, 3);
    assert_eq!(h.calls, vec![Call::Positions(0)]);
    assert_eq!(
        f.execute(group(), FormationCommand::Home, &mut h)
            .unwrap_err()
            .id,
        "execution.formation.stopped"
    );
}
#[test]
fn motion_refusal_keeps_attachment_and_respawn_position_before_clear() {
    // SpawnGroup.cpp:1417-1424.
    let (mut f, mut h) = fixture(1, false, 0);
    h.reject_motion = true;
    assert_eq!(
        f.execute(
            group(),
            FormationCommand::Attach(object(1), FormationSlotType::Static),
            &mut h
        )
        .unwrap_err()
        .id,
        "execution.formation.f5_motion_master_owner"
    );
    assert_eq!(f.master(group()).unwrap(), Some(object(1)));
    assert_eq!(
        f.state(group()).unwrap().spawn_position,
        Some([1.0, 2.0, 3.0, 4.0])
    );
    assert_eq!(
        h.calls,
        vec![Call::Motion(
            object(1),
            FormationMotion::Clear {
                reset: true,
                all: true
            }
        )]
    );
}
#[test]
fn follower_timer_is_strict_and_precedes_periodic_variation_refusal() {
    // SpawnGroup.cpp:1196-1201,1494-1508.
    let (mut f, mut h) = fixture(1, false, 0);
    run(&mut f, &mut h, FormationCommand::StopFollowers);
    h.now = 6000;
    run(&mut f, &mut h, FormationCommand::Update);
    assert!(f.state(group()).unwrap().followers_stopped);
    h.now = 6001;
    run(&mut f, &mut h, FormationCommand::Update);
    assert!(!f.state(group()).unwrap().followers_stopped);
    run(&mut f, &mut h, FormationCommand::Shape(0));
    run(&mut f, &mut h, FormationCommand::StopFollowers);
    h.now = 11002;
    h.reject_variation = true;
    assert_eq!(
        f.execute(group(), FormationCommand::Update, &mut h)
            .unwrap_err()
            .id,
        "execution.formation.f4_13b_periodic_variation_owner"
    );
    assert!(!f.state(group()).unwrap().followers_stopped);
    assert_eq!(h.calls.last(), Some(&Call::Variation));
}
#[test]
fn random_fallback_before_real_master_initialization_refuses_after_swap_and_clear() {
    // SpawnGroup.cpp:904 reads uninitialized RespawnPosition, SpawnGroup.h:111-114.
    let (mut f, mut h) = fixture(2, false, 1);
    attach(&mut f, &mut h, 2);
    h.units.remove(&object(1));
    assert_eq!(
        f.execute(group(), FormationCommand::Reset, &mut h)
            .unwrap_err()
            .id,
        "execution.formation.spawn_position_uninitialized_undefined"
    );
    assert_eq!(f.master(group()).unwrap(), Some(object(2)));
    assert_eq!(
        h.calls,
        vec![
            Call::Clock,
            Call::Positions(0),
            Call::Motion(
                object(2),
                FormationMotion::Clear {
                    reset: true,
                    all: true
                }
            )
        ]
    );
}
#[test]
fn random_fallback_after_real_master_initialization_uses_retained_origin() {
    // SpawnGroup.cpp:1418 initializes the later :904 read.
    let (mut f, mut h) = fixture(2, false, 1);
    attach(&mut f, &mut h, 1);
    attach(&mut f, &mut h, 2);
    h.calls.clear();
    run(&mut f, &mut h, FormationCommand::Death(object(1)));
    assert!(h
        .calls
        .contains(&Call::Random(object(2), [1.0, 2.0, 3.0, 4.0])));
}
#[test]
fn other_motion_types_do_not_read_uninitialized_random_origin() {
    // SpawnGroup.cpp:888-909 reads m_spawnPos only for RANDOM.
    for motion in [0, 2, 3, 4] {
        let (mut f, mut h) = fixture(2, false, motion);
        attach(&mut f, &mut h, 2);
        h.units.remove(&object(1));
        run(&mut f, &mut h, FormationCommand::Reset);
        assert_eq!(f.state(group()).unwrap().spawn_position, None);
        assert_eq!(f.master(group()).unwrap(), Some(object(2)));
    }
}
#[test]
fn deletion_uses_death_and_reset_last_waypoint_keeps_path() {
    // SpawnGroup.cpp:1072-1075; SpawnGroup.h:229.
    let (mut f, mut h) = fixture(1, false, 0);
    attach(&mut f, &mut h, 1);
    run(&mut f, &mut h, FormationCommand::Delete(object(1)));
    assert_eq!(f.binding(object(1)), None);
    assert_eq!(f.state(group()).unwrap().last_waypoint, 7);
    run(&mut f, &mut h, FormationCommand::ResetLastWaypoint);
    assert_eq!(f.state(group()).unwrap().last_waypoint, 0);
    assert_eq!(f.state(group()).unwrap().waypoint_path, 99);
}
#[test]
fn movement_update_shares_definition_but_not_other_maps_motion_cache() {
    // SpawnGroup.cpp:1477-1487 mutates shared FormationEntry and only this FormationData cache.
    let (mut f, mut h) = fixture(1, false, 0);
    let mut second = group();
    second.map.creation_generation = 2;
    let entry = f.definition(group()).unwrap().entry.clone();
    f.create(second, entry, &[], false).unwrap();
    attach(&mut f, &mut h, 1);
    run(
        &mut f,
        &mut h,
        FormationCommand::Movement { kind: 1, path: 23 },
    );
    assert_eq!(f.definition(second).unwrap().entry.path, 23);
    assert_eq!(f.definition(second).unwrap().entry.movement, 1);
    assert_eq!(f.state(second).unwrap().motion, 0);
    assert_eq!(
        f.state(group()).unwrap().spawn_position,
        Some([1.0, 2.0, 3.0, 23.0])
    );
}

#[test]
fn group_callbacks_construct_and_dispatch_the_actual_formation_owner() {
    // SpawnGroup.cpp:506-507,613-614,575-577,520-521; Creature.cpp:1580-1581.
    use super::spawn_group_tests::{fixture as groups_fixture, spawn as group_spawn, template};
    use benilla_sim::source_spawn_groups::{GroupInputData, GroupPhase};
    let entry = Formation {
        group: 10,
        kind: 1,
        spread_bits: 3_f32.to_bits(),
        options: 0,
        path: 0,
        movement: 0,
        comment: vec![],
    };
    let (groups, _states, g, _work) =
        groups_fixture(template(10, 0, 0), &[], false, |rows, owner| {
            rows.push(GroupPhase::Formations, GroupInputData::Formation(entry));
            for (slot, guid) in [(0, 100), (1, 101)] {
                rows.push(
                    GroupPhase::Members,
                    GroupInputData::Member {
                        group: 10,
                        guid,
                        slot,
                        chance: 0,
                    },
                );
                owner.spawns.insert(
                    SpawnKey {
                        kind: GridSpawnKind::Creature,
                        guid,
                    },
                    group_spawn(10),
                );
            }
        });
    let mut formations = SourceFormations::default();
    let mut host = Host::default();
    host.units.insert(object(2), unit(101, false));
    GroupCallback::FormationCreate
        .formation(g, &groups, &mut formations, &mut host)
        .unwrap();
    formations
        .execute(
            g,
            FormationCommand::Attach(object(2), FormationSlotType::Static),
            &mut host,
        )
        .unwrap();
    assert!(formations.binding(object(2)).is_some());
    GroupCallback::FormationUpdate
        .formation(g, &groups, &mut formations, &mut host)
        .unwrap();
    GroupCallback::FormationHome
        .formation(g, &groups, &mut formations, &mut host)
        .unwrap();
    GroupCallback::ResetLastWaypoint
        .formation(g, &groups, &mut formations, &mut host)
        .unwrap();
    GroupCallback::FormationRemove { object: object(2) }
        .formation(g, &groups, &mut formations, &mut host)
        .unwrap();
    assert_eq!(formations.binding(object(2)), None);
    assert_eq!(
        host.calls,
        vec![Call::Motion(object(2), FormationMotion::Initialize)]
    );
}

#[test]
fn dynamic_creation_without_existing_formation_resets_loaded_members() {
    // SpawnGroup.cpp:596-606 and ScriptMgr.cpp:3088-3098.
    let mut f = SourceFormations::default();
    let mut h = Host::default();
    h.units.insert(object(2), unit(101, false));
    let member = GroupMember {
        group: 10,
        guid: 101,
        slot: 1,
        chance: 0,
        ordinal: 0,
        own_entry: 1,
        random_entry: false,
    };
    let entry = Formation {
        group: 10,
        kind: 1,
        spread_bits: 3_f32.to_bits(),
        options: 0,
        path: 0,
        movement: 0,
        comment: vec![],
    };
    f.set_definition(group(), Some(entry), &[member], &mut h)
        .unwrap();
    assert!(f.definition(group()).unwrap().dynamic);
    assert_eq!(f.binding(object(2)).unwrap().id, 1);
    assert_eq!(f.master(group()).unwrap(), None);
    assert!(h.calls.is_empty());
    f.set_definition(group(), None, &[member], &mut h).unwrap();
    assert_eq!(f.binding(object(2)), None);
    assert_eq!(
        f.state(group()).unwrap_err().id,
        "execution.formation.absent"
    );
    assert_eq!(
        h.calls,
        vec![Call::Motion(object(2), FormationMotion::Initialize)]
    );
}

#[test]
fn disbanded_default_lookup_inserts_null_and_later_traversal_refuses() {
    // SpawnGroup.cpp:1098 uses operator[]; :1192 dereferences that inserted null.
    let (mut f, mut h) = fixture(1, false, 0);
    run(&mut f, &mut h, FormationCommand::Disband);
    assert_eq!(
        run(
            &mut f,
            &mut h,
            FormationCommand::Attach(object(1), FormationSlotType::Static)
        ),
        FormationOutcome::Slot(None)
    );
    assert_eq!(f.state(group()).unwrap().slots.get(&0), Some(&None));
    assert_eq!(
        f.execute(group(), FormationCommand::StartFollowers, &mut h)
            .unwrap_err()
            .id,
        "execution.formation.null_slot_undefined"
    );
    let (mut f, mut h) = fixture(0, false, 0);
    run(&mut f, &mut h, FormationCommand::StartFollowers);
}

#[test]
fn follower_deadline_refuses_only_overflow_after_stopping_slots() {
    // SpawnGroup.cpp:1198-1201 changes flags before adding five seconds.
    let (mut f, mut h) = fixture(1, false, 0);
    h.now = i64::MAX - 5000;
    run(&mut f, &mut h, FormationCommand::StopFollowers);
    assert_eq!(
        f.state(group()).unwrap().follower_start_millis,
        Some(i64::MAX)
    );
    let (mut f, mut h) = fixture(1, false, 0);
    h.now = i64::MAX - 4999;
    assert_eq!(
        f.execute(group(), FormationCommand::StopFollowers, &mut h)
            .unwrap_err()
            .id,
        "execution.formation.follower_deadline_overflow_undefined"
    );
    assert!(f.state(group()).unwrap().followers_stopped);
    assert!(
        !f.slot(f.state(group()).unwrap().slots[&0].unwrap())
            .unwrap()
            .can_follow
    );
}

#[test]
fn retained_constructor_formation_projection_or_explicit_absence() {
    let Some(path) = std::env::var_os("BENILLA_F1_FORMATION_FRAMING")
        .or_else(|| std::env::var_os("BENILLA_F1_SEAL8_FRAMING"))
    else {
        println!("SKIP unsealed MG/P0 constructor formation projection: set BENILLA_F1_FORMATION_FRAMING");
        return;
    };
    let path = std::path::PathBuf::from(path);
    let (held, raw) = metadata(&path.join("source-sites.json"), 2_097_152).unwrap();
    let sites: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    let direct = sites
        .as_object()
        .unwrap()
        .values()
        .filter(|site| {
            [
                site["callsite"]["function"].as_str(),
                site["provenance"]["instrumented_function"].as_str(),
            ]
            .into_iter()
            .flatten()
            .any(|s| s.contains("FormationData::") || s.contains("FormationSlotData::"))
        })
        .count();
    held.check().unwrap();
    println!("Unsealed formation metadata projection: source_sites_sha256={}, direct_sites={direct}, independently_matched_transitions=0",Sha256Id::of(&raw).as_str());
    println!("SKIP unsealed MG/P0 constructor formation comparison: no independently bound formation slot/master/callback projection");
}

#[test]
fn script_create_and_remove_change_live_group_presence_not_immutable_definition() {
    // SpawnGroup.cpp:596-614,575-577 use m_formationData, not m_entry.formationEntry.
    use super::spawn_group_tests::{fixture as groups_fixture, template};
    fn stop_on_update(
        _: &mut super::spawn_group_tests::Work<'_>,
        _: &mut SourceSpawnGroups,
        _: &mut SourceManagers,
        _: GroupIdentity,
        callback: GroupCallback,
    ) -> Result<()> {
        if callback == GroupCallback::FormationUpdate {
            return Err(refuse("authored.real_formation_update"));
        }
        Ok(())
    }
    let (mut groups, mut states, g, mut work) =
        groups_fixture(template(10, 0, 0), &[], false, |_, _| {});
    let mut formations = SourceFormations::default();
    let mut host = Host::default();
    let entry = Formation {
        group: 10,
        kind: 1,
        spread_bits: 3_f32.to_bits(),
        options: 0,
        path: 0,
        movement: 0,
        comment: vec![],
    };
    assert!(!groups.state(g).unwrap().has_formation);
    groups
        .set_formation_data(g, Some(entry.clone()), &mut formations, &mut host)
        .unwrap();
    assert!(groups.state(g).unwrap().has_formation);
    assert!(groups.definitions().unwrap()[&10].formation.is_none());
    groups
        .set_formation_data(g, None, &mut formations, &mut host)
        .unwrap();
    assert!(!groups.state(g).unwrap().has_formation);
    work.callback_hook = Some(stop_on_update);
    groups.update(g, &mut states, &mut work).unwrap();
    groups
        .set_formation_data(g, Some(entry), &mut formations, &mut host)
        .unwrap();
    assert_eq!(
        groups.update(g, &mut states, &mut work).unwrap_err().id,
        "authored.real_formation_update"
    );
}

#[test]
fn duplicate_slot_keys_keep_first_owner_and_last_master_guid_and_count_not_maximum() {
    // SpawnGroup.cpp:742-754 emplaces slots, assigns real master and uses size().
    let (base, mut host) = fixture(1, false, 0);
    let entry = base.definition(group()).unwrap().entry.clone();
    let members: Vec<_> = [(0, 100), (0, 200), (9, 300)]
        .into_iter()
        .enumerate()
        .map(|(ordinal, (slot, guid))| GroupMember {
            group: 10,
            guid,
            slot,
            chance: 0,
            ordinal: ordinal as u64,
            own_entry: 1,
            random_entry: false,
        })
        .collect();
    let mut formations = SourceFormations::default();
    formations.create(group(), entry, &members, false).unwrap();
    let state = formations.state(group()).unwrap();
    assert_eq!(state.slot_guid, 2);
    assert_eq!(state.real_master_guid, Some(200));
    assert_eq!(
        formations
            .slot(state.slots[&0].unwrap())
            .unwrap()
            .real_owner_guid,
        100
    );
    host.units.insert(object(8), unit(888, false));
    run(
        &mut formations,
        &mut host,
        FormationCommand::AddCreature(object(8)),
    );
    assert_eq!(formations.binding(object(8)).unwrap().id, 2);
}

#[test]
fn mismatched_player_removal_skips_player_slot_and_invalid_respawn_cast_refuses() {
    // SpawnGroup.cpp:1345-1369 skips PLAYER slots; :1484 casts master to Creature.
    let (mut f, mut h) = fixture(1, true, 0);
    attach(&mut f, &mut h, 1);
    h.units.insert(object(9), unit(0, true));
    run(&mut f, &mut h, FormationCommand::AddPlayer(object(9)));
    run(&mut f, &mut h, FormationCommand::TryMaster(Some(object(9))));
    h.calls.clear();
    run(&mut f, &mut h, FormationCommand::Remove(object(9)));
    assert_eq!(f.master(group()).unwrap(), Some(object(9)));
    assert!(h.calls.is_empty());
    assert_eq!(
        f.execute(
            group(),
            FormationCommand::Movement { kind: 1, path: 9 },
            &mut h
        )
        .unwrap_err()
        .id,
        "execution.formation.player_respawn_downcast_undefined"
    );
    assert_eq!(f.definition(group()).unwrap().entry.path, 9);
}

#[test]
fn follower_motion_refusal_preserves_the_attached_slot_and_precedes_follow() {
    // SpawnGroup.cpp:798-800 clears before MoveInFormation and recompute assignment.
    let (mut f, mut h) = fixture(2, false, 0);
    attach(&mut f, &mut h, 1);
    attach(&mut f, &mut h, 2);
    let slot = f.binding(object(2)).unwrap();
    f.slot_mut(slot).unwrap().recompute_position = false;
    h.targets.clear();
    h.calls.clear();
    h.reject_motion = true;
    assert_eq!(
        f.execute(group(), FormationCommand::BindFollowers, &mut h)
            .unwrap_err()
            .id,
        "execution.formation.f5_motion_clear_owner"
    );
    assert_eq!(f.binding(object(2)), Some(slot));
    assert!(!f.slot(slot).unwrap().recompute_position);
    assert_eq!(
        h.calls,
        vec![Call::Motion(
            object(2),
            FormationMotion::Clear {
                reset: false,
                all: true
            }
        )]
    );
}

mod shapes;
