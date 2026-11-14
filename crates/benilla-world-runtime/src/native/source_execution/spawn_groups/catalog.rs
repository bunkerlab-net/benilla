// SPDX-License-Identifier: GPL-3.0-or-later
//! Catalog lookups plus explicitly bound effective ObjectMgr spawn products.
use super::*;

/// These are LoadCreatures/LoadGameObjects products, never raw catalog rows.
/// The startup owner retains its real static grids and dynamic vectors here.
pub struct EffectiveGroupSpawns<'a> {
    pub definitions: &'a BTreeMap<SpawnKey, GroupSpawnDefinition>,
    pub random_entries: &'a BTreeSet<SpawnKey>,
    pub static_grids: &'a mut BTreeMap<u32, GridSpawnSets>,
    pub dynamic_guids: &'a mut BTreeMap<(u32, GridSpawnKind), Vec<u32>>,
}

pub struct CatalogGroupLoadOwner<'a> {
    catalog: &'a mut VerifiedCatalog,
    spawns: Option<EffectiveGroupSpawns<'a>>,
    pre_terrain: Option<SourceEffectiveSpawns>,
    autoload: Option<bool>,
    ids: BTreeMap<&'static str, BTreeSet<u32>>,
    creature_flags: Option<BTreeMap<u32, u32>>,
}
impl<'a> CatalogGroupLoadOwner<'a> {
    pub fn new(
        catalog: &'a mut VerifiedCatalog,
        spawns: Option<EffectiveGroupSpawns<'a>>,
        autoload: Option<bool>,
    ) -> Self {
        Self {
            catalog,
            spawns,
            pre_terrain: None,
            autoload,
            ids: BTreeMap::new(),
            creature_flags: None,
        }
    }
    /// Diagnostic pre-terrain map fields, not completed startup products.
    pub fn with_pre_terrain(
        catalog: &'a mut VerifiedCatalog,
        spawns: SourceEffectiveSpawns,
    ) -> Self {
        let mut this = Self::new(catalog, None, None);
        this.pre_terrain = Some(spawns);
        this
    }
    fn ids(&mut self, table: &'static str, column: &str) -> Result<&BTreeSet<u32>> {
        if !self.ids.contains_key(table) {
            let ids = self
                .catalog
                .execution_group_operands(table, [column])?
                .into_iter()
                .map(|row| row[0])
                .collect();
            self.ids.insert(table, ids);
        }
        Ok(&self.ids[table])
    }
    fn spawns(&mut self, kind: GridSpawnKind) -> Result<&mut EffectiveGroupSpawns<'a>> {
        self.spawns.as_mut().ok_or_else(|| {
            refuse(match kind {
                GridSpawnKind::Creature => {
                    "execution.spawn_groups.objectmgr_load_creatures_effective_spawn_owner"
                }
                GridSpawnKind::GameObject => {
                    "execution.spawn_groups.objectmgr_load_gameobjects_effective_spawn_owner"
                }
            })
        })
    }
}
impl GroupLoadOwner for CatalogGroupLoadOwner<'_> {
    fn condition_exists(&mut self, id: i32) -> Result<bool> {
        // ObjectMgr.cpp:1151-1158 consumes the condition storage ID.
        Ok(self
            .ids("conditions", "condition_entry")?
            .contains(&(id as u32)))
    }
    fn expression_exists(&mut self, id: i32) -> Result<bool> {
        Ok(self
            .ids("worldstate_expression", "id")?
            .contains(&(id as u32)))
    }
    fn string_exists(&mut self, id: u32) -> Result<bool> {
        Ok(self.ids("string_id", "id")?.contains(&id))
    }
    fn spawn(&mut self, key: SpawnKey) -> Result<Option<GroupSpawnDefinition>> {
        // ObjectMgr.cpp:1283-1313 reads effective data. In particular id=0 can
        // consume startup irand before insertion, :1916-1932 / ObjectMgr.h:600-604.
        if let Some(spawns) = &self.pre_terrain {
            return spawns.pre_terrain_group_definition(key);
        }
        Ok(self.spawns(key.kind)?.definitions.get(&key).copied())
    }
    fn template_exists(&mut self, kind: GridSpawnKind, entry: u32) -> Result<bool> {
        let table = match kind {
            GridSpawnKind::Creature => "creature_template",
            GridSpawnKind::GameObject => "gameobject_template",
        };
        Ok(self.ids(table, "entry")?.contains(&entry))
    }
    fn autoload_active(&mut self) -> Result<bool> {
        self.autoload
            .ok_or_else(|| refuse("execution.spawn_groups.autoload_active_config_owner"))
    }
    fn creature_template_flags(&mut self, entry: u32) -> Result<Option<u32>> {
        if self.creature_flags.is_none() {
            self.creature_flags = Some(
                self.catalog
                    .execution_group_operands("creature_template", ["entry", "ExtraFlags"])?
                    .into_iter()
                    .map(|row| (row[0], row[1]))
                    .collect(),
            );
        }
        Ok(self.creature_flags.as_ref().unwrap().get(&entry).copied())
    }
    fn has_random_entries(&mut self, key: SpawnKey) -> Result<bool> {
        if let Some(spawns) = &self.pre_terrain {
            return Ok(spawns.random_entries.contains(&key));
        }
        Ok(self.spawns(key.kind)?.random_entries.contains(&key))
    }
    fn detach_spawn(&mut self, key: SpawnKey, definition: GroupSpawnDefinition) -> Result<()> {
        if let Some(spawns) = &mut self.pre_terrain {
            spawns.complete_data(key)?;
            let cell = definition.cell;
            if let Some(grid) =
                spawns
                    .static_grids
                    .get_mut(&(definition.map, cell.x(), cell.y(), key.kind))
            {
                grid.remove(&key.guid);
            }
            if let Some(guids) = spawns.dynamic_guids.get_mut(&(definition.map, key.kind)) {
                guids.retain(|guid| *guid != key.guid);
            }
            return Ok(());
        }
        let spawns = self.spawns(key.kind)?;
        require(
            spawns.definitions.get(&key) == Some(&definition),
            "execution.spawn_groups.effective_spawn_binding",
        )?;
        // ObjectMgr.cpp:1500-1502,1528-1530: grid first, then every vector occurrence.
        if let Some(grid) = spawns.static_grids.get_mut(&definition.map) {
            grid.remove(definition.cell, key.kind, GridSpawnOrigin::Static, key.guid);
        }
        if let Some(guids) = spawns.dynamic_guids.get_mut(&(definition.map, key.kind)) {
            guids.retain(|guid| *guid != key.guid);
        }
        Ok(())
    }
}
impl SourceSpawnGroups {
    /// Source query order is templates, formations, members, entries, links,
    /// squads, then finalization. ObjectMgr.cpp:1125,1192,1250,1355,1398,1426,1466.
    pub fn load_bound_catalog<'a>(
        &mut self,
        catalog: &'a mut VerifiedCatalog,
        occurrences: &Path,
        spawns: Option<EffectiveGroupSpawns<'a>>,
        autoload: Option<bool>,
    ) -> Result<()> {
        let mut rows = catalog.execution_spawn_group_rows(occurrences)?;
        self.load(
            &mut rows,
            &mut CatalogGroupLoadOwner::new(catalog, spawns, autoload),
        )
    }
}

