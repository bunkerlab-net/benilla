// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored rows only; bit meanings are from pinned SpellDefines.h:351–357.
use super::*;
use crate::native::{synthetic_fixture as f, synthetic_spell_fixture as fixture};

fn inputs(flags: u32, cast_ms: u32) -> (Profile, init::StarterInput, SpellAuthorityInput) {
    let (mut profile, mut initial, mut source) = fixture::inputs();
    let AbilityAdmission::Available(spec) = &mut profile.abilities[0].admission else {
        unreachable!()
    };
    spec.movement_interrupts = flags & 0x01 != 0;
    spec.damage_pushback = flags & 0x02 != 0;
    spec.cast_ms = cast_ms;
    source.spells[0].words[21] = flags;
    source.spells[0].cast_time.base_ms = cast_ms as i32;
    initial.spells[0].words[21] = flags;
    profile
        .initialize_player(&initial, f::float(100.0), 1)
        .unwrap();
    (profile, initial, source)
}

fn profile(flags: u32, cast_ms: u32) -> Profile {
    let (mut profile, initial, source) = inputs(flags, cast_ms);
    profile
        .initialize_spell_authority(&initial, source)
        .unwrap();
    profile
}

fn world(profile: Profile) -> NativeWorld {
    NativeWorld::synthetic_component_test(profile, [8; 16], f::player(), 7).unwrap()
}

#[test]
fn raw_interrupt_mask_accepts_source_bits_with_owned_combat_domain_all_combinations() {
    let (_, _, source) = fixture::inputs();
    let mut raw = source.spells[0].clone();
    // All combinations of the five source-defined flags, including actual
    // Fireball's 0x0f and the previously misinterpreted 0x08/0x0a masks.
    for flags in 0..32 {
        raw.words[21] = flags;
        assert_eq!(raw.validate().is_ok(), flags <= 15, "flags={flags:#x}");
    }
    // Every unsupported bit, alone and combined with every supported mask.
    for bit in 4..32 {
        for supported in 0..16 {
            raw.words[21] = (1 << bit) | supported;
            assert_eq!(
                raw.validate(),
                Err(NativeError::UnsupportedMechanic(
                    "raw spell interrupt lifecycle"
                )),
                "bit={bit}, supported={supported:#x}"
            );
        }
    }
}

#[test]
fn each_raw_interrupt_boolean_is_bound_independently_of_the_other() {
    for flags in 0..4 {
        let (profile, _, source) = inputs(flags, 200);
        let AbilityAdmission::Available(original) = &profile.abilities[0].admission else {
            unreachable!()
        };
        for movement in [false, true] {
            for pushback in [false, true] {
                let mut spec = original.clone();
                spec.movement_interrupts = movement;
                spec.damage_pushback = pushback;
                assert_eq!(
                    source.spells[0].matches_spec(2, &spec).is_ok(),
                    movement == (flags & 1 != 0) && pushback == (flags & 2 != 0),
                    "flags={flags}, movement={movement}, pushback={pushback}"
                );
            }
        }
        assert_eq!(fixture::source(&profile).spells[0].words[21], flags);
    }
}

#[test]
fn selected_unsupported_interrupt_masks_fail_atomically_before_world_construction() {
    for flags in [16, 31, 0x8000_0000] {
        let (mut profile, initial, source) = inputs(flags, 200);
        let before = profile.clone();
        assert_eq!(
            profile.initialize_spell_authority(&initial, source),
            Err(NativeError::UnsupportedMechanic(
                "raw spell interrupt lifecycle"
            )),
            "flags={flags:#x}"
        );
        assert_eq!(profile, before);
        assert!(!profile.has_source_spell_authority());
    }
}

#[test]
fn selected_movement_start_cancels_only_bit_one_and_preserves_turns() {
    for flags in 0..4 {
        for moved in [false, true] {
            let mut world = world(profile(flags, 200));
            let initial = world.checkpoint().unwrap();
            let mut authority = fixture::spatial_only();
            let cast = f::cast(&world, 133);
            world.step(1, Some(cast), &mut authority).unwrap();
            let pose = crate::native::synthetic_quest_fixture::command(
                &world,
                Command::MovementPose {
                    destination: f::point(if moved { 1.0 } else { 0.0 }),
                    orientation: Orientation::new(1.0).unwrap(),
                    movement_flags: benilla_sim::creature_motion::GroundMovementFlags::from_bits(
                        u32::from(moved),
                    )
                    .unwrap(),
                },
            );
            world.step(2, Some(pose), &mut authority).unwrap();
            let interrupted = world.last_tick().unwrap().events.iter().any(|event| {
                matches!(
                    event,
                    Event::Spell {
                        event: spell::SpellEvent::Failed(spell::CastFailure::Interrupted),
                        ..
                    }
                )
            });
            assert_eq!(
                interrupted,
                moved && flags & 1 != 0,
                "flags={flags}, moved={moved}"
            );
            assert!(world.last_tick().unwrap().draws.is_empty());
            let mut replay =
                NativeWorld::restore_synthetic_component_test(world.profile.clone(), initial)
                    .unwrap();
            replay.replay(world.records()).unwrap();
            assert_eq!(replay.checkpoint().unwrap(), world.checkpoint().unwrap());
            let restored = NativeWorld::restore_synthetic_component_test(
                world.profile.clone(),
                world.checkpoint().unwrap(),
            )
            .unwrap();
            assert_eq!(restored.state_digest(), world.state_digest());
        }
    }
}

