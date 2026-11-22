// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_sim::player_progression::Rules;

// Physical seal16 trace lines: request bytes, result shape, result death.
#[derive(Clone, Copy)]
struct Query {
    request: u64,
    result: u64,
    end: u64,
    rows: usize,
    cols: usize,
    logical_id: &'static str,
}

const fn query(
    request: u64,
    result: u64,
    end: u64,
    rows: usize,
    cols: usize,
    logical_id: &'static str,
) -> Query {
    Query {
        request,
        result,
        end,
        rows,
        cols,
        logical_id,
    }
}

const LOOT: [Query; 9] = [
    query(46445169, 46446443, 50870516, 151783, 7, "4294967595"),
    query(50870614, 50870628, 50875194, 155, 7, "4294967596"),
    query(50875222, 50875425, 51241179, 12547, 7, "4294967597"),
    query(51241214, 51241284, 51356922, 3966, 7, "4294967598"),
    query(51356943, 51356964, 51359210, 75, 7, "4294967599"),
    query(51359231, 51359406, 51563367, 6994, 7, "4294967600"),
    query(51563416, 51563493, 51645242, 2802, 7, "4294967601"),
    query(51645277, 51645291, 51647661, 80, 7, "4294967602"),
    query(51647696, 51648074, 52609100, 32972, 7, "4294967603"),
];
const NAMES: Query = query(52609121, 52609156, 52614740, 551, 2, "4294967604");
const NAMES_SQL: &str = "SELECT entry, name FROM `reference_loot_template_names`";
const CONDITIONS: [Query; 3] = [
    query(46007623, 46007749, 46007915, 1, 1, "4294967545"),
    query(46007780, 46007899, 46008137, 1, 1, "4294967546"),
    query(46007932, 46008072, 46049926, 1260, 8, "4294967547"),
];
const CONDITION_SQL: [&str; 3] = [
    "SELECT MAX(condition_entry) FROM conditions",
    "SELECT COUNT(*) FROM conditions",
    "SELECT * FROM conditions",
];

#[derive(Clone, Copy)]
struct TraceRow<'a> {
    line: u64,
    kind: &'a str,
    fields: &'a [&'a str],
}

#[derive(Default)]
struct Lifetime {
    requested: bool,
    token: Option<String>,
    ended: bool,
}

impl Lifetime {
    fn observe(&mut self, query: Query, sql: &str, TraceRow { line, kind, fields }: TraceRow<'_>) {
        if line == query.request {
            assert_eq!(kind, "sql_request_bytes");
            assert_eq!(unhex(fields.last().unwrap()), sql.as_bytes());
            self.requested = true;
        } else if line == query.result {
            assert!(self.requested);
            assert_eq!(kind, "sql_query_result");
            assert_eq!(fields[3].parse::<usize>().unwrap(), query.rows);
            assert_eq!(fields[4].parse::<usize>().unwrap(), query.cols);
            assert_eq!(&fields[5..], ["0"]);
            self.token = Some(fields[2].to_owned());
        } else if line == query.end {
            assert!(self.token.is_some());
            assert_eq!(kind, "sql_result_end");
            assert_eq!(fields, [query.logical_id]);
            self.ended = true;
        }
    }

    fn matches_field(&self, kind: &str, fields: &[&str]) -> bool {
        kind == "sql_field" && !self.ended && Some(fields[0]) == self.token.as_deref()
    }
}

struct RecordedRows<T> {
    lifetime: Lifetime,
    column: usize,
    words: [u32; 7],
    rows: Vec<T>,
}

impl<T> Default for RecordedRows<T> {
    fn default() -> Self {
        Self {
            lifetime: Lifetime::default(),
            column: 0,
            words: [0; 7],
            rows: Vec::new(),
        }
    }
}

fn decode_field<T: std::str::FromStr>(fields: &[&str]) -> T
where
    T::Err: std::fmt::Debug,
{
    let bytes = unhex(fields.last().unwrap());
    std::str::from_utf8(&bytes).unwrap().parse().unwrap()
}

