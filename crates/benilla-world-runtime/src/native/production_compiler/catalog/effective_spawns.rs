// SPDX-License-Identifier: GPL-3.0-or-later
//! Verified ObjectMgr joined occurrences and bounded prerequisite operands.
use super::*;

#[derive(Clone, Debug)]
pub struct ObjectMgrSpawnRow {
    pub ordinal: u64,
    pub guid: u32,
    pub entry: u32,
    pub map: u32,
    pub pose: [f32; 4],
    pub respawn: [u32; 2],
    pub radius: f32,
    pub movement: u8,
    pub mask: u8,
    pub event: i16,
    pub pools: [i16; 2],
    pub spawn_template: u32,
    pub rotation: [f32; 4],
}

pub struct ObjectMgrSpawnRows {
    metadata: reader::Held,
    streams: [reader::Held; 2],
    rows: [u64; 2],
    ordinal: [u64; 2],
    at: [u64; 2],
    buffer: Vec<u8>,
}
impl ObjectMgrSpawnRows {
    pub fn open(catalog: &mut VerifiedCatalog, occurrences: &Path) -> Result<Self> {
        let metadata = reader::Held::open(&occurrences.join("occurrences.json"), CHUNK as u64)?;
        let raw = metadata.read(0, metadata.len() as usize, &mut Metrics::default(), CACHE)?;
        let meta = bounded_json(&raw)?;
        catalog_schema::fields(
            &meta,
            &["format", "streams", "source_snapshot", "complete"],
            "spawn.occurrences",
        )?;
        check(
            meta["format"] == "benilla-spawn-occurrences-v3"
                && meta["source_snapshot"] == "phase0-sequential-diagnostic-not-F1"
                && meta["complete"] == false,
            "spawn.occurrences",
        )?;
        catalog_schema::fields(
            &meta["streams"],
            &["creature", "gameobject"],
            "spawn.occurrences",
        )?;
        let streams = [
            reader::Held::open(&occurrences.join("creature.rows"), MAX_RAW)?,
            reader::Held::open(&occurrences.join("gameobject.rows"), MAX_RAW)?,
        ];
        let views = SpawnViews::build(catalog)?;
        let mut rows = [0; 2];
        let mut buffer = vec![0; FRAME];
        for (slot, name) in ["creature", "gameobject"].into_iter().enumerate() {
            let d = &meta["streams"][name];
            catalog_schema::fields(d, &["query", "rows", "bytes", "sha256"], "spawn.query")?;
            let id = if slot == 0 {
                "Globals/ObjectMgr:1877"
            } else {
                "Globals/ObjectMgr:2086"
            };
            check(
                d["query"] == catalog_schema::POLICY["loaders"][id]["query"],
                "spawn.query",
            )?;
            let expected = views.expected_join(catalog, name)?;
            verify_stream(
                &streams[slot],
                if slot == 0 { 16 } else { 17 },
                &expected,
                d,
                &mut buffer,
            )?;
            rows[slot] = expected.len() as u64;
        }
        metadata.check()?;
        Ok(Self {
            metadata,
            streams,
            rows,
            ordinal: [0; 2],
            at: [0; 2],
            buffer,
        })
    }
    pub fn next(&mut self, table: &str) -> Result<Option<ObjectMgrSpawnRow>> {
        let slot = match table {
            "creature" => 0,
            "gameobject" => 1,
            _ => return Err(fail("effective_spawns.joined_relation")),
        };
        check(
            slot == 0 || self.ordinal[0] == self.rows[0],
            "effective_spawns.loader_order",
        )?;
        self.metadata.check()?;
        self.streams[slot].check()?;
        if self.ordinal[slot] == self.rows[slot] {
            check(self.at[slot] == self.streams[slot].len(), "spawn.trailing")?;
            return Ok(None);
        }
        let ordinal = self.ordinal[slot];
        let at = self.at[slot];
        self.streams[slot].read_into(at, &mut self.buffer[..4])?;
        let size = le(&self.buffer, 0, 4)? as usize;
        check((16..=FRAME).contains(&size), "catalog.frame_limit")?;
        self.streams[slot].read_into(at + 4, &mut self.buffer[4..size])?;
        let row = RowView::new(
            &self.buffer[..size],
            ordinal,
            if slot == 0 { 16 } else { 17 },
        )?;
        row.validate()?;
        let mut cells = [None; 256];
        fill(&row, &mut cells)?;
        let n = |i| uint(cells[i]);
        let f = |i| floating(cells[i]);
        let (mask, event, pool) = if slot == 0 {
            (11, 12, 13)
        } else {
            (13, 14, 15)
        };
        let result = ObjectMgrSpawnRow {
            ordinal,
            guid: n(0)?,
            entry: n(1)?,
            map: n(2)?,
            pose: [f(3)?, f(4)?, f(5)?, f(6)?],
            respawn: if slot == 0 {
                [n(7)?, n(8)?]
            } else {
                [n(11)?, n(12)?]
            },
            radius: if slot == 0 { f(9)? } else { 0.0 },
            movement: if slot == 0 { n(10)? as u8 } else { 0 },
            mask: n(mask)? as u8,
            event: n(event)? as i16,
            pools: [n(pool)? as i16, n(pool + 1)? as i16],
            spawn_template: if slot == 0 { n(15)? } else { 0 },
            rotation: if slot == 0 {
                [0.0; 4]
            } else {
                [f(7)?, f(8)?, f(9)?, f(10)?]
            },
        };
        self.ordinal[slot] += 1;
        self.at[slot] += size as u64;
        Ok(Some(result))
    }
}
impl VerifiedCatalog {
    pub(crate) fn execution_effective_spawn_operands<const N: usize>(
        &mut self,
        table: &str,
        names: [&str; N],
    ) -> Result<Vec<[u32; N]>> {
        check(
            matches!(
                table,
                "creature_spawn_entry"
                    | "gameobject_spawn_entry"
                    | "creature_conditional_spawn"
                    | "creature_spawn_data_template"
                    | "gameobject_template"
                    | "gameobject_addon"
                    | "string_id"
            ),
            "effective_spawns.operand_relation",
        )?;
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let cols = columns(&schema, table)?;
        let positions = names
            .map(|name| position(&cols, name))
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
        let mut output = Vec::new();
        self.scan(table, 0, 256, |row| {
            let mut cells = [None; 256];
            fill(&row, &mut cells)?;
            let mut words = [0; N];
            for (word, &pos) in words.iter_mut().zip(&positions) {
                *word = uint(cells[pos])?;
            }
            output.push(words);
            check(
                output.len() <= DECODED / std::mem::size_of::<[u32; N]>().max(1),
                "effective_spawns.view_limit",
            )
        })?;
        Ok(output)
    }
    pub(crate) fn execution_effective_creature_templates(
        &mut self,
    ) -> Result<Vec<(u32, u32, bool)>> {
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let cols = columns(&schema, "creature_template")?;
        let [entry, flags, ai] = [
            position(&cols, "Entry")?,
            position(&cols, "ExtraFlags")?,
            position(&cols, "AIName")?,
        ];
        let mut output = Vec::new();
        self.scan("creature_template", 0, 256, |row| {
            let mut cells = [None; 256];
            fill(&row, &mut cells)?;
            let name = cells[ai]
                .unwrap_or_default()
                .split(|b| *b == 0)
                .next()
                .unwrap_or_default();
            output.push((uint(cells[entry])?, uint(cells[flags])?, name == b"TotemAI"));
            check(output.len() <= DECODED / 16, "effective_spawns.view_limit")
        })?;
        Ok(output)
    }
    /// SQLInstanceLoader operands, including the preceding sorted ScriptDevAI name IDs.
    pub(crate) fn execution_instance_templates(
        &mut self,
    ) -> Result<Vec<crate::native::source_execution::InstanceTemplateRow>> {
        use crate::native::source_execution::InstanceTemplateRow;
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let mut names = BTreeSet::new();
        let spec = &catalog_schema::POLICY["loaders"]["AI/ScriptDevAI/ScriptDevAIMgr:480"];
        for relation in spec["relations"]
            .as_array()
            .ok_or_else(|| fail("scripts.query"))?
        {
            let table = relation.as_str().ok_or_else(|| fail("scripts.query"))?;
            let index = position(&columns(&schema, table)?, "ScriptName")?;
            self.scan(table, 0, 256, |row| {
                let mut cells = [None; 256];
                fill(&row, &mut cells)?;
                if let Some(name) = cells[index].filter(|v| !v.is_empty()) {
                    check(name.is_ascii(), "scripts.name_collation")?;
                    names.insert(name.to_vec());
                    check(names.len() * 256 <= DECODED, "scripts.view_limit")?;
                }
                Ok(())
            })?;
        }
        let normalized: BTreeSet<_> = names
            .iter()
            .map(|v| v.trim_ascii_end().to_ascii_lowercase())
            .collect();
        check(normalized.len() == names.len(), "scripts.name_collation")?;
        // ScriptDevAIMgr.cpp:479,515,532-544: empty name is zero; byte-sorted names follow.
        let ids: BTreeMap<_, _> = names
            .into_iter()
            .enumerate()
            .map(|(i, name)| (name, (i + 1) as u32))
            .collect();
        let cols = columns(&schema, "instance_template")?;
        // SQLStorages.cpp:34-35 defines this exact 11-column SELECT * layout.
        check(cols.len() == 11, "instance_templates.sql_layout")?;
        let mut output = Vec::new();
        self.scan("instance_template", 0, 256, |row| {
            let mut c = [None; 256];
            fill(&row, &mut c)?;
            let name = c[9]
                .unwrap_or_default()
                .split(|b| *b == 0)
                .next()
                .unwrap_or_default();
            output.push(InstanceTemplateRow {
                map: uint(c[0])?,
                parent: uint(c[1])?,
                level_min: uint(c[2])?,
                level_max: uint(c[3])?,
                max_players: uint(c[4])?,
                reset_days: uint(c[5])?,
                ghost_map: uint(c[6])? as i32,
                ghost_xy: [floating(c[7])?, floating(c[8])?],
                script_id: if name.is_empty() {
                    0
                } else {
                    *ids.get(name)
                        .ok_or_else(|| fail("instance_templates.script_name_owner"))?
                },
                mount_allowed: uint(c[10])? > 0,
            });
            check(
                output.len() <= DECODED / std::mem::size_of::<InstanceTemplateRow>(),
                "instance_templates.view_limit",
            )
        })?;
        Ok(output)
    }
}
