// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::{
    source_moon as moon,
    source_sine::{self, DispatchBinding, ReferenceSine},
};
use moon::Sine64;
use serde_json::{json, Value};

#[test]
fn all_authored_pinned_oracle_words_match() {
    let inputs = include_str!("../tools/fixtures/source_sine/inputs.txt")
        .lines()
        .collect::<Vec<_>>();
    let expected = include_str!("../tools/fixtures/source_sine/expected.tsv")
        .lines()
        .collect::<Vec<_>>();
    assert_eq!(inputs.len(), expected.len());
    assert_eq!(inputs.len(), 10236);
    for (input, line) in inputs.iter().zip(&expected) {
        let row = line.split_whitespace().collect::<Vec<_>>();
        assert_eq!(row.len(), 2);
        assert_eq!(input, &row[0]);
        let operand = u64::from_str_radix(input, 16).unwrap();
        let actual = source_sine::sine_bits(operand);
        assert_eq!(
            actual,
            u64::from_str_radix(row[1], 16).unwrap(),
            "input {input}"
        );
    }
    println!("{} authored sine oracle words match", inputs.len());
}

#[test]
fn every_recorded_moon_sine_word_matches() {
    let mut count = 0;
    for line in include_str!("../tools/fixtures/source_moon/expected.jsonl").lines() {
        let case: Value = serde_json::from_str(line).unwrap();
        for record in case["records"].as_array().unwrap() {
            if record[0] != "sine" {
                continue;
            }
            let operand = u64::from_str_radix(record[1].as_str().unwrap(), 16).unwrap();
            let expected = u64::from_str_radix(record[2].as_str().unwrap(), 16).unwrap();
            assert_eq!(
                source_sine::sine_bits(operand),
                expected,
                "case {} input {operand:016x}",
                case["case"]
            );
            count += 1;
        }
    }
    assert_eq!(count, 26426);
}

fn receipt() -> Value {
    serde_json::from_str(include_str!(
        "../tools/fixtures/source_sine/process-environment.json"
    ))
    .unwrap()
}

fn bind(round: u32, acquisition: &Value, receipt: &Value) -> moon::Result<DispatchBinding> {
    let acquisition =
        serde_json::from_value(acquisition.clone()).map_err(|_| moon::Error::MissingSineBinding)?;
    let receipt =
        serde_json::from_value(receipt.clone()).map_err(|_| moon::Error::MissingSineBinding)?;
    DispatchBinding::from_process_receipt(round, &acquisition, &receipt)
}
#[test]
fn historical_binding_requires_matching_actual_process_receipt() {
    let receipt = receipt();
    let acquisition = &receipt["source_acquisition_bindings"];
    let binding = bind(14, acquisition, &receipt).unwrap();

    let mut sine = ReferenceSine::new(
        moon::CallOwner { thread: 2, call: 3 },
        moon::SineOrder::SolarFirst,
        binding,
    );
    assert_eq!(
        sine.owner().unwrap(),
        moon::CallOwner { thread: 2, call: 3 }
    );
    assert_eq!(sine.order().unwrap(), moon::SineOrder::SolarFirst);
    assert_eq!(sine.evaluate(0, moon::Anomaly::Solar, 0).unwrap(), 0);
    for round in [0, 12, 13] {
        assert_eq!(
            bind(round, acquisition, &receipt),
            Err(moon::Error::MissingSineBinding)
        );
    }
    let paths: [(&[&str], Value); 10] = [
        (&["format"], json!("unbound")),
        (&["observed_inside_actual_source_process"], json!(false)),
        (&["platform"], json!("other")),
        (&["source_process_receipt_sha256"], json!(null)),
        (&["cpu_flags"], json!(["avx2"])),
        (
            &["loader_environment", "GLIBC_TUNABLES"],
            json!("glibc.cpu.hwcaps=-FMA,-AVX2"),
        ),
        (&["loader_environment", "LD_HWCAP_MASK"], json!("0")),
        (
            &["source_acquisition_bindings", "REFERENCE_MANIFEST_SHA256"],
            json!("5555555555555555555555555555555555555555555555555555555555555555"),
        ),
        (&["resolved_functions_before_gameplay"], json!([])),
        (
            &["resolved_functions_before_gameplay"],
            json!([{"function":"sin","module_sha256":source_sine::LIBM_SHA256,"relative_offset":"0x79720"}]),
        ),
    ];
    for (path, value) in paths {
        let mut bad = receipt.clone();
        let mut target = &mut bad;
        for key in path {
            target = &mut target[*key];
        }
        *target = value;
        assert_eq!(
            bind(14, acquisition, &bad),
            Err(moon::Error::MissingSineBinding),
            "{path:?}"
        );
    }
    assert_eq!(
        bind(14, acquisition, &Value::Null),
        Err(moon::Error::MissingSineBinding)
    );
}

#[test]
fn retained_round14_receipt_or_explicit_absence() {
    let Ok(path) = std::env::var("BENILLA_LUNAR_PROCESS_RECEIPT") else {
        println!("SKIP unsealed process-receipt binding: BENILLA_LUNAR_PROCESS_RECEIPT absent");
        return;
    };
    let receipt: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    bind(14, &receipt["source_acquisition_bindings"], &receipt).unwrap();
    println!("Unsealed round-14 actual-process sine dispatch receipt matches; no F1 acceptance");
}
