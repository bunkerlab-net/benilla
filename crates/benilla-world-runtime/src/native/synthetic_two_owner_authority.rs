// SPDX-License-Identifier: GPL-3.0-or-later
//! Complete expressly authored cell membership and directional AI visibility.
//! These assumed spatial answers are not an installed-map traversal producer.
use crate::native::*;

#[derive(Default)]
pub struct CompleteAuthority {
    pub base: synthetic_combat_fixture::SpatialOnly,
    pub queries: Vec<AuthorityRequest>,
    pub derived: Vec<AuthorityReply>,
    /// Explicit within-grid order; independent of the profile's update order.
    pub grid_order: Option<Vec<u64>>,
    pub not_visited: Vec<VisitorParticipant>,
    pub membership: Option<SceneVisitorFacts>,
    pub visibility: Option<benilla_sim::creature_ai::movement::Check>,
    pub visibility_overrides: Vec<(
        VisitorParticipant,
        VisitorParticipant,
        benilla_sim::creature_ai::movement::Check,
    )>,
    pub fail_visibility_owner: Option<u64>,
    pub fail_derived_owner: Option<u64>,
}
impl Authority for CompleteAuthority {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        use benilla_sim::creature_ai::movement::Check;
        self.queries.push(request.clone());
        let facts = match &request.query {
            Query::SourceMotion(MotionQuery::SceneVisitor(scene)) => {
                let membership = match &self.membership {
                    Some(facts) => facts.clone(),
                    None => {
                        let order = self.grid_order.clone().unwrap_or_else(|| {
                            scene.creatures.iter().map(|npc| npc.id.spawn).collect()
                        });
                        let grid_creatures = order
                            .into_iter()
                            .map(|spawn| {
                                scene
                                    .creatures
                                    .iter()
                                    .find(|npc| npc.id.spawn == spawn)
                                    .map(|npc| npc.id)
                                    .ok_or(NativeError::AuthorityBinding)
                            })
                            .collect::<Result<Vec<_>, _>>()?
                            .into_iter()
                            .filter(|id| {
                                !self
                                    .not_visited
                                    .contains(&VisitorParticipant::Creature(*id))
                            })
                            .collect();
                        let world_players = if self
                            .not_visited
                            .contains(&VisitorParticipant::Player(scene.player))
                        {
                            vec![]
                        } else {
                            vec![scene.player]
                        };
                        SceneVisitorFacts::Complete {
                            loaded_membership: Check::Passed,
                            grid_creatures,
                            world_players,
                            not_visited: self.not_visited.clone(),
                        }
                    }
                };
                MotionFacts::SceneVisitor(membership)
            }
            Query::SourceMotion(MotionQuery::VisitorVisibility(sight)) => {
                if matches!(sight.notification_owner, VisitorParticipant::Creature(id) if self.fail_visibility_owner == Some(id.spawn))
                {
                    return Err(NativeError::AuthorityUnavailable(
                        "authored late owner visibility fault",
                    ));
                }
                MotionFacts::VisitorVisibility(
                    self.visibility_overrides
                        .iter()
                        .find(|(observer, target, _)| {
                            *observer == sight.observer && *target == sight.target
                        })
                        .map(|(_, _, check)| *check)
                        .or(self.visibility)
                        .unwrap_or(Check::Passed),
                )
            }
            Query::SourceMotion(MotionQuery::VisitorSight(_)) => {
                return Err(NativeError::AuthorityUnavailable(
                    "runtime-owned visitor semantics escaped",
                ))
            }
            Query::SourceMotion(MotionQuery::Visitor(_)) => {
                return Err(NativeError::AuthorityUnavailable(
                    "selected visitors fell back to singleton adapter",
                ))
            }
            _ => return self.base.resolve(request),
        };
        Ok(AuthorityReply {
            request: request.clone(),
            facts: Facts::SourceMotion(facts),
        })
    }
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        self.derived.push(reply.clone());
        if matches!(&reply.request.query, Query::SourceMotion(MotionQuery::VisitorSight(sight)) if matches!(sight.notification_owner, VisitorParticipant::Creature(id) if self.fail_derived_owner == Some(id.spawn)))
        {
            return Err(NativeError::AuthorityUnavailable(
                "authored late owner derived receipt fault",
            ));
        }
        self.base.observe_derived(reply)
    }
}
