// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored full-raw-shaped lifecycle inputs, never captured client rows.
//! Cast16/duration35/range35 use the parent fixture's explicit test resolutions.
use super::*;
use crate::native::synthetic_quest_fixture::command;

fn reopen(w: &NativeWorld) -> NativeWorld {
    NativeWorld::restore_synthetic_component_test(w.profile.clone(), w.checkpoint().unwrap())
        .unwrap()
}

fn mainhand_profile() -> Profile {
    let (mut p, mut initial, source) = inputs();
    let init::Rows::Complete(skills) = &mut initial.default_skills else {
        unreachable!()
    };
    skills.push(init::DefaultSkill { id: 136, step: 0 });
    initial.skill_lines.push(init::SkillLineRow {
        id: 136,
        category: 6,
    });
    let item = &mut initial.items[0];
    item.class = 2;
    item.subclass = 10;
    item.inventory_type = 17;
    item.delay_ms = 2301;
    assert_eq!(item.max_durability, 0);
    assert!(item.damage.iter().all(|line| line.max_bits == 0));
    p.initialize_player(&initial, f::float(100.0), 1).unwrap();
    p.initialize_spell_authority(&initial, source).unwrap();
    fixture::initialize_authored_location(&mut p);
    p
}

#[test]
fn source_mainhand_delay_resets_then_decrements_without_offhand_or_swings() {
    let p = mainhand_profile();
    let mut w = NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 7).unwrap();
    let genesis = w.checkpoint().unwrap();
    let mut a = fixture::spatial_only();
    assert_eq!(timers(&w), [0, 0]);
    start(&mut w, &mut a);
    advance(&mut w, &mut a);
    advance(&mut w, &mut a);
    assert_eq!(timers(&w), [2201, 0]);
    assert_eq!(
        w.state
            .combat
            .as_ref()
            .unwrap()
            .source_attack_times(w.profile.combat.as_ref().unwrap())
            .unwrap(),
        [2301, 2000]
    );
    let saved = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    assert_eq!(
        saved["state"]["combat"]["melee_reset"]["intervals_ms"],
        serde_json::json!([2301, 2000])
    );
    for elapsed in 1..=25 {
        let mut opened = reopen(&w);
        advance(&mut w, &mut a);
        advance(&mut opened, &mut fixture::spatial_only());
        assert_eq!(w.checkpoint().unwrap(), opened.checkpoint().unwrap());
        assert_eq!(timers(&w), [2301u32.saturating_sub((elapsed + 1) * 100), 0]);
        let state = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
        assert_eq!(
            state["state"]["combat"]["melee_timers"]["melee_attacking"],
            false
        );
        assert!(state["state"]["combat"]["melee_timers"]["victim"].is_null());
    }
    assert!(w.state.casts.is_empty());
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), genesis).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(w.checkpoint().unwrap(), replay.checkpoint().unwrap());
}

