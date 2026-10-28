// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic source-shaped rows; these tests never construct a native capability.
use super::{synthetic_fixture::*, *};
fn advance(w: &mut NativeWorld, n: u64, a: &mut impl Authority) {
    for _ in 0..n {
        w.step(w.state().tick + 1, None, a).unwrap();
    }
}
fn restored(w: &NativeWorld) -> NativeWorld {
    NativeWorld::restore_synthetic_component_test(
        w.profile().clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap()
}
#[test]
fn initialized_cast_retaliation_break_skill_reactive_regen_resume_and_replay() {
    let mut w = initialized_world(9);
    let mut a = KernelAuthority::default();
    let genesis = w.checkpoint().unwrap();
    let initial = w.state().clone();
    assert_eq!(initial.combat.as_ref().unwrap().skills[&95].value, 1);
    let command = cast(&w, 60001);
    w.step(1, Some(command), &mut a).unwrap();
    advance(&mut w, 4, &mut a);
    let command = cast(&w, 60001);
    w.step(6, Some(command), &mut a).unwrap();
    advance(&mut w, 174, &mut a);
    let events = w
        .records()
        .iter()
        .flat_map(|r| &r.events)
        .collect::<Vec<_>>();
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Spell {
            event: spell::SpellEvent::PeriodicDamage { .. },
            ..
        }
    )));
    assert!(events.iter().any(
        |e| matches!(e,Event::Combat(CombatEvent::Durability(c)) if !c.stat_transitions.is_empty())
    ));
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Combat(CombatEvent::DefenseReactive { remaining_ms: 4000 })
    )));
    assert!(events.iter().any(|e| matches!(e, Event::Mana { .. })));
    let c = w.state().combat.as_ref().unwrap();
    assert!(c.skills[&95].value > 1);
    assert_eq!(c.equipment.item(4).unwrap().current_durability, 0);
    assert!(c.stats.max_mana < initial.combat.as_ref().unwrap().stats.max_mana);
    assert!(w.state().health < initial.health);
    assert_eq!(w.state().npcs[&2].ai.pursuit_timer_ms(), 0);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), genesis).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state(), w.state());
    let mut reload = restored(&w);
    advance(&mut reload, 17, &mut a);
    advance(&mut w, 17, &mut a);
    assert_eq!(reload.state_digest(), w.state_digest());
}
#[test]
fn semantic_draw_order_and_missing_callback_are_atomic() {
    let mut w = initialized_world(7);
    let mut a = KernelAuthority::default();
    let command = cast(&w, 60001);
    w.step(1, Some(command), &mut a).unwrap();
    while !w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|e| matches!(e, Event::Melee { .. }))
    {
        w.step(w.state().tick + 1, None, &mut a).unwrap();
    }
    let purposes = w
        .last_tick()
        .unwrap()
        .draws
        .iter()
        .map(|d| d.purpose)
        .collect::<Vec<_>>();
    let damage = purposes
        .iter()
        .position(|p| matches!(p, DrawPurpose::MeleeDamage { .. }))
        .unwrap();
    assert!(matches!(
        purposes[damage + 1],
        DrawPurpose::MeleeOutcome { .. }
    ));
    let before = w.checkpoint().unwrap();
    let count = w.records().len();
    a.rear = true;
    assert!(matches!(
        w.step(w.state().tick + 1, None, &mut a),
        Err(NativeError::Melee(_))
    ));
    assert_eq!(w.checkpoint().unwrap(), before);
    assert_eq!(w.records().len(), count);
    a.rear = false;
    a.stale_action = true;
    assert_eq!(
        w.step(w.state().tick + 1, None, &mut a),
        Err(NativeError::AuthorityBinding)
    );
    assert_eq!(w.checkpoint().unwrap(), before);
    a.stale_action = false;
    w.step(w.state().tick + 1, None, &mut a).unwrap();
    let record = w.last_tick().unwrap().clone();
    let after = w.checkpoint().unwrap();
    assert!(w.replay(&[record]).is_err());
    assert_eq!(w.checkpoint().unwrap(), after);
}
#[test]
fn unsupported_slots_and_lethal_callback_roll_back_every_component() {
    let mut w = initialized_world(7);
    let mut a = KernelAuthority::default();
    let command = cast(&w, 60001);
    w.step(1, Some(command), &mut a).unwrap();
    advance(&mut w, 15, &mut a);
    w.state.combat.as_mut().unwrap().has_channeled_spell = true;
    let before = w.checkpoint().unwrap();
    assert!(matches!(
        w.step(w.state().tick + 1, None, &mut a),
        Err(NativeError::Melee(_))
    ));
    assert_eq!(w.checkpoint().unwrap(), before);
    w.state.combat.as_mut().unwrap().has_channeled_spell = false;
    w.state.combat.as_mut().unwrap().has_auto_repeat_spell = true;
    let before = w.checkpoint().unwrap();
    assert!(matches!(
        w.step(w.state().tick + 1, None, &mut a),
        Err(NativeError::Melee(_))
    ));
    assert_eq!(w.checkpoint().unwrap(), before);
    w.state.combat.as_mut().unwrap().has_auto_repeat_spell = false;
    w.state.health = 1;
    loop {
        let before = w.checkpoint().unwrap();
        match w.step(w.state().tick + 1, None, &mut a) {
            Err(NativeError::UnsupportedMechanic(reason)) => {
                assert!(reason.contains("Player::Kill"));
                assert_eq!(w.checkpoint().unwrap(), before);
                break;
            }
            Ok(_) => {}
            Err(e) => panic!("{e:?}"),
        }
    }
}
#[test]
fn resigned_combat_stat_recipe_skill_reactive_and_old_schema_edits_rejected() {
    let mut w = initialized_world(9);
    let mut a = KernelAuthority::default();
    let command = cast(&w, 60001);
    w.step(1, Some(command), &mut a).unwrap();
    advance(&mut w, 49, &mut a);
    let good = w.checkpoint().unwrap();
    for path in ["armor", "skill", "recipe", "reactive", "old"] {
        let mut v = serde_json::to_value(&good).unwrap();
        match path {
            "armor" => v["state"]["combat"]["stats"]["resistances"][0] = 999.into(),
            "skill" => v["state"]["combat"]["skills"]["95"]["maximum"] = 6.into(),
            "recipe" => v["state"]["combat"]["recipe"]["modifiers"]["create_health"] = 9000.into(),
            "reactive" => v["state"]["combat"]["defense_reactive_ms"] = 1.into(),
            "old" => v["state"]["version"] = 1.into(),
            _ => unreachable!(),
        }
        let mut forged: Checkpoint = serde_json::from_value(v).unwrap();
        forged.digest = crate::digest(b"benilla-native-state-v67\0", &forged.state);
        forged.checksum = crate::digest(
            b"benilla-native-checkpoint-v67\0",
            &(&forged.state, forged.digest, forged.history_tip),
        );
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile().clone(), forged).is_err(),
            "{path}"
        );
    }
}

