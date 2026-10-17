// SPDX-License-Identifier: GPL-3.0-or-later
//! Byte-backed dry initialization composed with authored idle visitors and AGGRO
//! text. This is a bounded component callback/lifecycle proof, not entry-6 combat
//! parity. Base tests leave assistance unselected; the child assistance tests
//! separately own its bounded complete-empty traversal/event continuation.
//! Membership, camera, LOS, models, spells and text are explicitly authored.
#[path = "idle_event_ai_tests/assistance_tests.rs"]
mod assistance_tests;
#[path = "idle_event_ai_tests/neighbor_checkpoint_tests.rs"]
mod neighbor_checkpoint_tests;
#[path = "idle_event_ai_tests/neighbor_tests.rs"]
mod neighbor_tests;

use super::{lifecycle_tests as life, *};
use benilla_world_runtime::native::{
    synthetic_combat_fixture as combat, synthetic_event_ai_fixture as text,
    synthetic_spell_fixture as spells, synthetic_two_owner_fixture as visitors,
};

fn composed_profile(binding: CreatureInitializationSnapshot, chance: u32) -> Profile {
    let p = profile_at_with_faction(
        binding,
        [77; 32],
        0.5f32 * (64.0 * 533.333_3),
        2,
        None,
        true,
    );
    compose_owned_profile(p, chance, false)
}

fn compose_owned_profile(p: Profile, chance: u32, neutral_neighbors: bool) -> Profile {
    compose_owned_profile_with_helper(p, chance, neutral_neighbors, None)
}

