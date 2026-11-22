// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::xp::*;

#[test]
fn pinned_cpp_oracle_all_classic_player_and_boss_pairs() {
    let mut differences = Vec::new();
    for line in include_str!("fixtures/xp_cmangos.tsv").lines().skip(1) {
        let values: Vec<u32> = line
            .split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect();
        let unit = Level::new(values[0] as u8).unwrap();
        let mob = Level::new(values[1] as u8).unwrap();
        let actual = [
            is_trivial(unit, mob) as u32,
            quest_green_range(unit),
            gray_level(unit),
            zero_difference(unit),
            color_code(unit, mob) as u32,
        ];
        assert_eq!(actual, values[2..7], "unit {unit:?} mob {mob:?}");
        let ratio = base_gain(unit, mob);
        // Floats are confined to this oracle comparison, not authoritative code.
        let upstream = f32::from_bits(values[7]) as f64;
        let exact = ratio.numerator() as f64 / ratio.denominator() as f64;
        assert!(
            (exact - upstream).abs() < 0.0001,
            "unit {unit:?} mob {mob:?}"
        );
        let rounded = ratio.round_ties_even();
        if rounded != values[8] {
            differences.push(format!(
                "{}\t{}\t{}\t{}",
                unit.get(),
                mob.get(),
                rounded,
                values[8]
            ));
        }
    }
    let expected: Vec<&str> = include_str!("fixtures/rounding_differences.tsv")
        .lines()
        .skip(1)
        .collect();
    assert_eq!(
        differences, expected,
        "Unreviewed CMaNGOS rounding deviation"
    );
}
