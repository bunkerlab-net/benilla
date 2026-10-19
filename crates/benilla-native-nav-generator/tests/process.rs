// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(benilla_native_generator)]
use benilla_native_nav_generator::{
    GenerationRequest, GenerationResponse, GenerationStatus, MapParams, SourceConfig, TriangleMesh,
    DEFAULT_ALLOCATION_BUDGET, GRID_SIZE,
};
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn request() -> GenerationRequest {
    GenerationRequest {
        map_id: 0,
        tile_x: 32,
        tile_y: 32,
        uses_liquids: true,
        allocation_budget_bytes: DEFAULT_ALLOCATION_BUDGET,
        map: MapParams {
            origin: [-GRID_SIZE, f32::MIN_POSITIVE, -GRID_SIZE],
            tile_width: GRID_SIZE,
            tile_height: GRID_SIZE,
            max_tiles: 1,
            max_polys: 0x8000_0000,
        },
        bmin: [-GRID_SIZE, f32::MIN_POSITIVE, -GRID_SIZE],
        bmax: [0.0, f32::MAX, 0.0],
        config: SourceConfig::default(),
        solid: TriangleMesh::default(),
        liquid: TriangleMesh::default(),
        liquid_areas: vec![],
    }
}
fn run(bytes: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_benilla-native-navigation"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the package test must build the actual helper executable");
    child.stdin.take().unwrap().write_all(bytes).unwrap();
    child.wait_with_output().unwrap()
}
fn success(request: &GenerationRequest) -> GenerationResponse {
    let output = run(&request.encode().unwrap());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    GenerationResponse::decode(&output.stdout).unwrap()
}
fn assert_response(actual: &GenerationResponse, expected: &GenerationResponse) {
    assert_eq!(actual.status, expected.status);
    assert_eq!(actual.polygon_count, expected.polygon_count);
    assert_eq!(actual.native_peak_bytes, expected.native_peak_bytes);
    for (label, actual, expected) in [
        ("MMap", actual.mmap.as_slice(), expected.mmap.as_slice()),
        (
            "MMTile",
            actual.mmtile.as_slice(),
            expected.mmtile.as_slice(),
        ),
    ] {
        if actual == expected {
            continue;
        }
        // Stable diagnostic checksum only; full byte equality above decides success.
        let digest = |bytes: &[u8]| {
            bytes.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
            })
        };
        let offsets: Vec<_> = (0..actual.len().max(expected.len()))
            .filter(|&offset| actual.get(offset) != expected.get(offset))
            .take(8)
            .collect();
        panic!(
            "{label}: byte mismatch; actual_len={} expected_len={} actual_fnv1a64={:016x} expected_fnv1a64={:016x} first_offsets={offsets:?}",
            actual.len(),
            expected.len(),
            digest(actual),
            digest(expected),
        );
    }
}
#[test]
fn response_diagnostics_check_full_bytes_with_bounded_output() {
    let response = GenerationResponse {
        status: GenerationStatus::Generated,
        mmap: vec![0; 28],
        mmtile: vec![0; 1_000_000],
        polygon_count: 2,
        native_peak_bytes: 1,
    };
    assert_response(&response, &response);
    let mut changed = response.clone();
    changed.mmtile[999_999] = 1;
    for expected in [
        changed,
        GenerationResponse {
            mmtile: vec![0; 999_999],
            ..response.clone()
        },
    ] {
        let panic = std::panic::catch_unwind(|| assert_response(&response, &expected))
            .expect_err("unequal complete products must fail");
        let message = panic.downcast_ref::<String>().unwrap();
        assert!(message.len() < 300 && message.contains("first_offsets=[999999]"));
        assert!(message.contains("actual_fnv1a64="));
    }
}
fn plane(hole: bool) -> GenerationRequest {
    let mut input = request();
    input.bmin[1] = 0.0;
    input.bmax[1] = 0.0;
    let n = 32u32;
    for z in 0..=n {
        for x in 0..=n {
            input.solid.vertices.push([
                -GRID_SIZE + x as f32 * (GRID_SIZE / n as f32),
                0.0,
                -GRID_SIZE + z as f32 * (GRID_SIZE / n as f32),
            ]);
        }
    }
    for z in 0..n {
        for x in 0..n {
            if hole && (12..20).contains(&x) && (12..20).contains(&z) {
                continue;
            }
            let a = z * (n + 1) + x;
            let b = a + 1;
            let c = a + n + 1;
            let d = c + 1;
            input.solid.triangles.extend([[a, c, b], [b, c, d]]);
        }
    }
    input
}
#[test]
fn empty_request_runs_without_inherited_environment() {
    let expected = success(&request());
    let clean_run = |system_root: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_benilla-native-navigation"));
        command.env_clear();
        if system_root {
            if let Some(root) = std::env::var_os("SystemRoot") {
                command.env("SystemRoot", root);
            }
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let written = child
            .stdin
            .take()
            .unwrap()
            .write_all(&request().encode().unwrap());
        (written, child.wait_with_output().unwrap())
    };
    let (written, output) = clean_run(false);
    #[cfg(windows)]
    if !output.status.success() {
        let (written, diagnostic) = clean_run(true);
        eprintln!(
            "SystemRoot-only diagnostic: status {:?}, stdin {written:?}, stderr {}",
            diagnostic.status,
            String::from_utf8_lossy(&diagnostic.stderr[..diagnostic.stderr.len().min(512)]),
        );
    }
    assert!(
        output.status.success(),
        "clean-environment helper status {:?}, stdin {written:?}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr[..output.stderr.len().min(512)])
    );
    written.unwrap();
    assert!(output.stderr.is_empty());
    assert_response(
        &GenerationResponse::decode(&output.stdout).unwrap(),
        &expected,
    );
}

