// SPDX-License-Identifier: GPL-3.0-or-later
//! Real initialized dry owners execute initial-notify peer help and later combat.
//! Entry 990006 and spatial membership are authored component inputs; entry 6
//! remains neutral-shaped. No loaded map traversal or client-content admission.
use super::*;

const HELPER: u64 = 8;
const HELPER_EVENT: u32 = text::EVENT + 1;

fn helper_profile(binding: CreatureInitializationSnapshot, notify_ms: u32) -> Profile {
    let p = profile_at_with_faction(
        binding,
        [77; 32],
        0.5f32 * (64.0 * 533.333_3),
        2,
        None,
        true,
    );
    let mut p = compose_owned_profile_with_helper(p, 100, true, Some(HELPER));
    let sources = p
        .npcs
        .iter()
        .map(|npc| CombatAssistanceSource {
            spawn_id: npc.ai.creature_id,
            entry: npc.entry,
            generation_source: p
                .creature_generations
                .as_ref()
                .unwrap()
                .source(npc.ai.creature_id)
                .unwrap()
                .identity(),
            // Source pull radius is independent of the CheckForHelp config.
            call_for_help: f::real(if npc.entry == 6 { 0.01 } else { 5.0 }),
            excluded_hooks: SourceVisitorRows::Complete(vec![]),
        })
        .collect();
    p.initialize_combat_assistance(sources, 2000).unwrap();
    p.initialize_peer_help_radius(f::real(5.0)).unwrap();
    p.source_motion
        .as_mut()
        .unwrap()
        .creature_initial_aggro_delay_ms = notify_ms;
    p.validate().unwrap();
    p
}

fn peer(reply: &AuthorityReply) -> Option<(&VisitorSightRequest, &PeerSightReceipt)> {
    match (&reply.request.query, &reply.facts) {
        (
            Query::SourceMotion(MotionQuery::VisitorSight(request)),
            Facts::SourceMotion(MotionFacts::VisitorSight(SightDisposition::Peer(receipt))),
        ) => Some((request, receipt)),
        _ => None,
    }
}

fn entered_payload(world: &NativeWorld, spawn: u64) -> &AssistancePayload {
    world.state().npcs[&spawn]
        .events
        .assistance()
        .unwrap()
        .history()
        .iter()
        .find_map(|entry| match entry {
            AssistanceHistory::Enter(payload) => Some(payload.as_ref()),
            _ => None,
        })
        .unwrap()
}

fn assert_helper_owners(world: &NativeWorld) {
    let helper = &world.state().npcs[&HELPER];
    assert_eq!(helper.initialized_creature().unwrap().entry, 990006);
    assert_eq!(helper.initialized_creature().unwrap().faction, 3);
    assert_eq!(
        world.profile().npc(HELPER).unwrap().ai.reaction,
        ai::Reaction::Defensive
    );
    let capability = helper.creature_capabilities().unwrap().witness();
    assert_eq!(capability.inhabit_type(), 3);
    assert_eq!(capability.swimming_movement_flags(), 0);
    let dry = capability.initialization().unwrap();
    assert_eq!(dry.status, environment::LiquidStatus::NoWater);
    assert_eq!(dry.request.binding.npc, Some(helper.id));
    assert_eq!(
        dry.request.collision_height,
        helper
            .creature_collision_height()
            .unwrap()
            .witness()
            .height()
    );
    for spawn in OWNERS.into_iter().filter(|spawn| *spawn != HELPER) {
        let npc = &world.state().npcs[&spawn];
        assert_eq!(npc.initialized_creature().unwrap().entry, 6);
        assert_eq!(npc.initialized_creature().unwrap().faction, 2);
    }
}

