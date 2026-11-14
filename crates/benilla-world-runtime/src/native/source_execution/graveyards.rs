// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! World.cpp:2644-2728 and GraveyardManager.cpp:130-135, without gameplay admission.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

pub const GRAVEYARD_QUERY: &str = "SELECT id,ghost_loc,link_kind,faction FROM game_graveyard_zone";
pub const DUNGEON_ENCOUNTER_BOUNDS_OWNER: &str =
    "execution.object_mgr.dungeon_encounter_bounds_owner";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraveyardData {
    pub safe_location: u32,
    pub team: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum GraveyardSkip {
    LinkKind,
    SafeLocation,
    Area,
    Map,
    Team,
    Duplicate,
}
#[derive(Default, Debug, PartialEq, Eq)]
pub struct GraveyardCensus {
    /// The source log increments before validation, including skipped rows.
    pub visited: usize,
    pub inserted: usize,
    pub skipped: BTreeMap<GraveyardSkip, usize>,
}
pub struct GraveyardReferences {
    pub safe_locations: BTreeSet<u32>,
    pub areas: BTreeSet<u32>,
    pub maps: BTreeSet<u32>,
}
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct SourceGraveyardManager {
    loaded: bool,
    // GraveyardManager.h:43's multimap sorts keys and preserves insertion order
    // among equivalent keys. A Vec per key preserves that order, not safeLocId order.
    entries: BTreeMap<u32, Vec<GraveyardData>>,
}
impl SourceGraveyardManager {
    pub fn entries(&self) -> &BTreeMap<u32, Vec<GraveyardData>> {
        &self.entries
    }
    /// World.cpp:2652-2724 never clears the map, despite its opening comment.
    /// A missing/empty query leaves existing links intact, as does reload.
    pub fn load_zones(
        &mut self,
        rows: Option<&[[u32; 4]]>,
        references: &GraveyardReferences,
    ) -> GraveyardCensus {
        let mut census = GraveyardCensus::default();
        for &[safe_location, location, kind, team] in rows.unwrap_or_default() {
            census.visited += 1;
            let skip = if kind > 1 {
                Some(GraveyardSkip::LinkKind)
            } else if !references.safe_locations.contains(&safe_location) {
                Some(GraveyardSkip::SafeLocation)
            } else if kind == 0 && !references.areas.contains(&location) {
                Some(GraveyardSkip::Area)
            } else if kind == 1 && !references.maps.contains(&location) {
                Some(GraveyardSkip::Map)
            } else if !matches!(team, 0 | 67 | 469) {
                Some(GraveyardSkip::Team)
            } else {
                // uint32 shift, including location's pre-existing high bit;
                // GraveyardManager.cpp:208-210. No signed-shift UB occurs here.
                let key = location | (kind << 31);
                let entries = self.entries.entry(key).or_default();
                // FindGraveYardData ignores team; .cpp:186-196.
                if entries
                    .iter()
                    .any(|data| data.safe_location == safe_location)
                {
                    Some(GraveyardSkip::Duplicate)
                } else {
                    entries.push(GraveyardData {
                        safe_location,
                        team,
                    });
                    census.inserted += 1;
                    None
                }
            };
            if let Some(reason) = skip {
                *census.skipped.entry(reason).or_default() += 1;
            }
        }
        self.loaded = true;
        census
    }
    /// Map.cpp:196, GraveyardManager.cpp:130-135 copies every link, not a map filter.
    pub fn init(&mut self, world: &Self) -> Result<()> {
        require(world.loaded, "execution.world.load_graveyard_zones_owner")?;
        self.entries.clone_from(&world.entries);
        self.loaded = true;
        Ok(())
    }
    pub fn load_catalog(
        &mut self,
        catalog: &mut super::super::production_compiler::VerifiedCatalog,
    ) -> Result<GraveyardCensus> {
        let (rows, references) = catalog.execution_graveyard_operands()?;
        Ok(self.load_zones(Some(&rows), &references))
    }
}

#[cfg(test)]
#[path = "graveyards_tests.rs"]
mod tests;
