// SPDX-License-Identifier: GPL-3.0-or-later
// Complete pinned C++ GridMap/AREA methods, independently generated expected bits.
// Consumer inputs cross the real MAPS and complete WDBC byte decoders only.
use super::super::super::TileId;
use super::super::AreaTable;
use super::{GridLiquidData, GridLiquidStatus, LiquidStores, LiquidTypes, TerrainTile};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const FIXTURE: &str = include_str!(
    "../../../../../../../../../benilla-native-geometry-tests/tools/fixtures/area-liquid.tsv"
);
const AREA_CYCLE: [u16; 24] = [
    100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 112, 114, 115, 0, 300, 65535, 200, 201,
    202, 500, 9000, 107, 109,
];
const ENTRY_CYCLE: [u16; 8] = [0, 1, 2, 3, 20, 21, 22, 77];
const FLAG_CYCLE: [u8; 5] = [0, 1, 16, 32, 63];
const STRINGS: &[u8] = b"\0a\0b\0c\0d\0e\0f\0g\0h\0";
const DATA_SENTINEL: GridLiquidData = GridLiquidData {
    entry: 0xdead_beef,
    type_flags: 0xdeca_fbad,
    level: 0x4a12_3456,
    depth_level: 0xca65_4321,
};

fn word(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend(value.to_le_bytes());
}

fn complete_wdbc(rows: &[Vec<u32>], fields: usize) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for value in [rows.len(), fields, fields * 4, STRINGS.len()] {
        word(&mut bytes, u32::try_from(value).unwrap());
    }
    for row in rows {
        assert_eq!(row.len(), fields);
        for &value in row {
            word(&mut bytes, value);
        }
    }
    bytes.extend(STRINGS);
    bytes
}

fn authored_stores() -> BTreeMap<u32, (LiquidTypes, AreaTable)> {
    assert!(
        FIXTURE.contains("# LiquidType format: niii; AreaTable format: niiiixxxxxissssssssxixxxi")
    );
    assert!(FIXTURE.contains("# DBC strings are hex 0061006200630064006500660067006800;"));
    let mut liquid_rows: BTreeMap<u32, Vec<Vec<u32>>> = BTreeMap::new();
    let mut area_rows: BTreeMap<u32, Vec<Vec<u32>>> = BTreeMap::new();
    for line in FIXTURE.lines() {
        let rows = if line.starts_with("# liquid_row\t") {
            &mut liquid_rows
        } else if line.starts_with("# area_row\t") {
            &mut area_rows
        } else {
            continue;
        };
        let numbers: Vec<u32> = line
            .split('\t')
            .skip(1)
            .map(|number| number.parse().unwrap())
            .collect();
        rows.entry(numbers[0])
            .or_default()
            .push(numbers[1..].to_vec());
    }
    assert_eq!(liquid_rows.len(), 4);
    assert_eq!(area_rows.len(), 4);
    let mut stores = BTreeMap::new();
    for variant in 0..4 {
        assert_eq!(liquid_rows[&variant].len(), 9 + (variant & 1) as usize);
        assert_eq!(
            area_rows[&variant].len(),
            20 + ((variant >> 1) & 1) as usize
        );
        let liquid_bytes = complete_wdbc(&liquid_rows[&variant], 4);
        let area_bytes = complete_wdbc(&area_rows[&variant], 25);
        let liquid = LiquidTypes::decode(&liquid_bytes).unwrap();
        let areas = AreaTable::decode(&area_bytes).unwrap();
        // Hash the entire real WDBC inputs, including unused fields/localized names.
        let liquid_digest: [u8; 32] = Sha256::digest(&liquid_bytes).into();
        let area_digest: [u8; 32] = Sha256::digest(&area_bytes).into();
        assert_eq!(liquid.digest(), liquid_digest);
        assert_eq!(areas.digest(), area_digest);
        assert!(stores.insert(variant, (liquid, areas)).is_none());
    }
    stores
}

struct Profile {
    store: u32,
    area: Vec<u8>,
    liquid: Vec<u8>,
}