fn assert_join(world: &NativeWorld, record: &TickRecord, notify_ms: u64) {
    let joined: Vec<_> = record
        .replies
        .iter()
        .filter_map(peer)
        .filter(|(_, receipt)| receipt.joined_combat())
        .collect();
    assert_eq!(joined.len(), 1);
    let (request, receipt) = joined[0];
    let helper = &world.state().npcs[&HELPER];
    assert_eq!(
        request.notification_owner,
        VisitorParticipant::Creature(helper.id)
    );
    assert_eq!(request.observer, VisitorParticipant::Creature(helper.id));
    assert_eq!(
        request.target,
        VisitorParticipant::Creature(world.state().npcs[&2].id)
    );
    assert_eq!(request.direction, VisitorDirection::OwnerToVisited);
    assert_eq!(request.callback.due.event_id, 1);
    assert_eq!(request.callback.due.scheduled_ms, notify_ms);
    assert_eq!(request.callback.due.now_ms, notify_ms);
    assert!(receipt.assistance.is_some());
    assert_eq!(helper.ai.phase(), ai::Phase::Combat);
    assert_eq!(helper.ai.target(), Some(1));
    assert!(helper.events.can_aggro());
    assert!(helper.events.notify_handle().is_none());
    let payload = entered_payload(world, HELPER);
    assert!(matches!(
        payload.request.callback,
        EventAiCallbackKind::PeerHelp { .. }
    ));
    assert_eq!(payload.request.unit_clock_ms, notify_ms);
    assert_eq!(payload.deadline_ms, notify_ms + 2000);
    let traversal = payload.traversal.as_ref().unwrap();
    assert_eq!(traversal.candidates.len(), 7);
    for candidate in &traversal.candidates {
        assert_eq!(
            candidate.denial,
            if candidate.npc.spawn == HELPER {
                AssistanceDenial::SelfCandidate
            } else if candidate.npc.spawn == 2 {
                AssistanceDenial::AlreadyInCombat
            } else {
                AssistanceDenial::CannotAttackEnemyOnSight
            }
        );
    }
    let queue = helper.events.queue_snapshot();
    let reenable = queue
        .queued
        .iter()
        .find(|event| event.event_id == payload.reenable_event)
        .unwrap();
    let linking = queue
        .queued
        .iter()
        .find(|event| event.event_id == payload.linking_event)
        .unwrap();
    assert_eq!(
        (reenable.exec_time_ms, linking.exec_time_ms),
        (payload.deadline_ms, payload.deadline_ms)
    );
    assert!(reenable.queue.unwrap().insertion_id < linking.queue.unwrap().insertion_id);
    assert!(!helper.events.assistance().unwrap().can_check_for_help());
    assert!(helper.events.assistance().unwrap().already_called());
    assert_eq!(record.events.iter().filter(|event| matches!(event, Event::Npc { id, effect: ai::Effect::CombatStarted { target: 1 } } if id.spawn == HELPER)).count(), 1);
    let evaluation = record
        .events
        .iter()
        .find_map(|event| match event {
            Event::EventAiEvaluation(value) if value.callback.npc.spawn == HELPER => Some(value),
            _ => None,
        })
        .unwrap();
    assert!(
        matches!(&evaluation.callback.kind, EventAiCallbackKind::PeerHelp { request: cause, receipt: proof } if cause.as_ref() == request && Some(proof.as_ref()) == receipt.assistance.as_deref())
    );
    assert_eq!(evaluation.occurrence, 0);
    assert_eq!(evaluation.actions.len(), 1);
    assert_eq!(evaluation.actions[0].event_id, HELPER_EVENT);
    assert!(evaluation.actions[0].speech.is_some());
    assert_eq!(
        payload.request.event_ai_callback,
        Some(evaluation.callback.digest())
    );
    assert_eq!(record.draws.iter().filter(|draw| matches!(draw.purpose, DrawPurpose::EventAiChance { npc, event_id: HELPER_EVENT, .. } | DrawPurpose::EventAiAction { npc, event_id: HELPER_EVENT, .. } if npc == helper.id)).count(), 2);
    for spawn in OWNERS
        .into_iter()
        .filter(|spawn| ![2, HELPER].contains(spawn))
    {
        assert_eq!(world.state().npcs[&spawn].ai.phase(), ai::Phase::Idle);
        assert!(world.state().npcs[&spawn].ai.target().is_none());
    }
}

