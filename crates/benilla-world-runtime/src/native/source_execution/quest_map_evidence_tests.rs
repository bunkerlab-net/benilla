use super::*;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};

const QUEST_IDS: [u32; 2] = [7, 783];
type RawQuest = [Option<Vec<u8>>; 131];
type Relations = [Vec<[u32; 2]>; 4];

fn relation_kind(table: &str) -> Option<QuestRelationKind> {
    QuestRelationKind::SOURCE_ORDER
        .into_iter()
        .find(|kind| kind.table() == table)
}

fn mysql_tuple(line: &str) -> Vec<Option<Vec<u8>>> {
    let mut values = Vec::new();
    let mut bytes = line.as_bytes()[1..].iter().copied().peekable();
    loop {
        let value = if bytes.next_if_eq(&b'\'').is_some() {
            let mut text = Vec::new();
            loop {
                match bytes.next().expect("quoted SQL field") {
                    b'\'' => break,
                    b'\\' => text.push(match bytes.next().unwrap() {
                        b'0' => 0,
                        b'b' => 8,
                        b't' => b'\t',
                        b'n' => b'\n',
                        b'r' => b'\r',
                        b'Z' => 26,
                        v => v,
                    }),
                    byte => text.push(byte),
                }
            }
            Some(text)
        } else {
            let mut word = Vec::new();
            while let Some(byte) = bytes.next_if(|byte| !matches!(byte, b',' | b')')) {
                word.push(byte);
            }
            (word != b"NULL").then_some(word)
        };
        values.push(value);
        match bytes.next() {
            Some(b',') => {}
            Some(b')') => return values,
            other => panic!("unexpected SQL tuple delimiter {other:?}"),
        }
    }
}

fn stock(path: &Path) -> ([RawQuest; 2], Relations, BTreeSet<u32>) {
    let mut columns = Vec::new();
    let mut reading_quest_columns = false;
    let mut table = String::new();
    let mut quests: [Option<RawQuest>; 2] = [None, None];
    let mut relations: Relations = Default::default();
    let mut creatures = BTreeSet::new();
    let mut counts = [0usize; 4];
    for line in BufReader::new(File::open(path).unwrap()).lines() {
        let line = line.unwrap();
        if line.starts_with("CREATE TABLE `quest_template`") {
            reading_quest_columns = true;
        } else if reading_quest_columns {
            if let Some((column, _)) = line
                .trim_start()
                .strip_prefix('`')
                .and_then(|s| s.split_once('`'))
            {
                columns.push(column.to_owned());
            } else if line.starts_with(')') {
                reading_quest_columns = false;
            }
        }
        if let Some((name, _)) = line
            .strip_prefix("INSERT INTO `")
            .and_then(|s| s.split_once('`'))
        {
            table.clear();
            table.push_str(name);
            continue;
        }
        let Some((entry, rest)) = line.strip_prefix('(').and_then(|s| s.split_once(',')) else {
            continue;
        };
        if table == "quest_template" {
            if let Some(i) = QUEST_IDS.iter().position(|id| entry.parse() == Ok(*id)) {
                let raw = mysql_tuple(&line);
                assert_eq!(raw.len(), columns.len());
                let mut columns: BTreeMap<_, _> =
                    columns.iter().map(String::as_str).zip(raw).collect();
                let query = QUEST_TEMPLATE_QUERY
                    .strip_prefix("SELECT ")
                    .unwrap()
                    .strip_suffix(" FROM quest_template")
                    .unwrap();
                quests[i] = Some(
                    query
                        .split(',')
                        .map(|column| columns.remove(column.trim()).unwrap())
                        .collect::<Vec<_>>()
                        .try_into()
                        .unwrap(),
                );
            }
        } else if let Some(kind) = relation_kind(&table) {
            let (quest, _) = rest.split_once(')').unwrap();
            let quest = quest.parse().unwrap();
            counts[kind as usize] += 1;
            if QUEST_IDS.contains(&quest) {
                relations[kind as usize].push([entry.parse().unwrap(), quest]);
            }
        } else if table == "creature_template" {
            match entry {
                "6" | "197" | "823" => {
                    creatures.insert(entry.parse().unwrap());
                }
                _ => {}
            }
        }
    }
    assert_eq!(counts, [257, 208, 3827, 3951]);
    (quests.map(Option::unwrap), relations, creatures)
}