fn authored_profiles() -> BTreeMap<u32, Profile> {
    assert!(FIXTURE.contains(
        "# AREA cells[i]=[100,101,102,103,104,105,106,107,108,109,110,112,114,115,0,300,65535,200,201,202,500,9000,107,109][i%24]"
    ));
    assert!(FIXTURE.contains(
        "# MLIQ entries[i]=[0,1,2,3,20,21,22,77][(i/24)%8]; flags[i]=[0,1,16,32,63][i%5]"
    ));
    assert!(FIXTURE.contains("MLIQ is flat level +0, offX=offY=0, width=height=128."));
    let mut profiles = BTreeMap::new();
    for line in FIXTURE
        .lines()
        .filter(|line| line.starts_with("# profile\t"))
    {
        let values: Vec<_> = line.split('\t').collect();
        if values[1] == "x_bits" {
            continue;
        }
        assert_eq!(values.len(), 8, "{line}");
        let number = |i: usize| values[i].parse::<u32>().unwrap();
        let area_table = number(6) != 0;
        let type_table = number(7) != 0;
        let mut area = b"AREA".to_vec();
        area.extend(u16::from(!area_table).to_le_bytes());
        area.extend(u16::try_from(number(3)).unwrap().to_le_bytes());
        if area_table {
            for i in 0..256 {
                area.extend(AREA_CYCLE[i % AREA_CYCLE.len()].to_le_bytes());
            }
        }
        let mut liquid = b"MLIQ".to_vec();
        liquid.push(2 | u8::from(!type_table));
        liquid.push(u8::try_from(number(5)).unwrap());
        liquid.extend(u16::try_from(number(4)).unwrap().to_le_bytes());
        liquid.extend([0, 0, 128, 128]);
        word(&mut liquid, 0);
        if type_table {
            for i in 0..256 {
                liquid.extend(ENTRY_CYCLE[(i / 24) % ENTRY_CYCLE.len()].to_le_bytes());
            }
            for i in 0..256 {
                liquid.push(FLAG_CYCLE[i % FLAG_CYCLE.len()]);
            }
        }
        assert!(profiles
            .insert(
                number(1),
                Profile {
                    store: number(2),
                    area,
                    liquid
                }
            )
            .is_none());
    }
    assert_eq!(profiles.len(), 35);
    profiles
}

fn authored_map(profile: &Profile, ground: u32) -> Vec<u8> {
    let mut bytes = b"MAPSz1.4".to_vec();
    let area_len = u32::try_from(profile.area.len()).unwrap();
    let liquid_len = u32::try_from(profile.liquid.len()).unwrap();
    let height_offset = 40 + area_len;
    let liquid_offset = height_offset + 16;
    for value in [
        40,
        area_len,
        height_offset,
        16,
        liquid_offset,
        liquid_len,
        liquid_offset + liquid_len,
        512,
    ] {
        word(&mut bytes, value);
    }
    bytes.extend(&profile.area);
    bytes.extend(b"MHGT");
    word(&mut bytes, 1);
    word(&mut bytes, ground);
    word(&mut bytes, ground);
    bytes.extend(&profile.liquid);
    bytes.extend([0; 512]);
    bytes
}

