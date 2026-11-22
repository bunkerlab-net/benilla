// SPDX-License-Identifier: GPL-3.0-or-later
// Independent reader of classic_export/reputation.py's bounded lookup contract.
use super::*;
pub(in super::super) const RATE: &str = "reputation_reward_rate";
pub(in super::super) const SPILL: &str = "reputation_spillover_template";
pub(in super::super) const KILL: &str = "creature_onkill_reputation";

fn columns(table: &str) -> Vec<String> {
    match table {
        RATE => ["faction", "quest_rate", "creature_rate", "spell_rate"]
            .map(String::from)
            .to_vec(),
        SPILL => std::iter::once("faction".into())
            .chain((1..=4).flat_map(|i| {
                [
                    format!("faction{i}"),
                    format!("rate_{i}"),
                    format!("rank_{i}"),
                ]
            }))
            .collect(),
        KILL => [
            "creature_id",
            "RewOnKillRepFaction1",
            "RewOnKillRepFaction2",
            "IsTeamAward1",
            "MaxStanding1",
            "RewOnKillRepValue1",
            "IsTeamAward2",
            "MaxStanding2",
            "RewOnKillRepValue2",
            "TeamDependent",
        ]
        .map(String::from)
        .to_vec(),
        _ => unreachable!(),
    }
}
fn validate_ids(value: &Value) -> Result<Vec<i128>> {
    let entries = array(value, "reputation.selector_limit")?;
    check(entries.len() <= 4096, "reputation.selector_limit")?;
    let ids = entries
        .iter()
        .map(|v| integer(v, 1, 0xffffffff, "reputation.selector"))
        .collect::<Result<Vec<_>>>()?;
    check(
        ids.windows(2).all(|w| w[0] < w[1]),
        "reputation.selector_order",
    )?;
    Ok(ids)
}
pub(super) fn validate(b: &Value) -> Result<()> {
    let value = &b["reputation_policy_capture"];
    object(value, "reputation.extension")?;
    let mut schema = Map::new();
    for table in [RATE, SPILL, KILL] {
        let names = columns(table);
        let columns: Map<_, _> = names
            .iter()
            .map(|name| {
                let (family, unsigned) = if name.ends_with("_rate") || name.starts_with("rate_") {
                    ("float", false)
                } else if table == RATE || name == "creature_id" {
                    ("mediumint", true)
                } else if table == SPILL {
                    (
                        if name.starts_with("rank_") {
                            "tinyint"
                        } else {
                            "smallint"
                        },
                        true,
                    )
                } else if name.starts_with("RewOnKillRepFaction") {
                    ("smallint", false)
                } else if name.starts_with("RewOnKillRepValue") {
                    ("mediumint", false)
                } else {
                    ("tinyint", name == "TeamDependent")
                };
                (name.to_lowercase(), json!([family, unsigned, false]))
            })
            .collect();
        schema.insert(
            table.into(),
            json!({"columns":columns,"primary_key":[names[0].to_lowercase()]}),
        );
    }
    check(
        value["policy_schema"] == Value::Object(schema),
        "reputation.schema",
    )?;
    let factions = validate_ids(&value["faction_ids"])?;
    let sha = string(&value["faction_sha256"], "reputation.faction_hash")?;
    check(
        sha.len() == 64
            && sha
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
        "reputation.faction_hash",
    )?;
    let creatures: Vec<_> = ids(b, "creature_template", "Entry").into_iter().collect();
    let lookups = &value["lookups"];
    fields(lookups, &[RATE, SPILL, KILL], "reputation.lookup_tables")?;
    for table in [RATE, SPILL, KILL] {
        let selected = if table == KILL { &creatures } else { &factions };
        let records = array(&lookups[table], "reputation.lookup_coverage")?;
        check(
            records.len() == selected.len(),
            "reputation.lookup_coverage",
        )?;
        let names = columns(table);
        for (record, id) in records.iter().zip(selected) {
            fields(record, &["id", "row"], "reputation.lookup_selection")?;
            check(
                record["id"].as_i64().map(i128::from) == Some(*id),
                "reputation.lookup_selection",
            )?;
            let row = &record["row"];
            if row.is_null() {
                continue;
            }
            fields(
                row,
                &names.iter().map(String::as_str).collect::<Vec<_>>(),
                "reputation.row_fields",
            )?;
            for name in &names {
                if name.ends_with("_rate") || name.starts_with("rate_") {
                    fields(&row[name], &["decimal"], "reputation.decimal")?;
                    let raw = string(&row[name]["decimal"], "row.decimal_type")?;
                    let kind = SqlType {
                        family: "float".into(),
                        precision: None,
                        scale: None,
                        unsigned: false,
                    };
                    check(
                        normalize_decimal(raw, &kind)? == raw,
                        "reputation.noncanonical_row",
                    )?;
                } else {
                    let (bits, unsigned) = if table == RATE || name == "creature_id" {
                        (24, true)
                    } else if table == SPILL {
                        (if name.starts_with("rank_") { 8 } else { 16 }, true)
                    } else if name.starts_with("RewOnKillRepFaction") {
                        (16, false)
                    } else if name.starts_with("RewOnKillRepValue") {
                        (24, false)
                    } else {
                        (8, name == "TeamDependent")
                    };
                    integer(
                        &row[name],
                        if unsigned { 0 } else { -(1i128 << (bits - 1)) },
                        (1i128 << if unsigned { bits } else { bits - 1 }) - 1,
                        "row.sql_integer_range",
                    )?;
                }
            }
            check(int(&row[&names[0]]) == *id, "reputation.row_selection")?;
        }
    }
    coverage(
        b,
        "reputation_policy_capture",
        json!({"format":"benilla-classic-reputation-policy-v1", "faction_ids":factions.iter().map(|id|*id as u32).collect::<Vec<_>>(), "faction_sha256":sha, "creature_ids":creatures.iter().map(|id|*id as u32).collect::<Vec<_>>(), "lookups":lookups,"policy_schema":value["policy_schema"],
        "binding":{"selection_sha256":digest(&b["selection"]), "schema_sha256":b["schema_sha256"], "table_sha256":digest(&b["table_sha256"]), "db_version_sha256":digest(&b["db_version"]), "consistency":b["consistency"], "lookups_sha256":digest(lookups)}}),
        "reputation.coverage_mismatch",
    )
}
