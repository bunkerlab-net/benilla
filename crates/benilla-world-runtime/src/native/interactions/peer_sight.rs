// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, UnitAI.cpp:68-101, Relations.cpp:351-416/1144-1161,
// Creature.cpp:2271 and Unit.cpp:6228; upstream GPL-2.0-or-later.
//! Read-only ordinary NPC/NPC sight prefix after the notifier's visibility gate.
use super::*;
use benilla_formats::Reaction;

/// Runtime-derived no-op receipts, never externally supplied AI decisions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PeerSightDisposition {
    ExistingOutdoorVictim,
    NeutralToAll,
    NonAggressive,
    CannotInitiateAttack,
    CannotAttackOnSight,
}

/// The notifying owner may already be removed by its consecutive Unit update.
/// Both callback directions borrow this complete overlay without removing a peer,
/// advancing its observation sequence, or reconstructing any pre-frame actor.
pub(in crate::native) fn source_peer_sight(
    profile: &Profile,
    state: &State,
    removed_owner: Option<&Npc>,
    observer: NpcId,
    target: NpcId,
) -> Result<PeerSightDisposition, NativeError> {
    let visitors = profile
        .source_visitors
        .as_ref()
        .ok_or(NativeError::UnsupportedMechanic(
            "peer sight requires complete source visitors",
        ))?;
    visitors.validate(profile)?;
    let catalog = profile
        .interactions
        .as_ref()
        .ok_or(NativeError::InvalidProfile)?;
    validate_abilities(state.abilities(profile))?;
    validate_scene(profile, state, removed_owner)?;
    let observer = current_npc(state, removed_owner, observer)?;
    let target = current_npc(state, removed_owner, target)?;
    if observer.id == target.id
        || !alive(observer)
        || !alive(target)
        || observer.ai.phase() == ai::Phase::ReturningHome
    {
        return Err(NativeError::AuthorityBinding);
    }
    let observer_profile = profile
        .npc(observer.id.spawn)
        .ok_or(NativeError::InvalidState)?;
    let target_profile = profile
        .npc(target.id.spawn)
        .ok_or(NativeError::InvalidState)?;
    let observer_source = &catalog.npcs[&observer_profile.entry];
    let target_source = &catalog.npcs[&target_profile.entry];
    let observer_faction = &catalog.factions[&observer_profile.faction_template];
    let target_faction = &catalog.factions[&target_profile.faction_template];

    // The selected closure derives ordinary Aggressive/Defensive EventAI from
    // raw ExtraFlags and proves ground actors. Passive/flying branches cannot run.
    if active_attack_victim(observer)? {
        return Ok(PeerSightDisposition::ExistingOutdoorVictim);
    }
    // Unit::IsNeutralToAll short-circuits faction zero before masks/reputation.
    let neutral = observer_faction.faction == 0
        || (!observer_faction
            .reputation_list_id
            .is_some_and(|id| id >= 0)
            && observer_faction.enemies.iter().all(|enemy| *enemy == 0)
            && observer_faction.enemy_group_mask == 0
            && observer_faction.friend_group_mask == 0);
    if neutral {
        return Ok(PeerSightDisposition::NeutralToAll);
    }
    // Core CheckForHelp precedes the non-Aggressive return and CanInitiateAttack.
    // Empty linking/spawn-group rows do not remove this source invocation.
    if target.ai.phase() == ai::Phase::Combat {
        return Err(NativeError::UnsupportedMechanic(
            "peer sight reached CheckForHelp",
        ));
    }
    // UnitAI.cpp:86: Defensive actors can reach help, but never initiate here.
    if observer_profile.ai.reaction != ai::Reaction::Aggressive {
        return Ok(PeerSightDisposition::NonAggressive);
    }
    let observer_flags = current_flags(observer_source, observer);
    if !relations::can_initiate_attack(relations::CreatureInitiateAttackInput {
        unit_state: observer_source.control.unit_state,
        unit_flags: observer_flags,
        force_attacking_capability: profile.source_pathfinding_extra_flags(observer.id.spawn)?
            & 0x0008_0000
            != 0,
        can_aggro: observer.events.can_aggro(),
    }) {
        return Ok(PeerSightDisposition::CannotInitiateAttack);
    }

    // UnitAI::AssistPlayerInCombat's base implementation is false. The complete
    // ordinary-AI closure excludes overrides, pets and successful feign death.
    if target.ai.phase() == ai::Phase::ReturningHome {
        return Ok(PeerSightDisposition::CannotAttackOnSight);
    }
    let target_flags = current_flags(target_source, target);
    if target_flags & relations::ATTACK_BLOCKING_FLAGS != 0
        || target_flags & unit_flags::IMMUNE_TO_NPC != 0
        || observer_flags & unit_flags::IMMUNE_TO_NPC != 0
    {
        return Ok(PeerSightDisposition::CannotAttackOnSight);
    }
    let observer_template = observer_faction.template();
    let target_template = target_faction.template();
    let observer_enemy =
        relations::faction_reaction(&observer_template, &target_template) == Reaction::Hostile;
    let target_enemy =
        relations::faction_reaction(&target_template, &observer_template) == Reaction::Hostile;
    let can_attack = observer_enemy || target_enemy;
    if !can_attack || !observer_enemy {
        return Ok(PeerSightDisposition::CannotAttackOnSight);
    }
    Err(NativeError::UnsupportedMechanic(
        "peer sight attack/accessibility continuation",
    ))
}

