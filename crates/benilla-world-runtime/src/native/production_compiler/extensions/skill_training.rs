// SPDX-License-Identifier: GPL-3.0-or-later
// classic_export/skill_training.py: complete selected SQL effect probes, not client inference.
use super::*;
pub(in super::super) fn training(row: &Value) -> Option<(u16, u16)> {
    for slot in 1..=3 {
        let skill = int(&row[format!("EffectMiscValue{slot}")]);
        let step = int(&row[format!("EffectBasePoints{slot}")]) + 1;
        if int(&row[format!("Effect{slot}")]) == 118 && skill != 0 && step != 0 {
            return Some((skill as u16, step as u16));
        }
    }
    None
}
pub(super) fn validate(bundle: &Value) -> Result<()> {
    let value = &bundle["skill_training_capture"];
    object(value, "skill_training.extension")?;
    let hashes = object(&value["dbc_sha256"], "skill_training.dbc_hashes")?;
    check(
        !hashes.is_empty() && hashes.len() <= 32,
        "skill_training.dbc_hashes",
    )?;
    for (name, hash) in hashes {
        check(
            !name.is_empty() && name.len() <= 64 && name.bytes().all(|c| c.is_ascii_alphanumeric()),
            "skill_training.dbc_name",
        )?;
        valid_hash(hash, "skill_training.dbc_hash")?;
    }
    let validate_ids = |v: &Value| -> Result<BTreeSet<i128>> {
        let array = array(v, "skill_training.probe_limit")?;
        check(array.len() <= 4096, "skill_training.probe_limit")?;
        let mut set = BTreeSet::new();
        let mut previous = 0;
        for value in array {
            let id = integer(value, 1, 0xffffffff, "skill_training.spell_id")?;
            check(id > previous, "skill_training.probe_order")?;
            previous = id;
            set.insert(id);
        }
        Ok(set)
    };
    let selected = validate_ids(&value["spell_ids"])?;
    let rs = array(&value["rows"], "skill_training.row_limit")?;
    check(rs.len() <= 4096, "skill_training.row_limit")?;
    let fields: Vec<String> = ["Id".into()]
        .into_iter()
        .chain((1..=3).flat_map(|slot| {
            ["Effect", "EffectMiscValue", "EffectBasePoints"]
                .into_iter()
                .map(move |name| format!("{name}{slot}"))
        }))
        .collect();
    let mut previous = 0;
    for row in rs {
        let o = object(row, "skill_training.row_fields")?;
        check(
            o.len() == fields.len() && fields.iter().all(|name| o.contains_key(name)),
            "skill_training.row_fields",
        )?;
        for name in &fields {
            let signed =
                name.starts_with("EffectMiscValue") || name.starts_with("EffectBasePoints");
            integer(
                &row[name],
                if signed { i32::MIN as i128 } else { 0 },
                if signed {
                    i32::MAX as i128
                } else {
                    u32::MAX as i128
                },
                "skill_training.field_domain",
            )?;
        }
        let id = integer(&row["Id"], 1, 0xffffffff, "skill_training.spell_id")?;
        check(id > previous, "skill_training.probe_order")?;
        previous = id;
        check(selected.contains(&id), "skill_training.unselected_row")?;
    }
    let expected = json!({"format":"benilla-classic-skill-training-selection-v1","spell_ids":value["spell_ids"],"rows":value["rows"],"dbc_sha256":hashes,"binding":{"selection_sha256":digest(&bundle["selection"]),"schema_sha256":bundle["schema_sha256"],"table_sha256":digest(&bundle["table_sha256"]),"db_version_sha256":digest(&bundle["db_version"]),"consistency":bundle["consistency"],"rows_sha256":digest(&value["rows"]),"selectors_sha256":digest(&value["spell_ids"])}});
    coverage(
        bundle,
        "skill_training_capture",
        expected,
        "skill_training.coverage_mismatch",
    )?;
    let known: BTreeMap<_, _> = rows(bundle, "spell_template")
        .iter()
        .map(|row| (int(&row["Id"]), row))
        .collect();
    for row in rs {
        if let Some(known) = known.get(&int(&row["Id"])) {
            check(
                fields.iter().all(|name| known[name] == row[name]),
                "skill_training.effective_row_mismatch",
            )?;
        }
    }
    Ok(())
}
