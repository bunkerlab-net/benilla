// SPDX-License-Identifier: GPL-3.0-or-later
//! Dry initialization + ordinary addon + Defensive visitors + EventAI + selected
//! combat-entry Unit events. Receiver emptiness is explicitly authored; it is
//! never inferred from the singleton profile or the ordinary sight visitor.
use super::*;

fn assisted_profile(binding: CreatureInitializationSnapshot, delay_ms: u32) -> Profile {
    assisted_profile_with_chance(binding, delay_ms, 100)
}

fn assisted_profile_with_chance(
    binding: CreatureInitializationSnapshot,
    delay_ms: u32,
    chance: u32,
) -> Profile {
    let mut profile = composed_profile(binding, chance);
    let generation_source = profile
        .creature_generations
        .as_ref()
        .unwrap()
        .source(2)
        .unwrap()
        .identity();
    profile
        .initialize_combat_assistance(
            vec![CombatAssistanceSource {
                spawn_id: 2,
                entry: 6,
                generation_source,
                call_for_help: ai::Real::new(5.0).unwrap(),
                excluded_hooks: SourceVisitorRows::Complete(vec![]),
            }],
            delay_ms,
        )
        .unwrap();
    profile
}

#[derive(Clone, Copy)]
enum AssistanceFault {
    None,
    Missing,
    Unknown,
    Nonempty,
    Stale,
}
struct AssistedAuthority<'a> {
    dry: ComposedAuthority<'a>,
    queries: Vec<AuthorityRequest>,
    fault: AssistanceFault,
}
impl<'a> AssistedAuthority<'a> {
    fn new(provider: SourceCreatureInitializationProvider<'a>) -> Self {
        Self {
            dry: ComposedAuthority::new(provider),
            queries: vec![],
            fault: AssistanceFault::None,
        }
    }
}
impl Authority for AssistedAuthority<'_> {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if let Query::CombatAssistance(query) = &request.query {
            self.queries.push(request.clone());
            let mut echo = query.as_ref().clone();
            if matches!(self.fault, AssistanceFault::Stale) {
                echo.unit_clock_ms += 1;
            }
            let facts = match self.fault {
                AssistanceFault::Missing => CombatAssistanceFacts::Missing,
                AssistanceFault::Unknown => CombatAssistanceFacts::Unknown,
                AssistanceFault::Nonempty => CombatAssistanceFacts::NonemptyCandidates,
                _ => CombatAssistanceFacts::AuthoredCompleteEmptyTraversal {
                    request: Box::new(echo),
                    self_container: AssistanceSelfContainer::Grid,
                },
            };
            return Ok(AuthorityReply {
                request: request.clone(),
                facts: Facts::CombatAssistance(facts),
            });
        }
        self.dry.resolve(request)
    }
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        self.dry.observe_derived(reply)
    }
}
fn state(world: &NativeWorld) -> &CombatAssistanceState {
    world.state().npcs[&2].events.assistance().unwrap()
}

