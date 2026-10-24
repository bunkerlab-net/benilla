// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use n::synthetic_fixture::{cast, world, SyntheticAuthority};

#[test]
fn committed_regeneration_updates_stock_npc_health_and_restored_initial_values() {
    let mut world = world();
    let mut projection =
        NativeProjection::new(&world, world.state().player, &super::tests::Catalog).unwrap();
    let mut authority = SyntheticAuthority::default();
    let cast = cast(&world, 60001);
    world.step(1, Some(cast), &mut authority).unwrap();
    projection.advance(&world).unwrap();
    let mut healed = false;
    for tick in 2..=300 {
        // Let the projectile damage the NPC first, then let the NPC kill this
        // authored player and return home under the existing AI state machine.
        if world.state().npcs[&2].health < 1000 {
            authority.melee_damage = 1000;
        }
        world.step(tick, None, &mut authority).unwrap();
        let events = projection.advance(&world).unwrap();
        let npc = &world.state().npcs[&2];
        if let Some(n::Event::NpcHealthRegenerated { id, before, after }) = world
            .last_tick()
            .unwrap()
            .events
            .iter()
            .find(|e| matches!(e, n::Event::NpcHealthRegenerated { .. }))
        {
            assert!(*after > *before);
            assert_eq!(*after, npc.health);
            let guid = projection.npc_guid(*id).unwrap();
            assert!(events.iter().any(|e|matches!(e,SessionEvent::ObjectValues{guid:g,fields} if *g==guid && fields.unit_health()==Some(*after))));
            let restored = n::NativeWorld::restore_synthetic_component_test(
                world.profile().clone(),
                n::Checkpoint::decode(&world.checkpoint().unwrap().encode().unwrap()).unwrap(),
            )
            .unwrap();
            let fresh =
                NativeProjection::new(&restored, restored.state().player, &super::tests::Catalog)
                    .unwrap();
            assert_eq!(
                fresh
                    .npc_fields(&restored, &restored.state().npcs[&2])
                    .unwrap()
                    .unit_health(),
                Some(*after)
            );
            assert_eq!(
                restored.state().npcs[&2].regen_elapsed_ms,
                npc.regen_elapsed_ms
            );
            healed = true;
            break;
        }
    }
    assert!(
        healed,
        "damaged NPC should regenerate during ordinary evade/home updates"
    );
}

struct BlockChase {
    inner: SyntheticAuthority,
    block: bool,
}
impl n::Authority for BlockChase {
    fn resolve(
        &mut self,
        request: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        let mut result = n::Authority::resolve(&mut self.inner, request)?;
        if self.block
            && matches!(&request.query,n::Query::Movement(r) if matches!(r.purpose,ai::movement::MovementPurpose::Chase{..}))
        {
            if let n::Facts::Movement(outcome) = &mut result.facts {
                outcome.result =
                    ai::movement::MovementResult::Blocked(ai::movement::BlockReason::Obstacle);
            }
        }
        Ok(result)
    }
}
#[test]
fn timed_evade_projects_zero_damage_and_accumulated_absorb_after_reopen() {
    let mut profile = n::synthetic_fixture::profile();
    profile.npcs[0].ai.spawn = n::synthetic_fixture::point(1.0);
    profile.npcs[0].ai.reaction = ai::Reaction::Defensive;
    let mut world = n::NativeWorld::synthetic_component_test(
        profile,
        [8; 16],
        n::synthetic_fixture::player(),
        1234,
    )
    .unwrap();
    let mut authority = BlockChase {
        inner: SyntheticAuthority::default(),
        block: false,
    };
    let input = cast(&world, 60001);
    world.step(1, Some(input), &mut authority).unwrap();
    for tick in 2..=30 {
        world.step(tick, None, &mut authority).unwrap();
    }
    let mut projection =
        NativeProjection::new(&world, world.state().player, &super::tests::Catalog).unwrap();
    let command = n::OwnedInput {
        authenticated_owner: world.state().player.owner,
        input: n::Input {
            world: world.state().world,
            tick: 31,
            player: world.state().player,
            sequence: world.state().next_sequence,
            command: n::Command::Move {
                destination: n::synthetic_fixture::point(100.0),
            },
        },
    };
    world.step(31, Some(command), &mut authority).unwrap();
    projection.advance(&world).unwrap();
    authority.block = true;
    world.step(32, None, &mut authority).unwrap();
    projection.advance(&world).unwrap();
    let health = world.state().npcs[&2].health;
    let mut world = n::NativeWorld::restore_synthetic_component_test(
        world.profile().clone(),
        n::Checkpoint::decode(&world.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap();
    let mut projection =
        NativeProjection::new(&world, world.state().player, &super::tests::Catalog).unwrap();
    let guid = projection.npc_guid(world.state().npcs[&2].id).unwrap();
    let mut absorbed = false;
    for tick in 33..=38 {
        world.step(tick, None, &mut authority).unwrap();
        let events = projection.advance(&world).unwrap();
        absorbed|=events.iter().any(|e|matches!(e,SessionEvent::PeriodicAuraLog(log) if log.target==guid && matches!(&log.ticks[..],[p::PeriodicTick::Damage{amount:0,absorb,resist:0,..}] if *absorb>0)));
        assert_eq!(world.state().npcs[&2].health, health);
    }
    assert!(absorbed);
}