use super::super::super::super::super::{
    core::{Body, PlayerMotion, Session},
    products::vmap::VmapSnapshot,
    source_vmap::{geometry_revision, SourceVmapProvider},
};
use benilla_world_runtime::native::synthetic_player_model_fixture as player_models;

struct HelperAuthority<'a> {
    cluster: ClusterAuthority<'a>,
    profile: Profile,
    session: Session,
    vmap: SourceVmapProvider,
    geometry_queries: Vec<AuthorityRequest>,
}
impl Authority for HelperAuthority<'_> {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if let Query::SourceMotion(MotionQuery::PeerHelp(query)) = &request.query {
            self.geometry_queries.push(request.clone());
            let facts = match query.as_ref() {
                PeerHelpQuery::PeerLineOfSight(_) => self
                    .vmap
                    .peer_help_line_of_sight(&self.session, &self.profile, request, deadline())
                    .map_err(|_| NativeError::AuthorityUnavailable("loaded helper peer LOS"))?,
                PeerHelpQuery::VictimVisibility(_) | PeerHelpQuery::PeerRange(_) => {
                    // Explicit component closures for false-detect visibility and
                    // common map membership. Neither claims loaded traversal.
                    Facts::SourceMotion(MotionFacts::PeerHelp(PeerHelpFacts {
                        query: query.clone(),
                        check: ai::movement::Check::Passed,
                    }))
                }
            };
            return Ok(AuthorityReply {
                request: request.clone(),
                facts,
            });
        }
        self.cluster.resolve(request)
    }
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        self.cluster.observe_derived(reply)
    }
}

fn helper_world<'a>(
    snapshot: MapHeightSnapshot<'a>,
    vmap: VmapSnapshot,
    notify_ms: u32,
) -> (NativeWorld, HelperAuthority<'a>) {
    let mut profile = helper_profile(snapshot_binding(&snapshot), notify_ms);
    profile.geometry_revision = geometry_revision([77; 32], vmap.binding());
    profile
        .initialize_player_collision_height(
            &player_models::catalog(),
            &models::catalog_with_height_fields(1.375, 2.0, 1.0, 1.0, false),
        )
        .unwrap();
    let environment = products::authored_player_environment(profile.map_id, profile.content_digest);
    profile
        .initialize_player_environment(
            environment.environment_binding(),
            environment.environment_catalog().clone(),
        )
        .unwrap();
    // No SetPosition callback has occurred: source initial cache flags are zero.
    // This selects actual cache ownership, not a fabricated dry-position receipt.
    profile.validate().unwrap();
    let session = Session {
        world: [8; 16],
        profile: profile.digest(),
        policy: profile.authority_policy,
        content: profile.content_digest,
        geometry: profile.geometry_revision,
        map: profile.map_id,
        bodies: profile
            .npcs
            .iter()
            .map(|npc| {
                (
                    npc.ai.creature_id,
                    Body {
                        radius: npc.ai.bounding_radius,
                        height: profile
                            .npc_motion(npc.ai.creature_id)
                            .unwrap()
                            .npc_policy
                            .settings
                            .collision_height,
                    },
                )
            })
            .collect(),
        player: Some(PlayerMotion {
            body: Body {
                radius: profile.player.bounding_radius,
                height: profile
                    .source_motion
                    .as_ref()
                    .unwrap()
                    .player_collision_height,
            },
            speed: f::real(7.0),
        }),
        participants: Some(NonRenderParticipants::authored_fixture(
            profile.content_digest,
            profile.map_id,
        )),
        static_los: None,
        primary_near_point: None,
        empty_selector_near_point: None,
        player_environment: None,
        source_location: None,
        same_polygon_route: None,
    };
    let vmap = SourceVmapProvider::bind(&session, vmap, [77; 32]).unwrap();
    let provider = SourceCreatureInitializationProvider::bind([8; 16], &profile, snapshot).unwrap();
    let mut authority = HelperAuthority {
        cluster: ClusterAuthority::new(provider),
        profile: profile.clone(),
        session,
        vmap,
        geometry_queries: vec![],
    };
    let world = NativeWorld::synthetic_component_test_with_initialization(
        profile,
        [8; 16],
        f::player(),
        7,
        None,
        &mut authority,
    )
    .unwrap();
    (world, authority)
}