#[test]
fn latest_release_survives_target_respawn_and_replaces_only_on_next_launch() {
    let mut p = mainhand_profile();
    p.npcs[0].max_health = 1;
    p.npcs[0].ai.corpse_decay_ms = 0;
    p.npcs[0].ai.respawn_delay_seconds = 1;
    p.validate().unwrap();
    // Find a naturally successful source hit; no authored roll or live-health edit.
    let (mut w, initial) = (0..64)
        .find_map(|seed| {
            let mut w =
                NativeWorld::synthetic_component_test(p.clone(), [8; 16], f::player(), seed)
                    .unwrap();
            let initial = w.checkpoint().unwrap();
            let mut a = fixture::spatial_only();
            start(&mut w, &mut a);
            advance(&mut w, &mut a);
            advance(&mut w, &mut a);
            let launch = w.state.casts[&1].execution.launch_context().unwrap();
            (launch.rolls.hit > launch.resist_chance).then_some((w, initial))
        })
        .expect("source RNG reaches a successful hit");
    let old = serde_json::to_value(w.checkpoint().unwrap()).unwrap()["state"]["combat"]
        ["melee_reset"]
        .clone();
    assert_eq!(old["target"]["generation"], 0);
    let mut a = fixture::spatial_only();
    let mut saw_dead = false;
    while w.state.npcs[&2].id.generation == 0 {
        assert!(w.state.tick < 80);
        advance(&mut w, &mut a);
        saw_dead |= w.state.npcs[&2].health == 0;
        assert_eq!(reopen(&w).checkpoint().unwrap(), w.checkpoint().unwrap());
    }
    assert!(saw_dead);
    assert!(w.state.casts.is_empty());
    assert_eq!(w.state.npcs[&2].health, 1);
    assert_eq!(
        serde_json::to_value(w.checkpoint().unwrap()).unwrap()["state"]["combat"]["melee_reset"],
        old
    );
    while w.state.now_ms < 1600 {
        advance(&mut w, &mut a);
    }
    let generation = w.state.npcs[&2].id.generation;
    start(&mut w, &mut a);
    advance(&mut w, &mut a);
    advance(&mut w, &mut a);
    assert_eq!(timers(&w), [2201, 0]);
    let valid = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    let current = &valid["state"]["combat"]["melee_reset"];
    assert_eq!(current["cast_id"], 2);
    assert_eq!(current["target"]["generation"], generation);
    assert_ne!(*current, old);
    assert_eq!(reopen(&w).checkpoint().unwrap(), w.checkpoint().unwrap());
    for mutation in 0..6 {
        let mut forged = valid.clone();
        let witness = &mut forged["state"]["combat"]["melee_reset"];
        match mutation {
            0 => *witness = old.clone(),
            1 => witness["target"]["generation"] = serde_json::json!(generation + 1),
            2 => witness["target"]["generation"] = serde_json::json!(0),
            3 => witness["cast_id"] = serde_json::json!(1),
            4 => {
                witness["release_receipt"]["request"]["query"]["SourceSpell"]["Semantics"]
                    ["history"] = serde_json::to_value([0u8; 32]).unwrap()
            }
            _ => witness["before_execution"]["created_ms"] = serde_json::json!(0),
        }
        if mutation == 0 {
            // Even a matching old countdown cannot hide a newer paid release.
            forged["state"]["combat"]["melee_timers"]["remaining_ms"] = serde_json::json!([0, 0]);
        }
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), rehash(forged))
                .is_err(),
            "mutation{mutation}"
        );
    }
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(w.checkpoint().unwrap(), replay.checkpoint().unwrap());
}

struct OrderedSpatial {
    inner: fixture::SpatialOnly,
    order: Vec<&'static str>,
}
impl Authority for OrderedSpatial {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if matches!(&request.query, Query::SourceMotion(MotionQuery::Visitor(v)) if v.owner == VisitorOwner::Player)
        {
            self.order.push("notify");
        }
        self.inner.resolve(request)
    }
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        if matches!(&reply.request.query, Query::SourceSpell(SourceSpellQuery::Semantics(q)) if q.phase == SourceSpellPhase::ReleaseHit)
        {
            self.order.push("release");
        }
        self.inner.observe_derived(reply)
    }
}

#[test]
fn co_due_notification_preserves_enqueue_order_and_cancel_precedes_release_tick() {
    for notify_first in [false, true] {
        for cancel in [false, true] {
            let mut p = profile();
            p.source_motion.as_mut().unwrap().relocation_aggro_delay_ms =
                if notify_first { 101 } else { 1 };
            let mut w = NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 7).unwrap();
            let initial = w.checkpoint().unwrap();
            let mut a = OrderedSpatial {
                inner: fixture::spatial_only(),
                order: vec![],
            };
            start(&mut w, &mut a);
            let notify_tick = if notify_first { 2 } else { 3 };
            for tick in 2..=3 {
                a.order.clear();
                let mut batch = vec![];
                if tick == notify_tick {
                    batch.push(command(
                        &w,
                        Command::MovementPose {
                            destination: w.state.position,
                            orientation: Orientation::new(0.5).unwrap(),
                            movement_flags:
                                benilla_sim::creature_motion::GroundMovementFlags::STOPPED,
                        },
                    ));
                }
                if tick == 3 && cancel {
                    let mut input = command(
                        &w,
                        Command::CancelCast {
                            spell_id: 133,
                            target: CancelCastTarget::Instance(1),
                        },
                    );
                    input.input.sequence += batch.len() as u64;
                    batch.push(input);
                }
                w.step_batch(tick, batch, &mut a).unwrap();
            }
            assert_eq!(
                a.order,
                if cancel {
                    vec!["notify"]
                } else if notify_first {
                    vec!["notify", "release"]
                } else {
                    vec!["release", "notify"]
                }
            );
            assert_eq!(timers(&w), if cancel { [0, 0] } else { [1900, 0] });
            assert_eq!(
                w.state.combat.as_ref().unwrap().melee_reset.is_some(),
                !cancel
            );
            if cancel {
                assert!(w.last_tick().unwrap().draws.is_empty());
            }
            assert_eq!(reopen(&w).checkpoint().unwrap(), w.checkpoint().unwrap());
            let mut replay =
                NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
            replay.replay(w.records()).unwrap();
            assert_eq!(w.checkpoint().unwrap(), replay.checkpoint().unwrap());
        }
    }
}

