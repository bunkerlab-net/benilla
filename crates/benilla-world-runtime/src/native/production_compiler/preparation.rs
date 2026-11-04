// SPDX-License-Identifier: GPL-3.0-or-later
// Independent Rust translation of native_starter/compiler.py. Preparation never admits gameplay.
use super::{
    check,
    dbc::{self, Row, Tables},
    fail,
    json::*,
    source::*,
    ProductionRequest, Result, COMPILER_CONTRACT,
};
use serde_json::{json, Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};
mod canonical_order;
mod learning;
mod learning_numeric;
pub(in crate::native) mod loot_assembly;
mod plans;
mod reached_hooks;
mod reputation;
#[cfg(test)]
mod starter_draft;
static PLAN: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("preparation/plan_contract.json"))
        .expect("reviewed plan metadata")
});
const PROFILE: &str = "cmangos-8ec338a1-native-starter-bindings-v1";
const FORMAT: &str = "benilla-native-starter-preparation-v1";
const FIREBALL_ADAPTER: &str = "captured_fireball_133_existing_spell_authority_v1";
const LOOT_ADAPTER: &str = "benilla_sim::loot::captured::CapturedLoot::compile";
fn num(n: i128) -> Value {
    if n < 0 {
        json!(n as i64)
    } else {
        json!(n as u64)
    }
}
fn numbers(set: impl IntoIterator<Item = i128>) -> Value {
    json!(set.into_iter().map(num).collect::<Vec<_>>())
}
fn nonzero(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::String(s) => !s.is_empty(),
        Value::Number(_) => int(v) != 0,
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}
fn filtered(b: &Value, table: &str, key: &str, id: i128) -> Value {
    json!(rows(b, table)
        .iter()
        .filter(|r| int(&r[key]) == id)
        .collect::<Vec<_>>())
}
fn numeric_scope(b: &Value, id: i128) -> Value {
    if let Some(v) = b.get("spell_numeric_capture") {
        if let Some(row) = v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| int(&r["Id"]) == id)
        {
            return json!({"status":"complete","row":row,"facing":v["facing"].as_array().unwrap().iter().filter(|r|int(&r["entry"])==id).collect::<Vec<_>>(),"coverage_sha256":digest(v)});
        }
    }
    Value::Null
}
fn scoped(b: &Value, id: i128, extension: &str, table: &str, key: &str) -> Value {
    b.get(extension).map_or(Value::Null,|v|json!({"status":"complete","rows":filtered(b,table,key,id),"coverage_sha256":digest(v)}))
}
struct SelectedRow {
    raw: Value,
    used: BTreeSet<usize>,
}
struct Compiler<'a> {
    source: &'a Value,
    source_sha: &'a str,
    dbcs: &'a Tables,
    request: ProductionRequest,
    issues: Vec<Value>,
    dependencies: BTreeSet<(String, i128, String)>,
    selected: BTreeMap<(&'a str, u32), SelectedRow>,
    binding_bytes: usize,
}
impl<'a> Compiler<'a> {
    fn issue(&mut self, table: &str, key: Value, reason: &str) {
        self.issues
            .push(json!({"table":table,"key":key,"reason":reason}));
    }
    fn context_issue(&mut self, table: &str, key: Value, reason: &str, context: Value) {
        let mut issue = json!({"table":table,"key":key,"reason":reason});
        for (n, v) in context.as_object().unwrap() {
            issue[n] = v.clone();
        }
        self.issues.push(issue);
    }
    fn need(&mut self, table: &str, id: i128, reason: &str) {
        self.dependencies.insert((table.into(), id, reason.into()));
        self.issue(table, json!([num(id)]), reason);
    }
    fn use_row(
        &mut self,
        row: Row<'a>,
        columns: impl IntoIterator<Item = usize>,
    ) -> Result<Row<'a>> {
        let id = if row.table.name == "CharBaseInfo" {
            row.word(0) << 8 | row.word(1)
        } else {
            row.word(0)
        };
        let key = (row.table.name.as_str(), id);
        match self.selected.entry(key) {
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                entry.get_mut().used.extend(columns);
            }
            std::collections::btree_map::Entry::Vacant(entry) => {
                let raw = row.raw();
                self.binding_bytes += bounded_len(
                    &raw,
                    32 * 1024 * 1024 - self.binding_bytes,
                    "compiler.dbc_binding_byte_limit",
                )?;
                entry.insert(SelectedRow {
                    raw,
                    used: columns.into_iter().collect(),
                });
            }
        }
        Ok(row)
    }
    fn lookup(
        &mut self,
        table: &str,
        id: i128,
        columns: impl IntoIterator<Item = usize>,
    ) -> Result<Option<Row<'a>>> {
        let row = u32::try_from(id)
            .ok()
            .and_then(|id| self.dbcs.get(table).and_then(|t| t.get(id)));
        if let Some(row) = row {
            self.use_row(row, columns)?;
        } else {
            self.need(table, id, "dbc.required_record_missing");
        }
        Ok(row)
    }
    fn identity(&mut self) -> Result<Value> {
        let s = self.request;
        let creation = &rows(self.source, "playercreateinfo")[0];
        let race = self.lookup(
            "ChrRaces",
            s.race as i128,
            [0, 1, 2, 4, 5, 8, 14, 15, 16].into_iter().chain(17..25),
        )?;
        let class = self.lookup(
            "ChrClasses",
            s.class as i128,
            [0, 3, 14, 15].into_iter().chain(5..13),
        )?;
        let map = self.lookup("Map", int(&creation["map"]), [0, 1, 2, 19])?;
        let pair = self
            .dbcs
            .get("CharBaseInfo")
            .and_then(|t| t.get(((s.race as u32) << 8) | s.class as u32));
        if let Some(pair) = pair {
            self.use_row(pair, [0, 1])?;
        } else {
            self.issue(
                "CharBaseInfo",
                json!([s.race, s.class]),
                "player.race_class_pair_missing",
            );
        }
        if race.is_some_and(|r| r.word(1) & 1 != 0) {
            self.issue("ChrRaces", json!([s.race]), "player.race_not_playable");
        }
        if class.is_some_and(|r| ![0, 1, 3].contains(&r.word(3))) {
            self.issue(
                "ChrClasses",
                json!([s.class]),
                "player.power_type_unsupported",
            );
        }
        if map.is_some_and(|r| r.word(2) != 0) {
            self.issue(
                "Map",
                json!([creation["map"]]),
                "player.instance_start_unsupported",
            );
        }
        Ok(
            json!({"selection":s.json(),"start":creation,"display_id":race.map(|r|r.word(4+s.gender as usize)),"faction_template_id":race.map(|r|r.word(2)),"power_type":class.map(|r|r.word(3)),"spell_family":class.map(|r|r.word(15)),"class_file":class.map(|r|r.text(14)),"race_file":race.map(|r|r.text(15)),"map_directory":map.map(|r|r.text(1)),"race_class_pair_found":pair.is_some()}),
        )
    }
    fn progression(&self) -> Value {
        let levels: BTreeMap<_, _> = rows(self.source, "player_levelstats")
            .iter()
            .map(|r| (int(&r["level"]), r))
            .collect();
        let xp: BTreeMap<_, _> = rows(self.source, "player_xp_for_level")
            .iter()
            .map(|r| (int(&r["lvl"]), &r["xp_for_next_level"]))
            .collect();
        json!(rows(self.source,"player_classlevelstats").iter().map(|r|{let stats=levels[&int(&r["level"])];json!({"level":r["level"],"create_health":r["basehp"],"create_mana":r["basemana"],"strength":stats["str"],"agility":stats["agi"],"stamina":stats["sta"],"intellect":stats["inte"],"spirit":stats["spi"],"xp_for_next_level":xp.get(&int(&r["level"]))})}).collect::<Vec<_>>())
    }
    fn outfit(&mut self) -> Result<Value> {
        let s = self.request;
        let packed = s.race as u32 | ((s.class as u32) << 8) | ((s.gender as u32) << 16);
        let selected: Vec<_> = self.dbcs["CharStartOutfit"]
            .by_id
            .values()
            .map(|i| self.dbcs["CharStartOutfit"].row(*i))
            .filter(|r| r.word(0) != 0 && r.word(1) == packed)
            .collect();
        let items: BTreeMap<_, _> = rows(self.source, "item_template")
            .iter()
            .map(|r| (int(&r["entry"]), r))
            .collect();
        if selected.len() != 1 {
            self.issue(
                "CharStartOutfit",
                json!([s.race, s.class, s.gender]),
                if selected.is_empty() {
                    "player.outfit_missing"
                } else {
                    "player.outfit_ambiguous"
                },
            );
        }
        for row in &selected {
            self.use_row(*row, 0..14)?;
        }
        let mut grants = Vec::new();
        if selected.len() == 1 {
            for slot in 0..12 {
                let id = selected[0].word(slot + 2) as i32 as i128;
                if id <= 0 {
                    continue;
                }
                let count = if let Some(item) = items.get(&id) {
                    let count = int(&item["BuyCount"]);
                    if !(1..=0x7fffffff).contains(&count) {
                        self.issue(
                            "item_template",
                            json!([num(id)]),
                            "item.outfit_quantity_outside_domain",
                        );
                        Value::Null
                    } else {
                        let count = if int(&item["class"]) == 0 && int(&item["subclass"]) == 5 {
                            if int(&item["spellcategory_1"]) == 11 && int(&item["stackable"]) > 4 {
                                4
                            } else if int(&item["spellcategory_1"]) == 59
                                && int(&item["stackable"]) > 2
                            {
                                2
                            } else {
                                count
                            }
                        } else {
                            count
                        };
                        num(count)
                    }
                } else {
                    self.need("item_template", id, "player.outfit_item_not_captured");
                    Value::Null
                };
                grants.push(json!({"source":"CharStartOutfit","source_slot":slot,"item_id":num(id),"count":count}));
            }
        }
        for (slot, item) in canonical_order::collection(self.source, "playercreateinfo_item")
            .into_iter()
            .enumerate()
        {
            grants.push(json!({"source":"playercreateinfo_item","source_slot":slot,"item_id":item["itemid"],"count":item["amount"]}));
        }
        let (plan, reasons) = if selected.len() == 1 {
            plans::outfit(&grants, &items)
        } else {
            (Value::Null, Vec::new())
        };
        for reason in reasons {
            self.issue(
                "item_template",
                json!([reason.get("item_id").cloned().unwrap_or(json!(0))]),
                reason["reason"].as_str().unwrap(),
            );
        }
        // A shape plan does not execute learned eligibility or actual placement.
        self.issue(
            "playercreateinfo_item",
            json!([s.race, s.class]),
            "player.native_equipment_placement_required",
        );
        Ok(
            json!({"outfit_record_id":if selected.len()==1{Some(selected[0].word(0))}else{None},"grants_in_source_order":grants,"equipment_slots":null,"placement_plan":plan}),
        )
    }
    fn factions(&mut self, identity: &Value, rules: &Value) -> Result<Value> {
        let start = self.issues.len();
        let mut templates = ids(self.source, "creature_template", "Faction");
        if !identity["faction_template_id"].is_null() {
            templates.insert(int(&identity["faction_template_id"]));
        } else {
            self.issue(
                "ChrRaces",
                json!([self.request.race]),
                "player.initial_reputation_identity_missing",
            );
        }
        for id in &templates {
            if let Some(row) = self.lookup("FactionTemplate", *id, 0..14)? {
                let referenced: BTreeSet<_> = [row.word(1)]
                    .into_iter()
                    .chain((6..14).map(|i| row.word(i)))
                    .filter(|n| *n != 0)
                    .collect();
                for f in referenced {
                    self.lookup("Faction", f as i128, 0..19)?;
                }
            }
        }
        let mut required = ids(self.source, "item_template", "RequiredReputationFaction");
        for quest in rows(self.source, "quest_template") {
            for field in [
                "RepObjectiveFaction",
                "RequiredMinRepFaction",
                "RequiredMaxRepFaction",
            ]
            .into_iter()
            .map(String::from)
            .chain((1..=5).map(|i| format!("RewRepFaction{i}")))
            {
                required.insert(int(&quest[field]));
            }
        }
        required.remove(&0);
        for f in required {
            self.lookup("Faction", f, 0..19)?;
        }
        let table = &self.dbcs["Faction"];
        let mut entries = Vec::new();
        let mut indices = BTreeSet::new();
        if table.count > 4096 || templates.len() > 4096 {
            self.issue("Faction", json!([]), "player.initial_reputation_row_limit");
        } else {
            let race_mask = 1u32 << (self.request.race - 1);
            let class_mask = 1u32 << (self.request.class - 1);
            for index in 0..table.count {
                let row = self.use_row(table.row(index), 0..19)?;
                let id = row.word(0);
                let list_id = row.word(1) as i32;
                if id == 0 || !(-1..64).contains(&list_id) {
                    self.issue(
                        "Faction",
                        json!([id]),
                        "player.initial_reputation_index_or_id",
                    );
                }
                if (14..18).any(|i| row.word(i) & !0x7f != 0) {
                    self.issue(
                        "Faction",
                        json!([id]),
                        "player.initial_reputation_flag_bits",
                    );
                }
                let last = (0..4)
                    .filter(|i| {
                        [2, 6, 10, 14]
                            .iter()
                            .any(|offset| row.word(offset + i) != 0)
                    })
                    .max()
                    .unwrap_or(0);
                let matched: Vec<_> = (0..4)
                    .filter(|i| {
                        (row.word(2 + i) == 0 || row.word(2 + i) & race_mask != 0)
                            && (row.word(6 + i) == 0 || row.word(6 + i) & class_mask != 0)
                    })
                    .collect();
                let values: BTreeSet<_> = matched
                    .iter()
                    .filter(|i| **i <= last)
                    .map(|i| (row.word(10 + i), row.word(14 + i)))
                    .collect();
                if values.len() > 1 {
                    self.issue(
                        "Faction",
                        json!([id]),
                        "player.initial_reputation_conflicting_slots",
                    );
                }
                if list_id < 0 {
                    continue;
                }
                if !indices.insert(list_id) {
                    self.issue(
                        "Faction",
                        json!([id]),
                        "player.initial_reputation_duplicate_index",
                    );
                }
                let slot = matched.first().copied();
                entries.push(json!({"faction":id,"reputation_list_id":list_id,"matched_slot":slot,"base_standing":slot.map_or(0,|s|row.word(10+s) as i32),"stored_delta":0,"flags":slot.map_or(0,|s|row.word(14+s)),"need_send":true,"need_save":true}));
            }
        }
        let complete = self.issues.len() == start;
        if !complete {
            self.issue(
                "Faction",
                json!([]),
                "player.native_reputation_initialization_required",
            );
        }
        entries.sort_by_key(|r| int(&r["reputation_list_id"]));
        Ok(
            json!({"status":if complete{"supported"}else{"blocked"},"coverage":"complete_raw_table","table_sha256":table.sha256,"initial_standings":if complete{json!(entries)}else{Value::Null},"aura156_affects_initial_standing":false,"reward_rates_and_spillover":reputation::supported(self.source, table, rules)?}),
        )
    }
    fn spells(&mut self) -> Result<Value> {
        let mut result = Vec::new();
        let items = ids(self.source, "item_template", "entry");
        let initial = ids(self.source, "playercreateinfo_spell", "Spell");
        for source in rows(self.source, "spell_template") {
            let id = int(&source["Id"]);
            let numeric = numeric_scope(self.source, id);
            let effective = if numeric.is_null() {
                source
            } else {
                &numeric["row"]
            };
            let mapped = if numeric.is_null() {
                &COMPILER_CONTRACT["spell_fields"]
            } else {
                &COMPILER_CONTRACT["numeric_words"]
            };
            let floats = names(
                &COMPILER_CONTRACT[if numeric.is_null() {
                    "float_spell_fields"
                } else {
                    "numeric_floats"
                }],
            );
            let order = &PLAN[if numeric.is_null() {
                "spell_field_order"
            } else {
                "numeric_word_order"
            }];
            let used = mapped
                .as_object()
                .unwrap()
                .values()
                .map(|v| v.as_u64().unwrap() as usize)
                .chain(40..58)
                .chain([159, 161, 162, 163, 167, 168, 169]);
            let row = self.lookup("Spell", id, used)?;
            let mut differences = Vec::new();
            let mut server_words = row.map(|r| (0..r.len()).map(|i| r.word(i)).collect::<Vec<_>>());
            if let Some(r) = row {
                for name in order.as_array().unwrap() {
                    let name = name.as_str().unwrap();
                    let column = mapped[name].as_u64().unwrap() as usize;
                    let value = &effective[name];
                    let bits = if floats.contains(name) {
                        check(
                            value
                                .as_object()
                                .is_some_and(|o| o.len() == 1 && o.contains_key("decimal")),
                            "spell.source_float_domain",
                        )?;
                        dbc::finite(r.word(column), "spell.dbc_nonfinite")?;
                        dbc::binary32(value["decimal"].as_str().unwrap())?
                    } else {
                        integer(
                            value,
                            i32::MIN as i128,
                            u32::MAX as i128,
                            "spell.source_integer_domain",
                        )? as u32
                    };
                    let matches = bits == r.word(column)
                        || (floats.contains(name)
                            && bits & 0x7fffffff == 0
                            && r.word(column) & 0x7fffffff == 0);
                    server_words.as_mut().unwrap()[column] = bits;
                    if !matches {
                        differences.push(json!(name));
                        self.context_issue(
                            "spell_template",
                            json!([num(id)]),
                            "spell.server_client_field_mismatch",
                            json!({"field":name,"sql_value":value,"dbc_word":r.word(column)}),
                        );
                    }
                }
                if !numeric.is_null() {
                    let family = int(&effective["SpellFamilyFlags"]) as u64;
                    for (column, bits) in [(161, family as u32), (162, (family >> 32) as u32)] {
                        server_words.as_mut().unwrap()[column] = bits;
                        if bits != r.word(column) {
                            differences.push(json!("SpellFamilyFlags"));
                            self.context_issue("spell_template",json!([num(id)]),"spell.server_client_field_mismatch",json!({"field":"SpellFamilyFlags","sql_value":effective["SpellFamilyFlags"],"dbc_value":r.word(column)}));
                        }
                    }
                }
            }
            let cast = if nonzero(&source["CastingTimeIndex"]) {
                self.lookup("SpellCastTimes", int(&source["CastingTimeIndex"]), 0..4)?
            } else {
                None
            };
            let range = self.lookup("SpellRange", int(&source["rangeIndex"]), 0..4)?;
            if nonzero(&source["Category"]) {
                self.lookup("SpellCategory", int(&source["Category"]), 0..2)?;
            }
            let duration = if nonzero(&source["DurationIndex"])
                || self.dbcs["SpellDuration"].get(0).is_some()
            {
                self.lookup("SpellDuration", int(&source["DurationIndex"]), 0..4)?
            } else {
                None
            };
            if nonzero(&source["RequiresSpellFocus"]) {
                self.lookup("SpellFocusObject", int(&source["RequiresSpellFocus"]), [0])?;
            }
            for slot in 1..=3 {
                if nonzero(&source[format!("EffectRadiusIndex{slot}")]) {
                    self.lookup(
                        "SpellRadius",
                        int(&source[format!("EffectRadiusIndex{slot}")]),
                        0..4,
                    )?;
                }
                if nonzero(&source[format!("EffectItemType{slot}")]) {
                    self.context_issue(
                        "spell_template",
                        json!([num(id)]),
                        "spell.effect_item_type_semantics_required",
                        json!({"slot":slot,"field_value":source[format!("EffectItemType{slot}")]}),
                    );
                }
            }
            if let Some(r) = row {
                let references: BTreeSet<_> = (40..42)
                    .map(|i| r.word(i) as i128)
                    .chain((42..50).map(|i| r.word(i) as i32 as i128))
                    .collect();
                for item in references {
                    if item > 0 && !items.contains(&item) {
                        self.need(
                            "item_template",
                            item,
                            "spell.dbc_reagent_or_totem_not_captured",
                        );
                    }
                }
            }
            let range_input = if let Some(r) = range {
                let low = dbc::finite(r.word(1), "spell.range_nonfinite")?;
                let high = dbc::finite(r.word(2), "spell.range_nonfinite")?;
                let l = if low & 0x7fffffff == 0 { 0 } else { low };
                let h = if high & 0x7fffffff == 0 { 0 } else { high };
                if l & 0x80000000 != 0 || h & 0x80000000 != 0 || l > h {
                    self.issue(
                        "SpellRange",
                        json!([source["rangeIndex"]]),
                        "spell.range_domain",
                    );
                }
                json!({"id":source["rangeIndex"],"minimum_binary32":low,"maximum_binary32":high,"flags":r.word(3)})
            } else {
                Value::Null
            };
            let effect_names = [
                "Effect",
                "EffectDieSides",
                "EffectBaseDice",
                "EffectBasePoints",
                "EffectImplicitTargetA",
                "EffectImplicitTargetB",
                "EffectApplyAuraName",
                "EffectAmplitude",
                "EffectMiscValue",
                "EffectItemType",
                "EffectTriggerSpell",
                "EffectBonusCoefficient",
                "EffectBonusCoefficientFromAP",
            ];
            let effects: Vec<Value> = (1..=3)
                .map(|slot| {
                    Value::Object(
                        effect_names
                            .iter()
                            .map(|n| ((*n).into(), source[format!("{n}{slot}")].clone()))
                            .collect(),
                    )
                })
                .collect();
            for (index, effect) in effects.iter().enumerate() {
                let slot = index + 1;
                if nonzero(&effect["Effect"]) {
                    let branch = match int(&effect["Effect"]) {
                        2 => "school_damage",
                        6 => "apply_aura",
                        10 => "heal",
                        _ => "other",
                    };
                    self.context_issue(
                        "spell_template",
                        json!([num(id)]),
                        "spell.effect_execution_required",
                        json!({"slot":slot,"effect_id":effect["Effect"],"branch":branch}),
                    );
                }
                if nonzero(&effect["EffectImplicitTargetA"])
                    || nonzero(&effect["EffectImplicitTargetB"])
                {
                    self.context_issue("spell_template",json!([num(id)]),"spell.target_selector_execution_required",json!({"slot":slot,"target_a":effect["EffectImplicitTargetA"],"target_b":effect["EffectImplicitTargetB"]}));
                }
            }
            for field in [
                "Attributes",
                "AttributesEx",
                "AttributesEx2",
                "AttributesEx3",
                "AttributesEx4",
                "AttributesServerside",
            ] {
                if nonzero(&source[field]) {
                    self.context_issue(
                        "spell_template",
                        json!([num(id)]),
                        "spell.attribute_bits_execution_required",
                        json!({"field":field,"bits":source[field]}),
                    );
                }
            }
            if source["speed"]["decimal"] != "0" {
                self.issue(
                    "spell_template",
                    json!([num(id)]),
                    "spell.projectile_timing_execution_required",
                );
            }
            let registry = scoped(
                self.source,
                id,
                "spell_script_capture",
                "spell_scripts",
                "Id",
            );
            if registry.is_null() {
                self.issue(
                    "spell_template",
                    json!([num(id)]),
                    "spell.registry_coverage_not_proven",
                );
            } else if !registry["rows"].as_array().unwrap().is_empty() {
                self.issue(
                    "spell_scripts",
                    json!([num(id)]),
                    "spell.registry_row_requires_named_handler_semantics",
                );
            }
            let hooks=self.source.get("initial_hook_capture").map_or(Value::Null,|v|json!({"status":"complete","coverage_sha256":digest(v),"spell_area":filtered(self.source,"spell_area","spell",id),"spell_proc_event":filtered(self.source,"spell_proc_event","entry",id)}));
            let cast_input=cast.map_or(Value::Null,|r|json!({"id":source["CastingTimeIndex"],"base_ms":r.word(1) as i32,"per_level_ms":r.word(2) as i32,"minimum_ms":r.word(3) as i32}));
            let cast_input = if !nonzero(&source["CastingTimeIndex"]) {
                json!({"id":0,"base_ms":0,"per_level_ms":0,"minimum_ms":0})
            } else {
                cast_input
            };
            let duration_input=duration.map_or(Value::Null,|r|json!({"id":source["DurationIndex"],"base_ms":r.word(1) as i32,"per_level_ms":r.word(2) as i32,"maximum_ms":r.word(3) as i32}));
            let fireball = plans::fireball(
                self.source,
                id,
                row,
                &cast_input,
                &range_input,
                &duration_input,
            )?;
            result.push(json!({"id":num(id),"initial_sql_spell":initial.contains(&id),"script_registry":registry,"initial_hooks":hooks,"name_locales":row.map(|r|(120..128).map(|i|r.text(i)).collect::<Vec<_>>()),"cast_time":cast_input,"range":range_input,"effects":effects,"speed":source["speed"],"duration":duration_input,"server_client_differences":differences,"source_row":source,"server_numeric_words":server_words,"dbc_only_inputs":row.map(|r|json!({"totems":(40..42).map(|i|r.word(i)).collect::<Vec<_>>(),"reagents":(42..50).map(|i|r.word(i) as i32).collect::<Vec<_>>(),"reagent_counts":(50..58).map(|i|r.word(i)).collect::<Vec<_>>(),"max_target_level":r.word(159),"family_mask_words":[r.word(161),r.word(162)],"max_affected_targets":r.word(163),"damage_multiplier_binary32":(167..170).map(|i|r.word(i)).collect::<Vec<_>>()})),"numeric_capture":numeric,"fireball_plan":fireball}));
            self.issue(
                "spell_template",
                json!([num(id)]),
                "spell.server_overlay_and_learning_closure_required",
            );
        }
        for index in 0..result.len() {
            let plan =
                plans::spell_execution::supported_plan(self.source, &result[index], &result)?;
            result[index]["execution_plan"] = plan;
        }
        Ok(json!(result))
    }
    fn creatures(&mut self, rules: &Value) -> Result<Value> {
        plans::creatures(self, rules)
    }
    fn paths(&self, spells: &Value) -> Value {
        let spell_candidates: Vec<_> = spells
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| {
                s["initial_sql_spell"] == true
                    && s["effects"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|e| [2, 10].contains(&int(&e["Effect"])))
            })
            .map(|s| s["id"].clone())
            .collect();
        let mut paths = Vec::new();
        for q in rows(self.source, "quest_template") {
            let objectives:Vec<_>=(1..=4).filter(|i|nonzero(&q[format!("ReqCreatureOrGOId{i}")])||nonzero(&q[format!("ReqCreatureOrGOCount{i}")])).map(|i|json!({"slot":i,"entry":q[format!("ReqCreatureOrGOId{i}")],"count":q[format!("ReqCreatureOrGOCount{i}")],"spell":q[format!("ReqSpellCast{i}")]})).collect();
            let items:Vec<_>=(1..=4).filter(|i|nonzero(&q[format!("ReqItemId{i}")])||nonzero(&q[format!("ReqItemCount{i}")])).map(|i|json!({"slot":i,"item":q[format!("ReqItemId{i}")],"count":q[format!("ReqItemCount{i}")]})).collect();
            let starts: BTreeSet<_> = rows(self.source, "creature_questrelation")
                .iter()
                .filter(|r| r["quest"] == q["entry"])
                .map(|r| int(&r["id"]))
                .collect();
            let finishes: BTreeSet<_> = rows(self.source, "creature_involvedrelation")
                .iter()
                .filter(|r| r["quest"] == q["entry"])
                .map(|r| int(&r["id"]))
                .collect();
            let targets: BTreeSet<_> = objectives.iter().map(|o| int(&o["entry"])).collect();
            let spawn_ids = |set: &BTreeSet<i128>| {
                rows(self.source, "creature")
                    .iter()
                    .filter(|r| set.contains(&int(&r["id"])))
                    .map(|r| r["guid"].clone())
                    .collect::<Vec<_>>()
            };
            paths.push(json!({"quest_id":q["entry"],"title":q["Title"],"giver_spawn_guids":spawn_ids(&starts),"finisher_spawn_guids":spawn_ids(&finishes),"target_spawn_guids":spawn_ids(&targets),"objectives":objectives,"initial_direct_effect_spell_ids":spell_candidates,"item_objectives":items,"previous_quest":q["PrevQuestId"],"next_quest":q["NextQuestId"],"next_in_chain":q["NextQuestInChain"],"eligibility":"not_evaluated","runtime_profile":null}));
        }
        json!(paths)
    }
    fn dispositions(&self, relationships: bool, learning: bool, creatures: &Value) -> Vec<Value> {
        let complete: BTreeMap<_, _> = creatures
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["generation_plan"]["status"] == "complete_inputs")
            .map(|c| (int(&c["entry"]), &c["generation_plan"]))
            .collect();
        let mut result = Vec::new();
        for (table, rs) in self.source["tables"].as_object().unwrap() {
            for row in rs.as_array().unwrap() {
                let mut consumed = COMPILER_CONTRACT["consumed"]
                    .get(table)
                    .map(names)
                    .unwrap_or_default();
                if table == "quest_template" && relationships {
                    consumed.extend([
                        "entry",
                        "PrevQuestId",
                        "NextQuestId",
                        "NextQuestInChain",
                        "ExclusiveGroup",
                        "BreadcrumbForQuestId",
                    ]);
                }
                if table == "creature_template" && complete.contains_key(&int(&row["Entry"])) {
                    consumed.extend(names(&PLAN["creature_fields"]));
                }
                if table == "creature_template_classlevelstats"
                    && complete
                        .values()
                        .any(|p| p["class_level_rows"].as_array().unwrap().contains(row))
                {
                    consumed = row
                        .as_object()
                        .unwrap()
                        .keys()
                        .map(String::as_str)
                        .collect();
                }
                if plans::spawn::consumes(table, row, creatures) {
                    consumed = row
                        .as_object()
                        .unwrap()
                        .keys()
                        .map(String::as_str)
                        .collect();
                }
                let learning_table = [
                    "playercreateinfo_skills",
                    "playercreateinfo_spell",
                    "spell_chain",
                    "spell_learn_spell",
                ]
                .contains(&table.as_str());
                if learning_table && !learning {
                    consumed.clear();
                }
                let comments: BTreeSet<_> = ["Note", "note", "comments"]
                    .into_iter()
                    .filter(|n| row.get(*n).is_some())
                    .collect();
                let blocked: BTreeSet<_> = row
                    .as_object()
                    .unwrap()
                    .keys()
                    .map(String::as_str)
                    .filter(|n| !consumed.contains(n) && !comments.contains(n))
                    .collect();
                let mut reason = COMPILER_CONTRACT["blocked"][table]
                    .as_str()
                    .unwrap_or("player.native_creation_progression_rules_required");
                if table == "creature" && self.source.get("native_spawn_capture").is_some() {
                    reason = "creature.native_spawn_resolution_required";
                }
                if table == "creature_template" && complete.contains_key(&int(&row["Entry"])) {
                    reason = "creature.template_lifecycle_rules_required";
                }
                let mut v = json!({"table":table,"key":keys(table).iter().map(|k|row[*k].clone()).collect::<Vec<_>>(),"capability":PROFILE,"consumed_by_preparation":consumed,"inert_source_annotations":comments,"blocked_for_gameplay":{}});
                if !blocked.is_empty() {
                    v["blocked_for_gameplay"][reason] = json!(blocked);
                }
                if learning_table {
                    v["loaded_learning_maps_resolved"] = json!(learning);
                    v["effective_learning_proven"] = json!(false);
                    v["remaining_obligation"] =
                        json!("player.native_skill_obtainability_and_learning_required");
                }
                result.push(v);
            }
        }
        result
    }
}

