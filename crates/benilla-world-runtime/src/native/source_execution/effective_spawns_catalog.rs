// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

impl EffectiveSpawnInputs {
    /// Catalog operands do not bind source occurrence order or startup RNG position.
    pub fn catalog_diagnostic(catalog: &mut VerifiedCatalog) -> Result<Self> {
        let mut this = Self {
            creatures: catalog
                .execution_effective_creature_templates()?
                .into_iter()
                .map(|(id, flags, totem_ai)| {
                    (
                        id,
                        EffectiveSpawnTemplate {
                            flags,
                            totem_ai,
                            ..Default::default()
                        },
                    )
                })
                .collect(),
            ..Default::default()
        };
        for [id, flags, display, kind, data3, data5] in catalog.execution_effective_spawn_operands(
            "gameobject_template",
            ["entry", "ExtraFlags", "displayId", "type", "data3", "data5"],
        )? {
            this.gameobjects.insert(
                id,
                EffectiveSpawnTemplate {
                    flags,
                    display,
                    despawn_at_action: kind == 3 && data3 != 0 || kind == 10 && data5 != 0,
                    ..Default::default()
                },
            );
        }
        this.maps = catalog.execution_map_entries()?.into_iter().collect();
        let mut displays = catalog.dbc_view("GameObjectDisplayInfo.dbc")?;
        for ordinal in 0..displays.record_count() {
            let row = displays.record(ordinal)?;
            this.displays.insert(
                row["key"]
                    .as_u64()
                    .ok_or_else(|| refuse("execution.effective_spawns.display_key"))?
                    as u32,
            );
        }
        drop(displays);
        this.spawn_templates = catalog
            .execution_effective_spawn_operands("creature_spawn_data_template", ["Entry"])?
            .into_iter()
            .map(|[id]| id)
            .collect();
        this.strings = catalog
            .execution_effective_spawn_operands("string_id", ["id"])?
            .into_iter()
            .map(|[id]| id)
            .collect();
        // The conditional postprocessor logs absent templates but retains rows, :1747-1751.
        this.conditional = catalog
            .execution_effective_spawn_operands(
                "creature_conditional_spawn",
                ["Guid", "EntryAlliance", "EntryHorde"],
            )?
            .into_iter()
            .map(|[guid, a, h]| (guid, [a, h]))
            .collect();
        for kind in [GridSpawnKind::Creature, GridSpawnKind::GameObject] {
            let creature = kind == GridSpawnKind::Creature;
            let table = if creature {
                "creature_spawn_entry"
            } else {
                "gameobject_spawn_entry"
            };
            for [guid, entry] in
                catalog.execution_effective_spawn_operands(table, ["guid", "entry"])?
            {
                this.load_random_entry(kind, guid, entry);
            }
        }
        Ok(this)
    }
}
impl SourceEffectiveSpawns {
    /// Every deterministic row is probed independently. This is not startup execution.
    /// Random rows always refuse because catalog order is not an F3 startup binding.
    pub fn inspect_catalog_pre_terrain(
        catalog: &mut VerifiedCatalog,
        occurrences: &Path,
    ) -> Result<Self> {
        struct Unbound;
        impl EffectiveSpawnOwner for Unbound {}
        let inputs = EffectiveSpawnInputs::catalog_diagnostic(catalog)?;
        let mut rows = ObjectMgrSpawnRows::open(catalog, occurrences)?;
        let mut this = Self {
            random_entries: inputs.random.keys().copied().collect(),
            ..Default::default()
        };
        let mut gameobjects_seen = false;
        for kind in [GridSpawnKind::Creature, GridSpawnKind::GameObject] {
            while let Some(row) = rows.next(kind.table())? {
                gameobjects_seen |= kind == GridSpawnKind::GameObject;
                let _refused = this.inspect_row(kind, row, &inputs, &mut Unbound);
            }
        }
        // This query executes after all GO rows, including rows skipped post-insert.
        // Its prerequisite emptiness is audited, but post-terrain addon state stays private.
        if gameobjects_seen {
            let addons = catalog.execution_effective_spawn_operands(
                "gameobject_addon",
                ["guid", "animprogress", "state", "stringId"],
            )?;
            this.apply_gameobject_addons(&addons, &inputs.strings)?;
        }
        Ok(this)
    }
    pub fn pre_terrain_refusals(&self) -> &BTreeMap<SpawnKey, Refusal> {
        &self.refusals
    }
    pub fn pre_terrain_map(&self) -> &BTreeMap<SpawnKey, EffectiveSpawnData> {
        &self.pre_terrain
    }
    pub fn complete_count(&self) -> usize {
        self.completed.len()
    }
}
