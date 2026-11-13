// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::{source_trig as old, source_trig_fma as fma};

#[test]
fn pinned_fma_vectors_match_native_and_software_results() {
    let inputs = include_str!("../tools/fixtures/source_trig_fma/inputs.txt");
    let expected = include_str!("../tools/fixtures/source_trig_fma/expected.tsv");
    let mut count = 0;
    for (input, row) in inputs.lines().zip(expected.lines()) {
        let words: Vec<u32> = row
            .split_whitespace()
            .map(|s| u32::from_str_radix(s, 16).unwrap())
            .collect();
        assert_eq!(words.len(), 3);
        assert_eq!(words[0], u32::from_str_radix(input, 16).unwrap());
        let result = [words[1], words[2]];
        assert_eq!(
            fma::sin_cos_bits(words[0]),
            result,
            "software input {input}"
        );
        assert_eq!(
            fma::sin_cos_bits_native(words[0]),
            result,
            "native input {input}"
        );
        count += 1;
    }
    assert_eq!(inputs.lines().count(), 83);
    assert_eq!(expected.lines().count(), 83);
    assert_eq!(count, 83);
}

#[test]
fn ten_known_assembler_inputs_distinguish_fma_from_the_retained_offline_profile() {
    for sign in [0, 0x8000_0000] {
        for (input, before, after) in [
            (0x418a_3adb, 0xb7b4_f76f, 0xb7b4_f770),
            (0x418a_3adc, 0xb7a4_f76f, 0xb7a4_f770),
            (0x418a_3add, 0xb794_f76f, 0xb794_f770),
            (0x418a_3ade, 0xb784_f76f, 0xb784_f770),
            (0x41bc_76d9, 0xbb80_3f6a, 0xbb80_3f6b),
        ] {
            let input = input | sign;
            assert_eq!(
                old::sin_cos_assembler_radians_bits_native(input).unwrap()[1],
                before
            );
            assert_eq!(fma::sin_cos_bits(input)[1], after);
            assert_eq!(fma::sin_cos_bits_native(input)[1], after);
        }
    }
}

#[test]
fn software_and_native_operations_agree_across_exponents_and_reduction_tables() {
    for bits in (0..=u32::MAX).step_by(65537) {
        assert_eq!(
            fma::sin_cos_bits(bits),
            fma::sin_cos_bits_native(bits),
            "{bits:08x}"
        );
    }
}

#[test]
fn migrated_caller_wrappers_preserve_every_old_domain_refusal() {
    for bits in [
        0,
        1,
        old::TAU_BITS,
        old::TAU_BITS + 1,
        old::FALLBACK_MAX_ANGLE_BITS,
        old::FALLBACK_MAX_ANGLE_BITS + 1,
        old::EULER_MAX_RADIANS_BITS,
        old::EULER_MAX_RADIANS_BITS + 1,
        0x7f80_0000,
        0x7f80_0001,
        0x7fc1_2345,
    ] {
        for sign in [0, 0x8000_0000] {
            let bits = bits | sign;
            assert_eq!(
                fma::sin_cos_radians_bits_soft(bits).map(|_| ()),
                old::sin_cos_radians_bits_soft(bits).map(|_| ())
            );
            assert_eq!(
                fma::sin_cos_fallback_radians_bits_soft(bits).map(|_| ()),
                old::sin_cos_fallback_radians_bits_soft(bits).map(|_| ())
            );
            assert_eq!(
                fma::sin_cos_euler_radians_bits_native(bits).map(|_| ()),
                old::sin_cos_euler_radians_bits_native(bits).map(|_| ())
            );
        }
    }
}

#[test]
fn rotation_dispatch_requires_both_actual_process_functions_since_round14() {
    use benilla_sim::source_sine::{AcquisitionBinding, ProcessReceipt};
    use serde_json::json;
    let mut receipt: serde_json::Value = serde_json::from_str(include_str!(
        "../tools/fixtures/source_sine/process-environment.json"
    ))
    .unwrap();
    receipt["resolved_functions_before_gameplay"] = json!([
        {"function":"sinf", "module_sha256":fma::LIBM_SHA256, "relative_offset":fma::SINF_OFFSET},
        {"function":"cosf", "module_sha256":fma::LIBM_SHA256, "relative_offset":fma::COSF_OFFSET}
    ]);
    let acquisition: AcquisitionBinding =
        serde_json::from_value(receipt["source_acquisition_bindings"].clone()).unwrap();
    let bind = |round, value: serde_json::Value| {
        let receipt: ProcessReceipt = serde_json::from_value(value).unwrap();
        fma::DispatchBinding::from_process_receipt(round, &acquisition, &receipt)
    };
    assert!(bind(14, receipt.clone()).is_ok());
    assert!(bind(16, receipt.clone()).is_ok());
    assert_eq!(bind(13, receipt.clone()), Err(fma::MissingDispatchBinding));
    for (key, value) in [
        ("format", json!("unbound")),
        ("observed_inside_actual_source_process", json!(false)),
        ("platform", json!("other")),
        ("cpu_flags", json!(["avx2"])),
        ("source_process_receipt_sha256", json!("bad")),
        (
            "source_acquisition_bindings",
            json!({
                "REFERENCE_MANIFEST_SHA256":"5".repeat(64),
                "REFERENCE_SCHEMA_SHA256":acquisition.schema,
                "REFERENCE_INSTRUMENTATION_SHA256":acquisition.instrumentation
            }),
        ),
        (
            "loader_environment",
            json!({"GLIBC_TUNABLES":"glibc.cpu.hwcaps=-FMA,-AVX2","LD_HWCAP_MASK":null}),
        ),
    ] {
        let mut bad = receipt.clone();
        bad[key] = value;
        assert_eq!(bind(14, bad), Err(fma::MissingDispatchBinding), "{key}");
    }
    for index in [0, 1] {
        for (key, value) in [
            ("relative_offset", json!("0x72810")),
            ("module_sha256", json!("0".repeat(64))),
        ] {
            let mut bad = receipt.clone();
            bad["resolved_functions_before_gameplay"][index][key] = value;
            assert_eq!(bind(14, bad), Err(fma::MissingDispatchBinding));
        }
        let mut missing = receipt.clone();
        missing["resolved_functions_before_gameplay"]
            .as_array_mut()
            .unwrap()
            .remove(index);
        assert_eq!(bind(14, missing), Err(fma::MissingDispatchBinding));
        let mut duplicate = receipt.clone();
        let function = duplicate["resolved_functions_before_gameplay"][index].clone();
        duplicate["resolved_functions_before_gameplay"]
            .as_array_mut()
            .unwrap()
            .push(function);
        assert_eq!(bind(14, duplicate), Err(fma::MissingDispatchBinding));
    }
}