fn compose_owned_profile_with_helper(
    mut p: Profile,
    chance: u32,
    neutral_neighbors: bool,
    helper: Option<u64>,
) -> Profile {
    // Already in melee reach, outside backpedal distance. No dry observation is
    // used as a certificate for habitat-3 near-point, chase or random movement.
    let spawn = p.npcs[0].ai.spawn;
    p.player.start = ai::Point::new(
        spawn.x.as_f32() + if neutral_neighbors { 2.75 } else { 3.0 },
        spawn.y.as_f32(),
        spawn.z.as_f32(),
    )
    .unwrap();
    for npc in &mut p.npcs {
        npc.combat = Some(CreatureCombatProfile {
            minimum_damage: f::float(1.0),
            maximum_damage: f::float(2.0),
            no_crushing_blows: false,
            creature_is_critter: false,
        });
    }
    let AbilityAdmission::Available(spec) = &mut p.abilities[0].admission else {
        unreachable!()
    };
    spec.spell_rank = 1;
    spec.mana_cost = 0;
    spec.projectile_speed = f::float(100.0);
    spec.effects[1] = benilla_sim::classic_spell::MageEffect::Empty;
    let benilla_sim::classic_spell::MageEffect::SchoolDamage(value) = &mut spec.effects[0] else {
        unreachable!()
    };
    value.base_points = 30;
    value.die_sides = 1;
    p.source_motion
        .as_mut()
        .unwrap()
        .spell_hits
        .get_mut(&60001)
        .unwrap()
        .implicit_target_a[1] = 0;
    let mut initial = f::initialized_starter_input();
    initial.items[0].max_durability = 0;
    spells::rekey(&mut p, &mut initial);
    let mut spell_sources = spells::source(&p);
    spell_sources.npcs.sort_by_key(|row| row.entry);
    spell_sources.npcs.dedup_by_key(|row| row.entry);
    for source in &spell_sources.spells {
        initial
            .spells
            .iter_mut()
            .find(|row| row.id == source.words[0])
            .unwrap()
            .words = source.words.clone().try_into().unwrap();
    }
    p.initialize_player(&initial, f::float(100.0), 1).unwrap();
    let mut interactions = combat::source_rows(&p);
    interactions.npcs.sort_by_key(|row| row.entry);
    interactions.npcs.dedup_by_key(|row| row.entry);
    if neutral_neighbors {
        interactions
            .factions
            .iter_mut()
            .find(|row| row.template_id == 2)
            .unwrap()
            .enemy_group_mask = 0;
    }
    p.initialize_interactions(interactions).unwrap();
    p.initialize_combat_authority(vec![NpcCombatAuthoritySource {
        entry: 6,
        extra_flags: 2,
        static_flags1: 0,
        static_flags2: 0,
        static_flags3: 0,
        static_flags4: 0,
    }])
    .unwrap();
    if neutral_neighbors {
        p = neighbor_tests::expand_cluster_owners_with_helper(p, helper);
    }
    p = text::install(p, chance);
    if let Some(helper) = helper {
        p = neighbor_tests::positive_help_tests::install_helper_text(p, helper);
    }
    spell_sources.npcs = spells::source(&p).npcs;
    spell_sources.npcs.sort_by_key(|row| row.entry);
    spell_sources.npcs.dedup_by_key(|row| row.entry);
    for row in &mut spell_sources.npcs {
        let npc = p.npcs.iter().find(|npc| npc.entry == row.entry).unwrap();
        let template = &p
            .creature_generations
            .as_ref()
            .unwrap()
            .source(npc.ai.creature_id)
            .unwrap()
            .input
            .template;
        row.minimum_level = template.minimum_level;
        row.maximum_level = template.maximum_level;
        row.creature_type = template.creature_type;
        let owner = p
            .source_visitors
            .as_ref()
            .unwrap()
            .owner(npc.ai.creature_id)
            .unwrap();
        row.ai_name = owner.ai_name.clone();
        let SourceVisitorRows::Complete(events) = &owner.entry_events else {
            unreachable!()
        };
        row.event_scripts = SpellSourceRows::Complete(events.clone());
    }
    p.initialize_spell_authority(&initial, spell_sources)
        .unwrap();
    p
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fault {
    None,
    Audience,
    StaleAudience,
    AfterAudience,
    ExpiredInitialization,
    WetInitialization,
}
struct ComposedAuthority<'a> {
    provider: SourceCreatureInitializationProvider<'a>,
    initializations: Vec<AuthorityRequest>,
    unsupported_motion: Vec<AuthorityRequest>,
    text: text::Authority,
    spells: spells::SpatialOnly,
    fault: Fault,
    rejected: Option<AuthorityRequest>,
    saw_audience: bool,
}
impl<'a> ComposedAuthority<'a> {
    fn new(provider: SourceCreatureInitializationProvider<'a>) -> Self {
        Self {
            provider,
            initializations: vec![],
            unsupported_motion: vec![],
            text: Default::default(),
            spells: Default::default(),
            fault: Fault::None,
            rejected: None,
            saw_audience: false,
        }
    }
}
impl Authority for ComposedAuthority<'_> {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if matches!(
            request.query,
            Query::SourceMotion(
                MotionQuery::NearPoint(_) | MotionQuery::Route(_) | MotionQuery::Visitor(_)
            )
        ) {
            self.unsupported_motion.push(request.clone());
            return Err(NativeError::UnsupportedMechanic(
                "dry idle fixture has no route authority",
            ));
        }
        if matches!(request.query, Query::CreatureInitialization(_)) {
            self.initializations.push(request.clone());
            if self.fault == Fault::ExpiredInitialization {
                assert_eq!(
                    self.provider.resolve(request, Instant::now()),
                    Err(Failure::Deadline)
                );
                self.rejected = Some(request.clone());
                return Err(NativeError::AuthorityUnavailable(
                    "expired dry initialization",
                ));
            }
            let mut facts = self.provider.resolve(request, deadline()).unwrap();
            if self.fault == Fault::WetInitialization {
                let Facts::CreatureInitialization(facts) = &mut facts else {
                    unreachable!()
                };
                facts.status = environment::LiquidStatus::UnderWater;
                self.rejected = Some(request.clone());
            }
            return Ok(AuthorityReply {
                request: request.clone(),
                facts,
            });
        }
        if matches!(
            request.query,
            Query::SourceSpell(_) | Query::SourceLocation(_)
        ) {
            return self.spells.resolve(request);
        }
        if matches!(request.query, Query::EventAiAudience(_)) {
            self.saw_audience = true;
            if self.fault == Fault::Audience {
                self.rejected = Some(request.clone());
                return Err(NativeError::AuthorityUnavailable("missing authored camera"));
            }
            let mut reply = self.text.resolve(request)?;
            if self.fault == Fault::StaleAudience {
                reply.request.binding.tick += 1;
                self.rejected = Some(request.clone());
            }
            return Ok(reply);
        }
        self.text.resolve(request)
    }
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        if self.fault == Fault::AfterAudience && self.saw_audience {
            self.rejected = Some(reply.request.clone());
            return Err(NativeError::AuthorityUnavailable(
                "late authored callback receipt",
            ));
        }
        self.spells.observe_derived(reply)?;
        self.text.observe_derived(reply)
    }
}

