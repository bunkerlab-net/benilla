// SPDX-License-Identifier: GPL-3.0-or-later
//! Same loaded source owner across scalar height, environment and source1024 navigation.
//! These are dependency-reader tests; no body, Route or production admission is claimed.
use super::super::{
    dry_route::tests as dry, same_polygon_fixture as fixture, tests as product_tests, vmap,
    ProductId,
};
use super::*;
use benilla_world_runtime::native::{
    synthetic_combat_fixture as combat, synthetic_fixture, synthetic_motion_fixture as motion,
    EffectiveDisabledMMapSet, SelectedMMapConfiguration,
};
use std::time::Duration;

fn until() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

fn query(snapshot: &MapHeightSnapshot<'_>) -> MapHeightRequest {
    MapHeightRequest {
        snapshot: snapshot.binding(),
        ticket: 1,
        position: fixture::FROM.map(f32::to_bits),
        swim: false,
    }
}

#[test]
fn same_arc_and_source_identity_survive_owned_dry_height_projection() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let products = fixture::products();
    let config = fixture::height_configuration(&products);
    let proof = NonRenderParticipants::authored_fixture(config.content, config.map);
    let weak = Arc::downgrade(&products);
    let snapshot = Arc::clone(&products)
        .load_dry_map_height_owned(config, Some(&proof))
        .unwrap();
    assert_eq!(
        Arc::strong_count(&products),
        2,
        "height retains exactly the original Arc"
    );
    assert_eq!(snapshot.configuration(), config);
    let environment = dry::environment(Arc::clone(&products));
    let raw = fixture::RawMap::new(fixture::TerrainKind::Flat);
    let mut profile = synthetic_fixture::initialized_profile();
    profile.map_id = config.map;
    profile.content_digest = config.content;
    profile.npcs[0].ai.map_id = config.map;
    profile.npcs[0].ai.content_revision =
        benilla_sim::creature_ai::movement::Revision(config.content);
    profile.npcs[0].ai.reaction = benilla_sim::creature_ai::Reaction::Aggressive;
    profile.npcs[0].faction_template = 2;
    profile.source_motion = Some(motion::genesis(&profile, 2));
    // Actual selected pathfinder configuration requires complete interaction flags,
    // even though this reader test does not dispatch motion or supply any Facts.
    combat::install(&mut profile);
    profile
        .initialize_creature_capabilities(motion::creature_capability_sources(&profile))
        .unwrap();
    profile
        .initialize_pathfinder_configuration(
            SelectedMMapConfiguration {
                enabled: true,
                effective_disabled_maps: EffectiveDisabledMMapSet::Complete(Default::default()),
                source_instance_id: 0,
            },
            &raw.maps(),
        )
        .unwrap();
    let source = products.load_source_query_projection(&profile).unwrap();
    assert_eq!(source.navigation().query().query_capacity(), 1024);
    let height = snapshot.binding();
    let navigation = source.navigation().binding();
    let environment_binding = environment.binding();
    for actual in [
        (
            navigation.map,
            navigation.content,
            navigation.source_epoch,
            navigation.inventory,
        ),
        (
            environment_binding.map,
            environment_binding.content,
            environment_binding.source_epoch,
            environment_binding.inventory,
        ),
    ] {
        assert_eq!(
            actual,
            (
                height.map,
                height.content,
                height.source_epoch,
                height.inventory
            )
        );
    }
    assert_eq!(
        snapshot.static_vmap().binding(),
        source.static_vmap().binding()
    );
    let request = query(&snapshot);
    assert_eq!(
        snapshot.query(request, until()).unwrap(),
        MapHeightReply {
            request,
            height: 0f32.to_bits()
        }
    );
    assert_eq!(
        snapshot.query(request, Instant::now()),
        Err(Error::Deadline)
    );
    drop(environment);
    drop(products);
    assert_eq!(
        weak.strong_count(),
        1,
        "the exact original owner stays alive through height"
    );
    assert_eq!(
        snapshot.query(request, until()).unwrap().height,
        0f32.to_bits()
    );
    drop(snapshot);
    assert!(weak.upgrade().is_none());
}

