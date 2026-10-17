// SPDX-License-Identifier: GPL-3.0-or-later
//! Seven independently owned entry-6-shaped dry actors. Factions, model rows,
//! poses and complete traversal order are authored component inputs, not a real
//! Northshire map load or client-asset verification.
use super::*;

#[path = "positive_help_tests.rs"]
pub(super) mod positive_help_tests;
#[path = "neighbor_visible_help_tests.rs"]
mod visible_help_tests;

const OWNERS: [u64; 7] = [2, 3, 4, 5, 6, 7, 8];
const UPDATE_ORDER: [u64; 7] = [6, 2, 8, 3, 7, 4, 5];
const COLD_ORDER: [u64; 7] = [8, 4, 2, 7, 3, 6, 5];

fn cluster_profile(binding: CreatureInitializationSnapshot) -> Profile {
    let p = profile_at_with_faction(
        binding,
        [77; 32],
        0.5f32 * (64.0 * 533.333_3),
        2,
        None,
        true,
    );
    let mut p = compose_owned_profile(p, 100, true);
    let sources = OWNERS
        .into_iter()
        .map(|spawn_id| CombatAssistanceSource {
            spawn_id,
            entry: 6,
            generation_source: p
                .creature_generations
                .as_ref()
                .unwrap()
                .source(spawn_id)
                .unwrap()
                .identity(),
            call_for_help: ai::Real::new(5.0).unwrap(),
            excluded_hooks: SourceVisitorRows::Complete(vec![]),
        })
        .collect();
    p.initialize_combat_assistance(sources, 2000).unwrap();
    p
}