impl<T> RecordedRows<T> {
    fn observe(
        &mut self,
        query: Query,
        sql: &str,
        event: TraceRow<'_>,
        decode: impl Fn(usize, &[&str]) -> u32,
        row: impl Fn([u32; 7]) -> T,
    ) {
        self.lifetime.observe(query, sql, event);
        let TraceRow { line, kind, fields } = event;
        if self.lifetime.matches_field(kind, fields) {
            assert_eq!(fields[1].parse::<usize>().unwrap(), self.column);
            if self.column < self.words.len() {
                self.words[self.column] = decode(self.column, fields);
            }
            self.column += 1;
            if self.column == query.cols {
                self.rows.push(row(self.words));
                self.column = 0;
            }
        }
        if line == query.end {
            assert_eq!(self.column, 0);
            assert_eq!(self.rows.len(), query.rows);
        }
    }
}

#[derive(Default)]
pub(super) struct Evidence {
    loot: [RecordedRows<LootRow>; 9],
    next_loot: usize,
    names: Lifetime,
    condition_scalars: [Lifetime; 2],
    condition_max: Option<u32>,
    condition_count: Option<u32>,
    conditions: RecordedRows<ConditionRow>,
}

impl Evidence {
    pub(super) fn observe(&mut self, line: u64, kind: &str, fields: &[&str]) {
        let event = TraceRow { line, kind, fields };
        if (CONDITIONS[0].request..=CONDITIONS[2].end).contains(&line) {
            for (index, lifetime) in self.condition_scalars.iter_mut().enumerate() {
                lifetime.observe(CONDITIONS[index], CONDITION_SQL[index], event);
                if lifetime.matches_field(kind, fields) {
                    assert_eq!(fields[1], "0");
                    let slot = if index == 0 {
                        &mut self.condition_max
                    } else {
                        &mut self.condition_count
                    };
                    assert!(slot.replace(decode_field(fields)).is_none());
                }
            }
            self.conditions.observe(
                CONDITIONS[2],
                CONDITION_SQL[2],
                event,
                |_, fields| decode_field::<i64>(fields) as u32,
                |[entry, kind, a, b, c, d, flags]| ConditionRow {
                    entry,
                    kind: kind as i32,
                    values: [a, b, c, d],
                    flags: flags as u8,
                },
            );
        } else if let Some(&query) = LOOT.get(self.next_loot) {
            let store = LootStoreKind::LOAD_ORDER[self.next_loot];
            self.loot[self.next_loot].observe(
                query,
                store.query(),
                event,
                |col, fields| {
                    if col == 2 {
                        (decode_field::<f64>(fields) as f32).to_bits()
                    } else {
                        decode_field::<i64>(fields) as u32
                    }
                },
                |[entry, item, chance, group, min, max_count, condition]| LootRow {
                    entry,
                    item,
                    chance: f32::from_bits(chance),
                    group: group as u8,
                    min_count_or_ref: min as i32,
                    max_count,
                    condition: condition as u16,
                },
            );
            if line == query.end {
                self.next_loot += 1;
            }
        } else {
            self.names.observe(NAMES, NAMES_SQL, event);
        }
    }

    // World.cpp:1120 uses validated rows, including the high u32 keys.
    fn condition_rows(&self, catalog: &mut VerifiedCatalog) -> Vec<ConditionRow> {
        assert!(self.condition_scalars.iter().all(|q| q.ended));
        assert!(self.conditions.lifetime.ended);
        let rows = catalog.execution_condition_rows().unwrap();
        assert_eq!(rows, self.conditions.rows);
        assert_eq!(self.condition_count, Some(1260));
        assert_eq!(self.condition_max, Some(5310010));
        assert_eq!(self.condition_count.unwrap() as usize, rows.len());
        assert_eq!(self.condition_max, rows.last().map(|row| row.entry));
        rows
    }

