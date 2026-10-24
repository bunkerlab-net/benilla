// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::native::{synthetic_fixture as f, synthetic_two_owner_fixture as fixture};

fn world() -> NativeWorld {
    NativeWorld::synthetic_component_test(fixture::profile(), [8; 16], f::player(), 7).unwrap()
}
fn defensive_profile(owner: u64) -> Profile {
    let mut profile = fixture::profile();
    profile
        .npcs
        .iter_mut()
        .find(|npc| npc.ai.creature_id == owner)
        .unwrap()
        .ai
        .reaction = ai::Reaction::Defensive;
    profile
        .initialize_combat_authority(
            profile
                .npcs
                .iter()
                .map(|npc| NpcCombatAuthoritySource {
                    entry: npc.entry,
                    extra_flags: if npc.ai.creature_id == owner { 0x2 } else { 0 },
                    static_flags1: 0,
                    static_flags2: 0,
                    static_flags3: 0,
                    static_flags4: 0,
                })
                .collect(),
        )
        .unwrap();
    profile
}
fn sight(
    world: &NativeWorld,
    observer: u64,
    target: u64,
) -> Result<PeerSightDisposition, NativeError> {
    source_peer_sight(
        &world.profile,
        &world.state,
        None,
        world.state.npcs[&observer].id,
        world.state.npcs[&target].id,
    )
    .map(|receipt| receipt.disposition)
}
fn enable_aggro(world: &mut NativeWorld) {
    for npc in world.state.npcs.values_mut() {
        npc.events.can_aggro = true;
    }
}
/// Direct source-AI callback inputs isolate this prefix; the end-to-end
/// visitor tests separately supply player combat and spatial authority.
fn observation(world: &NativeWorld, owner: u64) -> ai::Observation {
    let npc = &world.state.npcs[&owner];
    ai::Observation {
        sequence: npc.ai.next_sequence().unwrap(),
        elapsed_ms: world.profile.tick_ms,
        geometry_revision: ai::movement::Revision(world.profile.geometry_revision),
        target: Some(ai::Target {
            id: SOLO_UNIT,
            position: npc.ai.position(),
            level: world.profile.player.level,
            combat_reach: world.profile.player.combat_reach,
            bounding_radius: world.profile.player.bounding_radius,
            moving_and_running: false,
            alive: true,
            online: true,
            can_attack_in_combat: true,
            can_attack_on_sight: true,
            visible: true,
            accessible: true,
            line_of_sight: true,
        }),
        can_initiate_attack: true,
        casting_non_melee: false,
        stationary_melee_refresh_eligible: false,
        creature_moving_and_running: false,
        movement: None,
        receipts: vec![],
    }
}
fn attacked_by_player(world: &mut NativeWorld, owner: u64) {
    let mut observed = observation(world, owner);
    observed.receipts.push(ai::Receipt {
        id: 1,
        event: ai::CombatEvent::AttackedBy { target: SOLO_UNIT },
    });
    world
        .state
        .npcs
        .get_mut(&owner)
        .unwrap()
        .ai
        .source_combat_event(&observed)
        .unwrap();
    assert_eq!(world.state.npcs[&owner].ai.phase(), ai::Phase::Combat);
}
fn complete_ai(world: &mut NativeWorld, owner: u64, lethal: bool) {
    let observed = observation(world, owner);
    let ai = &mut world.state.npcs.get_mut(&owner).unwrap().ai;
    let token = ai.begin_source_receipts(&observed, false).unwrap();
    let mut swung = false;
    ai.finish_receipts(observed, token, &mut |_, _, _| {
        swung = true;
        Ok(if lethal {
            ai::MainhandResult::TargetDied
        } else {
            ai::MainhandResult::Unchanged
        })
    })
    .unwrap();
    assert!(swung);
    if lethal {
        world.state.health = 0;
    }
}

#[test]
fn idle_prefix_derives_raw_neutrality_can_aggro_and_peer_relation_without_mutating() {
    let mut world = world();
    let before = world.state.clone();
    assert_eq!(sight(&world, 3, 2), Ok(PeerSightDisposition::NeutralToAll));
    assert_eq!(
        sight(&world, 2, 3),
        Ok(PeerSightDisposition::CannotInitiateAttack)
    );
    assert_eq!(world.state, before);
    enable_aggro(&mut world);
    let before = world.state.clone();
    assert_eq!(
        sight(&world, 2, 3),
        Ok(PeerSightDisposition::CannotAttackOnSight)
    );
    assert_eq!(sight(&world, 3, 2), Ok(PeerSightDisposition::NeutralToAll));
    assert_eq!(world.state, before);
    assert!(world
        .profile
        .npcs
        .iter()
        .all(|npc| npc.ai.reaction == ai::Reaction::Aggressive));
}

