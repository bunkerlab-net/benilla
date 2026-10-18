//! Authored MPQ closure tests; no player assets.
use super::*;

fn liquid(height: f32) -> Vec<u8> {
    let mut bytes = vec![0; 30];
    for (at, n) in [(0, 2), (4, 2), (8, 1), (12, 1)] {
        set_word(&mut bytes, at, n);
    }
    for _ in 0..4 {
        bytes.extend([0; 4]);
        bytes.extend(height.to_le_bytes());
    }
    bytes.push(0);
    bytes
}

fn with_liquid(height: f32, reset: bool) -> Vec<Entry> {
    let mut entries = wmo_entries(1);
    let group = entries.last_mut().unwrap();
    group.bytes.extend(chunk(b"MLIQ", &liquid(height)));
    if reset {
        group.bytes.extend(chunk(b"MODR", &[]));
    }
    group.declared = group.bytes.len() as u32;
    entries
}

#[test]
fn source_bound_liquid_is_private_complete_and_rechecked() {
    let temp = Temp::new();
    let data = temp.data();
    let entries = with_liquid(7.0, false);
    save(&data, "wmo.MPQ", &entries);
    let prepared = prepare(&data, 0, || false).unwrap();
    assert_eq!(prepared.summary().wmo_liquid_groups, 1);
    assert_eq!(
        prepared.summary().liquid_header_policy,
        WMO_LIQUID_HEADER_ZERO_PADDING_V1
    );
    let raw = prepared.products()[0].raw_vmap().unwrap();
    let at = raw.windows(4).position(|b| b == b"LIQU").unwrap();
    // 32-byte ABI header + four 8-byte source vertices + one tile flag.
    assert_eq!(word(raw, at + 4), 65);
    // Actual payload contains only four height floats, preserving the source quirk.
    assert_eq!(raw.len() - at - 8, 49);
    assert_eq!(&raw[at + 8 + 30..at + 8 + 32], &[0, 0]);
    assert!(prepared.verify_unchanged(|| false).is_ok());
    let epoch = prepared.source_epoch();
    save(&data, "wmo.MPQ", &with_liquid(8.0, false));
    assert!(prepared.verify_unchanged(|| false).is_err());
    assert_ne!(prepare(&data, 0, || false).unwrap().source_epoch(), epoch);
}

#[test]
fn later_physical_chunk_resets_liquid_but_capture_keeps_its_identity() {
    let temp = Temp::new();
    let data = temp.data();
    save(&data, "wmo.MPQ", &with_liquid(7.0, true));
    let prepared = prepare(&data, 0, || false).unwrap();
    assert_eq!(prepared.summary().wmo_liquid_groups, 0);
    assert!(!prepared.products()[0]
        .raw_vmap()
        .unwrap()
        .windows(4)
        .any(|b| b == b"LIQU"));
    save(&data, "wmo.MPQ", &with_liquid(8.0, true));
    assert!(prepared.verify_unchanged(|| false).is_err());
}

#[test]
fn invalid_reset_liquid_is_rejected_before_any_product_can_escape() {
    let temp = Temp::new();
    let data = temp.data();
    save(&data, "wmo.MPQ", &with_liquid(f32::NAN, true));
    let error = prepare(&data, 0, || false).unwrap_err();
    assert!(matches!(
        error.category(),
        Error::Wmo(benilla_native_vmap::WmoError::NonFiniteLiquidHeight { .. })
    ));
    assert!(!format!("{error:?}").contains("Temple"));
}

#[test]
fn type_override_uses_normalized_basename_but_unchanged_directory_spelling() {
    for (original, lookup, group, expected) in [
        (
            "World\\Stratholme_raid.wmo",
            "World\\Stratholme_Raid.wmo",
            "World\\Stratholme_Raid_000.wmo",
            4u16,
        ),
        (
            "World\\Stratholme_raid\\room.wmo",
            "World\\Stratholme_raid\\Room.wmo",
            "World\\Stratholme_raid\\Room_000.wmo",
            21,
        ),
    ] {
        let temp = Temp::new();
        let data = temp.data();
        let mut entries = prerequisites();
        entries.push(raw_entry(
            "(listfile)",
            format!("{original}\r\n\0").into_bytes(),
        ));
        let mut root = wmo_root(1);
        set_word(&mut root, 20 + 60, 4); // MOHD root flags.
        entries.push(raw_entry(lookup, root));
        let mut bytes = wmo_group(false);
        set_word(&mut bytes, 20 + 52, 4); // MOGP group liquid type.
        bytes.extend(chunk(b"MLIQ", &liquid(7.0)));
        entries.push(raw_entry(group, bytes));
        save(&data, "wmo.MPQ", &entries);
        let prepared = prepare(&data, 0, || false).unwrap();
        let raw = prepared.products()[0].raw_vmap().unwrap();
        let at = raw.windows(4).position(|b| b == b"LIQU").unwrap();
        assert_eq!(&raw[at + 8 + 28..at + 8 + 30], expected.to_le_bytes());
        assert_eq!(prepared.summary().wmo_liquid_groups, 1);
    }
}