#[test]
fn authored_hostile_helper_initial_notify_joins_then_melees_dies_and_respawns_with_replay() {
    let (products, config) = products::authored_ground_height();
    let participants = NonRenderParticipants::authored_fixture(config.content, config.map);
    let load = |notify| {
        helper_world(
            products
                .load_map_height(config, Some(&participants))
                .unwrap(),
            products.load_static_vmap().unwrap(),
            notify,
        )
    };
    let (mut calibration, mut authority) = load(10_000);
    enter(&mut calibration, &mut authority);
    let deadline = entered_payload(&calibration, 2).deadline_ms;
    let tick_ms = u64::from(calibration.profile().tick_ms);
    let notify = deadline + tick_ms;
    let (mut world, mut authority) = load(notify.try_into().unwrap());
    let initial = world.checkpoint().unwrap();
    assert_helper_owners(&world);
    assert_eq!(authority.cluster.dry.initializations.len(), 7);
    enter(&mut world, &mut authority);
    assert_eq!(entered_payload(&world, 2).deadline_ms, deadline);
    assert_eq!(
        entered_payload(&world, 2)
            .traversal
            .as_ref()
            .unwrap()
            .candidates
            .iter()
            .find(|candidate| candidate.npc.spawn == HELPER)
            .unwrap()
            .denial,
        AssistanceDenial::OutOfRange
    );
    assert_eq!(world.state().npcs[&HELPER].ai.phase(), ai::Phase::Idle);
    let notify_handle = world.state().npcs[&HELPER].events.notify_handle().unwrap();
    assert_eq!(
        world.state().npcs[&HELPER]
            .events
            .event(notify_handle)
            .unwrap()
            .exec_time_ms,
        notify
    );
    while world.state().npcs[&HELPER].events.clock_ms() + tick_ms < notify {
        step(&mut world, &mut authority);
    }
    assert!(world.state().npcs[&2]
        .events
        .assistance()
        .unwrap()
        .can_check_for_help());
    let before = world.checkpoint().unwrap();
    let before_records = world.records().to_vec();
    authority.cluster.dry.text.base.derived.clear();
    authority.cluster.dry.text.base.fail_derived_owner = Some(5);
    assert_eq!(
        world
            .step(world.state().tick + 1, None, &mut authority)
            .unwrap_err(),
        NativeError::AuthorityUnavailable("authored late owner derived receipt fault")
    );
    assert!(authority
        .cluster
        .dry
        .text
        .base
        .derived
        .iter()
        .filter_map(peer)
        .any(|(_, receipt)| receipt.joined_combat()));
    assert_eq!(world.checkpoint().unwrap(), before);
    assert_eq!(world.records(), before_records);
    authority.cluster.dry.text.base.fail_derived_owner = None;
    step(&mut world, &mut authority);
    let joined = world.last_tick().unwrap().clone();
    assert_join(&world, &joined, notify);
    let mut retry =
        NativeWorld::restore_synthetic_component_test(world.profile().clone(), before.clone())
            .unwrap();
    step(&mut retry, &mut authority);
    assert_eq!(retry.last_tick(), Some(&joined));
    assert_eq!(retry.checkpoint().unwrap(), world.checkpoint().unwrap());
    assert_eq!(
        authority
            .geometry_queries
            .iter()
            .filter(|r| matches!(r.query, Query::SourceMotion(MotionQuery::PeerHelp(_))))
            .count(),
        9
    );

    reject_forged_join(&world, &before, &joined);
    reject_forged_peer_geometry(&authority);
    assert_denied_geometry_keeps_helper_idle(&world, &before, &mut authority);
    let mut later_melee = false;
    for _ in 0..50 {
        step(&mut world, &mut authority);
        if world
            .last_tick()
            .unwrap()
            .events
            .iter()
            .any(|event| matches!(event, Event::Melee { id, .. } if id.spawn == HELPER))
        {
            later_melee = true;
            break;
        }
    }
    assert!(
        later_melee,
        "helper runs its later ordinary combat update and melee"
    );
    let old = world.state().npcs[&HELPER].id;
    let old_model = world.state().npcs[&HELPER]
        .initialized_creature()
        .unwrap()
        .clone();
    for _ in 0..100 {
        if world.state().npcs[&HELPER].health == 0 {
            break;
        }
        let command = world.state().casts.is_empty().then(|| {
            let mut cast = f::cast(&world, 133);
            cast.input.command = Command::Cast {
                ability: 133,
                target: old,
            };
            cast
        });
        world
            .step(world.state().tick + 1, command, &mut authority)
            .unwrap();
    }
    assert_eq!(
        world.state().npcs[&HELPER].health,
        0,
        "actual lethal helper projectile"
    );
    assert_eq!(world.state().npcs[&HELPER].ai.phase(), ai::Phase::Corpse);
    assert!(!world.state().npcs[&HELPER]
        .events
        .assistance()
        .unwrap()
        .already_called());
    assert_helper_owners(&world);
    for _ in 0..60 {
        if world.state().npcs[&HELPER].id != old {
            break;
        }
        step(&mut world, &mut authority);
    }
    let helper = &world.state().npcs[&HELPER];
    assert_eq!(helper.id.generation, old.generation + 1);
    assert_eq!(
        helper.historical_initialized_creature(old),
        Some(&old_model)
    );
    assert_eq!(helper.ai.phase(), ai::Phase::Idle);
    assert!(helper.ai.target().is_none());
    assert!(!helper.events.can_aggro());
    assert!(!helper.events.assistance().unwrap().already_called());
    assert_helper_owners(&world);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(world.profile().clone(), initial).unwrap();
    replay.replay(world.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), world.checkpoint().unwrap());
    let saved = world.checkpoint().unwrap();
    world.checkpoint_committed(&saved).unwrap();
    let mut restored =
        NativeWorld::restore_synthetic_component_test(world.profile().clone(), saved).unwrap();
    step(&mut world, &mut authority);
    step(&mut restored, &mut authority);
    assert_eq!(restored.checkpoint().unwrap(), world.checkpoint().unwrap());
    assert!(authority.cluster.dry.unsupported_motion.is_empty());
}