#[test]
fn defensive_peer_returns_before_can_aggro_or_hostile_attack_continuation() {
    let mut profile = defensive_profile(2);
    let factions = &mut profile.interactions.as_mut().unwrap().factions;
    factions.get_mut(&2).unwrap().enemy_group_mask |= 4;
    factions.get_mut(&3).unwrap().group_mask = 4;
    assert_eq!(
        relations::faction_reaction(&factions[&2].template(), &factions[&3].template()),
        Reaction::Hostile
    );
    let mut world =
        NativeWorld::synthetic_component_test(profile, [8; 16], f::player(), 7).unwrap();
    for can_aggro in [false, true] {
        world.state.npcs.get_mut(&2).unwrap().events.can_aggro = can_aggro;
        let before = world.state.clone();
        assert_eq!(sight(&world, 2, 3), Ok(PeerSightDisposition::NonAggressive));
        assert_eq!(world.state, before);
        assert!(world.state.npcs[&2].ai.target().is_none());
    }
}

#[test]
fn defensive_peer_keeps_victim_and_neutrality_before_help_and_reaction() {
    let mut world =
        NativeWorld::synthetic_component_test(defensive_profile(3), [8; 16], f::player(), 7)
            .unwrap();
    attacked_by_player(&mut world, 2);
    let before = world.state.clone();
    assert_eq!(sight(&world, 3, 2), Ok(PeerSightDisposition::NeutralToAll));
    assert_eq!(world.state, before);
    attacked_by_player(&mut world, 3);
    let before = world.state.clone();
    assert_eq!(
        sight(&world, 3, 2),
        Ok(PeerSightDisposition::ExistingOutdoorVictim)
    );
    assert_eq!(world.state, before);
}

#[test]
fn defensive_peer_reached_help_rejects_before_nonaggressive_return() {
    let mut world =
        NativeWorld::synthetic_component_test(defensive_profile(2), [8; 16], f::player(), 7)
            .unwrap();
    attacked_by_player(&mut world, 3);
    for can_aggro in [false, true] {
        world.state.npcs.get_mut(&2).unwrap().events.can_aggro = can_aggro;
        let before = world.state.clone();
        assert_eq!(
            sight(&world, 2, 3),
            Err(NativeError::AuthorityUnavailable(
                "peer help selected assistance owners"
            ))
        );
        assert_eq!(world.state, before);
    }
    world
        .state
        .npcs
        .get_mut(&3)
        .unwrap()
        .ai
        .detach_dead_victim(SOLO_UNIT, 100)
        .unwrap();
    world.state.health = 0;
    assert_eq!(
        sight(&world, 2, 3),
        Err(NativeError::UnsupportedMechanic(
            "peer help attacker fallback"
        ))
    );
}

#[test]
fn defensive_peer_requires_complete_current_overlay_and_generation() {
    let world =
        NativeWorld::synthetic_component_test(defensive_profile(2), [8; 16], f::player(), 7)
            .unwrap();
    let ids = [world.state.npcs[&2].id, world.state.npcs[&3].id];
    for removed in [2, 3] {
        let mut state = world.state.clone();
        let owner = state.npcs.remove(&removed).unwrap();
        let before = (state.clone(), owner.clone());
        assert_eq!(
            source_peer_sight(&world.profile, &state, Some(&owner), ids[0], ids[1]),
            Ok(PeerSightDisposition::NonAggressive.into())
        );
        assert_eq!(
            source_peer_sight(&world.profile, &state, None, ids[0], ids[1]),
            Err(NativeError::AuthorityBinding)
        );
        assert_eq!((state.clone(), owner.clone()), before);
        let mut stale_owner = owner;
        stale_owner.id.generation += 1;
        assert_eq!(
            source_peer_sight(&world.profile, &state, Some(&stale_owner), ids[0], ids[1]),
            Err(NativeError::AuthorityBinding)
        );
    }
}

#[test]
fn bare_defensive_reaction_is_not_a_source_peer_noop() {
    let mut world = world();
    world.profile.npcs[0].ai.reaction = ai::Reaction::Defensive;
    assert_eq!(sight(&world, 2, 3), Err(NativeError::InvalidProfile));
}

#[test]
fn either_removed_notifying_owner_supplies_the_same_complete_read_only_scene() {
    let mut world = world();
    enable_aggro(&mut world);
    let ids = [world.state.npcs[&2].id, world.state.npcs[&3].id];
    for removed in [2, 3] {
        let mut state = world.state.clone();
        let owner = state.npcs.remove(&removed).unwrap();
        let before = (state.clone(), owner.clone());
        assert_eq!(
            source_peer_sight(&world.profile, &state, Some(&owner), ids[0], ids[1]),
            Ok(PeerSightDisposition::CannotAttackOnSight.into())
        );
        assert_eq!(
            source_peer_sight(&world.profile, &state, Some(&owner), ids[1], ids[0]),
            Ok(PeerSightDisposition::NeutralToAll.into())
        );
        assert_eq!((state, owner), before);
    }
}

