// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-authored component worlds; no live health mutation or native admission.
use super::*;
use synthetic_equipment_fixture::EquipmentAuthority;

fn world(rate: Option<f32>, lethal: bool) -> NativeWorld {
    let mut p = if lethal {
        synthetic_equipment_fixture::lethal_weapon_profile()
    } else {
        synthetic_equipment_fixture::weapon_profile()
    };
    // Explicit authored host control keeps real cast mana spent through escape,
    // so the same scheduled pulse can prove health-before-mana nonvacuously.
    p.player.world_mana_rate = synthetic_fixture::float(0.0625);
    if let Some(rate) = rate {
        p.initialize_player_health_regeneration(synthetic_fixture::float(rate))
            .unwrap();
    }
    NativeWorld::synthetic_component_test(p, [1; 16], synthetic_fixture::player(), 42).unwrap()
}
fn tick(w: &mut NativeWorld, command: Option<Command>) {
    let input = command.map(|c| synthetic_quest_fixture::command(w, c));
    w.step(w.state.tick + 1, input, &mut EquipmentAuthority::default())
        .unwrap();
}
fn healed(w: &NativeWorld) -> bool {
    w.last_tick()
        .unwrap()
        .events
        .iter()
        .any(|e| matches!(e, Event::PlayerHealthRegenerated { .. }))
}
fn reopen(w: &NativeWorld) {
    let restored =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), w.checkpoint().unwrap())
            .unwrap();
    assert_eq!(restored.checkpoint().unwrap(), w.checkpoint().unwrap());
}
fn damage(w: &mut NativeWorld) {
    let before = w.state.health;
    let target = w.state.npcs[&2].id;
    tick(
        w,
        Some(Command::Cast {
            ability: 133,
            target,
        }),
    );
    for _ in 0..150 {
        if w.state.health < before {
            break;
        }
        tick(w, None);
    }
    assert!(w.state.health < before, "actual NPC must damage the player");
    assert!(w.state.combat.as_ref().unwrap().in_combat);
}
fn escape(w: &mut NativeWorld) {
    let orientation = w.state.orientation;
    let movement_flags = w.state.player_movement_flags.unwrap();
    tick(
        w,
        Some(Command::MovementPose {
            destination: synthetic_fixture::point(100.0),
            orientation,
            movement_flags,
        }),
    );
    for _ in 0..300 {
        if !w.state.combat.as_ref().unwrap().in_combat {
            return;
        }
        tick(w, None);
    }
    panic!("actual leash/evade must clear player combat");
}
#[test]
fn real_damage_combat_suppression_then_health_before_mana_and_replay() {
    let mut w = world(Some(1.0), false);
    let anchor = w.checkpoint().unwrap();
    damage(&mut w);
    for _ in 0..25 {
        tick(&mut w, None);
        assert!(w.state.combat.as_ref().unwrap().in_combat);
        assert!(!healed(&w));
    }
    escape(&mut w);
    let damaged = w.state.health;
    let mut observed = false;
    for _ in 0..25 {
        tick(&mut w, None);
        if healed(&w) {
            observed = true;
            let events = &w.last_tick().unwrap().events;
            let health_index = events
                .iter()
                .position(|e| matches!(e, Event::PlayerHealthRegenerated { .. }))
                .unwrap();
            assert!(events
                .iter()
                .any(|event| matches!(event, Event::Mana { .. })));
            assert!(events
                .iter()
                .enumerate()
                .filter(|(_, e)| matches!(e, Event::Mana { .. }))
                .all(|(index, _)| health_index < index));
            assert!(matches!(
                events[health_index],
                Event::PlayerHealthRegenerated { diff_ms: 2000, .. }
            ));
            assert_eq!(w.state.resources.regen_elapsed_ms(), 0);
            break;
        }
    }
    assert!(observed);
    assert!(w.state.health > damaged);
    assert!(w.state.health <= w.state.combat.as_ref().unwrap().stats.max_health);
    reopen(&w);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), anchor).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
}
#[test]
fn full_health_consumes_shared_cadence_without_health_events_or_new_clock() {
    let mut w = world(Some(1.0), false);
    let health = w.state.health;
    for count in 1..=41 {
        tick(&mut w, None);
        assert_eq!(w.state.health, health);
        assert!(!healed(&w));
        assert_eq!(w.state.resources.regen_elapsed_ms(), (count * 100) % 2000);
    }
    reopen(&w);
}
#[test]
fn explicit_zero_and_absent_policy_never_heal_damaged_player() {
    for rate in [None, Some(0.0)] {
        let mut w = world(rate, false);
        assert_eq!(
            w.profile.player_health_regeneration.is_some(),
            rate.is_some()
        );
        damage(&mut w);
        escape(&mut w);
        let health = w.state.health;
        for _ in 0..41 {
            tick(&mut w, None);
            assert_eq!(w.state.health, health);
            assert!(!healed(&w));
        }
        reopen(&w);
    }
}
#[test]
fn actual_death_freezes_shared_remainder_and_never_resurrects() {
    let mut w = world(Some(1.0), true);
    let target = w.state.npcs[&2].id;
    tick(
        &mut w,
        Some(Command::Cast {
            ability: 133,
            target,
        }),
    );
    for _ in 0..150 {
        if !w.state.player_alive() {
            break;
        }
        tick(&mut w, None);
    }
    assert!(!w.state.player_alive());
    assert_eq!(w.state.health, 0);
    let remainder = w.state.resources.regen_elapsed_ms();
    let anchor = w.checkpoint().unwrap();
    let prefix = w.records().len();
    for _ in 0..41 {
        tick(&mut w, None);
        assert_eq!(w.state.health, 0);
        assert_eq!(w.state.resources.regen_elapsed_ms(), remainder);
        assert!(!healed(&w));
    }
    reopen(&w);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), anchor).unwrap();
    replay.replay(&w.records()[prefix..]).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
}
#[test]
fn configuration_is_explicit_atomic_and_checkpoint_bound() {
    let mut unsupported = synthetic_fixture::profile();
    let before = unsupported.digest();
    assert!(unsupported
        .initialize_player_health_regeneration(synthetic_fixture::float(1.0))
        .is_err());
    assert_eq!(unsupported.digest(), before);
    assert!(unsupported.player_health_regeneration.is_none());
    let mut p = synthetic_equipment_fixture::weapon_profile();
    let before = p.digest();
    assert!(p
        .initialize_player_health_regeneration(synthetic_fixture::float(-1.0))
        .is_err());
    assert_eq!(p.digest(), before);
    p.initialize_player_health_regeneration(synthetic_fixture::float(0.0))
        .unwrap();
    let chosen = p.digest();
    assert_ne!(chosen, before);
    p.initialize_player_health_regeneration(synthetic_fixture::float(0.0))
        .unwrap();
    assert_eq!(p.digest(), chosen);
    assert!(p
        .initialize_player_health_regeneration(synthetic_fixture::float(1.0))
        .is_err());
    assert_eq!(p.digest(), chosen);
    let w = world(Some(1.0), false);
    assert!(NativeWorld::restore_synthetic_component_test(p, w.checkpoint().unwrap()).is_err());
    assert!(NativeWorld::restore_synthetic_component_test(
        world(None, false).profile.clone(),
        w.checkpoint().unwrap()
    )
    .is_err());
}
