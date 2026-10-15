// SPDX-License-Identifier: GPL-3.0-or-later
// Complete pinned C++ status method over authored MLIQ/DBC inputs and flat ground.
// This proof does not cover terrain interpolation, area overrides or TerrainInfo.
use super::super::super::TileId;
use super::{GridLiquidData, GridLiquidStatus, LiquidTypes, TerrainTile};
use std::collections::{BTreeMap, BTreeSet};

const FIXTURE: &str = include_str!(
    "../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/wet-liquid.tsv"
);
const ENTRY_CYCLE: [u16; 7] = [21, 22, 23, 24, 77, 0, 20];
const FLAG_CYCLE: [u8; 9] = [0, 1, 2, 4, 8, 16, 24, 32, 63];
const DATA_SENTINEL: GridLiquidData = GridLiquidData {
    entry: 0xdead_beef,
    type_flags: 0xdeca_fbad,
    level: 0x4a12_3456,
    depth_level: 0xca65_4321,
};

fn word(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend(value.to_le_bytes());
}

fn complete_authored_dbc() -> Vec<u8> {
    assert!(FIXTURE.contains(
        "# DBC rows (Id,LiquidId,Type,SpellId): 21,23,3,0;22,29,1,0;23,35,0,0;24,41,2,0"
    ));
    let mut bytes = b"WDBC".to_vec();
    for value in [4, 4, 16, 1] {
        word(&mut bytes, value);
    }
    for row in [
        [21, 23, 3, 0],
        [22, 29, 1, 0],
        [23, 35, 0, 0],
        [24, 41, 2, 0],
    ] {
        for value in row {
            word(&mut bytes, value);
        }
    }
    bytes.push(0);
    bytes
}

fn authored_liquid_sections() -> BTreeMap<usize, Vec<u8>> {
    assert!(FIXTURE.contains(
        "# Cell entries[i] = [21,22,23,24,77,0,20][i%7]; flags[i] = [0,1,2,4,8,16,24,32,63][i%9]"
    ));
    assert!(FIXTURE.contains(
        "# Height-map levels[row*width+col] = binary32(10 + binary32(row*0.5) + binary32(col*0.25))."
    ));
    let mut sections = BTreeMap::new();
    for line in FIXTURE
        .lines()
        .filter(|line| line.starts_with("# profile\t"))
    {
        let values: Vec<_> = line.split('\t').collect();
        // The column heading begins with the same prefix as the metadata records.
        if values[1] == "x_bits" {
            continue;
        }
        assert_eq!(values.len(), 12, "{line}");
        let number = |i: usize| values[i].parse::<u32>().unwrap();
        let id = number(1) as usize;
        let has_types = number(9) != 0;
        let has_heights = number(10) != 0;
        let zero_flags = number(11) != 0;
        let mut bytes = b"MLIQ".to_vec();
        bytes.push(u8::from(!has_types) | (u8::from(!has_heights) << 1));
        bytes.push(u8::try_from(number(3)).unwrap());
        bytes.extend(u16::try_from(number(2)).unwrap().to_le_bytes());
        for i in 4..=7 {
            bytes.push(u8::try_from(number(i)).unwrap());
        }
        word(&mut bytes, u32::from_str_radix(values[8], 16).unwrap());
        if has_types {
            for i in 0..256 {
                bytes.extend(ENTRY_CYCLE[i % ENTRY_CYCLE.len()].to_le_bytes());
            }
            for i in 0..256 {
                bytes.push(if zero_flags {
                    0
                } else {
                    FLAG_CYCLE[i % FLAG_CYCLE.len()]
                });
            }
        }
        if has_heights {
            for row in 0..number(7) {
                for column in 0..number(6) {
                    let level = 10.0_f32 + row as f32 * 0.5 + column as f32 * 0.25;
                    word(&mut bytes, level.to_bits());
                }
            }
        }
        assert!(sections.insert(id, bytes).is_none());
    }
    assert_eq!(sections.len(), 10);
    sections
}