#[test]
fn incomplete_duplicate_foreign_stale_or_wrong_owner_views_reject() {
    let world = world();
    let ids = [world.state.npcs[&2].id, world.state.npcs[&3].id];
    let mut missing = world.state.clone();
    let removed = missing.npcs.remove(&2).unwrap();
    assert_eq!(
        source_peer_sight(&world.profile, &missing, None, ids[0], ids[1]),
        Err(NativeError::AuthorityBinding)
    );
    assert_eq!(
        source_peer_sight(&world.profile, &world.state, Some(&removed), ids[0], ids[1]),
        Err(NativeError::AuthorityBinding)
    );
    let mut stale = ids[0];
    stale.generation += 1;
    assert_eq!(
        source_peer_sight(&world.profile, &world.state, None, stale, ids[1]),
        Err(NativeError::AuthorityBinding)
    );
    let mut replaced = removed.clone();
    replaced.id.generation += 1;
    assert_eq!(
        source_peer_sight(
            &world.profile,
            &missing,
            Some(&replaced),
            replaced.id,
            ids[1]
        ),
        Err(NativeError::AuthorityBinding)
    );
    let mut wrong_key = world.state.clone();
    let actor = wrong_key.npcs.remove(&2).unwrap();
    wrong_key.npcs.insert(99, actor);
    assert_eq!(
        source_peer_sight(&world.profile, &wrong_key, None, ids[0], ids[1]),
        Err(NativeError::AuthorityBinding)
    );
    assert_eq!(
        source_peer_sight(&world.profile, &world.state, None, ids[0], ids[0]),
        Err(NativeError::AuthorityBinding)
    );
}

#[test]
fn just_started_attack_has_a_victim_before_threat_manager_selects_one() {
    let mut world = world();
    attacked_by_player(&mut world, 2);
    assert_eq!(world.state.npcs[&2].ai.current_victim(), None);
    assert_eq!(world.state.npcs[&2].ai.target(), Some(SOLO_UNIT));
    let before = world.state.clone();
    assert_eq!(
        sight(&world, 2, 3),
        Ok(PeerSightDisposition::ExistingOutdoorVictim)
    );
    // Neutrality precedes the peer's in-combat CheckForHelp branch.
    assert_eq!(sight(&world, 3, 2), Ok(PeerSightDisposition::NeutralToAll));
    assert_eq!(world.state, before);
    attacked_by_player(&mut world, 3);
    // Existing Unit victim precedes neutrality and reached assistance.
    assert_eq!(
        sight(&world, 3, 2),
        Ok(PeerSightDisposition::ExistingOutdoorVictim)
    );
    assert_eq!(
        sight(&world, 2, 3),
        Ok(PeerSightDisposition::ExistingOutdoorVictim)
    );
}

#[test]
fn earlier_owner_completed_update_is_read_without_bumping_its_sequence() {
    let mut world = world();
    attacked_by_player(&mut world, 2);
    complete_ai(&mut world, 2, false);
    assert_eq!(world.state.npcs[&2].ai.next_sequence(), Some(2));
    assert_eq!(world.state.npcs[&2].ai.current_victim(), Some(SOLO_UNIT));
    let before = world.state.clone();
    assert_eq!(
        sight(&world, 2, 3),
        Ok(PeerSightDisposition::ExistingOutdoorVictim)
    );
    assert_eq!(sight(&world, 3, 2), Ok(PeerSightDisposition::NeutralToAll));
    assert_eq!(world.state, before);
}

#[test]
fn lethal_player_damage_detaches_victim_while_retaining_ai_target() {
    let mut world = world();
    enable_aggro(&mut world);
    attacked_by_player(&mut world, 2);
    complete_ai(&mut world, 2, true);
    let npc = &world.state.npcs[&2];
    assert_eq!(npc.ai.phase(), ai::Phase::Combat);
    assert_eq!(npc.ai.target(), Some(SOLO_UNIT));
    assert_eq!(npc.ai.current_victim(), None);
    assert!(npc.ai.victim_detached_at_ms().is_some());
    assert!(npc.ai.hostile_reference().is_none());
    let before = world.state.clone();
    assert_eq!(
        sight(&world, 2, 3),
        Ok(PeerSightDisposition::CannotAttackOnSight)
    );
    assert_eq!(world.state, before);
}

