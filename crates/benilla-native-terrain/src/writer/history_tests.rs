// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored boundary queries and sequential last-writer checks; no installation data.
use super::*;

fn with_liquids(mut make: impl FnMut(usize) -> Option<(u32, Vec<u8>)>) -> Inputs {
    let mut input = Inputs::new();
    let original = input.adt.clone();
    input.adt.truncate(4188);
    for slot in 0..CELLS {
        let root = word(&original, 92 + slot * 16).unwrap() as usize;
        let size = word(&original, root + 4).unwrap() as usize;
        let mut cell = original[root + 8..root + 8 + size].to_vec();
        if let Some((flags, data)) = make(slot) {
            set_word(&mut cell, 0, flags);
            let relative = cell.len() + 8;
            set_word(&mut cell, 0x60, relative as u32);
            set_word(&mut cell, 0x64, 812);
            cell.extend(chunk(b"QLCM", &data));
        }
        let root = chunk(b"KNCM", &cell);
        let at = input.adt.len();
        set_word(&mut input.adt, 92 + slot * 16, at as u32);
        set_word(&mut input.adt, 92 + slot * 16 + 4, root.len() as u32);
        input.adt.extend(root);
    }
    input
}

fn liquid_data(mut height: impl FnMut(usize, usize) -> f32, visible: &[(usize, usize)]) -> Vec<u8> {
    let mut data = vec![0; 804];
    data[656..720].fill(0x0f);
    for y in 0..=8 {
        for x in 0..=8 {
            set_word(&mut data, 12 + (y * 9 + x) * 8, height(x, y).to_bits());
        }
    }
    for &(x, y) in visible {
        data[656 + y * 8 + x] = 0;
    }
    data
}

fn exact_61_selected() -> Inputs {
    with_liquids(|slot| {
        let (row, col) = (slot / 16, slot % 16);
        let visible: &[(usize, usize)] = match (row, col) {
            (0, 5) => &[(7, 0)],
            (0, 15) => &[(4, 0)],
            (15, 5) => &[(7, 7)],
            (15, 6 | 7) => &[],
            _ => return None,
        };
        Some((
            if visible.is_empty() { 0 } else { 4 },
            liquid_data(
                |x, y| 100.0 + row as f32 * 10.0 + col as f32 + y as f32 * 2.0 + x as f32 / 4.0,
                visible,
            ),
        ))
    })
}

fn with_history(input: &Inputs, history: &BoundaryHistory) -> Result<Vec<u8>> {
    produce_with_history(
        &input.map,
        &input.area,
        &input.liquid,
        &input.wdt,
        &input.adt,
        77,
        4,
        9,
        history,
    )
}

fn liquid_sample_bits(map: &[u8], x: usize, y: usize) -> u32 {
    let at = word(map, 24).unwrap() as usize;
    assert_eq!(&map[at..at + 4], b"MLIQ");
    assert_eq!(map[at + 4] & 2, 0);
    let (min_x, min_y, width) = (
        map[at + 8] as usize,
        map[at + 9] as usize,
        map[at + 10] as usize,
    );
    let types = if map[at + 4] & 1 == 0 { 768 } else { 0 };
    word(map, at + 16 + types + ((y - min_y) * width + x - min_x) * 4).unwrap()
}

#[test]
fn exact_61_missing_row_128_shape_is_authored_not_asset_derived() {
    let input = exact_61_selected();
    let history = boundary_history(&input.adt).unwrap();
    assert_eq!(
        history.unresolved().collect::<Vec<_>>(),
        (65..=125).map(|x| (x, 128)).collect::<Vec<_>>()
    );
    assert!(!history.is_resolved());
    let diagnostic = diagnose_tile(&input.adt).unwrap();
    assert_eq!(diagnostic.active_mclq_cells, 5);
    assert_eq!(
        serde_json::to_value(diagnostic.liquid_rectangle).unwrap(),
        serde_json::json!({"x":47,"y":0,"width":79,"height":129})
    );
    assert_eq!(
        serde_json::to_value(diagnostic.missing_liquid_heights).unwrap(),
        serde_json::json!({"count":61,"first_x":65,"first_y":128})
    );
    assert_eq!(
        diagnostic.liquid_result.unwrap_err().to_string(),
        "terrain.liquid_history"
    );
    assert_eq!(
        with_history(&input, &history).unwrap_err().to_string(),
        "terrain.liquid_history"
    );
}

