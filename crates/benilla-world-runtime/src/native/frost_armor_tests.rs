// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::native as n;
use synthetic_distinct_quest_fixture as scene;
fn world(seed: u64) -> NativeWorld {
    NativeWorld::synthetic_component_test(
        synthetic_frost_armor_fixture::profile(),
        [18; 16],
        synthetic_fixture::player(),
        seed,
    )
    .unwrap()
}
fn advance(w: &mut NativeWorld, a: &mut impl Authority, command: Option<Command>) {
    let input = command.map(|c| scene::command(w, c));
    w.step(w.state.tick + 1, input, a).unwrap();
}
#[test]
fn self_cast_armor_shared_resources_expiry_and_reopen() {
    let mut w = world(42);
    let mut a = n::synthetic_frost_armor_fixture::SpatialOnly::default();
    let before = w.state.effective_armor().unwrap();
    let mana = w.state.resources.caster.mana;
    advance(&mut w, &mut a, Some(Command::CastSelf { ability: 168 }));
    assert_eq!(w.state.effective_armor().unwrap(), before + 45);
    assert_eq!(w.state.resources.caster.mana, mana - 7);
    assert_eq!(w.state.frost_armor().unwrap().aura.remaining_ms(), 3900);
    let witness = w
        .state
        .combat
        .as_ref()
        .unwrap()
        .melee_reset
        .as_ref()
        .unwrap();
    assert_eq!(witness.source_origin().0, w.state.now_ms);
    assert_eq!(
        w.state.combat.as_ref().unwrap().melee_timers.remaining_ms(),
        [witness.source_origin().1[0] - w.profile.tick_ms, 0]
    );
    assert_eq!(
        Some(witness.release_ms()),
        w.state.resources.caster.last_mana_use_ms
    );
    let checkpoint = w.checkpoint().unwrap();
    let mut resumed =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), checkpoint).unwrap();
    let mut b = n::synthetic_frost_armor_fixture::SpatialOnly::default();
    for _ in 0..54 {
        advance(&mut w, &mut a, None);
        advance(&mut resumed, &mut b, None);
        assert_eq!(w.last_tick(), resumed.last_tick());
        assert_eq!(w.checkpoint().unwrap(), resumed.checkpoint().unwrap());
    }
    assert!(w.state.frost_armor().is_none());
    assert_eq!(w.state.effective_armor().unwrap(), before);
    assert!(w.state.npcs.values().all(|n| n.chilled.is_none()));
}
#[test]
fn cooldown_rejection_does_not_spend_mana_or_replace_the_holder() {
    let mut w = world(42);
    let mut a = n::synthetic_frost_armor_fixture::SpatialOnly::default();
    advance(&mut w, &mut a, Some(Command::CastSelf { ability: 168 }));
    let mana = w.state.resources.caster.mana;
    let cast = w.state.frost_armor().unwrap().cast_id;
    advance(&mut w, &mut a, Some(Command::CastSelf { ability: 168 }));
    assert_eq!(w.state.resources.caster.mana, mana);
    assert_eq!(w.state.frost_armor().unwrap().cast_id, cast);
    assert!(w.last_tick().unwrap().events.iter().any(|e| matches!(
        e,
        Event::Rejected(Rejection::CastFailed(spell::CastFailure::NotReady))
    )));
}
#[test]
fn melee_proc_has_real_binary_misses_and_non_neutral_reopen() {
    let mut hit = false;
    let mut miss = false;
    for seed in 0..128 {
        let mut w = world(seed);
        let mut a = n::synthetic_frost_armor_fixture::SpatialOnly::default();
        advance(&mut w, &mut a, Some(Command::CastSelf { ability: 168 }));
        for _ in 0..38 {
            if w.last_tick().unwrap().events.iter().any(|e| {
                matches!(
                    e,
                    Event::FrostArmor(FrostArmorEvent::Chilled { missed: true, .. })
                )
            }) {
                miss = true;
            }
            if let Some(npc) = w.state.npcs.values().find(|n| n.chilled.is_some()) {
                hit = true;
                let base = w.profile.npc(npc.id.spawn).unwrap();
                let selected = npc.initialized_creature().unwrap();
                assert_eq!(
                    npc.ai.effective_profile().mainhand_interval_ms,
                    selected.mainhand_interval_ms * 115 / 100
                );
                assert!(npc.ai.effective_profile().run_speed.as_f32() < base.ai.run_speed.as_f32());
                let mut resumed = NativeWorld::restore_synthetic_component_test(
                    w.profile.clone(),
                    w.checkpoint().unwrap(),
                )
                .unwrap();
                let mut b = n::synthetic_frost_armor_fixture::SpatialOnly::default();
                advance(&mut w, &mut a, None);
                advance(&mut resumed, &mut b, None);
                assert_eq!(w.last_tick(), resumed.last_tick());
                assert_eq!(w.checkpoint().unwrap(), resumed.checkpoint().unwrap());
                break;
            }
            advance(&mut w, &mut a, None);
        }
        if hit && miss {
            break;
        }
    }
    assert!(hit, "authored combat must apply Chilled");
    assert!(
        miss,
        "a successful Frost Armor proc must still resolve the triggered spell's miss"
    );
}

