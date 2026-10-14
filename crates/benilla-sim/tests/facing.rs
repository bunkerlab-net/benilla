// SPDX-License-Identifier: GPL-3.0-or-later
// Numeric expected values come exclusively from pinned source bodies; no host trig.
use benilla_sim::facing::{atan2_bits, bearing_bits, has_in_arc_bits, normalize_bits};
use sha2::{Digest, Sha256};

const SIGN: u32 = 0x8000_0000;
const COORD: u32 = 0x46ff_f000;
const ANGLE: u32 = 0x41c9_0fdb;

fn evaluate(kind: &str, values: &[u32]) -> u32 {
    match kind {
        "atan2" => atan2_bits(values[0], values[1]).unwrap(),
        "normalize" => normalize_bits(values[0]).unwrap(),
        "bearing" => bearing_bits([values[0], values[1]], [values[2], values[3]]).unwrap(),
        "arc" => u32::from(
            has_in_arc_bits(
                [values[0], values[1]],
                values[2],
                [values[3], values[4]],
                values[5],
                values[6] != 0,
            )
            .unwrap(),
        ),
        _ => panic!("unknown oracle operation: {kind}"),
    }
}

fn hex(word: &str) -> u32 {
    u32::from_str_radix(word, 16).unwrap()
}

#[test]
fn pinned_source_binary32_oracle() {
    let mut count = 0;
    let mut rare_fallbacks = 0;
    for line in include_str!("../tools/fixtures/facing/oracle.tsv").lines() {
        if line.starts_with('#') {
            continue;
        }
        let words: Vec<_> = line.split_whitespace().collect();
        let name = words[0];
        let kind = words[1];
        let values: Vec<_> = words[2..words.len() - 2].iter().map(|s| hex(s)).collect();
        let expected = hex(words[words.len() - 2]);
        assert_eq!(evaluate(kind, &values), expected, "{name}: {values:x?}");
        if name.starts_with("rare_fallback_") {
            assert_eq!(words[words.len() - 1], "1", "source rare path: {name}");
            rare_fallbacks += 1;
        }
        count += 1;
    }
    assert_eq!(count, 3351);
    assert_eq!(rare_fallbacks, 8);
}

struct XorShift32(u32);
impl XorShift32 {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    fn bounded(&mut self, maximum: u32) -> u32 {
        let word = self.next();
        (word & SIGN) | ((word & 0x7fff_ffff) % (maximum + 1))
    }
    fn input(&mut self, kind: &str) -> Vec<u32> {
        match kind {
            "atan2" => vec![self.bounded(0x7f7f_ffff), self.bounded(0x7f7f_ffff)],
            "normalize" => vec![self.bounded(ANGLE)],
            "bearing" => (0..4).map(|_| self.bounded(COORD)).collect(),
            "arc" => {
                let mut values = [COORD, COORD, ANGLE, COORD, COORD, ANGLE]
                    .into_iter()
                    .map(|bound| self.bounded(bound))
                    .collect::<Vec<_>>();
                values.push(self.next() & 1);
                values
            }
            _ => panic!("unknown sample operation: {kind}"),
        }
    }
}

#[test]
fn large_deterministic_source_sample_digests() {
    let mut total = 0;
    for line in include_str!("../tools/fixtures/facing/sample-digests.tsv").lines() {
        if line.starts_with('#') {
            continue;
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        let kind = fields[0];
        let count = fields[1].parse::<usize>().unwrap();
        let mut rng = XorShift32(hex(fields[2]));
        let mut hash = Sha256::new();
        for _ in 0..count {
            let values = rng.input(kind);
            let answer = evaluate(kind, &values);
            for word in values.into_iter().chain([answer]) {
                hash.update(word.to_le_bytes());
            }
        }
        assert_eq!(
            format!("{:x}", hash.finalize()),
            fields[3],
            "{kind} samples"
        );
        total += count;
    }
    assert_eq!(total, 155_648);
}

#[test]
fn nonfinite_inputs_fail_closed_even_for_self_arc() {
    for invalid in [
        0x7f80_0000,
        0xff80_0000,
        0x7fc0_0000,
        0x7f80_0001,
        0xffc0_1234,
    ] {
        assert!(atan2_bits(invalid, 0).is_err());
        assert!(atan2_bits(0, invalid).is_err());
        assert!(normalize_bits(invalid).is_err());
        for axis in 0..4 {
            let mut points = [0; 4];
            points[axis] = invalid;
            assert!(bearing_bits([points[0], points[1]], [points[2], points[3]]).is_err());
        }
        for field in 0..6 {
            let mut values = [0; 6];
            values[field] = invalid;
            for same in [false, true] {
                assert!(has_in_arc_bits(
                    [values[0], values[1]],
                    values[2],
                    [values[3], values[4]],
                    values[5],
                    same,
                )
                .is_err());
            }
        }
    }
}

#[test]
fn finite_domain_bounds_are_inclusive_and_reject_adjacent_values() {
    for sign in [0, SIGN] {
        assert!(normalize_bits(ANGLE | sign).is_ok());
        assert!(bearing_bits([COORD | sign, 0], [0, COORD | sign]).is_ok());
        assert!(
            has_in_arc_bits([COORD | sign, 0], ANGLE | sign, [0, 0], ANGLE | sign, true).unwrap()
        );
        for invalid in [ANGLE + 1, 0x7f7f_ffff] {
            let invalid = invalid | sign;
            assert!(normalize_bits(invalid).is_err());
            for same in [false, true] {
                assert!(has_in_arc_bits([0, 0], invalid, [0, 0], 0, same).is_err());
                assert!(has_in_arc_bits([0, 0], 0, [0, 0], invalid, same).is_err());
            }
        }
        for invalid in [COORD + 1, 0x7f7f_ffff] {
            let invalid = invalid | sign;
            for axis in 0..4 {
                let mut points = [0; 4];
                points[axis] = invalid;
                let origin = [points[0], points[1]];
                let target = [points[2], points[3]];
                assert!(bearing_bits(origin, target).is_err());
                for same in [false, true] {
                    assert!(has_in_arc_bits(origin, 0, target, 0, same).is_err());
                }
            }
        }
    }
}

#[test]
fn same_object_shortcut_is_identity_not_coordinate_equality() {
    // The expected distinction is present in the source oracle's coincident rows.
    let same = has_in_arc_bits([0, 0], 0x4049_0fdb, [0, 0], 0, true).unwrap();
    let coincident = has_in_arc_bits([0, 0], 0x4049_0fdb, [0, 0], 0, false).unwrap();
    assert!(same);
    assert!(!coincident);
}