#[test]
fn selected_npc_damage_pushback_depends_on_bit_two_not_movement() {
    for flags in 0..4 {
        let mut world = world(profile(flags, 3000));
        let initial = world.checkpoint().unwrap();
        let mut authority = fixture::spatial_only();
        for tick in 1..=80 {
            let input = matches!(tick, 1 | 35).then(|| f::cast(&world, 133));
            world.step(tick, input, &mut authority).unwrap();
        }
        assert!(world.records().iter().any(|record| {
            record
                .events
                .iter()
                .any(|event| matches!(event, Event::Melee { .. }))
        }));
        let pushed = world.records().iter().any(|record| {
            record.events.iter().any(|event| {
                matches!(
                    event,
                    Event::Spell {
                        event: spell::SpellEvent::Pushback(_),
                        ..
                    }
                )
            })
        });
        let drew = world.records().iter().any(|record| {
            record
                .draws
                .iter()
                .any(|draw| draw.purpose == DrawPurpose::Pushback)
        });
        assert_eq!(pushed, flags & 2 != 0, "flags={flags}");
        assert_eq!(drew, pushed);
        let mut replay =
            NativeWorld::restore_synthetic_component_test(world.profile.clone(), initial).unwrap();
        replay.replay(world.records()).unwrap();
        assert_eq!(replay.checkpoint().unwrap(), world.checkpoint().unwrap());
    }
}

fn rehash(mut value: serde_json::Value) -> Checkpoint {
    let state: crate::native::persistence::StateCheckpoint =
        serde_json::from_value(value["state"].clone()).unwrap();
    let digest = crate::digest(b"benilla-native-state-v54\0", &state);
    value["digest"] = serde_json::to_value(digest).unwrap();
    let tip: crate::Digest = serde_json::from_value(value["history_tip"].clone()).unwrap();
    value["checksum"] = serde_json::to_value(crate::digest(
        b"benilla-native-checkpoint-v54\0",
        &(&state, digest, tip),
    ))
    .unwrap();
    serde_json::from_value(value).unwrap()
}

#[test]
fn old_policy_only_and_old_bit_eight_contexts_refuse_restore_and_replay_after_outer_rehash() {
    assert_eq!(
        SPELL_AUTHORITY_POLICY,
        "cmangos-8ec338a1-native-spell-authority-v7"
    );
    for old_policy in [
        "cmangos-8ec338a1-native-spell-authority-v3",
        "cmangos-8ec338a1-native-spell-authority-v4",
        "cmangos-8ec338a1-native-spell-authority-v5",
        "cmangos-8ec338a1-native-spell-authority-v6",
    ] {
        for old_bit_eight in [false, true] {
            let current = profile(u32::from(old_bit_eight), 200);
            let mut old = current.clone();
            let catalog = old
                .interactions
                .as_mut()
                .unwrap()
                .spell_authority
                .as_mut()
                .unwrap();
            catalog.policy = old_policy;
            if old_bit_eight {
                catalog.spells.get_mut(&133).unwrap().words[21] = 8;
                catalog.initial_modifiers.spell_words[0][21] = 8;
            }
            assert_eq!(old.validate(), Err(NativeError::InvalidProfile));
            let old_digest = old.digest();
            assert_ne!(old_digest, current.digest());
            let mut world = world(current.clone());
            let initial = world.checkpoint().unwrap();
            let mut authority = fixture::spatial_only();
            let cast = f::cast(&world, 133);
            world.step(1, Some(cast), &mut authority).unwrap();
            world.step(2, None, &mut authority).unwrap();
            for checkpoint in [initial.clone(), world.checkpoint().unwrap()] {
                let mut value = serde_json::to_value(checkpoint).unwrap();
                value["state"]["profile"] = serde_json::to_value(old_digest).unwrap();
                let old_checkpoint = rehash(value);
                let decoded = Checkpoint::decode(&old_checkpoint.encode().unwrap()).unwrap();
                assert_eq!(
                    NativeWorld::restore_synthetic_component_test(current.clone(), decoded.clone())
                        .err(),
                    Some(NativeError::Incompatible)
                );
                assert_eq!(
                    NativeWorld::restore_synthetic_component_test(old.clone(), decoded).err(),
                    Some(NativeError::InvalidProfile)
                );
            }
            let mut records = world.records().to_vec();
            assert_eq!(records.len(), 2);
            records[1].profile = old_digest;
            // A record's digest is derived; recomputing it does not authorize a
            // different profile. The reader preserves its old identity verbatim,
            // and replay must roll back the valid first record on the old tail.
            assert_ne!(records[1].digest(), world.records()[1].digest());
            let records = decode_records(&encode_records(&records).unwrap()).unwrap();
            let mut replay =
                NativeWorld::restore_synthetic_component_test(current, initial.clone()).unwrap();
            assert_eq!(replay.replay(&records), Err(NativeError::ReplayMismatch));
            assert_eq!(replay.checkpoint().unwrap(), initial);
        }
    }
    assert_eq!(NATIVE_FORMAT_VERSION, 54);
}