    pub(super) fn load_conditions(
        &self,
        catalog: &mut VerifiedCatalog,
        recording: &Path,
        owners: ConditionOwners<'_>,
    ) -> SourceConditions {
        let rows = self.condition_rows(catalog);
        let factions = catalog.execution_dbc_lookup_index("Faction.dbc").unwrap();
        let skills = catalog.execution_dbc_lookup_index("SkillLine.dbc").unwrap();
        let mut world_state_names = catalog
            .execution_db_script_operands("worldstate_name", ["Id"])
            .unwrap();
        world_state_names.sort_unstable();
        let conditions = SourceConditions::load(
            rows,
            &mut Validation {
                owners,
                factions,
                skills,
                world_state_names,
            },
        )
        .unwrap();
        self.assert_loaded_conditions(&conditions, recording);
        conditions
    }

    fn assert_loaded_conditions(&self, conditions: &SourceConditions, recording: &Path) {
        let log = std::fs::read_to_string(recording.join("run/console.log")).unwrap();
        assert!(log.contains(">> Loaded 1260 Condition definitions"));
        assert!(!log.contains("ObjectMgr::LoadConditions: invalid condition_entry"));
        assert!(conditions.rows().eq(&self.conditions.rows));
        assert!(conditions.get(700001).is_some() && conditions.get(5310010).is_some());
        // Stock conditions :79593-79603 and six gameobject references in trace :51218203-51218360.
        assert!((739..=744).all(|id| conditions.can_be_used_without_player(id)));
    }

    // World.cpp:1200 uses the same nine restored relations in trace order.
    pub(super) fn load(
        &self,
        catalog: &mut VerifiedCatalog,
        conditions: &SourceConditions,
        items: &SourceStartupItems,
    ) -> SourceLootStores {
        assert_eq!(self.next_loot, LOOT.len());
        assert!(self.names.ended);
        let family = SourceLootStores::load(&mut Inputs {
            evidence: self,
            catalog,
            conditions,
            items,
        })
        .unwrap();
        for (kind, templates) in LootStoreKind::LOAD_ORDER
            .into_iter()
            .zip([4237, 135, 577, 180, 75, 1569, 771, 37, 551])
        {
            let store = family.store(kind);
            assert_eq!(store.templates().count(), templates);
            assert_eq!(
                store
                    .templates()
                    .map(|(_, entry)| entry.row_count() as usize)
                    .sum::<usize>(),
                LOOT[kind as usize].rows
            );
        }
        assert_eq!(family.unused_reference_ids().len(), 551);
        let refs = family.store(LootStoreKind::Reference);
        assert_eq!(refs.get_loot_for(1).unwrap().row_count(), 17);
        let vermin = family
            .store(LootStoreKind::Creature)
            .get_loot_for(6)
            .unwrap();
        assert!(vermin.groups.is_empty());
        assert_eq!(
            vermin
                .entries
                .iter()
                .map(|row| (
                    row.item,
                    row.chance,
                    row.min_count_or_ref,
                    row.max_count,
                    row.condition
                ))
                .collect::<Vec<_>>(),
            [
                (159, 6.5536, 1, 1, 0),
                (755, 29.7889, 1, 1, 0),
                (4536, 13.4595, 1, 1, 0),
                (5364, 0.0073, 1, 1, 0),
                (60000, 9.0, -60000, 1, 0),
                (60441, 0.66, -60441, 1, 0),
            ]
        );
        for (id, count) in [(60000, 23), (60441, 5)] {
            let target = refs.get_loot_for(id).unwrap();
            assert!(target.entries.is_empty());
            let [group] = &target.groups[..] else {
                panic!("reference {id} group count")
            };
            assert!(group.explicit.is_empty());
            assert_eq!(group.equal.len(), count);
            assert!(group
                .equal
                .iter()
                .all(|row| row.min_count_or_ref == 1 && row.max_count == 1 && row.condition == 0));
        }
        family
    }
}