#[test]
fn complete_pinned_area_liquid_status_matches_real_maps_and_complete_wdbc() {
    let stores = authored_stores();
    let profiles = authored_profiles();
    let mut tiles = BTreeMap::new();
    let mut seen_profiles = BTreeSet::new();
    let mut seen_statuses = BTreeSet::new();
    let mut seen_masks = BTreeSet::new();
    let mut seen_entries = BTreeSet::new();
    let mut seen_tiles = BTreeSet::new();
    let mut seen_raw_areas = BTreeSet::new();
    let mut global_witnesses = BTreeMap::new();
    let mut count = 0;
    for line in FIXTURE.lines().filter(|line| !line.starts_with('#')) {
        let values: Vec<_> = line.split('\t').collect();
        assert_eq!(values.len(), 16, "{line}");
        let number = |i: usize| values[i].parse::<u32>().unwrap();
        let bits = |i: usize| u32::from_str_radix(values[i], 16).unwrap();
        let float = |i: usize| f32::from_bits(bits(i));
        let profile_id = number(0);
        let profile = &profiles[&profile_id];
        let ground = bits(6);
        let tile_x = u8::try_from(number(14)).unwrap();
        let tile_y = u8::try_from(number(15)).unwrap();
        let tile = tiles
            .entry((profile_id, ground, tile_x, tile_y))
            .or_insert_with(|| {
                TerrainTile::decode(
                    TileId {
                        x: tile_x,
                        y: tile_y,
                    },
                    &authored_map(profile, ground),
                )
                .unwrap()
            });
        assert_eq!(
            u32::from(tile.areas.get(float(1), float(2))),
            number(13),
            "raw AREA: {line}"
        );
        let (types, areas) = &stores[&profile.store];
        let actual = tile
            .grid_liquid_status_with_stores(
                float(1),
                float(2),
                float(3),
                u8::try_from(number(5)).unwrap(),
                float(4),
                LiquidStores {
                    types,
                    areas: Some(areas),
                },
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
                "source dry output changed: {line}"
            );
            assert_eq!(actual.data, None, "{line}");
        } else if number(7) == 0 {
            // Rust always returns data; null-source cases prove only status and
            // sentinel retention. Nonnull calls prove every returned data bit.
            assert_eq!(
                source_data, DATA_SENTINEL,
                "source null output changed: {line}"
            );
            assert!(actual.data.is_some(), "{line}");
        } else {
            assert_eq!(actual.data, Some(source_data), "{line}");
            seen_entries.insert(source_data.entry);
        }
        if profile_id != 31
            && profile_id != 32
            && number(5) == 0
            && float(3) == -1.0
            && float(4) == 2.0
            && float(6) == -100.0
            && number(7) == 1
        {
            global_witnesses.insert(profile_id, (number(8), number(9), number(10)));
        }
        seen_profiles.insert(profile_id);
        seen_statuses.insert(number(8));
        seen_masks.insert(number(5));
        seen_tiles.insert((tile_x, tile_y));
        seen_raw_areas.insert(number(13));
        count += 1;
    }
    assert_eq!(count, 1931);
    assert_eq!(seen_profiles, (0..35).collect());
    assert_eq!(seen_statuses, [0, 1, 2, 4, 8].into_iter().collect());
    assert_eq!(
        seen_masks,
        [0, 1, 2, 4, 8, 15, 16, 32, 63].into_iter().collect()
    );
    assert_eq!(seen_tiles, [(32, 32), (31, 33)].into_iter().collect());
    assert_eq!(
        seen_raw_areas,
        AREA_CYCLE.into_iter().map(u32::from).collect()
    );
    assert_eq!(
        seen_entries,
        [0, 1, 2, 3, 20, 21, 22, 23, 24, 77, 65535]
            .into_iter()
            .collect()
    );
    // Keep semantic witnesses explicit so changing fixture profiles cannot quietly
    // lose direct-vs-parent, zero-vs-missing, raw-ID, or one-hop coverage.
    for (profile, entry, flags) in [
        (0, 22, 20),
        (1, 23, 24),
        (2, 1, 17),
        (3, 1, 17),
        (4, 1, 17),
        (5, 1, 17),
        (6, 1, 17),
        (7, 1, 17),
        (8, 1, 17),
        (9, 1, 17),
        (10, 20, 24),
        (11, 65535, 18),
        (12, 24, 17),
        (13, 1, 17),
        (14, 1, 17),
        (15, 1, 17),
        (16, 1, 17),
        (17, 0, 24),
        (18, 0, 24),
        (19, 0, 24),
        (20, 0, 24),
        (21, 0, 24),
        (22, 24, 17),
        (23, 24, 17),
        (24, 21, 18),
        (25, 0, 63),
        (26, 22, 20),
        (28, 22, 20),
        (29, 22, 4),
        (30, 1, 17),
        (33, 77, 63),
        (34, 1, 17),
    ] {
        assert_eq!(
            global_witnesses[&profile],
            (4, entry, flags),
            "profile {profile}"
        );
    }
    assert_eq!(
        global_witnesses[&27],
        (0, DATA_SENTINEL.entry, DATA_SENTINEL.type_flags)
    );
}
