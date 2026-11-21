// SPDX-License-Identifier: GPL-3.0-or-later
// Independent validation of classic_export/addon_loader.py capture evidence.
use super::*;

pub(super) fn validate(b: &Value) -> Result<()> {
    require_tables(
        b,
        &["creature_addon", "creature_template_addon"],
        "addon_loader.tables_required",
    )?;
    let mut selected = BTreeSet::new();
    for table in ["creature_addon", "creature_template_addon"] {
        for row in rows(b, table) {
            for part in row["auras"].as_str().unwrap_or("").split(' ') {
                if !part.is_empty() && part.len() <= 10 && part.bytes().all(|v| v.is_ascii_digit())
                {
                    if let Ok(id) = part.parse::<u32>() {
                        if id != 0 {
                            selected.insert(i128::from(id));
                        }
                    }
                }
            }
        }
    }
    check(selected.len() <= 512, "addon_loader.selector_limit")?;
    let value = &b["addon_loader_capture"];
    let rs = array(&value["rows"], "addon_loader.rows")?;
    check(rs.len() <= 512, "addon_loader.rows")?;
    let names = [
        "Id",
        "CastingTimeIndex",
        "Effect1",
        "Effect2",
        "Effect3",
        "EffectImplicitTargetA1",
        "EffectImplicitTargetA2",
        "EffectImplicitTargetA3",
        "EffectImplicitTargetB1",
        "EffectImplicitTargetB2",
        "EffectImplicitTargetB3",
    ];
    let mut previous = 0;
    for row in rs {
        fields(row, &names, "addon_loader.row_fields")?;
        for name in names {
            integer(
                &row[name],
                if name == "Id" { 1 } else { 0 },
                0xffffffff,
                "addon_loader.numeric_domain",
            )?;
        }
        let id = int(&row["Id"]);
        check(
            id > previous && selected.contains(&id),
            "addon_loader.row_selection",
        )?;
        previous = id;
        if let Some(existing) = rows(b, "spell_template")
            .iter()
            .find(|s| int(&s["Id"]) == id)
        {
            check(
                names.iter().all(|f| existing[*f] == row[*f]),
                "addon_loader.conflicting_spell",
            )?;
        }
    }
    let hashes: Map<_, _> = ["creature_addon", "creature_template_addon"]
        .into_iter()
        .map(|t| (t.into(), Value::String(digest(&b["tables"][t]))))
        .collect();
    coverage(
        b,
        "addon_loader_capture",
        json!({"format":"benilla-classic-addon-loader-lookups-v1",
        "selected_ids": numbers(selected), "rows":rs, "rows_sha256":digest(&value["rows"]),
        "strict_log_filter":null,"snapshot":{"consistency":b["consistency"],
        "selection_sha256":digest(&b["selection"]),"schema_sha256":b["schema_sha256"],
        "db_version_sha256":digest(&b["db_version"]),"addon_tables_sha256":hashes}}),
        "addon_loader.coverage_mismatch",
    )
}
