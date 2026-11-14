// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored closed-domain runtime evidence, not native world admission.
use super::*;
use environment::{
    BoundLiquidResult, EnvironmentSnapshotBinding, LiquidCatalog, LiquidData, LiquidStatus,
};
use synthetic_fixture as f;

fn profile(masks: [u32; 2]) -> Profile {
    let mut p = synthetic_combat_fixture::profile();
    p.tick_ms = 25;
    p.source_motion
        .as_mut()
        .unwrap()
        .creature_initial_aggro_delay_ms = 1000;
    p.initialize_creature_capabilities(synthetic_motion_fixture::creature_capability_sources(&p))
        .unwrap();
    p.initialize_player_collision_height(
        &synthetic_player_model_fixture::catalog(),
        &synthetic_creature_model_fixture::catalog_with_height_fields(1.0, 2.0, 1.0, 1.0, false),
    )
    .unwrap();
    let sources = [(61004, -40), (61005, -60)]
        .into_iter()
        .zip(masks)
        .map(|((id, amount), mask)| {
            let mut s = player_slow::source::synthetic_source(id, amount, 2000);
            s.spell.words[22] = mask;
            s
        })
        .collect();
    p.player_slow = Some(
        PlayerSlowGenesis::from_sources(sources, player_slow::host::synthetic_host(&p))
            .unwrap()
            .with_fixture_effects(
                [61005, 61004]
                    .into_iter()
                    .map(|spell_id| synthetic_slow_fixture::SyntheticSlowEffect {
                        tick: 1,
                        spell_id,
                        caster: crate::NpcId {
                            spawn: 2,
                            generation: 0,
                        },
                        phase: synthetic_slow_fixture::SyntheticSlowPhase::BeforePlayerUpdate,
                    })
                    .collect(),
            ),
    );
    let mut bytes = b"WDBC".to_vec();
    for word in [1_u32, 4, 16, 1, 1, 1, 3, 0] {
        bytes.extend(word.to_le_bytes());
    }
    bytes.push(0);
    let catalog = LiquidCatalog::from_wdbc(&bytes).unwrap();
    let binding = EnvironmentSnapshotBinding {
        map: p.map_id,
        content: p.content_digest,
        source_epoch: [10; 32],
        inventory: [11; 32],
        policy: [12; 32],
        liquid_catalog: catalog.digest(),
    };
    p.initialize_player_environment(binding, catalog).unwrap();
    p.initialize_player_environment_surface_callbacks().unwrap();
    p
}
fn world(p: Profile) -> NativeWorld {
    NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 1234).unwrap()
}
#[derive(Default)]
struct AuthorityFixture {
    base: synthetic_combat_fixture::SpatialOnly,
    status: Option<LiquidStatus>,
    fail_late: bool,
    queries: usize,
}
impl Authority for AuthorityFixture {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if let Query::PlayerEnvironment(q) = &request.query {
            self.queries += 1;
            let status = self.status.unwrap_or(LiquidStatus::NoWater);
            return Ok(AuthorityReply {
                request: request.clone(),
                facts: Facts::PlayerEnvironment(PlayerEnvironmentFacts::new(
                    q,
                    BoundLiquidResult {
                        query: q.liquid_query(),
                        status,
                        data: (status != LiquidStatus::NoWater).then_some(LiquidData {
                            entry: 1,
                            type_flags: 8,
                            level: 0_f32.to_bits(),
                            depth_level: (-10_f32).to_bits(),
                        }),
                    },
                )?),
            });
        }
        if self.fail_late && self.queries != 0 && matches!(&request.query, Query::SourceMotion(_)) {
            return Err(NativeError::AuthorityUnavailable(
                "authored late wet callback failure",
            ));
        }
        self.base.resolve(request)
    }
}
fn step(
    w: &mut NativeWorld,
    a: &mut AuthorityFixture,
    pose: Option<f32>,
) -> Result<(), NativeError> {
    let input = pose.map(|x| OwnedInput {
        authenticated_owner: w.state().player.owner,
        input: Input {
            world: w.state().world,
            player: w.state().player,
            tick: w.state().tick + 1,
            sequence: w.state().next_sequence,
            command: Command::MovementPose {
                destination: f::point(x),
                orientation: Orientation::new(0.0).unwrap(),
                movement_flags: benilla_sim::creature_motion::GroundMovementFlags::STOPPED,
            },
        },
    });
    w.step_batch(w.state().tick + 1, input.into_iter().collect(), a)
        .map(|_| ())
}
fn ids(w: &NativeWorld) -> Vec<u32> {
    w.state()
        .player_slow
        .as_ref()
        .unwrap()
        .holders()
        .map(|h| h.spell_id)
        .collect()
}
fn reopen(w: &NativeWorld) -> NativeWorld {
    NativeWorld::restore_synthetic_component_test(
        w.profile().clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap()
}
fn removed(w: &NativeWorld) -> Vec<u32> {
    w.last_tick()
        .unwrap()
        .events
        .iter()
        .filter_map(|e| match e {
            Event::PlayerSlow(PlayerSlowEvent::Removed {
                holder,
                reason: PlayerSlowRemoval::Default,
                ..
            }) => Some(holder.spell_id),
            _ => None,
        })
        .collect()
}
#[test]
fn surface_entry_and_exit_remove_actual_holders_and_replay() {
    let mut w = world(profile([0x80, 0x100]));
    let start = w.checkpoint().unwrap();
    let mut a = AuthorityFixture::default();
    step(&mut w, &mut a, None).unwrap();
    assert_eq!(ids(&w), vec![61004, 61005]);
    a.status = Some(LiquidStatus::InWater);
    step(&mut w, &mut a, Some(0.1)).unwrap();
    assert_eq!(removed(&w), vec![61004]);
    assert_eq!(ids(&w), vec![61005]);
    assert_eq!(
        w.state().player_slow.as_ref().unwrap().speeds(),
        benilla_sim::player_slow::SpeedRates::from_amounts([-60]).unwrap()
    );
    let mut resumed = reopen(&w);
    // Countdown is 100ms: another accepted pose reuses the original cached liquid.
    let queried = a.queries;
    a.status = Some(LiquidStatus::NoWater);
    step(&mut w, &mut a, Some(0.2)).unwrap();
    assert_eq!(a.queries, queried);
    assert!(removed(&w).is_empty());
    let mut b = AuthorityFixture::default();
    step(&mut resumed, &mut b, Some(0.2)).unwrap();
    assert_eq!(w.state_digest(), resumed.state_digest());
    for _ in 0..4 {
        step(&mut w, &mut a, None).unwrap();
    }
    step(&mut w, &mut a, Some(0.3)).unwrap();
    assert_eq!(removed(&w), vec![61005]);
    assert!(ids(&w).is_empty());
    assert_eq!(
        w.state().player_slow.as_ref().unwrap().speeds(),
        benilla_sim::player_slow::SpeedRates::normal()
    );
    assert_eq!(reopen(&w).state_digest(), w.state_digest());
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), start).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
}
#[test]
fn later_failure_rolls_back_surface_effects_and_all_owned_state() {
    let mut p = profile([0x80, 0x80]);
    p.source_motion
        .as_mut()
        .unwrap()
        .creature_initial_aggro_delay_ms = 0;
    let mut w = world(p);
    let mut a = AuthorityFixture::default();
    step(&mut w, &mut a, None).unwrap();
    let before = w.state_digest();
    let checkpoint = w.checkpoint().unwrap().encode().unwrap();
    let records = w.records().len();
    a.status = Some(LiquidStatus::InWater);
    a.fail_late = true;
    assert!(step(&mut w, &mut a, Some(0.1)).is_err());
    assert_eq!(w.state_digest(), before);
    assert_eq!(w.checkpoint().unwrap().encode().unwrap(), checkpoint);
    assert_eq!(w.records().len(), records);
    assert_eq!(ids(&w), vec![61004, 61005]);
    a.fail_late = false;
    step(&mut w, &mut a, Some(0.1)).unwrap();
    assert_eq!(removed(&w), vec![61004, 61005]);
    assert!(ids(&w).is_empty());
}
#[test]
fn unsupported_underwater_transition_is_transactional() {
    let mut w = world(profile([0x80, 0x100]));
    let mut a = AuthorityFixture::default();
    step(&mut w, &mut a, None).unwrap();
    let before = w.state_digest();
    a.status = Some(LiquidStatus::UnderWater);
    assert!(step(&mut w, &mut a, Some(0.1)).is_err());
    assert_eq!(w.state_digest(), before);
}
#[test]
fn surface_selection_requires_actual_complete_aura_owner() {
    let mut p = profile([0, 0]);
    p.player_slow = None;
    assert!(p.validate().is_err());
    assert!(p.initialize_player_environment_surface_callbacks().is_err());
}