#[test]
fn reverse_projection_matches_near_and_distant_sequential_writes_without_overwriting_current() {
    let selected = exact_61_selected();
    let oldest =
        with_liquids(|slot| (slot / 16 == 15).then(|| (0, liquid_data(|_, _| 19.25, &[]))));
    let nearer =
        with_liquids(|slot| (slot == 15 * 16 + 8).then(|| (0, liquid_data(|_, _| -0.0, &[]))));
    let dry = Inputs::new();
    let mut history = boundary_history(&selected.adt).unwrap();
    history.apply_predecessor(&dry.adt).unwrap();
    assert_eq!(history.unresolved().count(), 61);
    history.apply_predecessor(&nearer.adt).unwrap();
    assert_eq!(history.unresolved().next(), Some((73, 128)));
    assert_eq!(history.unresolved().count(), 53);
    for _ in 0..4 {
        history.apply_predecessor(&dry.adt).unwrap();
    }
    history.apply_predecessor(&oldest.adt).unwrap();
    assert!(history.is_resolved());
    // Older events cannot overwrite the newest accepted writer, even signed zero.
    history.mark_proven_start_zero();
    let output = with_history(&selected, &history).unwrap();
    for x in 47..=125 {
        let expected: f32 = match x {
            47 => 100.0 + 150.0 + 5.0 + 16.0 + 7.0 / 4.0,
            48..=55 => 100.0 + 150.0 + 6.0 + 16.0 + (x - 48) as f32 / 4.0,
            56..=64 => 100.0 + 150.0 + 7.0 + 16.0 + (x - 56) as f32 / 4.0,
            65..=72 => -0.0,
            _ => 19.25,
        };
        assert_eq!(
            liquid_sample_bits(&output, x, 128),
            expected.to_bits(),
            "x={x}"
        );
    }
    assert_eq!(liquid_sample_bits(&output, 48, 1), (-500.0_f32).to_bits());
}

#[test]
fn initial_positive_zero_requires_explicit_exhausted_prefix_proof() {
    let selected = exact_61_selected();
    let mut history = boundary_history(&selected.adt).unwrap();
    history.apply_predecessor(&Inputs::new().adt).unwrap();
    assert!(with_history(&selected, &history).is_err());
    history.mark_proven_start_zero();
    assert!(history.is_resolved());
    let output = with_history(&selected, &history).unwrap();
    for x in 65..=125 {
        assert_eq!(liquid_sample_bits(&output, x, 128), 0.0_f32.to_bits());
    }
    assert_ne!(liquid_sample_bits(&output, 64, 128), 0.0_f32.to_bits());
}

#[test]
fn predecessor_hidden_and_untyped_cells_write_without_predecessor_output_or_history() {
    let selected = exact_61_selected();
    for (flags, value) in [(0, 37.75), (4, -81.5)] {
        let prior =
            with_liquids(|slot| (slot / 16 == 15).then(|| (flags, liquid_data(|_, _| value, &[]))));
        if flags == 0 {
            assert_eq!(word(&prior.produce().unwrap(), 24).unwrap(), 0);
        } else {
            assert_eq!(
                prior.produce().unwrap_err().to_string(),
                "terrain.liquid_empty_bounds"
            );
        }
        let mut history = boundary_history(&selected.adt).unwrap();
        history.apply_predecessor(&prior.adt).unwrap();
        let output = with_history(&selected, &history).unwrap();
        for x in 65..=125 {
            assert_eq!(liquid_sample_bits(&output, x, 128), value.to_bits());
        }
    }
    // A predecessor requiring its own history still contributes its direct writes.
    let prior = with_liquids(|slot| {
        let visible: &[(usize, usize)] = match slot {
            0 => &[(0, 0)],
            15 => &[(4, 0)],
            248 => &[(0, 7)],
            _ => return None,
        };
        Some((4, liquid_data(|_, _| 98.5, visible)))
    });
    assert_eq!(
        prior.produce().unwrap_err().to_string(),
        "terrain.liquid_history"
    );
    let mut history = boundary_history(&selected.adt).unwrap();
    history.apply_predecessor(&prior.adt).unwrap();
    assert_eq!(history.unresolved().count(), 53);
    assert_eq!(history.unresolved().next(), Some((73, 128)));
}