fn step(world: &mut NativeWorld, authority: &mut impl Authority) {
    world.step(world.state().tick + 1, None, authority).unwrap();
}
fn evaluation(record: &TickRecord) -> Option<&EventAiEvaluation> {
    record.events.iter().find_map(|event| match event {
        Event::EventAiEvaluation(evaluation) => Some(evaluation.as_ref()),
        _ => None,
    })
}
fn enter(world: &mut NativeWorld, authority: &mut impl Authority) -> TickRecord {
    for _ in 0..100 {
        let command = world.state().casts.is_empty().then(|| f::cast(world, 133));
        let record = world
            .step(world.state().tick + 1, command, authority)
            .unwrap()
            .clone();
        if evaluation(&record).is_some() {
            assert_eq!(world.state().npcs[&2].ai.phase(), ai::Phase::Combat);
            assert!(world.state().npcs[&2].health > 0);
            return record;
        }
    }
    panic!("actual hostile projectile callback must enter combat")
}
fn assert_evaluation(world: &NativeWorld, record: &TickRecord, chance: u32) -> bool {
    let evaluation = evaluation(record).unwrap();
    let id = world.state().npcs[&2].id;
    assert_eq!(evaluation.callback.npc, id);
    assert_eq!(evaluation.callback.invoker, world.state().player);
    assert!(matches!(
        evaluation.callback.kind,
        EventAiCallbackKind::CombatReceipt { .. }
    ));
    assert_eq!(evaluation.occurrence, 0);
    assert_eq!(evaluation.actions.len(), 1);
    let action = &evaluation.actions[0];
    assert_eq!(action.event_id, text::EVENT);
    let draws: Vec<_> = record
        .draws
        .iter()
        .filter(|draw| {
            matches!(
                draw.purpose,
                DrawPurpose::EventAiChance { .. } | DrawPurpose::EventAiAction { .. }
            )
        })
        .collect();
    assert!(
        matches!(draws[0].purpose, DrawPurpose::EventAiChance { npc, event_id, occurrence: 0 }
        if npc == id && event_id == text::EVENT)
    );
    let passed = (draws[0].value as u32) % 100 < chance;
    assert_eq!(draws.len(), if passed { 2 } else { 1 });
    assert_eq!(action.chance_counter, draws[0].first_counter);
    for draw in &draws {
        assert_eq!(
            (draw.low, draw.high, draw.consumed),
            (0, i64::from(u32::MAX), 1)
        );
    }
    assert_eq!(action.action_counter.is_some(), passed);
    assert_eq!(action.broadcast_id.is_some(), passed);
    if passed {
        assert!(
            matches!(draws[1].purpose, DrawPurpose::EventAiAction { npc, event_id, occurrence: 0 }
            if npc == id && event_id == text::EVENT)
        );
        assert_eq!(draws[1].first_counter, draws[0].first_counter + 1);
        assert_eq!(action.action_counter, Some(draws[1].first_counter));
        let speech = action.speech.as_ref().unwrap();
        assert_eq!(speech.callback.npc, id);
        assert_eq!(speech.recipient, world.state().player);
        assert!(matches!(
            speech.broadcast_id,
            text::TEXT_COMMON | text::TEXT_UNIVERSAL
        ));
        let start = record
            .events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    Event::Npc {
                        effect: ai::Effect::CombatStarted { .. },
                        ..
                    }
                )
            })
            .unwrap();
        let spoken = record
            .events
            .iter()
            .position(|event| matches!(event, Event::EventAiSpeech(_)))
            .unwrap();
        assert!(start < spoken);
    } else {
        assert!(action.speech.is_none());
        assert!(record
            .events
            .iter()
            .all(|event| !matches!(event, Event::EventAiSpeech(_))));
    }
    passed
}
fn assert_owners(world: &NativeWorld) {
    life::assert_dry_owner(world);
    let p = world.profile();
    let source = p.creature_generations.as_ref().unwrap().source(2).unwrap();
    assert_eq!(source.input.template.extra_flags, 2);
    assert_eq!(p.npcs[0].ai.reaction, ai::Reaction::Defensive);
    let owner = p.source_visitors.as_ref().unwrap().owner(2).unwrap();
    assert_eq!(owner.default_movement_type, 0);
    assert_eq!(owner.ai_name, "EventAI");
    assert_eq!(
        owner.addon_closure,
        SourceVisitorAddonClosure::GenerationSource {
            source_identity: source.identity(),
        }
    );
    assert_eq!(p.event_ai().unwrap().owner(2).unwrap().extra_flags, 2);
    let npc = &world.state().npcs[&2];
    let initialized = npc.initialized_creature().unwrap();
    let facts = npc
        .creature_capabilities()
        .unwrap()
        .witness()
        .initialization()
        .unwrap();
    let height = npc.creature_collision_height().unwrap().witness();
    let width = npc.creature_collision_width().unwrap().witness();
    assert_eq!(height.native_display_id(), initialized.model.display_id);
    assert_eq!(width.native_display_id(), initialized.model.display_id);
    assert_eq!(facts.request.model_id, height.model_id());
    assert_eq!(facts.request.model_id, width.model_id());
    assert_eq!(facts.request.collision_height, height.height());
    assert_eq!(facts.request.binding.npc, Some(npc.id));
    assert_eq!(facts.status, environment::LiquidStatus::NoWater);
    assert!(facts.data.is_none());
}
fn assert_no_routes(authority: &ComposedAuthority<'_>) {
    assert!(authority.unsupported_motion.is_empty());
    assert!(authority.text.base.queries.iter().all(|request| !matches!(
        request.query,
        Query::SourceMotion(
            MotionQuery::NearPoint(_) | MotionQuery::Route(_) | MotionQuery::Visitor(_)
        )
    )));
}

