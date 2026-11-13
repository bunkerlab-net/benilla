// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_native_vmap::live::FileState;
use benilla_sim::source_rng::{DrawCursor, Mt19937};

fn map(flags: u32, sample: u32) -> Vec<u8> {
    let width = match flags {
        0 => 4,
        2 | 6 => 2,
        4 => 1,
        _ => 0,
    };
    let length = 16 + (129 * 129 + 128 * 128) * width;
    let mut bytes = vec![0; 40];
    bytes[..8].copy_from_slice(b"MAPSz1.4");
    bytes[16..20].copy_from_slice(&40u32.to_le_bytes());
    // Source ignores the declared height size, even when it is zero.
    bytes.extend(b"MHGT");
    bytes.extend(flags.to_le_bytes());
    bytes.extend((-8.0f32).to_le_bytes());
    bytes.extend(
        (if flags == 2 || flags == 6 {
            65527.0f32
        } else {
            247.0
        })
        .to_le_bytes(),
    );
    for _ in 0..129 * 129 + 128 * 128 {
        match width {
            1 => bytes.push(sample as u8),
            2 => bytes.extend((sample as u16).to_le_bytes()),
            4 => bytes.extend(sample.to_le_bytes()),
            _ => (),
        }
    }
    assert_eq!(bytes.len(), 40 + length);
    bytes
}

#[test]
fn cold_grid_uses_real_float_flat_and_packed_storage_instead_of_header_base() {
    for (flags, sample, expected) in [
        (0, 12.0f32.to_bits(), 12.0),
        (1, 0, -8.0),
        (2, 32768, 32760.0),
        (4, 128, 120.0),
        (6, 65535, 65527.0),
    ] {
        let grid = SourceGridMap::load(Some(map(flags, sample).into())).unwrap();
        assert!(grid.load_success);
        assert_eq!(grid.height(0.0, 0.0).unwrap(), expected);
        assert_eq!(grid.area(0.0, 0.0).unwrap(), 0);
    }
    assert_eq!(
        SourceGridMap::load(None).unwrap().height(0.0, 0.0).unwrap(),
        -200000.0
    );
}

#[test]
fn exact_refusals_name_header_flags_and_each_reached_uninitialized_sample_kind() {
    for (flags, id) in [
        (0, "execution.terrain.height_float_sample_read_undefined"),
        (2, "execution.terrain.height_uint16_sample_read_undefined"),
        (4, "execution.terrain.height_uint8_sample_read_undefined"),
    ] {
        let full = map(flags, 0);
        for (n, coordinate) in [
            (56, 0.0),
            (full.len() - 1, -127.5 * (grid_height::GRID_SIZE / 128.0)),
        ] {
            let partial = SourceGridMap::load(Some(Arc::from(&full[..n]))).unwrap();
            assert!(partial.load_success);
            assert_eq!(partial.height(coordinate, coordinate).unwrap_err().id, id);
        }
        assert!(SourceGridMap::load(Some(full.into())).is_ok());
    }
    assert_eq!(
        SourceGridMap::load(Some(map(8, 0).into())).unwrap_err().id,
        "execution.terrain.height_invalid_header_flags"
    );
    let mut bad = map(1, 0);
    bad[40] = b'X';
    let grid = SourceGridMap::load(Some(bad.into())).unwrap();
    assert!(!grid.load_success);
    assert_eq!(grid.height(0.0, 0.0).unwrap(), -200000.0);
    let grid = SourceGridMap::load(Some(map(0, 0).into())).unwrap();
    assert_eq!(
        grid.height(f32::NAN, 0.0).unwrap_err().id,
        "execution.terrain.height_coordinate_conversion_undefined"
    );
}

#[test]
fn height_static_runs_full_grid_upgrade_before_disabled_vmap_branch_without_weakening_mmap_refusal()
{
    let mut files = SourceTerrainFiles::default();
    files.bytes(0, 32, 32, map(4, 128));
    let mut manager = SourceTerrainManager::new(files, SourceTerrainDbc::default());
    let mut mt = Mt19937::from_seed(5489);
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    let mut random = SourceRandom::new(1, &mut mt, &mut cursor, &mut sink).unwrap();
    manager.load_terrain(0, &mut random).unwrap();
    manager.query_grid(0, [0.0; 3], true).unwrap();
    let tile = manager.terrain(0).unwrap().residency.tile(32, 32).unwrap();
    assert_eq!(
        manager
            .terrain(0)
            .unwrap()
            .grid(32, 32)
            .unwrap()
            .height(0.0, 0.0)
            .unwrap(),
        120.0
    );
    assert_eq!(
        manager
            .height_static(0, [0.0; 3], false, 10.0)
            .unwrap_err()
            .id,
        "execution.vmap.product_unavailable"
    );
    manager
        .vmap
        .collision
        .files
        .observe(b"000.vmtree", FileState::Absent)
        .unwrap();
    assert_eq!(
        manager
            .height_static(0, [0.0; 3], false, 10.0)
            .unwrap_err()
            .id,
        "execution.terrain.mmap_tile_load_owner"
    );
    assert_eq!(manager.vmap.map_slots(), 1);
    assert!(!manager.vmap.has_tree(0));
    assert_eq!(
        manager.terrain(0).unwrap().residency.tile(32, 32).unwrap(),
        tile
    );
    assert_eq!(random.cursor().raw, 1);
}
