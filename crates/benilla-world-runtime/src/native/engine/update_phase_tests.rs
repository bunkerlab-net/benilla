// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::native::{synthetic_fixture as f, synthetic_motion_fixture as fixture};

fn frame<'a, A: Authority>(world: &'a mut NativeWorld, authority: &'a mut A) -> Frame<'a, A> {
    world.state.tick += 1;
    world.state.now_ms += u64::from(world.profile.tick_ms);
    let movement_origin = world.state.position;
    Frame {
        read_only: false,
        profile: &world.profile,
        profile_digest: world.profile_digest,
        state: &mut world.state,
        authority,
        replies: vec![],
        events: vec![],
        draws: vec![],
        receipts: BTreeMap::new(),
        source_alive_unit: None,
        moved: false,
        cast_inputs: BTreeMap::new(),
        movement_origin,
        movement_path: vec![],
        player_auras_updated: false,
    }
}

fn with_aura(mut world: NativeWorld, authority: &mut impl Authority) -> NativeWorld {
    let input = f::cast(&world, 60001);
    world.step(1, Some(input), authority).unwrap();
    while world.state.npcs[&2].auras.is_empty() {
        assert!(world.state.tick < 100);
        world.step(world.state.tick + 1, None, authority).unwrap();
    }
    world
}

#[test]
fn owner_aura_update_leaves_other_holders_untouched_and_legacy_wrapper_visits_once() {
    // The existing legacy multi-NPC fixture exercises only the owner-aura helper;
    // it does not admit a second source-motion owner or a peer sight callback.
    let mut profile = f::profile();
    let mut other = f::profile().npcs.remove(0);
    other.ai.creature_id = 3;
    profile.npcs.push(other);
    let mut authority = f::SyntheticAuthority::default();
    let mut world = with_aura(
        NativeWorld::synthetic_component_test(profile, [8; 16], f::player(), 1234).unwrap(),
        &mut authority,
    );
    let mut holder = world.state.npcs[&2].auras[&60001].clone();
    holder.id = 3;
    world
        .state
        .npcs
        .get_mut(&3)
        .unwrap()
        .auras
        .insert(60001, holder);
    let mut legacy = world.clone();
    let before = world.state.npcs[&3].auras.clone();
    let mut update = frame(&mut world, &mut authority);
    let first = update.state.npcs[&2].id;
    let second = update.state.npcs[&3].id;
    assert_eq!(
        update.npc_auras(NpcId {
            generation: first.generation + 1,
            ..first
        }),
        Err(NativeError::UnknownTarget)
    );
    assert!(update.replies.is_empty());
    update.npc_auras(first).unwrap();
    assert_eq!(update.state.npcs[&3].auras, before);
    assert_ne!(
        update.state.npcs[&2].auras[&60001].aura(),
        before[&60001].aura()
    );
    update.npc_auras(second).unwrap();
    assert_eq!(update.replies.len(), 2);
    let mut legacy_authority = f::SyntheticAuthority::default();
    let mut legacy_update = frame(&mut legacy, &mut legacy_authority);
    legacy_update.auras().unwrap();
    assert_eq!(update.replies, legacy_update.replies);
    assert_eq!(update.events, legacy_update.events);
    assert_eq!(update.draws, legacy_update.draws);
    for spawn in [2, 3] {
        assert_eq!(
            update.state.npcs[&spawn].auras,
            legacy_update.state.npcs[&spawn].auras
        );
        assert_eq!(
            update.state.npcs[&spawn].health,
            legacy_update.state.npcs[&spawn].health
        );
    }
}

#[test]
fn source_death_deferral_matches_the_exact_alive_owner_incarnation() {
    for kind in 0..4 {
        let mut world = fixture::world();
        let owner = world.state.npcs[&2].id;
        world.state.npcs.get_mut(&2).unwrap().health = 0;
        let mut authority = fixture::SyntheticMotionAuthority::default();
        let mut update = frame(&mut world, &mut authority);
        update.source_alive_unit = match kind {
            0 => Some(owner),
            1 => Some(NpcId { spawn: 3, ..owner }),
            2 => Some(NpcId {
                generation: owner.generation + 1,
                ..owner
            }),
            _ => None,
        };
        update
            .source_combat_receipt(owner, ai::CombatEvent::Died)
            .unwrap();
        assert_eq!(update.state.npcs[&2].ai.has_pre_unit_death(), kind != 0);
    }
}