#[test]
fn dry_idle_event_ai_hostile_entry_sheath_lives_replay_and_compaction() {
    let (products, config) = products::authored_ground_height();
    let participants = NonRenderParticipants::authored_fixture(config.content, config.map);
    let snapshot = products
        .load_map_height(config, Some(&participants))
        .unwrap();
    let p = composed_profile(snapshot_binding(&snapshot), 100);
    let provider = SourceCreatureInitializationProvider::bind([8; 16], &p, snapshot).unwrap();
    let mut authority = ComposedAuthority::new(provider);
    let mut world = NativeWorld::synthetic_component_test_with_initialization(
        p,
        [8; 16],
        f::player(),
        7,
        None,
        &mut authority,
    )
    .unwrap();
    let initial = world.checkpoint().unwrap();
    let cold_draws = life::cold_draws(&world);
    life::assert_query_order(&authority.initializations[0], &cold_draws);
    let level_index = cold_draws
        .iter()
        .position(|draw| matches!(draw.purpose, DrawPurpose::CreatureLevel { .. }))
        .unwrap();
    let delay_index = cold_draws
        .iter()
        .position(|draw| {
            matches!(
                draw.purpose,
                DrawPurpose::CreatureInitialRespawnDelay { .. }
            )
        })
        .unwrap();
    assert!(level_index < delay_index);
    assert_eq!(
        cold_draws[delay_index].first_counter,
        cold_draws[level_index].first_counter + u64::from(cold_draws[level_index].consumed)
    );
    let genesis_end = world.state().rng_counter;
    assert_owners(&world);
    for _ in 0..5 {
        step(&mut world, &mut authority);
    }
    assert_eq!(world.state().npcs[&2].ai.phase(), ai::Phase::Idle);
    assert!(world
        .records()
        .iter()
        .all(|record| record.draws.is_empty() && evaluation(record).is_none()));
    assert!(authority.text.base.derived.iter().any(|reply| matches!(
        &reply.facts,
        Facts::SourceMotion(MotionFacts::VisitorSight(
            SightDisposition::PlayerSightApplied
        ))
    )));
    let first = enter(&mut world, &mut authority);
    assert!(assert_evaluation(&world, &first, 100));
    assert!(evaluation(&first).unwrap().actions[0].chance_counter >= genesis_end);
    let previous = world.state().npcs[&2].id;
    let previous_model = world.state().npcs[&2]
        .initialized_creature()
        .unwrap()
        .clone();
    for _ in 0..100 {
        if world.state().npcs[&2].health == 0 {
            break;
        }
        let command = world.state().casts.is_empty().then(|| f::cast(&world, 133));
        let record = world
            .step(world.state().tick + 1, command, &mut authority)
            .unwrap();
        assert!(evaluation(record).is_none(), "one enter callback per life");
    }
    assert_eq!(world.state().npcs[&2].health, 0);
    let death = world.last_tick().unwrap().draws.iter().find(|draw|
        matches!(draw.purpose, DrawPurpose::CreatureDeathRespawnDelay { npc } if npc == previous)).unwrap();
    assert!(
        death.first_counter
            > evaluation(&first).unwrap().actions[0]
                .action_counter
                .unwrap()
    );
    assert_owners(&world);
    for _ in 0..50 {
        if world.state().npcs[&2].id != previous {
            break;
        }
        step(&mut world, &mut authority);
    }
    assert_eq!(world.state().npcs[&2].id.generation, 1);
    let respawn = world.last_tick().unwrap();
    life::assert_query_order(authority.initializations.last().unwrap(), &respawn.draws);
    assert_eq!(
        world.state().npcs[&2].historical_initialized_creature(previous),
        Some(&previous_model)
    );
    assert_owners(&world);
    for _ in 0..5 {
        step(&mut world, &mut authority);
        assert!(evaluation(world.last_tick().unwrap()).is_none());
    }
    assert_eq!(world.state().npcs[&2].ai.phase(), ai::Phase::Idle);
    let second = enter(&mut world, &mut authority);
    assert!(assert_evaluation(&world, &second, 100));
    assert_owners(&world);
    assert!(authority
        .spells
        .queries
        .iter()
        .any(|request| matches!(request.query, Query::SourceSpell(_))));
    assert_no_routes(&authority);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(world.profile().clone(), initial).unwrap();
    replay.replay(world.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), world.checkpoint().unwrap());
    let saved = world.checkpoint().unwrap();
    world.checkpoint_committed(&saved).unwrap();
    assert!(world.records().is_empty());
    let mut restored =
        NativeWorld::restore_synthetic_component_test(world.profile().clone(), saved.clone())
            .unwrap();
    step(&mut world, &mut authority);
    step(&mut restored, &mut authority);
    assert_eq!(restored.checkpoint().unwrap(), world.checkpoint().unwrap());
    assert!(evaluation(restored.last_tick().unwrap()).is_none());
    reject_history(&restored, saved);
}

