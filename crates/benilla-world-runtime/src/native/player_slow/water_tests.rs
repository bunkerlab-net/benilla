// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored 61004/61005 sources; no claim about stock spell1604 water flags.
use super::*;
use crate::native::{synthetic_fixture as f, synthetic_motion_fixture as motion};

fn world(masks: [u32; 2]) -> NativeWorld {
    let mut profile = synthetic_slow_fixture::with_effects(vec![]);
    let sources = [(61004, -60), (61005, -40)]
        .into_iter()
        .zip(masks)
        .map(|((spell, amount), mask)| {
            let mut source = source::synthetic_source(spell, amount, 6000);
            source.spell.words[22] = mask;
            source
        })
        .collect();
    let mut host = host::synthetic_host(&profile);
    host.max_player_level = 59;
    profile.player_slow = Some(
        PlayerSlowGenesis::from_sources(sources, host)
            .unwrap()
            .with_fixture_effects(
                // Reverse insertion proves selection is by spell ID, not holder allocation.
                [61005, 61004]
                    .into_iter()
                    .map(|spell_id| synthetic_slow_fixture::SyntheticSlowEffect {
                        tick: 1,
                        spell_id,
                        caster: crate::NpcId {
                            spawn: 2,
                            generation: 0,
                        },
                        phase: synthetic_slow_fixture::SyntheticSlowPhase::BeforePlayerUpdate,
                    })
                    .collect(),
            ),
    );
    let mut world =
        NativeWorld::synthetic_component_test(profile, [8; 16], f::player(), 1234).unwrap();
    world
        .step(
            1,
            None,
            &mut motion::SyntheticMotionAuthority {
                visits: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
    world
}

fn slow(world: &NativeWorld) -> &PlayerSlowState {
    world.state().player_slow.as_ref().unwrap()
}

fn removed(events: &[Event]) -> Vec<(u32, u64, u8, u8)> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::PlayerSlow(PlayerSlowEvent::Removed {
                holder,
                reason: PlayerSlowRemoval::Default,
                slot_level,
            }) => Some((holder.spell_id, holder.id, holder.slot, *slot_level)),
            _ => None,
        })
        .collect()
}

#[test]
fn water_masks_select_live_holders_and_zero_mask_survives() {
    for direction in [0x80, 0x100] {
        for mask in [0, 0x80, 0x100, 0x180] {
            let world = world([mask, 0]);
            let mut state = slow(&world).clone();
            let mut events = vec![];
            state
                .remove_interrupt_auras(
                    direction,
                    world.profile(),
                    &world.state().npcs,
                    &mut events,
                )
                .unwrap();
            let removes = mask & direction != 0;
            assert_eq!(
                removed(&events).len(),
                usize::from(removes),
                "mask={mask:x}, direction={direction:x}"
            );
            assert_eq!(
                state.holders().map(|h| h.spell_id).collect::<Vec<_>>(),
                if removes {
                    vec![61005]
                } else {
                    vec![61004, 61005]
                }
            );
            assert_eq!(
                state.speeds(),
                SpeedRates::from_amounts([if removes { -40 } else { -60 }]).unwrap()
            );
            let committed = state.clone();
            events.clear();
            state
                .remove_interrupt_auras(
                    direction,
                    world.profile(),
                    &world.state().npcs,
                    &mut events,
                )
                .unwrap();
            assert_eq!(state, committed);
            assert!(events.is_empty());
        }
    }
}

#[test]
fn restart_removal_recomputes_intermediate_rates_before_each_slot_clear() {
    let world = world([0x180, 0x180]);
    let mut state = slow(&world).clone();
    let mut events = vec![];
    state
        .remove_interrupt_auras(0x80, world.profile(), &world.state().npcs, &mut events)
        .unwrap();
    assert_eq!(
        removed(&events),
        vec![
            (61004, FIRST_HOLDER_ID + 1, 33, 1),
            (61005, FIRST_HOLDER_ID, 32, 1),
        ]
    );
    assert_eq!(events.len(), 8);
    for (chunk, amounts) in events.as_chunks::<4>().0.iter().zip([vec![-40], vec![]]) {
        let rates = SpeedRates::from_amounts(amounts).unwrap();
        for (event, kind) in
            chunk[..3]
                .iter()
                .zip([MoveType::Run, MoveType::RunBack, MoveType::Swim])
        {
            assert!(
                matches!(event, Event::PlayerSlow(PlayerSlowEvent::SpeedChanged {
                kind: actual, rate, speed,
            }) if *actual == kind && *rate == rates.run && *speed == rates.speed(kind).unwrap())
            );
        }
        assert!(matches!(
            chunk[3],
            Event::PlayerSlow(PlayerSlowEvent::Removed {
                reason: PlayerSlowRemoval::Default,
                ..
            })
        ));
    }
    assert_eq!(state.speeds(), SpeedRates::normal());
    assert_eq!(&state.slot_levels()[..3], &[1, 1, 0]);
    assert!(state.holders().next().is_none());
    events.clear();
    state
        .remove_interrupt_auras(0x100, world.profile(), &world.state().npcs, &mut events)
        .unwrap();
    assert!(events.is_empty());
}

