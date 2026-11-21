// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
// Upstream GPL-2.0-or-later; Rust translation GPL-3.0-or-later.
//! Pinned DBCStructure.h MapEntry / DBCfmt.h MapEntryfmt classification.
//! This opaque selection retains an exact loaded 42-word Map.dbc row. It proves
//! neither map geometry nor native admission. Common type is not IsContinent:
//! map identifiers 0/1 have no special meaning to this classification.
use benilla_formats::MapCatalog;
use serde::Serialize;
use std::collections::HashSet;

const POLICY: &str = "cmangos-8ec338a1-loaded-map-classification-v1";
const ROW_WORDS: usize = 42;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum MapKind {
    Common,
    Instance,
    Raid,
    Battleground,
}
impl MapKind {
    fn from_raw(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Common),
            1 => Some(Self::Instance),
            2 => Some(Self::Raid),
            3 => Some(Self::Battleground),
            _ => None,
        }
    }
}

/// Serialize-only evidence for an enclosing source identity. No deserializer,
/// public raw-row constructor, or caller-authored classification booleans.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MapClassificationSelection {
    policy: &'static str,
    map_id: u32,
    kind: MapKind,
    row: Vec<u32>,
}
impl MapClassificationSelection {
    pub fn select(catalog: &MapCatalog, map_id: u32) -> Option<Self> {
        select_rows(catalog.raw_rows()?, map_id)
    }
    pub fn map_id(&self) -> u32 {
        self.map_id
    }
    pub fn kind(&self) -> MapKind {
        self.kind
    }
    pub fn is_battleground(&self) -> bool {
        self.kind == MapKind::Battleground
    }
    pub fn instanceable(&self) -> bool {
        matches!(
            self.kind,
            MapKind::Instance | MapKind::Raid | MapKind::Battleground
        )
    }
    /// Recheck the retained selected row. Complete-table uniqueness was checked
    /// at construction; this method does not manufacture that loading evidence.
    pub fn validate(&self) -> bool {
        self.policy == POLICY
            && self.row.len() == ROW_WORDS
            && self.row[0] == self.map_id
            && MapKind::from_raw(self.row[2]) == Some(self.kind)
    }
}
fn select_rows(rows: &[[u32; ROW_WORDS]], map_id: u32) -> Option<MapClassificationSelection> {
    // Conservative supported subset: ANY duplicate ID in the loaded table is
    // refused, even identical/unselected rows. Do not select a last-writer winner.
    let mut ids = HashSet::new();
    if rows.iter().any(|row| !ids.insert(row[0])) {
        return None;
    }
    let row = rows.iter().find(|row| row[0] == map_id)?;
    let kind = MapKind::from_raw(row[2])?;
    Some(MapClassificationSelection {
        policy: POLICY,
        map_id,
        kind,
        row: row.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(id: u32, kind: u32) -> [u32; ROW_WORDS] {
        let mut row = [0; ROW_WORDS];
        row[0] = id;
        row[2] = kind;
        row
    }
    // Pure private-selector fixtures, not claims of real MPQ/DBC loading.
    #[test]
    fn exact_four_source_kinds_and_predicates() {
        for (raw, kind, bg, instanceable) in [
            (0, MapKind::Common, false, false),
            (1, MapKind::Instance, false, true),
            (2, MapKind::Raid, false, true),
            (3, MapKind::Battleground, true, true),
        ] {
            let selected = select_rows(&[row(7, raw)], 7).unwrap();
            assert!(selected.validate());
            assert_eq!(selected.map_id(), 7);
            assert_eq!(selected.kind(), kind);
            assert_eq!(selected.is_battleground(), bg);
            assert_eq!(selected.instanceable(), instanceable);
        }
    }
    #[test]
    fn map_id_is_not_continent_or_kind_inference() {
        for id in [0, 1, 7, u32::MAX] {
            assert_eq!(
                select_rows(&[row(id, 0)], id).unwrap().kind(),
                MapKind::Common
            );
            assert!(select_rows(&[row(id, 3)], id).unwrap().is_battleground());
        }
    }
    #[test]
    fn missing_unknown_and_duplicate_rows_fail_closed() {
        assert!(select_rows(&[], 7).is_none());
        assert!(select_rows(&[row(1, 0)], 7).is_none());
        for unknown in [4, 255, u32::MAX] {
            assert!(select_rows(&[row(7, unknown)], 7).is_none());
        }
        assert!(select_rows(&[row(7, 0), row(7, 0)], 7).is_none());
        assert!(select_rows(&[row(7, 0), row(7, 3)], 7).is_none());
        assert!(select_rows(&[row(7, 0), row(9, 1), row(9, 1)], 7).is_none());
        // An unknown unrelated kind does not change the selected row's meaning.
        assert!(select_rows(&[row(7, 0), row(9, 99)], 7).is_some());
    }
    #[test]
    fn complete_row_is_retained_without_reinterpreting_unused_words() {
        let mut raw = row(7, 2);
        raw[1] = u32::MAX;
        raw[41] = 0xdeadbeef;
        let selected = select_rows(&[raw], 7).unwrap();
        assert_eq!(selected.row, raw);
        assert!(selected.validate());
        assert_ne!(selected, select_rows(&[row(7, 2)], 7).unwrap());
    }
    #[test]
    fn retained_policy_shape_id_and_type_are_revalidated() {
        let valid = select_rows(&[row(7, 1)], 7).unwrap();
        for field in 0..6 {
            let mut changed = valid.clone();
            match field {
                0 => changed.policy = "other",
                1 => changed.row.clear(),
                2 => changed.row.push(0),
                3 => changed.row[0] = 8,
                4 => changed.row[2] = 4,
                5 => changed.kind = MapKind::Raid,
                _ => unreachable!(),
            }
            assert!(!changed.validate());
        }
    }
}