fn reject_forged_join(world: &NativeWorld, before: &Checkpoint, record: &TickRecord) {
    for mutation in 0..10 {
        let mut forged = record.clone();
        let receipt =
            forged
                .replies
                .iter_mut()
                .find_map(|reply| match &mut reply.facts {
                    Facts::SourceMotion(MotionFacts::VisitorSight(SightDisposition::Peer(
                        receipt,
                    ))) if receipt.joined_combat() => Some(receipt),
                    _ => None,
                })
                .unwrap();
        let assistance = receipt.assistance.as_mut().unwrap();
        match mutation {
            0 => assistance.continuation.request.source[0] ^= 1,
            1 => assistance.continuation.request.peer.generation += 1,
            2 => {
                assistance.continuation.request.helper_height =
                    assistance.continuation.request.peer_height.clone()
            }
            3 => {
                assistance.continuation.request.peer_pose =
                    assistance.continuation.request.helper_pose
            }
            4 => assistance.target_can_check = false,
            5 => {
                assistance
                    .continuation
                    .line_of_sight
                    .as_mut()
                    .unwrap()
                    .check = ai::movement::Check::Failed
            }
            6 => receipt.assistance = None,
            7 => assistance.continuation.request.radius = f::real(6.0),
            8 => {
                let mut water =
                    serde_json::to_value(&assistance.continuation.request.victim_water).unwrap();
                water["tick"] = serde_json::json!(record.tick + 1);
                assistance.continuation.request.victim_water =
                    serde_json::from_value(water).unwrap();
            }
            9 => {
                *assistance
                    .continuation
                    .line_of_sight
                    .as_mut()
                    .unwrap()
                    .query = PeerHelpQuery::PeerRange(assistance.continuation.request.clone())
            }
            _ => unreachable!(),
        }
        let mut replay =
            NativeWorld::restore_synthetic_component_test(world.profile().clone(), before.clone())
                .unwrap();
        assert!(
            replay.replay(&[forged]).is_err(),
            "forged positive help {mutation}"
        );
        assert_eq!(replay.checkpoint().unwrap(), *before);
        assert!(replay.records().is_empty());
    }
}