#[test]
fn spell_removal_is_exact_id_default_mode_and_repeat_is_empty() {
    let world = world([0, 0]);
    let mut state = slow(&world).clone();
    let mut events = vec![];
    // A catalog entry alone is not presence, and an unknown absent ID is a no-op.
    for spell in [1604, 999] {
        state
            .remove_spell_auras(spell, world.profile(), &world.state().npcs, &mut events)
            .unwrap();
    }
    assert_eq!(&state, slow(&world));
    assert!(events.is_empty());
    state
        .remove_spell_auras(61004, world.profile(), &world.state().npcs, &mut events)
        .unwrap();
    assert_eq!(removed(&events), vec![(61004, FIRST_HOLDER_ID + 1, 33, 1)]);
    assert_eq!(
        state.holders().map(|h| h.spell_id).collect::<Vec<_>>(),
        vec![61005]
    );
    assert_eq!(state.speeds(), SpeedRates::from_amounts([-40]).unwrap());
    events.clear();
    state
        .remove_spell_auras(61004, world.profile(), &world.state().npcs, &mut events)
        .unwrap();
    assert!(events.is_empty());
    state
        .remove_spell_auras(61005, world.profile(), &world.state().npcs, &mut events)
        .unwrap();
    assert_eq!(state.speeds(), SpeedRates::normal());
}

#[test]
fn removal_uses_current_unit_level_and_only_absence_uses_configured_maximum() {
    let world = world([0x80, 0x80]);
    let mut profile = world.profile().clone();
    // Isolate current Unit-level lookup without admitting mutable NPC levels into genesis.
    profile.npcs[0].ai.level = 7;
    for (npcs, expected) in [(&world.state().npcs, 7), (&BTreeMap::new(), 59)] {
        let mut state = slow(&world).clone();
        let mut events = vec![];
        state
            .remove_interrupt_auras(0x80, &profile, npcs, &mut events)
            .unwrap();
        assert!(removed(&events)
            .iter()
            .all(|(_, _, _, level)| *level == expected));
        assert_eq!(&state.slot_levels()[..3], &[expected, expected, 0]);
        assert!(events
            .iter()
            .filter_map(|e| match e {
                Event::PlayerSlow(PlayerSlowEvent::Removed { holder, .. }) =>
                    Some(holder.caster_level),
                _ => None,
            })
            .all(|level| level == 1));
    }
}

#[test]
fn unsupported_callback_mask_and_passive_unapplication_fail_before_mutation() {
    let world = world([0x180, 0x180]);
    for mask in [1, 0x181, 0x200, u32::MAX] {
        let mut state = slow(&world).clone();
        let mut events = vec![];
        assert!(state
            .remove_interrupt_auras(mask, world.profile(), &world.state().npcs, &mut events)
            .is_err());
        assert_eq!(&state, slow(&world));
        assert!(events.is_empty());
    }
    let mut profile = world.profile().clone();
    profile.combat = f::initialized_profile().combat;
    profile
        .combat
        .as_mut()
        .unwrap()
        .initial_auras
        .push(InitialAuraMetadata {
            spell_id: 61004,
            effect_index: 0,
            aura_type: 29,
            misc_value: 0,
            amount: 1,
            attributes: 0x40,
            attributes_ex3: 0,
            caster_aura_state: 0,
            aura_interrupt_flags: 0,
        });
    let mut state = slow(&world).clone();
    let mut events = vec![];
    assert_eq!(
        state.remove_spell_auras(61004, &profile, &world.state().npcs, &mut events),
        Err(NativeError::UnsupportedMechanic(
            "initial player passive aura removal"
        ))
    );
    assert_eq!(&state, slow(&world));
    assert!(events.is_empty());
    for mask in [0x80, 0x100, 1] {
        profile
            .combat
            .as_mut()
            .unwrap()
            .initial_auras
            .last_mut()
            .unwrap()
            .aura_interrupt_flags = mask;
        assert_eq!(
            state.remove_interrupt_auras(0x80, &profile, &world.state().npcs, &mut events),
            Err(NativeError::UnsupportedMechanic(
                "source motion interruptible initial player auras"
            ))
        );
        assert_eq!(&state, slow(&world));
        assert!(events.is_empty());
    }
}

