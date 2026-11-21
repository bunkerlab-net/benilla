// SPDX-License-Identifier: GPL-3.0-or-later
//! Real resident source initialization through lethal impact, corpse and respawn.
//! Authored combat-entry and sight exclusions keep the synthetic motion authority
//! from claiming habitat-3 route support; all initialization points use real bytes.
use super::*;
use benilla_sim::creature_init as init;
use std::collections::BTreeSet;

enum Attempt<'a> {
    Expired,
    FactsFor(&'a AuthorityRequest),
}
struct InitializationAttempt<'i, 'a, 't, 'b, 's> {
    inner: &'i mut ResidentAuthority<'a, 't, 'b, 's>,
    attempt: Attempt<'i>,
    observed: Option<AuthorityRequest>,
}
impl Authority for InitializationAttempt<'_, '_, '_, '_, '_> {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if !matches!(request.query, Query::CreatureInitialization(_)) {
            return self.inner.resolve(request);
        }
        self.observed = Some(request.clone());
        match self.attempt {
            Attempt::Expired => {
                assert!(matches!(
                    self.inner
                        .provider
                        .resolve(request, Instant::now(), || false),
                    Err(ResidentInitializationError::Context(Failure::Deadline))
                ));
                Err(NativeError::AuthorityUnavailable(
                    "expired resident initialization",
                ))
            }
            Attempt::FactsFor(other) => {
                // Requery the actual source for this internally valid transcript;
                // no stored NO_WATER fact is being replayed by the provider.
                let mut phases = Vec::new();
                let facts = self
                    .inner
                    .provider
                    .resolve_observed(other, deadline(), || false, |phase| phases.push(phase))
                    .unwrap();
                assert_eq!(phases, [0, 1]);
                let Facts::CreatureInitialization(observation) = &facts else {
                    unreachable!()
                };
                observation.validate_for(query(other)).unwrap();
                assert_ne!(observation.request, *query(request));
                // Preserve the exact current outer envelope, so rejection must
                // come from the generation owner's inner-request comparison.
                Ok(AuthorityReply {
                    request: request.clone(),
                    facts,
                })
            }
        }
    }
}
fn query(request: &AuthorityRequest) -> &CreatureInitializationRequest {
    let Query::CreatureInitialization(query) = &request.query else {
        unreachable!()
    };
    query
}
fn envelope(query: CreatureInitializationRequest) -> AuthorityRequest {
    AuthorityRequest {
        binding: query.binding.clone(),
        query: Query::CreatureInitialization(Box::new(query)),
    }
}
fn change_generation(query: &mut CreatureInitializationRequest, generation: u64) {
    let npc = query.binding.npc.as_mut().unwrap();
    npc.generation = generation;
    query.ticket = generation + 1;
    for draw in &mut query.model_gender_draws {
        match &mut draw.purpose {
            DrawPurpose::CreatureModel { npc: owner }
            | DrawPurpose::CreatureGender { npc: owner } => *owner = *npc,
            _ => unreachable!(),
        }
    }
}
fn noncurrent_requests(
    historical: &AuthorityRequest,
    current: &AuthorityRequest,
) -> Vec<(&'static str, AuthorityRequest)> {
    let live = query(current);
    let old = query(historical);
    assert!(old.binding.npc.unwrap().generation < live.binding.npc.unwrap().generation);
    assert!(old.first_counter < live.first_counter);

    let mut stale_generation = live.clone();
    change_generation(&mut stale_generation, old.binding.npc.unwrap().generation);

    let mut old_draws = old.clone();
    old_draws.binding = live.binding.clone();
    old_draws.now_ms = live.now_ms;
    change_generation(&mut old_draws, live.binding.npc.unwrap().generation);

    let mut other_clock = live.clone();
    other_clock.binding.tick += 1;
    // The caller fills the corresponding profile-clock value below.

    let mut other_counter = live.clone();
    other_counter.first_counter += 1;
    other_counter.before_level_counter += 1;
    for draw in &mut other_counter.model_gender_draws {
        draw.first_counter += 1;
    }

    let mut other_roll = live.clone();
    let model = &mut other_roll.model_gender_draws[0];
    assert!(matches!(model.purpose, DrawPurpose::CreatureModel { .. }));
    assert_eq!((model.low, model.high), (0, 1));
    // Both weighted rolls select the same authored root display. Equal selected
    // model fields do not make a different model draw the live RNG transcript.
    model.value = 1 - model.value;
    other_roll.model_roll = Some(model.value as u32);

    vec![
        ("historical exact request", historical.clone()),
        ("stale generation", envelope(stale_generation)),
        ("old model/gender draw history", envelope(old_draws)),
        ("changed clock", envelope(other_clock)),
        ("shifted draw counters", envelope(other_counter)),
        ("different equal-model roll", envelope(other_roll)),
    ]
}
fn assert_unchanged(
    world: &NativeWorld,
    checkpoint: &Checkpoint,
    records: &[TickRecord],
    rng: u64,
) {
    assert_eq!(&world.checkpoint().unwrap(), checkpoint);
    assert_eq!(world.records(), records);
    assert_eq!(world.state().rng_counter, rng);
}
fn assert_selection(world: &NativeWorld, request: &AuthorityRequest, draws: &[Draw]) {
    assert_order(world, request, draws);
    let query = query(request);
    let npc = &world.state().npcs[&2];
    let level = draws
        .iter()
        .find(|draw| {
            matches!(draw.purpose,
        DrawPurpose::CreatureLevel { npc: owner } if owner == npc.id)
        })
        .unwrap();
    let source = world
        .profile()
        .creature_generations
        .as_ref()
        .unwrap()
        .source(2)
        .unwrap();
    let (initialized, expected_selection) = init::prepare(&source.input)
        .unwrap()
        .resolve_with_model_receipt(init::SelectionRolls {
            model: query.model_roll,
            gender: query.gender_roll,
            level: Some(level.value as u32),
        })
        .unwrap();
    assert_eq!(npc.initialized_creature().unwrap(), &initialized);
    let facts = npc
        .creature_capabilities()
        .unwrap()
        .witness()
        .initialization()
        .unwrap();
    let selected = npc
        .creature_collision_width()
        .unwrap()
        .witness()
        .validated_model_selection(world.profile(), npc.id, Some(facts))
        .unwrap();
    assert_eq!(selected, expected_selection);
    assert_eq!(selected.native_display_id(), query.native_display_id);
    assert_eq!(selected.object_scale(), query.object_scale);
    assert_eq!(
        query.model_roll,
        Some(query.model_gender_draws[0].value as u32)
    );
    assert_eq!(
        query.gender_roll,
        Some(query.model_gender_draws[1].value as u32)
    );
}

