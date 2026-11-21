// SPDX-License-Identifier: GPL-3.0-or-later
//! Checked phase0 query operands. No effective definitions or F1 SQL identity.
use super::*;
use crate::native::source_execution::{
    EventSpawnKind, GameEventDateRow, GameEventDefinitionRow, GameEventMail, GameEventMailRow,
    GameEventMemberRow, GameEventRows, GameEventSubstitution, GameEventSubstitutionRow,
};

struct Stream {
    held: reader::Held,
    rows: u64,
    ordinal: u64,
    at: u64,
    columns: usize,
}
pub(crate) struct EventCatalogRows {
    metadata: reader::Held,
    streams: Vec<Stream>,
    buffer: Vec<u8>,
}
impl EventCatalogRows {
    fn next<T>(
        &mut self,
        slot: usize,
        consume: impl FnOnce(u64, &[Option<&[u8]>]) -> Result<T>,
    ) -> Result<Option<T>> {
        check(
            slot < 8 && self.streams[..slot].iter().all(|s| s.ordinal == s.rows),
            "events.loader_phase",
        )?;
        self.metadata.check()?;
        let stream = self
            .streams
            .get_mut(slot)
            .ok_or_else(|| fail("events.query"))?;
        stream.held.check()?;
        if stream.ordinal == stream.rows {
            check(stream.at == stream.held.len(), "events.trailing")?;
            return Ok(None);
        }
        check(stream.at + 4 <= stream.held.len(), "events.exhausted")?;
        stream.held.read_into(stream.at, &mut self.buffer[..4])?;
        let size = le(&self.buffer, 0, 4)? as usize;
        check((16..=FRAME).contains(&size), "catalog.frame_limit")?;
        stream
            .held
            .read_into(stream.at + 4, &mut self.buffer[4..size])?;
        let row = RowView::new(&self.buffer[..size], stream.ordinal, stream.columns)?;
        row.validate()?;
        let result = consume(stream.ordinal, &cells(&row)?[..stream.columns])?;
        stream.at += size as u64;
        stream.ordinal += 1;
        Ok(Some(result))
    }
}
fn n(cell: Option<&[u8]>) -> Result<u32> {
    super::super::spawns::uint(cell)
}
fn bytes(cell: Option<&[u8]>) -> Vec<u8> {
    cell.unwrap_or_default()
        .split(|byte| *byte == 0)
        .next()
        .unwrap_or_default()
        .to_vec()
}
impl GameEventRows for EventCatalogRows {
    fn maximum(&mut self) -> crate::native::source_execution::Result<Option<u16>> {
        Ok(self.next(0, |_, v| Ok(n(v[0])? as u16))?)
    }
    fn definition(
        &mut self,
    ) -> crate::native::source_execution::Result<Option<GameEventDefinitionRow>> {
        Ok(self.next(1, |ordinal, v| {
            Ok(GameEventDefinitionRow {
                ordinal,
                id: n(v[0])? as u16,
                schedule: n(v[1])?,
                occurrence: n(v[2])?,
                length: n(v[3])?,
                holiday: n(v[4])?,
                linked: n(v[5])?,
                description: bytes(v[6]),
            })
        })?)
    }
    fn date(&mut self) -> crate::native::source_execution::Result<Option<GameEventDateRow>> {
        Ok(self.next(2, |ordinal, v| {
            Ok(GameEventDateRow {
                ordinal,
                id: n(v[0])? as u16,
                start: bytes(v[1]),
                end: bytes(v[2]),
            })
        })?)
    }
    fn member(
        &mut self,
        kind: EventSpawnKind,
    ) -> crate::native::source_execution::Result<Option<GameEventMemberRow>> {
        Ok(self.next(
            if kind == EventSpawnKind::Creature {
                3
            } else {
                4
            },
            |ordinal, v| {
                Ok(GameEventMemberRow {
                    ordinal,
                    exists: v[0].is_some(),
                    guid: n(v[1])?,
                    event: n(v[2])? as u16 as i16,
                })
            },
        )?)
    }
    fn substitution(
        &mut self,
    ) -> crate::native::source_execution::Result<Option<GameEventSubstitutionRow>> {
        Ok(self.next(5, |ordinal, v| {
            Ok(GameEventSubstitutionRow {
                ordinal,
                exists: v[0].is_some(),
                event: n(v[1])? as u16,
                data: GameEventSubstitution {
                    guid: n(v[7])?,
                    model: n(v[2])?,
                    equipment: n(v[3])?,
                    entry: n(v[4])?,
                    start_spell: n(v[5])?,
                    end_spell: n(v[6])?,
                },
            })
        })?)
    }
    fn quest(&mut self) -> crate::native::source_execution::Result<Option<(u64, u32, u16)>> {
        Ok(self.next(6, |ordinal, v| Ok((ordinal, n(v[0])?, n(v[1])? as u16)))?)
    }
    fn mail(&mut self) -> crate::native::source_execution::Result<Option<GameEventMailRow>> {
        Ok(self.next(7, |ordinal, v| {
            Ok(GameEventMailRow {
                ordinal,
                event: n(v[0])? as u16,
                mail: GameEventMail {
                    race_mask: n(v[1])?,
                    quest: n(v[2])?,
                    template: n(v[3])?,
                    sender: n(v[4])?,
                },
            })
        })?)
    }
}
impl VerifiedCatalog {
    #[cfg(test)]
    /// Assigned existence only: ObjectMgr.cpp:1563-1600,4293-4297 retains these keys.
    pub(crate) fn execution_event_reference_ids(&mut self) -> Result<[BTreeSet<u32>; 5]> {
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let mut ids = std::array::from_fn(|_| BTreeSet::new());
        for (slot, table, name) in [
            (0, "creature_equip_template", "entry"),
            (1, "creature_template", "entry"),
            (2, "spell_template", "id"),
            (3, "quest_template", "entry"),
        ] {
            let index = position(columns(&schema, table)?, name)?;
            self.scan(table, 0, 256, |row| {
                ids[slot].insert(n(cells(&row)?[index])?);
                Ok(())
            })?;
        }
        let mut mail = self.dbc_view("MailTemplate.dbc")?;
        for ordinal in 0..mail.record_count() {
            let row = mail.record(ordinal)?;
            let id = row["values"]
                .as_array()
                .and_then(|values| values.iter().find(|value| value[0] == 0))
                .and_then(|value| value[2].as_u64())
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| fail("events.mail_dbc_layout"))?;
            ids[4].insert(id);
        }
        Ok(ids)
    }
    /// Checks complete event stream identities but does not certify source execution.
    pub(crate) fn execution_event_rows(&mut self, occurrences: &Path) -> Result<EventCatalogRows> {
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let manifest_path = occurrences.join("occurrences.json");
        let metadata = reader::Held::open(&manifest_path, CHUNK as u64)?;
        let manifest_raw =
            metadata.read(0, metadata.len() as usize, &mut Metrics::default(), CACHE)?;
        let manifest = bounded_json(&manifest_raw)?;
        catalog_schema::fields(
            &manifest,
            &["format", "streams", "complete", "source_snapshot"],
            "events.occurrences",
        )?;
        let keys: Vec<_> = (0..LOADERS.len()).map(|slot| slot.to_string()).collect();
        catalog_schema::fields(
            &manifest["streams"],
            &keys.iter().map(String::as_str).collect::<Vec<_>>(),
            "events.occurrences",
        )?;
        check(
            manifest["format"] == "benilla-event-occurrences-v1"
                && manifest["complete"] == false
                && manifest["source_snapshot"] == "phase0-sequential-diagnostic-not-F1",
            "events.occurrence_format",
        )?;
        let mut streams = Vec::with_capacity(8);
        for (slot, loader) in LOADERS.iter().enumerate().take(8) {
            let spec = &catalog_schema::POLICY["loaders"][*loader];
            let descriptor = &manifest["streams"][slot.to_string()];
            catalog_schema::fields(
                descriptor,
                &["query", "rows", "bytes", "sha256"],
                "events.query",
            )?;
            check(
                descriptor["rows"].as_u64().is_some_and(|n| n <= 2_000_000)
                    && descriptor["bytes"].as_u64().is_some_and(|n| n <= MAX_RAW),
                "events.hash",
            )?;
            check(descriptor["query"] == spec["query"], "events.query")?;
            let (columns, expected) = expected(self, &schema, spec)?;
            let path = occurrences.join(format!("{slot}.rows"));
            let held = reader::Held::open(&path, MAX_RAW)?;
            verify_held_stream(&held, columns, &expected, descriptor)?;
            streams.push(Stream {
                held,
                rows: expected.len() as u64,
                ordinal: 0,
                at: 0,
                columns,
            });
        }
        metadata.check()?;
        Ok(EventCatalogRows {
            metadata,
            streams,
            buffer: vec![0; FRAME],
        })
    }
    pub(crate) fn execution_event_initial(snapshot: &Path, environment: &Path) -> Result<Value> {
        initial_state(snapshot, environment)
    }

    /// Retain and validate F2.5 query slot eight without replaying unrelated event loaders.
    pub(crate) fn execution_weather_rows(
        &mut self,
        occurrences: &Path,
    ) -> Result<WeatherCatalogRows> {
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let metadata = reader::Held::open(&occurrences.join("occurrences.json"), CHUNK as u64)?;
        let raw = metadata.read(0, metadata.len() as usize, &mut Metrics::default(), CACHE)?;
        let manifest = bounded_json(&raw)?;
        catalog_schema::fields(
            &manifest,
            &["format", "streams", "complete", "source_snapshot"],
            "weather.occurrences",
        )?;
        let keys: Vec<_> = (0..LOADERS.len()).map(|slot| slot.to_string()).collect();
        catalog_schema::fields(
            &manifest["streams"],
            &keys.iter().map(String::as_str).collect::<Vec<_>>(),
            "weather.occurrences",
        )?;
        check(
            manifest["format"] == "benilla-event-occurrences-v1"
                && manifest["complete"] == false
                && manifest["source_snapshot"] == "phase0-sequential-diagnostic-not-F1",
            "weather.occurrence_format",
        )?;
        let spec = &catalog_schema::POLICY["loaders"][LOADERS[8]];
        let descriptor = &manifest["streams"]["8"];
        catalog_schema::fields(
            descriptor,
            &["query", "rows", "bytes", "sha256"],
            "weather.query",
        )?;
        check(
            descriptor["rows"].as_u64().is_some_and(|n| n <= 2_000_000)
                && descriptor["bytes"].as_u64().is_some_and(|n| n <= MAX_RAW),
            "weather.hash",
        )?;
        check(descriptor["query"] == spec["query"], "weather.query")?;
        let (columns, expected) = expected(self, &schema, spec)?;
        let held = reader::Held::open(&occurrences.join("8.rows"), MAX_RAW)?;
        verify_held_stream(&held, columns, &expected, descriptor)?;
        metadata.check()?;
        Ok(WeatherCatalogRows(EventCatalogRows {
            metadata,
            streams: vec![Stream {
                held,
                rows: expected.len() as u64,
                ordinal: 0,
                at: 0,
                columns,
            }],
            buffer: vec![0; FRAME],
        }))
    }
}

pub(crate) struct WeatherCatalogRows(EventCatalogRows);
impl WeatherCatalogRows {
    pub(crate) fn next_weather(
        &mut self,
    ) -> crate::native::source_execution::Result<
        Option<crate::native::source_execution::WeatherChanceRow>,
    > {
        Ok(self.0.next(0, |ordinal, v| {
            let mut seasons = [[0; 3]; 4];
            for (season, values) in seasons.iter_mut().enumerate() {
                for (kind, value) in values.iter_mut().enumerate() {
                    *value = n(v[season * 3 + kind + 1])?;
                }
            }
            Ok(crate::native::source_execution::WeatherChanceRow {
                ordinal,
                zone: n(v[0])?,
                seasons,
            })
        })?)
    }
}