#[test]
fn reached_help_rejects_before_can_aggro_or_later_raw_relation_denial() {
    let mut world = world();
    // A deliberate player hit can put ordinary neutral B into combat.
    attacked_by_player(&mut world, 3);
    for can_aggro in [false, true] {
        world.state.npcs.get_mut(&2).unwrap().events.can_aggro = can_aggro;
        let before = world.state.clone();
        assert_eq!(
            sight(&world, 2, 3),
            Err(NativeError::AuthorityUnavailable(
                "peer help selected assistance owners"
            ))
        );
        assert_eq!(world.state, before);
    }
    // A detached victim does not remove the peer's retained IN_COMBAT flag.
    world
        .state
        .npcs
        .get_mut(&3)
        .unwrap()
        .ai
        .detach_dead_victim(SOLO_UNIT, 100)
        .unwrap();
    world.state.health = 0;
    assert_eq!(
        sight(&world, 2, 3),
        Err(NativeError::UnsupportedMechanic(
            "peer help attacker fallback"
        ))
    );
}

#[test]
fn npc_relation_uses_directional_enemy_and_never_installs_a_peer_target() {
    let mut world = world();
    enable_aggro(&mut world);
    // B regards A as hostile; A is neutral towards B. Both raw directions
    // allow CanAttack, but only B passes the final IsEnemy sight predicate.
    let b = world
        .profile
        .interactions
        .as_mut()
        .unwrap()
        .factions
        .get_mut(&3)
        .unwrap();
    b.faction = 96_003;
    b.reputation_list_id = Some(-1);
    b.group_mask = 4;
    b.friend_group_mask = 4;
    b.enemy_group_mask = 2;
    world.profile.npcs[1].ai.neutral_to_all = false;
    // Recreate live actors with the exact changed immutable source profile.
    let mut world =
        NativeWorld::synthetic_component_test(world.profile, [8; 16], f::player(), 7).unwrap();
    enable_aggro(&mut world);
    let before = world.state.clone();
    assert_eq!(
        sight(&world, 2, 3),
        Ok(PeerSightDisposition::CannotAttackOnSight)
    );
    assert_eq!(
        sight(&world, 3, 2),
        Err(NativeError::UnsupportedMechanic(
            "peer sight attack/accessibility continuation"
        ))
    );
    assert_eq!(world.state, before);
    assert!(world
        .state
        .npcs
        .values()
        .all(|npc| npc.ai.target().is_none()));
}

#[test]
fn reached_npc_attack_blockers_are_raw_target_flags() {
    for flag in [
        unit_flags::SPAWNING,
        unit_flags::NOT_ATTACKABLE_1,
        unit_flags::UNTARGETABLE,
        unit_flags::UNINTERACTIBLE,
    ] {
        let mut profile = fixture::profile();
        profile
            .interactions
            .as_mut()
            .unwrap()
            .factions
            .get_mut(&2)
            .unwrap()
            .enemy_group_mask |= 4;
        profile
            .interactions
            .as_mut()
            .unwrap()
            .factions
            .get_mut(&3)
            .unwrap()
            .group_mask = 4;
        profile.npcs[1].health_regeneration.unit_flags = flag;
        let b_entry = profile.npcs[1].entry;
        profile
            .interactions
            .as_mut()
            .unwrap()
            .npcs
            .get_mut(&b_entry)
            .unwrap()
            .unit_flags = flag;
        let mut world =
            NativeWorld::synthetic_component_test(profile, [8; 16], f::player(), 7).unwrap();
        enable_aggro(&mut world);
        assert_eq!(
            sight(&world, 2, 3),
            Ok(PeerSightDisposition::CannotAttackOnSight)
        );
        // Faction zero remains neutral even when its group draws hostility.
        assert_eq!(sight(&world, 3, 2), Ok(PeerSightDisposition::NeutralToAll));
    }
}

#[test]
fn initiation_uses_observer_raw_flags_force_capability_and_current_can_aggro() {
    for force in [false, true] {
        let mut profile = fixture::profile();
        let a_entry = profile.npcs[0].entry;
        profile.npcs[0].health_regeneration.unit_flags = unit_flags::SPAWNING;
        profile
            .interactions
            .as_mut()
            .unwrap()
            .npcs
            .get_mut(&a_entry)
            .unwrap()
            .unit_flags = unit_flags::SPAWNING;
        profile
            .initialize_combat_authority(
                profile
                    .npcs
                    .iter()
                    .map(|npc| NpcCombatAuthoritySource {
                        entry: npc.entry,
                        extra_flags: if force && npc.ai.creature_id == 2 {
                            0x0008_0000
                        } else {
                            0
                        },
                        static_flags1: 0,
                        static_flags2: 0,
                        static_flags3: 0,
                        static_flags4: 0,
                    })
                    .collect(),
            )
            .unwrap();
        let mut world =
            NativeWorld::synthetic_component_test(profile, [8; 16], f::player(), 7).unwrap();
        assert_eq!(
            sight(&world, 2, 3),
            Ok(PeerSightDisposition::CannotInitiateAttack)
        );
        enable_aggro(&mut world);
        assert_eq!(
            sight(&world, 2, 3),
            Ok(if force {
                PeerSightDisposition::CannotAttackOnSight
            } else {
                PeerSightDisposition::CannotInitiateAttack
            })
        );
    }
}

