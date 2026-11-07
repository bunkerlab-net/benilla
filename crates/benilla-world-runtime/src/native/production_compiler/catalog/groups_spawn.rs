// SPDX-License-Identifier: GPL-3.0-or-later
//! Checked raw loader occurrences, not F2 dispositions or selected group state.
use super::*;
use benilla_sim::source_spawn_groups::{
    Formation, GroupInput, GroupInputData, GroupPhase, GroupTemplate, RandomEntry,
};

pub(crate) struct SpawnGroupCatalogRows {
    metadata: reader::Held,
    streams: [reader::Held; 6],
    rows: [u64; 6],
    at: [u64; 6],
    ordinal: [u64; 6],
    buffer: Vec<u8>,
}
impl SpawnGroupCatalogRows {
    pub(crate) fn open(catalog: &mut VerifiedCatalog, occurrences: &Path) -> Result<Self> {
        let (metadata, meta) = occurrence_manifest(occurrences)?;
        let schema =
            bounded_json(&catalog.object(&descriptor(&catalog.manifest["schema_root"])?, 0)?)?;
        let mut streams = Vec::with_capacity(6);
        let mut rows = [0; 6];
        let mut buffer = vec![0; FRAME];
        for slot in 0..6 {
            let spec = &catalog_schema::POLICY["loaders"][LOADERS[slot]];
            let d = &meta["streams"][slot.to_string()];
            catalog_schema::fields(d, &["query", "rows", "bytes", "sha256"], "groups.query")?;
            check(
                d["query"] == spec["query"]
                    && d["rows"].as_u64().is_some_and(|n| n <= 2_000_000)
                    && d["bytes"].as_u64().is_some_and(|n| n <= MAX_RAW),
                "groups.query",
            )?;
            let (columns, wanted) = expected(catalog, &schema, spec)?;
            check(columns == [10, 7, 4, 5, 2, 4][slot], "groups.query")?;
            let held = reader::Held::open(&occurrences.join(format!("{slot}.rows")), MAX_RAW)?;
            verify_held_stream(&held, columns, &wanted, d, &mut buffer)?;
            rows[slot] = wanted.len() as u64;
            streams.push(held);
        }
        metadata.check()?;
        Ok(Self {
            metadata,
            streams: streams.try_into().map_err(|_| fail("groups.query"))?,
            rows,
            at: [0; 6],
            ordinal: [0; 6],
            buffer,
        })
    }
    pub(crate) fn next(&mut self, phase: GroupPhase) -> Result<Option<GroupInput>> {
        let slot = phase as usize;
        self.metadata.check()?;
        self.streams[slot].check()?;
        check(
            (0..slot).all(|i| self.ordinal[i] == self.rows[i]),
            "groups.loader_phase",
        )?;
        if self.ordinal[slot] == self.rows[slot] {
            check(self.at[slot] == self.streams[slot].len(), "groups.trailing")?;
            return Ok(None);
        }
        let ordinal = self.ordinal[slot];
        let row = read_stream_row(
            &self.streams[slot],
            &mut self.buffer,
            &mut self.at[slot],
            ordinal,
            [10, 7, 4, 5, 2, 4][slot],
        )?;
        let mut cells = [None; 10];
        for (cell, value) in cells.iter_mut().zip(row.cells()) {
            *cell = value?;
        }
        let number = |i: usize| spawns::uint(cells[i]);
        // Field.h:49-51, matching the catalog's existing C-string projections.
        let text = |i: usize| {
            cells[i]
                .unwrap_or_default()
                .split(|byte| *byte == 0)
                .next()
                .unwrap_or_default()
                .to_vec()
        };
        let data = match phase {
            GroupPhase::Templates => GroupInputData::Template(GroupTemplate {
                id: number(0)?,
                name: text(1),
                kind: number(2)?,
                max_count: number(3)?,
                condition: number(4)? as i32,
                expression: number(5)? as i32,
                flags: number(6)?,
                string_id: number(7)?,
                respawn_min: cells[8].map(|v| spawns::uint(Some(v))).transpose()?,
                respawn_max: cells[9].map(|v| spawns::uint(Some(v))).transpose()?,
            }),
            GroupPhase::Formations => GroupInputData::Formation(Formation {
                group: number(0)?,
                kind: number(1)?,
                spread_bits: spawns::floating(cells[2])?.to_bits(),
                options: number(3)?,
                path: number(4)?,
                movement: number(5)?,
                comment: text(6),
            }),
            GroupPhase::Members => GroupInputData::Member {
                group: number(0)?,
                guid: number(1)?,
                slot: number(2)? as i32,
                chance: number(3)?,
            },
            GroupPhase::Entries => GroupInputData::Entry {
                group: number(0)?,
                entry: RandomEntry {
                    entry: number(1)?,
                    min: number(2)?,
                    max: number(3)?,
                    chance: number(4)?,
                },
            },
            GroupPhase::Links => GroupInputData::Link {
                group: number(0)?,
                linked: number(1)?,
            },
            GroupPhase::Squads => GroupInputData::Squad {
                group: number(0)?,
                squad: number(1)?,
                guid: number(2)?,
                entry: number(3)?,
            },
        };
        self.ordinal[slot] += 1;
        Ok(Some(GroupInput { ordinal, data }))
    }
}

#[cfg(test)]
#[path = "groups_spawn_tests.rs"]
mod tests;
