// SPDX-License-Identifier: GPL-3.0-or-later
//! TerrainInfo WMO lookup, CMaNGOS 8ec338a1 GridMap.cpp:904-918,981-1051.
use super::*;
use benilla_native_vmap::live::AreaInfo;
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceWmoAreaRow {
    pub id: u32,
    pub root: i32,
    pub adt: i32,
    pub group: i32,
    pub flags: u32,
    pub area: u32,
}
#[derive(Default)]
pub struct SourceWmoAreas {
    rows: Option<BTreeMap<(i32, i32, i32), Vec<SourceWmoAreaRow>>>,
}
impl SourceWmoAreas {
    /// Ascending indexed ID order, last physical duplicate wins, DBCStores.cpp:555-560.
    pub fn from_rows(rows: impl IntoIterator<Item = SourceWmoAreaRow>) -> Self {
        let indexed: BTreeMap<_, _> = rows.into_iter().map(|r| (r.id, r)).collect();
        let mut triples: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for row in indexed.into_values() {
            triples
                .entry((row.root, row.adt, row.group))
                .or_default()
                .push(row);
        }
        Self {
            rows: Some(triples),
        }
    }
    pub fn catalog(
        catalog: &mut super::super::production_compiler::VerifiedCatalog,
    ) -> Result<Self> {
        let rows = catalog.execution_wmo_area_dbc("WMOAreaTable.dbc")?;
        Ok(Self::from_rows(rows.into_iter().map(|r| {
            SourceWmoAreaRow {
                id: r[0],
                root: r[1] as i32,
                adt: r[2] as i32,
                group: r[3] as i32,
                flags: r[4],
                area: r[5],
            }
        })))
    }
    pub fn flag(
        &self,
        info: AreaInfo,
        areas: &BTreeMap<u32, SourceAreaEntry>,
        map: u32,
    ) -> Result<Option<u16>> {
        let rows = self
            .rows
            .as_ref()
            .ok_or_else(|| refuse("execution.terrain.wmo_area_catalog_owner"))?;
        let mut selected = None;
        if let Some(rows) = rows.get(&(info.root, info.adt, info.group)) {
            for row in rows {
                if let Some(area) = areas.get(&row.area).filter(|a| a.map == map) {
                    selected = Some(area.flag as u16);
                }
            }
        }
        Ok(selected)
    }
}
/// Preserve strict comparisons and source binary32 z+2.0f, GridMap.cpp:914.
pub fn area_hidden_by_terrain(z: f32, terrain: f32, vmap: f32) -> bool {
    z + 2.0 > terrain && terrain > vmap
}
impl SourceTerrainManager {
    /// Object.cpp:1242-1244 uses the current Map and position.
    pub fn object_zone_and_area(
        &mut self,
        objects: &SourceObjects,
        caster: ObjectHandle,
    ) -> Result<[u32; 2]> {
        let map = objects
            .get(caster)?
            .map
            .ok_or_else(|| refuse("execution.terrain.live_object_map_owner"))?;
        let pose = objects.pose(caster)?.0;
        self.zone_and_area(map.map, [pose[0], pose[1], pose[2]].map(f32::from_bits))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wmo_triples_preserve_signed_keys_last_map_match_and_missing_triples() {
        let areas = [
            SourceAreaEntry {
                id: 7,
                map: 0,
                zone: 1,
                flag: 65542,
            },
            SourceAreaEntry {
                id: 8,
                map: 1,
                zone: 1,
                flag: 9,
            },
            SourceAreaEntry {
                id: 9,
                map: 0,
                zone: 1,
                flag: 12,
            },
        ]
        .into_iter()
        .map(|a| (a.id, a))
        .collect();
        let rows = SourceWmoAreas::from_rows([
            SourceWmoAreaRow {
                id: 9,
                root: -1,
                adt: 2,
                group: 3,
                flags: 0,
                area: 9,
            },
            SourceWmoAreaRow {
                id: 1,
                root: -1,
                adt: 2,
                group: 3,
                flags: 0,
                area: 7,
            },
            SourceWmoAreaRow {
                id: 10,
                root: -1,
                adt: 2,
                group: 3,
                flags: 0,
                area: 8,
            },
            SourceWmoAreaRow {
                id: 11,
                root: -1,
                adt: 2,
                group: 3,
                flags: 0,
                area: 0,
            },
        ]);
        let info = AreaInfo {
            floor: 0.0,
            root: -1,
            adt: 2,
            group: 3,
            flags: 0,
        };
        assert_eq!(rows.flag(info, &areas, 0).unwrap(), Some(12));
        for missing in [
            AreaInfo { root: 2, ..info },
            AreaInfo { adt: 3, ..info },
            AreaInfo { group: 2, ..info },
        ] {
            assert_eq!(rows.flag(missing, &areas, 0).unwrap(), None);
        }
        assert_eq!(
            SourceWmoAreas::default()
                .flag(info, &areas, 0)
                .unwrap_err()
                .id,
            "execution.terrain.wmo_area_catalog_owner"
        );
    }
    #[test]
    fn terrain_hides_area_only_between_input_plus_two_and_floor() {
        assert!(area_hidden_by_terrain(10.0, 11.0, 4.0));
        assert!(!area_hidden_by_terrain(10.0, 12.0, 4.0));
        assert!(!area_hidden_by_terrain(10.0, 4.0, 4.0));
        assert!(!area_hidden_by_terrain(10.0, 3.0, 4.0));
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;
    use benilla_native_vmap::live::{FileState, LoadResult};
    use benilla_sim::source_rng::{DrawCursor, Mt19937, SourceRandom};
    #[test]
    fn genuine_loaded_tree_miss_preserves_map_only_zone_and_unloads_with_terrain() {
        let mut terrain = SourceTerrainManager::default();
        terrain.files.retain_source_absence(0, 32, 32);
        terrain.dbc.areas.insert(
            87,
            SourceAreaEntry {
                id: 87,
                map: 0,
                zone: 12,
                flag: 0,
            },
        );
        terrain.dbc.linked_zones.insert(0, 87);
        let mut mt = Mt19937::from_seed(1);
        let mut cursor = DrawCursor::default();
        let mut sink = |_| {};
        let mut random = SourceRandom::new(1, &mut mt, &mut cursor, &mut sink).unwrap();
        terrain.load_terrain(0, &mut random).unwrap();
        let before = terrain.zone_and_area(0, [0.0; 3]).unwrap();
        assert_eq!(before, [12, 87]);
        let mut bytes = b"VMAP_7.0\x01NODE".to_vec();
        for f in [0.0f32, 0.0, -10.0, 40000.0, 40000.0, 20.0] {
            bytes.extend(f.to_le_bytes());
        }
        for word in [3u32, 3 << 30, 0, 0, 0] {
            bytes.extend(word.to_le_bytes());
        }
        bytes.extend(b"GOBJ");
        terrain
            .vmap
            .collision
            .files
            .observe(b"000.vmtree", FileState::Present(bytes.into()))
            .unwrap();
        terrain
            .vmap
            .collision
            .files
            .observe(b"000_32_32.vmtile", FileState::Absent)
            .unwrap();
        assert_eq!(terrain.vmap.load_map(0, 32, 32).unwrap(), LoadResult::Ok);
        assert_eq!(terrain.vmap.loaded_trees(), 1);
        assert_eq!(terrain.zone_and_area(0, [0.0; 3]).unwrap(), before);
        assert_eq!(random.cursor().raw, 1);
        terrain.unload_all().unwrap();
        assert_eq!(terrain.vmap.loaded_trees(), 0);
    }
}
