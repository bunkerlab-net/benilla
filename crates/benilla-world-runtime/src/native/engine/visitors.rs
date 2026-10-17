// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, GridNotifiersImpl.h:49–56,104–123,147–181.
//! Selected complete authored grid/world traversal. Peer callbacks are read-only;
//! any reached unrepresented help/attack branch rejects the enclosing staged frame.
use super::*;
use benilla_sim::{creature_motion as m, event_processor::EventCallbackSnapshot};

#[derive(Clone, Copy)]
struct Visit {
    owner: VisitorParticipant,
    container: VisitorContainer,
    index: u32,
    direction: VisitorDirection,
    observer: NpcId,
    target: VisitorParticipant,
}
impl<A: Authority> Frame<'_, A> {
    fn visitor_scene(&self, working: Option<&Npc>) -> Result<State, NativeError> {
        let catalog = self
            .profile
            .source_visitors
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        catalog.validate_scene(self.profile)?;
        // The executing owner is absent from State while its event callback owns
        // its queue. Reinsert that exact current borrow, never a pre-frame copy.
        let mut scene = self.state.clone();
        if let Some(npc) = working {
            if scene.npcs.insert(npc.id.spawn, npc.clone()).is_some() {
                return Err(NativeError::AuthorityBinding);
            }
        }
        if scene.npcs.len() != catalog.owner_order().len()
            || catalog.owner_order().iter().any(|spawn| {
                scene
                    .npcs
                    .get(spawn)
                    .is_none_or(|npc| npc.id.spawn != *spawn)
            })
        {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(scene)
    }
    fn scene_witness(
        &self,
        working: Option<&Npc>,
        callback: &EventCallbackSnapshot,
    ) -> Result<SceneWitness, NativeError> {
        let scene = self.visitor_scene(working)?;
        Ok(SceneWitness {
            frame: interactions::witness(self.profile_digest, &scene, self.interaction_moment())?,
            callback: crate::digest(b"benilla-native-visitor-callback-v39\0", callback),
        })
    }
    fn visitor_binding(owner: VisitorParticipant) -> Option<NpcId> {
        match owner {
            VisitorParticipant::Player(_) => None,
            VisitorParticipant::Creature(id) => Some(id),
        }
    }
    fn selected_membership(
        &mut self,
        working: Option<&Npc>,
        callback: &EventCallbackSnapshot,
    ) -> Result<(Vec<NpcId>, Vec<crate::PlayerId>), NativeError> {
        let scene = self.visitor_scene(working)?;
        let owner = working.map_or(VisitorParticipant::Player(scene.player), |npc| {
            VisitorParticipant::Creature(npc.id)
        });
        let genesis = self
            .profile
            .source_motion
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let request = SceneVisitorRequest {
            witness: self.scene_witness(working, callback)?,
            notification_owner: owner,
            callback: callback.clone(),
            event_clock_ms: callback.due.now_ms,
            now_ms: scene.now_ms,
            radius: m::visitor_radius(
                match owner {
                    VisitorParticipant::Player(_) => 18,
                    VisitorParticipant::Creature(id) => {
                        self.profile.npc_motion(id.spawn)?.creature_detection_range
                    }
                },
                genesis.aggro_rate,
            )?,
            owner_order: self
                .profile
                .source_visitors
                .as_ref()
                .unwrap()
                .owner_order()
                .to_vec(),
            player: scene.player,
            player_pose: m::Pose {
                position: scene.position,
                orientation: ai::Real::from_bits(scene.orientation.bits())?,
            },
            player_alive: scene.health > 0,
            creatures: scene
                .npcs
                .values()
                .map(|npc| {
                    Ok(VisitorCreature {
                        id: npc.id,
                        pose: npc
                            .source_motion
                            .as_ref()
                            .ok_or(NativeError::InvalidState)?
                            .motion
                            .pose(),
                        health: npc.health,
                        phase: npc.ai.phase(),
                        can_aggro: npc.events.can_aggro(),
                    })
                })
                .collect::<Result<_, NativeError>>()?,
        };
        let facts = self.ask(
            Self::visitor_binding(owner),
            Query::SourceMotion(MotionQuery::SceneVisitor(Box::new(request))),
        )?;
        let (grid, world, outside) = match facts {
            Facts::SourceMotion(MotionFacts::SceneVisitor(SceneVisitorFacts::Complete {
                loaded_membership: ai::movement::Check::Passed,
                grid_creatures,
                world_players,
                not_visited,
            })) => (grid_creatures, world_players, not_visited),
            Facts::SourceMotion(MotionFacts::SceneVisitor(
                SceneVisitorFacts::UnrepresentedParticipants,
            )) => {
                return Err(NativeError::UnsupportedMechanic(
                    "source visitor unrepresented participants",
                ));
            }
            Facts::SourceMotion(MotionFacts::SceneVisitor(_)) => {
                return Err(NativeError::AuthorityUnavailable(
                    "complete selected source visitor membership",
                ));
            }
            _ => return Err(NativeError::AuthorityBinding),
        };
        // Validate exact inventory without rearranging either returned traversal.
        let mut remaining = scene
            .npcs
            .values()
            .map(|npc| VisitorParticipant::Creature(npc.id))
            .collect::<Vec<_>>();
        remaining.push(VisitorParticipant::Player(scene.player));
        if grid.len() + world.len() + outside.len() != remaining.len() {
            return Err(NativeError::AuthorityBinding);
        }
        for participant in grid
            .iter()
            .copied()
            .map(VisitorParticipant::Creature)
            .chain(world.iter().copied().map(VisitorParticipant::Player))
            .chain(outside)
        {
            let index = remaining
                .iter()
                .position(|p| *p == participant)
                .ok_or(NativeError::AuthorityBinding)?;
            remaining.remove(index);
        }
        if !remaining.is_empty() {
            return Err(NativeError::AuthorityBinding);
        }
        Ok((grid, world))
    }
    fn visitor_npc<'b>(
        &'b self,
        working: Option<&'b Npc>,
        id: NpcId,
    ) -> Result<&'b Npc, NativeError> {
        working
            .filter(|npc| npc.id == id)
            .or_else(|| self.state.npcs.get(&id.spawn).filter(|npc| npc.id == id))
            .ok_or(NativeError::AuthorityBinding)
    }
    fn sight_request(
        &self,
        working: Option<&Npc>,
        callback: &EventCallbackSnapshot,
        visit: Visit,
    ) -> Result<VisitorSightRequest, NativeError> {
        Ok(VisitorSightRequest {
            witness: self.scene_witness(working, callback)?,
            notification_owner: visit.owner,
            callback: callback.clone(),
            container: visit.container,
            visit_index: visit.index,
            direction: visit.direction,
            observer: VisitorParticipant::Creature(visit.observer),
            target: visit.target,
        })
    }
    fn record_sight(
        &mut self,
        request: VisitorSightRequest,
        disposition: SightDisposition,
    ) -> Result<(), NativeError> {
        if self.replies.len() >= 20_000 {
            return Err(NativeError::TooLarge);
        }
        let reply = AuthorityReply {
            request: AuthorityRequest {
                binding: Binding {
                    world: self.state.world,
                    tick: self.state.tick,
                    profile: self.profile_digest,
                    authority_policy: self.profile.authority_policy,
                    geometry: self.profile.geometry_revision,
                    npc: Self::visitor_binding(request.notification_owner),
                },
                query: Query::SourceMotion(MotionQuery::VisitorSight(Box::new(request))),
            },
            facts: Facts::SourceMotion(MotionFacts::VisitorSight(disposition)),
        };
        self.authority.observe_derived(&reply)?;
        self.replies.push(reply);
        Ok(())
    }
    fn selected_visible(
        &mut self,
        working: Option<&Npc>,
        callback: &EventCallbackSnapshot,
        visit: Visit,
    ) -> Result<bool, NativeError> {
        // All admitted raw creation/control hooks prove no lost-control state.
        // The current evade state still precedes AI visibility in the notifier.
        if self.visitor_npc(working, visit.observer)?.ai.is_evading() {
            let request = self.sight_request(working, callback, visit)?;
            self.record_sight(request, SightDisposition::Evading)?;
            return Ok(false);
        }
        let request = self.sight_request(working, callback, visit)?;
        let Facts::SourceMotion(MotionFacts::VisitorVisibility(check)) = self.ask(
            Self::visitor_binding(visit.owner),
            Query::SourceMotion(MotionQuery::VisitorVisibility(Box::new(request))),
        )?
        else {
            return Err(NativeError::AuthorityBinding);
        };
        match check {
            ai::movement::Check::Passed => Ok(true),
            ai::movement::Check::Failed => {
                let request = self.sight_request(working, callback, visit)?;
                self.record_sight(request, SightDisposition::NotVisible)?;
                Ok(false)
            }
            ai::movement::Check::Unknown => Err(NativeError::AuthorityUnavailable(
                "selected directional AI visibility",
            )),
        }
    }
    fn selected_peer_sight(
        &mut self,
        working: &Npc,
        callback: &EventCallbackSnapshot,
        visit: Visit,
    ) -> Result<(), NativeError> {
        if !self.selected_visible(Some(working), callback, visit)? {
            return Ok(());
        }
        let VisitorParticipant::Creature(target) = visit.target else {
            return Err(NativeError::InvalidState);
        };
        let disposition = interactions::source_peer_sight(
            self.profile,
            self.state,
            Some(working),
            visit.observer,
            target,
        )?;
        let request = self.sight_request(Some(working), callback, visit)?;
        self.record_sight(request, SightDisposition::Peer(disposition))
    }
    fn selected_player_sight(
        &mut self,
        npc: &mut Npc,
        callback: &EventCallbackSnapshot,
        visit: Visit,
    ) -> Result<(), NativeError> {
        if !self.selected_visible(Some(npc), callback, visit)? {
            return Ok(());
        }
        let facts = self.source_npc_facts_with_visibility(npc, CombatPhase::Sight, Some(true))?;
        let observation = self.source_observation(npc, facts, None, Vec::new())?;
        let effects = npc.ai.source_sight_event(&observation)?;
        self.source_prefix_effects(
            npc,
            &effects,
            &EventAiCallbackKind::Sight {
                notification_owner: visit.owner,
                callback: callback.clone(),
            },
        )?;
        let request = self.sight_request(Some(npc), callback, visit)?;
        self.record_sight(request, SightDisposition::PlayerSightApplied)
    }
    pub(super) fn source_selected_player_notify(
        &mut self,
        callback: &EventCallbackSnapshot,
    ) -> Result<(), NativeError> {
        let (grid, _world) = self.selected_membership(None, callback)?;
        if self.state.health == 0 {
            return Ok(());
        }
        let owner = VisitorParticipant::Player(self.state.player);
        for (index, id) in grid.into_iter().enumerate() {
            // Membership retains dead creatures; the notifier, not the provider,
            // suppresses their callbacks. The ordinary player has no player AI.
            if self.visitor_npc(None, id)?.health == 0 {
                continue;
            }
            let mut npc = self
                .state
                .npcs
                .remove(&id.spawn)
                .ok_or(NativeError::InvalidState)?;
            let result = self.selected_player_sight(
                &mut npc,
                callback,
                Visit {
                    owner,
                    container: VisitorContainer::GridCreatures,
                    index: index as u32,
                    direction: VisitorDirection::VisitedToOwner,
                    observer: id,
                    target: owner,
                },
            );
            self.state.npcs.insert(id.spawn, npc);
            result?;
        }
        Ok(())
    }
    pub(super) fn source_selected_npc_notify(
        &mut self,
        npc: &mut Npc,
        callback: &EventCallbackSnapshot,
    ) -> Result<(), NativeError> {
        let (grid, world) = self.selected_membership(Some(npc), callback)?;
        if npc.health == 0 {
            return Ok(());
        }
        let owner = VisitorParticipant::Creature(npc.id);
        // CellImpl.h visits the complete grid container before the world container.
        for (index, id) in grid.into_iter().enumerate() {
            if id == npc.id || self.visitor_npc(Some(npc), id)?.health == 0 {
                continue;
            }
            let visit = Visit {
                owner,
                container: VisitorContainer::GridCreatures,
                index: index as u32,
                direction: VisitorDirection::VisitedToOwner,
                observer: id,
                target: owner,
            };
            self.selected_peer_sight(npc, callback, visit)?;
            self.selected_peer_sight(
                npc,
                callback,
                Visit {
                    direction: VisitorDirection::OwnerToVisited,
                    observer: npc.id,
                    target: VisitorParticipant::Creature(id),
                    ..visit
                },
            )?;
        }
        for (index, player) in world.into_iter().enumerate() {
            if self.state.health == 0 {
                continue;
            }
            self.selected_player_sight(
                npc,
                callback,
                Visit {
                    owner,
                    container: VisitorContainer::WorldPlayers,
                    index: index as u32,
                    direction: VisitorDirection::OwnerToVisited,
                    observer: npc.id,
                    target: VisitorParticipant::Player(player),
                },
            )?;
        }
        Ok(())
    }
}
