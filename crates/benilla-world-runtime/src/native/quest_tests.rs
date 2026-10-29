// SPDX-License-Identifier: GPL-3.0-or-later
use super::synthetic_fixture::{float, player, point, real};
use super::synthetic_quest_fixture::*;
use super::*;
use benilla_sim::{
    player_progression::{self as pg, kill_xp as kill},
    quest_lifecycle as q,
};
#[test]
fn accept_authoritative_kill_turnin_levels_and_exactly_once_resume_replay() {
    let mut w = world();
    let genesis = w.checkpoint().unwrap();
    let mut a = QuestAuthority::default();
    accept(&mut w, &mut a);
    kill_target(&mut w, &mut a);
    let s = w.state.quest_progression.as_ref().unwrap();
    assert_eq!(s.player.view().level, 2);
    assert_eq!(s.player.view().xp, 10);
    assert_eq!(
        s.quests.progress(70001).unwrap().status,
        q::Status::Complete
    );
    assert!(!s.quests.progress(70001).unwrap().rewarded);
    let kill_events = &w.last_tick().unwrap().events;
    let xp=kill_events.iter().position(|e|matches!(e,Event::QuestProgression(QuestProgressionEvent::KillExperience(observation)) if matches!(observation.event, kill::Event::KillXpLog { .. }))).unwrap();
    let credit = kill_events
        .iter()
        .position(|e| {
            matches!(
                e,
                Event::QuestProgression(QuestProgressionEvent::Quest {
                    event: q::Event::KillCredit { .. },
                    ..
                })
            )
        })
        .unwrap();
    assert!(xp < credit);
    let i = command(
        &w,
        Command::TurnInQuest {
            quest: 70001,
            giver: w.state.npcs[&3].id,
        },
    );
    w.step(w.state.tick + 1, Some(i), &mut a).unwrap();
    let s = w.state.quest_progression.as_ref().unwrap();
    assert_eq!(s.player.view().level, 3);
    assert_eq!(s.player.view().xp, 30);
    assert_eq!(s.money, 18);
    assert!(s.quests.progress(70001).unwrap().rewarded);
    assert_eq!(w.state.combat.as_ref().unwrap().skills[&8].maximum, 15);
    assert_eq!(w.state.combat.as_ref().unwrap().recipe.snapshot().level, 3);
    assert_eq!(
        w.ability(60001).and_then(|a| match &a.admission {
            AbilityAdmission::Available(s) => Some(s.spell_rank),
            _ => None,
        }),
        Some(15)
    );
    let mut resumed = NativeWorld::restore_synthetic_component_test(
        w.profile.clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap();
    let i = command(
        &resumed,
        Command::TurnInQuest {
            quest: 70001,
            giver: resumed.state.npcs[&3].id,
        },
    );
    resumed
        .step(resumed.state.tick + 1, Some(i), &mut a)
        .unwrap();
    assert_eq!(resumed.state.quest_progression.as_ref().unwrap().money, 18);
    assert!(resumed.last_tick().unwrap().events.iter().any(|e| matches!(
        e,
        Event::Rejected(Rejection::Quest(q::Gate::AlreadyRewarded))
    )));
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), genesis).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
}
#[test]
fn interaction_authority_generation_distance_and_loot_are_not_assumed() {
    let mut w = world();
    let mut a = QuestAuthority {
        deny: true,
        ..Default::default()
    };
    accept(&mut w, &mut a);
    assert!(w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .quests
        .progress(70001)
        .is_none());
    let before = w.checkpoint().unwrap();
    a.deny = false;
    a.wrong = true;
    let i = command(
        &w,
        Command::AcceptQuest {
            quest: 70001,
            giver: w.state.npcs[&3].id,
        },
    );
    assert_eq!(
        w.step(w.state.tick + 1, Some(i), &mut a).unwrap_err(),
        NativeError::AuthorityBinding
    );
    assert_eq!(before, w.checkpoint().unwrap());
    let (mut p, input, mut data) = setup();
    data.kill.audit.native_loot_or_money = pg::Evidence::Present;
    assert!(matches!(
        p.initialize_quest_progression(&input, data),
        Err(NativeError::UnsupportedMechanic(_))
    ));
    assert!(
        !ai::within_interaction_distance(point(0.0), point(8.0), real(1.5), real(1.5)).unwrap()
    );
    assert!(
        ai::within_interaction_distance(point(0.0), point(7.999), real(1.5), real(1.5)).unwrap()
    );
}
#[test]
fn missing_level_rolls_back_whole_reward_and_source_callback_journals() {
    let (mut p, input, mut data) = setup();
    data.quests[0].reward.xp.reward_money_max_level = 6000;
    p.initialize_quest_progression(&input, data).unwrap();
    let mut w = NativeWorld::synthetic_component_test(p, [1; 16], player(), 42).unwrap();
    let mut a = QuestAuthority::default();
    accept(&mut w, &mut a);
    kill_target(&mut w, &mut a);
    let before = w.checkpoint().unwrap();
    let records = w.records().len();
    let i = command(
        &w,
        Command::TurnInQuest {
            quest: 70001,
            giver: w.state.npcs[&3].id,
        },
    );
    assert!(matches!(
        w.step(w.state.tick + 1, Some(i), &mut a),
        Err(NativeError::Quest(q::Error::Host(_)))
    ));
    assert_eq!(before, w.checkpoint().unwrap());
    assert_eq!(records, w.records().len());
    assert!(
        !w.state
            .quest_progression
            .as_ref()
            .unwrap()
            .quests
            .progress(70001)
            .unwrap()
            .rewarded
    );
}
#[test]
fn post_level_durability_preserves_callback_caches_and_restores_rebased_recipe() {
    let (mut p, input, mut data) = setup();
    let mut attacker = p.npcs[0].clone();
    attacker.ai.creature_id = 4;
    attacker.entry = 60004;
    attacker.max_health = 3000;
    p.npcs.push(attacker);
    let mut row = data.kill.creatures[0].clone();
    row.entry = 60004;
    row.reputation = kill::ReputationAdmission::ProvenAbsentOnKillRow { entry: 60004 };
    data.kill.creatures.push(row);
    p.initialize_quest_progression(&input, data).unwrap();
    let mut w = NativeWorld::synthetic_component_test(p, [1; 16], player(), 42).unwrap();
    let initial = w.checkpoint().unwrap();
    let mut a = QuestAuthority::default();
    accept(&mut w, &mut a);
    kill_target(&mut w, &mut a);
    let i = command(
        &w,
        Command::TurnInQuest {
            quest: 70001,
            giver: w.state.npcs[&3].id,
        },
    );
    w.step(w.state.tick + 1, Some(i), &mut a).unwrap();
    let before = w.state.combat.as_ref().unwrap().stats.clone();
    let i = command(
        &w,
        Command::Cast {
            ability: 60001,
            target: w.state.npcs[&4].id,
        },
    );
    w.step(w.state.tick + 1, Some(i), &mut a).unwrap();
    for _ in 0..40 {
        if w.state
            .combat
            .as_ref()
            .unwrap()
            .equipment
            .item(4)
            .unwrap()
            .is_broken()
        {
            break;
        }
        w.step(w.state.tick + 1, None, &mut a).unwrap();
    }
    let c = w.state.combat.as_ref().unwrap();
    assert!(c.equipment.item(4).unwrap().is_broken());
    assert_eq!(c.recipe.snapshot().level, 3);
    assert!(c.stats.max_mana < before.max_mana);
    assert_ne!(c.stats.crit_percent, before.crit_percent);
    assert!(w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .derived_calls
        .iter()
        .any(|e| matches!(e, DerivedCall::ItemBreak { .. })));
    let restored = NativeWorld::restore_synthetic_component_test(
        w.profile.clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(restored.state_digest(), w.state_digest());
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
}
#[test]
fn preview_authority_is_read_only_and_rejects_stale_giver() {
    let w = world();
    let mut a = QuestAuthority::default();
    let before = w.checkpoint().unwrap();
    let giver = w.state.npcs[&3].id;
    assert_eq!(
        w.preview_quest_interaction(giver, 70001, false, &mut a)
            .unwrap(),
        None
    );
    assert_eq!(w.checkpoint().unwrap(), before);
    assert!(w.records().is_empty());
    a.deny = true;
    assert_eq!(
        w.preview_quest_interaction(giver, 70001, false, &mut a)
            .unwrap(),
        Some(Rejection::QuestInteraction)
    );
    a.deny = false;
    a.wrong = true;
    assert_eq!(
        w.preview_quest_interaction(giver, 70001, false, &mut a),
        Err(NativeError::AuthorityBinding)
    );
    let stale = crate::NpcId {
        generation: giver.generation + 1,
        ..giver
    };
    assert_eq!(
        w.preview_quest_interaction(stale, 70001, false, &mut a)
            .unwrap(),
        Some(Rejection::UnknownTarget)
    );
    assert_eq!(w.checkpoint().unwrap(), before);
}
fn historical_world() -> NativeWorld {
    let (mut p, input, mut data) = setup();
    // Authored target survives the DoT; the independent original target grants XP.
    let mut other = p.npcs[0].clone();
    other.ai.creature_id = 4;
    other.entry = 60004;
    other.max_health = 1000;
    other.ai.spawn = point(2.0);
    other.ai.reaction = ai::Reaction::Defensive;
    p.npcs.push(other);
    let mut row = data.kill.creatures[0].clone();
    row.entry = 60004;
    row.reputation = kill::ReputationAdmission::ProvenAbsentOnKillRow { entry: 60004 };
    data.kill.creatures.push(row);
    // Scaling periodic value makes stale-impact rank observably incorrect.
    for ability in [&mut p.abilities[0], &mut data.abilities[0].ability] {
        if let AbilityAdmission::Available(s) = &mut ability.admission {
            if let spell::MageEffect::SchoolDamage(value) = &mut s.effects[0] {
                value.points_per_level = float(50.0);
            }
            if let spell::MageEffect::PeriodicDamage {
                value, duration_ms, ..
            } = &mut s.effects[1]
            {
                value.points_per_level = float(10.0);
                *duration_ms = 10000;
            }
        }
    }
    p.initialize_player(&input, float(100.0), 1).unwrap();
    p.initialize_quest_progression(&input, data).unwrap();
    NativeWorld::synthetic_component_test(p, [1; 16], player(), 42).unwrap()
}
fn cast_other(w: &mut NativeWorld, a: &mut QuestAuthority) {
    let i = command(
        w,
        Command::Cast {
            ability: 60001,
            target: w.state.npcs[&4].id,
        },
    );
    w.step(w.state.tick + 1, Some(i), a).unwrap();
}
#[test]
fn applied_aura_keeps_source_rank_and_level_through_xp_rebase_and_restore() {
    let mut w = historical_world();
    let genesis = w.checkpoint().unwrap();
    let mut a = QuestAuthority::default();
    accept(&mut w, &mut a);
    cast_other(&mut w, &mut a);
    for _ in 0..20 {
        if !w.state.npcs[&4].auras.is_empty() {
            break;
        }
        w.step(w.state.tick + 1, None, &mut a).unwrap();
    }
    let old = w.state.npcs[&4].aura_bindings[&60001].clone();
    assert_eq!(old.level, 1);
    kill_target(&mut w, &mut a);
    assert_eq!(w.player_level(), 2);
    assert_eq!(w.state.npcs[&4].aura_bindings[&60001], old);
    let cp = w.checkpoint().unwrap();
    let restored = NativeWorld::restore_synthetic_component_test(
        w.profile.clone(),
        Checkpoint::decode(&cp.encode().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(restored.state_digest(), w.state_digest());
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), genesis).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
    let mut forged = serde_json::to_value(cp).unwrap();
    forged["state"]["npcs"]["4"]["aura_bindings"]["60001"]["level"] = serde_json::json!(2);
    let state: super::persistence::StateCheckpoint =
        serde_json::from_value(forged["state"].clone()).unwrap();
    forged["digest"] =
        serde_json::to_value(crate::digest(b"benilla-native-state-v76\0", &state)).unwrap();
    let tip: crate::Digest = serde_json::from_value(forged["history_tip"].clone()).unwrap();
    let digest: crate::Digest = serde_json::from_value(forged["digest"].clone()).unwrap();
    forged["checksum"] = serde_json::to_value(crate::digest(
        b"benilla-native-checkpoint-v76\0",
        &(&state, digest, tip),
    ))
    .unwrap();
    assert!(NativeWorld::restore_synthetic_component_test(
        w.profile.clone(),
        serde_json::from_value(forged).unwrap()
    )
    .is_err());
}
#[test]
fn launched_payload_survives_reward_level_and_periodic_value_uses_impact_rank() {
    let mut w = historical_world();
    let mut a = QuestAuthority::default();
    accept(&mut w, &mut a);
    kill_target(&mut w, &mut a);
    assert_eq!(w.player_level(), 2);
    cast_other(&mut w, &mut a);
    for _ in 0..10 {
        if w.state
            .casts
            .values()
            .any(|c| matches!(c.execution.state(), spell::CastState::Traveling { .. }))
        {
            break;
        }
        w.step(w.state.tick + 1, None, &mut a).unwrap();
    }
    assert!(w
        .state
        .casts
        .values()
        .any(|c| matches!(c.execution.state(), spell::CastState::Traveling { .. })));
    let i = command(
        &w,
        Command::TurnInQuest {
            quest: 70001,
            giver: w.state.npcs[&3].id,
        },
    );
    w.step(w.state.tick + 1, Some(i), &mut a).unwrap();
    assert_eq!(w.player_level(), 3);
    let cp = w.checkpoint().unwrap();
    let mut restored = NativeWorld::restore_synthetic_component_test(
        w.profile.clone(),
        Checkpoint::decode(&cp.encode().unwrap()).unwrap(),
    )
    .unwrap();
    for _ in 0..15 {
        if !w.state.npcs[&4].auras.is_empty() {
            break;
        }
        w.step(w.state.tick + 1, None, &mut a).unwrap();
        restored
            .step(restored.state.tick + 1, None, &mut a)
            .unwrap();
    }
    assert_eq!(w.state.npcs[&4].aura_bindings[&60001].level, 3);
    assert!(w.last_tick().unwrap().events.iter().any(|e|matches!(e,Event::Spell{target,event:spell::SpellEvent::DirectDamage{amount,..},..} if target.spawn==4 && *amount<110)));
    assert!(w.state.npcs[&4].auras[&60001].aura().unwrap().amount >= 21);
    assert_eq!(restored.state_digest(), w.state_digest());
}
#[test]
fn reward_levels_during_prepare_without_replacing_spell_or_original_timer() {
    let mut w = release_world(false);
    let genesis = w.checkpoint().unwrap();
    let mut a = QuestAuthority::default();
    accept(&mut w, &mut a);
    kill_target(&mut w, &mut a);
    assert_eq!(w.player_level(), 2);
    assert!(w.ability(60005).is_none());
    cast_other(&mut w, &mut a);
    let prepared = w.state.casts.values().next().unwrap().clone();
    assert_eq!(prepared.binding.spec().unwrap().cast_ms, 350);
    assert_eq!(prepared.binding.spec().unwrap().mana_cost, 42);
    reward(&mut w, &mut a).unwrap();
    assert_eq!(w.player_level(), 3);
    assert!(w.ability(60005).is_some());
    let cast = &w.state.casts[&prepared.id];
    assert_eq!(cast.binding, prepared.binding);
    assert!(cast.release_binding.is_none());
    assert_eq!(
        cast.execution.state(),
        spell::CastState::Casting { remaining_ms: 250 }
    );
    let mut resumed = restore(&w);
    for _ in 0..10 {
        w.step(w.state.tick + 1, None, &mut a).unwrap();
        resumed.step(resumed.state.tick + 1, None, &mut a).unwrap();
        assert_eq!(w.state_digest(), resumed.state_digest());
        if matches!(
            w.state.casts[&prepared.id].execution.state(),
            spell::CastState::Traveling { .. }
        ) {
            break;
        }
    }
    let cast = &w.state.casts[&prepared.id];
    let released = cast.release_binding.as_ref().unwrap();
    assert_eq!(cast.ability, 60001);
    assert_eq!(cast.binding, prepared.binding);
    assert_eq!(released.ability.id, 60001);
    assert_eq!(released.spec().unwrap().spell_rank, 15);
    assert_eq!(released.spec().unwrap().mana_cost, 45);
    assert_eq!(released.spec().unwrap().cast_ms, 300);
    assert_eq!(released.captured_ms - prepared.binding.captured_ms, 400);
    assert!(w
        .last_tick()
        .unwrap()
        .draws
        .iter()
        .any(|d| d.cast_id == prepared.id
            && d.purpose == DrawPurpose::Direct { slot: 0 }
            && d.high == 10));
    assert_eq!(restore(&w).state_digest(), w.state_digest());
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), genesis).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
}
fn release_world(expensive: bool) -> NativeWorld {
    NativeWorld::synthetic_component_test(release_profile(expensive), [1; 16], player(), 42)
        .unwrap()
}
fn restore(w: &NativeWorld) -> NativeWorld {
    NativeWorld::restore_synthetic_component_test(
        w.profile.clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap()
}
fn reward(w: &mut NativeWorld, a: &mut QuestAuthority) -> Result<(), NativeError> {
    let i = command(
        w,
        Command::TurnInQuest {
            quest: 70001,
            giver: w.state.npcs[&3].id,
        },
    );
    w.step(w.state.tick + 1, Some(i), a).map(|_| ())
}
#[test]
fn release_no_power_keeps_committed_reward_and_gcd_without_cast_rng_or_cooldown() {
    let mut w = release_world(true);
    let mut a = QuestAuthority::default();
    accept(&mut w, &mut a);
    kill_target(&mut w, &mut a);
    cast_other(&mut w, &mut a);
    let id = *w.state.casts.keys().next().unwrap();
    reward(&mut w, &mut a).unwrap();
    assert_eq!(w.player_level(), 3);
    assert!(
        w.state
            .quest_progression
            .as_ref()
            .unwrap()
            .quests
            .progress(70001)
            .unwrap()
            .rewarded
    );
    let mana = w.state.resources.caster.mana;
    let gcd = w.state.resources.caster.global_ready_at(9);
    let cooldown = w.state.resources.caster.spell_ready_at(60001);
    assert!(mana < 445);
    for _ in 0..10 {
        if !w.state.casts.contains_key(&id) {
            break;
        }
        w.step(w.state.tick + 1, None, &mut a).unwrap();
        assert!(!w.last_tick().unwrap().draws.iter().any(|d| d.cast_id == id));
    }
    assert!(w.last_tick().unwrap().events.iter().any(|event| matches!(event, Event::Spell { cast_id, event: spell::SpellEvent::Failed(spell::CastFailure::NoPower), .. } if *cast_id == id)));
    assert_eq!(w.state.resources.caster.mana, mana);
    assert_eq!(w.state.resources.caster.global_ready_at(9), gcd);
    assert_eq!(w.state.resources.caster.spell_ready_at(60001), cooldown);
    assert_eq!(restore(&w).state_digest(), w.state_digest());
}
#[test]
fn cancellation_after_level_uses_prepared_gcd_without_spend_or_dice() {
    let mut w = release_world(false);
    let mut a = QuestAuthority::default();
    accept(&mut w, &mut a);
    kill_target(&mut w, &mut a);
    cast_other(&mut w, &mut a);
    let id = *w.state.casts.keys().next().unwrap();
    reward(&mut w, &mut a).unwrap();
    let mana = w.state.resources.caster.mana;
    let i = command(
        &w,
        Command::CancelCast {
            spell_id: 60001,
            target: CancelCastTarget::Instance(id),
        },
    );
    w.step(w.state.tick + 1, Some(i), &mut a).unwrap();
    assert!(w.state.casts.is_empty());
    assert_eq!(w.state.resources.caster.mana, mana);
    assert_eq!(w.state.resources.caster.global_ready_at(9), None);
    assert!(!w.last_tick().unwrap().draws.iter().any(|d| d.cast_id == id));
    assert_eq!(w.player_level(), 3);
}
#[test]
fn restore_rejects_historical_binding_on_unreleased_cast() {
    let mut w = historical_world();
    let mut a = QuestAuthority::default();
    accept(&mut w, &mut a);
    kill_target(&mut w, &mut a);
    let mut old = ExecutionBinding::capture(&w.profile, &w.state, 60001).unwrap();
    let i = command(
        &w,
        Command::TurnInQuest {
            quest: 70001,
            giver: w.state.npcs[&3].id,
        },
    );
    w.step(w.state.tick + 1, Some(i), &mut a).unwrap();
    cast_other(&mut w, &mut a);
    NativeWorld::restore_synthetic_component_test(w.profile.clone(), w.checkpoint().unwrap())
        .unwrap();
    let cast = w.state.casts.values_mut().next().unwrap();
    assert!(matches!(
        cast.execution.state(),
        spell::CastState::Casting { .. }
    ));
    assert_ne!(old.ability, cast.binding.ability);
    old.captured_ms = cast.binding.captured_ms;
    cast.binding = old;
    // Rechecksumming a valid historical context cannot replace the exact prepare witness.
    let cp = Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap();
    assert_eq!(
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), cp).err(),
        Some(NativeError::InvalidCheckpoint)
    );
}
#[test]
fn historical_binding_rejects_future_skill_cap_even_below_current_cap() {
    let mut w = historical_world();
    let mut a = QuestAuthority::default();
    let old = ExecutionBinding::capture(&w.profile, &w.state, 60001).unwrap();
    kill_target(&mut w, &mut a);
    assert_eq!(w.player_level(), 2);
    old.validate(
        &w.profile,
        w.state.quest_progression.as_ref(),
        w.state.now_ms,
        (
            w.profile.digest(),
            w.state.domain,
            w.state.world,
            w.state.player,
        ),
    )
    .unwrap();
    let mut forged = serde_json::to_value(old).unwrap();
    let skills = forged["progression"]["player"]["skills"]
        .as_array_mut()
        .unwrap();
    let magic = skills.iter_mut().find(|s| s["id"] == 8).unwrap();
    magic["value"] = serde_json::json!(10);
    magic["maximum"] = serde_json::json!(10);
    forged["ability"]["admission"]["Available"]["spell_rank"] = serde_json::json!(10);
    let forged: ExecutionBinding = serde_json::from_value(forged).unwrap();
    assert_eq!(
        forged.validate(
            &w.profile,
            w.state.quest_progression.as_ref(),
            w.state.now_ms,
            (
                w.profile.digest(),
                w.state.domain,
                w.state.world,
                w.state.player
            ),
        ),
        Err(NativeError::InvalidCheckpoint)
    );
}