#[test]
fn surface_threat_callback_visits_genuine_owned_hostile_reference() {
    let mut p = profile([0, 0]);
    p.npcs[0].ai.reaction = ai::Reaction::Aggressive;
    p.source_motion
        .as_mut()
        .unwrap()
        .creature_initial_aggro_delay_ms = 0;
    let mut w = world(p);
    let mut a = AuthorityFixture::default();
    while w.state().npcs[&2].ai.hostile_reference().is_none() {
        assert!(w.state().tick < 20, "fixture never entered actual combat");
        step(&mut w, &mut a, None).unwrap();
    }
    let reference = w.state().npcs[&2].ai.hostile_reference().unwrap().clone();
    assert!(reference.valid() && reference.online());
    let current = w.state().npcs[&2].ai.current_victim();
    a.status = Some(LiquidStatus::InWater);
    step(&mut w, &mut a, Some(0.1)).unwrap();
    assert_eq!(w.state().npcs[&2].ai.hostile_reference(), Some(&reference));
    assert_eq!(w.state().npcs[&2].ai.current_victim(), current);
    assert_eq!(reopen(&w).state_digest(), w.state_digest());
}

#[test]
fn version27_checkpoint_is_rejected_without_invented_owners() {
    let w = world(profile([0, 0]));
    let mut value: serde_json::Value =
        serde_json::from_slice(&w.checkpoint().unwrap().encode().unwrap()).unwrap();
    value["version"] = 27.into();
    assert!(Checkpoint::decode(&serde_json::to_vec(&value).unwrap()).is_err());
}
