// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn frame(ordinal: u64, value: u8) -> Vec<u8> {
    let mut raw = vec![0; 16];
    raw[4..12].copy_from_slice(&ordinal.to_le_bytes());
    raw[12..14].copy_from_slice(&2u16.to_le_bytes());
    raw[14..16].copy_from_slice(&1u16.to_le_bytes());
    raw.push(0);
    for v in [value, b'2'] {
        raw.extend_from_slice(&1u32.to_le_bytes());
        raw.push(v);
    }
    let len = raw.len() as u32;
    raw[..4].copy_from_slice(&len.to_le_bytes());
    raw
}
fn run(raw: &[u8], expected: &[[u8; 32]]) -> Result<Value> {
    let path = std::env::temp_dir().join(format!(
        "benilla-group-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    std::io::Write::write_all(&mut f, raw).unwrap();
    drop(f);
    let descriptor =
        json!({"bytes":raw.len(),"rows":expected.len(),"sha256":hex(&Sha256::digest(raw))});
    let result = verify_stream(&path, 2, expected, &descriptor);
    std::fs::remove_file(path).unwrap();
    result
}
#[test]
fn exact_occurrences_refuse_rehashed_forgery_trailing_exhaustion_and_bad_ordinals() {
    let expected = hash_cells([Some(b"1".as_slice()), Some(b"2".as_slice())]);
    assert_eq!(run(&frame(0, b'1'), &[expected]).unwrap()["rows"], 1);
    assert_eq!(run(&[], &[expected]).unwrap_err().id, "groups.exhausted");
    assert_eq!(
        run(&frame(0, b'9'), &[expected]).unwrap_err().id,
        "groups.view_mismatch"
    );
    assert_eq!(
        run(&frame(4, b'1'), &[expected]).unwrap_err().id,
        "catalog.duplicate_row"
    );
    let extra = [frame(0, b'1'), vec![0]].concat();
    assert_eq!(run(&extra, &[expected]).unwrap_err().id, "groups.trailing");
}
#[test]
fn permutation_changes_occurrence_identity_not_catalog_membership() {
    let mut expected = [
        hash_cells([Some(b"1".as_slice()), Some(b"2".as_slice())]),
        hash_cells([Some(b"3".as_slice()), Some(b"2".as_slice())]),
    ];
    expected.sort_unstable();
    let a = run(&[frame(0, b'1'), frame(1, b'3')].concat(), &expected).unwrap();
    let b = run(&[frame(0, b'3'), frame(1, b'1')].concat(), &expected).unwrap();
    assert_eq!(a["multiset_sha256"], b["multiset_sha256"]);
    assert_ne!(a["occurrence_sha256"], b["occurrence_sha256"]);
}