#[test]
fn future_active_recipes_obey_the_same_admission_as_genesis() {
    for bad in 0..3 {
        let (mut p, input, mut data) = learned_setup();
        let ability = &mut data.abilities[2].ability;
        let AbilityAdmission::Available(spec) = &mut ability.admission else {
            unreachable!()
        };
        match bad {
            0 => spec.spell_id = 60006,
            1 => spec.damage_pushback = false,
            _ => {
                let spell::MageEffect::PeriodicDamage { amplitude_ms, .. } = &mut spec.effects[1]
                else {
                    unreachable!()
                };
                *amplitude_ms = 1;
            }
        }
        assert!(p.initialize_quest_progression(&input, data).is_err());
    }
}
#[test]
fn newly_learned_cast_uses_bound_pushback_and_raw_damage_interrupt_flags() {
    for cancel in [false, true] {
        let (mut p, mut input, mut data) = learned_setup();
        if cancel {
            input
                .spells
                .iter_mut()
                .find(|s| s.id == 60005)
                .unwrap()
                .words[21] |= 0x10;
        }
        let mut other = p.npcs[0].clone();
        other.ai.creature_id = 4;
        other.entry = 60004;
        other.max_health = 3000;
        other.ai.spawn = point(2.0);
        other.ai.reaction = ai::Reaction::Defensive;
        p.npcs.push(other);
        let mut row = data.kill.creatures[0].clone();
        row.entry = 60004;
        row.reputation = kill::ReputationAdmission::ProvenAbsentOnKillRow { entry: 60004 };
        data.kill.creatures.push(row);
        p.initialize_player(&input, float(100.0), 1).unwrap();
        p.initialize_quest_progression(&input, data).unwrap();
        let mut w = NativeWorld::synthetic_component_test(p, [1; 16], player(), 42).unwrap();
        let genesis = w.checkpoint().unwrap();
        let mut a = QuestAuthority::default();
        accept(&mut w, &mut a);
        kill_target(&mut w, &mut a);
        assert!(w.profile.ability(60005).is_none());
        assert!(w.ability(60005).is_some());
        cast_other(&mut w, &mut a);
        for _ in 0..20 {
            w.step(w.state.tick + 1, None, &mut a).unwrap();
        }
        let i = command(
            &w,
            Command::Cast {
                ability: 60005,
                target: w.state.npcs[&4].id,
            },
        );
        w.step(w.state.tick + 1, Some(i), &mut a).unwrap();
        let saw_expected = |w: &NativeWorld| {
            w.last_tick().unwrap().events.iter().any(|e|matches!(e,
            Event::Spell{ability:60005,event:spell::SpellEvent::Pushback(_),..} if !cancel)
            || matches!(e,Event::Spell{ability:60005,event:spell::SpellEvent::Failed(spell::CastFailure::Interrupted),..} if cancel))
        };
        let mut observed = saw_expected(&w);
        assert!(observed || w.state.casts.values().any(|c| c.ability == 60005));
        for _ in 0..40 {
            if observed {
                break;
            }
            w.step(w.state.tick + 1, None, &mut a).unwrap();
            observed = saw_expected(&w);
        }
        assert!(observed, "cancel={cancel}");
        let restored = NativeWorld::restore_synthetic_component_test(
            w.profile.clone(),
            w.checkpoint().unwrap(),
        )
        .unwrap();
        assert_eq!(restored.state_digest(), w.state_digest());
        let mut replay =
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), genesis).unwrap();
        replay.replay(w.records()).unwrap();
        assert_eq!(replay.state_digest(), w.state_digest());
    }
}

