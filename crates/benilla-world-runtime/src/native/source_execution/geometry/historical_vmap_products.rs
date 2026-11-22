// SPDX-License-Identifier: GPL-3.0-or-later
//! Historical F1 products and round-16 dispatch, not sealed producer lineage.
use super::*;
use benilla_native_vmap::live::FileState;
use benilla_sim::source_sine::ProcessReceipt;
use std::path::PathBuf;

pub(in crate::native::source_execution) fn selected_root(
    framing: &Path,
    configured: Option<&Path>,
) -> Result<PathBuf> {
    let root = std::fs::canonicalize(framing)
        .map_err(|_| refuse("execution.vmap.historical_root_unavailable"))?;
    if let Some(configured) = configured {
        let configured = std::fs::canonicalize(configured)
            .map_err(|_| refuse("execution.vmap.historical_root_unavailable"))?;
        require(
            configured == root,
            "execution.vmap.historical_root_mismatch",
        )?;
    }
    Ok(root)
}

fn receipt(root: &Path) -> ProcessReceipt {
    let recording = root.join("sealing/source-choice/seal16-full-01");
    let bytes = std::fs::read(recording.join("actual-process-environment.json")).unwrap();
    assert_eq!(
        Sha256Id::of(&bytes),
        Sha256Id::parse("4b7d35a74a48e0829d942539b97f940f05b4d57416eae71a3caf4dd6269859b5")
            .unwrap()
    );
    let receipt: ProcessReceipt = serde_json::from_slice(&bytes).unwrap();
    for (name, expected) in [
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
            Sha256Id::of(&std::fs::read(recording.join(name)).unwrap()),
            Sha256Id::parse(expected).unwrap()
        );
    }
    receipt
}
pub(in crate::native::source_execution) fn bind_dispatch(
    manager: &mut SourceVMapManager,
    root: &Path,
) {
    let receipt = receipt(root);
    manager
        .bind_rotation_dispatch(16, &receipt.source_acquisition_bindings, &receipt)
        .unwrap();
}
pub(in crate::native::source_execution) fn bind_vmaps(
    root: &Path,
    manager: &mut SourceVMapManager,
    mut inspect: impl FnMut(&str, &[u8]),
) {
    let receipt = receipt(root);
    let bytes = std::fs::read(root.join("products-manifest.json")).unwrap();
    assert_eq!(
        Sha256Id::of(&bytes),
        Sha256Id::parse("5d681785ca7cbccd26b453464e5d86a178290ed41caac9bac4c890053f69559f")
            .unwrap()
    );
    let manifest: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    manager
        .bind_rotation_dispatch(16, &receipt.source_acquisition_bindings, &receipt)
        .unwrap();
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
        inspect(name, &bytes);
        manager
            .collision
            .files
            .observe(name.as_bytes(), FileState::Present(bytes.into()))
            .unwrap();
    }
}

#[test]
fn historical_vmap_root_refuses_a_conflicting_override() {
    let framing = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = std::fs::canonicalize(framing).unwrap();
    assert_eq!(selected_root(framing, None).unwrap(), root);
    assert_eq!(selected_root(framing, Some(framing)).unwrap(), root);
    assert_eq!(
        selected_root(framing, Some(framing.parent().unwrap()))
            .unwrap_err()
            .id,
        "execution.vmap.historical_root_mismatch"
    );
}
