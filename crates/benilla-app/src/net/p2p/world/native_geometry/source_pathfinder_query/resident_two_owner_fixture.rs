// SPDX-License-Identifier: GPL-3.0-or-later
//! Complete authored two-owner inputs for the same installed resident products.
//! No gameplay admission, map-visitor producer, Route or NearPoint is authored here.
use super::*;
use benilla_sim::creature_init as init;
use benilla_world_runtime::native::{
    synthetic_two_owner_fixture as visitors, CreatureCapabilityAddonClosure,
    CreatureCollisionWidthCatalog, CreatureCollisionWidthRows, CreatureCollisionWidthSource,
    CreatureGenerationCatalog, HomeAddonClosure, SourceVisitorAddonClosure, SourceVisitorCatalog,
    SourceVisitorRows,
};

/// At 25 ms per tick, both unchanged 200 ms initial NPC notification events have
/// completed before this input. Their can-aggro latches are set while the player
/// is still 40 units away. The unchanged 125 ms relocation notification then
/// visits both owners before the per-NPC updates, ordinarily at tick 15.
#[cfg(not(benilla_headless_geometry))]
pub(crate) const TWO_OWNER_MOVE_TICK: u64 = 10;

/// The same eight-unit Chase placement as the singleton fixture. Submit this as
/// an ordinary authenticated MovementPose; do not edit the live world's pose.
#[cfg(not(benilla_headless_geometry))]
pub(crate) fn two_owner_chase_destination(profile: &Profile) -> ai::Point {
    assert_eq!(profile.npcs.len(), 2);
    let start = profile.npc(2).unwrap().ai.spawn;
    ai::Point::new(start.x.as_f32() - 8., start.y.as_f32(), start.z.as_f32()).unwrap()
}

/// Authored complete membership and visibility only. All actual resident
/// initialization, environment, NearPoint, Route and other spatial requests
/// remain the caller's responsibility; None never supplies a fallback fact.
#[cfg(not(benilla_headless_geometry))]
#[derive(Default)]
pub(crate) struct TwoOwnerVisitors {
    authored: visitors::CompleteAuthority,
}
#[cfg(not(benilla_headless_geometry))]
impl TwoOwnerVisitors {
    pub(crate) fn resolve(
        &mut self,
        request: &AuthorityRequest,
    ) -> Result<Option<Facts>, NativeError> {
        match &request.query {
            Query::SourceMotion(
                MotionQuery::SceneVisitor(_) | MotionQuery::VisitorVisibility(_),
            ) => {
                let reply = self.authored.resolve(request)?;
                if reply.request != *request {
                    return Err(NativeError::AuthorityBinding);
                }
                Ok(Some(reply.facts))
            }
            _ => Ok(None),
        }
    }
}

/// Extend the caller-editable draft before pathfinder selection, generated query
/// binding or any live owner exists. Both NPCs deliberately share entry 6 and its
/// exact template/model/equipment/interaction rows but have distinct spawn owners.
pub(super) fn extend_profile(profile: &mut Profile) {
    assert_eq!(profile.npcs.len(), 1);
    assert_eq!(profile.npcs[0].ai.creature_id, 2);
    assert_eq!(profile.npcs[0].entry, 6);
    assert!(profile.pathfinder_configuration.is_none());
    assert!(profile.source_visitors.is_none());

    let first = profile
        .creature_generations
        .as_ref()
        .unwrap()
        .source(2)
        .unwrap()
        .clone();
    let mut second = first.clone();
    second.spawn_id = 3;
    second.input.spawn_id = 3;
    let init::Rows::Complete(addons) = &mut second.input.spawn_addons else {
        panic!("complete retained spawn addon query");
    };
    for addon in addons {
        assert_eq!(addon.guid, 2);
        addon.guid = 3;
    }
    let second_identity = second.identity();
    assert_ne!(first.identity(), second_identity);

    let mut npc = profile.npcs[0].clone();
    npc.ai.creature_id = 3;
    profile.npcs.push(npc);
    let mut motion = profile.npc_motion(2).unwrap().clone();
    motion.npc_policy.home.addon_closure = HomeAddonClosure::GenerationSource {
        source_identity: second_identity,
    };
    assert!(profile
        .source_motion
        .as_mut()
        .unwrap()
        .npcs
        .insert(3, motion)
        .is_none());

    let first_capability = profile
        .creature_capabilities
        .as_ref()
        .unwrap()
        .source(2)
        .unwrap()
        .clone();
    let mut second_capability = first_capability.clone();
    second_capability.spawn_id = 3;
    second_capability.addon_closure = CreatureCapabilityAddonClosure::GenerationSource {
        source_identity: second_identity,
    };
    // Co-located authored spawns retain the same actual DryWater snapshot and
    // respawn position. Habitat 3 and raw spawn-template flags remain unchanged.
    profile.creature_generations = Some(
        CreatureGenerationCatalog::validate_sources(
            init::Rows::Complete(vec![first, second.clone()]),
            vec![2, 3],
        )
        .unwrap(),
    );
    profile.creature_capabilities = Some(
        CreatureCapabilityCatalog::validate_sources(CreatureCapabilityRows::Complete(vec![
            first_capability,
            second_capability,
        ]))
        .unwrap(),
    );

    let mut visitor_input = visitors::visitor_input(profile, vec![2, 3]);
    let SourceVisitorRows::Complete(owners) = &mut visitor_input.owners else {
        unreachable!();
    };
    for owner in owners {
        owner.addon_closure = SourceVisitorAddonClosure::GenerationSource {
            source_identity: profile
                .creature_generations
                .as_ref()
                .unwrap()
                .source(owner.spawn_id)
                .unwrap()
                .identity(),
        };
    }
    profile.source_visitors = Some(SourceVisitorCatalog::validate_sources(visitor_input).unwrap());

    let first_width = profile
        .creature_collision_widths
        .as_ref()
        .unwrap()
        .source(2)
        .unwrap()
        .clone();
    let (_, selection) = init::prepare(&second.input)
        .unwrap()
        .resolve_with_model_receipt(init::SelectionRolls {
            model: Some(0),
            gender: Some(0),
            level: Some(1),
        })
        .unwrap();
    let second_width = CreatureCollisionWidthSource::from_initial_model(
        3,
        selection,
        &models::catalog_with_model_heights(2.875, 4.125),
    )
    .unwrap();
    profile.creature_collision_widths = Some(
        CreatureCollisionWidthCatalog::validate_sources(CreatureCollisionWidthRows::Complete(
            vec![first_width, second_width],
        ))
        .unwrap(),
    );
    // Rebuild the complete draft height catalog from both retained model sources.
    // This never replaces a live owner or edits an initialized runtime scalar.
    profile.creature_collision_heights = None;
    profile.initialize_creature_collision_heights().unwrap();
    // Entry-keyed interactions/combat/equipment sources already cover entry 6.
    // Re-running the per-NPC fixture installer would create duplicate entry rows.
    profile.validate().unwrap();
}