#[test]
fn combat_reset_requires_positive_effective_cost_but_legacy_zero_cost_stays_supported() {
    for combat in [false, true] {
        for positive_flat in [false, true] {
            let (mut p, mut initial, mut source) =
                if combat { inputs() } else { fixture::inputs() };
            let AbilityAdmission::Available(spec) = &mut p.abilities[0].admission else {
                unreachable!()
            };
            spec.mana_cost = 0;
            for effect in &mut spec.effects {
                match effect {
                    spell::MageEffect::SchoolDamage(value)
                    | spell::MageEffect::PeriodicDamage { value, .. } => value.base_level = 2,
                    spell::MageEffect::Empty => {}
                    spell::MageEffect::Unsupported { .. } => {
                        panic!("fixture must contain supported effects")
                    }
                }
            }
            source.spells[0].words[32] = u32::from(positive_flat);
            // Rank5 /5 - baseLevel2 = -1; a positive flat cost can still reduce to0.
            source.spells[0].words[33] = u32::from(positive_flat);
            source.spells[0].words[28] = 2;
            initial.spells[0].words = source.spells[0].words.clone().try_into().unwrap();
            p.initialize_player(&initial, f::float(100.0), 1).unwrap();
            let before = p.clone();
            let result = p.initialize_spell_authority(&initial, source);
            if combat {
                assert!(result.is_err());
                assert_eq!(p, before);
            } else {
                result.unwrap();
                fixture::initialize_authored_location(&mut p);
                let mut w =
                    NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 7).unwrap();
                let mana = w.state.resources.caster.mana;
                let mut a = fixture::spatial_only();
                start(&mut w, &mut a);
                advance(&mut w, &mut a);
                advance(&mut w, &mut a);
                assert!(w.state.casts[&1].release_binding.is_some());
                assert_eq!(w.state.resources.caster.mana, mana);
                assert_eq!(w.state.resources.caster.last_mana_use_ms, None);
                assert_eq!(timers(&w), [0, 0]);
                assert!(w.state.combat.as_ref().unwrap().melee_reset.is_none());
                assert_eq!(reopen(&w).checkpoint().unwrap(), w.checkpoint().unwrap());
            }
        }
    }
}

#[test]
fn retained_release_rolls_cannot_disagree_with_the_still_live_cast() {
    let w = (0..256)
        .find_map(|seed| {
            let mut w = world(seed);
            let mut a = fixture::spatial_only();
            start(&mut w, &mut a);
            advance(&mut w, &mut a);
            advance(&mut w, &mut a);
            w.state.casts[&1]
                .execution
                .launch_context()
                .unwrap()
                .rolls
                .direct[0]
                .is_some()
                .then_some(w)
        })
        .unwrap();
    let valid = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    let mut changed = valid.clone();
    let roll =
        &mut changed["state"]["combat"]["melee_reset"]["execution"]["launch"]["rolls"]["direct"][0];
    *roll = serde_json::json!(if roll.as_i64() == Some(1) { 2 } else { 1 });
    assert!(
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), rehash(changed)).is_err()
    );
    assert_eq!(reopen(&w).checkpoint().unwrap(), w.checkpoint().unwrap());
}