#[test]
fn unchanged_health_miss_is_single_use_and_defense_revision_binds_facts() {
    let mut w = initialized_world(9);
    let mut authority = KernelAuthority::default();
    let command = cast(&w, 60001);
    w.step(1, Some(command), &mut authority).unwrap();
    let (before, record) = loop {
        let before = w.clone();
        let record = w
            .step(w.state.tick + 1, None, &mut authority)
            .unwrap()
            .clone();
        if record
            .events
            .iter()
            .any(|e| matches!(e, Event::Melee { effective: 0, .. }))
        {
            break (before, record);
        }
    };
    assert_eq!(before.state.health, w.state.health);
    let committed = w.checkpoint().unwrap();
    let count = w.records().len();
    assert!(w.replay(std::slice::from_ref(&record)).is_err());
    assert_eq!(w.checkpoint().unwrap(), committed);
    assert_eq!(w.records().len(), count);
    let old = record
        .replies
        .iter()
        .find(|r| matches!(r.request.query, Query::NativeMelee { .. }))
        .unwrap()
        .clone();
    struct OldFacts {
        old: AuthorityReply,
        base: KernelAuthority,
    }
    impl Authority for OldFacts {
        fn resolve(&mut self, r: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            if matches!(r.query, Query::NativeMelee { .. }) {
                Ok(self.old.clone())
            } else {
                self.base.resolve(r)
            }
        }
    }
    for kind in ["defense", "armor", "standing", "slot"] {
        let mut changed = before.clone();
        let c = changed.state.combat.as_mut().unwrap();
        match kind {
            "defense" => c.skills.get_mut(&95).unwrap().value += 1,
            "armor" => c.stats.resistances[0] += 1,
            "standing" => c.standing = false,
            "slot" => c.has_channeled_spell = true,
            _ => unreachable!(),
        };
        let checkpoint = changed.checkpoint().unwrap();
        let mut stale = OldFacts {
            old: old.clone(),
            base: KernelAuthority::default(),
        };
        assert_eq!(
            changed.step(changed.state.tick + 1, None, &mut stale),
            Err(NativeError::AuthorityBinding),
            "{kind}"
        );
        assert_eq!(changed.checkpoint().unwrap(), checkpoint);
    }
}
#[test]
fn zero_durability_chance_and_zero_skill_gain_still_consume_source_draws() {
    let mut profile = initialized_profile();
    let genesis = profile.combat.as_mut().unwrap();
    genesis.durability_hit_chance = spell::SpellFloat::ZERO;
    genesis.defense_gain = 0;
    let mut w = NativeWorld::synthetic_component_test(profile, [1; 16], player(), 9).unwrap();
    let mut authority = KernelAuthority::default();
    let command = cast(&w, 60001);
    w.step(1, Some(command), &mut authority).unwrap();
    advance(&mut w, 60, &mut authority);
    let mut hits = 0;
    for record in w.records() {
        if let Some(Event::Melee { effective, .. }) = record
            .events
            .iter()
            .find(|e| matches!(e, Event::Melee { .. }))
        {
            let purposes = record
                .draws
                .iter()
                .filter(|d| d.cast_id == 0)
                .map(|d| d.purpose)
                .collect::<Vec<_>>();
            assert!(matches!(purposes[0], DrawPurpose::MeleeDamage { .. }));
            assert!(matches!(purposes[1], DrawPurpose::MeleeOutcome { .. }));
            if *effective > 0 {
                hits += 1;
                assert!(matches!(purposes[2], DrawPurpose::DurabilityChance { .. }));
                assert!(matches!(purposes[3], DrawPurpose::DefenseGain { .. }));
                assert_eq!(purposes.len(), 4);
            } else {
                assert!(matches!(purposes[2], DrawPurpose::DefenseGain { .. }));
                assert_eq!(purposes.len(), 3);
            }
            for draw in record.draws.iter().filter(|d| {
                matches!(
                    d.purpose,
                    DrawPurpose::DefenseGain { .. } | DrawPurpose::DurabilityChance { .. }
                )
            }) {
                assert_eq!((draw.low, draw.high), (0, (1i64 << 53) - 1));
            }
        }
    }
    assert!(hits > 0);
    assert_eq!(w.state.combat.as_ref().unwrap().skills[&95].value, 1);
    assert_eq!(
        w.state
            .combat
            .as_ref()
            .unwrap()
            .equipment
            .item(4)
            .unwrap()
            .current_durability,
        1
    );
    assert_eq!(restored(&w).state_digest(), w.state_digest());
}