#[test]
fn phase_witness_tampering_and_valid_but_wrong_release_clock_reject() {
    let mut w = release_world(false);
    let mut a = QuestAuthority::default();
    accept(&mut w, &mut a);
    kill_target(&mut w, &mut a);
    cast_other(&mut w, &mut a);
    let id = *w.state.casts.keys().next().unwrap();
    reward(&mut w, &mut a).unwrap();
    let mut forged = w.clone();
    let released = ExecutionBinding::capture(&w.profile, &w.state, 60001).unwrap();
    forged.state.casts.get_mut(&id).unwrap().release_binding = Some(released);
    assert!(NativeWorld::restore_synthetic_component_test(
        forged.profile.clone(),
        forged.checkpoint().unwrap()
    )
    .is_err());
    for _ in 0..10 {
        if matches!(
            w.state.casts[&id].execution.state(),
            spell::CastState::Traveling { .. }
        ) {
            break;
        }
        w.step(w.state.tick + 1, None, &mut a).unwrap();
    }
    assert!(matches!(
        w.state.casts[&id].execution.state(),
        spell::CastState::Traveling { .. }
    ));
    let reject = |world: NativeWorld| {
        assert!(NativeWorld::restore_synthetic_component_test(
            world.profile.clone(),
            world.checkpoint().unwrap()
        )
        .is_err());
    };
    let mut forged = w.clone();
    forged.state.casts.get_mut(&id).unwrap().release_binding = None;
    reject(forged);
    let mut forged = w.clone();
    let cast = forged.state.casts.get_mut(&id).unwrap();
    cast.release_binding = Some(cast.binding.clone());
    reject(forged);
    let mut forged = w.clone();
    let cast = forged.state.casts.get_mut(&id).unwrap();
    let released = cast.release_binding.as_mut().unwrap();
    if let AbilityAdmission::Available(spec) = &mut released.ability.admission {
        spec.mana_cost += 1;
    }
    reject(forged);
    let mut forged = w.clone();
    let cast = forged.state.casts.get_mut(&id).unwrap();
    let released = cast.release_binding.as_mut().unwrap();
    released.captured_ms -= 100;
    // Both witnesses remain independently source-valid, and the pair is allowed.
    // Only the exact release clock must detect this otherwise coherent forgery.
    released
        .validate(
            &w.profile,
            w.state.quest_progression.as_ref(),
            w.state.now_ms,
            (
                w.profile.digest(),
                w.state.domain,
                w.state.world,
                w.state.player,
            ),
        )
        .unwrap();
    released.release_context(&cast.binding).unwrap();
    reject(forged);
}
#[test]
fn missing_level_source_during_prepare_rolls_back_reward_without_consuming_anything() {
    let (mut p, input, mut data) = release_setup(false);
    data.quests[0].reward.xp.reward_money_max_level = 6000;
    p.initialize_quest_progression(&input, data).unwrap();
    let mut w = NativeWorld::synthetic_component_test(p, [1; 16], player(), 42).unwrap();
    let mut a = QuestAuthority::default();
    accept(&mut w, &mut a);
    kill_target(&mut w, &mut a);
    cast_other(&mut w, &mut a);
    let before = w.checkpoint().unwrap();
    let records = w.records().to_vec();
    assert!(reward(&mut w, &mut a).is_err());
    assert_eq!(w.checkpoint().unwrap(), before);
    assert_eq!(w.records(), records);
}
#[test]
fn older_projectile_kill_levels_while_new_cast_still_prepares() {
    let mut w = release_world(false);
    let genesis = w.checkpoint().unwrap();
    let mut a = QuestAuthority::default();
    let input = command(
        &w,
        Command::Cast {
            ability: 60001,
            target: w.state.npcs[&2].id,
        },
    );
    w.step(1, Some(input), &mut a).unwrap();
    for _ in 0..10 {
        if matches!(
            w.state.casts[&1].execution.state(),
            spell::CastState::Traveling { .. }
        ) {
            break;
        }
        w.step(w.state.tick + 1, None, &mut a).unwrap();
    }
    let spell::CastState::Traveling { impact_at_ms } = w.state.casts[&1].execution.state() else {
        panic!("first projectile did not launch")
    };
    for _ in 0..10 {
        if w.state.now_ms + 300 >= impact_at_ms {
            break;
        }
        w.step(w.state.tick + 1, None, &mut a).unwrap();
    }
    cast_other(&mut w, &mut a);
    let prepared = w.state.casts[&2].binding.clone();
    assert_eq!(prepared.level, 1);
    for _ in 0..10 {
        if w.player_level() == 2 {
            break;
        }
        w.step(w.state.tick + 1, None, &mut a).unwrap();
    }
    assert_eq!(w.player_level(), 2);
    assert!(matches!(
        w.state.casts[&2].execution.state(),
        spell::CastState::Casting { .. }
    ));
    assert_eq!(w.state.casts[&2].binding, prepared);
    assert_eq!(restore(&w).state_digest(), w.state_digest());
    for _ in 0..10 {
        if matches!(
            w.state.casts[&2].execution.state(),
            spell::CastState::Traveling { .. }
        ) {
            break;
        }
        w.step(w.state.tick + 1, None, &mut a).unwrap();
    }
    assert_eq!(w.state.casts[&2].release_binding.as_ref().unwrap().level, 2);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), genesis).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
}

#[path = "quest_abandon_tests.rs"]
mod quest_abandon_tests;