fn reject_history(world: &NativeWorld, original: Checkpoint) {
    let base = serde_json::to_value(&original).unwrap();
    assert_eq!(life::rehash(base.clone()), original);
    let mutations = [
        (
            "/state/event_ai/owners/2/id/generation",
            serde_json::json!(0),
        ),
        (
            "/state/event_ai/history/0/EnterCombat/callback/npc/generation",
            serde_json::json!(1),
        ),
        (
            "/state/event_ai/history/0/EnterCombat/actions/0/chance_counter",
            serde_json::json!(0),
        ),
        (
            "/state/event_ai/history/0/EnterCombat/actions/0/action_counter",
            serde_json::Value::Null,
        ),
        (
            "/state/event_ai/history/0/EnterCombat/actions/0/speech/text",
            serde_json::json!("foreign line"),
        ),
        (
            "/state/npcs/2/source_generation/generations/0/initialization",
            serde_json::Value::Null,
        ),
        (
            "/state/npcs/2/source_generation/generations/1/initialization/status",
            serde_json::json!("UnderWater"),
        ),
        ("/state/event_ai/history", serde_json::json!([])),
    ];
    for (case, (path, replacement)) in mutations.into_iter().enumerate() {
        let mut value = base.clone();
        let existing = value.pointer_mut(path).expect("existing retained field");
        match case {
            0 => assert_eq!(*existing, serde_json::json!(1)),
            1 => assert_eq!(*existing, serde_json::json!(0)),
            2 | 3 => assert!(existing.as_u64().unwrap() > 0),
            4 => assert!(!existing.as_str().unwrap().is_empty()),
            5 => assert!(existing.is_object()),
            6 => assert_eq!(*existing, serde_json::json!("NoWater")),
            _ => assert!(existing.as_array().unwrap().len() >= 3),
        }
        assert_ne!(*existing, replacement);
        *existing = replacement;
        assert!(
            NativeWorld::restore_synthetic_component_test(
                world.profile().clone(),
                life::rehash(value)
            )
            .is_err(),
            "case {case}"
        );
    }
}