pub(super) fn expand_cluster_owners_with_helper(mut p: Profile, helper: Option<u64>) -> Profile {
    let binding = match &p
        .creature_capabilities
        .as_ref()
        .unwrap()
        .source(2)
        .unwrap()
        .initialization
    {
        CreatureInitializationClosure::DryWater { snapshot, .. } => *snapshot,
        _ => unreachable!(),
    };
    let npc = p.npcs[0].clone();
    let generation = p
        .creature_generations
        .as_ref()
        .unwrap()
        .source(2)
        .unwrap()
        .clone();
    let capability = p
        .creature_capabilities
        .as_ref()
        .unwrap()
        .source(2)
        .unwrap()
        .clone();
    let selection = p
        .creature_collision_widths
        .as_ref()
        .unwrap()
        .source(2)
        .unwrap()
        .selection()
        .clone();
    let motion = p.source_motion.as_ref().unwrap().npcs[&2].clone();
    p.npcs.clear();
    p.source_motion.as_mut().unwrap().npcs.clear();
    let mut generations = vec![];
    let mut capabilities = vec![];
    let mut widths = vec![];
    for spawn in OWNERS {
        let mut n = npc.clone();
        n.ai.creature_id = spawn;
        // Same dry product tile, distinct physical positions and identities.
        n.ai.spawn.y = ai::Real::new(n.ai.spawn.y.as_f32() - (spawn - 2) as f32 * 0.25).unwrap();
        let mut g = generation.clone();
        g.spawn_id = spawn;
        g.input.spawn_id = spawn;
        if helper == Some(spawn) {
            // A separate authored source, never a hostile reinterpretation of
            // entry 6. Opposite the player, outside the original pull radius.
            n.entry = 990006;
            n.name = "Authored defensive helper".into();
            n.faction_template = 3;
            n.ai.spawn = ai::Point::new(
                npc.ai.spawn.x.as_f32() + 5.5,
                npc.ai.spawn.y.as_f32(),
                npc.ai.spawn.z.as_f32(),
            )
            .unwrap();
            g.spawn_entry = n.entry;
            g.input.template.entry = n.entry;
            g.input.template.faction = n.faction_template;
            let init::Rows::Complete(addons) = &mut g.input.template_addons else {
                unreachable!()
            };
            addons[0].entry = n.entry;
        }
        let mut c = capability.clone();
        c.spawn_id = spawn;
        c.spawn_entry = n.entry;
        let CreatureCapabilityRows::Complete(templates) = &mut c.templates else {
            unreachable!()
        };
        templates[0].entry = n.entry;
        c.addon_closure = CreatureCapabilityAddonClosure::GenerationSource {
            source_identity: g.identity(),
        };
        c.initialization = CreatureInitializationClosure::DryWater {
            map: binding.map,
            respawn_position: n.ai.spawn,
            snapshot: binding,
        };
        let mut m = motion.clone();
        m.npc_policy.home.addon_closure = HomeAddonClosure::GenerationSource {
            source_identity: g.identity(),
        };
        p.source_motion.as_mut().unwrap().npcs.insert(spawn, m);
        generations.push(g);
        capabilities.push(c);
        widths.push(
            CreatureCollisionWidthSource::from_initial_model(
                spawn,
                if helper == Some(spawn) {
                    init::prepare(&generations.last().unwrap().input)
                        .unwrap()
                        .resolve_with_model_receipt(init::SelectionRolls {
                            model: Some(0),
                            gender: Some(0),
                            level: Some(1),
                        })
                        .unwrap()
                        .1
                } else {
                    selection.clone()
                },
                &models::catalog_with_model_heights(2.875, 4.125),
            )
            .unwrap(),
        );
        p.npcs.push(n);
    }
    p.creature_generations = Some(
        CreatureGenerationCatalog::validate_sources(
            init::Rows::Complete(generations),
            COLD_ORDER.to_vec(),
        )
        .unwrap(),
    );
    p.creature_capabilities = Some(
        CreatureCapabilityCatalog::validate_sources(CreatureCapabilityRows::Complete(capabilities))
            .unwrap(),
    );
    p.creature_collision_widths = Some(
        CreatureCollisionWidthCatalog::validate_sources(CreatureCollisionWidthRows::Complete(
            widths,
        ))
        .unwrap(),
    );
    if helper.is_some() {
        // Rebuild entry-keyed catalogs from this new authored source set. The
        // public initializers validate their entire profile, so use a separate
        // non-generation catalog-construction draft until the mutually bound
        // real visitor/height owners below are ready. This draft never executes
        // or contributes capability/geometry evidence to the resulting world.
        let mut draft = p.clone();
        draft.interactions = None;
        draft.source_visitors = None;
        draft.source_motion = None;
        draft.creature_generations = None;
        draft.creature_capabilities = None;
        draft.creature_collision_widths = None;
        draft.creature_collision_heights = None;
        let mut input = combat::source_rows(&p);
        input.npcs.sort_by_key(|row| row.entry);
        input.npcs.dedup_by_key(|row| row.entry);
        input
            .factions
            .iter_mut()
            .find(|row| row.template_id == 2)
            .unwrap()
            .enemy_group_mask = 0;
        let mut hostile = input
            .factions
            .iter()
            .find(|row| row.template_id == 2)
            .unwrap()
            .clone();
        hostile.template_id = 3;
        hostile.faction = 990006;
        hostile.enemy_group_mask = 1;
        input.factions.push(hostile);
        draft.initialize_interactions(input).unwrap();
        draft.source_motion = p.source_motion.clone();
        for owner in draft.source_motion.as_mut().unwrap().npcs.values_mut() {
            owner.npc_policy.home.addon_closure = HomeAddonClosure::Absent;
        }
        draft.creature_capabilities = Some(
            CreatureCapabilityCatalog::validate_sources(motion::creature_capability_sources(
                &draft,
            ))
            .unwrap(),
        );
        draft.source_visitors = Some(
            SourceVisitorCatalog::validate_sources(visitors::visitor_input(
                &draft,
                UPDATE_ORDER.to_vec(),
            ))
            .unwrap(),
        );
        draft
            .initialize_combat_authority(
                [6, 990006]
                    .map(|entry| NpcCombatAuthoritySource {
                        entry,
                        extra_flags: 2,
                        static_flags1: 0,
                        static_flags2: 0,
                        static_flags3: 0,
                        static_flags4: 0,
                    })
                    .into(),
            )
            .unwrap();
        p.interactions = draft.interactions;
    }
    let mut input = visitors::visitor_input(&p, UPDATE_ORDER.to_vec());
    let SourceVisitorRows::Complete(rows) = &mut input.owners else {
        unreachable!()
    };
    for row in rows {
        row.addon_closure = SourceVisitorAddonClosure::GenerationSource {
            source_identity: p
                .creature_generations
                .as_ref()
                .unwrap()
                .source(row.spawn_id)
                .unwrap()
                .identity(),
        };
    }
    p.source_visitors = Some(SourceVisitorCatalog::validate_sources(input).unwrap());
    p.creature_collision_heights = None;
    p.initialize_creature_collision_heights().unwrap();
    p
}