#[test]
fn empty_is_explicit_and_source_map_bits_are_preserved() {
    let result = success(&request());
    assert_eq!(result.status, GenerationStatus::Empty);
    assert_eq!(result.mmap.len(), 28);
    assert_eq!(&result.mmap[4..8], &f32::MIN_POSITIVE.to_le_bytes());
    assert_eq!(&result.mmap[24..28], &0x8000_0000u32.to_le_bytes());
    assert!(result.mmtile.is_empty());
}
#[test]
fn malformed_frames_and_configuration_fail_closed() {
    let valid = request().encode().unwrap();
    let mut trailing = valid.clone();
    trailing.push(0);
    let mut nonfinite = valid.clone();
    nonfinite[88..92].copy_from_slice(&f32::NAN.to_le_bytes());
    let mut bad_origin = valid.clone();
    bad_origin[36..40].copy_from_slice(&0f32.to_le_bytes());
    let mut excessive = valid.clone();
    excessive[132..136].copy_from_slice(&u32::MAX.to_le_bytes());
    for bytes in [
        vec![],
        valid[..100].to_vec(),
        trailing,
        nonfinite,
        bad_origin,
        excessive,
    ] {
        let output = run(&bytes);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty() && output.stderr.len() < 512);
    }
}
#[test]
fn source_flat_exact625_skip_is_retained() {
    let result = success(&plane(false));
    assert_eq!(result.status, GenerationStatus::Exact625);
    assert_eq!(result.polygon_count, 625);
    assert!(result.mmtile.is_empty());
}
#[test]
fn nontrivial_hole_generates_deterministically_and_budget_failures_are_clean() {
    let input = plane(true);
    let first = success(&input);
    assert_eq!(first.status, GenerationStatus::Generated);
    assert!(first.polygon_count > 0 && first.polygon_count != 625);
    let vertices = u32::from_le_bytes(first.mmtile[48..52].try_into().unwrap());
    assert_eq!(
        vertices % 2,
        0,
        "this regression must exercise the shifted dtLink buffer"
    );
    assert_response(&first, &success(&input));
    for budget in [
        1,
        128,
        192,
        224,
        256,
        4096,
        32768,
        65536,
        131072,
        262144,
        first.native_peak_bytes / 2,
        first.native_peak_bytes - 1,
    ] {
        let mut limited = input.clone();
        limited.allocation_budget_bytes = budget;
        let result = run(&limited.encode().unwrap());
        assert_eq!(
            result.status.code(),
            Some(2),
            "budget={budget}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(result.stdout.is_empty());
        assert!(String::from_utf8_lossy(&result.stderr).contains("allocation budget exceeded"));
    }
    let mut exact_budget = input;
    exact_budget.allocation_budget_bytes = first.native_peak_bytes;
    assert_response(&first, &success(&exact_budget));
}
#[test]
fn native_index_validation_is_independent_of_rust_encoder() {
    let mut input = plane(false).encode().unwrap();
    let vertices = u32::from_le_bytes(input[132..136].try_into().unwrap()) as usize;
    let first_index = 140 + vertices * 12;
    input[first_index..first_index + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    let output = run(&input);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("triangle index"));
}

#[test]
fn liquid_only_mesh_preserves_source_area_and_flags() {
    for (area, flags) in [(9u8, 4u16), (8u8, 8u16)] {
        let mut input = plane(true);
        input.liquid = std::mem::take(&mut input.solid);
        input.liquid_areas = vec![area; input.liquid.triangles.len()];
        let result = success(&input);
        assert_eq!(result.status, GenerationStatus::Generated);
        let vertices = u32::from_le_bytes(result.mmtile[48..52].try_into().unwrap()) as usize;
        let polygons = 120 + vertices * 12;
        for index in 0..result.polygon_count as usize {
            let polygon = polygons + index * 32;
            assert_eq!(
                u16::from_le_bytes(
                    result.mmtile[polygon + 28..polygon + 30]
                        .try_into()
                        .unwrap()
                ),
                flags
            );
            assert_eq!(result.mmtile[polygon + 31] & 0x3f, area);
        }
    }
}