#[test]
fn returning_home_target_is_not_an_attack_on_sight_candidate() {
    let mut profile = fixture::profile();
    profile
        .interactions
        .as_mut()
        .unwrap()
        .factions
        .get_mut(&2)
        .unwrap()
        .enemy_group_mask |= 4;
    profile
        .interactions
        .as_mut()
        .unwrap()
        .factions
        .get_mut(&3)
        .unwrap()
        .group_mask = 4;
    let mut world =
        NativeWorld::synthetic_component_test(profile, [8; 16], f::player(), 7).unwrap();
    enable_aggro(&mut world);
    attacked_by_player(&mut world, 3);
    world
        .state
        .npcs
        .get_mut(&3)
        .unwrap()
        .ai
        .detach_dead_victim(SOLO_UNIT, 100)
        .unwrap();
    world.state.health = 0;
    let mut observed = observation(&world, 3);
    observed.target.as_mut().unwrap().alive = false;
    let ai = &mut world.state.npcs.get_mut(&3).unwrap().ai;
    let token = ai.begin_source_receipts(&observed, false).unwrap();
    ai.finish_receipts(observed, token, &mut |_, _, _| {
        panic!("dead victim cannot swing")
    })
    .unwrap();
    assert_eq!(world.state.npcs[&3].ai.phase(), ai::Phase::ReturningHome);
    let before = world.state.clone();
    assert_eq!(
        sight(&world, 2, 3),
        Ok(PeerSightDisposition::CannotAttackOnSight)
    );
    assert_eq!(world.state, before);
}

#[test]
fn missing_source_closure_is_not_a_successful_noop() {
    let mut world = world();
    world.profile.source_visitors = None;
    assert_eq!(
        sight(&world, 3, 2),
        Err(NativeError::UnsupportedMechanic(
            "peer sight requires complete source visitors"
        ))
    );
    let mut world = self::world();
    world.profile.creature_capabilities = None;
    assert!(sight(&world, 3, 2).is_err());
    let mut world = self::world();
    world.profile.npcs[1].ai.neutral_to_all = false;
    assert_eq!(sight(&world, 3, 2), Err(NativeError::InvalidProfile));
}