#[test]
fn holder_removal_checkpoint_retains_slot_residue_and_binds_catalog_mask() {
    let mut world = world([0x80, 0x100]);
    let profile = world.profile().clone();
    let npcs = world.state().npcs.clone();
    world
        .state
        .player_slow
        .as_mut()
        .unwrap()
        .remove_interrupt_auras(0x80, &profile, &npcs, &mut vec![])
        .unwrap();
    let bytes = world.checkpoint().unwrap().encode().unwrap();
    let mut reopened = NativeWorld::restore_synthetic_component_test(
        profile.clone(),
        Checkpoint::decode(&bytes).unwrap(),
    )
    .unwrap();
    assert_eq!(reopened.state_digest(), world.state_digest());
    assert_eq!(reopened.checkpoint().unwrap().encode().unwrap(), bytes);
    let mut first_events = vec![];
    let mut second_events = vec![];
    world
        .state
        .player_slow
        .as_mut()
        .unwrap()
        .remove_interrupt_auras(0x100, &profile, &npcs, &mut first_events)
        .unwrap();
    reopened
        .state
        .player_slow
        .as_mut()
        .unwrap()
        .remove_interrupt_auras(0x100, &profile, &npcs, &mut second_events)
        .unwrap();
    assert_eq!(first_events, second_events);
    assert_eq!(world.state_digest(), reopened.state_digest());
    assert_eq!(&slow(&reopened).slot_levels()[..3], &[1, 1, 0]);

    let mut changed = profile.clone();
    let mut sources: Vec<_> = profile
        .player_slow
        .as_ref()
        .unwrap()
        .catalog
        .spell_ids()
        .map(|id| {
            profile
                .player_slow
                .as_ref()
                .unwrap()
                .catalog
                .source(id)
                .unwrap()
                .clone()
        })
        .collect();
    sources[1].spell.words[22] = 0x80;
    changed.player_slow.as_mut().unwrap().catalog =
        PlayerSlowCatalog::validate_sources(sources).unwrap();
    assert!(NativeWorld::restore_synthetic_component_test(
        changed,
        Checkpoint::decode(&bytes).unwrap()
    )
    .is_err());
    let mut forged = serde_json::to_value(Checkpoint::decode(&bytes).unwrap()).unwrap();
    forged["state"]["player_slow"]["holders"][0]["aura_interrupt_flags"] = serde_json::json!(0x80);
    assert!(Checkpoint::decode(&serde_json::to_vec(&forged).unwrap()).is_err());
}

#[test]
fn actual_holder_selection_matches_extracted_source_restart_loops() {
    let fixture = include_str!("../../../tools/fixtures/water-aura-selection.tsv");
    let mut cases = 0;
    for row in fixture.lines().filter(|row| !row.starts_with('#')) {
        let values: Vec<u32> = row.split_whitespace().map(|v| v.parse().unwrap()).collect();
        assert_eq!(values.len(), 7);
        let world = world([values[0], values[1]]);
        let mut state = slow(&world).clone();
        let mut events = vec![];
        if values[2] == 0 {
            state
                .remove_interrupt_auras(
                    values[3],
                    world.profile(),
                    &world.state().npcs,
                    &mut events,
                )
                .unwrap();
        } else {
            state
                .remove_spell_auras(values[3], world.profile(), &world.state().npcs, &mut events)
                .unwrap();
        }
        assert_eq!(
            removed(&events)
                .iter()
                .map(|(id, ..)| *id)
                .collect::<Vec<_>>(),
            values[5..]
                .iter()
                .copied()
                .filter(|id| *id != 0)
                .collect::<Vec<_>>(),
            "{row}"
        );
        assert_eq!(state.holders().count(), 2 - values[4] as usize, "{row}");
        assert_eq!(
            state.speeds(),
            SpeedRates::from_amounts(state.holders().map(|h| h.amount)).unwrap()
        );
        cases += 1;
    }
    assert_eq!(cases, 112);
}