/// Keep a failed group load visible to each reached ScriptMgr lookup.
#[cfg(test)]
pub(crate) fn catalog_group_census_owner(
    catalog: &mut VerifiedCatalog,
) -> impl DbScriptLoaderOwner {
    struct CensusGroups(Result<SourceSpawnGroups>);
    impl DbScriptLoaderOwner for CensusGroups {
        fn contains(&mut self, kind: DbScriptLookup, id: u32) -> Result<bool> {
            if kind == DbScriptLookup::SpawnGroup {
                return match &mut self.0 {
                    Ok(groups) => groups.contains(kind, id),
                    Err(error) => Err(error.clone()),
                };
            }
            Err(refuse(&format!(
                "execution.db_scripts.{}_owner",
                kind.owner()
            )))
        }
    }
    let mut groups = SourceSpawnGroups::default();
    let result = std::env::var_os("BENILLA_F2_GROUP_OCCURRENCES")
        .ok_or_else(|| refuse("execution.spawn_groups.group_occurrences_owner"))
        .and_then(|path| {
            let Some(spawn_path) = std::env::var_os("BENILLA_F2_SPAWN_OCCURRENCES") else {
                return groups.load_bound_catalog(catalog, Path::new(&path), None, None);
            };
            let spawns = SourceEffectiveSpawns::inspect_catalog_pre_terrain(
                catalog,
                Path::new(&spawn_path),
            )?;
            let mut rows = catalog.execution_spawn_group_rows(Path::new(&path))?;
            groups.load(
                &mut rows,
                &mut CatalogGroupLoadOwner::with_pre_terrain(catalog, spawns),
            )
        })
        .map(|()| groups);
    CensusGroups(result)
}
