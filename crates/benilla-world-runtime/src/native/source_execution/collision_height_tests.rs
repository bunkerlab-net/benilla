// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_sim::creature_capabilities::MOVEFLAG_SWIMMING;

fn models(height: f32) -> SourceCollisionModels {
    SourceCollisionModels {
        displays: BTreeMap::from([(1192, 19)]),
        heights: BTreeMap::from([(19, height.to_bits())]),
    }
}

#[test]
fn native_display_and_object_scale_are_the_only_height_operands() {
    let mut catalog = models(2.5);
    catalog.displays.insert(1200, 20);
    catalog.heights.insert(20, 7f32.to_bits());
    assert_eq!(
        catalog.collision_height(1192, 1.5).unwrap().to_bits(),
        0x40700000
    );
    assert_eq!(
        catalog.collision_height(1200, 1.5).unwrap().to_bits(),
        0x41280000
    );
    // Unit.cpp:11957 tests the product, not either operand and not its sign.
    for (scale, raw, expected) in [
        (0.0, 2.5, 2.03128f32),
        (-0.0, 2.5, 2.03128),
        (1.5, 0.0, 2.03128),
        (1.5, -0.0, 2.03128),
        (-1.5, 2.5, -3.75),
        (f32::from_bits(1), 0.25, 2.03128),
    ] {
        assert_eq!(
            models(raw).collision_height(1192, scale).unwrap().to_bits(),
            expected.to_bits()
        );
    }
}

#[test]
fn nonfinite_height_is_ieee_not_an_early_numeric_refusal() {
    assert_eq!(
        models(f32::INFINITY).collision_height(1192, 2.0).unwrap(),
        f32::INFINITY
    );
    assert_eq!(
        models(2.0)
            .collision_height(1192, f32::NEG_INFINITY)
            .unwrap(),
        f32::NEG_INFINITY
    );
    assert!(models(f32::INFINITY)
        .collision_height(1192, 0.0)
        .unwrap()
        .is_nan());
    for bits in [0x7fc12345, 0x7f812345, 0xffc12345] {
        assert_eq!(
            models(f32::from_bits(bits))
                .collision_height(1192, 1.0)
                .unwrap()
                .to_bits(),
            bits | 0x00400000
        );
    }
}

#[test]
fn both_source_asserts_refuse_at_lookup_before_terrain_or_flag_write() {
    for (models, display, id, expected_counts) in [
        (
            models(2.5),
            1200,
            "execution.collision_height.Unit_11952.display_info_assert",
            (1, 0),
        ),
        (
            SourceCollisionModels {
                displays: BTreeMap::from([(1192, 19)]),
                heights: BTreeMap::new(),
            },
            1192,
            "execution.collision_height.Unit_11954.model_data_assert",
            (0, 1),
        ),
    ] {
        let mut movement = SourceCreatureMovement::construct();
        let mut census = SourceCreatureSwimmingCensus::default();
        let error = movement
            .init_entry_flags_from_bits([2, 0], false, false, Ok(None), || {
                models.init_entry_swimmable(
                    display,
                    1.5,
                    |_| panic!("terrain preceded assertion"),
                    &mut census,
                )
            })
            .unwrap_err();
        assert_eq!(error.id, id);
        assert_eq!(movement.flags.unwrap(), 0x100);
        assert_eq!(census.reached_441, 1);
        assert_eq!(
            (census.display_assert_11952, census.model_assert_11954),
            expected_counts
        );
        assert_eq!((census.swimming_set, census.swimming_not_set), (0, 0));
    }
    // A real row with ModelId zero is not an assertion when the model store has ID zero.
    let models = SourceCollisionModels {
        displays: BTreeMap::from([(1192, 0)]),
        heights: BTreeMap::from([(0, 2.5f32.to_bits())]),
    };
    assert_eq!(models.collision_height(1192, 1.5).unwrap(), 3.75);
}

#[test]
fn liquid_owner_receives_respawn_pose_and_scaled_height_before_swimming_write() {
    let Some(mut geometry) = super::super::pathfinder::tests::geometry(21, 1) else {
        return;
    };
    let models = models(2.5);
    let mut census = SourceCreatureSwimmingCensus::default();
    for (scale, expected) in [(1.5, true), (4.0, false)] {
        let mut movement = SourceCreatureMovement::construct();
        let mut queries = 0;
        movement
            .init_entry_flags_from_bits([2, 0], false, false, Ok(None), || {
                models.init_entry_swimmable(
                    1192,
                    scale,
                    |height| {
                        queries += 1;
                        geometry.is_swimmable(0, [0., 0., 4.], height, None)
                    },
                    &mut census,
                )
            })
            .unwrap();
        assert_eq!(queries, 1);
        assert_eq!(
            movement.flags.unwrap(),
            0x100 | if expected { MOVEFLAG_SWIMMING } else { 0 }
        );
    }
    assert_eq!(
        census,
        SourceCreatureSwimmingCensus {
            reached_441: 2,
            swimming_set: 1,
            swimming_not_set: 1,
            ..Default::default()
        }
    );
}

#[test]
fn terrain_refusal_preserves_flags_after_successful_height_lookups() {
    let mut movement = SourceCreatureMovement::construct();
    let mut census = SourceCreatureSwimmingCensus::default();
    let error = movement
        .init_entry_flags_from_bits([2, 0], false, false, Ok(None), || {
            models(2.5).init_entry_swimmable(
                1192,
                1.5,
                |height| {
                    assert_eq!(height, 3.75);
                    Err(refuse("authored.terrain.next_owner"))
                },
                &mut census,
            )
        })
        .unwrap_err();
    assert_eq!(error.id, "authored.terrain.next_owner");
    assert_eq!(movement.flags.unwrap(), 0x100);
    assert_eq!(
        census,
        SourceCreatureSwimmingCensus {
            reached_441: 1,
            ..Default::default()
        }
    );
}

#[test]
fn catalog_model_height_lookup_census_is_not_constructed_creature_execution() {
    let Some(root) = std::env::var_os("BENILLA_F2_CATALOG") else {
        eprintln!("SKIP collision model census: BENILLA_F2_CATALOG absent");
        return;
    };
    let mut catalog = VerifiedCatalog::open(Path::new(&root), false).unwrap();
    let models = SourceCollisionModels::load(&mut catalog).unwrap();
    let mut missing_models = 0;
    for &display in models.displays.keys() {
        if let Err(error) = models.collision_height(display, 1.0) {
            assert_eq!(
                error.id,
                "execution.collision_height.Unit_11954.model_data_assert"
            );
            missing_models += 1;
        }
    }
    println!("F6_COLLISION_MODEL_LOOKUP displays={} models={} missing_models={missing_models} scope=all_dbc_displays_not_constructed_execution", models.displays.len(), models.heights.len());
}
