// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, GridNotifiers.cpp:265-281 and
// Relations.cpp:458-513,1144-1161,1420-1437; upstream GPL-2.0-or-later.
//! Read-only source assistance predicates over an exact current scene overlay.
use super::*;
use benilla_formats::Reaction;

#[cfg(test)]
mod tests;

/// `scene` includes the just-updated owner, with its current callback, queue and
/// latch. No provider chooses eligibility, and no pre-frame owner is reconstructed.
pub(in crate::native) fn source_assistance_traversal(
    profile: &Profile,
    scene: &State,
    request: &CombatAssistanceRequest,
    facts: &CombatAssistanceFacts,
) -> Result<AssistanceTraversalReceipt, NativeError> {
    let candidates = request.candidates(facts)?;
    let visitors = profile
        .source_visitors
        .as_ref()
        .ok_or(NativeError::AuthorityUnavailable(
            "assistance complete source visitors",
        ))?;
    visitors.validate(profile)?;
    let catalog = profile
        .interactions
        .as_ref()
        .ok_or(NativeError::InvalidProfile)?;
    catalog.validate(profile)?;
    validate_abilities(scene.abilities(profile))?;
    if scene.npcs.len() != profile.npcs.len()
        || request.binding.world != scene.world
        || request.binding.tick != scene.tick
        || request.binding.profile != profile.digest()
        || request.binding.authority_policy != profile.authority_policy
        || request.binding.geometry != profile.geometry_revision
        || request.content != profile.content_digest
        || request.enemy != scene.player
        || request.now_ms != scene.now_ms
        || request.rng_counter != scene.rng_counter
        || request.scene_inventory != scene.npcs.values().map(|npc| npc.id).collect::<Vec<_>>()
        || request.scene != witness(profile.digest(), scene, request.moment)?.draft
    {
        return Err(NativeError::AuthorityBinding);
    }
    for row in &profile.npcs {
        let npc = scene
            .npcs
            .get(&row.ai.creature_id)
            .ok_or(NativeError::AuthorityBinding)?;
        if npc.id.spawn != row.ai.creature_id
            || npc.health > npc.effective_profile(profile)?.max_health
            || npc.source_motion.is_none()
        {
            return Err(NativeError::AuthorityBinding);
        }
        npc.creature_capabilities()
            .ok_or(NativeError::InvalidState)?
            .witness()
            .validate_for(profile, npc.id)?;
    }
    let owner_id = request.binding.npc.ok_or(NativeError::AuthorityBinding)?;
    let owner = scene
        .npcs
        .get(&owner_id.spawn)
        .filter(|npc| npc.id == owner_id)
        .ok_or(NativeError::AuthorityBinding)?;
    if request.owner_pose != owner.source_motion.as_ref().unwrap().motion.pose()
        || request.enemy_pose.position != scene.position
        || request.enemy_pose.orientation.bits() != scene.orientation.bits()
        || request.unit_clock_ms != owner.events.clock_ms()
    {
        return Err(NativeError::AuthorityBinding);
    }
    let owner_profile = profile
        .npc(owner_id.spawn)
        .ok_or(NativeError::InvalidState)?;
    let owner_source = &catalog.npcs[&owner_profile.entry];
    let owner_faction = catalog.factions[&owner_profile.faction_template].template();
    let mut receipts = Vec::with_capacity(candidates.len());
    for (container, id) in candidates {
        let candidate = &scene.npcs[&id.spawn];
        // Self is pointer identity, never entry identity. The outdoor combat
        // denial precedes CanAssist's flags/faction and enemy sight predicates.
        let denial = if id == owner_id {
            AssistanceDenial::SelfCandidate
        } else if candidate.ai.phase() == ai::Phase::Combat {
            AssistanceDenial::AlreadyInCombat
        } else if owner_source.unit_flags & unit_flags::UNINTERACTIBLE != 0 {
            AssistanceDenial::OwnerUninteractible
        } else {
            let candidate_profile = profile.npc(id.spawn).ok_or(NativeError::InvalidState)?;
            let candidate_source = &catalog.npcs[&candidate_profile.entry];
            let candidate_faction =
                catalog.factions[&candidate_profile.faction_template].template();
            if relations::faction_reaction(&candidate_faction, &owner_faction) < Reaction::Friendly
            {
                AssistanceDenial::NotFriendlyToOwner
            } else {
                // The source audits exclude masters, charm and player control;
                // ordinary friendly NPCs therefore pass CanAssist immediately.
                if (candidate_source.unit_flags | owner_source.unit_flags)
                    & unit_flags::PLAYER_CONTROLLED
                    != 0
                {
                    return Err(NativeError::UnsupportedMechanic(
                        "controlled assistance candidate",
                    ));
                }
                // WorkingNpc requires exactly its own current owner detached.
                // Restore the caller first (in scene), then detach this candidate.
                let mut overlay = scene.clone();
                let candidate = overlay
                    .npcs
                    .remove(&id.spawn)
                    .ok_or(NativeError::AuthorityBinding)?;
                if WorkingNpc::from_npc(&candidate)?
                    .npc_semantics(profile, &overlay)?
                    .can_attack_on_sight
                {
                    return Err(NativeError::UnsupportedMechanic(
                        "assistance candidate reached range/LOS",
                    ));
                }
                AssistanceDenial::CannotAttackEnemyOnSight
            }
        };
        receipts.push(AssistanceCandidateReceipt {
            container,
            npc: id,
            denial,
        });
    }
    let receipt = AssistanceTraversalReceipt {
        request: crate::digest(b"benilla-native-assistance-traversal-v46\0", request),
        candidates: receipts,
    };
    receipt.validate(request, facts)?;
    Ok(receipt)
}