#[test]
fn dry_idle_event_ai_chance_ten_uses_one_or_two_words_after_generation_prefix() {
    let (products, config) = products::authored_ground_height();
    let participants = NonRenderParticipants::authored_fixture(config.content, config.map);
    let mut outcomes = std::collections::BTreeSet::new();
    for seed in 0..128 {
        let snapshot = products
            .load_map_height(config, Some(&participants))
            .unwrap();
        let p = composed_profile(snapshot_binding(&snapshot), 10);
        let provider = SourceCreatureInitializationProvider::bind([8; 16], &p, snapshot).unwrap();
        let mut authority = ComposedAuthority::new(provider);
        let mut world = NativeWorld::synthetic_component_test_with_initialization(
            p,
            [8; 16],
            f::player(),
            seed,
            None,
            &mut authority,
        )
        .unwrap();
        let initial = world.checkpoint().unwrap();
        let prefix = world.state().rng_counter;
        let record = enter(&mut world, &mut authority);
        let passed = assert_evaluation(&world, &record, 10);
        assert!(evaluation(&record).unwrap().actions[0].chance_counter >= prefix);
        assert_eq!(authority.text.queries.len(), usize::from(passed));
        assert_owners(&world);
        let mut replay =
            NativeWorld::restore_synthetic_component_test(world.profile().clone(), initial)
                .unwrap();
        replay.replay(world.records()).unwrap();
        assert_eq!(replay.checkpoint().unwrap(), world.checkpoint().unwrap());
        outcomes.insert(passed);
        if outcomes.len() == 2 {
            break;
        }
    }
    assert_eq!(outcomes, std::collections::BTreeSet::from([false, true]));
}

#[test]
fn dry_idle_event_ai_camera_faults_rollback_callback_rng_and_owner_history() {
    for fault in [Fault::Audience, Fault::StaleAudience, Fault::AfterAudience] {
        let (products, config) = products::authored_ground_height();
        let participants = NonRenderParticipants::authored_fixture(config.content, config.map);
        let snapshot = products
            .load_map_height(config, Some(&participants))
            .unwrap();
        let p = composed_profile(snapshot_binding(&snapshot), 100);
        let provider = SourceCreatureInitializationProvider::bind([8; 16], &p, snapshot).unwrap();
        let mut authority = ComposedAuthority::new(provider);
        let mut world = NativeWorld::synthetic_component_test_with_initialization(
            p,
            [8; 16],
            f::player(),
            7,
            None,
            &mut authority,
        )
        .unwrap();
        authority.fault = fault;
        let mut reached = false;
        for _ in 0..100 {
            let before = world.checkpoint().unwrap();
            let records = world.records().to_vec();
            let mut clean = world.clone();
            let tick = world.state().tick + 1;
            let command = world.state().casts.is_empty().then(|| f::cast(&world, 133));
            let result = world.step(tick, command.clone(), &mut authority);
            if authority.rejected.is_some() {
                assert!(result.is_err());
                assert_eq!(world.checkpoint().unwrap(), before);
                assert_eq!(world.records(), records);
                assert_eq!(world.state().npcs[&2].ai.phase(), ai::Phase::Idle);
                authority.fault = Fault::None;
                authority.rejected = None;
                authority.saw_audience = false;
                world.step(tick, command.clone(), &mut authority).unwrap();
                clean.step(tick, command, &mut authority).unwrap();
                assert_eq!(world.checkpoint().unwrap(), clean.checkpoint().unwrap());
                assert_eq!(world.records(), clean.records());
                assert!(assert_evaluation(&world, world.last_tick().unwrap(), 100));
                assert_owners(&world);
                reached = true;
                break;
            }
            result.unwrap();
        }
        assert!(reached, "fault must reach the AGGRO callback frame");
    }
}

