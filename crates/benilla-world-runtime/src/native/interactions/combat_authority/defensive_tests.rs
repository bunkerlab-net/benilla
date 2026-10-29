// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored ordinary source-host probes, not Northshire content admission.
use super::*;
use crate::native::{
    synthetic_combat_fixture as combat, synthetic_fixture as f, synthetic_spell_fixture as spells,
};

fn raw(profile: &Profile, flags: u32) -> NpcCombatAuthoritySource {
    NpcCombatAuthoritySource {
        entry: profile.npcs[0].entry,
        extra_flags: flags,
        static_flags1: 0,
        static_flags2: 0,
        static_flags3: 0,
        static_flags4: 0,
    }
}
fn defensive(mut profile: Profile) -> Profile {
    profile.npcs[0].ai.reaction = ai::Reaction::Defensive;
    profile
        .initialize_combat_authority(vec![raw(&profile, NO_AGGRO_ON_SIGHT)])
        .unwrap();
    profile
}
fn world(profile: Profile) -> NativeWorld {
    NativeWorld::synthetic_component_test(profile, [8; 16], f::player(), 7).unwrap()
}

#[test]
fn reaction_is_derived_from_exact_retained_extra_flag_and_updates_atomically() {
    for flags in [
        0,
        NO_AGGRO_ON_SIGHT,
        FORCE_ATTACKING_CAPABILITY,
        NO_AGGRO_ON_SIGHT | FORCE_ATTACKING_CAPABILITY,
    ] {
        let mut p = combat::profile();
        p.npcs[0].ai.reaction = if flags & NO_AGGRO_ON_SIGHT != 0 {
            ai::Reaction::Defensive
        } else {
            ai::Reaction::Aggressive
        };
        p.initialize_combat_authority(vec![raw(&p, flags)]).unwrap();
        let before = p.digest();
        for unknown in [1, 4, 0x800, 0x4000000] {
            assert!(p
                .initialize_combat_authority(vec![raw(&p, flags | unknown)])
                .is_err());
            assert_eq!(p.digest(), before);
        }
        let mismatched = flags ^ NO_AGGRO_ON_SIGHT;
        assert!(p
            .initialize_combat_authority(vec![raw(&p, mismatched)])
            .is_err());
        assert_eq!(p.digest(), before);
        for reaction in [
            ai::Reaction::Passive,
            ai::Reaction::Defensive,
            ai::Reaction::Aggressive,
        ] {
            if reaction == p.npcs[0].ai.reaction {
                continue;
            }
            let mut changed = p.clone();
            changed.npcs[0].ai.reaction = reaction;
            assert!(changed.validate().is_err());
        }
    }
}

