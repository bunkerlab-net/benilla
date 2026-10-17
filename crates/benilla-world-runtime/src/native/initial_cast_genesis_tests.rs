// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic source contracts and exact native RNG; never actual Human admission.
use super::*;
use benilla_sim::starter_init as init;
use synthetic_initial_cast_fixture as fixture;

fn resign(value: serde_json::Value) -> Checkpoint {
    let mut cp: Checkpoint = serde_json::from_value(value).unwrap();
    cp.digest = crate::digest(b"benilla-native-state-v38\0", &cp.state);
    cp.checksum = crate::digest(
        b"benilla-native-checkpoint-v38\0",
        &(&cp.state, cp.digest, cp.history_tip),
    );
    cp
}
fn restore(profile: &Profile, checkpoint: Checkpoint) -> Result<NativeWorld, NativeError> {
    NativeWorld::restore_synthetic_component_test(profile.clone(), checkpoint)
}
#[test]
fn pending_projection_requires_source_seal_without_optional_spell_catalog() {
    let (mut draft, input, prepared, source) = fixture::inputs();
    assert!(init::initialize_mage(&input).is_err());
    assert_eq!(prepared.creation_plan().draw_requests().len(), 1);
    assert_eq!(prepared.first_entry_plan().draw_requests().len(), 1);
    assert_ne!(
        prepared.creation_plan().identity(),
        prepared.first_entry_plan().identity()
    );
    draft.validate().unwrap(); // Intermediate draft setters remain usable.
    assert!(NativeWorld::synthetic_component_test(
        draft.clone(),
        [8; 16],
        synthetic_fixture::player(),
        7
    )
    .is_err());
    let before = draft.clone();
    let mut missing = source.clone();
    missing.dependencies = SpellSourceRows::Unknown;
    assert!(draft
        .initialize_initial_cast_source(&input, missing)
        .is_err());
    assert_eq!(draft, before);
    let mut wrong = source.clone();
    wrong.store_identity[0] ^= 1;
    assert!(draft.initialize_initial_cast_source(&input, wrong).is_err());
    assert_eq!(draft, before);
    let mut wrong = source.clone();
    wrong.hooks.conditions = SpellSourceRows::Unknown;
    assert!(draft.initialize_initial_cast_source(&input, wrong).is_err());
    assert_eq!(draft, before);
    draft
        .initialize_initial_cast_source(&input, source)
        .unwrap();
    assert!(!draft.has_source_spell_authority());
    let w = NativeWorld::synthetic_component_test(draft, [8; 16], synthetic_fixture::player(), 7)
        .unwrap();
    assert_eq!(w.state.rng_counter, 1); // Creation has not also consumed a die.
    assert_eq!(
        (w.state.tick, w.state.next_sequence, w.state.next_cast_id),
        (0, 0, 1)
    );
    assert!(w.records().is_empty());
    assert_eq!(w.state.initial_cast_genesis.draws()[0].value, 5);
    assert_eq!(w.state.initial_cast_genesis.draws()[0].evaluated_amount, 5);
}
#[test]
fn exact_genesis_is_repeatable_and_does_not_recharge_on_restore_or_validation() {
    for (seed, expected) in [(7, 5), (13, 0), (1, 6)] {
        let mut w = fixture::world(seed);
        let genesis = w.state.initial_cast_genesis.clone();
        assert_eq!(genesis.draws()[0].value, expected);
        assert_eq!(genesis.draws()[0].consumed_words, 1);
        for _ in 0..3 {
            w.profile.validate().unwrap();
            let cp = w.checkpoint().unwrap();
            w = restore(
                w.profile(),
                Checkpoint::decode(&cp.encode().unwrap()).unwrap(),
            )
            .unwrap();
            w.checkpoint_committed(&cp).unwrap();
            assert_eq!(w.state.rng_counter, 1);
            assert_eq!(w.state.initial_cast_genesis, genesis);
        }
        let again = fixture::world(seed);
        assert_eq!(w.state_digest(), again.state_digest());
    }
    assert_eq!(
        fixture::world(1).profile_digest(),
        fixture::world(7).profile_digest()
    );
    let mut w = fixture::world(7);
    let mut draws = vec![];
    let sample = w
        .state
        .draw_range(1, DrawPurpose::Direct { slot: 0 }, 0, 6, &mut draws)
        .unwrap();
    assert_eq!(draws[0].first_counter, 1);
    assert_eq!(sample, (0xd3f45c88cd86539du64 % 7) as i64);
}
#[test]
fn rehashed_genesis_ledger_and_key_transplants_reject() {
    let w = fixture::world(7);
    let cp = w.checkpoint().unwrap();
    let base = serde_json::to_value(&cp).unwrap();
    let mutations = [
        ("/state/initial_cast_genesis/version", serde_json::json!(2)),
        (
            "/state/initial_cast_genesis/boundary",
            serde_json::json!("FirstWorldEntryOnly"),
        ),
        (
            "/state/initial_cast_genesis/first_counter",
            serde_json::json!(1),
        ),
        (
            "/state/initial_cast_genesis/end_counter",
            serde_json::json!(2),
        ),
        (
            "/state/initial_cast_genesis/semantic_requests",
            serde_json::json!(0),
        ),
        (
            "/state/initial_cast_genesis/draws/0/occurrence",
            serde_json::json!(1),
        ),
        (
            "/state/initial_cast_genesis/draws/0/spell_id",
            serde_json::json!(3050),
        ),
        (
            "/state/initial_cast_genesis/draws/0/effect_slot",
            serde_json::json!(1),
        ),
        (
            "/state/initial_cast_genesis/draws/0/first_counter",
            serde_json::json!(1),
        ),
        (
            "/state/initial_cast_genesis/draws/0/consumed_words",
            serde_json::json!(2),
        ),
        (
            "/state/initial_cast_genesis/draws/0/low",
            serde_json::json!(1),
        ),
        (
            "/state/initial_cast_genesis/draws/0/high",
            serde_json::json!(7),
        ),
        (
            "/state/initial_cast_genesis/draws/0/value",
            serde_json::json!(4),
        ),
        (
            "/state/initial_cast_genesis/draws/0/evaluated_amount",
            serde_json::json!(4),
        ),
        ("/state/seed", serde_json::json!(13)),
        ("/state/world/0", serde_json::json!(9)),
        ("/state/player/owner/0", serde_json::json!(9)),
        ("/state/player/character/0", serde_json::json!(9)),
        (
            "/state/initial_cast_genesis/realized_trace/0",
            serde_json::json!(255),
        ),
        (
            "/state/initial_cast_genesis/source_binding/Prepared/entry_plan/0",
            serde_json::json!(255),
        ),
    ];
    for (path, replacement) in mutations {
        let mut value = base.clone();
        let target = value.pointer_mut(path).unwrap();
        if *target == replacement {
            continue;
        }
        *target = replacement;
        assert!(restore(w.profile(), resign(value)).is_err(), "{path}");
    }
    let mut no_draw = base.clone();
    no_draw["state"]["initial_cast_genesis"]["draws"] = serde_json::json!([]);
    assert!(restore(w.profile(), resign(no_draw)).is_err());
    {
        let name = "initial_cast_genesis";
        let mut absent = base.clone();
        absent["state"].as_object_mut().unwrap().remove(name);
        assert!(serde_json::from_value::<Checkpoint>(absent).is_err());
        let mut null = base.clone();
        null["state"][name] = serde_json::Value::Null;
        assert!(serde_json::from_value::<Checkpoint>(null).is_err());
    }
}
#[test]
fn baseline_lower_upper_checked_bounds_and_live_guards() {
    let w = fixture::world(7);
    let base = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    for counter in [0, 2] {
        let mut v = base.clone();
        v["state"]["rng_counter"] = counter.into();
        assert!(restore(w.profile(), resign(v)).is_err());
    }
    assert!(initial_cast_genesis::validate_counter(1, u64::MAX, 1).is_err());
    assert!(initial_cast_genesis::validate_counter(u64::MAX, 1, u64::MAX).is_err());
    let mut w = w;
    w.step(
        1,
        None,
        &mut synthetic_fixture::SyntheticAuthority::default(),
    )
    .unwrap();
    let actual = w.checkpoint().unwrap();
    for counter in [0, MAX_RNG_WORDS_PER_TICK + 2] {
        let mut v = serde_json::to_value(&actual).unwrap();
        v["state"]["rng_counter"] = counter.into();
        assert!(restore(w.profile(), resign(v)).is_err());
    }
    let before = w.clone();
    w.state.rng_counter = 0;
    assert!(w.checkpoint().is_err());
    assert!(w
        .step(
            2,
            None,
            &mut synthetic_fixture::SyntheticAuthority::default()
        )
        .is_err());
    assert_eq!(w.state.tick, before.state.tick);
}
#[test]
fn no_draw_cast_order_is_bound_and_empty_legacy_receipts_are_explicit() {
    let (mut p, input, prepared, source) = fixture::inputs();
    p.initialize_initial_cast_source(&input, source.clone())
        .unwrap();
    let first =
        NativeWorld::synthetic_component_test(p.clone(), [8; 16], synthetic_fixture::player(), 7)
            .unwrap();
    let mut reordered = input.clone();
    let init::Rows::Complete(defaults) = &mut reordered.default_spells else {
        unreachable!()
    };
    defaults.reverse();
    let mut numeric = prepared.dependencies().clone();
    numeric.source_identity = reordered.source_identity();
    let other = init::prepare_starter(&reordered, &numeric, prepared.order()).unwrap();
    assert_ne!(
        prepared.first_entry_plan().identity(),
        other.first_entry_plan().identity()
    );
    assert_eq!(
        prepared.first_entry_plan().draw_requests().len(),
        other.first_entry_plan().draw_requests().len()
    );
    let mut second = synthetic_fixture::initialized_profile();
    second
        .initialize_player_prepared(&reordered, &other, synthetic_fixture::float(100.0), 1)
        .unwrap();
    let mut source = source;
    source.source_identity = reordered.source_identity();
    second
        .initialize_initial_cast_source(&reordered, source)
        .unwrap();
    let second =
        NativeWorld::synthetic_component_test(second, [8; 16], synthetic_fixture::player(), 7)
            .unwrap();
    assert_eq!(
        first.state.initial_cast_genesis.draws()[0].value,
        second.state.initial_cast_genesis.draws()[0].value
    );
    assert_ne!(
        first.state.initial_cast_genesis.realized_trace(),
        second.state.initial_cast_genesis.realized_trace()
    );
    let empty = synthetic_fixture::world();
    assert_eq!(empty.state.initial_cast_genesis.end_counter(), 0);
    assert!(empty.state.initial_cast_genesis.draws().is_empty());
    assert!(
        serde_json::to_value(empty.checkpoint().unwrap()).unwrap()["state"]["initial_cast_genesis"]
            .is_object()
    );
}
#[test]
fn gameplay_replay_and_compaction_preserve_the_genesis_receipt() {
    let mut w = fixture::world(7);
    let original = w.checkpoint().unwrap();
    let receipt = w.state.initial_cast_genesis.clone();
    let command = synthetic_fixture::cast(&w, 60001);
    let mut authority = synthetic_fixture::KernelAuthority::default();
    w.step(1, Some(command), &mut authority).unwrap();
    for tick in 2..=20 {
        w.step(tick, None, &mut authority).unwrap();
    }
    let first_gameplay = w.records().iter().flat_map(|r| &r.draws).next().unwrap();
    assert_eq!(first_gameplay.first_counter, receipt.end_counter());
    assert_ne!(first_gameplay.cast_id, 0);
    let mut replay = restore(w.profile(), original).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
    let cp = w.checkpoint().unwrap();
    w.checkpoint_committed(&cp).unwrap();
    let restored = restore(w.profile(), cp).unwrap();
    assert_eq!(restored.state.initial_cast_genesis, receipt);
    assert_eq!(restored.state.rng_counter, w.state.rng_counter);
}

