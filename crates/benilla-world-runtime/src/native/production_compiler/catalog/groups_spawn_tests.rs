// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn cursor(phase: GroupPhase, cells: &[Option<&[u8]>]) -> (Directory, SpawnGroupCatalogRows) {
    let dir = Directory(std::env::temp_dir().join(format!(
        "benilla-group-cursor-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::create_dir(&dir.0).unwrap();
    let bitmap = cells.len().div_ceil(8);
    let mut raw = vec![0; 16 + bitmap];
    raw[12..14].copy_from_slice(&(cells.len() as u16).to_le_bytes());
    raw[14..16].copy_from_slice(&(bitmap as u16).to_le_bytes());
    for (index, value) in cells.iter().enumerate() {
        if let Some(value) = value {
            raw.extend_from_slice(&(value.len() as u32).to_le_bytes());
            raw.extend_from_slice(value);
        } else {
            raw[16 + index / 8] |= 1 << (index % 8);
        }
    }
    let len = raw.len() as u32;
    raw[..4].copy_from_slice(&len.to_le_bytes());
    let mut streams = Vec::new();
    let mut rows = [0; 6];
    for (slot, count) in rows.iter_mut().enumerate() {
        let path = dir.0.join(format!("{slot}.rows"));
        std::fs::write(
            &path,
            if slot == phase as usize {
                raw.as_slice()
            } else {
                &[]
            },
        )
        .unwrap();
        *count = u64::from(slot == phase as usize);
        streams.push(reader::Held::open(&path, MAX_RAW).unwrap());
    }
    let path = dir.0.join("manifest.json");
    std::fs::write(&path, b"{}").unwrap();
    let cursor = SpawnGroupCatalogRows {
        metadata: reader::Held::open(&path, MAX_RAW).unwrap(),
        streams: streams.try_into().ok().unwrap(),
        rows,
        at: [0; 6],
        ordinal: [0; 6],
        buffer: vec![0; FRAME],
    };
    (dir, cursor)
}
#[test]
fn typed_raw_cursor_preserves_unsigned_signed_nullable_and_float_storage_conversions() {
    // Field.h:53-62; ObjectMgr.cpp:1132-1187,1199-1206,1259-1265.
    let (_dir, mut rows) = cursor(
        GroupPhase::Templates,
        &[
            Some(b"1"),
            Some(b"authored"),
            Some(b"0"),
            Some(b"2"),
            Some(b"-2"),
            Some(b"-3"),
            Some(b"4"),
            Some(b"5"),
            Some(b"0"),
            None,
        ],
    );
    let GroupInputData::Template(row) = rows.next(GroupPhase::Templates).unwrap().unwrap().data
    else {
        panic!()
    };
    assert_eq!((row.id, row.condition, row.expression), (1, -2, -3));
    assert_eq!(row.respawn_min, Some(0));
    assert_eq!(row.respawn_max, None);
    assert!(rows.next(GroupPhase::Templates).unwrap().is_none());
    drop(rows);
    let (_dir, mut rows) = cursor(
        GroupPhase::Formations,
        &[
            Some(b"1"),
            Some(b"2"),
            Some(b"1.25"),
            Some(b"3"),
            Some(b"4"),
            Some(b"0"),
            None,
        ],
    );
    let GroupInputData::Formation(row) = rows.next(GroupPhase::Formations).unwrap().unwrap().data
    else {
        panic!()
    };
    assert_eq!(row.spread_bits, 1.25_f32.to_bits());
    assert!(row.comment.is_empty());
    drop(rows);
    let (_dir, mut rows) = cursor(
        GroupPhase::Members,
        &[Some(b"1"), Some(b"7"), Some(b"-1"), Some(b"50")],
    );
    assert!(matches!(
        rows.next(GroupPhase::Members).unwrap().unwrap().data,
        GroupInputData::Member {
            group: 1,
            guid: 7,
            slot: -1,
            chance: 50
        }
    ));
    drop(rows);
    let (_dir, mut rows) = cursor(
        GroupPhase::Entries,
        &[
            Some(b"1"),
            Some(b"9"),
            None,
            Some(b"0"),
            Some(b"-2147483648"),
        ],
    );
    let GroupInputData::Entry { group, entry } =
        rows.next(GroupPhase::Entries).unwrap().unwrap().data
    else {
        panic!()
    };
    assert_eq!(group, 1);
    assert_eq!(
        entry,
        RandomEntry {
            entry: 9,
            min: 0,
            max: 0,
            chance: 0x8000_0000
        }
    );
    drop(rows);
    let (_dir, mut rows) = cursor(GroupPhase::Links, &[Some(b"1"), Some(b"2")]);
    assert!(matches!(
        rows.next(GroupPhase::Links).unwrap().unwrap().data,
        GroupInputData::Link {
            group: 1,
            linked: 2
        }
    ));
    drop(rows);
    let (_dir, mut rows) = cursor(
        GroupPhase::Squads,
        &[Some(b"1"), Some(b"2"), Some(b"7"), Some(b"9")],
    );
    assert!(matches!(
        rows.next(GroupPhase::Squads).unwrap().unwrap().data,
        GroupInputData::Squad {
            group: 1,
            squad: 2,
            guid: 7,
            entry: 9
        }
    ));
    drop(rows);
}
#[test]
fn cursor_rejects_consuming_a_later_query_before_an_earlier_query_finishes() {
    let (_dir, mut rows) = cursor(
        GroupPhase::Members,
        &[Some(b"1"), Some(b"7"), Some(b"-1"), Some(b"0")],
    );
    assert_eq!(
        rows.next(GroupPhase::Squads).unwrap_err().id,
        "groups.loader_phase"
    );
    drop(rows);
}
#[test]
fn retained_cursor_consumes_every_checked_raw_group_occurrence_or_reports_explicit_absence() {
    let Some(root) = std::env::var_os("BENILLA_F2_CATALOG") else {
        println!("SKIP private raw group cursor: set BENILLA_F2_CATALOG");
        return;
    };
    let Some(occurrences) = std::env::var_os("BENILLA_F2_GROUP_OCCURRENCES") else {
        println!("SKIP private raw group cursor: set BENILLA_F2_GROUP_OCCURRENCES");
        return;
    };
    let mut catalog = VerifiedCatalog::open(Path::new(&root), false).unwrap();
    let mut rows = SpawnGroupCatalogRows::open(&mut catalog, Path::new(&occurrences)).unwrap();
    let expected = rows.rows;
    let mut seen = [0; 6];
    for phase in GroupPhase::ALL {
        while let Some(row) = rows.next(phase).unwrap() {
            assert_eq!(row.ordinal, seen[phase as usize]);
            seen[phase as usize] += 1;
        }
    }
    assert_eq!(seen, expected);
    assert!(seen.iter().sum::<u64>() > 0);
}

#[test]
fn c_string_fields_stop_at_nul_without_trimming_or_utf8_replacement() {
    // Field.h:49-51 constructs std::string from the nullable C string.
    for (bytes, expected) in [
        (b" \xff\0ignored".as_slice(), b" \xff".as_slice()),
        (b"\0ignored".as_slice(), b"".as_slice()),
    ] {
        let (_dir, mut rows) = cursor(
            GroupPhase::Templates,
            &[
                Some(b"1"),
                Some(bytes),
                Some(b"0"),
                Some(b"1"),
                None,
                None,
                None,
                None,
                None,
                None,
            ],
        );
        let GroupInputData::Template(row) = rows.next(GroupPhase::Templates).unwrap().unwrap().data
        else {
            panic!()
        };
        assert_eq!(row.name, expected);
        drop(rows);
        let (_dir, mut rows) = cursor(
            GroupPhase::Formations,
            &[
                Some(b"1"),
                Some(b"0"),
                Some(b"0"),
                None,
                None,
                None,
                Some(bytes),
            ],
        );
        let GroupInputData::Formation(row) =
            rows.next(GroupPhase::Formations).unwrap().unwrap().data
        else {
            panic!()
        };
        assert_eq!(row.comment, expected);
        drop(rows);
    }
}