#[test]
fn dry_idle_event_ai_respawn_fault_preserves_previous_life_and_retries_exact_query() {
    let (products, config) = products::authored_ground_height();
    let participants = NonRenderParticipants::authored_fixture(config.content, config.map);
    let snapshot = products
        .load_map_height(config, Some(&participants))
        .unwrap();
    let p = composed_profile(snapshot_binding(&snapshot), 100);
    let provider = SourceCreatureInitializationProvider::bind([8; 16], &p, snapshot).unwrap();
    let mut authority = ComposedAuthority::new(provider);
    let mut world = NativeWorld::synthetic_component_test_with_initialization(
        p,
        [8; 16],
        f::player(),
        7,
        None,
        &mut authority,
    )
    .unwrap();
    enter(&mut world, &mut authority);
    for _ in 0..100 {
        if world.state().npcs[&2].health == 0 {
            break;
        }
        let command = world.state().casts.is_empty().then(|| f::cast(&world, 133));
        world
            .step(world.state().tick + 1, command, &mut authority)
            .unwrap();
    }
    assert_eq!(world.state().npcs[&2].health, 0);
    let previous = world.state().npcs[&2].id;
    let compact = world.checkpoint().unwrap();
    world.checkpoint_committed(&compact).unwrap();
    let mut reached = false;
    for _ in 0..50 {
        let before = world.checkpoint().unwrap();
        let records = world.records().to_vec();
        let tick = world.state().tick + 1;
        authority.fault = Fault::ExpiredInitialization;
        let result = world.step(tick, None, &mut authority);
        if let Some(rejected) = authority.rejected.take() {
            assert!(result.is_err());
            assert_eq!(world.checkpoint().unwrap(), before);
            assert_eq!(world.records(), records);
            assert_eq!(world.state().npcs[&2].id, previous);
            // Wet answers fail the same composed transaction and do not consume
            // generation/model/level or EventAI respawn state on a second retry.
            authority.fault = Fault::WetInitialization;
            assert!(world.step(tick, None, &mut authority).is_err());
            assert_eq!(world.checkpoint().unwrap(), before);
            assert_eq!(world.records(), records);
            assert_eq!(authority.rejected.take().unwrap(), rejected);
            authority.fault = Fault::None;
            world.step(tick, None, &mut authority).unwrap();
            assert_eq!(authority.initializations.last().unwrap(), &rejected);
            life::assert_query_order(&rejected, &world.last_tick().unwrap().draws);
            assert_eq!(
                world.state().npcs[&2].id.generation,
                previous.generation + 1
            );
            assert_owners(&world);
            let mut replay =
                NativeWorld::restore_synthetic_component_test(world.profile().clone(), compact)
                    .unwrap();
            replay.replay(world.records()).unwrap();
            assert_eq!(replay.checkpoint().unwrap(), world.checkpoint().unwrap());
            reached = true;
            break;
        }
        result.unwrap();
    }
    assert!(reached, "composed owner must reach respawn initialization");
}

