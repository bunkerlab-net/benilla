// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS 8ec338a1, SQLStorages.cpp:40,55; DBCStructure.h:716-814.
// SpellMgr.cpp:56-73,137-141 defines missing duration/cast-time lookups.
// Checked inputs are not loaded order, hook absence, or an effective spellbook.
use super::*;
use benilla_sim::starter_init::{
    CapturedLearnSpell, CapturedSpellChain, CapturedStarterLearning, CreationSkill, CreationSpell,
    LearningSpellNumeric, Rows,
};

fn words<const N: usize>(row: Row<'_>) -> [u32; N] {
    std::array::from_fn(|i| row.word(i))
}
fn chain(row: &Value) -> CapturedSpellChain {
    CapturedSpellChain {
        spell: int(&row["spell_id"]) as u32,
        previous: int(&row["prev_spell"]) as u32,
        first: int(&row["first_spell"]) as u32,
        rank: int(&row["rank"]) as u32,
        required: int(&row["req_spell"]) as u32,
    }
}
fn learn(row: &Value) -> CapturedLearnSpell {
    CapturedLearnSpell {
        entry: int(&row["entry"]) as u32,
        spell: int(&row["SpellID"]) as u32,
        active: int(&row["Active"]) != 0,
    }
}
fn adapter(
    c: &Compiler<'_>,
    closure: &Value,
    numeric: Vec<LearningSpellNumeric>,
    identity: [u8; 32],
) -> CapturedStarterLearning {
    let source = c.source;
    let selected: BTreeSet<_> = closure["spell_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| int(v) as u32)
        .collect();
    let skills: BTreeSet<_> = closure["skill_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| int(v) as u32)
        .collect();
    let infos: Vec<[u32; 8]> = c.dbcs["SkillRaceClassInfo"]
        .by_id
        .keys()
        .filter_map(|id| c.dbcs["SkillRaceClassInfo"].get(*id))
        .filter(|row| skills.contains(&row.word(1)))
        .map(words)
        .collect();
    let tiers: BTreeSet<_> = infos.iter().map(|row| row[6]).collect();
    let raw_spells: BTreeMap<_, _> = rows(source, "spell_template")
        .iter()
        .map(|row| (int(&row["Id"]) as u32, row))
        .collect();
    let probes = source.get("skill_training_capture");
    let probe_rows: BTreeMap<_, _> = probes
        .into_iter()
        .flat_map(|p| p["rows"].as_array().unwrap())
        .map(|row| (int(&row["Id"]) as u32, row))
        .collect();
    let training = closure["training_probe_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| {
            let id = int(id) as u32;
            (
                id,
                raw_spells
                    .get(&id)
                    .or_else(|| probe_rows.get(&id))
                    .and_then(|row| super::super::extensions::skill_training::training(row)),
            )
        })
        .collect();
    let mut existence: BTreeMap<_, _> = raw_spells.keys().map(|id| (*id, true)).collect();
    for id in closure["training_probe_ids"].as_array().unwrap() {
        let id = int(id) as u32;
        existence.entry(id).or_insert(probe_rows.contains_key(&id));
    }
    CapturedStarterLearning {
        source_identity: identity,
        selected_spell_ids: selected.iter().copied().collect(),
        creation_skills: Rows::Complete(
            rows(source, "playercreateinfo_skills")
                .iter()
                .map(|row| CreationSkill {
                    race_mask: int(&row["raceMask"]) as u32,
                    class_mask: int(&row["classMask"]) as u32,
                    skill: int(&row["skill"]) as u32,
                    step: int(&row["step"]) as u32,
                })
                .collect(),
        ),
        creation_spells: Rows::Complete(
            rows(source, "playercreateinfo_spell")
                .iter()
                .map(|row| CreationSpell {
                    race: int(&row["race"]) as u8,
                    class: int(&row["class"]) as u8,
                    spell: int(&row["Spell"]) as u32,
                })
                .collect(),
        ),
        skill_lines: Rows::Complete(
            skills
                .iter()
                .filter_map(|id| c.dbcs["SkillLine"].get(*id))
                .map(words)
                .collect(),
        ),
        skill_race_class: Rows::Complete(infos),
        skill_tiers: Rows::Complete(
            tiers
                .iter()
                .filter_map(|id| c.dbcs["SkillTiers"].get(*id))
                .map(words)
                .collect(),
        ),
        skill_training: Rows::Complete(training),
        skill_abilities: Rows::Complete(
            closure["ability_candidates"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| std::array::from_fn(|i| int(&row["words"][i]) as u32))
                .collect(),
        ),
        talents: Rows::Complete(
            c.dbcs["Talent"]
                .by_id
                .keys()
                .filter_map(|id| c.dbcs["Talent"].get(*id))
                .filter(|row| (4..9).any(|i| selected.contains(&row.word(i))))
                .map(words)
                .collect(),
        ),
        chains: Rows::Complete(rows(source, "spell_chain").iter().map(chain).collect()),
        learn_spells: Rows::Complete(
            rows(source, "spell_learn_spell")
                .iter()
                .map(learn)
                .collect(),
        ),
        global_chain_probe: Rows::Complete(
            source["learning_capture"]["global_chain_probe"]["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(chain)
                .collect(),
        ),
        global_learn_probe: Rows::Complete(
            source["learning_capture"]["global_learn_spell_probe"]["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(learn)
                .collect(),
        ),
        spell_existence: Rows::Complete(existence.into_iter().collect()),
        numeric: Rows::Complete(numeric),
    }
}

pub(super) fn candidate(
    c: &Compiler<'_>,
    closure: &Value,
    spells: &Value,
    loader_plan: &Value,
) -> Result<(Value, Option<CapturedStarterLearning>)> {
    if c.source.get("spell_numeric_capture").is_none() {
        return Ok((
            json!({"format":"benilla-native-learning-numeric-candidate-v1","status":"blocked",
            "selected_spell_count":closure["spell_ids"].as_array().unwrap().len(),"checked_numeric_count":0,
            "sub_obligations":{"complete_effective_projection":false,"loaded_order":false,"initial_hooks":false,"initial_cast_dispatch":false,"effective_learning":false},
            "refusals":[{"table":"spell_template","key":[],"reason":"player.learning_numeric_complete_rows_required"}]}),
            None,
        ));
    }
    let mut identity = [0; 32];
    for (i, byte) in identity.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&c.source_sha[i * 2..i * 2 + 2], 16)
            .map_err(|_| fail("learning.numeric_source_identity"))?;
    }
    let selected = closure["spell_ids"].as_array().unwrap();
    let indexed: BTreeMap<_, _> = spells
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (int(&r["id"]), r))
        .collect();
    let mapped = COMPILER_CONTRACT["numeric_words"].as_object().unwrap();
    let floats = names(&COMPILER_CONTRACT["numeric_floats"]);
    let mut report_rows = Vec::new();
    let mut numeric = Vec::new();
    let mut refusals = Vec::new();
    if closure["dependency_request"]["source_sha256"] != c.source_sha
        || closure["dependency_request"]["dbc_sha256"]
            != json!(c
                .dbcs
                .iter()
                .map(|(name, table)| (name, &table.sha256))
                .collect::<BTreeMap<_, _>>())
    {
        refusals.push(json!({"table":"spell_template","key":[0],"reason":"player.learning_numeric_input_binding"}));
    }
    if loader_plan["loaded_learning_maps_resolved"] != true {
        refusals.push(json!({"table":"spell_template","key":[0],"reason":"player.learning_numeric_adapter_prerequisites"}));
    }
    for id in selected {
        let id = int(id);
        let capture = numeric_scope(c.source, id);
        let client = c.dbcs.get("Spell").and_then(|t| t.get(id as u32));
        let Some(client) = client.filter(|_| !capture.is_null()) else {
            refusals.push(json!({"table":"spell_template","key":[num(id)],"reason":"player.learning_numeric_complete_rows_required"}));
            continue;
        };
        let sql = &capture["row"];
        let client_words = words::<173>(client);
        let mut effective = client_words;
        for (name, column) in mapped {
            effective[column.as_u64().unwrap() as usize] = if floats.contains(name.as_str()) {
                dbc::binary32(sql[name]["decimal"].as_str().unwrap())?
            } else {
                int(&sql[name]) as u32
            };
        }
        let family = int(&sql["SpellFamilyFlags"]) as u64;
        effective[161] = family as u32;
        effective[162] = (family >> 32) as u32;
        if json!(effective.as_slice()) != indexed[&id]["server_numeric_words"] {
            refusals.push(json!({"table":"spell_template","key":[num(id)],"reason":"player.learning_numeric_projection_conflict"}));
        }
        let mut lookups = Vec::new();
        let mut duration_ms = None;
        for (table, key) in [
            ("SpellCastTimes", "CastingTimeIndex"),
            ("SpellDuration", "DurationIndex"),
            ("SpellRange", "rangeIndex"),
            ("SpellCategory", "Category"),
            ("SpellFocusObject", "RequiresSpellFocus"),
            ("SpellRadius", "EffectRadiusIndex1"),
            ("SpellRadius", "EffectRadiusIndex2"),
            ("SpellRadius", "EffectRadiusIndex3"),
        ] {
            let entry = int(&sql[key]) as u32;
            if entry == 0 && !["SpellCastTimes", "SpellDuration", "SpellRange"].contains(&table) {
                lookups.push(
                    json!({"table":table,"id":entry,"status":"source_zero_no_lookup","words":null}),
                );
                continue;
            }
            let dbc = c.dbcs.get(table);
            let row = dbc.and_then(|t| t.get(entry));
            if dbc.is_none() || row.is_none() && (entry != 0 || table == "SpellRange") {
                refusals.push(json!({"table":"spell_template","key":[num(id)],"reason":"player.learning_numeric_auxiliary_lookup_required"}));
            }
            if table == "SpellDuration" {
                duration_ms = if let Some(row) = row {
                    let raw = row.word(1) as i32;
                    if raw == i32::MIN {
                        refusals.push(json!({"table":"spell_template","key":[num(id)],"reason":"player.learning_numeric_duration_domain"}));
                        Some(raw)
                    } else {
                        Some(if raw == -1 { -1 } else { raw.abs() })
                    }
                } else {
                    dbc.map(|_| 0)
                };
            }
            lookups.push(json!({"table":table,"id":entry,"status":if row.is_some(){"present"}else if dbc.is_some(){"verified_absent"}else{"unknown"},
                "words":row.map(|r|(0..r.len()).map(|i|r.word(i)).collect::<Vec<_>>())}));
        }
        let mut server_only = Map::new();
        for (name, value) in sql.as_object().unwrap() {
            if !mapped.contains_key(name) && name != "SpellFamilyFlags" {
                server_only.insert(
                    name.clone(),
                    if floats.contains(name.as_str()) {
                        json!(dbc::binary32(value["decimal"].as_str().unwrap())?)
                    } else {
                        value.clone()
                    },
                );
            }
        }
        report_rows.push(json!({"id":num(id),"client_words_sha256":digest(&json!(client_words.as_slice())),
            "effective_words_sha256":digest(&json!(effective.as_slice())),"duration_ms":duration_ms,
            "auxiliary_lookups":lookups,"server_only":server_only,"numeric_capture_sha256":capture["coverage_sha256"]}));
        numeric.push(LearningSpellNumeric {
            source_identity: identity,
            id: id as u32,
            client_words,
            effective_words: effective,
            duration_ms,
        });
    }
    let complete = closure["captured_inputs_complete"] == true
        && refusals.is_empty()
        && numeric.len() == selected.len();
    let checked = if complete {
        // Exercise the existing loader adapter without inventing order or hook inputs.
        let capture = adapter(c, closure, numeric, identity);
        capture
            .compile(c.request.race, c.request.class)
            .map_err(|_| fail("learning.numeric_adapter_rejected"))?;
        Some(capture)
    } else {
        None
    };
    let mut outcomes = BTreeMap::from([
        ("present", 0usize),
        ("verified_absent", 0),
        ("source_zero_no_lookup", 0),
        ("unknown", 0),
    ]);
    for row in &report_rows {
        for lookup in row["auxiliary_lookups"].as_array().unwrap() {
            *outcomes
                .get_mut(lookup["status"].as_str().unwrap())
                .unwrap() += 1;
        }
    }
    Ok((
        json!({"format":"benilla-native-learning-numeric-candidate-v1","status":if complete{"checked"}else{"blocked"},
        "source_identity":closure["dependency_request"]["source_sha256"],"dbc_binding_sha256":digest(&closure["dependency_request"]["dbc_sha256"]),
        "seed_independent":true,"effective_learning_proven":false,
        "sub_obligations":{"complete_effective_projection":complete,"loaded_order":false,"initial_hooks":false,"initial_cast_dispatch":false,"effective_learning":false},
        "selected_spell_count":selected.len(),"checked_numeric_count":if complete{report_rows.len()}else{0},
        "auxiliary_lookup_count":outcomes.values().sum::<usize>()-outcomes["source_zero_no_lookup"],"auxiliary_outcomes":outcomes,
        "server_only_field_count":report_rows.iter().map(|r|r["server_only"].as_object().unwrap().len()).sum::<usize>(),
        "rows_sha256":digest(&json!(report_rows)),"refusals":refusals}),
        checked,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_candidate_returns_bound_adapter_inputs_without_hooks() {
        let corpus = decode(
            include_bytes!(
                "../../../../../../scripts/native_starter/production_verifier_cases.json"
            ),
            8 * 1024 * 1024,
        )
        .unwrap();
        let fixture = corpus["fixtures"]
            .as_array()
            .unwrap()
            .iter()
            .find(|fixture| fixture["name"] == "numeric")
            .unwrap();
        let source = &fixture["source"];
        let tables: Tables = fixture["dbc_hex"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, value)| {
                let bytes = value
                    .as_str()
                    .unwrap()
                    .as_bytes()
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                    .collect();
                (name.clone(), dbc::Table::parse(name, bytes).unwrap())
            })
            .collect();
        let sha = digest(source);
        let mut compiler = Compiler {
            source,
            source_sha: &sha,
            dbcs: &tables,
            location: None,
            request: ProductionRequest {
                race: 1,
                class: 8,
                gender: 0,
            },
            issues: Vec::new(),
            dependencies: BTreeSet::new(),
            selected: BTreeMap::new(),
            binding_bytes: 0,
        };
        let closure = learning::discover(&mut compiler).unwrap();
        let loader_plan = learning::plan(source, &tables, &closure);
        let mut spells = compiler.spells().unwrap();
        let (report, capture) = candidate(&compiler, &closure, &spells, &loader_plan).unwrap();
        assert_eq!(report["status"], "checked");
        let capture = capture.unwrap();
        let Rows::Complete(numeric) = &capture.numeric else {
            unreachable!()
        };
        assert_eq!(numeric.len(), 4);
        let fireball = numeric.iter().find(|row| row.id == 133).unwrap();
        assert_ne!(fireball.client_words[167], fireball.effective_words[167]);
        let missing_duration = numeric.iter().find(|row| row.id == 930001).unwrap();
        assert_eq!(missing_duration.effective_words[30], 0);
        assert_eq!(missing_duration.duration_ms, Some(0));
        let compiled = capture.compile(1, 8).unwrap();
        assert!(compiled.bind_spell_rows(numeric, &Rows::Missing).is_err());
        spells[0]["server_numeric_words"][76] = json!(123456);
        let (conflict, capture) = candidate(&compiler, &closure, &spells, &loader_plan).unwrap();
        assert_eq!(conflict["status"], "blocked");
        assert!(capture.is_none());
    }
}
