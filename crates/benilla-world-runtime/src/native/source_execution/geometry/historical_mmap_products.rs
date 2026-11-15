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
    )?;
    let path = root.join("products").join(path);
    if name.ends_with(".mmap") {
        let map = name[..3].parse().unwrap();
        let short_params = std::fs::metadata(&path).unwrap().len() < 28;
        manager.files.retain_map(map, &path, hash)?;
        Ok(Some(Product::Map { map, short_params }))
    } else if !name.starts_with("go") {
        let map = name[..3].parse().unwrap();
        let x = name[3..5].parse().unwrap();
        let y = name[5..7].parse().unwrap();
        let length = std::fs::metadata(&path).unwrap().len();
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
