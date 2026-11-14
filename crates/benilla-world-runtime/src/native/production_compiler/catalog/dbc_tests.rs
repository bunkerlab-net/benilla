// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
fn bytes(rows: &[&[u32]], fields: u32, strings: &[u8]) -> Vec<u8> {
    let size = rows.first().map_or(8, |r| r.len() * 4);
    let mut raw = b"WDBC".to_vec();
    for n in [rows.len() as u32, fields, size as u32, strings.len() as u32] {
        raw.extend_from_slice(&n.to_le_bytes());
    }
    for row in rows {
        for word in *row {
            raw.extend_from_slice(&word.to_le_bytes());
        }
    }
    raw.extend_from_slice(strings);
    raw
}
fn table(raw: Vec<u8>, fmt: &str, size: usize) -> Result<DbcView<'static>> {
    let d = Descriptor {
        sha256: json::hash_bytes(&raw),
        bytes: raw.len() as u64,
    };
    DbcView::new(
        Box::new(move |at, size| Ok(raw[at as usize..at as usize + size].to_vec())),
        d,
        Some(json!({"format":fmt,"record_bytes":size})),
    )
}
#[test]
fn sparse_duplicate_ids_preserve_physical_rows_and_last_winner() {
    let mut v = table(bytes(&[&[9, 17], &[2, 3], &[9, 99]], 2, b"\0"), "ni", 8).unwrap();
    assert_eq!(
        v.lookup(9).unwrap().unwrap(),
        json!({"ordinal":2,"key":9,"values":[[0,"n",9],[1,"i",99]]})
    );
    assert_eq!(v.record(0).unwrap()["values"][1][2], 17);
    assert!(v.lookup(3).unwrap().is_none());
    let report = v.report(None).unwrap();
    assert_eq!(report["lookup_extent"], 10);
    assert_eq!(report["duplicate_keys"], 1);
}
#[test]
fn unindexed_rows_are_indexed_by_physical_ordinal() {
    let mut v = table(bytes(&[&[5, 10], &[5, 11]], 2, b"\0"), "ii", 8).unwrap();
    assert_eq!(
        v.lookup(1).unwrap().unwrap()["values"],
        json!([[0, "i", 5], [1, "i", 11]])
    );
    assert!(v.lookup(5).unwrap().is_none());
}
#[test]
fn packed_byte_layout_and_source_outfit_packed_word() {
    let mut v = table(bytes(&[&[0x04030209]], 4, b"\0"), "bbbb", 4).unwrap();
    assert_eq!(
        v.record(0).unwrap()["values"],
        json!([[0, "b", 9], [1, "b", 2], [2, "b", 3], [3, "b", 4]])
    );
    let l = layout("CharStartOutfit.dbc").unwrap();
    let mut words = vec![0; 38];
    words[0] = 7;
    words[1] = 0x00010801;
    let mut v = table(
        bytes(&[&words], 41, b"\0"),
        l["format"].as_str().unwrap(),
        152,
    )
    .unwrap();
    assert_eq!(
        v.lookup(7).unwrap().unwrap()["values"][0],
        json!([1, "i", 0x00010801])
    );
}
#[test]
fn strings_are_source_bytes_and_float_sign_bits_survive() {
    let mut v = table(bytes(&[&[2, 1, 0x80000000]], 3, b"\0\xff\0"), "nsf", 12).unwrap();
    assert_eq!(
        v.record(0).unwrap()["values"],
        json!([[0, "n", 2], [1, "s", "ff"], [2, "f", 0x80000000u32]])
    );
    assert_eq!(
        table(bytes(&[&[2, 4]], 2, b"\0a\0"), "ns", 8)
            .err()
            .unwrap()
            .id,
        "dbc.string_offset"
    );
    let mut strings = vec![b'a'; 16387];
    strings[0] = 0;
    strings[16386] = 0;
    assert_eq!(
        table(bytes(&[&[2, 1]], 2, &strings), "ns", 8)
            .err()
            .unwrap()
            .id,
        "dbc.string_termination_or_limit"
    );
}
#[test]
fn malformed_zero_length_header_layout_and_trailing_refuse() {
    assert_eq!(
        table(Vec::new(), "ni", 8).err().unwrap().id,
        "dbc.byte_limit_or_truncated_header"
    );
    let mut raw = bytes(&[&[2, 3]], 2, b"\0");
    raw.push(0);
    assert_eq!(table(raw, "ni", 8).err().unwrap().id, "dbc.exact_length");
    let raw = bytes(&[&[2, 3]], 3, b"\0");
    assert_eq!(table(raw, "ni", 8).err().unwrap().id, "dbc.layout");
}
#[test]
fn locale_fills_empty_slots_in_physical_order_without_replacing_nonempty() {
    let mut base = table(bytes(&[&[9, 0, 1]], 3, b"\0base\0"), "nss", 12).unwrap();
    let overlay = table(bytes(&[&[2, 1, 7]], 3, b"\0local\0other\0"), "nss", 12).unwrap();
    let merged = base.record_with_locales(0, &mut [overlay]).unwrap();
    assert_eq!(
        merged,
        json!({"ordinal":0,"key":9,"values":[[0,"n",9],[1,"s","6c6f63616c"],[2,"s","62617365"]]})
    );
    let empty = table(bytes(&[], 3, b"\0"), "nss", 8).unwrap();
    assert_eq!(
        base.record_with_locales(0, &mut [empty]).unwrap_err().id,
        "dbc.locale_record_count"
    );
}
#[test]
fn physical_permutation_changes_record_and_lookup_identity() {
    let a = table(bytes(&[&[10, 5], &[2, 7]], 2, b"\0"), "ni", 8)
        .unwrap()
        .report(None)
        .unwrap();
    let b = table(bytes(&[&[2, 7], &[10, 5]], 2, b"\0"), "ni", 8)
        .unwrap()
        .report(None)
        .unwrap();
    for key in ["physical_sha256", "decoded_sha256", "index_sha256"] {
        assert_ne!(a[key], b[key]);
    }
}
#[test]
fn mandatory_layout_inventory_excludes_spell_and_disabled_calls() {
    assert_eq!(POLICY["loaders"].as_array().unwrap().len(), 61);
    assert_eq!(
        require_loaders(&json!({})).unwrap_err().id,
        "dbc.mandatory_file_missing"
    );
    assert!(layout("Spell.dbc").is_none());
    assert!(layout("EmotesTextSound.dbc").is_none());
    assert!(layout("WorldSafeLocs.dbc").is_none());
    assert_eq!(POLICY["zero_byte_archive"].as_array().unwrap().len(), 4);
    assert_eq!(layout("TaxiPathNode.dbc").unwrap()["format"], "diiifffii");
}
#[test]
fn changed_chunk_and_bad_hash_are_not_rows() {
    use std::{cell::RefCell, rc::Rc};
    let raw = Rc::new(RefCell::new(bytes(&[&[2, 3]], 2, b"\0")));
    let d = Descriptor {
        sha256: json::hash_bytes(&raw.borrow()),
        bytes: raw.borrow().len() as u64,
    };
    let held = raw.clone();
    let mut v = DbcView::new(
        Box::new(move |at, size| Ok(held.borrow()[at as usize..at as usize + size].to_vec())),
        d,
        Some(json!({"format":"ni","record_bytes":8})),
    )
    .unwrap();
    raw.borrow_mut()[24] = 99;
    v.cached = None;
    assert_eq!(v.lookup(2).unwrap_err().id, "catalog.hash");
}
#[test]
fn output_record_limit_refuses_without_clipping() {
    let mut strings = vec![b'a'; 16386];
    strings[0] = 0;
    strings[16385] = 0;
    let mut v = table(bytes(&[&[2, 1, 1, 1, 1]], 5, &strings), "nssss", 20).unwrap();
    assert_eq!(v.report(None).unwrap_err().id, "dbc.record_output_limit");
}
#[test]
fn animation_timeline_overwrites_by_id_iteration_not_physical_order() {
    let mut v = table(
        bytes(
            &[
                &[10, 52, 100, 0, 0, 0, 0],
                &[2, 52, 100, 99, 0, 0, 0],
                &[7, 52, 200, 0, 0, 0, 0],
            ],
            7,
            b"\0",
        ),
        "diifffx",
        28,
    )
    .unwrap();
    let report = animation_timeline(&mut v).unwrap();
    assert_eq!(report["52"]["total_time"], 200);
    assert_eq!(report["52"]["nodes"][0]["ordinal"], 0);
    assert_eq!(report["52"]["nodes"][0]["words"][3], 0);
}
#[test]
fn global_locale_falls_back_to_available_human_race_names() {
    let race = json!({"values":(0..8).map(|i|json!([17+i,"s",if i==2 || i==5 {"78"} else {""}])).collect::<Vec<_>>()});
    assert_eq!(
        locale_selection(Some(race.clone()), 0).unwrap()["fallback_locales"],
        json!([2, 2, 2, 2, 2, 5, 2, 2])
    );
    assert_eq!(
        locale_selection(Some(race.clone()), 5).unwrap()["default_locale"],
        5
    );
    assert_eq!(
        locale_selection(Some(race.clone()), 255).unwrap()["default_locale"],
        2
    );
    assert_eq!(
        locale_selection(Some(race), 8).unwrap()["effective_config_locale"],
        0
    );
    assert_eq!(
        locale_selection(None, 0).unwrap_err().id,
        "dbc.locale_human_missing"
    );
    let empty = json!({"values":(0..8).map(|i|json!([17+i,"s",""])).collect::<Vec<_>>()});
    assert_eq!(
        locale_selection(Some(empty), 0).unwrap_err().id,
        "dbc.locale_undetermined"
    );
}

#[test]
fn persistent_maps_follow_effective_id_order_and_last_physical_duplicate() {
    let v = table(
        bytes(&[&[9, 0, 1], &[2, 0, 0], &[9, 0, 3]], 3, b"\0"),
        "nxi",
        12,
    )
    .unwrap();
    assert_eq!(v.persistent_map_entries().unwrap(), [(2, 0), (9, 3)]);
}
