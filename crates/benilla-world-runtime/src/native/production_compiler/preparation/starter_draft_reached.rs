// SPDX-License-Identifier: GPL-3.0-or-later
//! Complete global capture projected at the actual conditional learning trace.
use super::*;
use crate::native::{
    InitialCastDependencies, NativeError, NativeInitialCastSourceInput, SpellAreaSourceRow,
    SpellAreaSourceRows, SpellGroupSource, SpellHookSources, SpellRankSource, SpellSourceRows,
};

pub(super) fn bind(
    c: &Compiler<'_>,
    candidate: &init::CapturedStarterLearning,
    draft: &StarterDraft,
) -> std::result::Result<
    (
        init::StarterInput,
        init::PreparedStarter,
        NativeInitialCastSourceInput,
    ),
    NativeError,
> {
    if draft.context_identity != super::context_identity(c) {
        return Err(NativeError::AuthorityBinding);
    }
    let capture = &c.source["global_hook_capture"];
    let contract = &super::super::super::global_hooks::CONTRACT;
    if capture.is_null()
        || capture["registration_manifest_sha256"] != digest(&contract["manifest"])
        || capture["configuration"] != contract["manifest"]["configuration"]
    {
        return Err(NativeError::AuthorityUnavailable(
            "reached registration manifest",
        ));
    }
    let raw = &capture["rows"];
    let reached: BTreeSet<_> = draft
        .projection
        .reached_dependencies()
        .iter()
        .map(|d| i128::from(d.spell_id))
        .collect();
    let start = &rows(c.source, "playercreateinfo")[0];
    let selection = &c.source["selection"];
    if start["map"] != selection["map_id"]
        || ["position_x", "position_y", "position_z"]
            .iter()
            .enumerate()
            .any(|(i, key)| {
                let value = start[*key]["decimal"]
                    .as_str()
                    .unwrap()
                    .parse::<f32>()
                    .unwrap();
                let milli = f64::from(value) * 1000.0;
                milli < int(&selection["min_milliunits"][i]) as f64
                    || milli > int(&selection["max_milliunits"][i]) as f64
            })
    {
        return Err(NativeError::UnsupportedMechanic(
            "first entry outside captured selection",
        ));
    }
    for row in draft
        .input
        .spells
        .iter()
        .filter(|r| reached.contains(&i128::from(r.id)))
    {
        if row.words[24] != 0
            || (0..3).any(|i| {
                row.words[103 + i] != 0 && matches!(row.words[91 + i], 107 | 108 | 109 | 112)
            })
        {
            return Err(NativeError::UnsupportedMechanic(
                "reached intrinsic modifier hook",
            ));
        }
    }
    let domain = super::super::super::global_hooks::domain(
        reached.iter().copied(),
        &super::super::super::global_hooks::rank_edges(raw, c.dbcs),
    );
    let queried = |table: &str, key: &str| -> Vec<u32> {
        raw[table]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| domain.contains(&int(&r[key])))
            .map(|r| int(&r[key]) as u32)
            .collect()
    };
    let mut scripts = vec![];
    let mut aura_scripts = BTreeMap::<u32, Vec<String>>::new();
    for row in raw["spell_scripts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| reached.contains(&int(&r["Id"])))
    {
        let name = row["ScriptName"].as_str().unwrap();
        let registration = &contract["manifest"]["registrations"][name];
        if registration.is_null() {
            return Err(NativeError::AuthorityUnavailable(
                "reached script registration",
            ));
        }
        match registration["kind"].as_str() {
            Some("spell") => scripts.push(name.to_owned()),
            Some("aura") => aura_scripts
                .entry(int(&row["Id"]) as u32)
                .or_default()
                .push(name.to_owned()),
            _ => {
                return Err(NativeError::AuthorityUnavailable(
                    "reached script registration kind",
                ))
            }
        }
    }
    // The complete raw area table is retained, including unrelated rows and
    // condition-first predicates. Missing location is not an empty area result.
    let areas = raw["spell_area"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| SpellAreaSourceRow {
            spell: int(&r["spell"]) as u32,
            area: int(&r["area"]) as u32,
            quest_start: int(&r["quest_start"]) as u32,
            quest_start_active: int(&r["quest_start_active"]) as u32,
            quest_end: int(&r["quest_end"]) as u32,
            condition_id: int(&r["condition_id"]) as u32,
            aura_spell: int(&r["aura_spell"]) as i32,
            racemask: int(&r["racemask"]) as u32,
            gender: int(&r["gender"]) as u32,
            autocast: int(&r["autocast"]) as u32,
        })
        .collect();
    let ranks: Vec<_> = raw["spell_chain"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| domain.contains(&int(&r["spell_id"])))
        .map(|r| SpellRankSource {
            spell_id: int(&r["spell_id"]) as u32,
            previous_spell: int(&r["prev_spell"]) as u32,
            first_spell: int(&r["first_spell"]) as u32,
            rank: int(&r["rank"]) as u32,
            required_spell: int(&r["req_spell"]) as u32,
        })
        .collect();
    let group_ids: BTreeSet<_> = raw["spell_group_spell"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| domain.contains(&int(&r["SpellId"])))
        .map(|r| int(&r["Id"]))
        .collect();
    let groups: Vec<_> = raw["spell_group"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| group_ids.contains(&int(&r["Id"])))
        .map(|r| SpellGroupSource {
            id: int(&r["Id"]) as u32,
            rule: int(&r["Rule"]) as u32,
            name: r["Name"].as_str().unwrap().to_owned(),
            spell_ids: raw["spell_group_spell"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|m| m["Id"] == r["Id"])
                .map(|m| int(&m["SpellId"]) as u32)
                .collect(),
        })
        .collect();
    let dependencies: Vec<_> = draft
        .projection
        .reached_dependencies()
        .iter()
        .map(|d| InitialCastDependencies {
            spell_id: d.spell_id,
            cast_time: d.cast_time.clone(),
            duration: d.duration.clone(),
            aura_script_names: SpellSourceRows::Complete(
                aura_scripts.remove(&d.spell_id).unwrap_or_default(),
            ),
            server_side: d.server_side,
            server_attributes: d.server_attributes,
            bonus_coefficients: d.bonus_coefficients,
            attack_power_coefficients: d.attack_power_coefficients,
        })
        .collect();
    let mut scope: BTreeSet<_> = reached.iter().map(|id| *id as u32).collect();
    scope.extend(
        draft
            .projection
            .projected_creation()
            .abilities
            .values()
            .filter(|a| {
                a.active
                    && matches!(
                        a.availability,
                        init::AbilityAvailability::Available
                            | init::AbilityAvailability::Unavailable(_)
                    )
            })
            .map(|a| a.spell_id),
    );
    for rank in &ranks {
        scope.extend(
            [
                rank.spell_id,
                rank.previous_spell,
                rank.first_spell,
                rank.required_spell,
            ]
            .into_iter()
            .filter(|id| *id != 0),
        );
    }
    let stores: BTreeMap<_, _> = c
        .dbcs
        .iter()
        .map(|(name, table)| (name, &table.sha256))
        .collect();
    let mut source = NativeInitialCastSourceInput {
        source_identity: [0; 32],
        entry_location: None,
        store_identity: crate::digest(b"benilla-reached-starter-dbc-stores-v1\0", &stores),
        dependencies: SpellSourceRows::Complete(dependencies),
        queried_spell_ids: SpellSourceRows::Complete(scope.into_iter().collect()),
        hooks: SpellHookSources {
            spell_area: SpellAreaSourceRows::CompleteTable(areas),
            spell_script_names: SpellSourceRows::Complete(scripts),
            conditions: SpellSourceRows::Complete(vec![]),
            spell_proc_event: SpellSourceRows::Complete(queried("spell_proc_event", "entry")),
            spell_affect: SpellSourceRows::Complete(queried("spell_affect", "entry")),
            spell_bonus_data: SpellSourceRows::Complete(vec![]),
            spell_threat: SpellSourceRows::Complete(queried("spell_threat", "entry")),
            spell_elixir: SpellSourceRows::Complete(queried("spell_elixir", "entry")),
            spell_group_membership: SpellSourceRows::Complete(groups),
            spell_rank_chain: SpellSourceRows::Complete(ranks),
        },
    };
    let learning = candidate.compile(c.request.race, c.request.class)?;
    let init::Rows::Complete(numeric) = &candidate.numeric else {
        return Err(NativeError::AuthorityUnavailable("reached numeric rows"));
    };
    let spell_rows =
        source.bind_reached_learning_spell_rows(&learning, numeric, &draft.projection)?;
    let input = learning.populate(draft.input.clone(), spell_rows)?;
    source.source_identity = input.source_identity();
    let dependencies = init::StarterInitialDependencies {
        source_identity: input.source_identity(),
        store_identity: source.store_identity,
        hook_scope_identity: source.hook_scope_identity(),
        queried_spell_ids: draft
            .projection
            .reached_dependencies()
            .iter()
            .map(|d| d.spell_id)
            .collect(),
        rows: init::Rows::Complete(
            draft
                .projection
                .reached_dependencies()
                .iter()
                .map(|d| {
                    let mut d = d.clone();
                    d.source_hash = input
                        .spells
                        .iter()
                        .find(|r| r.id == d.spell_id)
                        .unwrap()
                        .source_hash();
                    d
                })
                .collect(),
        ),
    };
    let prepared =
        init::prepare_starter(&input, &dependencies, &init::StarterCastOrder::canonical())?;
    Ok((input, prepared, source))
}