fn alive(npc: &Npc) -> bool {
    npc.health != 0 && !matches!(npc.ai.phase(), ai::Phase::Corpse | ai::Phase::Despawned)
}

fn current_flags(source: &NpcInteractionSource, npc: &Npc) -> u32 {
    source.unit_flags
        | if npc.ai.phase() == ai::Phase::Combat {
            unit_flags::IN_COMBAT
        } else {
            0
        }
}

fn current_npc<'a>(
    state: &'a State,
    removed_owner: Option<&'a Npc>,
    id: NpcId,
) -> Result<&'a Npc, NativeError> {
    removed_owner
        .filter(|npc| npc.id.spawn == id.spawn)
        .or_else(|| state.npcs.get(&id.spawn))
        .filter(|npc| npc.id == id)
        .ok_or(NativeError::AuthorityBinding)
}

fn validate_scene(
    profile: &Profile,
    state: &State,
    removed_owner: Option<&Npc>,
) -> Result<(), NativeError> {
    if state.npcs.len() + usize::from(removed_owner.is_some()) != profile.npcs.len()
        || removed_owner.is_some_and(|npc| state.npcs.contains_key(&npc.id.spawn))
        || state.npcs.iter().any(|(spawn, npc)| *spawn != npc.id.spawn)
    {
        return Err(NativeError::AuthorityBinding);
    }
    for row in &profile.npcs {
        let npc = removed_owner
            .filter(|npc| npc.id.spawn == row.ai.creature_id)
            .or_else(|| state.npcs.get(&row.ai.creature_id))
            .ok_or(NativeError::AuthorityBinding)?;
        // A live capability owner also binds the current generation. A public
        // receipt or a stale NpcId cannot construct or replace this live state.
        npc.creature_capabilities()
            .ok_or(NativeError::InvalidState)?
            .witness()
            .validate_for(profile, npc.id)?;
        if npc.source_motion.is_none() || npc.health > row.max_health {
            return Err(NativeError::InvalidState);
        }
    }
    Ok(())
}

fn active_attack_victim(npc: &Npc) -> Result<bool, NativeError> {
    if npc.ai.phase() != ai::Phase::Combat {
        return Ok(false);
    }
    // AttackStart installs Unit's victim before ThreatManager chooses its cached
    // victim. AttackStop(false) after lethal player damage retains TargetGuid.
    let detached = npc.ai.victim_detached_at_ms().is_some();
    if npc.ai.target() != Some(SOLO_UNIT)
        || npc.ai.hostile_reference().is_some() == detached
        || npc
            .ai
            .hostile_reference()
            .is_some_and(|reference| reference.target_guid() != SOLO_UNIT)
        || npc.ai.current_victim().is_some_and(|victim| {
            victim != SOLO_UNIT
                || npc
                    .ai
                    .hostile_reference()
                    .is_none_or(|reference| !reference.valid() || !reference.online())
        })
    {
        return Err(NativeError::InvalidState);
    }
    Ok(!detached)
}

#[cfg(test)]
mod tests {
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
                Err(NativeError::UnsupportedMechanic(
                    "peer sight reached CheckForHelp"
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
                "peer sight reached CheckForHelp"
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
                Ok(PeerSightDisposition::NonAggressive)
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
                Ok(PeerSightDisposition::CannotAttackOnSight)
            );
            assert_eq!(
                source_peer_sight(&world.profile, &state, Some(&owner), ids[1], ids[0]),
                Ok(PeerSightDisposition::NeutralToAll)
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
                Err(NativeError::UnsupportedMechanic(
                    "peer sight reached CheckForHelp"
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
                "peer sight reached CheckForHelp"
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
}