#[test]
fn prepared_source_fireball_continues_after_entry_without_fabricated_gameplay_cast() {
    let (p, input, mut spells) = synthetic_spell_fixture::inputs();
    let (mut p, input, _, source) = fixture::prepare(p, input);
    spells.initial_cast_dependencies = source.dependencies.clone();
    spells.modifier_spell_ids = SpellSourceRows::Complete(vec![81, 133, 3050]);
    p.initialize_initial_cast_source(&input, source).unwrap();
    p.initialize_spell_authority(&input, spells).unwrap();
    let mut w =
        NativeWorld::synthetic_component_test(p, [8; 16], synthetic_fixture::player(), 7).unwrap();
    let before = w.checkpoint().unwrap();
    let command = synthetic_fixture::cast(&w, 133);
    let mut authority = synthetic_spell_fixture::spatial_only();
    w.step(1, Some(command), &mut authority).unwrap();
    for tick in 2..=20 {
        w.step(tick, None, &mut authority).unwrap();
    }
    let hit = w
        .records()
        .iter()
        .flat_map(|r| &r.draws)
        .find(|d| d.purpose == DrawPurpose::Hit)
        .unwrap();
    assert_eq!(hit.first_counter, 1);
    assert_eq!(hit.cast_id, 1);
    let mut replay = restore(w.profile(), before).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
}
#[test]
fn independent_seal_retains_noninitial_rank_rows_and_exact_expanded_scope() {
    let (mut p, input, prepared, mut source) = fixture::inputs();
    source.hooks.spell_rank_chain = SpellSourceRows::Complete(vec![
        SpellRankSource {
            spell_id: 60001,
            previous_spell: 0,
            first_spell: 60001,
            rank: 1,
            required_spell: 0,
        },
        SpellRankSource {
            spell_id: 60003,
            previous_spell: 60001,
            first_spell: 60001,
            rank: 2,
            required_spell: 0,
        },
    ]);
    let SpellSourceRows::Complete(ids) = &mut source.queried_spell_ids else {
        unreachable!()
    };
    ids.push(60003);
    let mut numeric = prepared.dependencies().clone();
    numeric.hook_scope_identity = source.hook_scope_identity();
    let prepared = init::prepare_starter(&input, &numeric, prepared.order()).unwrap();
    p.initialize_player_prepared(&input, &prepared, synthetic_fixture::float(100.0), 1)
        .unwrap();
    p.initialize_initial_cast_source(&input, source.clone())
        .unwrap();
    NativeWorld::synthetic_component_test(p.clone(), [8; 16], synthetic_fixture::player(), 7)
        .unwrap();
    let SpellSourceRows::Complete(ids) = &mut source.queried_spell_ids else {
        unreachable!()
    };
    ids.pop();
    assert!(p.initialize_initial_cast_source(&input, source).is_err());
}
#[test]
fn event_ai_cannot_regenerate_its_receipt_inside_genesis_span() {
    use benilla_sim::event_ai as source;
    let (mut p, input, _, proof) = fixture::prepare(
        synthetic_event_ai_fixture::profile(),
        synthetic_fixture::initialized_starter_input(),
    );
    p.initialize_initial_cast_source(&input, proof).unwrap();
    let mut w =
        NativeWorld::synthetic_component_test(p, [8; 16], synthetic_fixture::player(), 7).unwrap();
    for tick in 1..=5 {
        w.step(
            tick,
            None,
            &mut synthetic_event_ai_fixture::Authority::default(),
        )
        .unwrap();
        if !w.state.event_ai.as_ref().unwrap().history.is_empty() {
            break;
        }
    }
    let cp = w.checkpoint().unwrap();
    restore(w.profile(), cp.clone()).unwrap();
    let selected = w.profile.event_ai().unwrap();
    let event = selected.catalog().events(2).unwrap()[0];
    let event_ai::EventAiHistory::EnterCombat(e) =
        &mut w.state.event_ai.as_mut().unwrap().history[0]
    else {
        unreachable!()
    };
    assert!(e.actions[0].chance_counter >= 1);
    let action = &mut e.actions[0];
    action.chance_counter = 0;
    action.action_counter = Some(1);
    let mut holder = source::AggroTextHolder::new(event);
    holder.begin_enter_combat();
    holder
        .consume_chance(rng::full_u32(w.state.world, w.state.seed, 0))
        .unwrap();
    let selection = holder
        .consume_action(rng::full_u32(w.state.world, w.state.seed, 1))
        .unwrap();
    action.broadcast_id = Some(selection.text_id);
    action.text_slot = Some(selection.text_slot);
    action.speech =
        Some(EventAiState::speech(selected, &e.callback, e.occurrence, selection, None).unwrap());
    let mut anchor = event_ai::EventAiCallbackAnchor::default();
    anchor.append(e).unwrap();
    w.state.npcs.get_mut(&2).unwrap().events.event_ai_anchor = Some(anchor);
    let mut value = serde_json::to_value(cp).unwrap();
    value["state"] = serde_json::to_value(w.snapshot_of(&w.state)).unwrap();
    assert!(restore(w.profile(), resign(value)).is_err());
}
#[test]
fn loot_cannot_regenerate_its_receipt_inside_genesis_span() {
    let (p, input, quests) = synthetic_loot_fixture::setup();
    let (mut p, input, _, source) = fixture::prepare(p, input);
    p.initialize_initial_cast_source(&input, source).unwrap();
    p.initialize_quest_progression(&input, quests).unwrap();
    let mut w =
        NativeWorld::synthetic_component_test(p, [8; 16], synthetic_fixture::player(), 42).unwrap();
    let command = synthetic_fixture::cast(&w, 60001);
    let mut authority = synthetic_loot_fixture::LootAuthority::default();
    w.step(1, Some(command), &mut authority).unwrap();
    for tick in 2..=30 {
        w.step(tick, None, &mut authority).unwrap();
        if w.state.npcs[&2].health == 0 {
            break;
        }
    }
    let cp = w.checkpoint().unwrap();
    restore(w.profile(), cp.clone()).unwrap();
    let corpse = w.state.npcs[&2].id;
    let g = w
        .profile
        .quest_progression
        .as_ref()
        .unwrap()
        .loot
        .as_ref()
        .unwrap();
    let row = &g.creatures[&w.profile.npc(2).unwrap().entry];
    let mut probe = w.state.clone();
    probe.rng_counter = 0;
    let mut draws = vec![];
    let generated = g
        .catalog
        .generate(
            row.loot_id,
            row.minimum_money,
            row.maximum_money,
            &g.rules,
            &mut loot::RandomHost {
                state: &mut probe,
                corpse,
                draws: &mut draws,
            },
        )
        .unwrap();
    let receipt = w
        .state
        .npcs
        .get_mut(&2)
        .unwrap()
        .corpse_loot
        .as_mut()
        .unwrap();
    assert!(receipt.first_counter >= 1);
    receipt.first_counter = 0;
    receipt.end_counter = probe.rng_counter;
    receipt.draws = draws;
    receipt.generated = generated;
    assert!(w.validate_loot_state().is_err());
    let mut value = serde_json::to_value(cp).unwrap();
    value["state"] = serde_json::to_value(w.snapshot_of(&w.state)).unwrap();
    assert!(restore(w.profile(), resign(value)).is_err());
}

