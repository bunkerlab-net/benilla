// SPDX-License-Identifier: GPL-3.0-or-later
//! Historical, unsealed F1 MMap inputs. Loading order remains with each caller.
use super::*;

pub(in crate::native::source_execution) enum Product {
    Map {
        map: u32,
        short_params: bool,
    },
    Tile {
        map: u32,
        x: u16,
        y: u16,
        short_header: bool,
    },
}
pub(in crate::native::source_execution) fn bind_product(
    root: &Path,
    row: &serde_json::Value,
    manager: &mut SourceMMapManager,
) -> Result<Option<Product>> {
    let path = row["path"]
        .as_str()
        .ok_or_else(|| refuse("execution.mmap.product_path"))?;
    let Some(name) = path.strip_prefix("mmaps/") else {
        return Ok(None);
    };
    let hash = Sha256Id::parse(
        row["sha256"]
            .as_str()
            .ok_or_else(|| refuse("execution.mmap.product_hash"))?,
    )
    .map_err(|_| refuse("execution.mmap.product_hash"))?;
    let path = root.join("products").join(path);
    if let Some(map) = name.strip_suffix(".mmap") {
        let map = map
            .parse()
            .map_err(|_| refuse("execution.mmap.product_map"))?;
        let short_params = std::fs::metadata(&path)
            .map_err(|_| refuse("execution.mmap.product_metadata"))?
            .len()
            < 28;
        manager.files.retain_map(map, &path, hash)?;
        Ok(Some(Product::Map { map, short_params }))
    } else if !name.starts_with("go") {
        let map = name
            .get(..3)
            .ok_or_else(|| refuse("execution.mmap.product_map"))?
            .parse()
            .map_err(|_| refuse("execution.mmap.product_map"))?;
        let x = name
            .get(3..5)
            .ok_or_else(|| refuse("execution.mmap.product_map"))?
            .parse()
            .map_err(|_| refuse("execution.mmap.product_map"))?;
        let y = name
            .get(5..7)
            .ok_or_else(|| refuse("execution.mmap.product_map"))?
            .parse()
            .map_err(|_| refuse("execution.mmap.product_map"))?;
        let length = std::fs::metadata(&path)
            .map_err(|_| refuse("execution.mmap.product_metadata"))?
            .len();
        let short_header = (1..20).contains(&length);
        manager.files.retain_tile(map, x, y, &path, hash)?;
        Ok(Some(Product::Tile {
            map,
            x,
            y,
            short_header,
        }))
    } else {
        Ok(None)
    }
}

fn binding_refusal(name: &str, hash: &str) -> String {
    let row = serde_json::json!({"path": name, "sha256": hash});
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    bind_product(&root, &row, &mut SourceMMapManager::default())
        .err()
        .expect("expected a named refusal")
        .id
}

const AUTHORED_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[test]
fn historical_mmap_bad_coordinates_refuse_by_name() {
    for name in [
        "mmaps/abc.mmap",
        "mmaps/000bad.mmap",
        "mmaps/4294967296.mmap",
        "mmaps/abc0000.mmtile",
        "mmaps/000aa00.mmtile",
        "mmaps/00000aa.mmtile",
    ] {
        assert_eq!(
            binding_refusal(name, AUTHORED_HASH),
            "execution.mmap.product_map"
        );
    }
}

#[test]
fn historical_mmap_short_names_refuse_by_name() {
    for name in [
        "mmaps/0",
        "mmaps/0000",
        "mmaps/000000",
        "mmaps/00é0000.mmtile",
    ] {
        assert_eq!(
            binding_refusal(name, AUTHORED_HASH),
            "execution.mmap.product_map"
        );
    }
}

#[test]
fn historical_mmap_bad_hash_refuses_by_name() {
    assert_eq!(
        binding_refusal("mmaps/000.mmap", "not-a-hash"),
        "execution.mmap.product_hash"
    );
}

#[test]
fn historical_mmap_metadata_failure_refuses_by_name() {
    for name in ["mmaps/000.mmap", "mmaps/0000000.mmtile"] {
        assert_eq!(
            binding_refusal(name, AUTHORED_HASH),
            "execution.mmap.product_metadata"
        );
    }
}
