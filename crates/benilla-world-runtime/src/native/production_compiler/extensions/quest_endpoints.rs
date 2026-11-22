// SPDX-License-Identifier: GPL-3.0-or-later
//! Independent mirror of classic_export.quest_endpoints.
use super::*;

const TABLES: [&str; 2] = ["creature_questrelation", "creature_involvedrelation"];
fn inside(s: &Selection, r: &Value) -> bool {
    int(&r["map"]) == s.map
        && ["position_x", "position_y", "position_z"]
            .iter()
            .enumerate()
            .all(|(i, f)| super::super::source::inside(&r[*f], s.min[i], s.max[i]))
}
pub(super) fn validate(b: &Value, s: &Selection, p: &Profiles) -> Result<()> {
    let value = &b["quest_endpoint_capture"];
    let raw = &value["rows"];
    fields(
        raw,
        &[TABLES[0], TABLES[1], "creature"],
        "quest_endpoints.tables",
    )?;
    for table in [TABLES[0], TABLES[1], "creature"] {
        validate_rows(table, &raw[table], &p[table], 4096)?;
    }
    for table in TABLES {
        check(
            raw[table] == b["tables"][table],
            "quest_endpoints.relation_coverage_mismatch",
        )?;
    }
    let entries: BTreeSet<_> = TABLES
        .iter()
        .flat_map(|t| raw[*t].as_array().unwrap())
        .map(|r| int(&r["id"]))
        .collect();
    let spawns = raw["creature"].as_array().unwrap();
    check(
        spawns.iter().all(|r| entries.contains(&int(&r["id"]))),
        "quest_endpoints.unselected_spawn",
    )?;
    let selected = spawns.iter().filter(|r| inside(s, r));
    let expected_selected = rows(b, "creature")
        .iter()
        .filter(|r| entries.contains(&int(&r["id"])));
    check(
        selected.eq(expected_selected),
        "quest_endpoints.selected_spawn_coverage_mismatch",
    )?;
    let mut endpoints = Vec::new();
    for table in TABLES {
        for relation in raw[table].as_array().unwrap() {
            let mut matching = spawns
                .iter()
                .filter(|r| r["id"] == relation["id"])
                .peekable();
            let placement = if matching.clone().any(|r| inside(s, r)) {
                "inside_selection"
            } else if matching.peek().is_none() {
                "unspawned"
            } else {
                "outside_selection"
            };
            endpoints.push(json!({"table":table,"id":relation["id"],"quest":relation["quest"],"placement":placement,"spawn_guids":matching.map(|r| &r["guid"]).collect::<Vec<_>>()}));
        }
    }
    let mut snapshot = identity_snapshot(b);
    snapshot["tables_sha256"] = json!(digest(&b["table_sha256"]));
    let expected = json!({"format":"benilla-classic-quest-endpoints-v1","status":"complete_raw_projection","selectors":{"quests":numbers(ids(b,"quest_template","entry")),"entries":numbers(entries)},"rows":raw,"rows_sha256":digest(raw),"endpoints":endpoints,"snapshot":snapshot});
    check(*value == expected, "quest_endpoints.binding_mismatch")
}
pub(in super::super) fn proved(b: &Value, table: &str, quest: i128) -> bool {
    let Some(endpoints) = b["quest_endpoint_capture"]["endpoints"].as_array() else {
        return false;
    };
    let mut matching = endpoints
        .iter()
        .filter(|r| r["table"] == table && int(&r["quest"]) == quest)
        .peekable();
    matching.peek().is_some() && matching.all(|r| r["placement"] == "outside_selection")
}
pub(in super::super) fn discharges(b: &Value, issue: &Value) -> bool {
    issue["reason"] == "quest.relation_missing_or_outside_selection"
        && issue["table"].as_str().is_some_and(|t| {
            TABLES.contains(&t)
                && issue["key"]
                    .as_str()
                    .and_then(|k| k.parse::<u32>().ok())
                    .is_some_and(|q| proved(b, t, i128::from(q)))
        })
}
