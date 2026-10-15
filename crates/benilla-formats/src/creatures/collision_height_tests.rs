//! Authored numeric WDBC rows loaded through the production MPQ chain and catalog reader.
//! No game installation, models, textures or captured records are used.

use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

// Reuse the permissive MPQ crate's test-only encoder rather than another crypto algorithm.
#[path = "../../../benilla-mpq/src/crypto.rs"]
mod crypto;

const MODEL_STRINGS: &[u8] = b"\0Authored\\HeightFixture.mdx\0";

struct Temp(PathBuf);

impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "benilla-authored-model-height-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, members: &[(&str, Vec<u8>)]) {
        std::fs::write(self.0.join(name), archive(members)).unwrap();
    }

    fn load(&self) -> Result<CreatureCatalog> {
        load_creature_catalog(&mut Chain::open(&self.0)?)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn archive(members: &[(&str, Vec<u8>)]) -> Vec<u8> {
    use crypto::{encrypt_block, hash_string, hash_type};
    const SLOTS: u32 = 8;
    assert!(members.len() < SLOTS as usize);
    let hash_pos = 32;
    let block_pos = hash_pos + SLOTS * 16;
    let mut data_pos = block_pos + members.len() as u32 * 16;
    let mut hashes = vec![u32::MAX; SLOTS as usize * 4];
    let mut blocks = Vec::new();
    for (index, (name, data)) in members.iter().enumerate() {
        let mut slot = (hash_string(name, hash_type::TABLE_OFFSET) & (SLOTS - 1)) as usize;
        while hashes[slot * 4 + 3] != u32::MAX {
            slot = (slot + 1) & (SLOTS as usize - 1);
        }
        hashes[slot * 4..slot * 4 + 4].copy_from_slice(&[
            hash_string(name, hash_type::NAME_A),
            hash_string(name, hash_type::NAME_B),
            0,
            index as u32,
        ]);
        blocks.extend([data_pos, data.len() as u32, data.len() as u32, 0x8000_0000]);
        data_pos += data.len() as u32;
    }
    encrypt_block(
        &mut hashes,
        hash_string("(hash table)", hash_type::FILE_KEY),
    );
    encrypt_block(
        &mut blocks,
        hash_string("(block table)", hash_type::FILE_KEY),
    );
    let mut bytes = vec![0; 32];
    for (offset, value) in [
        (0, 0x1a51_504d_u32),
        (4, 32),
        (8, data_pos),
        (16, hash_pos),
        (20, block_pos),
        (24, SLOTS),
        (28, members.len() as u32),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    for word in hashes.into_iter().chain(blocks) {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    for (_, data) in members {
        bytes.extend_from_slice(data);
    }
    bytes
}

fn dbc<const N: usize>(rows: &[[u32; N]], strings: &[u8], stride: usize) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for value in [
        rows.len() as u32,
        N as u32,
        stride as u32,
        strings.len() as u32,
    ] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    for row in rows {
        let start = bytes.len();
        for value in row {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        // Short rows retain partial words; long rows contain authored padding bytes.
        bytes.resize(start + stride, 0x5a);
    }
    bytes.extend_from_slice(strings);
    bytes
}

fn model(id: u32, height_bits: u32, scale_bits: u32) -> [u32; 16] {
    let mut row = [0; 16];
    row[0] = id;
    row[2] = 1;
    row[4] = scale_bits;
    row[14] = 13.75_f32.to_bits(); // Width is deliberately unrelated to height.
    row[15] = height_bits;
    row
}

fn display(id: u32, model_id: u32, scale_bits: u32) -> [u32; 12] {
    let mut row = [0; 12];
    row[0] = id;
    row[1] = model_id;
    row[4] = scale_bits;
    row
}

fn load_bytes(models: Vec<u8>, displays: Vec<u8>) -> Result<CreatureCatalog> {
    let temp = Temp::new();
    temp.write(
        "dbc.MPQ",
        &[
            (CREATURE_MODEL_DATA, models),
            (CREATURE_DISPLAY_INFO, displays),
        ],
    );
    temp.load()
}

fn catalog(models: &[[u32; 16]], displays: &[[u32; 12]]) -> CreatureCatalog {
    load_bytes(dbc(models, MODEL_STRINGS, 64), dbc(displays, b"\0", 48)).unwrap()
}

#[test]
fn raw_height_joins_selected_display_to_exact_model_without_combining_scales() {
    let cat = catalog(
        &[
            model(7, 2.875_f32.to_bits(), 2.5_f32.to_bits()),
            model(9, 7.75_f32.to_bits(), 0.25_f32.to_bits()),
            model(u32::MAX - 1, 9.125_f32.to_bits(), 0.5_f32.to_bits()),
        ],
        &[
            display(101, 7, 3.0_f32.to_bits()),
            display(102, 9, 4.0_f32.to_bits()),
            display(103, 7, 5.0_f32.to_bits()),
            display(u32::MAX, u32::MAX - 1, 1.5_f32.to_bits()),
        ],
    );
    let selected: crate::CreatureCollisionHeightRow = cat.collision_height_row(101).unwrap();
    assert_eq!(
        selected,
        CreatureCollisionHeightRow {
            display_id: 101,
            model_id: 7,
            collision_height: 2.875,
            display_scale: 3.0,
            model_scale: 2.5,
        }
    );
    for (id, model_id, height, display_scale, model_scale) in [
        (102, 9, 7.75, 4.0, 0.25),
        (103, 7, 2.875, 5.0, 2.5),
        (u32::MAX, u32::MAX - 1, 9.125, 1.5, 0.5),
    ] {
        assert_eq!(
            cat.collision_height_row(id),
            Some(CreatureCollisionHeightRow {
                display_id: id,
                model_id,
                collision_height: height,
                display_scale,
                model_scale,
            })
        );
    }
    assert_eq!(cat.collision_height(101), Some(2.875));
    let render = cat.model(101).unwrap();
    assert_eq!(render.model_path, r"Authored\HeightFixture.mdx");
    assert_eq!(render.collision_height, 2.875);
    assert_eq!(render.scale, 7.5);
    assert_eq!(cat.model_scale(101), Some(7.5));
    assert_eq!(cat.display_scale(101), Some(3.0));
    assert_eq!(cat.collision_width_row(101).unwrap().collision_width, 13.75);
}

#[test]
fn production_reader_retains_height_and_scale_bits_without_numeric_validation() {
    let mut patterns = vec![
        0x0000_0000, // +zero
        0x8000_0000, // -zero
        0x0000_0001, // smallest positive subnormal
        0x8000_0001, // smallest negative subnormal
        0x007f_ffff, // largest positive subnormal
        0x807f_ffff, // largest negative subnormal
        0x0080_0000, // smallest positive normal
        0x8080_0000, // smallest negative normal
        0x3f80_0000, // positive finite
        0xbf80_0000, // negative finite
        0x7f7f_ffff, // largest positive finite
        0xff7f_ffff, // largest negative finite
        0x7f80_0000, // +infinity
        0xff80_0000, // -infinity
        0x7fc0_1234, // positive quiet NaN payload
        0xffc0_5678, // negative quiet NaN payload
        0x7f80_0001, // positive signaling NaN payload
        0xff80_0123, // negative signaling NaN payload
    ];
    // Exercise every bit position independently in every retained float, without f32 arithmetic.
    for bit in 0..32 {
        patterns.extend([1_u32 << bit, !(1_u32 << bit)]);
    }
    let models: Vec<_> = patterns
        .iter()
        .enumerate()
        .map(|(i, &bits)| model(i as u32 + 1, bits, bits))
        .collect();
    let displays: Vec<_> = patterns
        .iter()
        .enumerate()
        .map(|(i, &bits)| display(i as u32 + 1001, i as u32 + 1, bits))
        .collect();
    let cat = catalog(&models, &displays);
    for (i, bits) in patterns.into_iter().enumerate() {
        let raw = cat.collision_height_row(i as u32 + 1001).unwrap();
        assert_eq!(raw.collision_height.to_bits(), bits);
        assert_eq!(raw.display_scale.to_bits(), bits);
        assert_eq!(raw.model_scale.to_bits(), bits);
    }
}

#[test]
fn missing_rows_or_model_names_never_supply_a_height() {
    let mut empty_name = model(8, 8.0_f32.to_bits(), 1.0_f32.to_bits());
    empty_name[2] = 0;
    let mut invalid_name = model(9, 9.0_f32.to_bits(), 1.0_f32.to_bits());
    invalid_name[2] = MODEL_STRINGS.len() as u32 + 1;
    let cat = catalog(
        &[
            model(7, 2.875_f32.to_bits(), 1.0_f32.to_bits()),
            empty_name,
            invalid_name,
        ],
        &[
            display(101, 7, 1.0_f32.to_bits()),
            display(102, 8, 1.0_f32.to_bits()),
            display(103, 9, 1.0_f32.to_bits()),
            display(104, 10, 1.0_f32.to_bits()),
        ],
    );
    assert!(cat.collision_height_row(101).is_some());
    for id in [0, 7, 102, 103, 104, 105, u32::MAX] {
        assert_eq!(cat.collision_height_row(id), None);
    }
    assert_eq!(CreatureCatalog::default().collision_height_row(101), None);
    let empty_models = catalog(&[], &[display(101, 7, 1.0_f32.to_bits())]);
    assert_eq!(empty_models.collision_height_row(101), None);
    let empty_displays = catalog(&[model(7, 0, 0)], &[]);
    assert_eq!(empty_displays.collision_height_row(101), None);
}

#[test]
fn short_partial_and_padded_model_records_do_not_certify_raw_height() {
    let height_bits = 0x4123_4567;
    let models = [model(7, height_bits, 2.5_f32.to_bits())];
    let displays = [display(101, 7, 3.0_f32.to_bits())];
    for stride in [12, 16, 20, 56, 60, 61, 62, 63, 65, 68] {
        let cat = load_bytes(
            dbc(&models, MODEL_STRINGS, stride),
            dbc(&displays, b"\0", 48),
        )
        .unwrap();
        assert_eq!(cat.collision_height_row(101), None, "stride {stride}");
        // Keep the old client accessor's exact permissive partial-word/zero-padding behavior.
        let expected = match stride {
            0..=60 => 0,
            61 => height_bits & 0xff,
            62 => height_bits & 0xffff,
            63 => height_bits & 0xff_ffff,
            _ => height_bits,
        };
        assert_eq!(cat.collision_height(101).unwrap().to_bits(), expected);
        assert_eq!(cat.model(101).unwrap().collision_height.to_bits(), expected);
    }
}

#[test]
fn short_partial_and_padded_display_records_do_not_certify_raw_height_join() {
    let models = [model(7, 2.875_f32.to_bits(), 2.5_f32.to_bits())];
    let displays = [display(101, 7, 3.0_f32.to_bits())];
    for stride in [8, 16, 19, 20, 44, 45, 46, 47, 49, 52] {
        let cat = load_bytes(
            dbc(&models, MODEL_STRINGS, 64),
            dbc(&displays, b"\0", stride),
        )
        .unwrap();
        assert_eq!(cat.collision_height_row(101), None, "stride {stride}");
        assert_eq!(cat.collision_height(101), Some(2.875));
    }
}

#[test]
fn strict_height_and_width_availability_agree_for_every_retention_failure() {
    let mut empty_name = model(8, 0x7fc0_1234, 0x8000_0000);
    empty_name[2] = 0;
    let mut invalid_name = model(9, 0xbf80_0000, 0x7f80_0000);
    invalid_name[2] = MODEL_STRINGS.len() as u32 + 1;
    let models = [model(7, 0xff80_0001, 0x8000_0001), empty_name, invalid_name];
    let displays = [
        display(101, 7, 0x7fc0_5678),
        display(102, 8, 0),
        display(103, 9, 0),
        display(104, 10, 0),
    ];
    for model_stride in [12, 60, 61, 62, 63, 64, 65, 68] {
        for display_stride in [8, 44, 45, 46, 47, 48, 49, 52] {
            let cat = load_bytes(
                dbc(&models, MODEL_STRINGS, model_stride),
                dbc(&displays, b"\0", display_stride),
            )
            .unwrap();
            for id in [0, 7, 101, 102, 103, 104, 105, u32::MAX] {
                let height = cat.collision_height_row(id);
                let width = cat.collision_width_row(id);
                assert_eq!(
                    height.is_some(),
                    width.is_some(),
                    "display {id}, model stride {model_stride}, display stride {display_stride}"
                );
                assert_eq!(
                    height.is_some(),
                    id == 101 && model_stride == 64 && display_stride == 48
                );
            }
        }
    }
}

#[test]
fn missing_or_malformed_dbc_members_keep_production_load_errors() {
    let models = dbc(
        &[model(7, 2.875_f32.to_bits(), 2.5_f32.to_bits())],
        MODEL_STRINGS,
        64,
    );
    let displays = dbc(&[display(101, 7, 3.0_f32.to_bits())], b"\0", 48);
    for member in [
        (CREATURE_MODEL_DATA, models.clone()),
        (CREATURE_DISPLAY_INFO, displays.clone()),
    ] {
        let temp = Temp::new();
        temp.write("dbc.MPQ", &[member]);
        assert!(temp.load().is_err());
    }
    for is_model in [true, false] {
        let original = if is_model { &models } else { &displays };
        let mut bad_magic = original.clone();
        bad_magic[0] = b'X';
        let mut wrong_fields = original.clone();
        wrong_fields[8..12].copy_from_slice(&1_u32.to_le_bytes());
        let mut missing_record = original.clone();
        missing_record[4..8].copy_from_slice(&2_u32.to_le_bytes());
        for malformed in [
            bad_magic,
            wrong_fields,
            missing_record,
            original[..original.len() - 1].to_vec(),
            original[..21].to_vec(),
            original[..19].to_vec(),
        ] {
            let result = if is_model {
                load_bytes(malformed, displays.clone())
            } else {
                load_bytes(models.clone(), malformed)
            };
            assert!(result.is_err());
        }
    }
}

#[test]
fn winning_display_mapping_and_model_bytes_come_from_the_actual_patch_chain() {
    let temp = Temp::new();
    temp.write(
        "dbc.MPQ",
        &[
            (
                CREATURE_MODEL_DATA,
                dbc(
                    &[model(7, 1.0_f32.to_bits(), 1.0_f32.to_bits())],
                    MODEL_STRINGS,
                    64,
                ),
            ),
            (
                CREATURE_DISPLAY_INFO,
                dbc(&[display(101, 7, 1.0_f32.to_bits())], b"\0", 48),
            ),
        ],
    );
    temp.write(
        "patch.MPQ",
        &[
            (
                CREATURE_MODEL_DATA,
                dbc(
                    &[model(9, 7.75_f32.to_bits(), 0.25_f32.to_bits())],
                    MODEL_STRINGS,
                    64,
                ),
            ),
            (
                CREATURE_DISPLAY_INFO,
                dbc(&[display(101, 9, 4.0_f32.to_bits())], b"\0", 48),
            ),
        ],
    );
    assert_eq!(
        temp.load().unwrap().collision_height_row(101),
        Some(CreatureCollisionHeightRow {
            display_id: 101,
            model_id: 9,
            collision_height: 7.75,
            display_scale: 4.0,
            model_scale: 0.25,
        })
    );
    // A short winning model must not borrow a complete base row or its height.
    temp.write(
        "patch-2.MPQ",
        &[(
            CREATURE_MODEL_DATA,
            dbc(
                &[model(9, 9.0_f32.to_bits(), 1.0_f32.to_bits())],
                MODEL_STRINGS,
                60,
            ),
        )],
    );
    let cat = temp.load().unwrap();
    assert_eq!(cat.collision_height_row(101), None);
    assert_eq!(cat.collision_height(101), Some(0.0));
}
