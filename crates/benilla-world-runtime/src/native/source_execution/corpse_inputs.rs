// SPDX-License-Identifier: GPL-3.0-or-later
//! Retained character operands and SQL occurrence order, never authored corpse outcomes.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

pub const CORPSE_LOAD_QUERY: &str = "SELECT corpse.guid, player, corpse.position_x, corpse.position_y, corpse.position_z, corpse.orientation, corpse.map, time, corpse_type, instance, gender, race, class, playerBytes, playerBytes2, equipmentCache, guildId, playerFlags FROM corpse JOIN characters ON player = characters.guid LEFT JOIN guild_member ON player=guild_member.guid WHERE corpse_type <> 0";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorpseLoadRow {
    pub guid: u32,
    pub player: u32,
    pub position: [u32; 4],
    pub map: u32,
    pub time: u64,
    pub corpse_type: u32,
    pub instance: u32,
    pub gender: u8,
    pub race: u8,
    pub class: u8,
    pub player_bytes: u32,
    pub player_bytes2: u32,
    pub equipment_cache: Vec<u8>,
    pub guild: u32,
    pub player_flags: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpseJoinOccurrence {
    pub corpse: u64,
    pub character: u64,
    pub guild: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpseExecutionBinding {
    pub format: String,
    pub execution_binding_sha256: Sha256Id,
    pub initial_snapshot_sha256: Sha256Id,
    pub owner: InputOwner,
    pub occurrences: Vec<CorpseJoinOccurrence>,
}
pub struct CorpseInitialInputs<'a> {
    rows: BTreeMap<CorpseJoinOccurrence, CorpseLoadRow>,
    order: Vec<CorpseJoinOccurrence>,
    query_owner: Option<InputOwner>,
    held: Vec<ExecutionFile>,
    execution: Option<&'a VerifiedExecutionInputs>,
    pub raw_counts: [usize; 3],
}
impl<'a> CorpseInitialInputs<'a> {
    pub fn open(
        inputs: &'a VerifiedExecutionInputs,
        binding_path: &Path,
        approved: &Sha256Id,
    ) -> Result<Self> {
        let (file, bytes) = metadata(binding_path, 1_048_576)?;
        require(
            Sha256Id::of(&bytes) == *approved,
            "execution.corpse.binding_hash",
        )?;
        let binding: CorpseExecutionBinding = decode(&bytes, 1_048_576)?;
        require(
            binding.format == "benilla-source-corpse-inputs-v1"
                && binding.execution_binding_sha256 == *inputs.identity()
                && binding.initial_snapshot_sha256 == inputs.binding().initial_state.sha256,
            "execution.corpse.snapshot_binding",
        )?;
        binding.owner.callsite.validate()?;
        require(
            binding.owner.thread > 0
                && binding.owner.operation.thread == binding.owner.thread
                && binding.owner.operation.ordinal > 0
                && binding.owner.occurrence > 0
                && binding.owner.map_instance.is_none()
                && binding.owner.object.is_none()
                && binding.owner.callsite.source_file == "src/game/Globals/ObjectMgr.cpp"
                && binding.owner.callsite.line == 7371
                && binding.owner.callsite.function == "ObjectMgr::LoadCorpses",
            "execution.corpse.query_owner",
        )?;
        let tables = Tables::read(inputs.manager_snapshot()?)?;
        let mut this = Self::from_tables(tables, Some(binding.occurrences))?;
        this.held.push(file);
        this.execution = Some(inputs);
        this.query_owner = Some(binding.owner);
        Ok(this)
    }
    /// Diagnostic snapshot access remains explicitly non-admitting, like the manager input reader.
    pub fn inspect_snapshot(path: &Path, approved: &Sha256Id) -> Result<Self> {
        let file = ExecutionFile::open(path, 64 * 1024 * 1024)?;
        let mut hash = Sha256::new();
        let mut buffer = vec![0; 65536];
        let mut at = 0;
        while at < file.len() {
            let n = (file.len() - at).min(buffer.len() as u64) as usize;
            file.read_into(at, &mut buffer[..n])?;
            hash.update(&buffer[..n]);
            at += n as u64;
        }
        require(
            Sha256Id::parse(&format!("{:x}", hash.finalize()))? == *approved,
            "execution.corpse.snapshot_hash",
        )?;
        let mut this = Self::from_tables(Tables::read(&file)?, None)?;
        this.held.push(file);
        Ok(this)
    }
    fn from_tables(tables: Tables, order: Option<Vec<CorpseJoinOccurrence>>) -> Result<Self> {
        let raw_counts = std::array::from_fn(|i| tables.0[i].rows.len());
        let mut rows = BTreeMap::new();
        // Execute the inner character join and left guild join, without inventing a missing owner; ObjectMgr.cpp:7371-7375.
        for (corpse, values) in tables.0[0].rows.iter().enumerate() {
            if number(values, 8)? == 0 {
                continue;
            }
            let player = number(values, 1)? as u32;
            let Some(&character) = tables.0[1].keys.get(&u64::from(player)) else {
                continue;
            };
            let c = &tables.0[1].rows[character];
            let guild = tables.0[2]
                .keys
                .get(&u64::from(player))
                .map(|&n| (n, &tables.0[2].rows[n]));
            let id = CorpseJoinOccurrence {
                corpse: corpse as u64,
                character: character as u64,
                guild: guild.map(|(n, _)| n as u64),
            };
            rows.insert(
                id,
                CorpseLoadRow {
                    guid: number(values, 0)? as u32,
                    player,
                    position: [
                        float(values, 2)?,
                        float(values, 3)?,
                        float(values, 4)?,
                        float(values, 5)?,
                    ],
                    map: number(values, 6)? as u32,
                    time: number(values, 7)?,
                    corpse_type: number(values, 8)? as u32,
                    instance: number(values, 9)? as u32,
                    gender: number(c, 1)? as u8,
                    race: number(c, 2)? as u8,
                    class: number(c, 3)? as u8,
                    player_bytes: number(c, 4)? as u32,
                    player_bytes2: number(c, 5)? as u32,
                    equipment_cache: c[6].clone().unwrap_or_default(),
                    guild: guild.map(|(_, g)| number(g, 1)).transpose()?.unwrap_or(0) as u32,
                    player_flags: number(c, 7)? as u32,
                },
            );
        }
        let order = order.unwrap_or_else(|| rows.keys().copied().collect());
        require(
            order.iter().copied().collect::<BTreeSet<_>>() == rows.keys().copied().collect()
                && order.len() == rows.len(),
            "execution.corpse.query_occurrences",
        )?;
        Ok(Self {
            rows,
            order,
            held: Vec::new(),
            execution: None,
            raw_counts,
            query_owner: None,
        })
    }
    pub fn check(&self) -> Result<()> {
        for held in &self.held {
            held.check()?;
        }
        if let Some(execution) = self.execution {
            execution.manager_snapshot()?;
        }
        Ok(())
    }
    pub fn query_owner(&self) -> Option<&InputOwner> {
        self.query_owner.as_ref()
    }
    pub fn rows(&self) -> impl Iterator<Item = (CorpseJoinOccurrence, &CorpseLoadRow)> {
        self.order.iter().map(|id| (*id, &self.rows[id]))
    }
    pub fn validate_query_result(&self, status: CharacterQueryStatus) -> Result<()> {
        self.check()?;
        require(
            match status {
                CharacterQueryStatus::Rows => !self.rows.is_empty(),
                CharacterQueryStatus::Empty => self.rows.is_empty(),
                CharacterQueryStatus::Error => true,
            },
            "execution.corpse.query_result",
        )
    }
    pub fn require_sealed_execution(&self) -> Result<()> {
        Err(refuse("execution.f1_binding_pending"))
    }
    #[cfg(test)]
    pub(super) fn authored(raw: &[u8], order: Option<Vec<CorpseJoinOccurrence>>) -> Result<Self> {
        let mut tables = Tables::default();
        require(
            raw.last() == Some(&b'\n'),
            "execution.corpse.snapshot_truncated",
        )?;
        for line in raw[..raw.len() - 1].split(|b| *b == b'\n') {
            tables.line(line)?;
        }
        Self::from_tables(tables.finish()?, order)
    }
}
const NAMES: [&str; 3] = ["corpse", "characters", "guild_member"];
const COLUMNS: [&[&str]; 3] = [
    &[
        "guid",
        "player",
        "position_x",
        "position_y",
        "position_z",
        "orientation",
        "map",
        "time",
        "corpse_type",
        "instance",
    ],
    &[
        "guid",
        "gender",
        "race",
        "class",
        "playerBytes",
        "playerBytes2",
        "equipmentCache",
        "playerFlags",
    ],
    &["guid", "guildid"],
];
const CREATE: [&[u8]; 3] = [
    b"CREATE TABLE `corpse` (",
    b"CREATE TABLE `characters` (",
    b"CREATE TABLE `guild_member` (",
];
const LOCK: [&[u8]; 3] = [
    b"LOCK TABLES `corpse` WRITE;",
    b"LOCK TABLES `characters` WRITE;",
    b"LOCK TABLES `guild_member` WRITE;",
];
const INSERT: [&[u8]; 3] = [
    b"INSERT INTO `corpse` VALUES",
    b"INSERT INTO `characters` VALUES",
    b"INSERT INTO `guild_member` VALUES",
];
const INSERT_TABLE: [&[u8]; 3] = [
    b"INSERT INTO `corpse`",
    b"INSERT INTO `characters`",
    b"INSERT INTO `guild_member`",
];
type Cells = Vec<Option<Vec<u8>>>;
#[derive(Default)]
struct Table {
    columns: Vec<Option<usize>>,
    seen: BTreeSet<String>,
    rows: Vec<Cells>,
    keys: BTreeMap<u64, usize>,
    created: bool,
    locked: bool,
    unlocked: bool,
}
#[derive(Default)]
struct Tables([Table; 3], Option<usize>, Option<usize>, Option<usize>);
impl Tables {
    fn read(file: &ExecutionFile) -> Result<Self> {
        let mut parser = Self::default();
        let mut buffer = vec![0; 65536];
        let mut line = Vec::new();
        let mut at = 0;
        while at < file.len() {
            let n = (file.len() - at).min(buffer.len() as u64) as usize;
            file.read_into(at, &mut buffer[..n])?;
            for &byte in &buffer[..n] {
                if byte == b'\n' {
                    parser.line(&line)?;
                    line.clear();
                } else {
                    require(
                        line.len() < 1_048_576,
                        "execution.corpse.snapshot_line_limit",
                    )?;
                    line.push(byte);
                }
            }
            at += n as u64;
        }
        require(line.is_empty(), "execution.corpse.snapshot_truncated")?;
        file.check()?;
        parser.finish()
    }
    fn line(&mut self, line: &[u8]) -> Result<()> {
        if let Some(table) = self.3 {
            let done = line.ends_with(b";");
            let body = line
                .strip_suffix(if done { b";" } else { b"," })
                .ok_or_else(|| refuse("execution.corpse.snapshot_insert"))?;
            self.insert(table, body)?;
            if done {
                self.3 = None;
            }
            return Ok(());
        }
        if let Some(index) = self.1 {
            if let Some(rest) = line.strip_prefix(b"  `") {
                let name = std::str::from_utf8(rest.split(|b| *b == b'`').next().unwrap())
                    .map_err(|_| refuse("execution.corpse.snapshot_schema"))?;
                require(
                    self.0[index].seen.insert(name.to_ascii_lowercase())
                        && self.0[index].columns.len() < 256,
                    "execution.corpse.snapshot_schema",
                )?;
                self.0[index].columns.push(
                    COLUMNS[index]
                        .iter()
                        .position(|c| c.eq_ignore_ascii_case(name)),
                );
            } else if line.starts_with(b") ENGINE=") {
                self.1 = None;
            } else {
                require(
                    line.starts_with(b"  PRIMARY KEY")
                        || line.starts_with(b"  KEY")
                        || line.starts_with(b"  UNIQUE KEY"),
                    "execution.corpse.snapshot_schema",
                )?;
            }
            return Ok(());
        }
        for index in 0..NAMES.len() {
            if line == CREATE[index] {
                require(
                    !self.0[index].created,
                    "execution.corpse.duplicate_relation",
                )?;
                self.0[index].created = true;
                self.1 = Some(index);
                return Ok(());
            }
            if line == LOCK[index] {
                require(
                    self.0[index].created && !self.0[index].locked && self.2.is_none(),
                    "execution.corpse.snapshot_lock",
                )?;
                self.0[index].locked = true;
                self.2 = Some(index);
                return Ok(());
            }
            if let Some(rest) = line.strip_prefix(INSERT[index]) {
                require(
                    self.2 == Some(index),
                    "execution.corpse.snapshot_insert_scope",
                )?;
                if rest.is_empty() {
                    self.3 = Some(index);
                } else {
                    let body = rest
                        .strip_prefix(b" ")
                        .and_then(|b| b.strip_suffix(b";"))
                        .ok_or_else(|| refuse("execution.corpse.snapshot_insert"))?;
                    self.insert(index, body)?;
                }
                return Ok(());
            }
            require(
                !line.starts_with(INSERT_TABLE[index]),
                "execution.corpse.snapshot_insert",
            )?;
        }
        if line == b"UNLOCK TABLES;" {
            if let Some(index) = self.2.take() {
                self.0[index].unlocked = true;
            }
        } else if self.2.is_some() {
            require(
                line.starts_with(b"/*!40000 ALTER TABLE `")
                    && (line.ends_with(b" DISABLE KEYS */;")
                        || line.ends_with(b" ENABLE KEYS */;")),
                "execution.corpse.snapshot_statement",
            )?;
        }
        Ok(())
    }
    fn insert(&mut self, index: usize, mut bytes: &[u8]) -> Result<()> {
        require(!bytes.is_empty(), "execution.corpse.snapshot_insert")?;
        loop {
            bytes = bytes
                .strip_prefix(b"(")
                .ok_or_else(|| refuse("execution.corpse.snapshot_insert"))?;
            let mut values = vec![None; COLUMNS[index].len()];
            for (column, selected) in self.0[index].columns.iter().enumerate() {
                let (value, rest) = super::manager_inputs::sql_value(bytes)?;
                if let Some(selected) = selected {
                    values[*selected] = value.map(|v| v.into_owned());
                }
                let delimiter = if column + 1 == self.0[index].columns.len() {
                    b')'
                } else {
                    b','
                };
                require(
                    rest.first() == Some(&delimiter),
                    "execution.corpse.snapshot_insert",
                )?;
                bytes = &rest[1..];
            }
            let ordinal = self.0[index].rows.len();
            require(
                self.0[index]
                    .keys
                    .insert(number(&values, 0)?, ordinal)
                    .is_none(),
                "execution.corpse.snapshot_duplicate_key",
            )?;
            require(
                self.0[index].rows.len() < 2_000_000,
                "execution.corpse.snapshot_row_limit",
            )?;
            self.0[index].rows.push(values);
            if bytes.is_empty() {
                break;
            }
            bytes = bytes
                .strip_prefix(b",")
                .filter(|b| !b.is_empty())
                .ok_or_else(|| refuse("execution.corpse.snapshot_insert"))?;
        }
        Ok(())
    }
    fn finish(self) -> Result<Self> {
        require(
            self.1.is_none()
                && self.2.is_none()
                && self.3.is_none()
                && self.0.iter().enumerate().all(|(i, t)| {
                    t.created
                        && t.locked
                        && t.unlocked
                        && COLUMNS[i]
                            .iter()
                            .all(|c| t.seen.iter().any(|s| s.eq_ignore_ascii_case(c)))
                }),
            "execution.corpse.character_relation_absent",
        )?;
        Ok(self)
    }
}
fn number(values: &Cells, index: usize) -> Result<u64> {
    let value = values[index].as_deref().unwrap_or(b"0");
    std::str::from_utf8(value)
        .ok()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| refuse("execution.corpse.snapshot_number"))
}
fn float(values: &Cells, index: usize) -> Result<u32> {
    let value = values[index].as_deref().unwrap_or(b"0");
    let value: f64 = std::str::from_utf8(value)
        .ok()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| refuse("execution.corpse.snapshot_float"))?;
    Ok((value as f32).to_bits())
}