#[test]
fn resident_respawn_requires_live_request_and_retries_exact_init_entry_order() {
    let install = Install::new(&shared_entries());
    let before_install = install.bytes();
    let terrain = terrain(&install);
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let height = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    let binding = height
        .initialization_binding(CONTENT, CONFIG, deadline(), || false)
        .unwrap();
    let mut p = runtime_profile(binding);
    p.source_motion
        .as_mut()
        .unwrap()
        .npcs
        .get_mut(&2)
        .unwrap()
        .npc_can_enter_combat = false;
    let AbilityAdmission::Available(spell) = &mut p.abilities[0].admission else {
        unreachable!()
    };
    spell.mana_cost = 0;
    let benilla_sim::classic_spell::MageEffect::SchoolDamage(damage) = &mut spell.effects[0] else {
        unreachable!()
    };
    damage.base_points = 1000;
    p.validate().unwrap();
    let mut responder = authority(&p, &height);
    responder.motion.critical_chance = Some(0);
    responder.motion.resist_chance = Some(0);
    responder.motion.visits = Some(false);
    let mut world = initialize(p, &mut responder).unwrap();
    let historical = responder.initializations[0].clone();
    assert_selection(&world, &historical, &initial_draws(&world));
    let mut heights = BTreeSet::from([query(&historical).collision_height.bits()]);
    let initial = world.checkpoint().unwrap();

    for _ in 0..16 {
        let old = world.state().npcs[&2].id;
        let command = f::cast(&world, 60001);
        world
            .step(world.state().tick + 1, Some(command), &mut responder)
            .unwrap();
        for _ in 0..50 {
            if world.state().npcs[&2].health == 0 {
                break;
            }
            world
                .step(world.state().tick + 1, None, &mut responder)
                .unwrap();
        }
        assert_eq!(world.state().npcs[&2].health, 0, "actual lethal impact");
        assert!(world
            .records()
            .iter()
            .flat_map(|r| &r.events)
            .any(|event| matches!(event,
            Event::Npc { id, effect: ai::Effect::Died } if *id == old)));

        let mut respawned = false;
        for _ in 0..50 {
            let checkpoint = world.checkpoint().unwrap();
            let records = world.records().to_vec();
            let rng = world.state().rng_counter;
            let tick = world.state().tick + 1;
            let mut expired = InitializationAttempt {
                inner: &mut responder,
                attempt: Attempt::Expired,
                observed: None,
            };
            let outcome = world.step(tick, None, &mut expired);
            let Some(current) = expired.observed else {
                outcome.unwrap();
                continue;
            };
            assert!(matches!(
                outcome,
                Err(NativeError::AuthorityUnavailable(
                    "expired resident initialization"
                ))
            ));
            assert_unchanged(&world, &checkpoint, &records, rng);
            assert!(records
                .iter()
                .flat_map(|r| &r.events)
                .any(|event| matches!(event,
                Event::Npc { id, effect: ai::Effect::CorpseRemoved } if *id == old)));
            let live = query(&current);
            assert_eq!(live.binding.npc.unwrap().generation, old.generation + 1);
            assert_eq!(live.binding.tick, tick);
            assert_eq!(
                live.now_ms,
                world.profile().clock_origin_ms + tick * u64::from(world.profile().tick_ms)
            );

            if old.generation == 0 {
                for (label, mut other) in noncurrent_requests(&historical, &current) {
                    let Query::CreatureInitialization(other_query) = &mut other.query else {
                        unreachable!()
                    };
                    other_query.now_ms = world.profile().clock_origin_ms
                        + other_query.binding.tick * u64::from(world.profile().tick_ms);
                    other_query.validate_for(world.profile()).unwrap();
                    assert_ne!(other, current, "{label}");
                    let mut substituted = InitializationAttempt {
                        inner: &mut responder,
                        attempt: Attempt::FactsFor(&other),
                        observed: None,
                    };
                    assert!(
                        matches!(
                            world.step(tick, None, &mut substituted),
                            Err(NativeError::AuthorityBinding)
                        ),
                        "{label}"
                    );
                    assert_eq!(substituted.observed.as_ref(), Some(&current), "{label}");
                    assert_unchanged(&world, &checkpoint, &records, rng);
                }
            }

            let accepted_record = world.step(tick, None, &mut responder).unwrap().clone();
            let accepted = responder.initializations.last().unwrap();
            assert_eq!(
                accepted, &current,
                "fresh retry keeps clock, model selection and RNG prefix"
            );
            assert_selection(&world, accepted, &accepted_record.draws);
            assert_eq!(world.state().npcs[&2].id.generation, old.generation + 1);
            assert!(world.state().npcs[&2].health > 0);
            assert!(accepted_record.events.iter().any(|event| matches!(event,
                Event::Npc { id, effect: ai::Effect::Respawned { .. } }
                    if *id == world.state().npcs[&2].id)));
            heights.insert(query(accepted).collision_height.bits());
            respawned = true;
            break;
        }
        assert!(
            respawned,
            "real corpse timers reach the resident initialization query"
        );
        if heights.len() == 2 {
            break;
        }
    }
    assert_eq!(
        heights,
        BTreeSet::from([(2.875f32 * 1.25).to_bits(), (4.125f32 * 1.25).to_bits()])
    );
    assert!(responder.motion.queries.iter().all(|request| !matches!(
        request.query,
        Query::SourceMotion(MotionQuery::NearPoint(_) | MotionQuery::Route(_))
    )));
    let mut replay =
        NativeWorld::restore_synthetic_component_test(world.profile().clone(), initial).unwrap();
    replay.replay(world.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), world.checkpoint().unwrap());
    assert_eq!(install.bytes(), before_install);
}