#[test]
fn durability_then_generic_pushback_then_defense_use_source_draw_order() {
    let mut p = initialized_profile();
    p.combat.as_mut().unwrap().defense_gain = 0;
    let mut w = NativeWorld::synthetic_component_test(p, [1; 16], player(), 9).unwrap();
    let mut a = KernelAuthority::default();
    let command = cast(&w, 60001);
    w.step(1, Some(command), &mut a).unwrap();
    advance(&mut w, 18, &mut a);
    let command = cast(&w, 60001);
    w.step(20, Some(command), &mut a).unwrap();
    advance(&mut w, 20, &mut a);
    let record = w
        .records()
        .iter()
        .find(|r| r.draws.iter().any(|d| d.purpose == DrawPurpose::Pushback))
        .unwrap();
    let purposes = record.draws.iter().map(|d| d.purpose).collect::<Vec<_>>();
    let start = purposes
        .iter()
        .position(|d| matches!(d, DrawPurpose::MeleeDamage { .. }))
        .unwrap();
    assert!(matches!(
        purposes[start + 1],
        DrawPurpose::MeleeOutcome { .. }
    ));
    assert!(matches!(
        purposes[start + 2],
        DrawPurpose::DurabilityChance { .. }
    ));
    assert!(matches!(
        purposes[start + 3],
        DrawPurpose::DurabilitySlot { .. }
    ));
    assert_eq!(purposes[start + 4], DrawPurpose::Pushback);
    assert!(matches!(
        purposes[start + 5],
        DrawPurpose::DefenseGain { .. }
    ));
    let damage = record
        .events
        .iter()
        .position(|e| matches!(e, Event::Melee { .. }))
        .unwrap();
    let durability = record
        .events
        .iter()
        .position(|e| matches!(e, Event::Combat(CombatEvent::Durability(_))))
        .unwrap();
    let pushed = record
        .events
        .iter()
        .position(|e| {
            matches!(
                e,
                Event::Spell {
                    event: spell::SpellEvent::Pushback(_),
                    ..
                }
            )
        })
        .unwrap();
    let committed = record
        .events
        .iter()
        .position(|e| matches!(e, Event::Combat(CombatEvent::ActionCommitted { .. })))
        .unwrap();
    assert!(damage < durability && durability < pushed && pushed < committed);
    assert_eq!(restored(&w).state_digest(), w.state_digest());
}

