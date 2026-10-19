// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual consumed source points and real InitEntry; catalogs remain authored fixtures.
use super::super::super::super::{
    core::Failure,
    source_creature_initialization::{
        resident::{
            ResidentCreatureInitializationProvider as Provider, ResidentInitializationError,
        },
        tests as initial,
    },
};
use super::super::super::source_height::{
    CompositionError, PreparedSourceHeight, ResidentInitializationConfiguration as Configuration,
};
use super::source_height_tests::shared_entries;
use super::*;
use benilla_native_terrain::{self as terrain_source, PreparedTerrain};
use benilla_sim::creature_ai as ai;
use benilla_world_runtime::native::{
    synthetic_fixture as f, synthetic_motion_fixture as motion, *,
};
use std::time::Duration;

const WORLD: [u8; 16] = [8; 16];
const CONTENT: [u8; 32] = [61; 32];
const CONFIG: Option<Configuration> = Some(Configuration::VmapHeightEnabled);
const SELECTED: terrain_source::Tile = terrain_source::Tile { x: 31, y: 30 };
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
fn terrain(install: &Install) -> PreparedTerrain {
    terrain_source::prepare(&install.0, 0, &[SELECTED]).unwrap()
}
fn runtime_profile(binding: CreatureInitializationSnapshot) -> Profile {
    let position = ai::Point::new(
        f32::from_bits(0x43e8_d540),
        f32::from_bits(0x4471_aaa0),
        8.0,
    )
    .unwrap();
    runtime_profile_at(binding, position)
}
fn runtime_profile_at(binding: CreatureInitializationSnapshot, position: ai::Point) -> Profile {
    let mut p = initial::profile(binding, [77; 32]);
    p.npcs[0].ai.spawn = position;
    p.player.start = ai::Point::new(
        position.x.as_f32() + 6.0,
        position.y.as_f32(),
        position.z.as_f32(),
    )
    .unwrap();
    let mut source = p
        .creature_capabilities
        .as_ref()
        .unwrap()
        .source(2)
        .unwrap()
        .clone();
    source.initialization = CreatureInitializationClosure::DryWater {
        map: binding.map,
        respawn_position: position,
        snapshot: binding,
    };
    p.creature_capabilities = Some(
        CreatureCapabilityCatalog::validate_sources(CreatureCapabilityRows::Complete(vec![source]))
            .unwrap(),
    );
    p.validate().unwrap();
    p
}
struct ResidentAuthority<'a, 'terrain, 'bundle, 'source> {
    provider: Provider<'a, 'terrain, 'bundle, 'source>,
    initializations: Vec<AuthorityRequest>,
    motion: motion::SyntheticMotionAuthority,
}
impl Authority for ResidentAuthority<'_, '_, '_, '_> {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if matches!(request.query, Query::CreatureInitialization(_)) {
            self.initializations.push(request.clone());
            let facts = self
                .provider
                .resolve(request, deadline(), || false)
                .map_err(|_| NativeError::AuthorityUnavailable("resident source initialization"))?;
            Ok(AuthorityReply {
                request: request.clone(),
                facts,
            })
        } else {
            self.motion.resolve(request)
        }
    }
}
fn authority<'a, 't, 'b, 's>(
    p: &Profile,
    height: &'a PreparedSourceHeight<'t, 'b, 's>,
) -> ResidentAuthority<'a, 't, 'b, 's> {
    ResidentAuthority {
        provider: Provider::bind(WORLD, p, height, CONFIG, deadline(), || false).unwrap(),
        initializations: vec![],
        motion: Default::default(),
    }
}
fn initialize(p: Profile, authority: &mut impl Authority) -> Result<NativeWorld, NativeError> {
    NativeWorld::synthetic_component_test_with_initialization(
        p,
        WORLD,
        f::player(),
        7,
        None,
        authority,
    )
}
fn assert_order(world: &NativeWorld, request: &AuthorityRequest, draws: &[Draw]) {
    let Query::CreatureInitialization(q) = &request.query else {
        unreachable!()
    };
    q.validate_for(world.profile()).unwrap();
    assert_eq!(q.required_type, 15);
    assert_eq!(q.classification_height.bits(), 0x4002_007e);
    assert_ne!(q.classification_height, q.collision_height);
    let prefix: Vec<_> = draws.iter().filter(|d| matches!(d.purpose,
        DrawPurpose::CreatureModel { npc } | DrawPurpose::CreatureGender { npc } if Some(npc) == q.binding.npc)).cloned().collect();
    assert_eq!(q.model_gender_draws, prefix);
    assert_eq!(prefix.len(), 2);
    assert_eq!(q.first_counter, prefix[0].first_counter);
    let level = draws.iter().find(|d| matches!(d.purpose, DrawPurpose::CreatureLevel { npc } if Some(npc) == q.binding.npc)).unwrap();
    assert_eq!(q.before_level_counter, level.first_counter);
    let npc = &world.state().npcs[&2];
    let witness = npc.creature_capabilities().unwrap().witness();
    let facts = witness.initialization().unwrap();
    assert_eq!(facts.request, **q);
    assert_eq!(facts.status, environment::LiquidStatus::NoWater);
    assert!(facts.data.is_none());
    assert_eq!(witness.inhabit_type(), 3);
    assert!(witness.capabilities().unwrap().can_swim());
    assert_eq!(witness.swimming_movement_flags(), 0);
    assert_eq!(npc.initialized_creature().unwrap().unit_flags & 0x8000, 0);
    let height = npc.creature_collision_height().unwrap().witness();
    assert_eq!(height.height().bits(), q.collision_height.bits());
}
fn initial_draws(world: &NativeWorld) -> Vec<Draw> {
    let value = serde_json::to_value(world.checkpoint().unwrap()).unwrap();
    serde_json::from_value(
        value["state"]["npcs"]["2"]["source_generation"]["generations"][0]["draws"].clone(),
    )
    .unwrap()
}
#[test]
fn resident_initialization_consumes_real_point_before_level_and_keeps_owned_height() {
    let install = Install::new(&shared_entries());
    let before = install.bytes();
    let terrain = terrain(&install);
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let height = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    let binding = height
        .initialization_binding(CONTENT, CONFIG, deadline(), || false)
        .unwrap();
    let p = runtime_profile(binding);
    assert!(NativeWorld::synthetic_component_test(p.clone(), WORLD, f::player(), 7).is_err());
    let mut responder = authority(&p, &height);
    let world = initialize(p, &mut responder).unwrap();
    assert_eq!(responder.initializations.len(), 1);
    assert_order(
        &world,
        &responder.initializations[0],
        &initial_draws(&world),
    );
    let restored = NativeWorld::restore_synthetic_component_test(
        world.profile().clone(),
        world.checkpoint().unwrap(),
    )
    .unwrap();
    assert_eq!(restored.state_digest(), world.state_digest());
    assert_eq!(install.bytes(), before);
}