fn reject_forged_peer_geometry(authority: &HelperAuthority<'_>) {
    let request = authority.geometry_queries.iter().find(|request| matches!(request.query, Query::SourceMotion(MotionQuery::PeerHelp(ref query)) if matches!(query.as_ref(), PeerHelpQuery::PeerLineOfSight(_)))).unwrap();
    assert_eq!(
        authority.vmap.peer_help_line_of_sight(
            &authority.session,
            &authority.profile,
            request,
            Instant::now()
        ),
        Err(Failure::Deadline)
    );
    for mutation in 0..6 {
        let mut forged = request.clone();
        let Query::SourceMotion(MotionQuery::PeerHelp(query)) = &mut forged.query else {
            unreachable!()
        };
        let PeerHelpQuery::PeerLineOfSight(help) = query.as_mut() else {
            unreachable!()
        };
        match mutation {
            0 => forged.binding.tick += 1,
            1 => help.peer.generation += 1,
            2 => help.helper_height = help.peer_height.clone(),
            3 => help.helper_width = help.peer_width.clone(),
            4 | 5 => {
                let mut width = serde_json::to_value(&help.helper_width).unwrap();
                if mutation == 4 {
                    width["generation_selection"]["world"][0] = serde_json::json!(99);
                } else {
                    width["generation_selection"]["created_tick"] =
                        serde_json::json!(help.binding.tick + 1);
                }
                help.helper_width = serde_json::from_value(width).unwrap();
            }
            _ => unreachable!(),
        }
        assert_eq!(
            authority.vmap.peer_help_line_of_sight(
                &authority.session,
                &authority.profile,
                &forged,
                deadline()
            ),
            Err(Failure::Binding)
        );
    }
}

/// Deliberate negative authority responses exercise predicate short-circuiting;
/// they are not alternate claims about the loaded positive LOS ray above.
struct DeniedPeerFact<'a, 'b> {
    inner: &'a mut HelperAuthority<'b>,
    phase: usize,
}
impl Authority for DeniedPeerFact<'_, '_> {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        let mut reply = self.inner.resolve(request)?;
        if let Facts::SourceMotion(MotionFacts::PeerHelp(facts)) = &mut reply.facts {
            let phase = match facts.query.as_ref() {
                PeerHelpQuery::VictimVisibility(_) => 0,
                PeerHelpQuery::PeerRange(_) => 1,
                PeerHelpQuery::PeerLineOfSight(_) => 2,
            };
            if phase == self.phase {
                facts.check = ai::movement::Check::Failed;
            }
        }
        Ok(reply)
    }
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        self.inner.observe_derived(reply)
    }
}
fn assert_denied_geometry_keeps_helper_idle(
    world: &NativeWorld,
    before: &Checkpoint,
    authority: &mut HelperAuthority<'_>,
) {
    for phase in 0..3 {
        let mut negative =
            NativeWorld::restore_synthetic_component_test(world.profile().clone(), before.clone())
                .unwrap();
        let mut denied = DeniedPeerFact {
            inner: authority,
            phase,
        };
        step(&mut negative, &mut denied);
        let helper = &negative.state().npcs[&HELPER];
        assert_eq!(helper.ai.phase(), ai::Phase::Idle);
        assert!(helper.ai.target().is_none());
        assert!(!helper.events.assistance().unwrap().already_called());
        assert_eq!(helper.events.assistance().unwrap().pending().count(), 0);
        let denied_help = negative
            .last_tick()
            .unwrap()
            .replies
            .iter()
            .filter_map(peer)
            .find_map(|(_, receipt)| {
                receipt
                    .help
                    .as_ref()
                    .filter(|help| help.observer.spawn == HELPER)
            })
            .unwrap();
        assert_eq!(
            denied_help.denial,
            [
                PeerHelpDenial::VictimNotVisible,
                PeerHelpDenial::PeerDifferentMap,
                PeerHelpDenial::PeerLineOfSightBlocked
            ][phase]
        );
        let mut replay =
            NativeWorld::restore_synthetic_component_test(world.profile().clone(), before.clone())
                .unwrap();
        replay.replay(negative.records()).unwrap();
        assert_eq!(replay.checkpoint().unwrap(), negative.checkpoint().unwrap());
    }
}

