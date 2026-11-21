use super::*;
use crate::native::production_compiler::VerifiedCatalog;
use benilla_sim::source_rng::{DrawCursor, Mt19937, MtState, SourceRandom};
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;

pub(super) fn script_quest_recording() -> Option<(PathBuf, PathBuf)> {
    let config = std::env::var_os("BENILLA_F5_EVENT_AI_CONFIG")?;
    let trace = std::env::var_os("BENILLA_F5_EVENT_AI_TRACE")?;
    Some((config.into(), trace.into()))
}

pub(super) fn load_script_quest_catalog(
    catalog: &mut VerifiedCatalog,
    spells: &SourceSpellStores,
    config: &Path,
    trace: &Path,
) -> SourceQuestMap {
    // Seal16 trace.tsv:27790240 is the first world MT state.
    load_recorded_quest_catalog(catalog, spells, config, trace, 1713563789)
}

pub(super) fn load_recorded_quest_catalog(
    catalog: &mut VerifiedCatalog,
    spells: &SourceSpellStores,
    config: &Path,
    trace: &Path,
    first_mt_byte: u64,
) -> SourceQuestMap {
    let config = std::fs::read(config).unwrap();
    let value = |name| spawn_loader_config::assignment(&config, name).unwrap().1;
    let progression = benilla_sim::player_progression::Rules::from_classic_config(
        value("MaxPlayerLevel").parse().unwrap(),
        value("AlwaysMaxSkillForLevel").parse::<u32>().unwrap() != 0,
        value("Rate.Talent").parse::<f32>().unwrap().to_bits(),
    )
    .unwrap();
    let mut stream = BufReader::new(std::fs::File::open(trace).unwrap());
    stream.seek(SeekFrom::Start(first_mt_byte)).unwrap();
    let mut line = String::new();
    stream.read_line(&mut line).unwrap();
    let mut state = line.split_ascii_whitespace();
    assert_eq!(state.next(), Some("1"));
    state.next().unwrap(); // Thread-local sequence.
    assert_eq!(state.next(), Some("mt_state"));
    state.next().unwrap(); // Engine address.
    let words = std::array::from_fn(|_| state.next().unwrap().parse().unwrap());
    let index = state.next().unwrap().parse().unwrap();
    assert!(state.next().is_none());
    let mut mt = Mt19937::from_state(MtState { words, index }).unwrap();
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    let mut random = SourceRandom::new(1, &mut mt, &mut cursor, &mut sink).unwrap();
    let items = SourceStartupItems::catalog_prefix(catalog, &mut random).unwrap();
    assert_eq!(items.items_validated, 17_718);
    let creatures = LoadedCreatureTemplates::catalog_diagnostic(catalog).unwrap();
    let gameobjects = LoadedGameObjectTemplates::catalog_diagnostic(catalog).unwrap();
    SourceQuestMap::load_catalog(
        catalog,
        QuestCatalogInputs {
            creatures: Some(&creatures),
            gameobjects: Some(&gameobjects),
            items: Some(&items),
            spells: Some(spells),
            progression: Some(&progression),
        },
    )
    .unwrap()
}

pub(super) struct Trace {
    reader: BufReader<File>,
    line: String,
}
impl Trace {
    pub(super) fn open(path: &Path) -> Self {
        Self {
            reader: BufReader::new(File::open(path).unwrap()),
            line: String::new(),
        }
    }

    pub(super) fn seek(&mut self, offset: u64) {
        self.reader.seek(SeekFrom::Start(offset)).unwrap();
    }
    fn event(&mut self, kind: &str) -> &str {
        loop {
            self.line.clear();
            assert_ne!(self.reader.read_line(&mut self.line).unwrap(), 0);
            if self.line.starts_with("1\t") && self.line.split('\t').nth(2) == Some(kind) {
                break;
            }
        }
        self.line
            .splitn(4, '\t')
            .nth(3)
            .unwrap()
            .trim_end_matches('\n')
    }
    pub(super) fn query(&mut self, offset: u64, sql: &str, rows: usize, cols: usize) -> String {
        self.seek(offset);
        let event = self.event("sql_request_bytes");
        assert_eq!(
            unhex(event.split_whitespace().last().unwrap()),
            sql.as_bytes()
        );
        let result: Vec<_> = self.event("sql_query_result").split_whitespace().collect();
        assert_eq!((result[3].parse(), result[4].parse()), (Ok(rows), Ok(cols)));
        result[2].to_owned()
    }
    pub(super) fn row<const N: usize>(&mut self, cursor: &str) -> [Option<Vec<u8>>; N] {
        assert_eq!(self.event("sql_row"), cursor);
        std::array::from_fn(|i| {
            let mut fields = self.event("sql_field").split_ascii_whitespace();
            assert_eq!(fields.next(), Some(cursor));
            assert_eq!(fields.next().unwrap().parse(), Ok(i));
            let _mysql_type = fields.next().unwrap();
            let len = fields.next().unwrap();
            if len == "NULL" {
                return None;
            }
            let len: usize = len.parse().unwrap();
            assert_eq!(fields.next(), Some("0"));
            let bytes = if len == 0 {
                Vec::new()
            } else {
                unhex(fields.next().unwrap())
            };
            assert_eq!(bytes.len(), len);
            Some(bytes)
        })
    }
}
fn unhex(bytes: &str) -> Vec<u8> {
    assert!(bytes.len().is_multiple_of(2));
    (0..bytes.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&bytes[i..i + 2], 16).unwrap())
        .collect()
}

pub(super) fn recorded_area_triggers(path: &Path) -> Vec<[u32; 2]> {
    let mut trace = Trace::open(path);
    // Seal16 trace.tsv:46239919,46240052,46240475.
    let cursor = trace.query(
        2907968848,
        "SELECT id,quest FROM areatrigger_involvedrelation",
        45,
        2,
    );
    (0..45)
        .map(|_| {
            trace.row(&cursor).map(|value| {
                std::str::from_utf8(&value.unwrap())
                    .unwrap()
                    .parse()
                    .unwrap()
            })
        })
        .collect()
}