#[test]
fn dry_owned_height_preserves_effective_config_and_participant_rejections() {
    let products = fixture::products();
    let configuration = fixture::height_configuration(&products);
    let proof = NonRenderParticipants::authored_fixture(configuration.content, configuration.map);
    for field in 0..5 {
        let mut config = configuration;
        match field {
            0 => config.map += 1,
            1 => config.content[0] ^= 1,
            2 => config.source_epoch[0] ^= 1,
            3 => config.effective_config[0] ^= 1,
            _ => config.vmap_height_enabled = false,
        }
        assert_eq!(
            Arc::clone(&products)
                .load_dry_map_height_owned(config, Some(&proof))
                .map(|_| ()),
            Err(if field == 4 {
                Error::Unsupported
            } else {
                Error::Identity
            })
        );
    }
    let wrong_map =
        NonRenderParticipants::authored_fixture(configuration.content, configuration.map + 1);
    let wrong_content = NonRenderParticipants::authored_fixture([99; 32], configuration.map);
    for participant in [None, Some(&wrong_map), Some(&wrong_content)] {
        assert_eq!(
            Arc::clone(&products)
                .load_dry_map_height_owned(configuration, participant)
                .map(|_| ()),
            Err(Error::Profile)
        );
    }
    assert_eq!(
        Arc::strong_count(&products),
        1,
        "rejected loads retain no source owner"
    );
}

#[test]
fn replacement_epoch_or_raw_source_inventory_cannot_answer_old_height_query() {
    let raw = fixture::RawMap::new(fixture::TerrainKind::Flat);
    let (spec, files) = fixture::fixture(&raw);
    let products = dry::load(spec.clone(), &files);
    let config = fixture::height_configuration(&products);
    let proof = NonRenderParticipants::authored_fixture(config.content, config.map);
    let original = Arc::clone(&products)
        .load_dry_map_height_owned(config, Some(&proof))
        .unwrap();
    let request = query(&original);
    for field in 0..2 {
        let mut changed = spec.clone();
        if field == 0 {
            changed.binding.source_epoch[0] ^= 1;
        } else {
            changed.binding.raw_static_inventory[0] ^= 1;
        }
        let replacement = dry::load(changed, &files);
        let config = fixture::height_configuration(&replacement);
        let snapshot = replacement
            .load_dry_map_height_owned(config, Some(&proof))
            .unwrap();
        assert_eq!(snapshot.query(request, until()), Err(Error::Identity));
        assert_eq!(
            original.query(request, until()).unwrap().height,
            0f32.to_bits()
        );
    }
}

#[test]
fn opt_in_wet_gridmap_or_vmap_cannot_project_to_dry_owned_map_height() {
    let raw = fixture::RawMap::new(fixture::TerrainKind::Flat);
    let (mut spec, mut files) = fixture::fixture(&raw);
    product_tests::mutate(
        &mut spec,
        &mut files,
        &ProductId::Terrain(fixture::TILE),
        |bytes| {
            // Existing authored GridMap serialization: flat global water, no type/height arrays.
            let at = u32::from_le_bytes(bytes[32..36].try_into().unwrap()) as usize;
            let mut liquid = b"MLIQ".to_vec();
            liquid.extend([3, 1]);
            liquid.extend(1u16.to_le_bytes());
            liquid.extend([0, 0, 129, 129]);
            liquid.extend(5f32.to_le_bytes());
            bytes.splice(at..at, liquid);
            bytes[24..28].copy_from_slice(&(at as u32).to_le_bytes());
            bytes[28..32].copy_from_slice(&16u32.to_le_bytes());
            bytes[32..36].copy_from_slice(&((at + 16) as u32).to_le_bytes());
        },
    );
    let grid = dry::load(spec, &files);
    let (spec, files) = vmap::liquid_fixture();
    let vmap = dry::load(spec, &files);
    for products in [grid, vmap] {
        let config = fixture::height_configuration(&products);
        let proof = NonRenderParticipants::authored_fixture(config.content, config.map);
        assert!(
            products.load_vmap_liquid().is_ok(),
            "valid opt-in inputs, not malformed fixtures"
        );
        assert_eq!(
            products
                .load_dry_map_height_owned(config, Some(&proof))
                .map(|_| ()),
            Err(Error::Unsupported)
        );
    }
}
