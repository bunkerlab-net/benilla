// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
#[test]
fn source_float_conversion_keeps_signed_zero_and_double_rounding() {
    assert_eq!(floating(Some(b"-0")).unwrap().to_bits(), 0x8000_0000);
    assert_eq!(floating(None).unwrap().to_bits(), 0);
    assert_eq!(
        floating(Some(b"1.0000000596046448")).unwrap().to_bits(),
        0x3f80_0000
    );
    assert_eq!(floating(Some(b" 1.25tail")).unwrap(), 1.25);
    assert_eq!(floating(Some(b".5e-2tail")).unwrap(), 0.005);
    assert_eq!(uint(Some(b"-1")).unwrap(), u32::MAX);
    assert_eq!(uint(None).unwrap(), 0);
    assert_eq!(uint(Some(b"4294967296")).unwrap(), 0);
    assert_eq!(coordinates(0.0, -0.0).unwrap(), (2080, 524800));
    assert_eq!(
        coordinates(100000.0, 0.0).unwrap_err().id,
        "spawn.invalid_coordinates"
    );
    assert_eq!(
        coordinates(f32::NAN, 0.0).unwrap_err().id,
        "spawn.invalid_coordinates"
    );
    assert_eq!(
        floating(Some(b"0x1p1")).unwrap_err().id,
        "spawn.numeric_unsupported"
    );
    assert_eq!(
        floating(Some(&[b'1'; 128])).unwrap_err().id,
        "spawn.numeric_limit"
    );
}
fn frame(ordinal: u64, first: u8, null: bool) -> Vec<u8> {
    let mut frame = vec![0; 16];
    frame[4..12].copy_from_slice(&ordinal.to_le_bytes());
    frame[12..14].copy_from_slice(&2u16.to_le_bytes());
    frame[14..16].copy_from_slice(&1u16.to_le_bytes());
    frame.push(if null { 2 } else { 0 });
    frame.extend_from_slice(&1u32.to_le_bytes());
    frame.push(first);
    if !null {
        frame.extend_from_slice(&1u32.to_le_bytes());
        frame.push(b'2');
    }
    let len = frame.len() as u32;
    frame[..4].copy_from_slice(&len.to_le_bytes());
    frame
}
fn run(raw: &[u8], expected: &[[u8; 32]], descriptor: &Value) -> Result<Value> {
    let path = std::env::temp_dir().join(format!(
        "benilla-spawn-stream-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    std::io::Write::write_all(&mut file, raw).unwrap();
    drop(file);
    let held = reader::Held::open(&path, MAX_RAW).unwrap();
    let result = verify_stream(&held, 2, expected, descriptor, &mut vec![0; FRAME]);
    std::fs::remove_file(&path).unwrap();
    result
}
fn descriptor_for(raw: &[u8], rows: usize) -> Value {
    json!({"sha256":hex(&Sha256::digest(raw)),"bytes":raw.len(),"rows":rows})
}
#[test]
fn occurrence_order_duplicates_and_null_extension_stay_distinct() {
    let first: [u8; 32] = Sha256::digest(b"\x01\0\0\x007\xff\xff\xff\xff").into();
    let second: [u8; 32] = Sha256::digest(b"\x01\0\0\x007\x01\0\0\x002").into();
    let mut expected = [first, first, second];
    expected.sort_unstable();
    let raw = [
        frame(0, b'7', true),
        frame(1, b'7', false),
        frame(2, b'7', true),
    ]
    .concat();
    let reordered = [
        frame(0, b'7', true),
        frame(1, b'7', true),
        frame(2, b'7', false),
    ]
    .concat();
    let a = run(&raw, &expected, &descriptor_for(&raw, 3)).unwrap();
    let b = run(&reordered, &expected, &descriptor_for(&reordered, 3)).unwrap();
    assert_eq!(a["rows"], 3);
    assert_eq!(a["multiset_sha256"], b["multiset_sha256"]);
    assert_ne!(a["occurrence_sha256"], b["occurrence_sha256"]);
}
#[test]
fn joined_stream_refuses_forgery_exhaustion_trailing_and_duplicate_ordinals() {
    let expected: [u8; 32] = Sha256::digest(b"\x01\0\0\x007\xff\xff\xff\xff").into();
    let raw = frame(0, b'7', true);
    let d = descriptor_for(&raw, 1);
    assert_eq!(
        run(&raw[..3], &[expected], &d).unwrap_err().id,
        "spawn.exhausted"
    );
    assert_eq!(
        run(&raw[..raw.len() - 1], &[expected], &d).unwrap_err().id,
        "catalog.truncated"
    );
    let mut extra = raw.clone();
    extra.push(0);
    assert_eq!(
        run(&extra, &[expected], &d).unwrap_err().id,
        "spawn.trailing"
    );
    let mut bad = d.clone();
    bad["sha256"] = json!("0".repeat(64));
    assert_eq!(run(&raw, &[expected], &bad).unwrap_err().id, "spawn.hash");
    let forged = frame(0, b'8', true);
    assert_eq!(
        run(&forged, &[expected], &descriptor_for(&forged, 1))
            .unwrap_err()
            .id,
        "spawn.join_mismatch"
    );
    let duplicate = [raw.clone(), raw].concat();
    assert_eq!(
        run(
            &duplicate,
            &[expected, expected],
            &descriptor_for(&duplicate, 2)
        )
        .unwrap_err()
        .id,
        "catalog.duplicate_row"
    );
}