pub(super) fn validate_rules(value: &Value) -> Result<()> {
    let fields_exact = |v: &Value, expected: &[&str]| {
        fields(v, expected, "world_rules.unsupported_or_missing_key")
    };
    fields_exact(
        value,
        &[
            "version",
            "start_player_level",
            "start_player_money",
            "progression",
            "kill",
            "quest_xp_rate_bits",
            "reputation",
            "loot",
            "combat",
        ],
    )?;
    integer(&value["version"], 2, 2, "world_rules.version")?;
    integer(
        &value["start_player_level"],
        1,
        1,
        "world_rules.StartPlayerLevel_other_than_one",
    )?;
    integer(
        &value["start_player_money"],
        0,
        0x7ffffffe,
        "world_rules.StartPlayerMoney",
    )?;
    let p = &value["progression"];
    fields_exact(
        p,
        &[
            "max_player_level",
            "always_max_skill_for_level",
            "global_skill_max",
            "talent_rate_bits",
        ],
    )?;
    let cap = integer(
        &p["max_player_level"],
        1,
        60,
        "world_rules.MaxPlayerLevel_outside_1_60",
    )?;
    check(
        p["always_max_skill_for_level"].is_boolean(),
        "world_rules.AlwaysMaxSkillForLevel",
    )?;
    integer(
        &p["global_skill_max"],
        cap * 5,
        cap * 5,
        "world_rules.global_skill_max",
    )?;
    fields_exact(&value["kill"], &["kill_rate_bits"])?;
    fields_exact(
        &value["reputation"],
        &["gain_rate", "low_level_kill_rate", "low_level_quest_rate"],
    )?;
    let loot = &value["loot"];
    fields_exact(
        loot,
        &[
            "quality_rate_bits",
            "quest_drop_rate",
            "referenced_rate_bits",
            "money_rate_bits",
            "corpse_empty_loot_show",
        ],
    )?;
    fields_exact(&loot["quest_drop_rate"], &["multiplier_bits"])?;
    check(
        loot["corpse_empty_loot_show"].is_boolean(),
        "world_rules.Corpse.EmptyLootShow",
    )?;
    let quality = array(&loot["quality_rate_bits"], "world_rules.quality_rate_bits")?;
    check(quality.len() == 7, "world_rules.quality_rate_bits")?;
    let rates = [
        &p["talent_rate_bits"],
        &value["kill"]["kill_rate_bits"],
        &value["quest_xp_rate_bits"],
        &value["reputation"]["gain_rate"],
        &value["reputation"]["low_level_kill_rate"],
        &value["reputation"]["low_level_quest_rate"],
        &loot["quest_drop_rate"]["multiplier_bits"],
        &loot["referenced_rate_bits"],
        &loot["money_rate_bits"],
    ];
    for bits in rates.into_iter().chain(quality) {
        let n = integer(
            bits,
            0,
            0x447a0000,
            "world_rules.rate_outside_bounded_binary32_domain",
        )?;
        check(
            n == 0 || n >= 0x00800000,
            "world_rules.rate_outside_bounded_binary32_domain",
        )?;
    }
    let combat = &value["combat"];
    let scalar_fields = [
        "aggro_rate_bits",
        "family_assistance_radius_bits",
        "family_flee_assistance_radius_bits",
        "check_for_help_radius_bits",
        "leash_radius_bits",
        "listen_say_range_bits",
        "listen_yell_range_bits",
        "listen_text_emote_range_bits",
    ];
    let delay_fields = [
        "family_assistance_delay_ms",
        "family_flee_delay_ms",
        "check_for_help_aggro_delay_ms",
        "linking_aggro_delay_ms",
        "respawn_aggro_delay_ms",
        "relocation_notify_delay_ms",
    ];
    fields_exact(
        combat,
        &[
            "aggro_rate_bits",
            "family_assistance_radius_bits",
            "family_assistance_delay_ms",
            "family_flee_assistance_radius_bits",
            "family_flee_delay_ms",
            "check_for_help_radius_bits",
            "check_for_help_aggro_delay_ms",
            "linking_aggro_delay_ms",
            "respawn_aggro_delay_ms",
            "relocation_notify_delay_ms",
            "leash_radius_bits",
            "listen_say_range_bits",
            "listen_yell_range_bits",
            "listen_text_emote_range_bits",
        ],
    )?;
    for field in scalar_fields {
        let bits = integer(
            &combat[field],
            0,
            0x447a0000,
            "world_rules.combat_scalar_outside_bounded_binary32_domain",
        )?;
        check(
            bits == 0 || bits >= 0x00800000,
            "world_rules.combat_scalar_outside_bounded_binary32_domain",
        )?;
    }
    for field in delay_fields {
        integer(
            &combat[field],
            0,
            0xffffffff,
            "world_rules.combat_delay_outside_uint32_domain",
        )?;
    }
    let rules: benilla_sim::world_rules::NativeWorldRules =
        serde_json::from_value(value.clone())
            .map_err(|_| fail("world_rules.unsupported_or_missing_key"))?;
    rules.validate().map_err(|_| fail("world_rules.invalid"))?;
    Ok(())
}
fn rules_plan(source: &Value, rules: &Value) -> Value {
    let rate = f32::from_bits(int(&rules["quest_xp_rate_bits"]) as u32);
    let money_rate = f32::from_bits(int(&rules["loot"]["money_rate_bits"]) as u32);
    let mut quests = Vec::new();
    let mut refusals = Vec::new();
    for row in rows(source, "quest_template") {
        let level = int(&row["QuestLevel"]);
        let money = int(&row["RewMoneyMaxLevel"]);
        let upper = ((money as f32) / (0.6f32)).ceil() as f64;
        if !(0..=255).contains(&level)
            || !(0..=0xffffffff).contains(&money)
            || upper >= 4294967296.0
            || (upper * rate as f64) as f32 >= 4294967296.0
            || (money as f32 * money_rate) >= 2147483648.0
        {
            refusals.push(json!({"quest":row["entry"],"reason":"quest_xp_numeric_domain"}));
        } else {
            quests.push(json!({"quest":row["entry"],"quest_level":row["QuestLevel"],"reward_money_max_level":row["RewMoneyMaxLevel"]}));
        }
    }
    json!({"status":"explicit_policy","rules":rules,"input_binding":digest(rules),"quest_xp_inputs":quests,"refusals":refusals,"independent_obligations":["consecutive_level_stat_and_xp_rows","effective_initialization","kill_source_tap_and_rest","reputation_sql_reward_rates_spillover_and_auras","loot_loader_item_and_source_closure","production_admission"]})
}
fn rules_discharges(issue: &Value, plan: &Value) -> bool {
    if issue["reason"] == "player.start_configuration_not_captured" {
        return issue["table"] == "playercreateinfo";
    }
    issue["reason"] == "quest.dynamic_xp"
        && issue["table"] == "quest_template"
        && plan["quest_xp_inputs"].as_array().unwrap().iter().any(|q| {
            issue["key"]
                .as_array()
                .is_some_and(|key| key.as_slice() == std::slice::from_ref(&q["quest"]))
                || issue["key"]
                    .as_str()
                    .and_then(|key| key.parse::<i128>().ok())
                    == Some(int(&q["quest"]))
        })
}

