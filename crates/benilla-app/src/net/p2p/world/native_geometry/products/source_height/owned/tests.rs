// SPDX-License-Identifier: GPL-3.0-or-later
use super::super::super::vmap::{installed_source_fixture as input, RayReply, StaticRay};
use super::super::{
    preparation_counts::{measure, Counts},
    Binding, HeightRequest, PointNoWaterReply, PointNoWaterRequest,
};
use super::*;
use std::{
    cell::Cell,
    time::{Duration, Instant},
};

fn capture(install: &input::Install) -> OwnedSourceProducts {
    let terrain =
        benilla_native_terrain::prepare(install.path(), 0, &input::guard_source_tiles()).unwrap();
    let discovery = benilla_native_vmap_source::prepare(install.path(), 0, || false).unwrap();
    let collision = discovery
        .into_map_bundle(input::profile(), || false)
        .unwrap()
        .unwrap();
    OwnedSourceProducts::new(terrain, collision, || false).unwrap()
}
fn sample(
    height: &PreparedSourceHeight<'_, '_, '_>,
) -> Result<(Binding, [u32; 2]), CompositionError> {
    let mut values = [0; 2];
    for (i, point) in [input::support().0, input::support().1]
        .into_iter()
        .enumerate()
    {
        values[i] = height
            .query(
                HeightRequest {
                    binding: height.binding(),
                    ticket: i as u64 + 1,
                    position: point.map(f32::to_bits),
                    max_search: 10_f32.to_bits(),
                },
                Instant::now() + Duration::from_secs(30),
            )?
            .height;
    }
    Ok((height.binding(), values))
}

#[test]
fn owned_sources_move_outlive_borrowed_views_and_preserve_exact_height() {
    fn move_static<T: 'static>(value: T) -> T {
        value
    }
    let install = input::install();
    let before = install.bytes();
    let terrain =
        benilla_native_terrain::prepare(install.path(), 0, &input::guard_source_tiles()).unwrap();
    let discovery = benilla_native_vmap_source::prepare(install.path(), 0, || false).unwrap();
    let expected = {
        let borrowed = discovery
            .assemble_map_bundle(input::profile(), || false)
            .unwrap()
            .unwrap();
        let height = PreparedSourceHeight::from_source(&terrain, &borrowed, || false).unwrap();
        sample(&height).unwrap()
    };
    let owned = move_static(
        OwnedSourceProducts::new(
            terrain,
            discovery
                .into_map_bundle(input::profile(), || false)
                .unwrap()
                .unwrap(),
            || false,
        )
        .unwrap(),
    );
    assert_eq!(owned.with_height(|| false, sample).unwrap(), expected);
    let moved = move_static(owned);
    assert_eq!(moved.with_height(|| false, sample).unwrap(), expected);
    assert_eq!(expected.1, [input::HEIGHT.to_bits(); 2]);
    assert_eq!(install.bytes(), before);
    drop(moved);
    assert_eq!(install.bytes(), before);
}

#[test]
fn owned_sources_refuse_changed_or_missing_sources_before_reader() {
    for remove in [false, true] {
        let install = input::install();
        let owned = capture(&install);
        if remove {
            drop(install);
        } else {
            install.write(&input::overlapping_entries());
        }
        let called = Cell::new(false);
        assert!(owned
            .with_height(
                || false,
                |_| {
                    called.set(true);
                    Ok(())
                }
            )
            .is_err());
        assert!(!called.get());
    }
}

#[test]
fn owned_sources_recheck_after_reader_and_preserve_cancellation() {
    let install = input::install();
    let owned = capture(&install);
    let ((), preparations) = measure(|| {
        let called = Cell::new(false);
        assert!(matches!(
            owned.with_height(
                || true,
                |_| {
                    called.set(true);
                    Ok(())
                }
            ),
            Err(CompositionError::Cancelled)
        ));
        assert!(!called.get());
        let cancelled = Cell::new(false);
        assert!(matches!(
            owned.with_height(
                || cancelled.get(),
                |_| {
                    cancelled.set(true);
                    Ok(())
                }
            ),
            Err(CompositionError::Cancelled)
        ));
        cancelled.set(false);
        assert_eq!(
            owned.with_height(|| cancelled.get(), sample).unwrap().1,
            [input::HEIGHT.to_bits(); 2]
        );
        assert!(owned
            .with_height(
                || false,
                |height| {
                    let result = sample(height)?;
                    install.write(&input::overlapping_entries());
                    Ok(result)
                }
            )
            .is_err());
    });
    assert_eq!(preparations, Counts::default());
}

#[derive(Debug, PartialEq, Eq)]
struct Observations {
    height: (Binding, [u32; 2]),
    liquid: [PointNoWaterReply; 2],
    ray: RayReply,
}