#[derive(Clone, Copy, Default)]
enum TraversalFault {
    #[default]
    None,
    Missing,
    Duplicate,
    Omitted,
    StaleGeneration,
}
struct ClusterAuthority<'a> {
    dry: ComposedAuthority<'a>,
    assistance_queries: Vec<AuthorityRequest>,
    traversal_fault: TraversalFault,
    fail_initialization: Option<u64>,
}
impl<'a> ClusterAuthority<'a> {
    fn new(provider: SourceCreatureInitializationProvider<'a>) -> Self {
        let mut dry = ComposedAuthority::new(provider);
        dry.text.base.grid_order = Some(vec![7, 4, 8, 2, 5, 3, 6]);
        Self {
            dry,
            assistance_queries: vec![],
            traversal_fault: TraversalFault::None,
            fail_initialization: None,
        }
    }
}
impl Authority for ClusterAuthority<'_> {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if matches!(request.query, Query::CreatureInitialization(_))
            && request
                .binding
                .npc
                .is_some_and(|id| Some(id.spawn) == self.fail_initialization)
        {
            return Err(NativeError::AuthorityUnavailable(
                "authored late cluster initialization",
            ));
        }
        if let Query::CombatAssistance(query) = &request.query {
            self.assistance_queries.push(request.clone());
            let id = |spawn| {
                *query
                    .scene_inventory
                    .iter()
                    .find(|id| id.spawn == spawn)
                    .unwrap()
            };
            let mut grid = [7, 4, 2, 5].map(id).to_vec();
            let world = [8, 3, 6].map(id).to_vec();
            match self.traversal_fault {
                TraversalFault::Duplicate => grid.push(grid[0]),
                TraversalFault::Omitted => {
                    grid.pop();
                }
                TraversalFault::StaleGeneration => grid[0].generation += 1,
                _ => {}
            }
            let facts = if matches!(self.traversal_fault, TraversalFault::Missing) {
                CombatAssistanceFacts::Missing
            } else {
                CombatAssistanceFacts::AuthoredCompleteTraversal {
                    request: query.clone(),
                    grid,
                    world,
                    outside_coverage: vec![],
                }
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

#[test]
fn seven_same_entry_dry_owners_initialize_independently_and_replay() {
    let (products, config) = products::authored_ground_height();
    let participants = NonRenderParticipants::authored_fixture(config.content, config.map);
    let snapshot = products
        .load_map_height(config, Some(&participants))
        .unwrap();
    let mut p = cluster_profile(snapshot_binding(&snapshot));
    let before = serde_json::to_vec(&p).unwrap();
    let contradictory = OWNERS
        .into_iter()
        .map(|spawn_id| CombatAssistanceSource {
            spawn_id,
            entry: 6,
            generation_source: p
                .creature_generations
                .as_ref()
                .unwrap()
                .source(spawn_id)
                .unwrap()
                .identity(),
            call_for_help: ai::Real::new(if spawn_id == 8 { 6.0 } else { 5.0 }).unwrap(),
            excluded_hooks: SourceVisitorRows::Complete(vec![]),
        })
        .collect();
    assert!(p.initialize_combat_assistance(contradictory, 2000).is_err());
    assert_eq!(serde_json::to_vec(&p).unwrap(), before);
    let provider = SourceCreatureInitializationProvider::bind([8; 16], &p, snapshot).unwrap();
    let mut authority = ClusterAuthority::new(provider);
    let mut world = NativeWorld::synthetic_component_test_with_initialization(
        p.clone(),
        [8; 16],
        f::player(),
        7,
        None,
        &mut authority,
    )
    .unwrap();
    assert_eq!(
        authority
            .dry
            .initializations
            .iter()
            .map(|r| r.binding.npc.unwrap().spawn)
            .collect::<Vec<_>>(),
        COLD_ORDER
    );
    assert_eq!(world.state().npcs.len(), 7);
    for spawn in OWNERS {
        let npc = &world.state().npcs[&spawn];
        assert_eq!(npc.initialized_creature().unwrap().entry, 6);
        assert_eq!(npc.initialized_creature().unwrap().unit_bytes_2, 0x1001);
        assert!(npc
            .creature_capabilities()
            .unwrap()
            .witness()
            .capabilities()
            .unwrap()
            .can_swim());
        assert_eq!(npc.id.spawn, spawn);
    }
    for _ in 0..4 {
        step(&mut world, &mut authority);
    }
    let initial = world.checkpoint().unwrap();
    let record = enter(&mut world, &mut authority);
    assert_evaluation(&world, &record, 100);
    assert_eq!(authority.assistance_queries.len(), 1);
    assert_eq!(
        world.state().npcs[&2]
            .events
            .assistance()
            .unwrap()
            .pending()
            .count(),
        1
    );
    let payload = world.state().npcs[&2]
        .events
        .assistance()
        .unwrap()
        .pending()
        .next()
        .unwrap();
    let traversal = payload.traversal.as_ref().unwrap();
    assert_eq!(
        traversal
            .candidates
            .iter()
            .map(|candidate| candidate.npc.spawn)
            .collect::<Vec<_>>(),
        [7, 4, 2, 5, 8, 3, 6]
    );
    for candidate in &traversal.candidates {
        assert_eq!(
            candidate.denial,
            if candidate.npc.spawn == 2 {
                AssistanceDenial::SelfCandidate
            } else {
                AssistanceDenial::CannotAttackEnemyOnSight
            }
        );
    }
    for spawn in OWNERS.into_iter().filter(|spawn| *spawn != 2) {
        let npc = &world.state().npcs[&spawn];
        assert!(!npc.events.assistance().unwrap().already_called());
        assert_eq!(npc.events.assistance().unwrap().pending().count(), 0);
    }
    super::neighbor_checkpoint_tests::assert_mutations_rejected(&world);
    for _ in 0..100 {
        if world.state().npcs[&2]
            .events
            .assistance()
            .unwrap()
            .pending()
            .next()
            .is_none()
        {
            break;
        }
        step(&mut world, &mut authority);
    }
    assert!(world.state().npcs[&2]
        .events
        .assistance()
        .unwrap()
        .pending()
        .next()
        .is_none());
    // Change the neighbor through an actual projectile, not checkpoint surgery.
    // Owner 2's retained denial remains entry-time history after this transition.
    let mut second_entered = false;
    for _ in 0..100 {
        let command = world.state().casts.is_empty().then(|| {
            let mut cast = f::cast(&world, 133);
            cast.input.command = Command::Cast {
                ability: 133,
                target: world.state().npcs[&3].id,
            };
            cast
        });
        let tick = world.state().tick + 1;
        let record = world.step(tick, command, &mut authority).unwrap();
        if record.events.iter().any(|event| {
            matches!(event,
                Event::EventAiEvaluation(value) if value.callback.npc.spawn == 3
            )
        }) {
            second_entered = true;
            break;
        }
    }
    assert!(second_entered);
    assert_eq!(world.state().npcs[&3].ai.phase(), ai::Phase::Combat);
    let second = world.state().npcs[&3]
        .events
        .assistance()
        .unwrap()
        .pending()
        .next()
        .unwrap();
    assert!(second
        .traversal
        .as_ref()
        .unwrap()
        .candidates
        .iter()
        .any(|candidate| candidate.npc.spawn == 2
            && candidate.denial == AssistanceDenial::AlreadyInCombat));
    let restored =
        NativeWorld::restore_synthetic_component_test(p.clone(), world.checkpoint().unwrap())
            .unwrap();
    assert_eq!(restored.state_digest(), world.state_digest());
    // Replay the entire post-checkpoint journal, retaining recorded traversal
    // order rather than querying current map membership again.
    let mut replay = NativeWorld::restore_synthetic_component_test(p, initial).unwrap();
    for tick in world
        .records()
        .iter()
        .filter(|r| r.tick > replay.state().tick)
        .cloned()
        .collect::<Vec<_>>()
    {
        replay.replay(&[tick]).unwrap();
    }
    assert_eq!(replay.state_digest(), world.state_digest());
    assert!(authority.dry.unsupported_motion.is_empty());
}

#[test]
fn late_cluster_traversal_failure_rolls_back_text_rng_and_all_owners() {
    for fault in [
        TraversalFault::Missing,
        TraversalFault::Duplicate,
        TraversalFault::Omitted,
        TraversalFault::StaleGeneration,
    ] {
        let (products, config) = products::authored_ground_height();
        let participants = NonRenderParticipants::authored_fixture(config.content, config.map);
        let snapshot = products
            .load_map_height(config, Some(&participants))
            .unwrap();
        let p = cluster_profile(snapshot_binding(&snapshot));
        let provider = SourceCreatureInitializationProvider::bind([8; 16], &p, snapshot).unwrap();
        let mut authority = ClusterAuthority::new(provider);
        let mut world = NativeWorld::synthetic_component_test_with_initialization(
            p.clone(),
            [8; 16],
            f::player(),
            7,
            None,
            &mut authority,
        )
        .unwrap();
        for _ in 0..4 {
            step(&mut world, &mut authority);
        }
        authority.traversal_fault = fault;
        let mut rejected = false;
        for _ in 0..100 {
            let before = world.checkpoint().unwrap();
            let before_bytes = before.encode().unwrap();
            let command = world.state().casts.is_empty().then(|| f::cast(&world, 133));
            let tick = world.state().tick + 1;
            if world.step(tick, command.clone(), &mut authority).is_err() {
                assert!(!authority.assistance_queries.is_empty());
                assert!(authority.dry.saw_audience);
                assert_eq!(world.checkpoint().unwrap().encode().unwrap(), before_bytes);
                authority.traversal_fault = TraversalFault::None;
                let actual = world
                    .step(tick, command.clone(), &mut authority)
                    .unwrap()
                    .clone();
                let mut expected =
                    NativeWorld::restore_synthetic_component_test(p.clone(), before).unwrap();
                let wanted = expected
                    .step(tick, command, &mut authority)
                    .unwrap()
                    .clone();
                assert_eq!(actual, wanted);
                assert_eq!(world.state_digest(), expected.state_digest());
                rejected = true;
                break;
            }
        }
        assert!(
            rejected,
            "fault must reach post-EventAI assistance traversal"
        );
    }
}

#[test]
fn seventh_dry_initialization_failure_does_not_consume_successful_retry_draws() {
    let (products, config) = products::authored_ground_height();
    let participants = NonRenderParticipants::authored_fixture(config.content, config.map);
    let snapshot = products
        .load_map_height(config, Some(&participants))
        .unwrap();
    let p = cluster_profile(snapshot_binding(&snapshot));
    let provider = SourceCreatureInitializationProvider::bind([8; 16], &p, snapshot).unwrap();
    let mut authority = ClusterAuthority::new(provider);
    authority.fail_initialization = Some(*COLD_ORDER.last().unwrap());
    assert!(NativeWorld::synthetic_component_test_with_initialization(
        p.clone(),
        [8; 16],
        f::player(),
        7,
        None,
        &mut authority,
    )
    .is_err());
    assert_eq!(authority.dry.initializations.len(), 6);
    authority.fail_initialization = None;
    authority.dry.initializations.clear();
    let retry = NativeWorld::synthetic_component_test_with_initialization(
        p.clone(),
        [8; 16],
        f::player(),
        7,
        None,
        &mut authority,
    )
    .unwrap();
    let expected = NativeWorld::synthetic_component_test_with_initialization(
        p,
        [8; 16],
        f::player(),
        7,
        None,
        &mut authority,
    )
    .unwrap();
    assert_eq!(
        retry.checkpoint().unwrap().encode().unwrap(),
        expected.checkpoint().unwrap().encode().unwrap()
    );
}
