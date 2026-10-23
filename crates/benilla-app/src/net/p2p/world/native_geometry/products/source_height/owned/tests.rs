// SPDX-License-Identifier: GPL-3.0-or-later
use super::super::super::vmap::installed_source_fixture as input;
use super::super::{Binding, HeightRequest};
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
    OwnedSourceProducts::new(terrain, collision)
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
    let owned = move_static(OwnedSourceProducts::new(
        terrain,
        discovery
            .into_map_bundle(input::profile(), || false)
            .unwrap()
            .unwrap(),
    ));
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
}