pub(super) fn progression(config: &[u8]) -> Rules {
    let value = |key| spawn_loader_config::assignment(config, key).unwrap().1;
    let max_level = value("MaxPlayerLevel").parse().unwrap();
    let always_max = value("AlwaysMaxSkillForLevel").parse::<u8>().unwrap();
    assert!(always_max <= 1);
    let talent_rate = value("Rate.Talent").parse::<f32>().unwrap();
    Rules::from_classic_config(max_level, always_max != 0, talent_rate.to_bits()).unwrap()
}

pub(super) struct ConditionOwners<'a> {
    pub items: &'a SourceStartupItems,
    pub spells: &'a SourceSpellStores,
    pub quests: &'a SourceQuestMap,
    pub creatures: &'a LoadedCreatureTemplates,
    pub events: &'a SourceGameEventManager,
    pub dungeon: &'a LoadedDungeonEncounters,
    pub rules: &'a Rules,
}

struct Validation<'a> {
    owners: ConditionOwners<'a>,
    factions: BTreeMap<u32, u32>,
    skills: BTreeMap<u32, u32>,
    world_state_names: Vec<[u32; 1]>,
}

impl ConditionValidationInputs for Validation<'_> {
    fn exists(&mut self, kind: ConditionDependency, id: u32) -> Result<Option<bool>> {
        use ConditionDependency::*;
        Ok(Some(match kind {
            SpellTemplate => self.owners.spells.template(id).is_some(),
            ItemPrototype => self.owners.items.item_prototype(id)?.is_some(),
            Area => self.owners.spells.dbc("AreaTable.dbc", id)?.is_some(),
            Faction => self.factions.contains_key(&id),
            SkillLine => self.skills.contains_key(&id),
            QuestTemplate => self.owners.quests.template(id).is_some(),
            GameEvent => self.owners.events.is_valid(id)?,
            LearnableAbility => return Ok(None),
            CreatureTemplate => self.owners.creatures.get(id).is_some(),
            WorldStateName => self.world_state_names.binary_search(&[id]).is_ok(),
        }))
    }

    fn progression(&self) -> Option<&Rules> {
        Some(self.owners.rules)
    }

    fn encounter_map(&mut self, id: u32) -> Result<Option<u32>> {
        Ok(self.owners.dungeon.rows().get(&id).map(|row| row.map))
    }
}

struct Inputs<'a> {
    evidence: &'a Evidence,
    catalog: &'a mut VerifiedCatalog,
    conditions: &'a SourceConditions,
    items: &'a SourceStartupItems,
}

impl LootInputs for Inputs<'_> {
    fn rows(&mut self, kind: LootStoreKind, query: &str) -> Result<Vec<LootRow>> {
        assert_eq!(query, kind.query());
        let evidence = &self.evidence.loot[kind as usize];
        assert!(evidence.lifetime.ended);
        let rows = self.catalog.execution_loot_rows(kind)?;
        let table = kind.table();
        assert_eq!(rows.len(), evidence.rows.len(), "{table} row count");
        for (index, (row, expected)) in rows.iter().zip(&evidence.rows).enumerate() {
            assert!(
                row == expected && row.chance.to_bits() == expected.chance.to_bits(),
                "{table} row {}: {row:?} != {expected:?}",
                index + 1
            );
        }
        Ok(rows)
    }

    fn condition_exists(&mut self, id: u16) -> Result<bool> {
        Ok(self.conditions.get(u32::from(id)).is_some())
    }

    fn condition_without_player(&mut self, id: u16) -> Result<bool> {
        Ok(self.conditions.can_be_used_without_player(u32::from(id)))
    }

    fn item_exists(&mut self, id: u32) -> Result<bool> {
        Ok(self.items.item_prototype(id)?.is_some())
    }
}
