// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::{
    classic_spell::SpellFloat,
    creature_capabilities::{InhabitType, MOVEFLAG_SWIMMING},
    creature_swimming::{
        initialize_movement_flags, is_swimmable, project_unit_flags, LIQUID_COLLISION_HEIGHT_BITS,
        LIQUID_MASK, UNIT_FLAG_SWIMMING, WALK_IN_WATER,
    },
};
use std::collections::BTreeSet;

const FIXTURE: &str = include_str!("fixtures/creature_swimming_cmangos.test-fixture");
fn number(bits: u32) -> SpellFloat {
    SpellFloat::from_bits(bits).unwrap()
}

#[test]
fn unchanged_source_bodies_match_scalar_threshold_and_initialization_flags() {
    let mut scalar = 0;
    let mut initialization = 0;
    let mut cases = BTreeSet::new();
    let mut statuses = BTreeSet::new();
    for line in FIXTURE.lines().filter(|line| !line.starts_with('#')) {
        assert!(cases.insert(line), "duplicate oracle input: {line}");
        let mut words = line.split_whitespace();
        let kind = words.next().unwrap();
        let row: Vec<u32> = words.map(|word| word.parse().unwrap()).collect();
        let prefix = match kind {
            "S" => {
                scalar += 1;
                0
            }
            "I" => {
                initialization += 1;
                4
            }
            _ => panic!("unknown source probe: {line}"),
        };
        assert_eq!(row.len(), prefix + 16, "{line}");
        let input = &row[prefix..prefix + 7];
        let output = &row[prefix + 7..];
        let [vmap, grid, status, level, depth, radius, external] = *input else {
            unreachable!()
        };
        statuses.insert((vmap, grid, status));
        let can_check = vmap != 0 || grid != 0;
        let swimmable = is_swimmable(
            can_check,
            status,
            number(level),
            number(depth),
            number(radius),
        )
        .unwrap();
        let queried = if kind == "S" {
            assert_eq!(output[0], u32::from(swimmable), "{line}");
            assert_eq!(output[1], 0, "{line}");
            true
        } else {
            let habitat = InhabitType::try_from_raw(row[0]).unwrap();
            let movement = initialize_movement_flags(row[2], habitat, row[1], swimmable);
            assert_eq!(movement, output[0], "{line}");
            assert_eq!(project_unit_flags(row[3], movement), output[1], "{line}");
            habitat.raw() & 2 != 0 && row[1] & WALK_IN_WATER == 0
        };
        assert_eq!(output[2], u32::from(queried), "availability: {line}");
        assert_eq!(output[3], u32::from(queried && vmap == 0), "grid: {line}");
        let liquid_queried = queried && can_check;
        assert_eq!(output[4], u32::from(liquid_queried), "liquid: {line}");
        assert_eq!(
            output[5],
            if liquid_queried {
                u32::from(LIQUID_MASK)
            } else {
                0
            },
            "mask: {line}"
        );
        assert_eq!(
            output[6],
            if liquid_queried {
                LIQUID_COLLISION_HEIGHT_BITS
            } else {
                0
            },
            "default classification height must not use radius: {line}"
        );
        let supplied_data_written = kind == "S" && external != 0 && liquid_queried && status != 0;
        assert_eq!(
            output[7],
            if supplied_data_written {
                level
            } else {
                0x42f6_0000
            },
            "{line}"
        );
        assert_eq!(
            output[8],
            if supplied_data_written {
                depth
            } else {
                0xc2f6_0000
            },
            "{line}"
        );
    }
    assert_eq!((scalar, initialization), (2432, 2648));
    for vmap in 0..=1 {
        for grid in 0..=1 {
            for status in 0..16 {
                assert!(statuses.contains(&(vmap, grid, status)));
            }
        }
    }
}

#[test]
fn false_later_initialization_preserves_the_historical_swimming_bit() {
    let habitat = InhabitType::try_from_raw(3).unwrap();
    let first = initialize_movement_flags(0x100, habitat, 0, true);
    assert_eq!(first, 0x100 | MOVEFLAG_SWIMMING);
    let second = initialize_movement_flags(first, habitat, 0, false);
    assert_eq!(second, first);
    assert_eq!(project_unit_flags(0, second), UNIT_FLAG_SWIMMING);
    assert_eq!(
        initialize_movement_flags(second, habitat, WALK_IN_WATER, true),
        first
    );
    assert_eq!(
        initialize_movement_flags(second, InhabitType::try_from_raw(1).unwrap(), 0, true),
        first
    );
    assert_eq!(
        project_unit_flags(UNIT_FLAG_SWIMMING, 0),
        UNIT_FLAG_SWIMMING
    );
}

#[test]
fn comparison_preserves_strict_equality_signed_zero_and_subnormal_intermediate() {
    assert!(!is_swimmable(true, 1, number(0x4000_0000), number(0), number(0x4000_0000)).unwrap());
    assert!(is_swimmable(true, 1, number(0x4000_0001), number(0), number(0x4000_0000)).unwrap());
    assert!(!is_swimmable(true, 8, number(0x8000_0000), number(0), number(0)).unwrap());
    assert!(is_swimmable(true, 2, number(0x0080_0001), number(0x0080_0000), number(0)).unwrap());
    // The scalar port uses C++ truth conversion; source-generated statuses are
    // 0..=15, while the pure status word is not reinterpreted as an enum tag.
    assert!(is_swimmable(
        true,
        u32::MAX,
        number(0x4040_0000),
        number(0),
        number(0x4000_0000)
    )
    .unwrap());
    assert!(!is_swimmable(
        false,
        8,
        number(0x4040_0000),
        number(0),
        number(0x4000_0000)
    )
    .unwrap());
    assert!(!is_swimmable(true, 0, number(0x4040_0000), number(0), number(0x4000_0000)).unwrap());
}

#[test]
fn unsupported_numeric_encodings_cannot_enter_the_typed_scalar_domain() {
    for bits in [
        0x7f80_0000,
        0xff80_0000,
        0x7fc0_0000,
        1,
        0x807f_ffff,
        0x4974_2401,
    ] {
        assert!(SpellFloat::from_bits(bits).is_err());
    }
}
