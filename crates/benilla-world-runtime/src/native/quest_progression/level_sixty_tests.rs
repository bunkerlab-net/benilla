// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored complete rows and real native progression hosts; no production capability.
use super::*;
use crate::native::synthetic_fixture::{float, player};
use crate::native::synthetic_quest_fixture::{self as fixture, command, QuestAuthority};

fn profile(maximize: bool, rate: u32) -> Profile {
    let (mut p, input, rows) = fixture::setup_with_levels(60, 60, maximize, rate);
    p.initialize_quest_progression(&input, rows).unwrap();
    p.admin = Some(AdminPolicy {
        world: [1; 16],
        world_owner: player().owner,
        allowlist: Default::default(),
        revision: 7,
    });
    p
}
fn world(maximize: bool, rate: u32) -> NativeWorld {
    NativeWorld::synthetic_component_test(profile(maximize, rate), [1; 16], player(), 42).unwrap()
}
pub(super) fn admin(w: &mut NativeWorld, operation: AdminOperation) {
    let mut key = [0; 16];
    key[..8].copy_from_slice(&(w.state.next_sequence + 1).to_le_bytes());
    let request = AdminRequest {
        key,
        world: w.state.world,
        player: w.state.player,
        policy_revision: 7,
        operation,
    };
    let input = command(w, Command::Admin(request));
    w.step(
        w.state.tick + 1,
        Some(input),
        &mut QuestAuthority::default(),
    )
    .unwrap();
}
fn gain(w: &mut NativeWorld) {
    admin(w, AdminOperation::GainOneLevel);
}
fn reload(w: &NativeWorld) -> NativeWorld {
    NativeWorld::restore_synthetic_component_test(
        w.profile.clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap()
}
pub(super) fn reach(w: &mut NativeWorld, level: u32) {
    while u32::from(w.player_level()) < level {
        gain(w);
        if w.player_level().is_multiple_of(10) {
            let cp = w.checkpoint().unwrap();
            w.checkpoint_committed(&cp).unwrap();
        }
    }
}

#[test]
fn native_unspent_talents_admin_boundaries_and_reload_through_sixty() {
    for maximize in [false, true] {
        for rate in [0, 0x3f000000, 0x3f800000] {
            let mut w = world(maximize, rate);
            for level in [9, 10, 11, 59, 60] {
                reach(&mut w, level);
                let q = w.state.quest_progression.as_ref().unwrap();
                let p = q.player.view();
                let expected = w
                    .profile
                    .quest_progression
                    .as_ref()
                    .unwrap()
                    .levels
                    .rules()
                    .unspent_talent_points(level)
                    .unwrap();
                assert_eq!(
                    (p.level, p.free_talent_points, p.used_talent_points),
                    (level, expected, 0)
                );
                assert_eq!(
                    w.state.combat.as_ref().unwrap().recipe.snapshot().level,
                    level
                );
                assert_eq!(p.skills[0].maximum, (level * 5) as u16);
                assert_eq!(p.xp, 0, "direct source GiveLevel preserves existing XP");
                let encoded = w.checkpoint().unwrap().encode().unwrap();
                assert!(encoded.len() < MAX_BYTES / 2);
                let restored = reload(&w);
                assert_eq!(restored.state_digest(), w.state_digest());
                w = restored;
                admin(&mut w, AdminOperation::SetXpGain { enabled: false });
                assert!(!w.state.quest_progression.as_ref().unwrap().xp_enabled);
                let restored = reload(&w);
                assert_eq!(restored.state_digest(), w.state_digest());
                w = restored;
                admin(&mut w, AdminOperation::SetXpGain { enabled: true });
            }
            let before = w
                .state
                .quest_progression
                .as_ref()
                .unwrap()
                .player
                .snapshot();
            gain(&mut w);
            let mut after = w
                .state
                .quest_progression
                .as_ref()
                .unwrap()
                .player
                .snapshot();
            // A rejected +1 still advances the ordinary simulation clock.
            after.played_time_level = before.played_time_level;
            assert_eq!(after, before);
            assert_eq!(
                w.admin_snapshot(w.state.player.owner)
                    .unwrap()
                    .supported_maximum,
                60
            );
        }
    }
}

#[test]
fn cap_sixty_quest_money_and_kill_xp_keep_source_semantics() {
    let mut w = world(true, 0x3f800000);
    reach(&mut w, 60);
    let mut a = QuestAuthority::default();
    fixture::accept(&mut w, &mut a);
    fixture::kill_target(&mut w, &mut a);
    let before = w.state.quest_progression.as_ref().unwrap().player.clone();
    let input = command(
        &w,
        Command::TurnInQuest {
            quest: 70001,
            giver: w.state.npcs[&3].id,
        },
    );
    w.step(w.state.tick + 1, Some(input), &mut a).unwrap();
    let q = w.state.quest_progression.as_ref().unwrap();
    assert_eq!(q.player.view().level, 60);
    assert_eq!(q.player.view().xp, before.view().xp);
    assert_eq!(q.player.view().free_talent_points, 51);
    assert_eq!(
        q.money, 78,
        "11 initial +7 ordinary +60 max-level conversion"
    );
    assert_eq!(reload(&w).state_digest(), w.state_digest());
}

#[test]
fn active_preparation_keeps_its_prefix_across_nine_ten_and_fifty_nine_sixty() {
    for start in [9, 59] {
        let (mut p, input, mut rows) = fixture::setup_with_levels(60, 60, true, 0x3f800000);
        for ability in &mut p.abilities {
            if let AbilityAdmission::Available(spec) = &mut ability.admission {
                spec.cast_ms = 1000;
            }
        }
        for recipe in &mut rows.abilities {
            if let AbilityAdmission::Available(spec) = &mut recipe.ability.admission {
                spec.cast_ms = 1000;
            }
            if let Some(time) = &mut recipe.cast_time {
                time.base_ms = 1000;
                time.minimum_ms = 1000;
            }
        }
        p.initialize_player(&input, float(100.0), 1).unwrap();
        p.initialize_quest_progression(&input, rows).unwrap();
        p.admin = Some(AdminPolicy {
            world: [1; 16],
            world_owner: player().owner,
            allowlist: Default::default(),
            revision: 7,
        });
        let mut w = NativeWorld::synthetic_component_test(p, [1; 16], player(), 42).unwrap();
        reach(&mut w, start);
        let command = command(
            &w,
            Command::Cast {
                ability: 60001,
                target: w.state.npcs[&2].id,
            },
        );
        w.step(
            w.state.tick + 1,
            Some(command),
            &mut QuestAuthority::default(),
        )
        .unwrap();
        let id = *w.state.casts.keys().next().unwrap();
        let prepared = w.state.casts[&id].binding.clone();
        gain(&mut w);
        assert_eq!(w.state.casts[&id].binding, prepared);
        assert_eq!(prepared.level, start as u8);
        assert_eq!(w.player_level(), start as u8 + 1);
        let restored = reload(&w);
        assert_eq!(restored.state.casts[&id].binding, prepared);
        assert_eq!(restored.state_digest(), w.state_digest());
    }
}

#[test]
fn sixty_rows_require_complete_learning_and_capacity_at_genesis() {
    let (mut p, mut input, mut rows) = fixture::setup_with_levels(60, 60, true, 0x3f800000);
    let mut extra = rows.abilities[0].clone();
    for id in 60100..60165 {
        extra.ability.id = id;
        rows.abilities.push(extra.clone());
    }
    assert!(p.initialize_quest_progression(&input, rows).is_err());
    let (mut p, original, rows) = fixture::setup_with_levels(60, 60, true, 0x3f800000);
    input = original;
    // A later threshold is a real raw learning row; no missing row may be
    // treated as an empty lookup just because it is unreachable at level one.
    let init::Rows::Complete(abilities) = &mut input.skill_abilities else {
        unreachable!()
    };
    abilities.push(init::SkillAbilityRow {
        row_id: 999,
        skill: 8,
        spell: 60999,
        forward_spell_id: 0,
        race_mask: 0,
        class_mask: 0,
        required_value: 300,
        learn_on_get_skill: 1,
        max_value: 0,
    });
    assert!(p.initialize_quest_progression(&input, rows).is_err());
}

#[test]
fn defense_sensitive_future_learning_is_not_hidden_by_level_only_probe() {
    let (mut p, mut input, rows) = fixture::setup_with_levels(60, 60, false, 0x3f800000);
    let init::Rows::Complete(abilities) = &mut input.skill_abilities else {
        unreachable!()
    };
    abilities.push(init::SkillAbilityRow {
        row_id: 999,
        skill: 95,
        spell: 60001,
        forward_spell_id: 0,
        race_mask: 0,
        class_mask: 0,
        required_value: 300,
        learn_on_get_skill: 1,
        max_value: 0,
    });
    // Use an actually supplied but initially unknown active spell, so this row
    // does not fail the source removal branch before capacity admission.
    abilities.last_mut().unwrap().spell = 60999;
    let mut spell = input.spells[0].clone();
    spell.id = 60999;
    spell.words[0] = 60999;
    input.spells.push(spell);
    input.active_support.push(init::ActiveAbilitySupport {
        spell_id: 60999,
        unavailable_reason: Some("authored future active".into()),
    });
    // Rebind the exact changed source without learning the future spell, then
    // independently exercise the defense-sensitive capacity admission guard.
    p.initialize_player(&input, float(100.0), 1).unwrap();
    assert_eq!(
        p.initialize_quest_progression(&input, rows).unwrap_err(),
        NativeError::UnsupportedMechanic("unproved defense-sensitive progression training")
    );
}

#[test]
fn multilevel_quest_xp_crosses_ten_and_cap_through_real_hosts() {
    let (mut p, input, mut rows) = fixture::setup_with_levels(60, 60, true, 0x3f800000);
    rows.quest_xp_rate = arithmetic::Binary32Rate::from_bits(1000.0f32.to_bits()).unwrap();
    p.initialize_quest_progression(&input, rows).unwrap();
    let mut w = NativeWorld::synthetic_component_test(p, [1; 16], player(), 42).unwrap();
    let mut a = QuestAuthority::default();
    fixture::accept(&mut w, &mut a);
    fixture::kill_target(&mut w, &mut a);
    assert_eq!(w.player_level(), 2);
    let cp = w.checkpoint().unwrap();
    w.checkpoint_committed(&cp).unwrap();
    let input = command(
        &w,
        Command::TurnInQuest {
            quest: 70001,
            giver: w.state.npcs[&3].id,
        },
    );
    w.step(w.state.tick + 1, Some(input), &mut a).unwrap();
    let q = w.state.quest_progression.as_ref().unwrap();
    assert_eq!(
        (q.player.view().level, q.player.view().free_talent_points),
        (60, 51)
    );
    // Initial kill 50 + ordinary quest 100 * captured rate 1000, then thresholds
    // 40*(1+...+59). Surplus is retained by the source at the configured cap.
    assert_eq!(q.player.view().xp, 50 + 100_000 - 40 * (59 * 60 / 2));
    // RewardQuest chooses XP rather than cap money BEFORE GiveXP; reaching 60
    // inside that callback does not retroactively award the extra 60 money.
    assert_eq!(q.money, 18);
    assert!(w.last_tick().unwrap().events.iter().any(|event| matches!(
        event,
        Event::QuestProgression(QuestProgressionEvent::Quest {
            event: q::Event::RewardAnnouncement {
                quest: 70001,
                xp: 100_000
            },
            level: 60,
            money: 18,
        })
    )));
    let level_calls: Vec<_> = q
        .derived_calls
        .iter()
        .filter_map(|call| {
            if let DerivedCall::Level { before, level, .. } = call {
                Some((*level, before.xp))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(level_calls.len(), 59);
    assert!(level_calls.iter().skip(1).all(|(_, xp)| *xp == 10));
    assert_eq!(reload(&w).state_digest(), w.state_digest());
}

#[test]
fn level_sixty_untrained_defense_catch_up_matches_checkpoint_replay() {
    let (mut p, input, rows) = fixture::setup_with_levels(60, 60, false, 0x3f800000);
    p.npcs[0].max_health = 100_000;
    p.initialize_quest_progression(&input, rows).unwrap();
    p.admin = Some(AdminPolicy {
        world: [1; 16],
        world_owner: player().owner,
        allowlist: Default::default(),
        revision: 7,
    });
    let mut w = NativeWorld::synthetic_component_test(p, [1; 16], player(), 42).unwrap();
    reach(&mut w, 60);
    assert_eq!(
        w.state.combat.as_ref().unwrap().skills[&combat::DEFENSE_SKILL].value,
        1
    );
    let mut reopened = reload(&w);
    let input = command(
        &w,
        Command::Cast {
            ability: 60001,
            target: w.state.npcs[&2].id,
        },
    );
    let first = w
        .step(
            w.state.tick + 1,
            Some(input.clone()),
            &mut QuestAuthority::default(),
        )
        .unwrap()
        .clone();
    assert_eq!(
        reopened
            .step(
                reopened.state.tick + 1,
                Some(input),
                &mut QuestAuthority::default(),
            )
            .unwrap(),
        &first
    );
    reopened = reload(&reopened);
    assert_eq!(reopened.state_digest(), w.state_digest());
    let mut defense_draws = first
        .draws
        .iter()
        .filter(|draw| matches!(draw.purpose, DrawPurpose::DefenseGain { .. }))
        .count();
    let mut gains = first
        .events
        .iter()
        .filter(|event| matches!(event, Event::Combat(CombatEvent::DefenseSkill { .. })))
        .count();
    for _ in 0..40 {
        let record = w
            .step(w.state.tick + 1, None, &mut QuestAuthority::default())
            .unwrap()
            .clone();
        let replay = reopened
            .step(
                reopened.state.tick + 1,
                None,
                &mut QuestAuthority::default(),
            )
            .unwrap();
        assert_eq!(
            replay, &record,
            "source draws, events, and state must replay exactly"
        );
        defense_draws += record
            .draws
            .iter()
            .filter(|draw| matches!(draw.purpose, DrawPurpose::DefenseGain { .. }))
            .count();
        gains += record
            .events
            .iter()
            .filter(|event| matches!(event, Event::Combat(CombatEvent::DefenseSkill { .. })))
            .count();
        assert_eq!(reopened.state_digest(), w.state_digest());
        assert_eq!(reopened.state.rng_counter, w.state.rng_counter);
        reopened = reload(&reopened);
        assert_eq!(reopened.state_digest(), w.state_digest());
    }
    assert!(
        defense_draws > 0,
        "fixture must reach the catch-up probability draw"
    );
    assert!(
        gains > 0,
        "fixture must actually advance undertrained defense"
    );
    let skill = &w.state.combat.as_ref().unwrap().skills[&combat::DEFENSE_SKILL];
    assert_eq!(skill.maximum, 300);
    assert_eq!(usize::from(skill.value), 1 + gains);
    assert_eq!(
        w.state
            .quest_progression
            .as_ref()
            .unwrap()
            .derived_calls
            .iter()
            .filter(|call| matches!(call, DerivedCall::Defense { .. }))
            .count(),
        gains
    );
}