/// Add an independent explicitly authored AGGRO holder. The six neutral entry-6
/// owners retain the original fixture's event and broadcast source rows.
pub(in super::super) fn install_helper_text(mut profile: Profile, helper: u64) -> Profile {
    let old = profile.event_ai().unwrap();
    let EventAiRows::Complete(events) = &old.catalog().owner(2).unwrap().entry_events else {
        unreachable!()
    };
    let mut event = events[0].clone();
    event.id = HELPER_EVENT;
    event.creature_id = profile.npc(helper).unwrap().entry as i32;
    event.comment = "Authored helper combat line".into();
    let owners = profile
        .npcs
        .iter()
        .map(|npc| {
            let mut owner = old.catalog().owner(npc.ai.creature_id).unwrap().clone();
            if owner.spawn_id == helper {
                owner.entry_events = EventAiRows::Complete(vec![event.clone()]);
            }
            owner
        })
        .collect();
    let catalog = EventAiCatalog::validate_sources(EventAiInput {
        owners: EventAiRows::Complete(owners),
        broadcasts: EventAiRows::Complete(
            [text::TEXT_COMMON, text::TEXT_UNIVERSAL]
                .into_iter()
                .map(|id| EventAiBroadcastSource {
                    id: id as i32,
                    text: Some(format!("Authored line {id}")),
                    text1: None,
                    chat_type: 0,
                    language: if id == text::TEXT_COMMON { 7 } else { 0 },
                    condition_id: 0,
                    emotes_id: 0,
                    flags: 1,
                    sounds: [0; 2],
                    emote_ids: [0; 3],
                    emote_delays: [0; 3],
                    verified_build: 0,
                })
                .collect(),
        ),
        locales: EventAiRows::Complete(vec![EventAiBroadcastLocale {
            id: text::TEXT_COMMON as i32,
            locale: "frFR".into(),
            text: Some("Réplique française".into()),
            text1: None,
            verified_build: 0,
        }]),
    })
    .unwrap();
    let catalog = EventAiRuntimeCatalog::validate_sources(
        catalog,
        EventAiRuntimeSource {
            session: old.session_policy(),
            locale: old.locale(),
            configured_say_range: old.say_range(),
            visibility_distance: old.visibility_distance(),
            owners: EventAiRows::Complete(
                profile
                    .npcs
                    .iter()
                    .map(|npc| old.owner(npc.ai.creature_id).unwrap().clone())
                    .collect(),
            ),
        },
    )
    .unwrap();
    let visitors = profile.source_visitors.as_ref().unwrap();
    let mut input = visitors::visitor_input(&profile, visitors.owner_order().to_vec());
    input.owners = SourceVisitorRows::Complete(
        profile
            .npcs
            .iter()
            .map(|npc| {
                let mut owner = visitors.owner(npc.ai.creature_id).unwrap().clone();
                if owner.spawn_id == helper {
                    owner.ai_name = "EventAI".into();
                    owner.entry_events = SourceVisitorRows::Complete(vec![HELPER_EVENT]);
                }
                owner
            })
            .collect(),
    );
    profile.source_visitors =
        Some(SourceVisitorCatalog::validate_sources_with_event_ai(input, catalog).unwrap());
    profile.validate().unwrap();
    profile
}