#[test]
fn source_owner_context_clears_after_success_and_each_failed_update_phase() {
    struct FailAt {
        phase: usize,
        npc_queries: usize,
        inner: fixture::SyntheticMotionAuthority,
    }
    impl Authority for FailAt {
        fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            let npc_query = matches!(request.query, Query::Npc { .. });
            let fails = match self.phase {
                1 => matches!(request.query, Query::SourceMotion(MotionQuery::Visitor(_))),
                2 => matches!(
                    request.query,
                    Query::Spell {
                        phase: CastPhase::Aura,
                        ..
                    }
                ),
                3 => npc_query && self.npc_queries == 0,
                4 => npc_query && self.npc_queries == 1,
                _ => false,
            };
            self.npc_queries += usize::from(npc_query);
            if fails {
                return Err(NativeError::AuthorityUnavailable(
                    "authored owner phase failure",
                ));
            }
            self.inner.resolve(request)
        }
    }
    let world = with_aura(
        fixture::world(),
        &mut fixture::SyntheticMotionAuthority::default(),
    );
    // With no visitor sight callback and no due damage, the two Npc queries
    // bind the receipt prelude and post-motion AI observation respectively.
    for phase in 0..5 {
        let mut world = world.clone();
        world
            .state
            .npcs
            .get_mut(&2)
            .unwrap()
            .events
            .schedule_notify(100, true)
            .unwrap();
        let owner = world.state.npcs[&2].id;
        let untouched = world.clone();
        let mut authority = FailAt {
            phase,
            npc_queries: 0,
            inner: fixture::SyntheticMotionAuthority {
                visits: Some(false),
                ..Default::default()
            },
        };
        let mut update = frame(&mut world, &mut authority);
        let result = update.source_npc_update(owner, &mut vec![]);
        assert_eq!(update.source_alive_unit, None, "phase={phase}");
        if phase == 0 {
            result.unwrap();
            assert_eq!(update.state.npcs[&2].id, owner);
        } else {
            assert_eq!(
                result,
                Err(NativeError::AuthorityUnavailable(
                    "authored owner phase failure"
                )),
                "phase={phase}"
            );
            let mut retry = untouched.clone();
            let mut failure = FailAt {
                phase,
                npc_queries: 0,
                inner: fixture::SyntheticMotionAuthority {
                    visits: Some(false),
                    ..Default::default()
                },
            };
            assert!(retry
                .step(retry.state.tick + 1, None, &mut failure)
                .is_err());
            assert_eq!(retry.checkpoint().unwrap(), untouched.checkpoint().unwrap());
            assert_eq!(retry.records(), untouched.records());
            let mut expected = untouched;
            for world in [&mut retry, &mut expected] {
                world
                    .step(
                        world.state.tick + 1,
                        None,
                        &mut fixture::SyntheticMotionAuthority {
                            visits: Some(false),
                            ..Default::default()
                        },
                    )
                    .unwrap();
            }
            assert_eq!(retry.checkpoint().unwrap(), expected.checkpoint().unwrap());
            assert_eq!(retry.last_tick(), expected.last_tick());
        }
    }
}

#[test]
fn source_owner_update_rejects_stale_identity_and_clears_context_across_respawn() {
    let mut world = fixture::world();
    let mut authority = fixture::SyntheticMotionAuthority::default();
    let owner = world.state.npcs[&2].id;
    {
        let mut update = frame(&mut world, &mut authority);
        assert_eq!(
            update.source_npc_update(
                NpcId {
                    generation: owner.generation + 1,
                    ..owner
                },
                &mut vec![]
            ),
            Err(NativeError::UnknownTarget)
        );
        assert_eq!(update.source_alive_unit, None);
        assert!(update.replies.is_empty());
    }
    let mut world = fixture::world();
    world.state.npcs.get_mut(&2).unwrap().health = 0;
    while world.state.npcs[&2].id.generation == owner.generation {
        assert!(world.state.tick < 30);
        let owner = world.state.npcs[&2].id;
        let mut update = frame(&mut world, &mut authority);
        update.source_npc_update(owner, &mut vec![]).unwrap();
        assert_eq!(update.source_alive_unit, None);
    }
    assert_eq!(world.state.npcs[&2].id.generation, owner.generation + 1);
}