#[test]
fn dry_idle_assistance_executes_ordered_physical_unit_events_and_replays() {
    let (products, config) = products::authored_ground_height();
    let participants = NonRenderParticipants::authored_fixture(config.content, config.map);
    let snapshot = products
        .load_map_height(config, Some(&participants))
        .unwrap();
    let profile = assisted_profile(snapshot_binding(&snapshot), 2000);
    let provider = SourceCreatureInitializationProvider::bind([8; 16], &profile, snapshot).unwrap();
    let mut authority = AssistedAuthority::new(provider);
    let mut world = NativeWorld::synthetic_component_test_with_initialization(
        profile,
        [8; 16],
        f::player(),
        7,
        None,
        &mut authority,
    )
    .unwrap();
    let initial = world.checkpoint().unwrap();
    assert!(state(&world).can_check_for_help());
    assert!(!state(&world).already_called());
    let record = enter(&mut world, &mut authority);
    assert!(assert_evaluation(&world, &record, 100));
    assert_owners(&world);
    assert_eq!(authority.queries.len(), 1);
    assert!(!state(&world).can_check_for_help());
    assert!(state(&world).already_called());
    let payload = state(&world).pending().next().unwrap().clone();
    assert!(
        !payload.saved_already_called,
        "literal source prior latch, not its negation"
    );
    assert_eq!(payload.request.radius, ai::Real::new(5.0).unwrap());
    assert_eq!(payload.request.binding.npc, Some(world.state().npcs[&2].id));
    let event_ai_end = record
        .draws
        .iter()
        .filter(|draw| {
            matches!(
                draw.purpose,
                DrawPurpose::EventAiChance { .. } | DrawPurpose::EventAiAction { .. }
            )
        })
        .map(|draw| draw.first_counter + u64::from(draw.consumed))
        .max()
        .unwrap();
    assert_eq!(payload.request.rng_counter, event_ai_end);
    assert!(payload.request.event_ai_callback.is_some());
    assert_eq!(payload.deadline_ms, payload.request.unit_clock_ms + 2000);
    let queue = world.state().npcs[&2].events.queue_snapshot();
    let first = queue
        .queued
        .iter()
        .find(|e| e.event_id == payload.reenable_event)
        .unwrap();
    let second = queue
        .queued
        .iter()
        .find(|e| e.event_id == payload.linking_event)
        .unwrap();
    assert_eq!(first.exec_time_ms, second.exec_time_ms);
    assert!(first.queue.unwrap().insertion_id < second.queue.unwrap().insertion_id);
    for _ in 0..100 {
        if state(&world).pending().next().is_none() {
            break;
        }
        step(&mut world, &mut authority);
    }
    assert!(state(&world).pending().next().is_none());
    assert!(state(&world).can_check_for_help());
    let outcomes: Vec<_> = state(&world)
        .history()
        .iter()
        .filter_map(|entry| match entry {
            AssistanceHistory::Callback { outcome, .. } => Some(*outcome),
            _ => None,
        })
        .collect();
    assert_eq!(
        outcomes,
        [
            AssistanceOutcome::Reenabled,
            AssistanceOutcome::EmptyContinuation {
                call_assistance: false
            }
        ]
    );
    assert_no_routes(&authority.dry);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(world.profile().clone(), initial).unwrap();
    replay.replay(world.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), world.checkpoint().unwrap());
    let saved = world.checkpoint().unwrap();
    world.checkpoint_committed(&saved).unwrap();
    let mut restored =
        NativeWorld::restore_synthetic_component_test(world.profile().clone(), saved).unwrap();
    step(&mut restored, &mut authority);
    step(&mut world, &mut authority);
    assert_eq!(restored.checkpoint().unwrap(), world.checkpoint().unwrap());
}

#[test]
fn dry_idle_assistance_rejects_unproven_receivers_after_event_ai_atomically() {
    for fault in [
        AssistanceFault::Missing,
        AssistanceFault::Unknown,
        AssistanceFault::Nonempty,
        AssistanceFault::Stale,
    ] {
        let (products, config) = products::authored_ground_height();
        let participants = NonRenderParticipants::authored_fixture(config.content, config.map);
        let snapshot = products
            .load_map_height(config, Some(&participants))
            .unwrap();
        let profile = assisted_profile(snapshot_binding(&snapshot), 2000);
        let provider =
            SourceCreatureInitializationProvider::bind([8; 16], &profile, snapshot).unwrap();
        let mut authority = AssistedAuthority::new(provider);
        let mut world = NativeWorld::synthetic_component_test_with_initialization(
            profile,
            [8; 16],
            f::player(),
            7,
            None,
            &mut authority,
        )
        .unwrap();
        authority.fault = fault;
        let mut rejected = false;
        for _ in 0..100 {
            let before = world.checkpoint().unwrap();
            let records = world.records().to_vec();
            let command = world.state().casts.is_empty().then(|| f::cast(&world, 133));
            if world
                .step(world.state().tick + 1, command.clone(), &mut authority)
                .is_err()
            {
                assert_eq!(world.checkpoint().unwrap(), before);
                assert_eq!(world.records(), records);
                assert!(state(&world).can_check_for_help());
                assert!(!state(&world).already_called());
                assert!(state(&world).history().is_empty());
                assert!(
                    authority.dry.saw_audience,
                    "receiver request follows EventAI text"
                );
                let request = authority.queries.last().unwrap().clone();
                authority.fault = AssistanceFault::None;
                let record = world
                    .step(world.state().tick + 1, command, &mut authority)
                    .unwrap()
                    .clone();
                assert_eq!(authority.queries.last(), Some(&request));
                assert!(assert_evaluation(&world, &record, 100));
                assert_eq!(state(&world).next_occurrence(), 2);
                rejected = true;
                break;
            }
        }
        assert!(rejected, "must reach dedicated receiver proof");
    }
}

