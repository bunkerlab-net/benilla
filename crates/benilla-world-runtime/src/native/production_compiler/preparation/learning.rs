// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
type Edge = (String, i128, String, i128, String, usize);
struct Discovery {
    nodes: BTreeMap<&'static str, BTreeSet<i128>>,
    pending: Vec<(&'static str, i128)>,
    edges: BTreeSet<Edge>,
    abilities: BTreeMap<u32, Value>,
    infos: BTreeMap<u32, Value>,
    learn: BTreeSet<(i128, i128)>,
    rank: BTreeSet<(i128, i128)>,
    references: BTreeSet<(String, i128)>,
    skill_steps: BTreeMap<i128, u16>,
    required_probes: BTreeSet<i128>,
    dispositions: BTreeMap<u32, &'static str>,
}
impl Discovery {
    fn add(
        &mut self,
        kind: &'static str,
        id: i128,
        parent: Option<(&str, i128, &str, usize)>,
    ) -> Result<()> {
        check(
            id > 0 && id <= if kind == "skill" { 65535 } else { 0xffffffff },
            "learning.dependency_id_domain",
        )?;
        if let Some((origin, from, role, slot)) = parent {
            self.edges
                .insert((origin.into(), from, kind.into(), id, role.into(), slot));
            check(self.edges.len() <= 16384, "learning.edge_limit")?;
        }
        if self.nodes.get_mut(kind).unwrap().insert(id) {
            check(
                self.nodes[kind].len()
                    <= match kind {
                        "skill" | "spell" => 512,
                        _ => 2048,
                    },
                &format!("learning.{kind}_limit"),
            )?;
            self.pending.push((kind, id));
        }
        Ok(())
    }
    fn lookup<'a>(
        &mut self,
        c: &mut Compiler<'a>,
        table: &str,
        id: i128,
        columns: impl IntoIterator<Item = usize>,
    ) -> Result<Option<Row<'a>>> {
        self.references.insert((table.into(), id));
        check(
            self.references.len() <= 16384,
            "learning.dbc_reference_limit",
        )?;
        c.lookup(table, id, columns)
    }
    fn ability<'a>(&mut self, c: &mut Compiler<'a>, row: Row<'a>) -> Result<()> {
        if self.abilities.contains_key(&row.word(0)) {
            return Ok(());
        }
        self.abilities.insert(row.word(0), row.raw());
        check(self.abilities.len() <= 4096, "learning.ability_row_limit")?;
        c.use_row(row, 0..15)?;
        for (kind, field, role) in [
            ("skill", 1, "ability_skill"),
            ("spell", 2, "ability_spell"),
            ("spell", 8, "ability_forward_spell"),
        ] {
            let id = row.word(field);
            if id != 0 {
                self.add(
                    kind,
                    id as i128,
                    Some(("SkillLineAbility", row.word(0) as i128, role, 0)),
                )?;
            }
        }
        Ok(())
    }
    fn training_disposition(
        &mut self,
        c: &Compiler<'_>,
        row: Row<'_>,
        spells: &BTreeMap<i128, &Value>,
        probes: &BTreeMap<i128, &Value>,
        probe_ids: &BTreeSet<i128>,
        infos: &BTreeMap<u32, Vec<Row<'_>>>,
    ) -> Result<&'static str> {
        let skill = row.word(1) as i128;
        let spell = row.word(2) as i128;
        self.required_probes.insert(spell);
        check(
            self.required_probes.len() <= 4096,
            "learning.ability_row_limit",
        )?;
        let step = self.skill_steps.get(&skill).copied().unwrap_or(0);
        let race_mask = 1u32 << (c.request.race - 1);
        let class_mask = 1u32 << (c.request.class - 1);
        if row.word(3) != 0 && row.word(3) & race_mask == 0
            || row.word(4) != 0 && row.word(4) & class_mask == 0
        {
            return Ok("unmatched_masks");
        }
        if row.word(9) == 0 {
            let source = spells.get(&spell).or_else(|| probes.get(&spell)).copied();
            if source.is_none() && !probe_ids.contains(&spell) {
                return Ok("repair_source_unknown");
            }
            let trained = source.and_then(super::super::extensions::skill_training::training);
            return Ok(
                if trained.is_some_and(|(trained_skill, trained_step)| {
                    trained_skill as i128 == skill && trained_step < step
                }) {
                    "profession_repair"
                } else {
                    "acquire_method_excluded"
                },
            );
        }
        let mut value = 65535u32;
        if let Some(line) = c.dbcs["SkillLine"].get(skill as u32) {
            let category = line.word(1);
            if category == 10 {
                value = 300;
            } else if category == 6 && skill != 473 || [40, 633].contains(&skill) {
                value = 1;
            } else if [6, 7, 8, 9, 11].contains(&category) {
                if let Some(info) = infos
                    .get(&(skill as u32))
                    .into_iter()
                    .flatten()
                    .find(|r| r.word(2) & race_mask != 0 && r.word(3) & class_mask != 0)
                {
                    value = 1;
                    if info.word(4) & 0x10 != 0 {
                        if info.word(6) == 0 {
                            value = 5;
                        } else {
                            let step = if step == 0 { 1 } else { step };
                            value = if step <= 16 {
                                c.dbcs["SkillTiers"]
                                    .get(info.word(6))
                                    .map_or(65535, |tier| tier.word(16 + step as usize) & 0xffff)
                            } else {
                                65535
                            };
                        }
                    }
                }
            }
        }
        Ok(if row.word(7) > value {
            "minimum_value_excluded"
        } else {
            "auto_training"
        })
    }
    fn effects(
        &mut self,
        c: &mut Compiler<'_>,
        id: i128,
        effects: impl IntoIterator<Item = (usize, i128, i128, i128, i128)>,
        origin: &str,
    ) -> Result<()> {
        for (slot, effect, misc, trigger, step) in effects {
            if [44, 118].contains(&effect) {
                if misc > 0 {
                    self.add(
                        "skill",
                        misc,
                        Some((
                            origin,
                            id,
                            if effect == 44 { "skill_step" } else { "skill" },
                            slot,
                        )),
                    )?;
                    let step = step as u16;
                    if step > self.skill_steps.get(&misc).copied().unwrap_or(0) {
                        self.skill_steps.insert(misc, step);
                        self.pending.push(("skill", misc));
                    }
                } else {
                    c.context_issue("spell_template",json!([num(id)]),"player.learning_skill_effect_invalid",json!({"slot":slot,"effect_id":num(effect),"raw_misc":num(misc),"source":origin}));
                }
            }
            if trigger != 0 {
                self.add(
                    "spell",
                    trigger,
                    Some((
                        origin,
                        id,
                        if effect == 36 {
                            "learn_spell"
                        } else {
                            "raw_trigger_reference"
                        },
                        slot,
                    )),
                )?;
                if effect == 36 {
                    self.learn.insert((id, trigger));
                }
            }
        }
        Ok(())
    }
}
fn cycle(pairs: &BTreeSet<(i128, i128)>) -> bool {
    let mut adjacency: BTreeMap<i128, BTreeSet<i128>> = BTreeMap::new();
    let mut degrees: BTreeMap<i128, usize> = BTreeMap::new();
    for (a, b) in pairs {
        adjacency.entry(*a).or_default().insert(*b);
        degrees.entry(*a).or_default();
        *degrees.entry(*b).or_default() += 1;
    }
    let mut ready: Vec<_> = degrees
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(n, _)| *n)
        .collect();
    let mut count = 0;
    while let Some(n) = ready.pop() {
        count += 1;
        for end in adjacency.get(&n).into_iter().flatten() {
            let d = degrees.get_mut(end).unwrap();
            *d -= 1;
            if *d == 0 {
                ready.push(*end);
            }
        }
    }
    count != degrees.len()
}
pub(super) fn discover(c: &mut Compiler<'_>) -> Result<Value> {
    let source = c.source;
    let spells: BTreeMap<_, _> = rows(source, "spell_template")
        .iter()
        .map(|r| (int(&r["Id"]), r))
        .collect();
    let items: BTreeMap<_, _> = rows(source, "item_template")
        .iter()
        .map(|r| (int(&r["entry"]), r))
        .collect();
    let mut d = Discovery {
        nodes: [
            ("skill", BTreeSet::new()),
            ("spell", BTreeSet::new()),
            ("item", BTreeSet::new()),
        ]
        .into_iter()
        .collect(),
        pending: Vec::new(),
        edges: BTreeSet::new(),
        abilities: BTreeMap::new(),
        infos: BTreeMap::new(),
        learn: BTreeSet::new(),
        rank: BTreeSet::new(),
        references: BTreeSet::new(),
        skill_steps: BTreeMap::new(),
        required_probes: BTreeSet::new(),
        dispositions: BTreeMap::new(),
    };
    let probes = source.get("skill_training_capture");
    let probe_rows: BTreeMap<_, _> = probes
        .into_iter()
        .flat_map(|v| v["rows"].as_array().unwrap())
        .map(|r| (int(&r["Id"]), r))
        .collect();
    let probe_ids: BTreeSet<_> = probes
        .into_iter()
        .flat_map(|v| v["spell_ids"].as_array().unwrap())
        .map(int)
        .collect();
    if let Some(probes) = probes {
        check(
            probes["dbc_sha256"]
                == json!(c
                    .dbcs
                    .iter()
                    .map(|(name, table)| (name, &table.sha256))
                    .collect::<BTreeMap<_, _>>()),
            "skill_training.dbc_binding",
        )?;
    }
    let race_mask = 1i128 << (c.request.race - 1);
    let class_mask = 1i128 << (c.request.class - 1);
    for row in rows(source, "playercreateinfo_skills") {
        if (int(&row["raceMask"]) == 0 || int(&row["raceMask"]) & race_mask != 0)
            && (int(&row["classMask"]) == 0 || int(&row["classMask"]) & class_mask != 0)
        {
            let skill = int(&row["skill"]);
            let step = int(&row["step"]) as u16;
            let current = d.skill_steps.entry(skill).or_default();
            *current = (*current).max(step);
        }
    }
    let mut by_skill: BTreeMap<u32, Vec<Row<'_>>> = BTreeMap::new();
    let mut by_spell = by_skill.clone();
    let mut by_forward = by_skill.clone();
    let mut infos = by_skill.clone();
    let mut talents = by_skill.clone();
    let table = &c.dbcs["SkillLineAbility"];
    for i in table.by_id.values() {
        let r = table.row(*i);
        by_skill.entry(r.word(1)).or_default().push(r);
        by_spell.entry(r.word(2)).or_default().push(r);
        if r.word(8) != 0 {
            by_forward.entry(r.word(8)).or_default().push(r);
        }
    }
    let table = &c.dbcs["SkillRaceClassInfo"];
    for i in table.by_id.values() {
        let r = table.row(*i);
        infos.entry(r.word(1)).or_default().push(r);
    }
    let table = &c.dbcs["Talent"];
    for i in table.by_id.values() {
        let r = table.row(*i);
        for spell in (4..9)
            .map(|i| r.word(i))
            .filter(|id| *id != 0)
            .collect::<BTreeSet<_>>()
        {
            talents.entry(spell).or_default().push(r);
        }
    }
    let chains: BTreeMap<_, _> = rows(source, "spell_chain")
        .iter()
        .map(|r| (int(&r["spell_id"]), r))
        .collect();
    let mut sql_learn: BTreeMap<i128, Vec<&Value>> = BTreeMap::new();
    for row in rows(source, "spell_learn_spell") {
        sql_learn.entry(int(&row["entry"])).or_default().push(row);
    }
    for r in rows(source, "playercreateinfo_skills") {
        d.add("skill", int(&r["skill"]), None)?;
    }
    for id in spells.keys() {
        d.add("spell", *id, None)?;
    }
    for id in items.keys() {
        d.add("item", *id, None)?;
    }
    let packed =
        c.request.race as u32 | ((c.request.class as u32) << 8) | ((c.request.gender as u32) << 16);
    let outfits: Vec<_> = c.dbcs["CharStartOutfit"]
        .by_id
        .values()
        .map(|i| c.dbcs["CharStartOutfit"].row(*i))
        .filter(|r| r.word(0) != 0 && r.word(1) == packed)
        .collect();
    if outfits.len() == 1 {
        let outfit = outfits[0];
        for slot in 0..12 {
            let id = outfit.word(2 + slot);
            if id as i32 > 0 {
                d.add(
                    "item",
                    id as i128,
                    Some((
                        "CharStartOutfit",
                        outfit.word(0) as i128,
                        "outfit_item",
                        slot,
                    )),
                )?;
            }
        }
    }
    for (table, id, _) in &c.dependencies {
        if table == "item_template" {
            d.add("item", *id, None)?;
        }
    }
    for r in rows(source, "quest_template") {
        if nonzero(&r["RequiredSkill"]) {
            d.add(
                "skill",
                int(&r["RequiredSkill"]),
                Some((
                    "quest_template",
                    int(&r["entry"]),
                    "quest_required_skill",
                    0,
                )),
            )?;
        }
    }
    while let Some((kind, id)) = d.pending.pop() {
        match kind {
            "skill" => {
                d.lookup(c, "SkillLine", id, [0, 1])?;
                for row in infos.get(&(id as u32)).into_iter().flatten() {
                    c.use_row(*row, 0..8)?;
                    d.infos.insert(row.word(0), row.raw());
                    if row.word(6) != 0 {
                        d.lookup(c, "SkillTiers", row.word(6) as i128, 0..33)?;
                    }
                }
                for row in by_skill.get(&(id as u32)).into_iter().flatten() {
                    c.use_row(*row, 0..15)?;
                    let disposition =
                        d.training_disposition(c, *row, &spells, &probe_rows, &probe_ids, &infos)?;
                    d.dispositions.insert(row.word(0), disposition);
                    check(d.dispositions.len() <= 4096, "learning.ability_row_limit")?;
                    if disposition == "repair_source_unknown" {
                        c.context_issue(
                            "SkillLineAbility",
                            json!([row.word(0)]),
                            "player.learning_repair_probe_unknown",
                            json!({"spell_id":row.word(2)}),
                        );
                    } else if ![
                        "unmatched_masks",
                        "acquire_method_excluded",
                        "minimum_value_excluded",
                    ]
                    .contains(&disposition)
                    {
                        d.ability(c, *row)?;
                    }
                }
            }
            "spell" => {
                if let Some(s) = spells.get(&id) {
                    d.effects(
                        c,
                        id,
                        (1..=3).map(|i| {
                            (
                                i,
                                int(&s[format!("Effect{i}")]),
                                int(&s[format!("EffectMiscValue{i}")]),
                                int(&s[format!("EffectTriggerSpell{i}")]),
                                int(&s[format!("EffectBasePoints{i}")]) + 1,
                            )
                        }),
                        "spell_template",
                    )?;
                    for (table, field) in [
                        ("SpellCastTimes", "CastingTimeIndex"),
                        ("SpellRange", "rangeIndex"),
                        ("SpellDuration", "DurationIndex"),
                        ("SpellCategory", "Category"),
                        ("SpellFocusObject", "RequiresSpellFocus"),
                    ] {
                        if nonzero(&s[field]) || table == "SpellRange" {
                            d.lookup(c, table, int(&s[field]), [0])?;
                        }
                    }
                    for slot in 1..=3 {
                        let id = int(&s[format!("EffectRadiusIndex{slot}")]);
                        if id != 0 {
                            d.lookup(c, "SpellRadius", id, [0])?;
                        }
                    }
                } else {
                    c.need(
                        "spell_template",
                        id,
                        "player.skill_candidate_spell_not_captured",
                    );
                    if let Some(projection) = probe_rows.get(&id) {
                        d.effects(
                            c,
                            id,
                            (1..=3).map(|i| {
                                (
                                    i,
                                    int(&projection[format!("Effect{i}")]),
                                    int(&projection[format!("EffectMiscValue{i}")]),
                                    0,
                                    int(&projection[format!("EffectBasePoints{i}")]) + 1,
                                )
                            }),
                            "spell_template",
                        )?;
                    }
                }
                if let Some(row) = d.lookup(c, "Spell", id, (58..115).chain(40..58))? {
                    for (table, field) in [
                        ("SpellCastTimes", 18),
                        ("SpellRange", 36),
                        ("SpellDuration", 30),
                        ("SpellCategory", 2),
                        ("SpellFocusObject", 15),
                    ] {
                        if row.word(field) != 0 || table == "SpellRange" {
                            d.lookup(c, table, row.word(field) as i128, [0])?;
                        }
                    }
                    for field in 88..91 {
                        if row.word(field) != 0 {
                            d.lookup(c, "SpellRadius", row.word(field) as i128, [0])?;
                        }
                    }
                    d.effects(
                        c,
                        id,
                        (0..3).map(|i| {
                            (
                                i + 1,
                                row.word(61 + i) as i128,
                                row.word(106 + i) as i32 as i128,
                                row.word(109 + i) as i128,
                                row.word(76 + i) as i32 as i128 + 1,
                            )
                        }),
                        "Spell",
                    )?;
                    for item in (40..42)
                        .map(|i| row.word(i) as i128)
                        .chain((42..50).map(|i| row.word(i) as i32 as i128))
                        .collect::<BTreeSet<_>>()
                    {
                        if item > 0 {
                            d.add("item", item, Some(("Spell", id, "reagent_or_totem", 0)))?;
                        }
                    }
                }
                for row in by_spell
                    .get(&(id as u32))
                    .into_iter()
                    .flatten()
                    .chain(by_forward.get(&(id as u32)).into_iter().flatten())
                {
                    d.ability(c, *row)?;
                }
                for talent in talents.get(&(id as u32)).into_iter().flatten() {
                    c.use_row(*talent, 0..21)?;
                    for reference in (4..9)
                        .map(|i| talent.word(i))
                        .chain([talent.word(20)])
                        .filter(|i| *i != 0)
                        .collect::<BTreeSet<_>>()
                    {
                        d.add(
                            "spell",
                            reference as i128,
                            Some((
                                "Talent",
                                talent.word(0) as i128,
                                "rank_or_required_spell",
                                0,
                            )),
                        )?;
                    }
                }
                if let Some(chain) = chains.get(&id) {
                    for field in ["prev_spell", "first_spell", "req_spell"] {
                        if nonzero(&chain[field]) {
                            d.add(
                                "spell",
                                int(&chain[field]),
                                Some(("spell_chain", id, field, 0)),
                            )?;
                        }
                    }
                    if nonzero(&chain["prev_spell"]) {
                        d.rank.insert((id, int(&chain["prev_spell"])));
                    }
                }
                for row in sql_learn.get(&id).into_iter().flatten() {
                    let target = int(&row["SpellID"]);
                    d.add(
                        "spell",
                        target,
                        Some(("spell_learn_spell", id, "learn_spell", 0)),
                    )?;
                    d.learn.insert((id, target));
                }
            }
            _ => {
                if let Some(row) = items.get(&id) {
                    if nonzero(&row["RequiredSkill"]) {
                        d.add(
                            "skill",
                            int(&row["RequiredSkill"]),
                            Some(("item_template", id, "required_skill", 0)),
                        )?;
                    }
                    for field in ["requiredspell".into()]
                        .into_iter()
                        .chain((1..=5).map(|i| format!("spellid_{i}")))
                    {
                        let target = int(&row[&field]);
                        if target > 0 {
                            d.add("spell", target, Some(("item_template", id, &field, 0)))?;
                        }
                    }
                } else {
                    c.need("item_template", id, "player.learning_item_not_captured");
                }
            }
        }
    }
    for (pairs, reason) in [
        (&d.learn, "player.learning_spell_cycle"),
        (&d.rank, "player.learning_rank_cycle"),
    ] {
        if cycle(pairs) {
            c.issue("spell_template", json!([]), reason);
        }
    }
    let probe_scope_matches = probes.is_none() || probe_ids == d.required_probes;
    if !probe_scope_matches {
        c.issue(
            "SkillLineAbility",
            json!([]),
            "player.learning_training_probe_scope_mismatch",
        );
    }
    let coverage = source.get("learning_capture");
    if coverage.is_none() {
        c.issue(
            "spell_learn_spell",
            json!([]),
            "player.server_learning_relationships_not_captured",
        );
    }
    let complete = d.nodes["spell"].iter().all(|id| spells.contains_key(id))
        && d.nodes["item"].iter().all(|id| items.contains_key(id))
        && coverage.is_some()
        && !d
            .dispositions
            .values()
            .any(|v| *v == "repair_source_unknown")
        && probe_scope_matches
        && d.references
            .iter()
            .all(|(t, id)| c.dbcs.get(t).and_then(|t| t.get(*id as u32)).is_some());
    Ok(json!({
        "format":"benilla-native-learning-dependencies-v1", "fixed_point_reached":true,
        "scope":"skill_ability_effect_item_and_server_learning_dependencies",
        "captured_inputs_complete":complete, "effective_learning_proven":false,
        "skill_ids":numbers(d.nodes["skill"].iter().copied()),
        "spell_ids":numbers(d.nodes["spell"].iter().copied()),
        "item_ids":numbers(d.nodes["item"].iter().copied()), "sql_relationship_coverage":coverage,
        "dbc_references":d.references.iter().map(|(t,id)|json!({"table":t,"id":num(*id)})).collect::<Vec<_>>(),
        "server_relationships":{"spell_chain":source["tables"].get("spell_chain"),"spell_learn_spell":source["tables"].get("spell_learn_spell")},
        "talent_membership":"raw_table_captured_effective_loader_unproven", "external_initial_hooks":"not_proven",
        "derived_learn_spell_scan":coverage.map_or("not_captured",|v|if v["global_learn_spell_probe"]["rows"].as_array().unwrap().is_empty(){"skipped_for_empty_table"}else{"enabled_by_nonempty_table"}),
        "edges":d.edges.iter().map(|(a,b,kind,id,role,slot)|json!({"from_table":a,"from_id":num(*b),"to_kind":kind,"to_id":num(*id),"role":role,"slot":slot})).collect::<Vec<_>>(),
        "race_class_candidates":d.infos.values().collect::<Vec<_>>(),
        "training_probe_ids":numbers(d.required_probes),
        "ability_dispositions":d.dispositions.into_iter().map(|(id,value)|json!({"row_id":id,"disposition":value})).collect::<Vec<_>>(),
        "ability_candidates":d.abilities.values().collect::<Vec<_>>(),
        "dependency_request":{"format":"benilla-native-dependency-request-v1","core_revision":source["core_revision"],"selection":source["selection"],"source_sha256":c.source_sha,"dbc_sha256":c.dbcs.iter().map(|(n,t)|(n,&t.sha256)).collect::<BTreeMap<_,_>>(),"item_ids":numbers(d.nodes["item"].iter().copied()),"spell_ids":numbers(d.nodes["spell"].iter().copied())}
    }))
}
fn refusal(refusals: &mut Vec<Value>, table: &str, id: i128, reason: &str) {
    refusals.push(
        json!({"table":table,"key":if id==0{json!([])}else{json!([num(id)])},"reason":reason}),
    );
}
fn existence(refusals: &mut Vec<Value>, spells: &BTreeMap<i128, &Value>, id: i128) -> bool {
    if spells.contains_key(&id) {
        true
    } else {
        refusal(
            refusals,
            "spell_template",
            id,
            "player.learning_server_existence_unknown",
        );
        false
    }
}
fn chain(spell: i128, prev: i128, first: i128, rank: i128, req: i128) -> Value {
    json!({"spell":num(spell),"previous":num(prev),"first":num(first),"rank":num(rank),"required":num(req)})
}
pub(super) fn plan(source: &Value, dbcs: &Tables, closure: &Value) -> Value {
    let coverage = source.get("learning_capture");
    let mut refusals = Vec::new();
    if coverage.is_none_or(|v| v["format"] != "benilla-classic-learning-selection-v2") {
        refusal(
            &mut refusals,
            "spell_chain",
            0,
            "player.learning_global_chain_probe_not_captured",
        );
    }
    if closure["captured_inputs_complete"] != true {
        refusal(
            &mut refusals,
            "spell_template",
            0,
            "player.learning_loader_closure_incomplete",
        );
    }
    if !refusals.is_empty() {
        return json!({"format":"benilla-native-starter-learning-plan-v1","status":"blocked","loaded_learning_maps_resolved":false,"effective_learning_proven":false,"refusals":refusals});
    }
    let coverage = coverage.unwrap();
    let lines = &dbcs["SkillLine"];
    let tiers = &dbcs["SkillTiers"];
    let table = &dbcs["SkillRaceClassInfo"];
    let mut infos: BTreeMap<u32, Vec<Row<'_>>> = BTreeMap::new();
    for i in table.by_id.values() {
        let row = table.row(*i);
        if lines.get(row.word(1)).is_some() {
            infos.entry(row.word(1)).or_default().push(row);
        }
    }
    let race = int(&source["selection"]["starter"]["race"]);
    let class = int(&source["selection"]["starter"]["class"]);
    let race_mask = 1u32 << (race as u32 - 1);
    let class_mask = 1u32 << (class as u32 - 1);
    let mut defaults = Vec::new();
    let mut dispositions = Vec::new();
    let mut seen = BTreeSet::new();
    for row in rows(source, "playercreateinfo_skills") {
        let skill = int(&row["skill"]);
        let step = int(&row["step"]);
        if !(1..=65535).contains(&skill) || !(0..=16).contains(&step) {
            refusal(
                &mut refusals,
                "playercreateinfo_skills",
                skill,
                "player.learning_skill_step_domain",
            );
            continue;
        }
        if nonzero(&row["raceMask"]) && int(&row["raceMask"]) & race_mask as i128 == 0
            || nonzero(&row["classMask"]) && int(&row["classMask"]) & class_mask as i128 == 0
        {
            dispositions.push(json!({"skill":num(skill),"disposition":"unmatched_sql_masks"}));
            continue;
        }
        if lines.get(skill as u32).is_none() {
            refusal(
                &mut refusals,
                "SkillLine",
                skill,
                "player.learning_loader_table_missing",
            );
            continue;
        }
        let mut obtainable = false;
        for info in infos.get(&(skill as u32)).into_iter().flatten() {
            if info.word(2) & race_mask == 0 || info.word(3) & class_mask == 0 {
                continue;
            }
            if step != 0 {
                let Some(tier) = tiers.get(info.word(6)) else {
                    refusal(
                        &mut refusals,
                        "SkillTiers",
                        info.word(6) as i128,
                        "player.learning_loader_table_missing",
                    );
                    continue;
                };
                if tier.word(16 + step as usize) == 0 {
                    continue;
                }
            }
            obtainable = true;
            break;
        }
        dispositions.push(
            json!({"skill":num(skill),"disposition":if obtainable{"loaded"}else{"unobtainable"}}),
        );
        if obtainable {
            if !seen.insert(skill) {
                refusal(
                    &mut refusals,
                    "playercreateinfo_skills",
                    skill,
                    "player.learning_duplicate_effective_skill",
                );
            }
            defaults.push(json!({"skill":num(skill),"step":num(step)}));
        }
    }
    let spells: BTreeMap<_, _> = rows(source, "spell_template")
        .iter()
        .map(|r| (int(&r["Id"]), r))
        .collect();
    let abilities = closure["ability_candidates"].as_array().unwrap();
    let selected: BTreeSet<_> = closure["spell_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(int)
        .collect();
    let mut ranks: BTreeMap<i128, Value> = BTreeMap::new();
    let mut talents = BTreeSet::new();
    let mut positions: BTreeMap<i128, (Row<'_>, usize)> = BTreeMap::new();
    let table = &dbcs["Talent"];
    for i in table.by_id.values() {
        let w = table.row(*i);
        if !(4..9).any(|i| selected.contains(&(w.word(i) as i128))) {
            continue;
        }
        for slot in 0..5 {
            let spell = w.word(4 + slot) as i128;
            if spell == 0 {
                continue;
            }
            if positions.insert(spell, (w, slot)).is_some() {
                refusal(
                    &mut refusals,
                    "Talent",
                    spell,
                    "player.learning_ambiguous_talent_membership",
                );
            }
            if !existence(&mut refusals, &spells, spell) {
                continue;
            }
            talents.insert(spell);
            if w.word(5) != 0 {
                ranks.insert(
                    spell,
                    chain(
                        spell,
                        if slot != 0 {
                            w.word(3 + slot) as i128
                        } else {
                            0
                        },
                        w.word(4) as i128,
                        slot as i128 + 1,
                        0,
                    ),
                );
            }
        }
        if w.word(20) != 0 {
            existence(&mut refusals, &spells, w.word(20) as i128);
        }
    }
    let ability_spells: BTreeSet<_> = abilities.iter().map(|r| int(&r["words"][2])).collect();
    let mut predecessors: BTreeMap<i128, i128> = BTreeMap::new();
    let mut predecessor_order = Vec::new();
    for row in abilities {
        let w = &row["words"];
        let spell = int(&w[2]);
        let forward = if spell == 2366 { 2368 } else { int(&w[8]) };
        if forward == 0
            || !existence(&mut refusals, &spells, spell)
            || !existence(&mut refusals, &spells, forward)
            || !ability_spells.contains(&forward)
        {
            continue;
        }
        let prev = ranks
            .get(&forward)
            .map(|r| int(&r["previous"]))
            .or(predecessors.get(&forward).copied())
            .unwrap_or(spell);
        if prev != spell {
            refusal(
                &mut refusals,
                "SkillLineAbility",
                forward,
                "player.learning_rank_predecessor_conflict",
            );
        }
        if !ranks.contains_key(&forward) {
            if !predecessors.contains_key(&forward) {
                predecessor_order.push(forward);
            }
            predecessors.insert(forward, spell);
        }
    }
    for spell in predecessor_order {
        let mut cursor = spell;
        let mut path = Vec::new();
        while !ranks.contains_key(&cursor) {
            if path.contains(&cursor) || path.len() > 30 {
                refusal(
                    &mut refusals,
                    "SkillLineAbility",
                    spell,
                    "player.learning_rank_cycle_or_depth",
                );
                break;
            }
            let Some(prev) = predecessors.get(&cursor) else {
                ranks.insert(cursor, chain(cursor, 0, cursor, 1, 0));
                break;
            };
            path.push(cursor);
            cursor = *prev;
        }
        if !ranks.contains_key(&cursor) {
            continue;
        }
        for current in path.into_iter().rev() {
            let prev = predecessors[&current];
            if int(&ranks[&prev]["rank"]) >= 32 {
                refusal(
                    &mut refusals,
                    "SkillLineAbility",
                    current,
                    "player.learning_rank_cycle_or_depth",
                );
                break;
            }
            ranks.insert(
                current,
                chain(
                    current,
                    prev,
                    int(&ranks[&prev]["first"]),
                    int(&ranks[&prev]["rank"]) + 1,
                    0,
                ),
            );
        }
    }
    for row in rows(source, "spell_chain") {
        let spell = int(&row["spell_id"]);
        let prev = int(&row["prev_spell"]);
        let first = int(&row["first_spell"]);
        let rank = int(&row["rank"]);
        let req = int(&row["req_spell"]);
        for reference in [spell, prev, first, req]
            .into_iter()
            .filter(|n| *n != 0)
            .collect::<BTreeSet<_>>()
        {
            existence(&mut refusals, &spells, reference);
        }
        if let Some(derived) = ranks.get_mut(&spell) {
            if (
                int(&derived["previous"]),
                int(&derived["first"]),
                int(&derived["rank"]),
            ) != (prev, first, rank)
            {
                refusal(
                    &mut refusals,
                    "spell_chain",
                    spell,
                    "player.learning_sql_derived_rank_conflict",
                );
            }
            derived["required"] = num(req);
        } else {
            if first == 0 || (spell == first) != (rank <= 1) || (spell == first) != (prev == 0) {
                refusal(
                    &mut refusals,
                    "spell_chain",
                    spell,
                    "player.learning_chain_integrity",
                );
            }
            if let Some((w, slot)) = positions.get(&spell) {
                if rank != *slot as i128 + 1
                    || first != w.word(4) as i128
                    || req != w.word(20) as i128
                    || *slot != 0 && prev != w.word(3 + slot) as i128
                {
                    refusal(
                        &mut refusals,
                        "spell_chain",
                        spell,
                        "player.learning_chain_integrity",
                    );
                }
            }
            ranks.insert(spell, chain(spell, prev, first, rank, req));
        }
    }
    for (spell, row) in &ranks {
        let previous = int(&row["previous"]);
        if previous != 0
            && ranks.get(&previous).is_none_or(|prev| {
                prev["first"] != row["first"] || int(&prev["rank"]) + 1 != int(&row["rank"])
            })
        {
            refusal(
                &mut refusals,
                "spell_chain",
                *spell,
                "player.learning_chain_integrity",
            );
        }
        let required = int(&row["required"]);
        if required != 0
            && ranks
                .get(&required)
                .is_none_or(|req| req["first"] == row["first"] || nonzero(&req["required"]))
        {
            refusal(
                &mut refusals,
                "spell_chain",
                *spell,
                "player.learning_chain_integrity",
            );
        }
    }
    let mut learned: BTreeMap<i128, Vec<Value>> = BTreeMap::new();
    if !coverage["global_learn_spell_probe"]["rows"]
        .as_array()
        .unwrap()
        .is_empty()
    {
        for row in rows(source, "spell_learn_spell") {
            let spell = int(&row["entry"]);
            let target = int(&row["SpellID"]);
            if talents.contains(&target) {
                refusal(
                    &mut refusals,
                    "spell_learn_spell",
                    spell,
                    "player.learning_sql_talent_target",
                );
            }
            if existence(&mut refusals, &spells, spell) && existence(&mut refusals, &spells, target)
            {
                learned.entry(spell).or_default().push(json!({"spell_id":num(target),"active":nonzero(&row["Active"]),"auto_learned":false}));
            }
        }
        for (spell, row) in &spells {
            for slot in 1..=3 {
                if int(&row[format!("Effect{slot}")]) != 36 {
                    continue;
                }
                let target = int(&row[format!("EffectTriggerSpell{slot}")]);
                if !existence(&mut refusals, &spells, target) {
                    continue;
                }
                let links = learned.entry(*spell).or_default();
                if links.iter().any(|l| int(&l["spell_id"]) == target) {
                    continue;
                }
                let auto = int(&row[format!("EffectImplicitTargetA{slot}")]) == 5
                    || talents.contains(spell)
                    || int(&row["Attributes"]) & 0x40 != 0
                    || (1..=3).any(|i| int(&row[format!("Effect{i}")]) == 44);
                links.push(json!({"spell_id":num(target),"active":true,"auto_learned":auto}));
            }
        }
    }
    let pairs = learned
        .iter()
        .flat_map(|(from, links)| links.iter().map(move |l| (*from, int(&l["spell_id"]))))
        .collect();
    if cycle(&pairs) {
        refusal(
            &mut refusals,
            "spell_learn_spell",
            0,
            "player.learning_spell_cycle",
        );
    }
    json!({"format":"benilla-native-starter-learning-plan-v1","status":if refusals.is_empty(){"supported"}else{"blocked"},"loaded_learning_maps_resolved":refusals.is_empty(),"effective_learning_proven":false,"source_identity":closure["dependency_request"]["source_sha256"],"selected_spell_ids":numbers(selected),"default_skill_candidates":defaults,"default_spell_candidates":rows(source,"playercreateinfo_spell").iter().map(|r|r["Spell"].clone()).collect::<Vec<_>>(),"root_order":"unknown_canonical_capture_order_is_not_loaded_order","skill_dispositions":dispositions,"ranks":ranks.values().collect::<Vec<_>>(),"talent_spell_ids":numbers(talents),"learned_spells":learned.iter().map(|(id,links)|json!({"entry":num(*id),"links":links})).collect::<Vec<_>>(),"numeric_overlay":"requires_independent_complete_effective_projection","initial_casts":"requires_independent_hooks_numeric_order_and_source_dispatch","refusals":refusals})
}