/// Authored source generations and real Unit assistance owners, independently
/// exercised from the app's byte-backed seven-owner callback/replay scenarios.
fn selected_profile(defensive: bool, hostile_to_player: bool) -> Profile {
    use crate::native::creature_generation_tests::{complete_scene_sources, install};
    let mut p = if defensive {
        defensive_profile(2)
    } else {
        fixture::profile()
    };
    if !hostile_to_player {
        let faction = p
            .interactions
            .as_mut()
            .unwrap()
            .factions
            .get_mut(&2)
            .unwrap();
        faction.enemy_group_mask = 0;
        faction.friend_group_mask = 2;
        faction.enemies = [0; 4];
    }
    let mut raw = complete_scene_sources(&p);
    for source in &mut raw {
        source.input.template.npc_flags =
            p.interactions.as_ref().unwrap().npcs[&source.spawn_entry].npc_flags;
    }
    p = install(p, raw, vec![2, 3]);
    let rows = p
        .npcs
        .iter()
        .map(|npc| {
            let raw = p
                .creature_generations
                .as_ref()
                .unwrap()
                .source(npc.ai.creature_id)
                .unwrap();
            CombatAssistanceSource {
                spawn_id: npc.ai.creature_id,
                entry: npc.entry,
                generation_source: raw.identity(),
                call_for_help: f::real(5.0),
                excluded_hooks: SourceVisitorRows::Complete(vec![]),
            }
        })
        .collect();
    p.initialize_combat_assistance(rows, 2000).unwrap();
    p
}
fn selected_world(defensive: bool, hostile_to_player: bool) -> NativeWorld {
    NativeWorld::synthetic_component_test(
        selected_profile(defensive, hostile_to_player),
        [8; 16],
        f::player(),
        7,
    )
    .unwrap()
}
fn full_sight(world: &NativeWorld) -> Result<PeerSightReceipt, NativeError> {
    source_peer_sight(
        &world.profile,
        &world.state,
        None,
        world.state.npcs[&2].id,
        world.state.npcs[&3].id,
    )
}
/// Drive the existing Unit queue transition, without writing its private latch.
fn suppress_help(world: &mut NativeWorld, spawn: u64) {
    let catalog = world.profile.combat_assistance().unwrap();
    let source = catalog.digest();
    let generation_source = catalog.source(spawn).unwrap().generation_source;
    let token = world
        .state
        .npcs
        .get_mut(&spawn)
        .unwrap()
        .events
        .prepare_assistance(2000, 0)
        .unwrap();
    let npc = &world.state.npcs[&spawn];
    let request = CombatAssistanceRequest {
        binding: Binding {
            world: world.state.world,
            tick: world.state.tick,
            profile: world.profile.digest(),
            authority_policy: world.profile.authority_policy,
            geometry: world.profile.geometry_revision,
            npc: Some(npc.id),
        },
        content: world.profile.content_digest,
        source,
        generation_source,
        callback: EventAiCallbackKind::CombatReceipt {
            receipt: ai::Receipt {
                id: 1,
                event: ai::CombatEvent::AttackedBy { target: SOLO_UNIT },
            },
        },
        moment: InteractionMoment::Frame { action_ordinal: 1 },
        now_ms: world.state.now_ms,
        occurrence: token.occurrence,
        unit_clock_ms: npc.events.clock_ms(),
        owner_pose: npc.source_motion.as_ref().unwrap().motion.pose(),
        enemy: world.state.player,
        enemy_pose: benilla_sim::creature_motion::Pose {
            position: world.state.position,
            orientation: ai::Real::from_bits(world.state.orientation.bits()).unwrap(),
        },
        radius: f::real(5.0),
        owner_bounding_radius: npc
            .effective_profile(&world.profile)
            .unwrap()
            .ai
            .bounding_radius,
        scene: interactions::witness(
            world.profile.digest(),
            &world.state,
            InteractionMoment::Frame { action_ordinal: 1 },
        )
        .unwrap()
        .draft,
        scene_inventory: world.state.npcs.values().map(|npc| npc.id).collect(),
        rng_counter: world.state.rng_counter,
        event_ai_callback: None,
    };
    let facts = CombatAssistanceFacts::AuthoredCompleteTraversal {
        request: Box::new(request.clone()),
        grid: vec![npc.id],
        world: vec![],
        outside_coverage: world
            .state
            .npcs
            .values()
            .filter(|other| other.id != npc.id)
            .map(|other| other.id)
            .collect(),
    };
    let traversal =
        interactions::source_assistance_traversal(&world.profile, &world.state, &request, &facts)
            .unwrap();
    world
        .state
        .npcs
        .get_mut(&spawn)
        .unwrap()
        .events
        .finish_assistance(request, Some(facts), Some(traversal), token, 0)
        .unwrap();
}

#[test]
fn help_reads_each_live_latch_independently_and_preserves_source_short_circuit_order() {
    for (observer_check, target_check, expected) in [
        (false, false, PeerHelpDenial::ObserverCannotCheck),
        (false, true, PeerHelpDenial::ObserverCannotCheck),
        (true, false, PeerHelpDenial::TargetCannotCheck),
        (true, true, PeerHelpDenial::CannotAttackVictimOnSight),
    ] {
        let mut w = selected_world(true, false);
        attacked_by_player(&mut w, 3);
        enable_aggro(&mut w);
        if !observer_check {
            suppress_help(&mut w, 2);
        }
        if !target_check {
            suppress_help(&mut w, 3);
        }
        let before = w.state.clone();
        let receipt = full_sight(&w).unwrap();
        assert_eq!(receipt.disposition, PeerSightDisposition::NonAggressive);
        let help = receipt.help.unwrap();
        assert_eq!(help.denial, expected);
        assert_eq!(
            (help.observer_can_check, help.target_can_check),
            (observer_check, target_check)
        );
        assert_eq!(help.helper_victim, w.state.player);
        assert_eq!(help.observer, w.state.npcs[&2].id);
        assert_eq!(help.target, w.state.npcs[&3].id);
        help.validate().unwrap();
        assert_eq!(w.state, before);
    }
}

#[test]
fn help_uses_active_victim_with_empty_threat_cache_before_can_aggro_and_reaction() {
    for defensive in [false, true] {
        let mut w = selected_world(defensive, true);
        attacked_by_player(&mut w, 3);
        assert_eq!(w.state.npcs[&3].ai.current_victim(), None);
        assert_eq!(w.state.npcs[&3].ai.target(), Some(SOLO_UNIT));
        let before = w.state.clone();
        let receipt = full_sight(&w).unwrap();
        assert_eq!(
            receipt.disposition,
            if defensive {
                PeerSightDisposition::NonAggressive
            } else {
                PeerSightDisposition::CannotInitiateAttack
            }
        );
        let help = receipt.help.unwrap();
        assert_eq!(help.denial, PeerHelpDenial::CannotInitiateAttack);
        assert_eq!(help.observer_can_initiate, Some(false));
        assert_eq!(help.observer_can_attack_victim, None);
        assert_eq!(w.state, before);
        enable_aggro(&mut w);
        let before = w.state.clone();
        assert_eq!(
            full_sight(&w),
            Err(NativeError::AuthorityUnavailable(
                "peer help callback witness"
            ))
        );
        assert_eq!(w.state, before);
    }
}

