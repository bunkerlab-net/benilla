// SPDX-License-Identifier: GPL-3.0-or-later
use super::super::super::vmap::installed_source_fixture as fixture;
use super::*;
use std::time::Duration;

fn empty_wmos() -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for n in [0u32, 20, 80, 1] {
        bytes.extend(n.to_le_bytes());
    }
    bytes.push(0);
    bytes
}

fn location_entries(mut entries: Vec<(String, Vec<u8>)>) -> Vec<(String, Vec<u8>)> {
    let mut areas = b"WDBC".to_vec();
    for n in [2u32, 25, 100, 1] {
        areas.extend(n.to_le_bytes());
    }
    for (id, parent, flag) in [(9u32, 12u32, 17u32), (12, 0, 18)] {
        let mut row = [0u32; 25];
        row[0] = id;
        row[2] = parent;
        row[3] = flag;
        for n in row {
            areas.extend(n.to_le_bytes());
        }
    }
    areas.push(0);
    entries
        .iter_mut()
        .find(|(name, _)| name == "DBFilesClient\\AreaTable.dbc")
        .unwrap()
        .1 = areas;
    let adt = &mut entries
        .iter_mut()
        .find(|(name, _)| name.ends_with("_40_40.adt"))
        .unwrap()
        .1;
    for slot in 0..256 {
        let at =
            u32::from_le_bytes(adt[92 + slot * 16..96 + slot * 16].try_into().unwrap()) as usize;
        adt[at + 8 + 0x34..at + 8 + 0x38].copy_from_slice(&9u32.to_le_bytes());
    }
    entries.push(("DBFilesClient\\WMOAreaTable.dbc".into(), empty_wmos()));
    entries
}
#[test]
fn genuine_captures_feed_existing_location_reader_and_refuse_uncaptured_tile() {
    let entries = location_entries(fixture::entries());
    let install = fixture::Install::new(&entries);
    let selected = [benilla_native_terrain::Tile { x: 40, y: 40 }];
    let capture = benilla_native_terrain::location::prepare(install.path(), 0, &selected).unwrap();
    let grid_flag = capture.grid_area_flag(-4276., -4276.).unwrap();
    let source = benilla_native_vmap_source::prepare(install.path(), 0, || false)
        .unwrap()
        .into_map_bundle(fixture::profile(), || false)
        .unwrap()
        .unwrap();
    let snapshot = TerrainAreaSnapshot::from_source(capture, &source, [9; 32]).unwrap();
    let query = TerrainAreaRequest {
        snapshot: snapshot.binding(),
        ticket: 1,
        position: [-4276.0f32, -4276., fixture::HEIGHT].map(f32::to_bits),
    };
    let reply = snapshot
        .query_location(query, Instant::now() + Duration::from_secs(10))
        .unwrap();
    assert_eq!(reply.result.area_flag, grid_flag);
    assert_eq!((reply.result.zone_id, reply.result.area_id), (12, 9));
    assert_eq!(snapshot.location_binding().content, [9; 32]);
    let outside = TerrainAreaRequest {
        position: [0.0f32, 0., 0.].map(f32::to_bits),
        ..query
    };
    assert_eq!(
        snapshot.query_location(outside, Instant::now() + Duration::from_secs(10)),
        Err(Error::Missing)
    );
}

#[test]
fn resident_location_refuses_reached_unloaded_exterior_placement() {
    let entries = location_entries(fixture::overlapping_entries());
    let install = fixture::Install::new(&entries);
    let capture = benilla_native_terrain::location::prepare(
        install.path(),
        0,
        &[benilla_native_terrain::Tile { x: 40, y: 40 }],
    )
    .unwrap();
    let discovery = benilla_native_vmap_source::prepare(install.path(), 0, || false).unwrap();
    let bounds = discovery
        .assembled_spawn_bounds(fixture::profile(), || false)
        .unwrap();
    let bound = bounds
        .bounds()
        .iter()
        .find(|bound| bound.flags & 1 == 0)
        .unwrap();
    // A point at the genuine unloaded bound's center must reach that tree slot.
    let internal: [f32; 3] =
        std::array::from_fn(|i| (bound.bounds[0][i] + bound.bounds[1][i]) * 0.5);
    let mid = 0.5_f32 * 64. * 533.333_3_f32;
    let position = [mid - internal[0], mid - internal[1], internal[2]];
    assert_eq!(
        capture.grid_area_flag(position[0], position[1]).unwrap(),
        17
    );
    let source = discovery
        .into_map_bundle(fixture::profile(), || false)
        .unwrap()
        .unwrap();
    let snapshot = TerrainAreaSnapshot::from_source(capture, &source, [9; 32]).unwrap();
    let query = TerrainAreaRequest {
        snapshot: snapshot.binding(),
        ticket: 1,
        position: position.map(f32::to_bits),
    };
    assert_eq!(
        snapshot.query_location(query, Instant::now() + Duration::from_secs(10)),
        Err(Error::Missing)
    );
}