#[test]
fn dry_idle_event_ai_rejects_random_movement_unowned_addons_and_missing_selection() {
    let (products, config) = products::authored_ground_height();
    let participants = NonRenderParticipants::authored_fixture(config.content, config.map);
    let snapshot = products
        .load_map_height(config, Some(&participants))
        .unwrap();
    let p = composed_profile(snapshot_binding(&snapshot), 10);
    for case in 0..6 {
        let mut input = visitors::visitor_input(&p, vec![2]);
        let SourceVisitorRows::Complete(owners) = &mut input.owners else {
            unreachable!()
        };
        owners[0] = p
            .source_visitors
            .as_ref()
            .unwrap()
            .owner(2)
            .unwrap()
            .clone();
        match case {
            0 => owners[0].default_movement_type = 1,
            1 => owners[0].addon_closure = SourceVisitorAddonClosure::Absent,
            2 => {
                owners[0].addon_closure = SourceVisitorAddonClosure::GenerationSource {
                    source_identity: [0; 32],
                }
            }
            3 => owners[0].entry_events = SourceVisitorRows::Unknown,
            4 => {
                owners[0].overrides =
                    SourceVisitorRows::Complete(vec![SourceVisitorOverride::OtherCreationHook])
            }
            _ => owners[0].ai_name.clear(),
        }
        let selected = SourceVisitorCatalog::validate_sources_with_event_ai(
            input,
            p.event_ai().unwrap().clone(),
        );
        if let Ok(selected) = selected {
            let mut changed = p.clone();
            changed.source_visitors = Some(selected);
            assert!(changed.validate().is_err(), "case {case}");
        }
    }
    let provider = SourceCreatureInitializationProvider::bind([8; 16], &p, snapshot).unwrap();
    let mut authority = ComposedAuthority::new(provider);
    authority.fault = Fault::WetInitialization;
    assert!(NativeWorld::synthetic_component_test_with_initialization(
        p,
        [8; 16],
        f::player(),
        7,
        None,
        &mut authority,
    )
    .is_err());
    assert!(authority.rejected.is_some());
}

#[test]
fn dry_idle_event_ai_reached_route_rejects_whole_combat_callback_frame() {
    let (products, config) = products::authored_ground_height();
    let participants = NonRenderParticipants::authored_fixture(config.content, config.map);
    let snapshot = products
        .load_map_height(config, Some(&participants))
        .unwrap();
    let mut p = composed_profile(snapshot_binding(&snapshot), 100);
    let spawn = p.npcs[0].ai.spawn;
    p.player.start =
        ai::Point::new(spawn.x.as_f32() + 8.0, spawn.y.as_f32(), spawn.z.as_f32()).unwrap();
    p.validate().unwrap();
    let provider = SourceCreatureInitializationProvider::bind([8; 16], &p, snapshot).unwrap();
    let mut authority = ComposedAuthority::new(provider);
    let mut world = NativeWorld::synthetic_component_test_with_initialization(
        p,
        [8; 16],
        f::player(),
        7,
        None,
        &mut authority,
    )
    .unwrap();
    let mut reached = false;
    for _ in 0..100 {
        let before = world.checkpoint().unwrap();
        let records = world.records().to_vec();
        let tick = world.state().tick + 1;
        let command = world.state().casts.is_empty().then(|| f::cast(&world, 133));
        let result = world.step(tick, command.clone(), &mut authority);
        if let Some(rejected) = authority.unsupported_motion.last().cloned() {
            assert_eq!(
                result.unwrap_err(),
                NativeError::UnsupportedMechanic("dry idle fixture has no route authority")
            );
            assert_eq!(world.checkpoint().unwrap(), before);
            assert_eq!(world.records(), records);
            assert_eq!(world.state().npcs[&2].ai.phase(), ai::Phase::Idle);
            assert!(
                authority.saw_audience,
                "rejection follows tentative AGGRO text"
            );
            let Query::SourceMotion(MotionQuery::NearPoint(near)) = &rejected.query else {
                panic!("initial chase must reach its near-point request first")
            };
            assert_eq!(near.searcher.npc(), world.state().npcs[&2].id);
            assert_eq!(near.searcher.inhabit_type(), 3);
            assert_eq!(
                near.searcher.allowed_position_z_branch().unwrap(),
                AllowedPositionZBranch::Swimming
            );
            let attempts = authority.unsupported_motion.len();
            assert_eq!(
                world.step(tick, command, &mut authority).unwrap_err(),
                NativeError::UnsupportedMechanic("dry idle fixture has no route authority")
            );
            assert_eq!(authority.unsupported_motion.len(), attempts + 1);
            assert_eq!(authority.unsupported_motion.last(), Some(&rejected));
            assert_eq!(world.checkpoint().unwrap(), before);
            assert_eq!(world.records(), records);
            reached = true;
            break;
        }
        result.unwrap();
    }
    assert!(
        reached,
        "out-of-reach combat must request unsupported habitat-3 motion"
    );
}