#[test]
fn only_last_mcin_row_and_column_write_boundary_and_last_slot_wins_seams_corner() {
    let mut prior =
        with_liquids(|slot| Some((0, liquid_data(|x, y| (slot * 100 + y * 9 + x) as f32, &[]))));
    // Header indices are deliberately misleading; MCIN placement owns the result.
    for slot in 0..CELLS {
        prior.header_word(slot, 4, u32::MAX);
        prior.header_word(slot, 8, u32::MAX);
    }
    let writes = liquid::direct_boundary_writes(&read_cells(&prior.adt).unwrap()).unwrap();
    assert_eq!(writes.len(), 257);
    for x in 0..=128 {
        let col = (x / 8).min(15);
        let expected = ((240 + col) * 100 + 8 * 9 + x - col * 8) as f32;
        assert_eq!(
            writes[&(128 * 129 + x)].to_bits(),
            expected.to_bits(),
            "bottom x={x}"
        );
    }
    for y in 0..=128 {
        let row = (y / 8).min(15);
        let expected = ((row * 16 + 15) * 100 + (y - row * 8) * 9 + 8) as f32;
        assert_eq!(
            writes[&(y * 129 + 128)].to_bits(),
            expected.to_bits(),
            "right y={y}"
        );
    }
    let a = word(&prior.adt, 92 + 254 * 16).unwrap();
    let b = word(&prior.adt, 92 + 255 * 16).unwrap();
    set_word(&mut prior.adt, 92 + 254 * 16, b);
    set_word(&mut prior.adt, 92 + 255 * 16, a);
    let writes = liquid::direct_boundary_writes(&read_cells(&prior.adt).unwrap()).unwrap();
    assert_eq!(writes[&(128 * 129 + 128)].to_bits(), 25480.0_f32.to_bits());
    assert_eq!(writes[&(128 * 129 + 120)].to_bits(), 25472.0_f32.to_bits());
}

#[test]
fn no_liquid_or_no_height_selected_output_does_not_request_history() {
    let dry = Inputs::new();
    let history = boundary_history(&dry.adt).unwrap();
    assert!(history.is_resolved());
    assert_eq!(
        with_history(&dry, &history).unwrap(),
        dry.produce().unwrap()
    );
    let mut selected = exact_61_selected();
    for slot in [5, 15, 245, 246, 247] {
        let (_, liquid) = fixture_liquid_at(&selected.adt, slot);
        for vertex in 0..81 {
            set_word(
                &mut selected.adt,
                liquid + 8 + 12 + vertex * 8,
                (-500.0_f32).to_bits(),
            );
        }
    }
    let history = boundary_history(&selected.adt).unwrap();
    assert_eq!(history.unresolved().count(), 0);
    let output = with_history(&selected, &history).unwrap();
    assert_eq!(output, selected.produce().unwrap());
    assert_ne!(output[word(&output, 24).unwrap() as usize + 4] & 2, 0);
}

#[test]
fn malformed_predecessors_cannot_partially_resolve_queries_and_mh2o_never_skips() {
    let selected = exact_61_selected();
    let prior = with_liquids(|slot| {
        (slot / 16 == 15 || slot == 0).then(|| (0, liquid_data(|_, _| 22.0, &[])))
    });
    for mutation in 0..6 {
        let mut raw = prior.adt.clone();
        match mutation {
            0 => set_word(&mut raw, 20 + 0x28, 1),
            1 => raw.extend(chunk(b"O2HM", &[])),
            2 => {
                let (_, liquid) = fixture_liquid_at(&raw, 255);
                set_word(&mut raw, liquid + 8 + 12, f32::NAN.to_bits());
            }
            3 => {
                let (_, liquid) = fixture_liquid_at(&raw, 0);
                set_word(&mut raw, liquid + 8 + 12, f32::INFINITY.to_bits());
            }
            4 => {
                let (_, liquid) = fixture_liquid_at(&raw, 255);
                set_word(&mut raw, liquid + 4, 803);
            }
            _ => {
                raw.pop();
            }
        }
        let mut history = boundary_history(&selected.adt).unwrap();
        assert!(
            history.apply_predecessor(&raw).is_err(),
            "mutation {mutation}"
        );
        assert_eq!(history.unresolved().count(), 61);
        assert!(with_history(&selected, &history).is_err());
    }
}

#[test]
fn histories_bind_exact_selected_bytes_and_preserve_all_normal_validation() {
    let mut selected = exact_61_selected();
    let mut history = boundary_history(&selected.adt).unwrap();
    history.mark_proven_start_zero();
    selected.header_word(0, 4, 123); // Output-inert, but still a changed raw selected input.
    assert_eq!(
        with_history(&selected, &history).unwrap_err().to_string(),
        "terrain.liquid_history_selection_changed"
    );
    selected.map.clear();
    assert_eq!(
        with_history(&selected, &history).unwrap_err().to_string(),
        "terrain.Map"
    );
}

