// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-ordered IsSwimmable observations from dry inventories or resident points.
//! This provider neither creates a native admission token nor invents model data.
use super::{
    core::Failure,
    products::{
        ground_height::MapHeightSnapshot,
        liquid::{LiquidRequest, LiquidStatus},
        Error,
    },
};
use benilla_world_runtime::{
    native::{
        environment, AuthorityRequest, CreatureInitializationClosure, CreatureInitializationFacts,
        CreatureInitializationSnapshot, Facts, Profile, Query,
    },
    WorldId,
};
use sha2::{Digest, Sha256};
use std::time::Instant;

#[path = "source_creature_initialization/resident.rs"]
pub(crate) mod resident;

const POLICY: &[u8] = b"cmangos-8ec338a1-creature-initialization-complete-dry-products-v1";

/// The loaded reader, not a public binding DTO, owns complete dry product proof.
/// Its existing MapHeight construction also requires positive effective VMap
/// configuration and a separately retained empty-dynamic participant closure.
pub(crate) struct SourceCreatureInitializationProvider<'a> {
    world: WorldId,
    profile: Profile,
    snapshot: MapHeightSnapshot<'a>,
    binding: CreatureInitializationSnapshot,
}

pub(crate) fn snapshot_binding(snapshot: &MapHeightSnapshot<'_>) -> CreatureInitializationSnapshot {
    let height = snapshot.binding();
    let liquid = snapshot.dry_liquid_binding();
    let configuration = snapshot.configuration();
    let mut policy = Sha256::new();
    policy.update(POLICY);
    policy.update(height.policy);
    policy.update(liquid.policy);
    policy.update(configuration.effective_config);
    policy.update([u8::from(configuration.vmap_height_enabled)]);
    CreatureInitializationSnapshot {
        map: height.map,
        content: height.content,
        source_epoch: height.source_epoch,
        inventory: height.inventory,
        policy: policy.finalize().into(),
        effective_config: configuration.effective_config,
        vmap_height_enabled: configuration.vmap_height_enabled,
    }
}

impl<'a> SourceCreatureInitializationProvider<'a> {
    pub(crate) fn bind(
        world: WorldId,
        profile: &Profile,
        snapshot: MapHeightSnapshot<'a>,
    ) -> Result<Self, Failure> {
        profile.validate().map_err(|_| Failure::Binding)?;
        let binding = snapshot_binding(&snapshot);
        if world == [0; 16]
            || binding.map != profile.map_id
            || binding.content != profile.content_digest
            || !binding.vmap_height_enabled
        {
            return Err(Failure::Binding);
        }
        let catalog = profile
            .creature_capabilities
            .as_ref()
            .ok_or(Failure::UnsupportedQuery)?;
        if !profile.npcs.iter().any(|npc| {
            catalog.source(npc.ai.creature_id).is_some_and(|source| {
                matches!(&source.initialization,
                    CreatureInitializationClosure::DryWater { snapshot, .. }
                        if snapshot == &binding)
            })
        }) {
            return Err(Failure::Binding);
        }
        Ok(Self {
            world,
            profile: profile.clone(),
            snapshot,
            binding,
        })
    }

    pub(crate) fn resolve(
        &self,
        request: &AuthorityRequest,
        deadline: Instant,
    ) -> Result<Facts, Failure> {
        if Instant::now() >= deadline {
            return Err(Failure::Deadline);
        }
        let Query::CreatureInitialization(query) = &request.query else {
            return Err(Failure::UnsupportedQuery);
        };
        if request.binding != query.binding
            || request.binding.world != self.world
            || request.binding.profile != self.profile.digest()
            || request.binding.authority_policy != self.profile.authority_policy
            || request.binding.geometry != self.profile.geometry_revision
            || query.snapshot != self.binding
        {
            return Err(Failure::Binding);
        }
        query
            .validate_for(&self.profile)
            .map_err(|_| Failure::Binding)?;
        // GridMap.cpp:1147-1160 passes selected model height as the depth
        // threshold; its nested liquid call uses the separate default height.
        // Complete NO_WATER makes the threshold comparison unreachable, while
        // both model inputs remain bound by the source-owned runtime request.
        let liquid_request = LiquidRequest {
            snapshot: self.snapshot.dry_liquid_binding(),
            ticket: query.ticket,
            position: [
                query.position.x.bits(),
                query.position.y.bits(),
                query.position.z.bits(),
            ],
        };
        let reply = self
            .snapshot
            .query_dry_liquid(liquid_request, deadline)
            .map_err(|error| match error {
                Error::Identity => Failure::Binding,
                Error::Deadline => Failure::Deadline,
                _ => Failure::MissingGeometry,
            })?;
        if reply.request != liquid_request || reply.status != LiquidStatus::NoWater {
            return Err(Failure::Binding);
        }
        if Instant::now() >= deadline {
            return Err(Failure::Deadline);
        }
        Ok(Facts::CreatureInitialization(Box::new(
            CreatureInitializationFacts {
                request: (**query).clone(),
                status: environment::LiquidStatus::NoWater,
                data: None,
            },
        )))
    }
}

#[cfg(test)]
#[path = "source_creature_initialization/tests.rs"]
pub(super) mod tests;