#[cfg(test)]
#[path = "resident_initialization_lifecycle_tests.rs"]
mod lifecycle;

#[test]
fn resident_initialization_rejects_edited_transcripts_and_unselected_configuration() {
    let install = Install::new(&shared_entries());
    let terrain = terrain(&install);
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let height = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    let binding = height
        .initialization_binding(CONTENT, CONFIG, deadline(), || false)
        .unwrap();
    let p = runtime_profile(binding);
    for config in [None, Some(Configuration::VmapHeightDisabled)] {
        assert!(matches!(
            height.initialization_binding(CONTENT, config, deadline(), || false),
            Err(CompositionError::Validation(Error::Unsupported))
        ));
        assert!(matches!(
            Provider::bind(WORLD, &p, &height, config, deadline(), || false),
            Err(ResidentInitializationError::Source(
                CompositionError::Validation(Error::Unsupported)
            ))
        ));
    }
    assert!(height
        .initialization_binding([0; 32], CONFIG, deadline(), || false)
        .is_err());
    assert!(height
        .initialization_binding(CONTENT, CONFIG, deadline(), || true)
        .is_err());
    assert!(height
        .initialization_binding(CONTENT, CONFIG, Instant::now(), || false)
        .is_err());
    for case in 0..6 {
        let mut wrong = binding;
        match case {
            0 => wrong.source_epoch[0] ^= 1,
            1 => wrong.inventory[0] ^= 1,
            2 => wrong.policy[0] ^= 1,
            3 => wrong.effective_config[0] ^= 1,
            4 => wrong.vmap_height_enabled = false,
            _ => wrong.map = 1,
        }
        let wrong_profile = runtime_profile(wrong);
        assert!(matches!(
            Provider::bind(WORLD, &wrong_profile, &height, CONFIG, deadline(), || false),
            Err(ResidentInitializationError::Context(Failure::Binding))
        ));
    }
    let mut responder = authority(&p, &height);
    initialize(p, &mut responder).unwrap();
    let original = &responder.initializations[0];
    for case in 0..23 {
        let mut bad = original.clone();
        let Query::CreatureInitialization(q) = &mut bad.query else {
            unreachable!()
        };
        match case {
            0 => q.snapshot.source_epoch[0] ^= 1,
            1 => q.snapshot.inventory[0] ^= 1,
            2 => q.snapshot.policy[0] ^= 1,
            3 => q.snapshot.effective_config[0] ^= 1,
            4 => q.snapshot.vmap_height_enabled = false,
            5 => q.capability_source[0] ^= 1,
            6 => q.generation_source[0] ^= 1,
            7 => q.model_source[0] ^= 1,
            8 => q.position.x = f::real(0.0),
            9 => q.native_display_id += 1,
            10 => q.model_id += 1,
            11 => q.object_scale = f::float(1.0),
            12 => q.raw_height = f::float(123.0),
            13 => q.collision_height = f::float(123.0),
            14 => q.classification_height = q.collision_height,
            15 => q.ticket += 1,
            16 => q.required_type = 10,
            17 => q.now_ms += 1,
            18 => q.model_gender_draws[0].consumed += 1,
            19 => q.first_counter += 1,
            20 => q.before_level_counter += 1,
            21 => bad.binding.geometry[0] ^= 1,
            _ => {
                bad.binding.world[0] ^= 1;
                q.binding.world = bad.binding.world;
            }
        }
        let observed = Cell::new(false);
        assert!(
            matches!(
                responder.provider.resolve_observed(
                    &bad,
                    deadline(),
                    || false,
                    |_| observed.set(true)
                ),
                Err(ResidentInitializationError::Context(Failure::Binding))
            ),
            "edited case {case}"
        );
        assert!(
            !observed.get(),
            "edited case {case} must reject before point observation"
        );
    }
}