#[test]
fn map_rows_are_physical_order_and_unvisited_names_are_not_paths() {
    let raw = map_dbc(&[
        (900, b"Unused Name"),
        (77, b"Authored"),
        (0, b"../Unvisited"),
    ]);
    assert_eq!(
        map_directories(&raw).unwrap(),
        vec![
            (900, "Unused Name".to_owned()),
            (77, "Authored".to_owned()),
            (0, "../Unvisited".to_owned())
        ]
    );
    assert_eq!(map_selection(&raw, 77).unwrap().selected_row_ordinal, 1);
    assert!(map_directories(&map_dbc(&[(0, b"A"), (0, b"B")])).is_err());
    assert!(map_directories(&map_dbc(&[(0, &[b'A'; 64])])).is_err());
    assert!(map_directories(&map_dbc(&[(0, &[0xff])])).is_err());
}

#[test]
fn history_wdt_uses_complete_exist_word_and_source_order_with_safe_global_empty_skip() {
    let mut input = Inputs::new();
    // MVER12 + MPHD40 + MAIN header8.
    set_word(&mut input.wdt, 60 + (3 * 64 + 63) * 8, 0x8000_0000);
    set_word(&mut input.wdt, 60 + (5 * 64) * 8, 2);
    assert_eq!(
        history_wdt_tiles(&input.wdt).unwrap(),
        vec![(3, 63), (4, 9), (5, 0)]
    );
    let mut global = chunk(b"REVM", &18_u32.to_le_bytes());
    let mut mphd = [0; 32];
    set_word(&mut mphd, 0, 1);
    global.extend(chunk(b"DHPM", &mphd));
    global.extend(chunk(b"NIAM", &[0; 64 * 64 * 8]));
    global.extend(chunk(b"OMWM", b"World\\Authored.wmo\0"));
    global.extend(chunk(b"FDOM", &[0; 64]));
    assert!(history_wdt_tiles(&global).unwrap().is_empty());
    assert!(validate_selection_wdt(&global, 4, 9).is_err());
    set_word(&mut global, 60, 1);
    assert!(history_wdt_tiles(&global).is_err());
    for end in [0, 11, 59, input.wdt.len() - 1] {
        assert!(history_wdt_tiles(&input.wdt[..end]).is_err());
    }
    input.wdt.extend(chunk(b"XXXX", &[]));
    assert!(history_wdt_tiles(&input.wdt).is_err());
}

#[test]
fn projected_history_matches_complete_unchanged_source_converter_sequences() {
    // Each fixture sequence starts in a fresh original C++ process. Exhausting
    // this complete authored prefix is the proof for the remaining initial zero.
    macro_rules! check {
        ($name:literal, [$($prior:literal),*]) => {{
            let selected = include_bytes!(concat!("../../tools/fixtures/", $name, ".adt.test-fixture"));
            let mut history = boundary_history(selected).unwrap();
            let predecessors: &[&[u8]] = &[$(include_bytes!(concat!("../../tools/fixtures/", $prior, ".adt.test-fixture"))),*];
            for prior in predecessors.iter().rev() {
                history.apply_predecessor(prior).unwrap();
            }
            history.mark_proven_start_zero();
            let actual = produce_with_history(
                include_bytes!("../../tools/fixtures/Map.dbc.test-fixture"),
                include_bytes!("../../tools/fixtures/AreaTable.dbc.test-fixture"),
                include_bytes!("../../tools/fixtures/LiquidType.dbc.test-fixture"),
                include_bytes!("../../tools/fixtures/source.wdt.test-fixture"),
                selected, 1, 31, 32, &history,
            ).unwrap();
            let expected = include_bytes!(concat!("../../tools/fixtures/", $name, ".map"));
            assert_eq!(actual.as_slice(), expected.as_slice(), "{}", $name);
        }};
    }
    check!("liquid_stale_edge_a", ["liquid_water_flat"]);
    check!("liquid_stale_edge_b", ["liquid_ocean_flat"]);
    check!("liquid_history61_initial", []);
    check!("liquid_history61_positive", ["liquid_water_flat"]);
    check!(
        "liquid_history61_negative",
        ["liquid_water_flat", "liquid_ocean_flat"]
    );
    check!("liquid_history61_dry", ["liquid_ocean_flat", "flat"]);
    check!(
        "liquid_history61_untyped",
        ["liquid_ocean_flat", "liquid_untyped"]
    );
    check!(
        "liquid_history61_hidden",
        ["liquid_water_flat", "liquid_history_hidden_seed"]
    );
    check!("liquid_history61_seams", ["liquid_seams"]);
    check!(
        "liquid_history61_partial",
        ["liquid_ocean_flat", "liquid_history_partial_seed"]
    );
}
