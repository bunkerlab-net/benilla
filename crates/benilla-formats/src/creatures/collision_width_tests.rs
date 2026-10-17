//! Original synthetic WDBC bytes exercise the production decoder and materialization path.
//! No rows, names or files are copied from a game installation.

use super::*;

const MODEL_STRINGS: &[u8] = b"\0Authored\\WidthFixture.mdx\0";
const DISPLAY_STRINGS: &[u8] = b"\0AuthoredSkin\0";

fn dbc<const N: usize>(rows: &[[u32; N]], strings: &[u8]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for value in [
        rows.len() as u32,
        N as u32,
        (N * 4) as u32,
        strings.len() as u32,
    ] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    for row in rows {
        for value in row {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    bytes.extend_from_slice(strings);
    bytes
}

fn model_row(id: u32, width: f32, height: f32, scale: f32) -> [u32; 16] {
    [
        id,
        0, // flags
        1, // model-name string offset
        2, // size class
        scale.to_bits(),
        11, // blood
        5,  // footprint texture
        36.0_f32.to_bits(),
        72.0_f32.to_bits(),
        1.0_f32.to_bits(), // footprint particle scale
        12,                // foley material
        13,                // footstep shake
        14,                // death-thud shake
        15,                // sound
        width.to_bits(),
        height.to_bits(),
    ]
}

fn display_row(id: u32, model_id: u32, scale: f32) -> [u32; 12] {
    [
        id,
        model_id,
        0,
        0,
        scale.to_bits(),
        129,
        1,
        0,
        0,
        u32::MAX,
        21,
        0,
    ]
}

fn catalog(models: &[[u32; 16]], displays: &[[u32; 12]]) -> CreatureCatalog {
    CreatureCatalog {
        models: model_rows_from(&dbc(models, MODEL_STRINGS)).unwrap(),
        display: display_rows_from(&dbc(displays, DISPLAY_STRINGS)).unwrap(),
        extra: HashMap::new(),
    }
}

#[test]
fn raw_width_uses_display_to_model_identity_without_folding_dbc_scales() {
    // The two model rows deliberately share a path but have different collision data.
    let cat = catalog(
        &[
            model_row(7, 1.375, 2.875, 2.5),
            model_row(9, 6.25, 7.75, 0.25),
        ],
        &[
            display_row(101, 7, 3.0),
            display_row(102, 9, 4.0),
            display_row(103, 7, 5.0),
        ],
    );
    let native: crate::CreatureCollisionWidthRow = cat.collision_width_row(101).unwrap();
    assert_eq!(
        native,
        CreatureCollisionWidthRow {
            display_id: 101,
            model_id: 7,
            collision_width: 1.375,
            display_scale: 3.0,
            model_scale: 2.5,
        }
    );
    let other = cat.collision_width_row(102).unwrap();
    assert_eq!((other.display_id, other.model_id), (102, 9));
    assert_eq!(other.collision_width, 6.25);
    assert_eq!((other.display_scale, other.model_scale), (4.0, 0.25));
    let shared = cat.collision_width_row(103).unwrap();
    assert_eq!((shared.display_id, shared.model_id), (103, 7));
    assert_eq!(shared.collision_width, native.collision_width);
    assert_eq!(shared.display_scale, 5.0);

    // Width retention must leave the existing rendering/height reads unchanged.
    let render = cat.model(101).unwrap();
    assert_eq!(render.collision_height, 2.875);
    assert_eq!(cat.collision_height(101), Some(2.875));
    assert_eq!(cat.collision_height(102), Some(7.75));
    assert_eq!(render.scale, 7.5);
    assert_eq!(cat.model_scale(101), Some(7.5));
    assert_eq!(cat.display_scale(101), Some(3.0));
    assert_eq!(render.model_path, r"Authored\WidthFixture.mdx");
    assert_eq!(render.textures, [Some("AuthoredSkin".into()), None, None]);
    assert!(render.npc_appearance.is_none());
    assert_eq!((render.blood_display, render.blood_model), (21, 11));
    assert_eq!(cat.size_class(101), Some(2));
    assert_eq!(cat.foley_material(101), Some(12));
    assert_eq!(cat.footstep_shake(101), Some(13));
    assert_eq!(cat.death_thud_shake(101), Some(14));
    assert_eq!(
        cat.footprint(101),
        Some(FootprintParams {
            texture_id: 5,
            length: 1.0,
            width: 2.0
        })
    );
}

#[test]
fn raw_zero_width_is_present_and_keeps_its_sign_without_a_fallback() {
    let cat = catalog(
        &[model_row(7, 0.0, 2.0, 2.5), model_row(9, -0.0, 3.0, 4.0)],
        &[display_row(101, 7, 3.0), display_row(102, 9, 5.0)],
    );
    for (display, expected_bits) in [(101, 0), (102, 0x8000_0000)] {
        assert_eq!(
            cat.collision_width_row(display)
                .unwrap()
                .collision_width
                .to_bits(),
            expected_bits
        );
    }
}

#[test]
fn missing_display_model_or_retained_model_name_does_not_supply_a_width() {
    let mut empty_name = model_row(8, 8.0, 2.0, 1.0);
    empty_name[2] = 0;
    let mut invalid_name = model_row(9, 9.0, 2.0, 1.0);
    invalid_name[2] = MODEL_STRINGS.len() as u32 + 1;
    let cat = catalog(
        &[model_row(7, 1.375, 2.0, 1.0), empty_name, invalid_name],
        &[
            display_row(101, 7, 1.0),
            display_row(102, 8, 1.0),
            display_row(103, 9, 1.0),
            display_row(104, 10, 1.0),
        ],
    );
    assert!(cat.collision_width_row(101).is_some());
    for missing in [7, 102, 103, 104, 105] {
        assert_eq!(cat.collision_width_row(missing), None);
        assert!(cat.model(missing).is_none());
    }
    assert_eq!(CreatureCatalog::default().collision_width_row(101), None);
}

#[test]
fn raw_float_bits_are_retained_without_claiming_numeric_validation() {
    for width_bits in [
        0x7fc0_1234, // NaN payload
        0x7f80_0000, // +infinity
        0xff80_0000, // -infinity
        0xbf80_0000, // negative finite width
        0x0000_0001, // subnormal
    ] {
        let cat = catalog(
            &[model_row(7, f32::from_bits(width_bits), 2.0, f32::INFINITY)],
            &[display_row(101, 7, f32::from_bits(0x7fc0_5678))],
        );
        let raw = cat.collision_width_row(101).unwrap();
        assert_eq!(raw.collision_width.to_bits(), width_bits);
        assert_eq!(raw.model_scale.to_bits(), 0x7f80_0000);
        assert_eq!(raw.display_scale.to_bits(), 0x7fc0_5678);
    }
}

#[test]
fn malformed_dbc_bytes_keep_existing_parse_errors() {
    let models = dbc(&[model_row(7, 1.375, 2.0, 1.0)], MODEL_STRINGS);
    let displays = dbc(&[display_row(101, 7, 1.0)], DISPLAY_STRINGS);
    let mut bad_magic = models.clone();
    bad_magic[0] = b'X';
    assert!(model_rows_from(&bad_magic).is_err());
    let mut wrong_fields = models.clone();
    wrong_fields[8..12].copy_from_slice(&15_u32.to_le_bytes());
    assert!(model_rows_from(&wrong_fields).is_err());
    assert!(model_rows_from(&models[..models.len() - 1]).is_err());
    let mut bad_magic = displays.clone();
    bad_magic[0] = b'X';
    assert!(display_rows_from(&bad_magic).is_err());
    let mut wrong_fields = displays.clone();
    wrong_fields[8..12].copy_from_slice(&11_u32.to_le_bytes());
    assert!(display_rows_from(&wrong_fields).is_err());
    assert!(display_rows_from(&displays[..displays.len() - 1]).is_err());
}

#[test]
fn incomplete_model_record_does_not_turn_reader_zero_padding_into_a_raw_width() {
    // The shared reader accepts shorter strides. Width lookup alone requires the full schema;
    // existing render/height parsing is left unchanged, even with all of field 14 present.
    for stride in [56, 60] {
        let mut models = dbc(&[model_row(7, 1.375, 2.0, 2.5)], MODEL_STRINGS);
        models[12..16].copy_from_slice(&(stride as u32).to_le_bytes());
        models.drain(20 + stride..20 + 64);
        let cat = CreatureCatalog {
            models: model_rows_from(&models).unwrap(),
            display: display_rows_from(&dbc(&[display_row(101, 7, 3.0)], DISPLAY_STRINGS)).unwrap(),
            extra: HashMap::new(),
        };
        assert_eq!(cat.collision_width_row(101), None);
        assert_eq!(cat.model(101).unwrap().scale, 7.5);
        assert_eq!(cat.collision_height(101), Some(0.0));
    }
}

#[test]
fn incomplete_display_record_does_not_certify_a_raw_width_join() {
    let mut displays = dbc(&[display_row(101, 7, 3.0)], DISPLAY_STRINGS);
    displays[12..16].copy_from_slice(&44_u32.to_le_bytes());
    displays.drain(20 + 44..20 + 48);
    let cat = CreatureCatalog {
        models: model_rows_from(&dbc(&[model_row(7, 1.375, 2.0, 2.5)], MODEL_STRINGS)).unwrap(),
        display: display_rows_from(&displays).unwrap(),
        extra: HashMap::new(),
    };
    assert_eq!(cat.collision_width_row(101), None);
    assert_eq!(cat.model(101).unwrap().scale, 7.5);
    assert_eq!(cat.collision_height(101), Some(2.0));
}