#[test]
fn dry_idle_assistance_pending_events_survive_corpse_dead_and_respawn() {
    let (products, config) = products::authored_ground_height();
    let participants = NonRenderParticipants::authored_fixture(config.content, config.map);
    let snapshot = products
        .load_map_height(config, Some(&participants))
        .unwrap();
    let profile = assisted_profile(snapshot_binding(&snapshot), 60_000);
    let provider = SourceCreatureInitializationProvider::bind([8; 16], &profile, snapshot).unwrap();
    let mut authority = AssistedAuthority::new(provider);
    let mut world = NativeWorld::synthetic_component_test_with_initialization(
        profile,
        [8; 16],
        f::player(),
        7,
        None,
        &mut authority,
    )
    .unwrap();
    let initial = world.checkpoint().unwrap();
    enter(&mut world, &mut authority);
    let old = world.state().npcs[&2].id;
    let payload = state(&world).pending().next().unwrap().clone();
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
    assert!(!state(&world).already_called());
    assert!(!state(&world).can_check_for_help());
    let mut saw_advance = false;
    let mut saw_freeze = false;
    for _ in 0..100 {
        if world.state().npcs[&2].id != old {
            break;
        }
        let clock = world.state().npcs[&2].events.clock_ms();
        step(&mut world, &mut authority);
        let next = world.state().npcs[&2].events.clock_ms();
        saw_advance |= next > clock;
        saw_freeze |= next == clock;
        assert_eq!(state(&world).pending().next(), Some(&payload));
    }
    assert!(saw_advance, "CORPSE advances the physical Unit clock");
    assert!(saw_freeze, "DEAD/respawn does not advance Unit events");
    assert_eq!(world.state().npcs[&2].id.generation, old.generation + 1);
    assert_eq!(payload.request.binding.npc, Some(old));
    assert!(!state(&world).can_check_for_help());
    assert!(!state(&world).already_called());
    assert_owners(&world);
    let queue = world.state().npcs[&2].events.queue_snapshot();
    assert!(queue
        .queued
        .iter()
        .any(|e| e.event_id == payload.reenable_event));
    assert!(queue
        .queued
        .iter()
        .any(|e| e.event_id == payload.linking_event));
    let mut replay =
        NativeWorld::restore_synthetic_component_test(world.profile().clone(), initial).unwrap();
    replay.replay(world.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), world.checkpoint().unwrap());
    assert_no_routes(&authority.dry);
}

#[test]
fn dry_idle_assistance_chance_ten_skip_still_owns_the_help_suffix() {
    let (products, config) = products::authored_ground_height();
    let participants = NonRenderParticipants::authored_fixture(config.content, config.map);
    let mut outcomes = std::collections::BTreeSet::new();
    for seed in 0..128 {
        let snapshot = products
            .load_map_height(config, Some(&participants))
            .unwrap();
        let profile = assisted_profile_with_chance(snapshot_binding(&snapshot), 2000, 10);
        let provider =
            SourceCreatureInitializationProvider::bind([8; 16], &profile, snapshot).unwrap();
        let mut authority = AssistedAuthority::new(provider);
        let mut world = NativeWorld::synthetic_component_test_with_initialization(
            profile,
            [8; 16],
            f::player(),
            seed,
            None,
            &mut authority,
        )
        .unwrap();
        let initial = world.checkpoint().unwrap();
        let record = enter(&mut world, &mut authority);
        let passed = assert_evaluation(&world, &record, 10);
        assert_eq!(authority.dry.text.queries.len(), usize::from(passed));
        assert_eq!(authority.queries.len(), 1);
        assert!(!state(&world).can_check_for_help());
        assert!(state(&world).already_called());
        let payload = state(&world).pending().next().unwrap();
        assert!(!payload.saved_already_called);
        assert!(payload.request.event_ai_callback.is_some());
        let last_draw = record
            .draws
            .iter()
            .filter(|draw| {
                matches!(
                    draw.purpose,
                    DrawPurpose::EventAiChance { .. } | DrawPurpose::EventAiAction { .. }
                )
            })
            .map(|draw| draw.first_counter + u64::from(draw.consumed))
            .max()
            .unwrap();
        assert_eq!(payload.request.rng_counter, last_draw);
        let mut replay =
            NativeWorld::restore_synthetic_component_test(world.profile().clone(), initial)
                .unwrap();
        replay.replay(world.records()).unwrap();
        assert_eq!(replay.checkpoint().unwrap(), world.checkpoint().unwrap());
        assert_no_routes(&authority.dry);
        outcomes.insert(passed);
        if outcomes.len() == 2 {
            break;
        }
    }
    assert_eq!(outcomes, std::collections::BTreeSet::from([false, true]));
}