#[test]
fn frost_reset_replaces_prior_fireball_origin_and_refuses_forged_history() {
    let mut w = world(42);
    let mut a = n::synthetic_frost_armor_fixture::SpatialOnly::default();
    let target = w.state.npcs[&scene::HOSTILE].id;
    advance(
        &mut w,
        &mut a,
        Some(Command::Cast {
            ability: 133,
            target,
        }),
    );
    for _ in 0..60 {
        if w.state.combat.as_ref().unwrap().melee_reset.is_some() {
            break;
        }
        advance(&mut w, &mut a, None);
    }
    let prior = w
        .state
        .combat
        .as_ref()
        .unwrap()
        .melee_reset
        .as_ref()
        .unwrap()
        .clone();
    let ready = w.state.resources.caster.global_ready_at(133).unwrap_or(0);
    while w.state.now_ms < ready {
        advance(&mut w, &mut a, None);
    }
    advance(&mut w, &mut a, Some(Command::CastSelf { ability: 168 }));
    let witness = w
        .state
        .combat
        .as_ref()
        .unwrap()
        .melee_reset
        .as_ref()
        .unwrap();
    assert_ne!(witness, &prior);
    assert!(witness.release_ms() > prior.release_ms());
    assert_eq!(
        Some(witness.release_ms()),
        w.state.resources.caster.last_mana_use_ms
    );
    assert_eq!(
        w.state.combat.as_ref().unwrap().melee_timers.remaining_ms(),
        [witness.source_origin().1[0] - w.profile.tick_ms, 0]
    );
    let checkpoint = w.checkpoint().unwrap();
    let mut resumed =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), checkpoint.clone())
            .unwrap();
    let mut b = n::synthetic_frost_armor_fixture::SpatialOnly::default();
    advance(&mut w, &mut a, None);
    advance(&mut resumed, &mut b, None);
    assert_eq!(w.checkpoint().unwrap(), resumed.checkpoint().unwrap());
    for field in ["intervals_ms", "binding"] {
        let mut value = serde_json::to_value(&checkpoint).unwrap();
        let witness = &mut value["state"]["combat"]["melee_reset"];
        if field == "intervals_ms" {
            witness[field] = serde_json::json!([0, 0]);
        } else {
            witness[field]["captured_ms"] = serde_json::json!(0);
        }
        let mut forged: Checkpoint = serde_json::from_value(value).unwrap();
        forged.digest = crate::digest(b"benilla-native-state-v89\0", &forged.state);
        forged.checksum = crate::digest(
            b"benilla-native-checkpoint-v89\0",
            &(&forged.state, forged.digest, forged.history_tip),
        );
        assert!(NativeWorld::restore_synthetic_component_test(w.profile.clone(), forged).is_err());
    }
}

#[test]
fn instant_frost_reset_is_a_real_mainhand_writer_and_reopens() {
    let mut w = NativeWorld::synthetic_component_test(
        synthetic_frost_armor_fixture::mainhand_profile(),
        [19; 16],
        synthetic_fixture::player(),
        42,
    )
    .unwrap();
    let mut a = n::synthetic_frost_armor_fixture::SpatialOnly::default();
    advance(&mut w, &mut a, Some(Command::CastSelf { ability: 168 }));
    assert_eq!(
        w.state.combat.as_ref().unwrap().melee_timers.remaining_ms(),
        [2000 - w.profile.tick_ms, 0]
    );
    let mut resumed =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), w.checkpoint().unwrap())
            .unwrap();
    let mut b = n::synthetic_frost_armor_fixture::SpatialOnly::default();
    advance(&mut w, &mut a, None);
    advance(&mut resumed, &mut b, None);
    assert_eq!(w.checkpoint().unwrap(), resumed.checkpoint().unwrap());
}