#[test]
fn denied_help_resumes_aggressive_npc_sight_instead_of_returning_a_help_noop() {
    let mut w = selected_world(false, false);
    attacked_by_player(&mut w, 3);
    enable_aggro(&mut w);
    suppress_help(&mut w, 3);
    let receipt = full_sight(&w).unwrap();
    assert_eq!(
        receipt.disposition,
        PeerSightDisposition::CannotAttackOnSight
    );
    assert_eq!(
        receipt.help.unwrap().denial,
        PeerHelpDenial::TargetCannotCheck
    );
    // The raw NPC/NPC continuation is hostile even while the help prefix denies.
    // These masks do not affect source generation identity or neutral-to-all.
    w.profile
        .interactions
        .as_mut()
        .unwrap()
        .factions
        .get_mut(&2)
        .unwrap()
        .enemy_group_mask |= 4;
    w.profile
        .interactions
        .as_mut()
        .unwrap()
        .factions
        .get_mut(&3)
        .unwrap()
        .group_mask = 4;
    let before = w.state.clone();
    assert_eq!(
        full_sight(&w),
        Err(NativeError::UnsupportedMechanic(
            "peer sight attack/accessibility continuation"
        ))
    );
    assert_eq!(w.state, before);
}

#[test]
fn helper_attacker_fallback_is_unrepresented_even_when_a_later_latch_denies() {
    let mut w = selected_world(true, false);
    attacked_by_player(&mut w, 3);
    suppress_help(&mut w, 3);
    w.state
        .npcs
        .get_mut(&3)
        .unwrap()
        .ai
        .detach_dead_victim(SOLO_UNIT, 100)
        .unwrap();
    w.state.health = 0;
    let before = w.state.clone();
    assert_eq!(
        full_sight(&w),
        Err(NativeError::UnsupportedMechanic(
            "peer help attacker fallback"
        ))
    );
    assert_eq!(w.state, before);
}

#[test]
fn selected_help_requires_both_current_assistance_owners_and_exact_source() {
    for spawn in [2, 3] {
        let mut w = selected_world(true, false);
        attacked_by_player(&mut w, 3);
        w.state.npcs.get_mut(&spawn).unwrap().events.assistance = None;
        assert_eq!(
            full_sight(&w),
            Err(NativeError::AuthorityUnavailable(if spawn == 2 {
                "peer help observer assistance owner"
            } else {
                "peer help target assistance owner"
            }))
        );
        w.state.npcs.get_mut(&spawn).unwrap().events.assistance =
            Some(CombatAssistanceState::new([99; 32]));
        assert_eq!(full_sight(&w), Err(NativeError::AuthorityBinding));
    }
}

#[test]
fn help_receipt_fields_are_required_and_later_flags_cannot_replace_first_denial() {
    let mut w = selected_world(true, false);
    attacked_by_player(&mut w, 3);
    suppress_help(&mut w, 2);
    suppress_help(&mut w, 3);
    let receipt = full_sight(&w).unwrap();
    let encoded = serde_json::to_value(&receipt).unwrap();
    assert_eq!(
        serde_json::from_value::<PeerSightReceipt>(encoded.clone()).unwrap(),
        receipt
    );
    let mut missing = encoded.clone();
    missing.as_object_mut().unwrap().remove("help");
    assert!(serde_json::from_value::<PeerSightReceipt>(missing).is_err());
    for field in encoded["help"].as_object().unwrap().keys() {
        let mut missing = encoded.clone();
        missing["help"].as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<PeerSightReceipt>(missing).is_err(),
            "{field}"
        );
    }
    let mut help = receipt.help.unwrap();
    help.denial = PeerHelpDenial::TargetCannotCheck;
    assert!(help.validate().is_err());
    help.denial = PeerHelpDenial::ObserverCannotCheck;
    help.observer_can_initiate = Some(false);
    assert!(help.validate().is_err());
    // Literal source order also covers source-excluded CC/callability states.
    help.observer_can_initiate = None;
    help.observer_can_check = true;
    help.target_can_check = true;
    help.target_unit_flags |= STUNNED | 0x0040_0000;
    help.target_unit_state |= 0x400;
    assert_eq!(help.early_denial(), Some(PeerHelpDenial::TargetRetreating));
    help.target_unit_state = 0;
    assert_eq!(help.early_denial(), Some(PeerHelpDenial::TargetConfused));
    help.target_unit_flags &= !0x0040_0000;
    assert_eq!(help.early_denial(), Some(PeerHelpDenial::TargetStunned));
    help.target_can_call = false;
    assert_eq!(help.early_denial(), Some(PeerHelpDenial::TargetCannotCall));
    help.observer_can_call = false;
    assert_eq!(
        help.early_denial(),
        Some(PeerHelpDenial::ObserverCannotCall)
    );
    help.observer_in_combat = true;
    assert_eq!(help.early_denial(), Some(PeerHelpDenial::ObserverInCombat));
}

