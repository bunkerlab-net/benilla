// SPDX-License-Identifier: GPL-3.0-or-later
//! Test-only hash/hex primitives; no runtime encoder, source operation or authority changes.
use super::products::vmap::installed_source_fixture as input;
use benilla_native_vmap_source::profiling::{self, Mode};
use sha2::{digest::Output, Digest, Sha256};
use std::{
    collections::BTreeMap,
    hint::black_box,
    time::{Duration, Instant},
};

type Sha256Output = Output<Sha256>;

fn original_hex(digest: &Sha256Output) -> String {
    format!("{digest:x}")
}

/// Safe fixed-size lowercase candidate, confined to this test module.
fn fixed_hex(digest: &Sha256Output) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = [0u8; 64];
    for (index, byte) in digest.iter().copied().enumerate() {
        encoded[index * 2] = HEX[usize::from(byte >> 4)];
        encoded[index * 2 + 1] = HEX[usize::from(byte & 15)];
    }
    String::from_utf8(encoded.to_vec()).expect("lowercase hex is ASCII")
}

fn original_full(payload: &[u8]) -> String {
    format!("{:x}", Sha256::digest(payload))
}

fn fixed_full(payload: &[u8]) -> String {
    fixed_hex(&Sha256::digest(payload))
}

fn assert_digest_equivalent(bytes: [u8; 32]) {
    let digest: Sha256Output = bytes.into();
    let actual = fixed_hex(&digest);
    assert_eq!(actual, original_hex(&digest));
    assert_eq!(actual.len(), 64);
    assert!(actual
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
}

#[test]
fn hash_hex_encoding_matches_original() {
    for byte in 0..=u8::MAX {
        assert_digest_equivalent([byte; 32]);
    }
    let ramp = std::array::from_fn(|index| index as u8);
    assert_digest_equivalent(ramp);
    assert_digest_equivalent(ramp.map(|byte| !byte));
    for zeros in 0..=32 {
        let mut digest = [0xa5; 32];
        digest[..zeros].fill(0);
        assert_digest_equivalent(digest);
    }
    for (payload, expected) in [
        (
            &b""[..],
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            &b"abc"[..],
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
    ] {
        assert_eq!(original_full(payload), expected);
        assert_eq!(fixed_full(payload), expected);
    }
    for length in [55, 56, 63, 64, 65, 119, 120, 127, 128, 129] {
        let payload: Vec<_> = (0..length).map(|index| (index * 29 + 7) as u8).collect();
        assert_eq!(fixed_full(&payload), original_full(&payload));
    }
}

const MAX_FULL_ROUNDS: usize = 128;
const MAX_ENCODING_ROUNDS: usize = 4096;
const MAX_BYTES_PER_SAMPLE: usize = 64 * 1024 * 1024;

fn full_sample(payloads: &[Vec<u8>], rounds: usize, encode: fn(&[u8]) -> String) -> Duration {
    let encode = black_box(encode);
    let started = Instant::now();
    for _ in 0..rounds {
        for payload in payloads {
            // Both function choices compute SHA-256 freshly for every operation.
            drop(black_box(encode(black_box(payload.as_slice()))));
        }
    }
    started.elapsed()
}

fn encoding_sample(
    digests: &[Sha256Output],
    rounds: usize,
    encode: fn(&Sha256Output) -> String,
) -> Duration {
    let encode = black_box(encode);
    let started = Instant::now();
    for _ in 0..rounds {
        for digest in digests {
            drop(black_box(encode(black_box(digest))));
        }
    }
    started.elapsed()
}

fn balanced_samples(kind: &str, operations: usize, mut run: impl FnMut(bool) -> Duration) {
    let mut samples = [[Duration::ZERO; 2]; 2];
    // Exactly two balanced blocks, A then B and B then A; no retry or best-of selection.
    for (block, order) in [[false, true], [true, false]].into_iter().enumerate() {
        for candidate in order {
            let elapsed = run(candidate);
            samples[usize::from(candidate)][block] = elapsed;
            eprintln!(
                "hash-hex primitive sample: kind={kind} block={block} candidate={candidate} operations={operations} wall_ns={} ns_per_op={:.3}",
                elapsed.as_nanos(),
                elapsed.as_secs_f64() * 1_000_000_000. / operations as f64,
            );
        }
    }
    eprintln!(
        "hash-hex primitive complete sample distribution: kind={kind} original={:?} candidate={:?}; no speed threshold or overhead correction",
        samples[0], samples[1],
    );
}

#[test]
#[ignore = "bounded primitive timing diagnostic; run explicitly in release profile"]
fn fixture_hash_hex_primitive_comparison() {
    let install = input::navigation_install();
    let bundle = benilla_native_vmap_source::prepare(install.path(), 0, || false)
        .unwrap()
        .into_map_bundle(input::profile(), || false)
        .unwrap()
        .unwrap();
    let lengths = bundle.successful_replay_payload_lengths_for_test();
    assert!(!lengths.is_empty());
    let mut histogram = BTreeMap::<usize, usize>::new();
    for &length in &lengths {
        *histogram.entry(length).or_default() += 1;
    }
    assert_eq!(histogram.values().sum::<usize>(), lengths.len());
    // Independently witness one complete successful replay, not a deadline-censored trace.
    // All source setup/verification and histogram work is outside primitive timings.
    let (verification, source_counts) =
        profiling::measure(Mode::CountOnly, || bundle.verify_unchanged(|| false));
    verification.unwrap();
    assert_eq!(source_counts.verify.succeeded, 1);
    assert_eq!(source_counts.payload_hash.started, lengths.len());
    assert_eq!(source_counts.payload_hash.succeeded, lengths.len());
    let bytes_per_sweep = lengths
        .iter()
        .try_fold(0usize, |total, &length| total.checked_add(length))
        .unwrap();
    assert!((1..=MAX_BYTES_PER_SAMPLE).contains(&bytes_per_sweep));
    let full_rounds = MAX_FULL_ROUNDS.min(MAX_BYTES_PER_SAMPLE / bytes_per_sweep);
    // Encoding-only cap counts the 64-byte String outputs rather than payload bytes.
    let encoding_bytes_per_sweep = lengths.len().checked_mul(64).unwrap();
    assert!((1..=MAX_BYTES_PER_SAMPLE).contains(&encoding_bytes_per_sweep));
    let encoding_rounds = MAX_ENCODING_ROUNDS.min(MAX_BYTES_PER_SAMPLE / encoding_bytes_per_sweep);
    // Length-matched deterministic synthetic bytes, NOT the original source payloads.
    // Repeated receipts remain separate operations in their original receipt order.
    let payloads: Vec<Vec<u8>> = lengths
        .iter()
        .enumerate()
        .map(|(receipt, &length)| {
            (0..length)
                .map(|index| (index.wrapping_mul(31) ^ receipt.wrapping_mul(17)) as u8)
                .collect()
        })
        .collect();
    let digests: Vec<_> = payloads.iter().map(Sha256::digest).collect();
    for payload in &payloads {
        assert_eq!(original_full(payload), fixed_full(payload));
    }
    eprintln!(
        "hash-hex primitive fixture: full_successful_receipts={} length_histogram={histogram:?} bytes_per_sweep={bytes_per_sweep} full_rounds={full_rounds} encoding_rounds={encoding_rounds} max_bytes_per_sample={MAX_BYTES_PER_SAMPLE}; deterministic length-matched synthetic bytes, not captured payloads; includes archive/listfile/repeated receipts; excludes source I/O, freshness, controller, and deadline claims",
        lengths.len(),
    );
    balanced_samples(
        "fresh_sha256_plus_hex",
        lengths.len() * full_rounds,
        |candidate| {
            let encode: fn(&[u8]) -> String = if candidate { fixed_full } else { original_full };
            full_sample(&payloads, full_rounds, encode)
        },
    );
    balanced_samples(
        "encoding_only",
        lengths.len() * encoding_rounds,
        |candidate| {
            let encode: fn(&Sha256Output) -> String =
                if candidate { fixed_hex } else { original_hex };
            encoding_sample(&digests, encoding_rounds, encode)
        },
    );
}