#[test]
fn move_into_range_melee_binds_committed_endpoint_not_previous_position() {
    struct PositionAuthority {
        base: KernelAuthority,
        position: Option<ai::Point>,
        swung: bool,
    }
    impl Authority for PositionAuthority {
        fn resolve(&mut self, r: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            match r.query {
                Query::Npc { position, .. } => self.position = Some(position),
                Query::NativeMelee {
                    npc_position,
                    player_in_combat,
                    ..
                } => {
                    assert!(player_in_combat);
                    assert_eq!(Some(npc_position), self.position);
                    self.swung = true;
                }
                _ => {}
            }
            let reply = self.base.resolve(r)?;
            if let Facts::Movement(ai::movement::MovementOutcome {
                result: ai::movement::MovementResult::Progress { position, .. },
                ..
            }) = &reply.facts
            {
                self.position = Some(*position);
            }
            Ok(reply)
        }
    }
    let mut p = initialized_profile();
    p.npcs[0].ai.spawn = point(16.0);
    let mut w = NativeWorld::synthetic_component_test(p, [1; 16], player(), 9).unwrap();
    let mut a = PositionAuthority {
        base: KernelAuthority::default(),
        position: None,
        swung: false,
    };
    let command = cast(&w, 60001);
    w.step(1, Some(command), &mut a).unwrap();
    for _ in 0..100 {
        w.step(w.state.tick + 1, None, &mut a).unwrap();
        if a.swung {
            break;
        }
    }
    assert!(a.swung);
    let first_combat = w
        .records()
        .iter()
        .find(|r| {
            r.events
                .iter()
                .any(|e| matches!(e, Event::Combat(CombatEvent::EnteredCombat)))
        })
        .unwrap();
    let entered = first_combat
        .events
        .iter()
        .position(|e| matches!(e, Event::Combat(CombatEvent::EnteredCombat)))
        .unwrap();
    if let Some(melee) = first_combat
        .events
        .iter()
        .position(|e| matches!(e, Event::Melee { .. }))
    {
        assert!(entered < melee);
    }
    assert!(w.last_tick().unwrap().replies.iter().any(|r| matches!(
        r.request.query,
        Query::NativeMelee {
            player_in_combat: true,
            ..
        }
    )));
    assert!(w.state.npcs[&2].ai.position() != point(16.0));
    assert!(w.last_tick().unwrap().events.iter().any(|e| matches!(
        e,
        Event::Npc {
            effect: ai::Effect::PositionCommitted(_),
            ..
        }
    )));
}

#[test]
fn first_nearby_combat_enters_player_before_same_tick_melee() {
    let mut w = initialized_world(9);
    let mut a = KernelAuthority::default();
    let command = cast(&w, 60001);
    w.step(1, Some(command), &mut a).unwrap();
    let record = loop {
        let r = w.step(w.state.tick + 1, None, &mut a).unwrap().clone();
        if r.events.iter().any(|e| {
            matches!(
                e,
                Event::Npc {
                    effect: ai::Effect::CombatStarted { .. },
                    ..
                }
            )
        }) {
            break r;
        }
    };
    let entered = record
        .events
        .iter()
        .position(|e| matches!(e, Event::Combat(CombatEvent::EnteredCombat)))
        .unwrap();
    let melee = record
        .events
        .iter()
        .position(|e| matches!(e, Event::Melee { .. }))
        .expect("nearby first CombatStarted must swing same tick");
    assert!(entered < melee);
    assert!(record.replies.iter().any(|r| matches!(
        r.request.query,
        Query::NativeMelee {
            player_in_combat: true,
            ..
        }
    )));
}
