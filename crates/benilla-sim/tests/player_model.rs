// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::classic_spell::SpellFloat;
use benilla_sim::player_model::{
    initial_collision_height, InitialPlayerModelInputs, PlayerModelError, PLAYER_MODEL_POLICY_ID,
};
use sha2::{Digest, Sha256};

fn sf(bits: u32) -> SpellFloat {
    SpellFloat::from_bits(bits).unwrap()
}
fn inputs(display: u32, model: u32, height: u32) -> InitialPlayerModelInputs {
    InitialPlayerModelInputs {
        native_display_id: 31,
        model_id: 51,
        display_scale: sf(display),
        model_scale: sf(model),
        raw_height: sf(height),
    }
}

#[test]
fn nonunit_player_scale_retains_both_raw_factors_and_separate_products() {
    let raw = inputs(0x3fa0_0000, 0x3fc0_0000, 0x4040_0000);
    let height = initial_collision_height(raw).unwrap();
    assert_eq!(height.policy(), PLAYER_MODEL_POLICY_ID);
    assert_eq!(height.native_display_id(), 31);
    assert_eq!(height.model_id(), 51);
    assert_eq!(height.display_scale(), raw.display_scale);
    assert_eq!(height.model_scale(), raw.model_scale);
    assert_eq!(height.raw_height(), raw.raw_height);
    assert_eq!(height.raw_scale_product().bits(), 0x3ff0_0000);
    assert_eq!(height.stored_object_scale().bits(), 0x3ff0_0000);
    assert_eq!(height.effective_object_scale().bits(), 0x3ff0_0000);
    assert_eq!(height.height().bits(), 0x40b4_0000);
    let serialized = serde_json::to_value(height).unwrap();
    assert_eq!(serialized["raw_scale_product"], 0x3ff0_0000u32);
    assert_eq!(serialized["stored_object_scale"], 0x3ff0_0000u32);
}

#[test]
fn sequential_binary32_rounding_cannot_be_reassociated_or_combined() {
    // Authored source-oracle row: each product rounds separately. Reassociation
    // or one final rounding of the exact triple product yields 0xbf05_edee.
    let result = initial_collision_height(inputs(0x3e1d_c131, 0x40b4_48dc, 0xbf1a_4e8b)).unwrap();
    assert_eq!(result.raw_scale_product().bits(), 0x3f5e_3179);
    assert_eq!(result.height().bits(), 0xbf05_edef);
    assert_ne!(result.height().bits(), 0xbf05_edee);
}

#[test]
fn either_scale_zero_preserves_product_sign_but_fresh_storage_stays_positive_zero() {
    for zero in [0, 0x8000_0000] {
        for other in [0x3fc0_0000, 0xbfc0_0000] {
            for (display, model) in [(zero, other), (other, zero)] {
                let height = initial_collision_height(inputs(display, model, 0x4040_0000)).unwrap();
                assert_eq!(
                    height.raw_scale_product().bits(),
                    (display ^ model) & 0x8000_0000
                );
                assert_eq!(height.stored_object_scale(), SpellFloat::ZERO);
                assert_eq!(height.effective_object_scale(), SpellFloat::ONE);
                assert_eq!(height.height().bits(), 0x4040_0000);
            }
        }
    }
}

#[test]
fn only_exact_signed_height_zero_selects_default() {
    for raw in [0, 0x8000_0000] {
        for scale in [0x3fc0_0000, 0xbfc0_0000] {
            let height = initial_collision_height(inputs(scale, 0x3f80_0000, raw)).unwrap();
            assert_eq!(height.raw_height().bits(), raw);
            assert_eq!(height.height().bits(), 0x4002_007e);
        }
    }
    for raw in [
        0x0080_0000,
        0x8080_0000,
        0x3f00_0000,
        0xbf00_0000,
        0x4002_007d,
        0x4002_007e,
        0x4002_007f,
    ] {
        assert_eq!(
            initial_collision_height(inputs(0x3f80_0000, 0x3f80_0000, raw))
                .unwrap()
                .height()
                .bits(),
            raw
        );
    }
}

#[test]
fn malformed_raw_inputs_and_missing_identity_fail_closed() {
    let raw = inputs(0x3f80_0000, 0x3f80_0000, 0x4040_0000);
    for invalid in [
        InitialPlayerModelInputs {
            native_display_id: 0,
            ..raw
        },
        InitialPlayerModelInputs { model_id: 0, ..raw },
    ] {
        assert_eq!(
            initial_collision_height(invalid),
            Err(PlayerModelError::InvalidIdentity)
        );
    }
    for bits in [
        1,
        0x007f_ffff,
        0x4974_2401,
        0x7f7f_ffff,
        0x7f80_0000,
        0x7fc0_0000,
    ] {
        for bits in [bits, bits | 0x8000_0000] {
            assert!(SpellFloat::from_bits(bits).is_err());
            for field in ["display_scale", "model_scale", "raw_height"] {
                let mut json = serde_json::to_value(raw).unwrap();
                json[field] = bits.into();
                assert!(serde_json::from_value::<InitialPlayerModelInputs>(json).is_err());
            }
        }
    }
    for malformed in ["null", "[]", "{}", "{\"native_display_id\": -1}"] {
        assert!(serde_json::from_str::<InitialPlayerModelInputs>(malformed).is_err());
    }
    let mut json = serde_json::to_value(raw).unwrap();
    json["height"] = 0x4040_0000u32.into();
    assert!(serde_json::from_value::<InitialPlayerModelInputs>(json).is_err());
}

