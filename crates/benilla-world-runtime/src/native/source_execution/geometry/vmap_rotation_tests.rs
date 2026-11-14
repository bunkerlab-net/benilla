// SPDX-License-Identifier: GPL-3.0-or-later
//! Optional diagnostic stock loading, never acquisition or gameplay admission.
use super::*;
use benilla_native_vmap::live::FileState;
use benilla_sim::source_sine::ProcessReceipt;
use std::{collections::BTreeMap, path::Path};

fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
// ModelInstance.cpp:156-194, MapTree.cpp:298-341,377-417.
fn angles(bytes: &[u8], tree: bool) -> Vec<[f32; 3]> {
    assert_eq!(&bytes[..8], b"VMAP_7.0");
    let (mut at, count) = if tree {
        if bytes[8] != 0 {
            return Vec::new();
        }
        assert_eq!(&bytes[9..13], b"NODE");
        let at = 13 + 24;
        let at = at + 4 + word(bytes, at) as usize * 4;
        let at = at + 4 + word(bytes, at) as usize * 4;
        assert_eq!(&bytes[at..at + 4], b"GOBJ");
        (at + 4, usize::MAX)
    } else {
        (12, word(bytes, 8) as usize)
    };
    let mut out = Vec::new();
    while at < bytes.len() && out.len() < count {
        let flags = word(bytes, at);
        out.push(std::array::from_fn(|i| {
            f32::from_bits(word(bytes, at + 22 + i * 4))
        }));
        at += 38 + if flags & 4 != 0 { 24 } else { 0 };
        let len = word(bytes, at) as usize;
        at += 4 + len + 4;
    }
    assert_eq!(at, bytes.len());
    if !tree {
        assert_eq!(out.len(), count);
    }
    out
}

#[test]
fn optional_stock_rotation_census_with_authenticated_round16_dispatch() {
    let Ok(root) = std::env::var("BENILLA_VMAP_ROTATION_F1") else {
        println!("SKIP stock rotation census: BENILLA_VMAP_ROTATION_F1 absent");
        return;
    };
    let root = Path::new(&root);
    let recording = root.join("sealing/source-choice/seal16-full-01");
    let receipt_bytes = std::fs::read(recording.join("actual-process-environment.json")).unwrap();
    assert_eq!(
        Sha256Id::of(&receipt_bytes),
        Sha256Id::parse("4b7d35a74a48e0829d942539b97f940f05b4d57416eae71a3caf4dd6269859b5")
            .unwrap()
    );
    let receipt: ProcessReceipt = serde_json::from_slice(&receipt_bytes).unwrap();
    for (file, expected) in [
        (
            "acquisition-input-manifest.json",
            &receipt.source_acquisition_bindings.manifest,
        ),
        (
            "trace-schema.json",
            &receipt.source_acquisition_bindings.schema,
        ),
        (
            "instrumentation-identity.json",
            &receipt.source_acquisition_bindings.instrumentation,
        ),
        (
            "run/trace.tsv.process-environment.bin",
            &receipt.source_process_receipt_sha256,
        ),
    ] {
        assert_eq!(
            Sha256Id::of(&std::fs::read(recording.join(file)).unwrap()),
            Sha256Id::parse(expected).unwrap()
        );
    }
    let manifest_bytes = std::fs::read(root.join("products-manifest.json")).unwrap();
    assert_eq!(
        Sha256Id::of(&manifest_bytes),
        Sha256Id::parse("5d681785ca7cbccd26b453464e5d86a178290ed41caac9bac4c890053f69559f")
            .unwrap()
    );
    let manifest: Vec<serde_json::Value> = serde_json::from_slice(&manifest_bytes).unwrap();
    let mut manager = SourceVMapManager::default();
    manager
        .bind_rotation_dispatch(16, &receipt.source_acquisition_bindings, &receipt)
        .unwrap();
    let mut loads = Vec::new();
    let (mut files, mut spawns, mut nonzero) = (0, 0, 0);
    for row in manifest {
        let path = row["path"].as_str().unwrap();
        let Some(name) = path.strip_prefix("vmaps/") else {
            continue;
        };
        let bytes = std::fs::read(root.join("products").join(path)).unwrap();
        assert_eq!(bytes.len() as u64, row["bytes"].as_u64().unwrap());
        assert_eq!(
            Sha256Id::of(&bytes),
            Sha256Id::parse(row["sha256"].as_str().unwrap()).unwrap()
        );
        if name.ends_with(".vmtree") || name.ends_with(".vmtile") {
            let tree = name.ends_with(".vmtree");
            let rows = angles(&bytes, tree);
            files += 1;
            spawns += rows.len();
            let n = rows.iter().flatten().filter(|&&v| v != 0.0).count();
            nonzero += n;
            let stem = name.split('.').next().unwrap();
            let parts: Vec<u16> = stem.split('_').map(|v| v.parse().unwrap()).collect();
            let (map, x, y) = if tree {
                (u32::from(parts[0]), 0, 0)
            } else {
                (u32::from(parts[0]), parts[2], parts[1])
            };
            if !tree || bytes[8] == 0 {
                loads.push((map, x, y, name.to_owned(), n));
            }
        }
        manager
            .collision
            .files
            .observe(name.as_bytes(), FileState::Present(bytes.into()))
            .unwrap();
    }
    assert_eq!((files, spawns, nonzero), (1292, 429016, 704524));
    let mut failures = BTreeMap::<String, usize>::new();
    let (mut loaded, mut admitted_angles) = (0, 0);
    for (map, x, y, name, count) in loads {
        match manager.load_map(map, x, y) {
            Ok(LoadResult::Ok) => {
                loaded += 1;
                admitted_angles += count;
            }
            Ok(other) => panic!("stock load {name}: {other:?}"),
            Err(error) => {
                println!("STOCK_ROTATION_NEXT {name} {} angles={count}", error.id);
                *failures.entry(error.id).or_default() += 1;
                // A refused tile can have partially installed instances. Do not reuse them.
                manager.collision.unload_map(map);
            }
        }
    }
    println!("STOCK_ROTATION files={files} spawns={spawns} nonzero={nonzero} loaded_files={loaded} admitted_angle_occurrences={admitted_angles} failures={failures:?}");
    assert_eq!(admitted_angles, 704524);
    assert!(
        failures.is_empty(),
        "next stock load refusals: {failures:?}"
    );
}