fn observations(
    height: &PreparedSourceHeight<'_, '_, '_>,
) -> Result<Observations, CompositionError> {
    let (from, to) = input::support();
    let liquid = |point: [f32; 3], ticket, mask| {
        height.point_no_water(
            PointNoWaterRequest {
                binding: height.binding(),
                ticket,
                position: point.map(f32::to_bits),
                mask,
            },
            Instant::now() + Duration::from_secs(30),
            || false,
        )
    };
    let ray_endpoint = |point: [f32; 3]| [point[0], point[1], point[2] + 2.0].map(f32::to_bits);
    Ok(Observations {
        height: sample(height)?,
        liquid: [liquid(from, 11, 15)?, liquid(to, 12, 10)?],
        ray: height.chase_ray(
            13,
            ray_endpoint(from),
            ray_endpoint(to),
            Instant::now() + Duration::from_secs(30),
            || false,
        )?,
    })
}

#[test]
fn owned_sources_prepare_once_and_reuse_exact_height_liquid_and_ray_after_moves() {
    fn move_static<T: 'static>(value: T) -> T {
        value
    }
    let install = input::install();
    let before = install.bytes();
    let terrain =
        benilla_native_terrain::prepare(install.path(), 0, &input::guard_source_tiles()).unwrap();
    let discovery = benilla_native_vmap_source::prepare(install.path(), 0, || false).unwrap();
    let expected = {
        let borrowed = discovery
            .assemble_map_bundle(input::profile(), || false)
            .unwrap()
            .unwrap();
        let height = PreparedSourceHeight::from_source(&terrain, &borrowed, || false).unwrap();
        observations(&height).unwrap()
    };
    let collision = discovery
        .into_map_bundle(input::profile(), || false)
        .unwrap()
        .unwrap();
    let (owned, construction) =
        measure(|| move_static(OwnedSourceProducts::new(terrain, collision, || false).unwrap()));
    assert_eq!(
        construction,
        Counts {
            height_preparations: 1,
            terrain_tiles: 9,
            collision_preparations: 1,
        }
    );
    assert_eq!(expected.height.1, [input::HEIGHT.to_bits(); 2]);
    for liquid in expected.liquid {
        assert_eq!(liquid.initial_static_height(), input::HEIGHT.to_bits());
        assert_eq!(liquid.effective_floor(), input::HEIGHT.to_bits());
        assert!(liquid.location().is_none());
    }
    assert_eq!(expected.ray.result, StaticRay::Miss);
    let ((), scoped_reads) = measure(|| {
        let mut owned = owned;
        for _ in 0..3 {
            assert_eq!(owned.with_height(|| false, observations).unwrap(), expected);
            owned = move_static(owned);
        }
        drop(owned);
    });
    assert_eq!(scoped_reads, Counts::default());
    assert_eq!(install.bytes(), before);
}

#[test]
fn owned_sources_construction_refuses_cancellation_changed_and_deleted_sources() {
    for mode in 0..3 {
        let install = input::install();
        let terrain =
            benilla_native_terrain::prepare(install.path(), 0, &input::guard_source_tiles())
                .unwrap();
        let collision = benilla_native_vmap_source::prepare(install.path(), 0, || false)
            .unwrap()
            .into_map_bundle(input::profile(), || false)
            .unwrap()
            .unwrap();
        match mode {
            0 => (),
            1 => install.write(&input::overlapping_entries()),
            2 => drop(install),
            _ => unreachable!(),
        }
        let result = OwnedSourceProducts::new(terrain, collision, || mode == 0);
        if mode == 0 {
            assert!(matches!(result, Err(CompositionError::Cancelled)));
        } else {
            assert!(result.is_err());
        }
    }
}

#[test]
fn owned_sources_preserve_typed_terrain_cancellation_and_genuine_source_failure() {
    let install = input::install();
    let owned = capture(&install);
    drop(install);
    let ((), preparations) = measure(|| {
        // Exercise the actual terrain adapter, not only the outer source-height poll.
        // Already-cancelled verification must stop even with the directory removed.
        assert!(matches!(
            super::super::verify_terrain_source(&owned.terrain, &|| true),
            Err(CompositionError::Cancelled)
        ));
        assert!(matches!(
            super::super::verify_terrain_source(&owned.terrain, &|| false),
            Err(CompositionError::TerrainSource)
        ));
        assert!(matches!(
            owned.with_height::<()>(|| true, |_| panic!("cancelled precheck invoked reader")),
            Err(CompositionError::Cancelled)
        ));
        assert!(matches!(
            owned.with_height::<()>(|| false, |_| panic!("missing source invoked reader")),
            Err(CompositionError::TerrainSource)
        ));
    });
    assert_eq!(preparations, Counts::default());
}