#[test]
fn exceptional_intermediates_are_rejected_even_when_final_height_could_fit() {
    // Tiny normal scale multiplication would underflow to zero and then native
    // GetObjectScale would select one. This bounded arithmetic refuses it first.
    for (display, model) in [(0x0080_0000, 0x0080_0000), (0x0080_0000, 0x3f00_0000)] {
        assert_eq!(
            initial_collision_height(inputs(display, model, 0x4040_0000)),
            Err(PlayerModelError::Numeric("stored scale"))
        );
    }
    // An out-of-domain stored intermediate is not rescued by a later small factor.
    assert_eq!(
        initial_collision_height(inputs(0x4000_0000, 0x4974_2400, 0x3f00_0000)),
        Err(PlayerModelError::Numeric("stored scale"))
    );
    for raw in [0x0080_0000, 0x3f00_0000] {
        assert_eq!(
            initial_collision_height(inputs(0x0080_0000, 0x3f80_0000, raw)),
            Err(PlayerModelError::Numeric("collision height"))
        );
    }
    assert_eq!(
        initial_collision_height(inputs(0x3f80_0000, 0x4000_0000, 0x4974_2400)),
        Err(PlayerModelError::Numeric("collision height"))
    );
}

fn underflow_to_zero(left: u32, right: u32, result: u32) -> bool {
    left & 0x7fff_ffff != 0 && right & 0x7fff_ffff != 0 && result & 0x7fff_ffff == 0
}

#[test]
fn unchanged_source_oracle_matches_each_admitted_stage_bit_for_bit() {
    const FIXTURE: &str = include_str!("fixtures/player_model_cmangos.test-fixture");
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "ddb156c1a44a422b757ccc33a21f9d50f5b4b2686cba540249132ce6389e994d"
    );
    let mut counts = [0usize; 7];
    for line in FIXTURE.lines() {
        let row: Vec<u32> = line
            .split_whitespace()
            .map(|v| v.parse().unwrap())
            .collect();
        assert_eq!(row.len(), 11, "{line}");
        let [gender, display_id, model_id, display, model, raw, product, stored, effective, height_product, expected]: [u32; 11] = row.try_into().unwrap();
        assert!(gender <= 1);
        assert_eq!(
            (display_id, model_id),
            if gender == 0 { (31, 51) } else { (37, 57) }
        );
        let parsed = [display, model, raw].map(SpellFloat::from_bits);
        let [Ok(display_scale), Ok(model_scale), Ok(raw_height)] = parsed else {
            counts[0] += 1;
            continue;
        };
        let result = initial_collision_height(InitialPlayerModelInputs {
            native_display_id: display_id,
            model_id,
            display_scale,
            model_scale,
            raw_height,
        });
        if SpellFloat::from_bits(product).is_err() || underflow_to_zero(display, model, product) {
            counts[1] += 1;
            assert_eq!(
                result,
                Err(PlayerModelError::Numeric("stored scale")),
                "{line}"
            );
        } else if SpellFloat::from_bits(height_product).is_err()
            || underflow_to_zero(effective, raw, height_product)
        {
            counts[2] += 1;
            assert_eq!(
                result,
                Err(PlayerModelError::Numeric("collision height")),
                "{line}"
            );
        } else {
            let result = result.unwrap();
            assert_eq!(result.raw_scale_product().bits(), product, "{line}");
            assert_eq!(result.stored_object_scale().bits(), stored, "{line}");
            assert_eq!(result.effective_object_scale().bits(), effective, "{line}");
            assert_eq!(result.height().bits(), expected, "{line}");
            assert_eq!(result.native_display_id(), display_id);
            assert_eq!(result.model_id(), model_id);
            counts[3] += 1;
            counts[4] += usize::from(product == 0x8000_0000);
            counts[5] += usize::from(height_product & 0x7fff_ffff == 0);
            counts[6] += usize::from(expected & 0x8000_0000 != 0);
        }
    }
    assert_eq!(FIXTURE.lines().count(), 6796);
    assert_eq!(counts, [48, 244, 274, 6230, 440, 436, 2580]);
}
