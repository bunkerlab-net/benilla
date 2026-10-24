// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::near_point::selector::{
    candidate_angle_bits, EmptySelector, Error, DEFAULT_SIZE_BITS, MAX_OFFSETS, PI_BITS,
};
use benilla_sim::source_trig::TAU_BITS;

fn word(text: &str) -> u32 {
    u32::from_str_radix(text.trim_start_matches("0x"), 16).unwrap()
}

#[test]
fn complete_empty_selector_sequences_match_original_cpp_oracle() {
    let mut cases = 0;
    for line in include_str!("../tools/fixtures/source_atan/selector.tsv").lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let cells: Vec<_> = line.split_whitespace().collect();
        let name = cells[0];
        let mut selector = EmptySelector::new(word(cells[1]), word(cells[2])).unwrap();
        assert_eq!(selector.ratio_bits(), word(cells[3]), "ratio {name}");
        assert_eq!(selector.half_angle_bits(), word(cells[4]), "half {name}");
        let expected_count: usize = cells[7].parse().unwrap();
        assert_eq!(cells.len(), 8 + expected_count, "schema {name}");
        let actual: Vec<_> = selector.by_ref().take(MAX_OFFSETS + 1).collect();
        let expected: Vec<_> = cells[8..].iter().map(|v| word(v)).collect();
        assert_eq!(actual, expected, "offsets {name}");
        assert!(actual.len() <= MAX_OFFSETS, "bound {name}");
        let final_steps = [word(cells[5]), word(cells[6])];
        assert_eq!(selector.step_bits(), final_steps, "terminal {name}");
        for _ in 0..3 {
            assert_eq!(selector.next(), None, "fused {name}");
            assert_eq!(selector.step_bits(), final_steps, "stable {name}");
        }
        cases += 1;
    }
    assert!(cases >= 20);
}

#[test]
fn zero_repairs_are_selector_only_and_both_signed_zeros_are_equal() {
    let tiny = EmptySelector::new(1, 0x46ff_f000).unwrap();
    assert_eq!(tiny.ratio_bits(), 0);
    assert_eq!(tiny.half_angle_bits(), 0);
    assert_eq!(tiny.step_bits(), [0, 0]);
    let expected = EmptySelector::new(DEFAULT_SIZE_BITS, DEFAULT_SIZE_BITS)
        .unwrap()
        .collect::<Vec<_>>();
    for size in [0, 0x8000_0000, DEFAULT_SIZE_BITS] {
        for distance in [0, 0x8000_0000, DEFAULT_SIZE_BITS] {
            assert_eq!(
                EmptySelector::new(size, distance)
                    .unwrap()
                    .collect::<Vec<_>>(),
                expected
            );
        }
    }
    for invalid in [0x7f80_0000, 0xff80_0000, 0x7fc0_0000] {
        assert!(matches!(
            EmptySelector::new(invalid, 1),
            Err(Error::NonFinite)
        ));
        assert!(matches!(
            EmptySelector::new(1, invalid),
            Err(Error::NonFinite)
        ));
    }
    for negative in [0x8000_0001, 0xbf80_0000, 0xff7f_ffff] {
        assert!(matches!(
            EmptySelector::new(negative, 1),
            Err(Error::Negative)
        ));
        assert!(matches!(
            EmptySelector::new(1, negative),
            Err(Error::Negative)
        ));
    }
}

#[test]
fn raw_base_plus_signed_offset_is_not_normalized() {
    assert_eq!(
        candidate_angle_bits(0, PI_BITS | 0x8000_0000),
        Ok(0xc049_0fdb)
    );
    assert_eq!(candidate_angle_bits(TAU_BITS, PI_BITS), Ok(0x4116_cbe4));
    assert_eq!(candidate_angle_bits(TAU_BITS, 0), Ok(TAU_BITS));
    assert_eq!(
        candidate_angle_bits(0x8000_0000, 0x8000_0000),
        Ok(0x8000_0000)
    );
    assert_eq!(candidate_angle_bits(TAU_BITS + 1, 0), Err(Error::BaseAngle));
    assert_eq!(candidate_angle_bits(0x8000_0001, 0), Err(Error::BaseAngle));
    for offset in [PI_BITS + 1, (PI_BITS + 1) | 0x8000_0000] {
        assert_eq!(candidate_angle_bits(0, offset), Err(Error::Offset));
    }
    assert_eq!(candidate_angle_bits(0x7f80_0000, 0), Err(Error::NonFinite));
}

#[test]
fn source_exact_pi_is_yielded_and_double_literal_rounding_is_retained() {
    // Found through the unchanged original C++ constructor/iterator, not by
    // overwriting half-angle state or deriving expectations from this iterator.
    let mut exact_pi = EmptySelector::new(0x4214_2315, 0x3f80_0000).unwrap();
    assert_eq!(exact_pi.half_angle_bits(), 0x3fc7_2456);
    let offsets: Vec<_> = exact_pi.by_ref().collect();
    assert_eq!(offsets.len(), 6);
    assert_eq!(&offsets[4..], &[PI_BITS, PI_BITS | 0x8000_0000]);
    assert_eq!(exact_pi.step_bits(), [PI_BITS; 2]);
    assert_eq!(exact_pi.next(), None);
    let mut ones = EmptySelector::new(0x3f80_0000, 0x3f80_0000).unwrap();
    // Using 0.01f instead of the source double literal gives 0x3c23d700.
    assert_eq!(ones.next(), Some(0x3c23_d70a));
}

#[test]
fn empty_selector_matches_independent_sample_sequence_digest() {
    use sha2::{Digest, Sha256};
    let row = include_str!("../tools/fixtures/source_atan/sample-digests.tsv")
        .lines()
        .find(|line| line.starts_with("selector "))
        .unwrap();
    let cells: Vec<_> = row.split_whitespace().collect();
    let count: usize = cells[1].parse().unwrap();
    let mut state = word(cells[2]);
    assert_eq!(count, 4096);
    assert_eq!(state, 0xa7a6_4105);
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state % 0x7f80_0000
    };
    let mut hash = Sha256::new();
    for _ in 0..count {
        let (size, distance) = (next(), next());
        let mut selector = EmptySelector::new(size, distance).unwrap();
        let angles: Vec<_> = selector.by_ref().take(MAX_OFFSETS + 1).collect();
        assert!(angles.len() <= MAX_OFFSETS);
        assert_eq!(selector.next(), None);
        let [plus, minus] = selector.step_bits();
        for word in [
            size,
            distance,
            selector.ratio_bits(),
            selector.half_angle_bits(),
            plus,
            minus,
            angles.len() as u32,
        ]
        .into_iter()
        .chain(angles)
        {
            hash.update(word.to_le_bytes());
        }
    }
    assert_eq!(format!("{:x}", hash.finalize()), cells[3]);
}