#[test]
fn resident_initialization_equal_byte_new_owner_requeries_historical_request() {
    let install = Install::new(&shared_entries());
    let terrain = terrain(&install);
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let height = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    let binding = height
        .initialization_binding(CONTENT, CONFIG, deadline(), || false)
        .unwrap();
    let p = runtime_profile(binding);
    let mut responder = authority(&p, &height);
    let world = initialize(p.clone(), &mut responder).unwrap();
    let request = &responder.initializations[0];
    let first = responder
        .provider
        .resolve(request, deadline(), || false)
        .unwrap();
    let other = Install::new(&shared_entries());
    let other_terrain = terrain_source::prepare(&other.0, 0, &[SELECTED]).unwrap();
    let other_capture = source::prepare(&other.0, 0, || false).unwrap();
    let other_bundle = other_capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let other_height =
        PreparedSourceHeight::from_source(&other_terrain, &other_bundle, || false).unwrap();
    let copied: CreatureInitializationSnapshot =
        serde_json::from_slice(&serde_json::to_vec(&binding).unwrap()).unwrap();
    assert_eq!(
        copied,
        other_height
            .initialization_binding(CONTENT, CONFIG, deadline(), || false)
            .unwrap()
    );
    let provider = Provider::bind(WORLD, &p, &other_height, CONFIG, deadline(), || false).unwrap();
    let mut phases = vec![];
    assert_eq!(
        provider
            .resolve_observed(request, deadline(), || false, |phase| phases.push(phase))
            .unwrap(),
        first
    );
    assert_eq!(phases, [0, 1]);
    // No retained opaque observation is reusable here: every answer queried its
    // own borrowed owner. Deserialization supplies no point evidence or liveness.
    assert_eq!(
        world.state().npcs[&2]
            .creature_capabilities()
            .unwrap()
            .witness()
            .initialization()
            .unwrap()
            .request
            .snapshot,
        copied
    );
    other.write(&super::resident_liquid_tests::liquid_group_entries());
    assert!(provider.resolve(request, deadline(), || false).is_err());
    assert!(responder
        .provider
        .resolve(request, deadline(), || false)
        .is_ok());
    let changed_terrain = terrain_source::prepare(&other.0, 0, &[SELECTED]).unwrap();
    let changed_capture = source::prepare(&other.0, 0, || false).unwrap();
    let changed_bundle = changed_capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let changed_height =
        PreparedSourceHeight::from_source(&changed_terrain, &changed_bundle, || false).unwrap();
    let changed_binding = changed_height
        .initialization_binding(CONTENT, CONFIG, deadline(), || false)
        .unwrap();
    assert_ne!(changed_binding.source_epoch, copied.source_epoch);
    assert_ne!(changed_binding.inventory, copied.inventory);
    assert!(matches!(
        Provider::bind(WORLD, &p, &changed_height, CONFIG, deadline(), || false),
        Err(ResidentInitializationError::Context(Failure::Binding))
    ));
}