fn authored_map(liquid: &[u8], ground_bits: u32) -> Vec<u8> {
    let mut bytes = b"MAPSz1.4".to_vec();
    let liquid_length = u32::try_from(liquid.len()).unwrap();
    // Consecutive MAPS header, AREA, flat MHGT, MLIQ and 16*16 uint16 holes.
    for value in [40, 8, 48, 16, 64, liquid_length, 64 + liquid_length, 512] {
        word(&mut bytes, value);
    }
    bytes.extend(b"AREA\x01\x00\x00\x00MHGT");
    word(&mut bytes, 1);
    word(&mut bytes, ground_bits);
    word(&mut bytes, ground_bits);
    bytes.extend(liquid);
    bytes.extend([0; 512]);
    bytes
}

#[test]
fn complete_pinned_liquid_status_matches_decoded_authored_maps_and_dbc() {
    let sections = authored_liquid_sections();
    let types = LiquidTypes::decode(&complete_authored_dbc()).unwrap();
    let mut tiles = BTreeMap::new();
    let mut seen_profiles = BTreeSet::new();
    let mut seen_statuses = BTreeSet::new();
    let mut seen_masks = BTreeSet::new();
    let mut count = 0;
    for line in FIXTURE.lines().filter(|line| !line.starts_with('#')) {
        let values: Vec<_> = line.split('\t').collect();
        assert_eq!(values.len(), 13, "{line}");
        let number = |i: usize| values[i].parse::<u32>().unwrap();
        let bits = |i: usize| u32::from_str_radix(values[i], 16).unwrap();
        let float = |i: usize| f32::from_bits(bits(i));
        let profile = number(0) as usize;
        let ground = bits(6);
        let tile = tiles.entry((profile, ground)).or_insert_with(|| {
            TerrainTile::decode(
                TileId { x: 32, y: 32 },
                &authored_map(&sections[&profile], ground),
            )
            .unwrap()
        });
        let actual = tile
            .grid_liquid_status(
                float(1),
                float(2),
                float(3),
                u8::try_from(number(5)).unwrap(),
                float(4),
                &types,
            )
            .unwrap_or_else(|error| panic!("{line}: {error:?}"));
        let expected_status = match number(8) {
            0 => GridLiquidStatus::NoWater,
            1 => GridLiquidStatus::AboveWater,
            2 => GridLiquidStatus::WaterWalk,
            4 => GridLiquidStatus::InWater,
            8 => GridLiquidStatus::UnderWater,
            other => panic!("unknown source status {other}"),
        };
        assert_eq!(actual.status, expected_status, "{line}");
        let source_data = GridLiquidData {
            entry: number(9),
            type_flags: number(10),
            level: bits(11),
            depth_level: bits(12),
        };
        if expected_status == GridLiquidStatus::NoWater {
            assert_eq!(
                source_data, DATA_SENTINEL,
                "source no-water wrote data: {line}"
            );
            assert_eq!(actual.data, None, "{line}");
        } else if number(7) == 0 {
            // The Rust API always returns data. Null-output source calls prove only
            // the status and the C++ sentinel; source nonnull cases prove the data.
            assert_eq!(
                source_data, DATA_SENTINEL,
                "source null pointer wrote data: {line}"
            );
            assert!(actual.data.is_some(), "{line}");
        } else {
            assert_eq!(actual.data, Some(source_data), "{line}");
        }
        seen_profiles.insert(profile);
        seen_statuses.insert(number(8));
        seen_masks.insert(number(5));
        count += 1;
    }
    assert_eq!(count, 2953);
    assert_eq!(seen_profiles, (0..10).collect());
    assert_eq!(seen_statuses, [0, 1, 2, 4, 8].into_iter().collect());
    assert_eq!(
        seen_masks,
        [0, 1, 2, 4, 8, 15, 16, 32, 63].into_iter().collect()
    );
}