#[test]
fn defensive_sight_stays_idle_but_source_damage_initiates_retaliation_and_replays() {
    let p = defensive(spells::profile());
    let mut w = world(p.clone());
    let initial = w.checkpoint().unwrap();
    let mut authority = spells::SpatialOnly::default();
    for tick in 1..=10 {
        w.step(tick, None, &mut authority).unwrap();
    }
    assert_eq!(w.state.npcs[&2].ai.phase(), ai::Phase::Idle);
    assert!(w.records().iter().all(|record| record.draws.is_empty()));
    assert!(authority
        .base
        .derived
        .iter()
        .any(|reply| matches!(&reply.facts,
        Facts::SourceMotion(MotionFacts::Combat(CombatFacts::NpcSemantics(facts)))
        if facts.can_initiate_attack && facts.can_attack_on_sight)));
    let cast = f::cast(&w, 133);
    w.step(11, Some(cast), &mut authority).unwrap();
    for tick in 12..=65 {
        w.step(tick, None, &mut authority).unwrap();
        if w.state.npcs[&2].ai.phase() == ai::Phase::Combat {
            break;
        }
    }
    assert!(w.state.npcs[&2].health < p.npcs[0].max_health);
    assert_eq!(w.state.npcs[&2].ai.phase(), ai::Phase::Combat);
    let mut replay = NativeWorld::restore_synthetic_component_test(p.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
    let cp = w.checkpoint().unwrap();
    w.checkpoint_committed(&cp).unwrap();
    let mut restored = NativeWorld::restore_synthetic_component_test(p, cp).unwrap();
    let tick = w.state.tick + 1;
    w.step(tick, None, &mut authority).unwrap();
    restored
        .step(tick, None, &mut spells::SpatialOnly::default())
        .unwrap();
    assert_eq!(w.state_digest(), restored.state_digest());
}

#[test]
fn defensive_late_source_fault_rolls_back_retaliation_frame() {
    let mut w = world(defensive(spells::profile()));
    let mut authority = spells::SpatialOnly::default();
    let input = f::cast(&w, 133);
    w.step(1, Some(input), &mut authority).unwrap();
    let mut failed = false;
    for _ in 0..50 {
        let before = w.checkpoint().unwrap();
        let records = w.records().len();
        authority.fail_phase = Some(SourceSpellPhase::Impact);
        match w.step(w.state.tick + 1, None, &mut authority) {
            Ok(_) => continue,
            Err(_) => {
                assert_eq!(w.checkpoint().unwrap(), before);
                assert_eq!(w.records().len(), records);
                assert_eq!(w.state.npcs[&2].ai.phase(), ai::Phase::Idle);
                authority.fail_phase = None;
                w.step(w.state.tick + 1, None, &mut authority).unwrap();
                failed = true;
                break;
            }
        }
    }
    assert!(failed, "impact receipt fault must be reached");
}

#[test]
fn old_catalog_policy_rejects_even_with_rehashed_matching_checkpoint_context() {
    let p = combat::profile();
    let w = world(p.clone());
    let mut old = p.clone();
    old.interactions
        .as_mut()
        .unwrap()
        .combat_authority
        .as_mut()
        .unwrap()
        .policy = "cmangos-8ec338a1-native-combat-authority-v1";
    assert_ne!(old.digest(), p.digest());
    assert_eq!(old.validate(), Err(NativeError::AuthorityBinding));
    let mut cp = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    cp["state"]["profile"] = serde_json::to_value(old.digest()).unwrap();
    let state: crate::native::persistence::StateCheckpoint =
        serde_json::from_value(cp["state"].clone()).unwrap();
    let digest = crate::digest(b"benilla-native-state-v74\0", &state);
    let tip: crate::Digest = serde_json::from_value(cp["history_tip"].clone()).unwrap();
    cp["digest"] = serde_json::to_value(digest).unwrap();
    cp["checksum"] = serde_json::to_value(crate::digest(
        b"benilla-native-checkpoint-v74\0",
        &(&state, digest, tip),
    ))
    .unwrap();
    let cp: Checkpoint = serde_json::from_value(cp).unwrap();
    assert!(NativeWorld::restore_synthetic_component_test(old, cp).is_err());
}

#[test]
fn defensive_reaction_survives_generation_respawn_without_proactive_aggro() {
    let mut p = defensive(spells::profile());
    p.npcs[0].max_health = 1;
    let mut w = world(p.clone());
    let initial = w.checkpoint().unwrap();
    let original_id = w.state.npcs[&2].id;
    let mut authority = spells::SpatialOnly::default();
    let cast = f::cast(&w, 133);
    w.step(1, Some(cast), &mut authority).unwrap();
    for tick in 2..=100 {
        w.step(tick, None, &mut authority).unwrap();
        if w.state.npcs[&2].id.generation != 0 {
            break;
        }
    }
    assert_eq!(w.state.npcs[&2].id.generation, 1);
    assert_ne!(w.state.npcs[&2].id, original_id);
    let start = w.records().len();
    for _ in 0..10 {
        w.step(w.state.tick + 1, None, &mut authority).unwrap();
    }
    assert_eq!(w.state.npcs[&2].ai.phase(), ai::Phase::Idle);
    assert_eq!(w.state.npcs[&2].health, 1);
    assert!(w.records()[start..]
        .iter()
        .all(|record| record.draws.is_empty()));
    let mut replay = NativeWorld::restore_synthetic_component_test(p, initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
}
