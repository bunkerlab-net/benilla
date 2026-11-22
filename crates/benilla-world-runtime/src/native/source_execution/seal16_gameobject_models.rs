// SPDX-License-Identifier: GPL-3.0-or-later
//! World.cpp:965-966 loads the retained model list before continent construction.
use super::*;
pub(super) fn load(
    recording: &Path,
    catalog: &mut VerifiedCatalog,
    geometry: &mut SourceTerrainManager,
) -> SourceDynamicModels {
    use benilla_native_vmap::live::FileState;
    let root = recording.ancestors().nth(3).unwrap();
    let bytes = std::fs::read(root.join("products-manifest.json")).unwrap();
    assert_eq!(
        Sha256Id::of(&bytes),
        Sha256Id::parse("5d681785ca7cbccd26b453464e5d86a178290ed41caac9bac4c890053f69559f")
            .unwrap()
    );
    let manifest: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    for row in manifest {
        let path = row["path"].as_str().unwrap();
        let Some(name) = path.strip_prefix("vmaps/") else {
            continue;
        };
        if !name.ends_with(".vmo") && name != "temp_gameobject_models" {
            continue;
        }
        let bytes = std::fs::read(root.join("products").join(path)).unwrap();
        assert_eq!(bytes.len() as u64, row["bytes"].as_u64().unwrap());
        assert_eq!(
            Sha256Id::of(&bytes),
            Sha256Id::parse(row["sha256"].as_str().unwrap()).unwrap()
        );
        geometry
            .vmap
            .collision
            .files
            .observe(name, FileState::Present(bytes.into()))
            .unwrap();
    }
    let mut models = SourceDynamicModels::default();
    models.load_displays(catalog).unwrap();
    models.load_model_list(&geometry.vmap).unwrap();
    models
}