pub(super) fn compile(
    source: &Value,
    dbcs: &Tables,
    request: ProductionRequest,
    extraction: Value,
    rules: &Value,
) -> Result<Value> {
    check(
        source["tables"]
            .as_object()
            .unwrap()
            .values()
            .flat_map(|v| v.as_array().unwrap())
            .map(|r| r.as_object().unwrap().len())
            .sum::<usize>()
            <= 250000,
        "compiler.field_limit",
    )?;
    let source_sha = digest(source);
    let mut c = Compiler {
        source,
        source_sha: &source_sha,
        dbcs,
        request,
        issues: Vec::new(),
        dependencies: BTreeSet::new(),
        selected: BTreeMap::new(),
        binding_bytes: 0,
    };
    let relationships = plans::relationships(source, &source_sha);
    let quests = plans::quests(source, dbcs, &relationships, &source_sha);
    let rules_plan = rules_plan(source, rules);
    let mut participant_plan = super::extensions::participants::supported(source, dbcs);
    c.issues.extend(
        participant_plan["refusals"]
            .as_array()
            .unwrap()
            .iter()
            .cloned(),
    );
    let mut captured_items = plans::item::supported(source, &source_sha);
    c.issues.extend(
        captured_items["refusals"]
            .as_array()
            .unwrap()
            .iter()
            .cloned(),
    );
    for refusal in relationships["refusals"].as_array().unwrap() {
        c.context_issue(
            "quest_template",
            json!([refusal["quest"]]),
            "quest.chain",
            json!({"edge":refusal}),
        );
    }
    let mut replaced: BTreeSet<&str> = [
        "player.chr_races_classes_map_dbc_not_captured",
        "player.faction_reputation_dbc_not_captured",
        "player.char_start_outfit_dbc_items_not_captured",
        "player.skill_line_race_class_tiers_ability_dbc_not_captured",
    ]
    .into_iter()
    .collect();
    if relationships["status"] == "resolved" {
        replaced.extend(["quest.chain", "quest.inbound_relationship"]);
    }
    for i in source["report"]["unsupported"].as_array().unwrap() {
        if !replaced.contains(i["reason"].as_str().unwrap())
            && !super::extensions::quest_endpoints::discharges(source, i)
        {
            c.issues.push(json!({"table":i["table"],"key":i["key"],"reason":i["reason"],"origin":"source_capture"}));
        }
    }
    let identity = c.identity()?;
    let outfit = c.outfit()?;
    let reputation = c.factions(&identity, rules)?;
    let learning = learning::discover(&mut c)?;
    let corpse_loot = plans::loot(source, &source_sha, &learning)?;
    if let Some(refusals) = corpse_loot["non_body_actions"]["refusals"].as_array() {
        c.issues.extend(refusals.iter().cloned());
    }
    let loader = learning::plan(source, dbcs, &learning);
    c.issues
        .extend(loader["refusals"].as_array().unwrap().iter().cloned());
    c.issue(
        "SkillRaceClassInfo",
        json!([]),
        "player.native_skill_obtainability_and_learning_required",
    );
    let mut skills = json!({"sql_candidates":source["tables"]["playercreateinfo_skills"],"race_class_candidates":learning["race_class_candidates"],"ability_candidates":learning["ability_candidates"],"dependency_closure":learning,"loader_plan":loader,"effective_skills":null});
    let spells = c.spells()?;
    let (numeric_candidate, _) =
        learning_numeric::candidate(&c, &learning, &spells, &skills["loader_plan"])?;
    skills["loader_plan"]["numeric_candidate"] = numeric_candidate;
    if skills["loader_plan"]["numeric_candidate"]["status"] == "checked" {
        skills["loader_plan"]["numeric_overlay"] = json!("checked_complete_effective_projection");
    }
    skills["loader_plan"]["reached_hooks"] = reached_hooks::plan(
        &c,
        &spells,
        &skills["loader_plan"],
        &skills["loader_plan"]["numeric_candidate"],
    );
    let talents = plans::talents(&dbcs["Talent"], &learning["spell_ids"]);
    let creatures = c.creatures(&rules_plan["rules"])?;
    let resolved: BTreeSet<_> = creatures
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["generation_plan"]["status"] == "complete_inputs")
        .map(|c| c["entry"].to_string())
        .collect();
    let replaced_creature = |i: &Value| {
        i["table"] == "creature_template"
            && i["key"].as_str().is_some_and(|key| resolved.contains(key))
            && [
                "creature.native_stats_combat_faction_required",
                "creature.variable_level",
            ]
            .iter()
            .any(|s| i["reason"] == *s)
    };
    c.issues
        .retain(|i| !(i["origin"] == "source_capture" && replaced_creature(i)));
    c.issues.retain(|i| !plans::event_discharges(i, &creatures));
    let spawn_discharged: Vec<_> = c
        .issues
        .iter()
        .filter(|i| plans::spawn::discharges(i, &creatures))
        .cloned()
        .collect();
    c.issues
        .retain(|i| !plans::spawn::discharges(i, &creatures));
    let rule_discharged: Vec<_> = c
        .issues
        .iter()
        .filter(|i| rules_discharges(i, &rules_plan))
        .cloned()
        .collect();
    c.issues.retain(|i| !rules_discharges(i, &rules_plan));
    let display_ids: BTreeSet<_> = creatures
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|c| c["selected_display_ids"].as_array().unwrap())
        .map(int)
        .collect();
    let display = &dbcs["CreatureDisplayInfo"];
    let display_evidence = json!({"coverage":"complete_raw_table","table_sha256":display.sha256,"row_count":display.count,"selected_ids":numbers(display_ids.iter().copied()),"missing_ids":numbers(display_ids.iter().filter(|id|display.get(**id as u32).is_none()).copied())});
    c.issue(
        "runtime",
        json!([]),
        "native.runtime_profile_not_implemented",
    );
    let mut dispositions = c.dispositions(
        relationships["status"] == "resolved",
        loader["loaded_learning_maps_resolved"] == true,
        &creatures,
    );
    for d in &mut dispositions {
        if d["table"] == "quest_template"
            && rules_plan["quest_xp_inputs"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| d["key"] == json!([r["quest"]]))
        {
            let reason = "quest.native_eligibility_credit_reward_required";
            let fields = vec![json!("QuestLevel"), json!("RewMoneyMaxLevel")];
            let blocked = d["blocked_for_gameplay"][reason]
                .as_array()
                .cloned()
                .unwrap_or_default();
            d["blocked_for_gameplay"][reason] = json!(blocked
                .into_iter()
                .filter(|f| !fields.contains(f))
                .collect::<Vec<_>>());
            d["supported_numeric_policy"] =
                json!({"fields":fields,"input_binding":rules_plan["input_binding"]});
        }
    }
    let fireball = spells
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == 133)
        .map(|s| &s["fireball_plan"])
        .unwrap_or(&Value::Null);
    let discharged: Vec<_> = c
        .issues
        .iter()
        .filter(|i| plans::fireball_discharges(i, fireball))
        .cloned()
        .collect();
    c.issues
        .retain(|i| !plans::fireball_discharges(i, fireball));
    let loot_discharged: Vec<_> = c
        .issues
        .iter()
        .filter(|i| plans::loot_discharges(i, &corpse_loot))
        .cloned()
        .collect();
    c.issues
        .retain(|i| !plans::loot_discharges(i, &corpse_loot));
    let item_discharged: Vec<_> = c
        .issues
        .iter()
        .filter(|i| plans::item::discharges(i, &captured_items))
        .cloned()
        .collect();
    c.issues
        .retain(|i| !plans::item::discharges(i, &captured_items));
    let execution_discharged: Vec<_> = c
        .issues
        .iter()
        .filter(|i| plans::spell_execution::discharges(i, &spells))
        .cloned()
        .collect();
    c.issues
        .retain(|i| !plans::spell_execution::discharges(i, &spells));
    for d in &mut dispositions {
        if d["table"] != "spell_template" {
            continue;
        }
        let plan = &spells
            .as_array()
            .unwrap()
            .iter()
            .find(|s| d["key"] == json!([s["id"]]))
            .unwrap()["execution_plan"];
        if plan["status"] == "supported_shape" {
            let fields = d["blocked_for_gameplay"]
                .as_object_mut()
                .unwrap()
                .remove("spell.native_effect_target_scaling_rules_required")
                .unwrap_or(json!([]));
            d["supported_catalog_plan"] = json!({"adapter":plan["adapter"],
                "fields":fields,"input_bindings":plan["input_bindings"]});
        }
    }
    for d in &mut dispositions {
        if d["table"] != "item_template" {
            continue;
        }
        let plan = captured_items["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| d["key"] == json!([p["entry"]]))
            .unwrap();
        if plan["status"] == "supported_shape" {
            let fields = d["blocked_for_gameplay"]
                .as_object_mut()
                .unwrap()
                .remove(plans::item::REASON)
                .unwrap_or(json!([]));
            let consumed: BTreeSet<_> = d["consumed_by_preparation"]
                .as_array()
                .unwrap()
                .iter()
                .chain(fields.as_array().unwrap())
                .map(|v| v.as_str().unwrap().to_owned())
                .collect();
            d["consumed_by_preparation"] = json!(consumed);
            let fields: BTreeSet<_> = plans::item::INVENTORY
                .iter()
                .map(|(source, _)| source)
                .collect();
            d["supported_catalog_plan"] = json!({"adapter":plans::item::ADAPTER,
                "scope":"carried_storage_only","fields":fields,
                "item_template_sha256":plan["item_template_sha256"],
                "starter_placement":plan["starter_placement"],
                "unavailable_operations":plan["unavailable_operations"]});
        }
    }
    if corpse_loot["status"] == "supported_shape" {
        c.issue(
            "loot",
            json!([]),
            "loot.explicit_rates_inventory_audit_and_host_binding_required",
        );
        for d in &mut dispositions {
            if ["creature_loot_template", "reference_loot_template"]
                .iter()
                .any(|t| d["table"] == *t)
            {
                d["blocked_for_gameplay"]
                    .as_object_mut()
                    .unwrap()
                    .remove("loot.native_chance_condition_rng_required");
                d["supported_catalog_plan"] = json!({"adapter":LOOT_ADAPTER,"fields":["entry","item","ChanceOrQuestChance","groupid","mincountOrRef","maxcount","condition_id"],"input_bindings":corpse_loot["input_bindings"]});
            }
        }
    }
    if fireball["status"] == "supported_shape" {
        for d in &mut dispositions {
            if d["table"] == "spell_template" && d["key"] == json!([133]) {
                let fs = d["blocked_for_gameplay"]
                    .as_object_mut()
                    .unwrap()
                    .remove("spell.native_effect_target_scaling_rules_required")
                    .unwrap_or(json!([]));
                d["supported_catalog_plan"] = json!({"adapter":FIREBALL_ADAPTER,"fields":fs,"input_bindings":fireball["input_bindings"]});
            }
        }
    }
    let mut bindings = Vec::new();
    for ((table, _), selected) in std::mem::take(&mut c.selected) {
        let mut v = selected.raw;
        let words = v["words"].as_array().unwrap().len();
        v["table"] = json!(table);
        v["consumed_by_preparation"] = json!(selected.used);
        v["blocked_for_gameplay"] = json!((0..words)
            .filter(|i| !selected.used.contains(i))
            .collect::<Vec<_>>());
        v["capability"] = json!(PROFILE);
        bindings.push(v);
    }
    let issues: BTreeMap<Vec<u8>, Value> = std::mem::take(&mut c.issues)
        .into_iter()
        .map(|i| (canonical(&i), i))
        .collect();
    let mut issues: Vec<_> = issues.into_values().collect();
    let admission = plans::classify(source, &spells, &mut issues, &learning);
    // Python retains these same refusal objects in both lists when classifying admission.
    // All loader refusals are player.learning.* and therefore selection-entry scoped.
    for refusal in skills["loader_plan"]["refusals"].as_array_mut().unwrap() {
        refusal["admission_scope"] = json!({"kind":"selection_entry"});
    }
    // Item refusals also share Python objects with the top-level issues.
    // item.* findings cannot be scoped to an unavailable active ability.
    for refusal in captured_items["refusals"].as_array_mut().unwrap() {
        refusal["admission_scope"] = json!({"kind":"selection_entry"});
    }
    for refusal in participant_plan["refusals"].as_array_mut().unwrap() {
        refusal["admission_scope"] = json!({"kind":"selection_entry"});
    }
    for item in captured_items["items"].as_array_mut().unwrap() {
        for refusal in item["refusals"].as_array_mut().unwrap() {
            refusal["admission_scope"] = json!({"kind":"selection_entry"});
        }
    }
    let annotate =
        |rs: &[Value], disposition: &str, adapter: Option<&str>, binding: Option<&Value>| {
            rs.iter()
                .map(|i| {
                    let mut v = i.clone();
                    v["disposition"] = json!(disposition);
                    if let Some(a) = adapter {
                        v["adapter"] = json!(a);
                    }
                    if let Some(b) = binding {
                        v["input_bindings"] = b.clone();
                    }
                    v
                })
                .collect::<Vec<_>>()
        };
    let mut rules_obligations = annotate(&rule_discharged, "supported_numeric_policy", None, None);
    for v in &mut rules_obligations {
        v["input_binding"] = rules_plan["input_binding"].clone();
    }
    let source_findings: Vec<_> = source["report"]["unsupported"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| {
            let mut v = i.clone();
            v["disposition"] = json!(if replaced.contains(i["reason"].as_str().unwrap())
                || replaced_creature(i)
            {
                "replaced_by_exact_dependency_report"
            } else if super::extensions::quest_endpoints::discharges(source, i) {
                "outside_selection_deferred"
            } else if plans::fireball_discharges(i, fireball)
                || plans::spawn::discharges(i, &creatures)
                || plans::event_discharges(i, &creatures)
                || plans::spell_execution::discharges(i, &spells)
                || plans::item::discharges(i, &captured_items)
            {
                "supported_shape"
            } else if rules_discharges(i, &rules_plan) {
                "supported_numeric_policy"
            } else {
                "retained"
            });
            v
        })
        .collect();
    let mut output = json!({
        "format": FORMAT,
        "capability": PROFILE,
        "core_revision": CORE,
        "client_build_claim": 5875,
        "client_build_authenticity_verified": false,
        "source_sha256": source_sha,
        "request": request.json(),
        "extraction_manifest": extraction,
        "dbc_manifest": dbcs.values().map(|t| t.manifest()).collect::<Vec<_>>(),
        "status": "blocked",
        "runtime_export": null,
        "ability_admission": admission,
        "dependency_request": learning["dependency_request"],
        "native_spawn_capture": source.get("native_spawn_capture"),
        "spawn_context_capture": source.get("spawn_context_capture"),
        "spawn_context_obligations": annotate(&spawn_discharged, "supported_shape", Some(plans::spawn::FORMAT), None),
        "spell_script_capture": source.get("spell_script_capture"),
        "initial_hook_capture": source.get("initial_hook_capture"),
        "spell_numeric_capture": source.get("spell_numeric_capture"),
        "fireball_plan_obligations": annotate(&discharged, "supported_shape",
            Some(FIREBALL_ADAPTER), Some(&fireball["input_bindings"])),
        "spell_execution_obligations": annotate(&execution_discharged, "supported_shape",
            Some(plans::spell_execution::ADAPTER), None),
        "creature_display_inputs": display_evidence,
        "quest_relationships": relationships,
        "quest_giver_presentation_plan": quests,
        "world_rules_plan": rules_plan,
        "world_rules_obligations": rules_obligations,
        "quest_loot_inputs": plans::quest_loot(source),
        "corpse_loot_plan": corpse_loot,
        "captured_item_plan": captured_items,
        "captured_item_obligations": item_discharged.iter().map(|i| {
            let mut v = i.clone();
            v["disposition"] = json!("supported_shape");
            v["adapter"] = json!(plans::item::ADAPTER);
            v["scope"] = json!("carried_storage_only");
            v
        }).collect::<Vec<_>>(),
        "corpse_loot_plan_obligations": annotate(&loot_discharged, "supported_shape",
            Some(LOOT_ADAPTER), None),
        "player": {
            "identity": identity,
            "level_inputs": c.progression(),
            "outfit": outfit,
            "initial_sql_spells": source["tables"]["playercreateinfo_spell"],
            "actions": source["tables"]["playercreateinfo_action"],
            "skills": skills,
            "initial_reputation": reputation,
        },
        "spells": spells,
        "creatures": creatures,
        "talent_inputs": talents,
        "candidate_paths": c.paths(&spells),
        "required_additional_rows": c.dependencies.iter()
            .map(|(t,i,r)| json!({"table":t, "id":num(*i), "reason":r})).collect::<Vec<_>>(),
        "source_field_dispositions": dispositions,
        "dbc_bindings": bindings,
        "source_capture_findings": source_findings,
        "unsupported": issues,
    });
    if source.get("global_hook_capture").is_some() {
        output["global_hook_plan"] = super::global_hooks::plan(source, dbcs);
    }
    output["participants_plan"] = participant_plan;
    output["preparation_sha256"] = json!(digest(&output));
    bounded_len(&output, 128 * 1024 * 1024, "compiler.report_byte_limit")?;
    Ok(output)
}
