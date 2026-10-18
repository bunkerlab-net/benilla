// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored synthetic raw members only. No install data or caller-supplied products.
use super::*;

fn set_word(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn chunk(magic: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut bytes = magic.to_vec();
    bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(data);
    bytes
}

fn dbc(fields: usize, rows: &[Vec<u32>], strings: &[u8]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for value in [rows.len(), fields, fields * 4, strings.len()] {
        bytes.extend_from_slice(&(value as u32).to_le_bytes());
    }
    for row in rows {
        assert_eq!(row.len(), fields);
        for value in row {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    bytes.extend_from_slice(strings);
    bytes
}

fn area_row(id: u32, explore: u32) -> Vec<u32> {
    let mut row = vec![0; 25];
    row[0] = id;
    row[1] = 77;
    row[3] = explore;
    row
}

struct Inputs {
    map: Vec<u8>,
    area: Vec<u8>,
    liquid: Vec<u8>,
    wdt: Vec<u8>,
    adt: Vec<u8>,
}

impl Inputs {
    fn new() -> Self {
        let mut map_row = vec![0; 42];
        map_row[0] = 77;
        map_row[1] = 1;
        let map = dbc(42, &[map_row], b"\0Authored\0");
        let area = dbc(25, &[area_row(7, 0x1234)], b"\0");
        let liquid = dbc(4, &[vec![1, 0, 0, 3]], b"\0");
        let mut wdt = chunk(b"REVM", &18u32.to_le_bytes());
        wdt.extend(chunk(b"DHPM", &[0; 32]));
        let mut main = vec![0; 64 * 64 * 8];
        set_word(&mut main, (4 * 64 + 9) * 8, 1);
        wdt.extend(chunk(b"NIAM", &main));
        wdt.extend(chunk(b"OMWM", &[]));
        let mut adt = chunk(b"REVM", &18u32.to_le_bytes());
        let mut mhdr = [0; 64];
        set_word(&mut mhdr, 4, 64);
        adt.extend(chunk(b"RDHM", &mhdr));
        adt.extend(chunk(b"NICM", &[0; CELLS * 16]));
        for slot in 0..CELLS {
            let mut header = [0; 128];
            set_word(&mut header, 4, (slot % 16) as u32);
            set_word(&mut header, 8, (slot / 16) as u32);
            set_word(&mut header, 0x14, 136);
            set_word(&mut header, 0x34, 7);
            set_word(&mut header, 0x70, 16.0f32.to_bits());
            let mut cell = header.to_vec();
            cell.extend(chunk(b"TVCM", &[0; 145 * 4]));
            let root = chunk(b"KNCM", &cell);
            let at = adt.len();
            set_word(&mut adt, 92 + slot * 16, at as u32);
            set_word(&mut adt, 92 + slot * 16 + 4, root.len() as u32);
            adt.extend(root);
        }
        Self {
            map,
            area,
            liquid,
            wdt,
            adt,
        }
    }

    fn produce(&self) -> Result<Vec<u8>> {
        produce(
            &self.map,
            &self.area,
            &self.liquid,
            &self.wdt,
            &self.adt,
            77,
            4,
            9,
        )
    }

    fn cell_at(&self, slot: usize) -> usize {
        word(&self.adt, 92 + slot * 16).unwrap() as usize
    }

    fn header_word(&mut self, slot: usize, field: usize, value: u32) {
        let at = self.cell_at(slot) + 8 + field;
        set_word(&mut self.adt, at, value);
    }

    fn height(&mut self, slot: usize, index: usize, value: f32) {
        let at = self.cell_at(slot) + 136 + 8 + index * 4;
        set_word(&mut self.adt, at, value.to_bits());
    }
}

fn height_header(bytes: &[u8]) -> usize {
    word(bytes, 16).unwrap() as usize
}

fn sample(bytes: &[u8], inner: bool, y: usize, x: usize) -> f32 {
    let start = height_header(bytes) + 16;
    let index = if inner {
        129 * 129 + y * 128 + x
    } else {
        y * 129 + x
    };
    number(bytes, start + index * 4).unwrap()
}

#[test]
fn flat_output_is_exact_source_layout_with_holes_even_when_zero() {
    let input = Inputs::new();
    assert_eq!(map_directory(&input.map, 77).unwrap(), "Authored");
    let bytes = input.produce().unwrap();
    assert_eq!(bytes.len(), 576);
    let mut expected = b"MAPSz1.4".to_vec();
    for value in [40u32, 8, 48, 16, 0, 0, 64, 512] {
        expected.extend(value.to_le_bytes());
    }
    expected.extend(b"AREA");
    expected.extend(1u16.to_le_bytes());
    expected.extend(0x1234u16.to_le_bytes());
    expected.extend(b"MHGT");
    expected.extend(1u32.to_le_bytes());
    expected.extend(16.0f32.to_le_bytes());
    expected.extend(16.0f32.to_le_bytes());
    expected.extend([0; 512]);
    assert_eq!(bytes, expected);
}

#[test]
fn source_interleave_v9_before_v8_and_row_major_seam_overwrite() {
    let mut input = Inputs::new();
    input.height(0, 0, 1.0);
    input.height(0, 9, 2.0);
    input.height(0, 8, 99.0); // Overwritten by the next cell at the shared column.
    input.height(1, 0, 3.0);
    input.height(0, 8 * 17, 88.0); // Overwritten by the next cell row.
    input.height(16, 0, 4.0);
    input.height(17, 0, 5.0); // Winner at the four-cell crossing.
    let bytes = input.produce().unwrap();
    assert_eq!(word(&bytes, height_header(&bytes) + 4).unwrap(), 0);
    assert_eq!(sample(&bytes, false, 0, 0), 17.0);
    assert_eq!(sample(&bytes, true, 0, 0), 18.0);
    assert_eq!(sample(&bytes, false, 0, 8), 19.0);
    assert_eq!(sample(&bytes, false, 8, 0), 20.0);
    assert_eq!(sample(&bytes, false, 8, 8), 21.0);
    assert_eq!(number(&bytes, height_header(&bytes) + 12).unwrap(), 21.0);
}

#[test]
fn mcin_slot_not_mcnk_indices_or_physical_order_defines_output() {
    let mut input = Inputs::new();
    input.header_word(0, 4, u32::MAX);
    input.header_word(0, 8, u32::MAX);
    input.header_word(0, 0x70, 32.0f32.to_bits());
    let first = word(&input.adt, 92).unwrap();
    let second = word(&input.adt, 108).unwrap();
    set_word(&mut input.adt, 92, second);
    set_word(&mut input.adt, 108, first);
    let bytes = input.produce().unwrap();
    assert_eq!(sample(&bytes, false, 0, 0), 16.0);
    assert_eq!(sample(&bytes, false, 0, 8), 32.0);
}

#[test]
fn float_zero_option_keeps_nearly_flat_values_and_default_height_limit() {
    let mut input = Inputs::new();
    input.height(0, 9, 0.001);
    let bytes = input.produce().unwrap();
    assert_eq!(word(&bytes, height_header(&bytes) + 4).unwrap(), 0);
    assert_eq!(
        sample(&bytes, true, 0, 0).to_bits(),
        (16.0f32 + 0.001).to_bits()
    );
    for slot in 0..CELLS {
        input.header_word(slot, 0x70, (-600.0f32).to_bits());
    }
    let bytes = input.produce().unwrap();
    assert_eq!(word(&bytes, height_header(&bytes) + 4).unwrap(), 1);
    assert_eq!(number(&bytes, height_header(&bytes) + 8).unwrap(), -500.0);
    assert_eq!(number(&bytes, height_header(&bytes) + 12).unwrap(), -500.0);
}

#[test]
fn preserves_source_extrema_seeds_outside_normal_world_range() {
    let mut input = Inputs::new();
    for slot in 0..CELLS {
        input.header_word(slot, 0x70, 30000.0f32.to_bits());
    }
    let bytes = input.produce().unwrap();
    assert_eq!(word(&bytes, height_header(&bytes) + 4).unwrap(), 0);
    assert_eq!(number(&bytes, height_header(&bytes) + 8).unwrap(), 20000.0);
    assert_eq!(number(&bytes, height_header(&bytes) + 12).unwrap(), 30000.0);
}

#[test]
fn absent_mcvt_assigns_base_directly_preserving_negative_zero() {
    let mut input = Inputs::new();
    input.adt.truncate(4188);
    for slot in 0..CELLS {
        let mut header = [0; 128];
        set_word(&mut header, 0x34, 7);
        set_word(&mut header, 0x70, (-0.0_f32).to_bits());
        let root = chunk(b"KNCM", &header);
        let at = input.adt.len();
        set_word(&mut input.adt, 92 + slot * 16, at as u32);
        set_word(&mut input.adt, 92 + slot * 16 + 4, root.len() as u32);
        input.adt.extend(root);
    }
    let bytes = input.produce().unwrap();
    let at = height_header(&bytes);
    assert_eq!(word(&bytes, at + 4).unwrap(), 1);
    assert_eq!(word(&bytes, at + 8).unwrap(), (-0.0_f32).to_bits());
    assert_eq!(word(&bytes, at + 12).unwrap(), (-0.0_f32).to_bits());
}

#[test]
fn area_fallback_narrowing_last_row_and_hole_order_are_source_derived() {
    let mut input = Inputs::new();
    input.area = dbc(
        25,
        &[area_row(7, 10), area_row(7, 0x12345), area_row(0, 42)],
        b"\0",
    );
    input.header_word(0, 0x34, 0); // ID zero stays fallback despite a row.
    input.header_word(1, 0x34, 999);
    input.header_word(2, 0x3c, 0xabcd);
    let bytes = input.produce().unwrap();
    assert_eq!(word(&bytes, 12).unwrap(), 520);
    assert_eq!(half(&bytes, 44).unwrap(), 0);
    assert_eq!(half(&bytes, 46).unwrap(), 0);
    assert_eq!(half(&bytes, 48).unwrap(), 0xffff);
    assert_eq!(half(&bytes, 50).unwrap(), 0xffff);
    assert_eq!(half(&bytes, 52).unwrap(), 0x2345);
    let holes = word(&bytes, 32).unwrap() as usize;
    assert_eq!(half(&bytes, holes + 4).unwrap(), 0xabcd);
}

#[test]
fn source_wdt_whole_word_selection_and_output_axis_order() {
    let input = Inputs::new();
    assert!(validate_selection_wdt(&input.wdt, 4, 9).is_ok());
    assert!(validate_selection_wdt(&input.wdt, 9, 4).is_err());
    assert!(validate_selection_wdt(&input.wdt, 64, 4).is_err());
    let mut wdt = input.wdt;
    set_word(&mut wdt, 60 + (4 * 64 + 9) * 8, 2);
    assert!(validate_selection_wdt(&wdt, 4, 9).is_ok());
}

#[test]
fn rejects_all_liquid_header_offset_size_and_root_representations() {
    for (field, value) in [(0, 4), (0, 8), (0, 16), (0, 32), (0x60, 1), (0x64, 1)] {
        let mut input = Inputs::new();
        input.header_word(0, field, value);
        assert!(input.produce().is_err(), "field {field} value {value}");
    }
    let mut input = Inputs::new();
    set_word(&mut input.adt, 20 + 0x28, 1);
    assert!(input.produce().is_err());
    let mut input = Inputs::new();
    input.adt.extend(chunk(b"O2HM", &[]));
    assert!(input.produce().is_err());
    let mut input = Inputs::new();
    let at = input.cell_at(0) + 136;
    input.adt[at..at + 4].copy_from_slice(b"QLCM");
    assert!(input.produce().is_err());
}

#[test]
fn rejects_duplicate_dangling_and_wrong_size_mcin_references() {
    let mut input = Inputs::new();
    let first = word(&input.adt, 92).unwrap();
    set_word(&mut input.adt, 108, first);
    assert!(input.produce().is_err());
    let mut input = Inputs::new();
    set_word(&mut input.adt, 92, 84);
    assert!(input.produce().is_err());
    let mut input = Inputs::new();
    set_word(&mut input.adt, 96, 1);
    assert!(input.produce().is_err());
    let mut input = Inputs::new();
    set_word(&mut input.adt, 24, 65);
    assert!(input.produce().is_err());
}

#[test]
fn refuses_display_parser_offset_fallback_and_malformed_raw_heights() {
    for offset in [0, 128, 144, u32::MAX] {
        let mut input = Inputs::new();
        input.header_word(0, 0x14, offset);
        assert!(input.produce().is_err(), "offset {offset}");
    }
    for size in [0, 576, 584, u32::MAX] {
        let mut input = Inputs::new();
        let at = input.cell_at(0) + 136 + 4;
        set_word(&mut input.adt, at, size);
        assert!(input.produce().is_err(), "size {size}");
    }
}

#[test]
fn rejects_nonfinite_raw_and_overflowing_sums() {
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut input = Inputs::new();
        input.height(0, 144, value);
        assert!(input.produce().is_err());
    }
    let mut input = Inputs::new();
    input.header_word(0, 0x70, f32::MAX.to_bits());
    input.height(0, 0, f32::MAX);
    assert!(input.produce().is_err());
}

#[test]
fn complete_inputs_not_filtered_records_are_validated() {
    let input = Inputs::new();
    for end in [0, 3, 11, 83, 91, 4187, input.adt.len() - 1] {
        assert!(produce(
            &input.map,
            &input.area,
            &input.liquid,
            &input.wdt,
            &input.adt[..end],
            77,
            4,
            9
        )
        .is_err());
    }
    let mut changed = Inputs::new();
    changed.adt.push(0);
    assert!(changed.produce().is_err());
    let mut changed = Inputs::new();
    changed.liquid.push(0); // Loaded even though no dry conversion indexes it.
    assert!(changed.produce().is_err());
    let mut changed = Inputs::new();
    changed.area[8] = 24;
    assert!(changed.produce().is_err());
    let mut changed = Inputs::new();
    set_word(&mut changed.wdt, 20, 1);
    assert!(changed.produce().is_err());
}

#[test]
fn map_directory_cannot_inject_paths_or_alias_another_row() {
    let mut row = vec![0; 42];
    row[0] = 77;
    row[1] = 1;
    for name in ["../Other", "D\\Other", "D/Other", "", ".", "a b"] {
        let strings = format!("\0{name}\0");
        assert!(map_directory(&dbc(42, &[row.clone()], strings.as_bytes()), 77).is_err());
    }
    assert!(map_directory(&dbc(42, &[row.clone(), row], b"\0Authored\0"), 77).is_err());
    assert!(map_directory(&Inputs::new().map, 78).is_err());
}

#[test]
fn complete_output_matches_independent_pinned_source_writer_bytes() {
    // Generated by the unchanged entire ConvertADT and original raw ADT/DBC
    // loaders. The oracle links no Rust writer and reads no installation assets.
    const MAP: &[u8] = include_bytes!("../../tools/fixtures/Map.dbc");
    const AREA: &[u8] = include_bytes!("../../tools/fixtures/AreaTable.dbc");
    const LIQUID: &[u8] = include_bytes!("../../tools/fixtures/LiquidType.dbc");
    const WDT: &[u8] = include_bytes!("../../tools/fixtures/source.wdt");
    macro_rules! check {
        ($name:literal) => {{
            let raw = include_bytes!(concat!("../../tools/fixtures/", $name, ".adt"));
            let expected = include_bytes!(concat!("../../tools/fixtures/", $name, ".map"));
            let actual = produce(MAP, AREA, LIQUID, WDT, raw, 1, 31, 32)
                .unwrap_or_else(|error| panic!("{}: {error:#}", $name));
            assert_eq!(actual.as_slice(), expected.as_slice(), "{}", $name);
        }};
    }
    check!("flat");
    check!("varied");
    check!("seams");
    check!("clamp_mixed");
    check!("clamp_flat");
    check!("tiny_range");
    check!("area_full");
    check!("area_fallback");
    check!("holes");
    check!("nonflat_holes");
    check!("upper_sentinel");
    check!("missing_mcvt");
    check!("missing_mcvt_signed_zero");
}