#[test]
fn oversized_prepared_input_is_bounded_before_replay_and_preserves_profile() {
    let (mut p, mut input, prepared, source) = fixture::inputs();
    let original = p.clone();
    let row = input.spells[0].clone();
    input.spells.resize(257, row);
    assert_eq!(
        p.initialize_player_prepared(&input, &prepared, synthetic_fixture::float(100.0), 1),
        Err(NativeError::TooLarge)
    );
    assert_eq!(p, original);
    assert_eq!(
        p.initialize_initial_cast_source(&input, source),
        Err(NativeError::TooLarge)
    );
    assert_eq!(p, original);
}

#[test]
fn later_catalog_cannot_introduce_unsealed_rank_or_group_observations() {
    for rank in [false, true] {
        let (p, input, mut spells) = synthetic_spell_fixture::inputs();
        let (mut p, input, prepared, mut source) = fixture::prepare(p, input);
        spells.initial_cast_dependencies = source.dependencies.clone();
        spells.modifier_spell_ids = SpellSourceRows::Complete(vec![81, 133, 3050]);
        p.initialize_initial_cast_source(&input, source.clone())
            .unwrap();
        let original = p.clone();
        let mut hooks = synthetic_spell_fixture::hooks();
        if rank {
            hooks.spell_rank_chain = SpellSourceRows::Complete(vec![
                SpellRankSource {
                    spell_id: 133,
                    previous_spell: 0,
                    first_spell: 133,
                    rank: 1,
                    required_spell: 0,
                },
                SpellRankSource {
                    spell_id: 143,
                    previous_spell: 133,
                    first_spell: 133,
                    rank: 2,
                    required_spell: 0,
                },
            ]);
            spells.modifier_spell_ids = SpellSourceRows::Complete(vec![81, 133, 143, 3050]);
        } else {
            hooks.spell_group_membership = SpellSourceRows::Complete(vec![SpellGroupSource {
                id: 1,
                rule: 1,
                name: "authored singleton".into(),
                spell_ids: vec![133],
            }]);
        }
        spells.modifier_hooks = hooks.clone();
        for spell in &mut spells.spells {
            spell.hooks = hooks.clone();
        }
        assert_eq!(
            p.initialize_spell_authority(&input, spells.clone()),
            Err(NativeError::AuthorityBinding)
        );
        assert_eq!(p, original);
        // The same observations succeed when genuinely retained before genesis.
        source.hooks = hooks;
        if rank {
            let SpellSourceRows::Complete(ids) = &mut source.queried_spell_ids else {
                unreachable!()
            };
            ids.push(143);
            ids.sort_unstable();
        }
        let mut numeric = prepared.dependencies().clone();
        numeric.hook_scope_identity = source.hook_scope_identity();
        let prepared = init::prepare_starter(&input, &numeric, prepared.order()).unwrap();
        p.initialize_player_prepared(&input, &prepared, synthetic_fixture::float(100.0), 1)
            .unwrap();
        p.initialize_initial_cast_source(&input, source).unwrap();
        p.initialize_spell_authority(&input, spells).unwrap();
    }
}