struct Trace {
    reader: BufReader<File>,
    line: String,
}
impl Trace {
    fn seek(&mut self, offset: u64) {
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
    fn query(&mut self, offset: u64, sql: &str, rows: usize, cols: usize) -> String {
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
    fn row<const N: usize>(&mut self, cursor: &str) -> [Option<Vec<u8>>; N] {
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

#[test]
fn northshire_recording_and_stock_dump_load_the_same_route_quests() {
    let (Some(stock_path), Some(trace_path)) = (
        std::env::var_os("BENILLA_QUEST_STOCK_SQL"),
        std::env::var_os("BENILLA_NORTHSHIRE_A_TRACE"),
    ) else {
        println!("SKIP stock/recorded quest rows: set BENILLA_QUEST_STOCK_SQL and BENILLA_NORTHSHIRE_A_TRACE");
        return;
    };
    let (stock_quests, stock_relations, creatures) = stock(Path::new(&stock_path));
    let mut trace = Trace {
        reader: BufReader::new(File::open(trace_path).unwrap()),
        line: String::new(),
    };
    // Northshire A trace.tsv:43533675,43533934,43536209,43938311.
    let cursor = trace.query(2731954066, QUEST_TEMPLATE_QUERY, 4245, 131);
    let recorded_quests: [RawQuest; 2] = [2732132314, 2758708408].map(|offset| {
        trace.seek(offset);
        trace.row(&cursor)
    });
    assert_eq!(recorded_quests, stock_quests);
    let mut relations: Relations = Default::default();
    // Northshire A trace.tsv:45817008,45819402,45821453,45856362.
    for (kind, (offset, count)) in QuestRelationKind::SOURCE_ORDER.into_iter().zip([
        (2884371141, 257),
        (2884510978, 208),
        (2884632125, 3827),
        (2886679923, 3951),
    ]) {
        let cursor = trace.query(
            offset,
            &format!("SELECT id,quest FROM {}", kind.table()),
            count,
            2,
        );
        for _ in 0..count {
            let [giver, quest] = trace
                .row(&cursor)
                .map(|v| std::str::from_utf8(&v.unwrap()).unwrap().parse().unwrap());
            if QUEST_IDS.contains(&quest) {
                relations[kind as usize].push([giver, quest]);
            }
        }
    }
    assert_eq!(relations, stock_relations);
    for (quests, mut relations) in [
        (stock_quests, stock_relations),
        (recorded_quests, relations),
    ] {
        let mut owner = Inputs::default();
        owner
            .present
            .extend([(QuestLookup::Area, 9), (QuestLookup::Faction, 72)]);
        owner
            .present
            .extend(creatures.iter().map(|&id| (QuestLookup::Creature, id)));
        let mut map = load(
            quests.iter().map(|row| {
                SourceQuestTemplate::from_sql_fields(std::array::from_fn(|i| row[i].as_deref()))
                    .unwrap()
            }),
            &mut owner,
        );
        map.load_quest_relations(
            |kind| Ok(std::mem::take(&mut relations[kind as usize])),
            &mut owner,
        )
        .unwrap();
        let intro = map.template(783).unwrap();
        let cleanup = map.template(7).unwrap();
        assert_eq!(
            (
                &*intro.title,
                intro.quest_level,
                intro.next_quest_in_chain,
                intro.special_flags
            ),
            (&b"A Threat Within"[..], 1, 7, 0)
        );
        assert_eq!(
            (
                &*cleanup.title,
                cleanup.quest_level,
                cleanup.prev_quest_id,
                cleanup.special_flags
            ),
            (&b"Kobold Camp Cleanup"[..], 2, 783, 0x30)
        );
        assert_eq!(
            (
                cleanup.required_targets[0],
                cleanup.required_target_counts[0],
                cleanup.required_targets_count()
            ),
            (6, 10, 1)
        );
        assert_eq!(cleanup.prev_quests(), &[783]);
        assert_eq!(cleanup.prev_chain_quests(), &[783]);
        assert_eq!(map.relations(QuestRelationKind::CreatureStart, 823), &[783]);
        assert_eq!(map.relations(QuestRelationKind::CreatureStart, 197), &[7]);
        assert_eq!(
            map.relations(QuestRelationKind::CreatureEnd, 197),
            &[7, 783]
        );
        assert_eq!(lines(&map), [7955, 7955]);
    }
}

#[test]
fn full_stock_catalog_refuses_at_the_first_unbound_spell_validity_check() {
    use crate::native::production_compiler::VerifiedCatalog;
    let (Some(root), Some(config)) = (
        std::env::var_os("BENILLA_F5_AI_CATALOG"),
        std::env::var_os("BENILLA_F5_EVENT_AI_CONFIG"),
    ) else {
        println!(
            "SKIP full quest catalog: set BENILLA_F5_AI_CATALOG and BENILLA_F5_EVENT_AI_CONFIG"
        );
        return;
    };
    let config = std::fs::read_to_string(config).unwrap();
    let value = |name| {
        config
            .lines()
            .filter_map(|line| line.split_once('='))
            .find_map(|(key, value)| (key.trim() == name).then(|| value.trim()))
            .unwrap()
    };
    let rules = benilla_sim::player_progression::Rules::from_classic_config(
        value("MaxPlayerLevel").parse().unwrap(),
        value("AlwaysMaxSkillForLevel").parse::<u32>().unwrap() != 0,
        value("Rate.Talent").parse::<f32>().unwrap().to_bits(),
    )
    .unwrap();
    let mut catalog = VerifiedCatalog::open(Path::new(&root), false).unwrap();
    let creatures = LoadedCreatureTemplates::catalog_diagnostic(&mut catalog).unwrap();
    let gameobjects = LoadedGameObjectTemplates::catalog_diagnostic(&mut catalog).unwrap();
    struct NoLaterOwners;
    impl SpellLoaderOwner for NoLaterOwners {}
    let spells = SourceSpellStores::load(&mut catalog, &mut NoLaterOwners).unwrap();
    // Stock SQL:782869 has RewSpell=28272 for quest 9364; the spell is at :859114.
    // ObjectMgr.cpp:4731 is its first unbound validation.
    assert!(spells.template(28272).is_some());
    let refused = SourceQuestMap::load_catalog(
        &mut catalog,
        QuestCatalogInputs {
            creatures: Some(&creatures),
            gameobjects: Some(&gameobjects),
            spells: Some(&spells),
            progression: Some(&rules),
        },
    )
    .unwrap_err();
    assert_eq!(
        refused.id,
        "execution.quests.ObjectMgr_4731.spell_validity_owner"
    );
}
