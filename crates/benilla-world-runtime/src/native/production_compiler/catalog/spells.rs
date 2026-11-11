// SPDX-License-Identifier: GPL-3.0-or-later
//! Complete spell and condition inputs, with no executable handler admission.
use super::super::super::source_execution::identity_table::IdentityTable;
use super::*;
use std::collections::BTreeSet;
use std::io::{BufWriter, Write};
use std::sync::LazyLock;
static POLICY: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../scripts/classic_export/catalog_spell_policy.json"
    )))
    .expect("spell policy")
});
const DBC_FILES: &[&str] = &[
    "AreaTable.dbc",
    "Faction.dbc",
    "Map.dbc",
    "SkillLine.dbc",
    "SkillLineAbility.dbc",
    "SkillRaceClassInfo.dbc",
    "SkillTiers.dbc",
    "SpellCastTimes.dbc",
    "SpellCategory.dbc",
    "SpellDuration.dbc",
    "SpellFocusObject.dbc",
    "SpellItemEnchantment.dbc",
    "SpellRadius.dbc",
    "SpellRange.dbc",
    "SpellShapeshiftForm.dbc",
    "Talent.dbc",
];
const REFUSALS: &[&str] = &[
    "spells.reference_execution_missing",
    "spells.sql_occurrences_not_F1",
    "spells.registration_and_handlers_unverified",
    "spells.external_loaded_dependencies_unverified",
    "spells.unit_condition_flags_unassigned_at_pin",
    "spells.expression_execution_unimplemented",
    "spells.gameplay_not_admitted",
];
const CONDITIONS: &[&str] = &[
    "conditions",
    "unit_condition",
    "combat_condition",
    "worldstate_expression",
    "worldstate_name",
];
const FIELDS: &[&str] = &[
    "attributes",
    "effect1",
    "effect2",
    "effect3",
    "effectmiscvalue1",
    "effectmiscvalue2",
    "effectmiscvalue3",
    "effectbasepoints1",
    "effectbasepoints2",
    "effectbasepoints3",
    "effecttriggerspell1",
    "effecttriggerspell2",
    "effecttriggerspell3",
    "effectimplicittargeta1",
    "effectimplicittargeta2",
    "effectimplicittargeta3",
    "effectimplicittargetb1",
    "effectimplicittargetb2",
    "effectimplicittargetb3",
    "effectapplyauraname1",
    "effectapplyauraname2",
    "effectapplyauraname3",
    "effectitemtype1",
    "effectitemtype2",
    "effectitemtype3",
];
type Spells = BTreeMap<u32, [u32; 25]>;
type Nodes = BTreeMap<u32, [u32; 4]>;
type DbcRows = BTreeMap<u32, Vec<u32>>;
type Statuses = BTreeMap<String, BTreeMap<u64, String>>;
fn hex(raw: &[u8]) -> String {
    super::hex(raw)
}
fn convert(raw: Option<&[u8]>, kind: &str) -> Result<Value> {
    match kind {
        "ignored" | "unassigned" => Ok(Value::Null),
        "text" => Ok(json!(hex(raw
            .unwrap_or_default()
            .split(|b| *b == 0)
            .next()
            .unwrap_or_default()))),
        "f32" => Ok(json!(spawns::floating(raw)?.to_bits())),
        "u64" => {
            let raw = raw.unwrap_or_default();
            check(raw.len() < 128, "spells.numeric_limit")?;
            let raw = raw.trim_ascii_start();
            let negative = raw.first() == Some(&b'-');
            let start = usize::from(matches!(raw.first(), Some(b'+' | b'-')));
            let mut value = 0u64;
            for &b in &raw[start..] {
                if !b.is_ascii_digit() {
                    break;
                }
                value = value
                    .checked_mul(10)
                    .and_then(|v| v.checked_add(u64::from(b - b'0')))
                    .ok_or_else(|| fail("spells.uint64_overflow"))?;
            }
            Ok(json!(if negative {
                value.wrapping_neg()
            } else {
                value
            }))
        }
        _ => {
            check(raw.is_none_or(|v| v.len() < 128), "spells.numeric_limit")?;
            let value = spawns::uint(raw)?;
            Ok(json!(match kind {
                "u8" => value & 255,
                "u16" => value & 65535,
                "bool" => u32::from((value as i32) > 0),
                "u32" => value,
                _ => return Err(fail("spells.conversion")),
            }))
        }
    }
}
struct Stream {
    file: Option<BufWriter<std::fs::File>>,
    hash: Sha256,
    rows: u64,
    bytes: u64,
}
impl Stream {
    fn new(output: Option<&Path>, name: &str) -> Result<Self> {
        let file = output
            .map(|p| {
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(p.join(format!("{name}.jsonl")))
                    .map(BufWriter::new)
            })
            .transpose()
            .map_err(|_| fail("spells.output"))?;
        Ok(Self {
            file,
            hash: Sha256::new(),
            rows: 0,
            bytes: 0,
        })
    }
    fn add(&mut self, record: &Value) -> Result<()> {
        let mut raw = json::canonical(record);
        raw.push(b'\n');
        check(raw.len() <= FRAME, "spells.record_limit")?;
        self.hash.update(&raw);
        self.rows += 1;
        self.bytes += raw.len() as u64;
        if let Some(file) = &mut self.file {
            file.write_all(&raw).map_err(|_| fail("spells.output"))?;
        }
        Ok(())
    }
    fn finish(mut self) -> Result<Value> {
        if let Some(file) = &mut self.file {
            file.flush().map_err(|_| fail("spells.output"))?;
        }
        Ok(json!({"rows":self.rows,"bytes":self.bytes,"sha256":hex(&self.hash.finalize())}))
    }
}
struct Table {
    names: Vec<String>,
    rows: Vec<(u64, Vec<Value>)>,
}
impl Table {
    fn index(&self, name: &str) -> usize {
        self.names
            .iter()
            .position(|n| n == name)
            .expect("policy field")
    }
    fn n(&self, row: &[Value], name: &str) -> u32 {
        row[self.index(name)].as_u64().expect("numeric field") as u32
    }
    fn object(&self, row: &[Value]) -> Value {
        json!(self.names.iter().zip(row).collect::<BTreeMap<_, _>>())
    }
}
fn fields(table: &str) -> &'static [Value] {
    POLICY["tables"][table]["fields"]
        .as_array()
        .expect("fields")
}
fn read_table(catalog: &mut VerifiedCatalog, name: &str, retained: &mut usize) -> Result<Table> {
    let mut result = Table {
        names: fields(name)
            .iter()
            .map(|v| v[0].as_str().unwrap().to_owned())
            .collect(),
        rows: Vec::new(),
    };
    catalog.scan(name, 0, 256, |row| {
        let values = row
            .cells()
            .zip(fields(name))
            .map(|(c, f)| convert(c?, f[1].as_str().unwrap()))
            .collect::<Result<Vec<_>>>()?;
        let bytes = 128
            + values
                .iter()
                .map(|v| 64 + v.as_str().map_or(0, |s| s.len() * 2))
                .sum::<usize>();
        *retained = retained
            .checked_add(bytes)
            .ok_or_else(|| fail("spells.view_limit"))?;
        check(*retained <= DECODED - 4_194_304, "spells.view_limit")?;
        result.rows.push((row.ordinal, values));
        Ok(())
    })?;
    Ok(result)
}
struct Chains {
    nodes: Nodes,
    order: Vec<u32>,
    fallback: Nodes,
    dispositions: BTreeMap<u64, String>,
    talents: BTreeMap<u32, (u32, Vec<u32>)>,
    abilities: BTreeSet<u32>,
}
fn chain_insert(
    nodes: &mut Nodes,
    order: &mut IdentityTable<()>,
    id: u32,
    node: [u32; 4],
) -> Result<()> {
    order
        .insert(u64::from(id), (), 100_000)
        .map_err(|e| fail(&e.id))?;
    nodes.insert(id, node);
    Ok(())
}
fn chains(spells: &Spells, talents: &DbcRows, abilities: &DbcRows, sql: &Table) -> Result<Chains> {
    let mut nodes = Nodes::new();
    let mut order = IdentityTable::default();
    let mut positions = BTreeMap::new();
    for w in talents.values() {
        for j in 0..5 {
            let spell = w[4 + j];
            if spell != 0 {
                positions.insert(spell, (j as u32 + 1, w.clone()));
            }
            if spell != 0 && w[5] != 0 && spells.contains_key(&spell) {
                chain_insert(
                    &mut nodes,
                    &mut order,
                    spell,
                    [if j == 0 { 0 } else { w[3 + j] }, w[4], j as u32 + 1, 0],
                )?;
            }
        }
    }
    let mut by_spell = BTreeMap::<u32, Vec<&[u32]>>::new();
    for w in abilities.values() {
        by_spell.entry(w[2]).or_default().push(w);
    }
    let mut previous = IdentityTable::default();
    for (&spell, rows) in &by_spell {
        for w in rows {
            let forward = if spell == 2366 { 2368 } else { w[8] };
            if !spells.contains_key(&spell)
                || forward == 0
                || !spells.contains_key(&forward)
                || !by_spell.contains_key(&forward)
            {
                continue;
            }
            if let Some(node) = nodes.get(&forward) {
                check(node[0] == spell, "spells.chain_conflict")?;
            } else if let Some(&p) = previous.get(u64::from(forward)) {
                check(p == spell, "spells.chain_conflict")?;
            } else if let Some(node) = nodes.get(&spell).copied() {
                chain_insert(
                    &mut nodes,
                    &mut order,
                    forward,
                    [spell, node[1], (node[2] + 1) & 255, 0],
                )?;
            } else {
                previous
                    .insert(u64::from(forward), spell, 100_000)
                    .map_err(|e| fail(&e.id))?;
            }
        }
    }
    while let Some(spell) = previous.first_key() {
        let mut parent = previous.erase(spell).expect("source begin exists");
        let spell = spell as u32;
        if let Some(node) = nodes.get(&spell) {
            check(node[0] == parent, "spells.chain_conflict")?;
            continue;
        }
        let mut path = vec![(spell, parent)];
        let mut seen = BTreeSet::from([spell]);
        while !nodes.contains_key(&parent) && previous.get(u64::from(parent)).is_some() {
            check(
                seen.insert(parent) && path.len() <= 30,
                "spells.chain_cycle",
            )?;
            let next = *previous
                .get(u64::from(parent))
                .expect("checked previous rank");
            path.push((parent, next));
            parent = next;
        }
        check(!seen.contains(&parent), "spells.chain_cycle")?;
        if !nodes.contains_key(&parent) {
            chain_insert(&mut nodes, &mut order, parent, [0, parent, 1, 0])?;
        }
        for (child, parent) in path.into_iter().rev() {
            let node = nodes[&parent];
            chain_insert(
                &mut nodes,
                &mut order,
                child,
                [parent, node[1], (node[2] + 1) & 255, 0],
            )?;
        }
    }
    let fallback = nodes.clone();
    let mut dispositions = BTreeMap::new();
    for (ordinal, row) in &sql.rows {
        let spell = sql.n(row, "spell_id");
        let n = [
            sql.n(row, "prev_spell"),
            sql.n(row, "first_spell"),
            sql.n(row, "rank"),
            sql.n(row, "req_spell"),
        ];
        let status = if !spells.contains_key(&spell) {
            "skip:missing_spell"
        } else if let Some(old) = nodes.get_mut(&spell) {
            if old[..3] != n[..3] {
                "skip:dbc_chain_conflict"
            } else if n[3] != 0 {
                old[3] = n[3];
                "loaded:req_update"
            } else {
                "skip:redundant_dbc_chain"
            }
        } else if n[0] != 0 && !spells.contains_key(&n[0]) {
            "skip:missing_previous"
        } else if !spells.contains_key(&n[1]) {
            "skip:missing_first"
        } else if (spell == n[1]) != (n[2] <= 1)
            || (spell == n[1]) != (n[0] == 0)
            || (n[2] <= 1) != (n[0] == 0)
        {
            "skip:incompatible_chain"
        } else if n[3] != 0 && !spells.contains_key(&n[3]) {
            "skip:missing_required"
        } else if positions.get(&spell).is_some_and(|(rank, w)| {
            n[2] != *rank
                || n[1] != w[4]
                || n[2] > 1 && n[0] != w[n[2] as usize + 2]
                || n[3] != w[20]
        }) {
            "skip:talent_conflict"
        } else {
            chain_insert(&mut nodes, &mut order, spell, n)?;
            "loaded"
        };
        dispositions.insert(*ordinal, status.to_owned());
    }
    Ok(Chains {
        nodes,
        order: order.iter().map(|(id, ())| id as u32).collect(),
        fallback,
        dispositions,
        talents: positions,
        abilities: by_spell.keys().copied().collect(),
    })
}
fn rank_inputs(
    name: &str,
    table: &Table,
    spells: &Spells,
    nodes: &Nodes,
    next_ready: bool,
    plans: &mut Stream,
) -> Result<BTreeMap<u64, String>> {
    let mut loaded = BTreeMap::<u32, usize>::new();
    let mut origins = BTreeMap::new();
    let mut firsts = BTreeSet::new();
    let mut status = BTreeMap::new();
    for (index, (ordinal, row)) in table.rows.iter().enumerate() {
        let spell = table.n(row, "entry");
        let first = nodes.get(&spell).map_or(spell, |n| n[1]);
        let outcome = if !spells.contains_key(&spell) {
            "skip:missing_spell"
        } else if first != spell
            && (name == "spell_proc_item_enchant"
                || (if name == "spell_proc_event" {
                    table.n(row, "ppmrate") & 0x7fffffff
                } else {
                    table.n(row, "threat")
                }) == 0)
        {
            "skip:nonfirst_rank"
        } else {
            loaded.insert(spell, index);
            origins.insert(spell, spell);
            firsts.insert(first);
            "loaded"
        };
        status.insert(*ordinal, outcome.to_owned());
    }
    let mut next = BTreeMap::<u32, Vec<u32>>::new();
    if next_ready {
        for (&spell, n) in nodes {
            for parent in [n[0], n[3]] {
                if parent != 0 {
                    next.entry(parent).or_default().push(spell);
                }
            }
        }
    }
    for first in firsts {
        let Some(&index) = loaded.get(&first) else {
            continue;
        };
        let mut pending: Vec<_> = next
            .get(&first)
            .into_iter()
            .flatten()
            .rev()
            .map(|s| (*s, false))
            .collect();
        let mut active = BTreeSet::from([first]);
        let mut steps = 0;
        while let Some((spell, exiting)) = pending.pop() {
            if exiting {
                active.remove(&spell);
                continue;
            }
            check(active.insert(spell), "spells.rank_cycle")?;
            steps += 1;
            check(steps <= 2_000_000, "spells.view_limit")?;
            if name == "spell_proc_item_enchant" || !loaded.contains_key(&spell) {
                loaded.insert(spell, index);
                origins.insert(spell, first);
            }
            pending.push((spell, true));
            if let Some(children) = next.get(&spell) {
                pending.extend(children.iter().rev().map(|s| (*s, false)));
            }
        }
    }
    for (spell, index) in loaded {
        plans.add(&json!([
            name,
            spell,
            origins[&spell],
            table.object(&table.rows[index].1)
        ]))?;
    }
    Ok(status)
}
fn pet_inputs(t: &Table, spells: &Spells) -> BTreeMap<u64, String> {
    let mut loaded = BTreeSet::new();
    let mut result = BTreeMap::new();
    for (ordinal, row) in &t.rows {
        let spell = t.n(row, "spell");
        let status = if loaded.contains(&spell) {
            "loaded_input:additional_pet"
        } else if let Some(w) = spells.get(&spell) {
            if !(0..3).any(|i| w[1 + i] == 3 || w[1 + i] == 6 && w[19 + i] == 4) {
                "skip:pet_dummy"
            } else if !spells.contains_key(&t.n(row, "aura")) {
                "skip:missing_aura"
            } else {
                loaded.insert(spell);
                "loaded_input"
            }
        } else {
            "skip:missing_spell"
        };
        result.insert(*ordinal, status.to_owned());
    }
    result
}
fn basic_disposition(
    table: &str,
    input: &Table,
    row: &[Value],
    spells: &Spells,
    nodes: &Nodes,
    ids: &BTreeMap<String, BTreeSet<u32>>,
) -> &'static str {
    if table == "spell_target_position" {
        if !ids["Map.dbc"].contains(&input.n(row, "target_map")) {
            return "skip:target_map";
        }
        if [
            "target_position_x",
            "target_position_y",
            "target_position_z",
        ]
        .iter()
        .all(|n| input.n(row, n) & 0x7fffffff == 0)
        {
            return "skip:target_coordinates";
        }
        let Some(w) = spells.get(&input.n(row, "id")) else {
            return "skip:missing_spell";
        };
        if !(13..19).any(|i| w[i] == 17) {
            return "skip:database_target";
        }
        return "loaded_input";
    }
    if matches!(
        table,
        "spell_facing"
            | "spell_elixir"
            | "spell_affect"
            | "spell_script_target"
            | "spell_target_position"
            | "spell_pet_auras"
            | "spell_area"
    ) {
        let key = if input.names.iter().any(|n| n == "entry") {
            "entry"
        } else if input.names.iter().any(|n| n == "id") {
            "id"
        } else {
            "spell"
        };
        if !spells.contains_key(&input.n(row, key)) {
            return "skip:missing_spell";
        }
    }
    if table == "spell_affect" {
        let spell = input.n(row, "entry");
        let effect = input.n(row, "effectid") as usize;
        if effect >= 3 {
            return "skip:effect_index";
        }
        if nodes.get(&spell).map_or(spell, |n| n[1]) != spell {
            return "skip:nonfirst_rank";
        }
        let w = spells[&spell];
        if w[1 + effect] != 6 || ![4, 107, 108, 109, 112].contains(&w[19 + effect]) {
            return "skip:affect_aura";
        }
        if w[22 + effect] != 0
            && u64::from(w[22 + effect]) == row[input.index("spellfamilymask")].as_u64().unwrap()
        {
            return "skip:redundant_mask";
        }
        return "loaded_input";
    }
    if matches!(
        table,
        "spell_area" | "spell_script_target" | "spell_target_position" | "spell_pet_auras"
    ) {
        "retained:cross_loader_validation_required"
    } else {
        "loaded_input"
    }
}
fn external_ids(
    catalog: &mut VerifiedCatalog,
    schema: &Value,
    table: &str,
    key: &str,
    budget: &mut usize,
) -> Result<BTreeSet<u32>> {
    let cols = schema["tables"][table]["columns"]
        .as_array()
        .ok_or_else(|| fail("spells.schema"))?;
    let index = cols
        .iter()
        .position(|c| {
            c["COLUMN_NAME"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case(key))
        })
        .ok_or_else(|| fail("spells.schema"))?;
    let mut ids = BTreeSet::new();
    catalog.scan(table, 0, 256, |row| {
        let cell = row
            .cells()
            .nth(index)
            .ok_or_else(|| fail("spells.schema"))??;
        if ids.insert(spawns::uint(cell)?) {
            *budget += 96;
            check(*budget <= 4_194_304, "spells.view_limit")?;
        }
        Ok(())
    })?;
    Ok(ids)
}
fn condition_inputs(
    catalog: &mut VerifiedCatalog,
    schema: &Value,
    aux: &BTreeMap<String, Table>,
    spells: &Spells,
    ids: &BTreeMap<String, BTreeSet<u32>>,
    chains: &Chains,
    plans: &mut Stream,
) -> Result<Statuses> {
    let mut status = Statuses::new();
    for &t in CONDITIONS {
        status.insert(t.to_owned(), BTreeMap::new());
    }
    for name in ["worldstate_expression", "worldstate_name"] {
        let t = &aux[name];
        for (ordinal, row) in &t.rows {
            plans.add(&json!([name, ordinal, t.object(row)]))?;
            status.get_mut(name).unwrap().insert(
                *ordinal,
                if name == "worldstate_expression" {
                    "loaded_input:execution_blocked"
                } else {
                    "loaded_input"
                }
                .into(),
            );
        }
    }
    let unit_ids: BTreeSet<_> = aux["unit_condition"]
        .rows
        .iter()
        .map(|(_, r)| aux["unit_condition"].n(r, "id"))
        .collect();
    let expr_ids: BTreeSet<_> = aux["worldstate_expression"]
        .rows
        .iter()
        .map(|(_, r)| aux["worldstate_expression"].n(r, "id"))
        .collect();
    let t = &aux["unit_condition"];
    for (ordinal, row) in &t.rows {
        let values: Vec<_> = (0..8)
            .map(|i| {
                let variable = t.n(row, &format!("variable_{i}"));
                let op = t.n(row, &format!("op_{i}"));
                json!([
                    if variable < 87 { variable } else { 0 },
                    if op < 7 { op } else { 0 },
                    t.n(row, &format!("value_{i}")) as i32
                ])
            })
            .collect();
        plans.add(&json!(["unit_condition", t.n(row, "id"), values]))?;
        status
            .get_mut("unit_condition")
            .unwrap()
            .insert(*ordinal, "blocked:flags_unassigned_at_pin".into());
    }
    let t = &aux["combat_condition"];
    for (ordinal, row) in &t.rows {
        let mut r = row.clone();
        for key in [
            "worldstateexpressionid",
            "selfconditionid",
            "targetconditionid",
            "friendconditionid_0",
            "friendconditionid_1",
            "enemyconditionid_0",
            "enemyconditionid_1",
        ] {
            let n = t.n(&r, key);
            if n != 0
                && !(if key == "worldstateexpressionid" {
                    &expr_ids
                } else {
                    &unit_ids
                })
                .contains(&n)
            {
                r[t.index(key)] = json!(0);
            }
        }
        for key in [
            "friendconditionop_0",
            "friendconditionop_1",
            "enemyconditionop_0",
            "enemyconditionop_1",
        ] {
            if t.n(&r, key) >= 7 {
                r[t.index(key)] = json!(0);
            }
        }
        plans.add(&json!(["combat_condition", t.n(row, "id"), t.object(&r)]))?;
        status
            .get_mut("combat_condition")
            .unwrap()
            .insert(*ordinal, "loaded_input:execution_blocked".into());
    }
    let names: BTreeSet<_> = aux["worldstate_name"]
        .rows
        .iter()
        .map(|(_, r)| aux["worldstate_name"].n(r, "id"))
        .collect();
    let mut external = BTreeMap::new();
    let mut external_budget = 0;
    for name in [
        "item_template",
        "quest_template",
        "creature_template",
        "game_event",
    ] {
        external.insert(
            name,
            external_ids(catalog, schema, name, "entry", &mut external_budget)?,
        );
    }
    let t = &aux["conditions"];
    let mut order: Vec<_> = t.rows.iter().collect();
    order.sort_by_key(|(_, r)| t.n(r, "condition_entry"));
    let mut loaded = BTreeMap::<u32, &str>::new();
    for (ordinal, row) in order {
        let entry = t.n(row, "condition_entry");
        let kind = t.n(row, "type") as i32;
        let a = t.n(row, "value1");
        let b = t.n(row, "value2");
        let c = t.n(row, "value3");
        let d = t.n(row, "value4");
        let outcome = match kind {
            -3..=-1 => {
                let refs = if kind == -3 {
                    vec![a]
                } else {
                    let mut v = vec![a, b];
                    if c != 0 {
                        v.push(c);
                    }
                    if d != 0 {
                        v.push(d);
                    }
                    v
                };
                if refs.iter().any(|r| *r >= entry || !loaded.contains_key(r)) {
                    "skip:condition_reference"
                } else if refs.iter().any(|r| loaded[r].starts_with("retained:")) {
                    "retained:condition_dependency_unverified"
                } else {
                    "loaded_input"
                }
            }
            1 if !spells.contains_key(&a) || b >= 3 => "skip:aura",
            2 | 3 | 23 if !external["item_template"].contains(&a) || kind != 3 && b < 1 => {
                "skip:item"
            }
            4 if !ids["AreaTable.dbc"].contains(&a) || b > 1 => "skip:area",
            5 | 30 if !ids["Faction.dbc"].contains(&a) || b >= 8 => "skip:reputation",
            6 if a != 469 && a != 67 => "skip:team",
            7 | 29 | 15 => "retained:world_config_required",
            8 | 9 | 19 | 22 if !external["quest_template"].contains(&a) => "skip:quest",
            12 => "retained:event_loaded_view_required",
            13 if a == 0 && b == 0 => "skip:area_flags",
            14 if a == 0 && b == 0 || a != 0 && a & 0xff == 0 || b != 0 && b & 0x5df == 0 => {
                "skip:race_class"
            }
            17 if !spells.contains_key(&a) || b > 1 => "skip:spell",
            28 if !chains.abilities.contains(&a)
                || b != 0 && !external["item_template"].contains(&b) =>
            {
                "skip:learnable"
            }
            37 | 39 if !external["creature_template"].contains(&a) || kind == 37 && b == 0 => {
                "skip:creature"
            }
            31 => "retained:encounter_loaded_view_required",
            33 if b > 2 => "skip:waypoint",
            35 if a >= 3 => "skip:gender",
            36 if a >= 4 => "skip:dead_state",
            42 if b > 7 || !names.contains(&a) => "skip:worldstate",
            43 if a > 1 => "skip:combat_state",
            0 | 1 | 2 | 3 | 4 | 5 | 6 | 8 | 9 | 10 | 11 | 13 | 14 | 17 | 18 | 19 | 22 | 23 | 26
            | 28 | 30 | 33 | 35 | 36 | 37 | 39 | 40 | 42 | 43 => "loaded_input",
            _ => "skip:unknown_condition_type",
        };
        status
            .get_mut("conditions")
            .unwrap()
            .insert(*ordinal, outcome.into());
        if !outcome.starts_with("skip:") {
            loaded.insert(entry, outcome);
        }
        plans.add(&json!(["condition", entry, outcome, t.object(row)]))?;
    }
    Ok(status)
}
/// Recomputes every field disposition and dependency from the current catalog.
pub fn diagnose_catalog_spells(root: &Path, output: Option<&Path>) -> Result<Value> {
    let mut catalog = VerifiedCatalog::open(root, false)?;
    prepare(&mut catalog, output)
}
pub(super) fn prepare(catalog: &mut VerifiedCatalog, output: Option<&Path>) -> Result<Value> {
    let schema = bounded_json(&catalog.object(&descriptor(&catalog.manifest["schema_root"])?, 0)?)?;
    check(
        catalog.manifest["core_revision"] == POLICY["core_revision"],
        "spells.source_pin",
    )?;
    for (name, spec) in POLICY["tables"].as_object().unwrap() {
        check(
            catalog.relations.contains_key(name),
            "spells.missing_relation",
        )?;
        let names: Vec<_> = schema["tables"][name]["columns"]
            .as_array()
            .ok_or_else(|| fail("spells.schema"))?
            .iter()
            .map(|c| c["COLUMN_NAME"].as_str().unwrap().to_ascii_lowercase())
            .collect();
        check(
            names
                == spec["fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|f| f[0].as_str().unwrap())
                    .collect::<Vec<_>>(),
            "spells.schema",
        )?;
    }
    if let Some(path) = output {
        std::fs::create_dir(path).map_err(|_| fail("spells.output"))?;
    }
    let mut dependencies = serde_json::Map::new();
    let mut ids = BTreeMap::new();
    let mut talents = DbcRows::new();
    let mut abilities = DbcRows::new();
    for &file in DBC_FILES {
        let mut view = catalog.dbc_view(file)?;
        dependencies.insert(file.into(), view.report(None)?);
        let mut keys = BTreeSet::new();
        for ordinal in 0..view.record_count() {
            let record = view.record(ordinal)?;
            let key = record["key"].as_u64().unwrap() as u32;
            keys.insert(key);
            if file == "Talent.dbc" || file == "SkillLineAbility.dbc" {
                let mut words = vec![0; if file == "Talent.dbc" { 21 } else { 15 }];
                for field in record["values"].as_array().unwrap() {
                    words[field[0].as_u64().unwrap() as usize] = field[2].as_u64().unwrap() as u32;
                }
                if file == "Talent.dbc" {
                    talents.insert(key, words);
                } else {
                    abilities.insert(key, words);
                }
            }
        }
        ids.insert(file.to_owned(), keys);
    }
    let mut spells = Spells::new();
    let indexes: Vec<_> = FIELDS
        .iter()
        .map(|name| {
            fields("spell_template")
                .iter()
                .position(|f| f[0] == *name)
                .unwrap()
        })
        .collect();
    let id_index = fields("spell_template")
        .iter()
        .position(|f| f[0] == "id")
        .unwrap();
    catalog.scan("spell_template", 0, 256, |row| {
        let cells = row.cells().collect::<Result<Vec<_>>>()?;
        let id = spawns::uint(cells[id_index])?;
        let mut words = [0; 25];
        for (i, &index) in indexes.iter().enumerate() {
            words[i] = spawns::uint(cells[index])?;
        }
        spells.insert(id, words);
        check(
            spells.len() * 256 < DECODED - 4_194_304,
            "spells.view_limit",
        )?;
        Ok(())
    })?;
    let mut aux = BTreeMap::new();
    let mut retained = spells.len() * 256
        + ids.values().map(|v| v.len() * 64).sum::<usize>()
        + talents
            .values()
            .chain(abilities.values())
            .map(|v| 128 + v.len() * 8)
            .sum::<usize>();
    for table in POLICY["tables"]
        .as_object()
        .unwrap()
        .keys()
        .filter(|n| *n != "spell_template")
    {
        aux.insert(table.clone(), read_table(catalog, table, &mut retained)?);
    }
    let chains = chains(&spells, &talents, &abilities, &aux["spell_chain"])?;
    let mut statuses = Statuses::from([("spell_chain".into(), chains.dispositions.clone())]);
    let mut plans = Stream::new(output, "plans")?;
    for (spell, node) in &chains.fallback {
        plans.add(&json!(["chain_fallback", spell, node]))?;
    }
    for (spell, node) in &chains.nodes {
        plans.add(&json!(["chain", spell, node]))?;
    }
    for name in [
        "spell_proc_event",
        "spell_threat",
        "spell_proc_item_enchant",
    ] {
        statuses.insert(
            name.into(),
            rank_inputs(
                name,
                &aux[name],
                &spells,
                &chains.nodes,
                !aux["spell_chain"].rows.is_empty(),
                &mut plans,
            )?,
        );
    }
    let mut groups = BTreeMap::new();
    let mut group_status = BTreeMap::new();
    let t = &aux["spell_group"];
    for (ordinal, row) in &t.rows {
        let rule = t.n(row, "rule");
        let status = if rule >= 3 {
            "skip:invalid_rule"
        } else {
            groups.entry(t.n(row, "id")).or_insert(rule);
            "loaded"
        };
        group_status.insert(*ordinal, status.into());
    }
    statuses.insert("spell_group".into(), group_status);
    let mut memberships = BTreeMap::new();
    let mut masks = BTreeMap::<u32, (u64, u32)>::new();
    let t = &aux["spell_group_spell"];
    for (ordinal, row) in &t.rows {
        let group = t.n(row, "id");
        let spell = t.n(row, "spellid");
        let status = if !groups.contains_key(&group) {
            "skip:missing_group"
        } else if !spells.contains_key(&spell) {
            "skip:missing_spell"
        } else {
            check((1..=64).contains(&group), "spells.stack_shift_undefined")?;
            masks.entry(spell).or_default().0 |= 1u64 << (group - 1);
            "loaded"
        };
        memberships.insert(*ordinal, status.into());
    }
    for (group, rule) in groups {
        for (_, row) in &t.rows {
            if t.n(row, "id") == group {
                if let Some((_, old)) = masks.get_mut(&t.n(row, "spellid")) {
                    if *old == 0 {
                        *old = rule;
                    }
                }
            }
        }
    }
    for (spell, (mask, rule)) in masks {
        plans.add(&json!(["stack", spell, [mask, u64::from(rule)]]))?;
    }
    statuses.insert("spell_group_spell".into(), memberships);
    let mut learned = BTreeMap::<u32, Vec<[u32; 3]>>::new();
    let t = &aux["spell_learn_spell"];
    let mut learn_status = BTreeMap::new();
    for (ordinal, row) in &t.rows {
        let a = t.n(row, "entry");
        let b = t.n(row, "spellid");
        let status = if !spells.contains_key(&a) || !spells.contains_key(&b) {
            "skip:missing_spell"
        } else if chains.talents.contains_key(&b) {
            "skip:talent"
        } else {
            learned
                .entry(a)
                .or_default()
                .push([b, t.n(row, "active"), 0]);
            "loaded"
        };
        learn_status.insert(*ordinal, status.into());
    }
    statuses.insert("spell_learn_spell".into(), learn_status);
    let mut skills = 0;
    for (&spell, w) in &spells {
        for i in 0..3 {
            if w[1 + i] == 118 && w[4 + i] & 65535 != 0 && w[7 + i].wrapping_add(1) & 65535 != 0 {
                plans.add(&json!([
                    "learn_skill",
                    spell,
                    [w[4 + i] & 65535, w[7 + i].wrapping_add(1) & 65535, 118]
                ]))?;
                skills += 1;
                break;
            }
        }
        if !t.rows.is_empty() {
            for i in 0..3 {
                let target = w[10 + i];
                if w[1 + i] == 36
                    && spells.contains_key(&target)
                    && !learned
                        .entry(spell)
                        .or_default()
                        .iter()
                        .any(|r| r[0] == target)
                {
                    let auto = w[13 + i] == 5
                        || chains.talents.contains_key(&spell)
                        || w[0] & 0x40 != 0
                        || w[1..4].contains(&44);
                    learned
                        .entry(spell)
                        .or_default()
                        .push([target, 1, u32::from(auto)]);
                }
            }
        }
    }
    for (spell, rows) in learned {
        for row in rows {
            plans.add(&json!(["learn_spell", spell, row]))?;
        }
    }
    statuses.insert(
        "spell_pet_auras".into(),
        pet_inputs(&aux["spell_pet_auras"], &spells),
    );
    statuses.extend(condition_inputs(
        catalog, &schema, &aux, &spells, &ids, &chains, &mut plans,
    )?);
    let lookup_fields = [
        ("SpellCastTimes.dbc", "castingtimeindex"),
        ("SpellRange.dbc", "rangeindex"),
        ("SpellDuration.dbc", "durationindex"),
        ("SpellCategory.dbc", "category"),
        ("SpellFocusObject.dbc", "requiresspellfocus"),
        ("SpellRadius.dbc", "effectradiusindex1"),
        ("SpellRadius.dbc", "effectradiusindex2"),
        ("SpellRadius.dbc", "effectradiusindex3"),
    ];
    let lookups: Vec<_> = lookup_fields
        .iter()
        .map(|(file, name)| {
            (
                *file,
                fields("spell_template")
                    .iter()
                    .position(|f| f[0] == *name)
                    .unwrap(),
            )
        })
        .collect();
    catalog.scan("spell_template", 0, 256, |row| {
        let cells = row.cells().collect::<Result<Vec<_>>>()?;
        let spell = spawns::uint(cells[id_index])?;
        for &(file, index) in &lookups {
            let key = spawns::uint(cells[index])?;
            if key != 0 || file == "SpellRange.dbc" {
                plans.add(&json!([
                    "dbc_lookup",
                    spell,
                    file,
                    key,
                    ids[file].contains(&key)
                ]))?;
            }
        }
        Ok(())
    })?;
    let mut handlers = BTreeMap::<(u32, u32), u64>::new();
    for w in spells.values() {
        for i in 0..3 {
            *handlers.entry((w[1 + i], w[19 + i])).or_default() += 1;
        }
    }
    for ((effect, aura), count) in handlers {
        plans.add(&json!(["handler_unverified", effect, aura, count]))?;
    }
    let mut families = serde_json::Map::new();
    for (name, spec) in POLICY["tables"].as_object().unwrap() {
        let mut raw = Sha256::new();
        let mut stream = Stream::new(output, name)?;
        let mut counts = BTreeMap::<String, u64>::new();
        let input = Table {
            names: fields(name)
                .iter()
                .map(|f| f[0].as_str().unwrap().to_owned())
                .collect(),
            rows: Vec::new(),
        };
        catalog.scan(name, 0, 256, |row| {
            raw.update(row.frame);
            let values = row
                .cells()
                .zip(fields(name))
                .map(|(c, f)| convert(c?, f[1].as_str().unwrap()))
                .collect::<Result<Vec<_>>>()?;
            let status = statuses
                .get(name)
                .and_then(|s| s.get(&row.ordinal))
                .map_or_else(
                    || basic_disposition(name, &input, &values, &spells, &chains.nodes, &ids),
                    String::as_str,
                );
            *counts.entry(status.to_owned()).or_default() += 1;
            stream.add(&json!({"ordinal":row.ordinal,"values":values,"disposition":status}))
        })?;
        families.insert(name.clone(),json!({"raw_sha256":hex(&raw.finalize()),"fields":spec["fields"],"source":spec["source"],"stream":stream.finish()?,"dispositions":counts,"whole_empty":catalog.relations[name].rows==0}));
    }
    let inventory = bounded_json(&catalog.object(&descriptor(&catalog.manifest["dbc_root"])?, 0)?)?;
    let client = dbc::open(catalog, &inventory, "Spell.dbc", None)?;
    let client_count = client.record_count();
    drop(client);
    let mut result = json!({"format":"benilla-spell-views-v1","catalog_sha256":catalog.manifest["catalog_sha256"],"policy_sha256":json::digest(&*POLICY),"families":families,"dbc_dependencies":dependencies,"client_spell":{"sha256":inventory["Spell.dbc"]["sha256"],"bytes":inventory["Spell.dbc"]["bytes"],"records":client_count,"server_store":false},"plans":plans.finish()?,"fallback_chain_rows":chains.fallback.len(),"chain_rows":chains.nodes.len(),"learn_skills":skills,"sql_spell_rows":spells.len(),"condition_rows":CONDITIONS.iter().map(|t|catalog.relations[*t].rows).sum::<u64>(),"complete":false,"refusals":REFUSALS});
    result["chain_next_cache_built"] = json!(!aux["spell_chain"].rows.is_empty());
    result["view_sha256"] = json!(json::digest(&result));
    check(
        json::canonical(&result).len() <= CHUNK,
        "spells.report_limit",
    )?;
    Ok(result)
}
impl VerifiedCatalog {
    /// Distinguishes a complete empty relation from an absent selected key.
    pub fn spell_lookup_state(&mut self, table: &str, key: &[u8]) -> Result<Value> {
        check(
            POLICY["tables"].get(table).is_some(),
            "spells.unknown_relation",
        )?;
        let whole_empty = self
            .relations
            .get(table)
            .ok_or_else(|| fail("spells.missing_relation"))?
            .rows
            == 0;
        let mut count = 0u64;
        self.lookup(table, key, |_| {
            count += 1;
            Ok(())
        })?;
        Ok(json!({"whole_empty":whole_empty,"selected_empty":count==0,"rows":count}))
    }
}

pub(crate) struct LoadedSpellChains {
    pub nodes: BTreeMap<u32, [u32; 4]>,
    pub order: Vec<u32>,
    pub rows: Vec<(u64, u32, String)>,
    pub build_next: bool,
}

impl VerifiedCatalog {
    /// Executes source field conversions over the complete retained relation.
    /// Ordinals are catalog order, not a sealed F1 SQL cursor.
    pub(crate) fn execution_spell_scan(
        &mut self,
        table: &str,
        mut visit: impl FnMut(u64, &[Value]) -> Result<()>,
    ) -> Result<()> {
        let spec = POLICY["tables"]
            .get(table)
            .ok_or_else(|| fail("spells.unknown_relation"))?;
        check(
            self.manifest["core_revision"] == POLICY["core_revision"],
            "spells.source_pin",
        )?;
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let names = schema["tables"][table]["columns"]
            .as_array()
            .ok_or_else(|| fail("spells.schema"))?;
        check(
            names
                .iter()
                .map(|c| c["COLUMN_NAME"].as_str().unwrap_or("").to_ascii_lowercase())
                .collect::<Vec<_>>()
                == spec["fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|f| f[0].as_str().unwrap().to_owned())
                    .collect::<Vec<_>>(),
            "spells.schema",
        )?;
        let mut index_wrap = false;
        if matches!(table, "spell_template" | "spell_cone") {
            // SQLStorageImpl.h:317 computes MAX(id)+1 before creating records.
            self.scan(table, 0, 256, |row| {
                index_wrap |=
                    spawns::uint(row.cells().next().ok_or_else(|| fail("spells.schema"))??)?
                        == u32::MAX;
                Ok(())
            })?;
        }
        self.scan(table, 0, 256, |row| {
            check(!index_wrap, "execution.spells.sql_index_write_undefined")?;
            let values = row
                .cells()
                .zip(fields(table))
                .map(|(c, f)| convert(c?, f[1].as_str().unwrap()))
                .collect::<Result<Vec<_>>>()?;
            visit(row.ordinal, &values)
        })
    }

    pub(crate) fn execution_spell_script_scan(
        &mut self,
        mut visit: impl FnMut(u64, u32, &[u8]) -> Result<()>,
    ) -> Result<()> {
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let cols = schema["tables"]["spell_scripts"]["columns"]
            .as_array()
            .ok_or_else(|| fail("spells.schema"))?;
        let column = |name: &str| {
            cols.iter()
                .position(|c| {
                    c["COLUMN_NAME"]
                        .as_str()
                        .is_some_and(|n| n.eq_ignore_ascii_case(name))
                })
                .ok_or_else(|| fail("spells.schema"))
        };
        let id = column("Id")?;
        let name = column("ScriptName")?;
        self.scan("spell_scripts", 0, 256, |row| {
            let c = row.cells().collect::<Result<Vec<_>>>()?;
            let name = c[name]
                .unwrap_or_default()
                .split(|b| *b == 0)
                .next()
                .unwrap_or_default();
            visit(row.ordinal, spawns::uint(c[id])?, name)
        })
    }

    pub(crate) fn execution_spell_chains(
        &mut self,
        spells: &BTreeMap<u32, [u32; 25]>,
        talents: &BTreeMap<u32, Vec<u32>>,
        abilities: &BTreeMap<u32, Vec<u32>>,
    ) -> Result<LoadedSpellChains> {
        let sql = read_table(self, "spell_chain", &mut 0)?;
        let loaded = chains(spells, talents, abilities, &sql)?;
        let rows = sql
            .rows
            .iter()
            .map(|(ordinal, row)| {
                (
                    *ordinal,
                    sql.n(row, "spell_id"),
                    loaded.dispositions[ordinal].clone(),
                )
            })
            .collect();
        Ok(LoadedSpellChains {
            nodes: loaded.nodes,
            order: loaded.order,
            rows,
            build_next: !sql.rows.is_empty(),
        })
    }

    pub(crate) fn execution_client_spell_ids(&mut self) -> Result<(String, usize, BTreeSet<u32>)> {
        let inventory = bounded_json(&self.object(&descriptor(&self.manifest["dbc_root"])?, 0)?)?;
        let mut view = dbc::open(
            self,
            &inventory,
            "Spell.dbc",
            Some(json!({"format":format!("n{}", "x".repeat(172)), "record_bytes":692})),
        )?;
        let count = view.record_count();
        let mut ids = BTreeSet::new();
        for ordinal in 0..count {
            ids.insert(
                view.record(ordinal)?["key"]
                    .as_u64()
                    .ok_or_else(|| fail("spells.client_layout"))? as u32,
            );
        }
        Ok((
            inventory["Spell.dbc"]["sha256"]
                .as_str()
                .ok_or_else(|| fail("spells.client_layout"))?
                .to_owned(),
            count,
            ids,
        ))
    }
}
#[cfg(test)]
mod tests;
