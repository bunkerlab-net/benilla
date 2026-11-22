// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn grids() -> Vec<AreaGrid> {
    let mut flags = vec![65535; 256];
    flags[2 * 16 + 3] = 17;
    vec![AreaGrid {
        tile: Tile { x: 48, y: 32 },
        area_ids: vec![0; 256],
        area_flags: flags,
        header_flags: 0,
        grid_area: 0,
    }]
}
#[test]
fn known_cell_uses_x_major_gridmap_index() {
    // Centers of cell x=2,y=3 in tile 48,32, independently calculated.
    assert_eq!(grid_area_flag(&grids(), -8616.667, -116.66667).unwrap(), 17);
    assert_eq!(
        grid_area_flag(&grids(), -8650.0, -116.66667).unwrap(),
        65535
    );
}
#[test]
fn outside_captured_tiles_refuses_instead_of_wrapping() {
    for (x, y) in [(0.0, 0.0), (-8616.667, 116.66667), (f32::NAN, 0.0)] {
        assert!(grid_area_flag(&grids(), x, y).is_err());
    }
}
#[test]
fn area_parent_chain_preserves_names_and_rejects_cycles() {
    // Original authored WDBC, not installation rows.
    let mut bytes = b"WDBC".to_vec();
    let strings = b"\0Region\0District\0Cell\0";
    for n in [3u32, 25, 100, strings.len() as u32] {
        bytes.extend(n.to_le_bytes());
    }
    for (id, parent, name) in [(7u32, 0u32, 1u32), (8, 7, 8), (9, 8, 17)] {
        let mut row = [0u32; 25];
        row[0] = id;
        row[2] = parent;
        row[3] = id + 10;
        row[11] = name;
        for n in row {
            bytes.extend(n.to_le_bytes());
        }
    }
    bytes.extend(strings);
    let areas = read_areas(&bytes).unwrap();
    let chain = parent_chain(&areas, 9).unwrap();
    assert_eq!(
        chain
            .iter()
            .map(|r| (r.id, r.name.as_str()))
            .collect::<Vec<_>>(),
        vec![(9, "Cell"), (8, "District"), (7, "Region")]
    );
    let mut cyclic = areas.clone();
    cyclic.get_mut(&7).unwrap().parent = 9;
    assert!(parent_chain(&cyclic, 9).is_err());
    assert!(parent_chain(&areas, 99).is_err());
}