#[test]
fn help_prefix_matches_pinned_cpp_source_leaves_without_admitting_excluded_states() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../scripts/native_assistance_source_oracle/peer_help_vectors.json"
    )))
    .unwrap();
    assert_eq!(
        vectors["source_revision"],
        "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f"
    );
    let mut observed_prefixes = 0;
    let mut observed_initiations = 0;
    for case in vectors["cases"].as_array().unwrap() {
        let units = case["input"]["authored_units"].as_array().unwrap();
        let observer = units.iter().find(|unit| unit["guid"] == 1).unwrap();
        let target = units.iter().find(|unit| unit["guid"] == 101).unwrap();
        if target["active_victim"] != 77 {
            continue; // General attacker fallback is deliberately not executable.
        }
        for observed in case["observations"].as_array().unwrap() {
            let leaves: Vec<_> = observed["leaves"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            assert_eq!(&leaves[..2], &["get_victim:101", "get_victim:101"]);
            // These are the C++ executable's last observed leaf, not hand-authored
            // expected decisions or a second implementation of its predicates.
            let source_denial = match *leaves.last().unwrap() {
                "flags_524288:1:true" => Some(PeerHelpDenial::ObserverInCombat),
                "can_call:1:false" => Some(PeerHelpDenial::ObserverCannotCall),
                "can_call:101:false" => Some(PeerHelpDenial::TargetCannotCall),
                "can_check_help:1:false" => Some(PeerHelpDenial::ObserverCannotCheck),
                "can_check_help:101:false" => Some(PeerHelpDenial::TargetCannotCheck),
                "state_1024:101:true" => Some(PeerHelpDenial::TargetRetreating),
                "flags_4194304:101:true" => Some(PeerHelpDenial::TargetConfused),
                "flags_262144:101:true" => Some(PeerHelpDenial::TargetStunned),
                _ => None,
            };
            let receipt = PeerHelpDenialReceipt {
                source: [0; 32],
                observer: NpcId {
                    spawn: 2,
                    generation: 0,
                },
                target: NpcId {
                    spawn: 3,
                    generation: 0,
                },
                helper_victim: f::player(),
                observer_in_combat: observer["flags"].as_u64().unwrap()
                    & u64::from(unit_flags::IN_COMBAT)
                    != 0,
                observer_can_call: observed["observer_can_call"].as_bool().unwrap(),
                target_can_call: observed["target_can_call"].as_bool().unwrap(),
                observer_can_check: observed["observer_can_check"].as_bool().unwrap(),
                target_can_check: observed["target_can_check"].as_bool().unwrap(),
                target_unit_state: target["authored_unit_state"].as_u64().unwrap() as u32,
                target_unit_flags: target["flags"].as_u64().unwrap() as u32,
                observer_can_initiate: None,
                observer_can_attack_victim: None,
                denial: PeerHelpDenial::CannotInitiateAttack,
                continuation: None,
            };
            assert_eq!(
                receipt.early_denial(),
                source_denial,
                "{} {}",
                case["name"],
                observed["phase"]
            );
            observed_prefixes += 1;
            if source_denial.is_none() {
                let can_initiate =
                    relations::can_initiate_attack(relations::CreatureInitiateAttackInput {
                        unit_state: observer["authored_unit_state"].as_u64().unwrap() as u32,
                        unit_flags: observer["flags"].as_u64().unwrap() as u32,
                        force_attacking_capability: observer["force_attacking"].as_bool().unwrap(),
                        can_aggro: observer["can_aggro"].as_bool().unwrap(),
                    });
                let source_reached_sight = leaves
                    .iter()
                    .any(|leaf| leaf.starts_with("evading_home:77:"));
                assert_eq!(
                    can_initiate, source_reached_sight,
                    "{} {}",
                    case["name"], observed["phase"]
                );
                observed_initiations += 1;
            }
        }
    }
    assert_eq!((observed_prefixes, observed_initiations), (55, 41));
}