#[test]
fn resident_initialization_wet_or_missing_source_never_mints_facts() {
    for case in 0..3 {
        let mut entries = shared_entries();
        if case == 0 {
            *entries.iter_mut().find(|entry| entry.name == ADT).unwrap() = raw_entry(
                ADT,
                super::resident_liquid_tests::placed_terrain("liquid_water_flat.adt.test-fixture"),
            );
        } else if case == 1 {
            entries = super::resident_liquid_tests::liquid_group_entries();
        }
        let install = Install::new(&entries);
        let terrain = terrain(&install);
        let capture = source::prepare(&install.0, 0, || false).unwrap();
        let bundle = capture
            .assemble_map_bundle(profile(), || false)
            .unwrap()
            .unwrap();
        let height = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
        // Metadata construction is deliberately possible for wet data: it makes
        // no dry assertion. The actual selected point must still be observed.
        let binding = height
            .initialization_binding(CONTENT, CONFIG, deadline(), || false)
            .unwrap();
        let p = if case == 2 {
            runtime_profile_at(binding, ai::Point::new(0.0, 0.0, 8.0).unwrap())
        } else {
            runtime_profile(binding)
        };
        let mut responder = authority(&p, &height);
        assert!(
            initialize(p, &mut responder).is_err(),
            "wet/missing case {case}"
        );
        assert_eq!(responder.initializations.len(), 1);
        assert!(
            matches!(
                responder
                    .provider
                    .resolve(&responder.initializations[0], deadline(), || false),
                Err(ResidentInitializationError::Source(
                    CompositionError::Validation(Error::Unsupported | Error::Missing)
                ))
            ),
            "wet/missing case {case}"
        );
    }
}

#[test]
fn resident_initialization_no_answer_escapes_late_cancel_deadline_or_source_mutation() {
    for case in 0..5 {
        let install = Install::new(&shared_entries());
        let terrain = terrain(&install);
        let capture = source::prepare(&install.0, 0, || false).unwrap();
        let bundle = capture
            .assemble_map_bundle(profile(), || false)
            .unwrap()
            .unwrap();
        let height = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
        let binding = height
            .initialization_binding(CONTENT, CONFIG, deadline(), || false)
            .unwrap();
        let p = runtime_profile(binding);
        let mut responder = authority(&p, &height);
        initialize(p, &mut responder).unwrap();
        let request = &responder.initializations[0];
        assert!(matches!(
            responder
                .provider
                .resolve(request, Instant::now(), || false),
            Err(ResidentInitializationError::Context(Failure::Deadline))
        ));
        assert!(matches!(
            responder.provider.resolve(request, deadline(), || true),
            Err(ResidentInitializationError::Source(
                CompositionError::Cancelled
            ))
        ));
        let limit = Instant::now() + Duration::from_secs(1);
        let cancelled = Cell::new(false);
        let mut observed = false;
        let result = responder.provider.resolve_observed(
            request,
            limit,
            || cancelled.get(),
            |phase| {
                if phase != u8::from(case != 4) {
                    return;
                }
                observed = true;
                match case {
                    0 => cancelled.set(true),
                    1 => std::thread::sleep(
                        limit.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
                    ),
                    2 | 4 => install.write(&super::resident_liquid_tests::liquid_group_entries()),
                    _ => {
                        let mut entries = shared_entries();
                        *entries.iter_mut().find(|entry| entry.name == ADT).unwrap() = raw_entry(
                            ADT,
                            super::resident_liquid_tests::placed_terrain(
                                "liquid_water_flat.adt.test-fixture",
                            ),
                        );
                        install.write(&entries);
                    }
                }
            },
        );
        assert!(observed, "reached observation boundary for {case}");
        match case {
            0 => assert!(matches!(
                result,
                Err(ResidentInitializationError::Source(
                    CompositionError::Cancelled
                ))
            )),
            1 => assert!(matches!(
                result,
                Err(ResidentInitializationError::Context(Failure::Deadline))
            )),
            _ => assert!(matches!(
                result,
                Err(ResidentInitializationError::Source(_))
            )),
        }
    }
}
