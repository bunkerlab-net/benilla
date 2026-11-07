// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic source lookup results, not extracted client rows or lookup observations.
//! This checks prerequisite reduction only; full spell/holder admission stays separate.
use super::*;

#[test]
fn extracted_numeric_lookup_vectors_match_only_the_bounded_preparation_domain() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/starter_numeric_cmangos.tsv"
    ));
    let mut accepted = [0; 2];
    let mut rejected = [0; 2];
    let mut source_only = 0;
    for line in fixture.lines().filter(|line| !line.starts_with('#')) {
        let f: Vec<_> = line.split('\t').collect();
        assert_eq!(f.len(), 14, "{line}");
        if f[2] == "source_only" {
            source_only += 1;
            continue;
        }
        assert!(matches!(f[2], "admitted" | "rejected"), "{line}");
        let present: u32 = f[3].parse().unwrap();
        assert!(present <= 1);
        let query: u32 = f[4].parse().unwrap();
        let id: u32 = f[5].parse().unwrap();
        let base: i32 = f[6].parse().unwrap();
        let per_level: i32 = f[7].parse().unwrap();
        let bound: i32 = f[8].parse().unwrap();
        let source_result: i32 = f[11].parse().unwrap();
        let source_max: i32 = f[12].parse().unwrap();
        let mut row = authored(81).row;
        row.duration_ms = Some(0);
        let mut dependency = dependencies(&input(&[81]), &[81]);
        let dependency = &mut rows(&mut dependency)[0];
        dependency.duration = InitialSpellLookup::Missing {
            index: row.words[30],
        };
        let (slot, admitted) = match f[1] {
            "C" => {
                row.words[18] = query;
                dependency.cast_time = if present == 0 {
                    InitialSpellLookup::Missing { index: id }
                } else {
                    InitialSpellLookup::Found(SpellCastTimeSource {
                        id,
                        base_ms: base,
                        per_level_ms: per_level,
                        minimum_ms: bound,
                    })
                };
                (
                    0,
                    id == query && (present == 0 || (base, per_level, bound) == (0, 0, 0)),
                )
            }
            "D" => {
                row.words[30] = query;
                row.duration_ms = Some(source_result);
                dependency.duration = if present == 0 {
                    InitialSpellLookup::Missing { index: id }
                } else {
                    InitialSpellLookup::Found(SpellDurationSource {
                        id,
                        base_ms: base,
                        per_level_ms: per_level,
                        maximum_ms: bound,
                    })
                };
                (
                    1,
                    id == query
                        && (present == 0
                            || (per_level == 0 && matches!(base, -1 | 0) && base == bound)),
                )
            }
            _ => panic!("unknown fixture kind: {line}"),
        };
        dependency.source_hash = row.source_hash();
        assert_eq!(admitted, f[2] == "admitted", "fixture domain label: {line}");
        let result = dependency.resolutions(&row);
        if admitted {
            let result = result.unwrap_or_else(|error| panic!("{line}: {error}"));
            if slot == 0 {
                assert_eq!(result.cast_time_index, query);
                assert_eq!(result.cast_time_ms, source_result, "{line}");
            } else {
                assert_eq!(result.duration_index, query);
                assert_eq!(result.duration_ms, source_result, "{line}");
                assert_eq!(source_result, source_max, "{line}");
            }
            accepted[slot] += 1;
            let original_identity = dependency.identity();
            if slot == 0 {
                dependency.cast_time = InitialSpellLookup::Unknown;
            } else {
                dependency.duration = InitialSpellLookup::Unknown;
            }
            assert_ne!(dependency.identity(), original_identity);
            assert!(
                dependency.resolutions(&row).is_err(),
                "unknown lookup: {line}"
            );
        } else {
            assert_eq!(
                result.err(),
                Some(InitError::Invalid(if slot == 0 {
                    "initial indexed cast-time resolution"
                } else {
                    "initial indexed duration resolution"
                })),
                "{line}"
            );
            rejected[slot] += 1;
        }
    }
    assert_eq!(accepted, [6, 6]);
    assert_eq!(rejected, [14, 9]);
    assert_eq!(source_only, 27);
}
