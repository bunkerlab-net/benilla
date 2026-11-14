// SPDX-License-Identifier: GPL-3.0-or-later
//! Independent canonical schedule reader. Source validation precedes this adapter.
use super::*;
use benilla_sim::canonical_order::{CONTRACT, POLICY};
static ORDER: LazyLock<Value> =
    LazyLock::new(|| serde_json::from_str(CONTRACT).expect("canonical order contract"));
static POLICY_HASH: LazyLock<String> = LazyLock::new(|| hash_bytes(CONTRACT.as_bytes()));

pub(super) fn ordered<'a>(table: &str, rows: &'a [Value]) -> Result<Vec<&'a Value>> {
    let keys = ORDER["keys"][table]
        .as_array()
        .ok_or_else(|| fail("canonical_order.table"))?;
    let mut keyed = Vec::with_capacity(rows.len());
    for row in rows {
        let mut key = [0; 15];
        for (index, field) in keys.iter().enumerate() {
            let value = row
                .get(field.as_str().unwrap())
                .ok_or_else(|| fail("canonical_order.missing_key"))?;
            check(
                value.as_i64().is_some() || value.as_u64().is_some(),
                "canonical_order.key_domain",
            )?;
            key[index] = int(value);
        }
        keyed.push((key, canonical(row), row));
    }
    keyed.sort_unstable_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    Ok(keyed.into_iter().map(|(_, _, row)| row).collect())
}
fn policy_hash() -> &'static str {
    POLICY_HASH.as_str()
}
pub(super) fn collection<'a>(source: &'a Value, table: &str) -> Vec<&'a Value> {
    ordered(table, rows(source, table)).expect("validated canonical source keys")
}
pub(super) fn learning(source: &Value, resolved: bool) -> Value {
    if !resolved {
        return json!({"policy":POLICY,"policy_sha256":policy_hash(),"status":"blocked","reason":"canonical_order.learning_inputs_incomplete"});
    }
    let tables: BTreeMap<_, _> = [
        "playercreateinfo_skills",
        "playercreateinfo_spell",
        "spell_learn_spell",
    ]
    .into_iter()
    .map(|name| (name, collection(source, name)))
    .collect();
    let tables = json!(tables);
    json!({"policy":POLICY,"policy_sha256":policy_hash(),"status":"complete_inputs","ordered_rows_sha256":digest(&tables),"collections":tables,"first_entry":"requires_actual_normalized_skill_set","source_sha256":digest(source)})
}
pub(super) fn relationships(source: &Value) -> Value {
    if source.get("quest_giver_presentation_capture").is_none() {
        return json!({"policy":POLICY,"policy_sha256":policy_hash(),"status":"blocked","reason":"canonical_order.relation_inputs_incomplete"});
    }
    let tables: BTreeMap<_, _> = ["creature_questrelation", "creature_involvedrelation"]
        .into_iter()
        .map(|name| (name, collection(source, name)))
        .collect();
    let tables = json!(tables);
    let scripts = source
        .get("quest_mechanics_capture")
        .map(|m| {
            json!(ordered(
                "dbscripts_on_quest_end",
                m["rows"]["dbscripts_on_quest_end"].as_array().unwrap()
            )
            .expect("validated script keys"))
        })
        .unwrap_or(Value::Null);
    json!({"policy":POLICY,"policy_sha256":policy_hash(),"status":"complete_inputs","collections":tables,"ordered_rows_sha256":digest(&tables),"scripts_sha256":if scripts.is_null(){Value::Null}else{json!(digest(&scripts))},"scripts":scripts,"source_sha256":digest(source)})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_keys_ignore_source_permutation_and_missing_key_refuses() {
        let rows = vec![
            json!({"race":1,"class":8,"Spell":100}),
            json!({"race":1,"class":8,"Spell":20}),
            json!({"race":1,"class":8,"Spell":20}),
        ];
        let expected = vec![&rows[1], &rows[2], &rows[0]];
        assert_eq!(ordered("playercreateinfo_spell", &rows).unwrap(), expected);
        let mut reversed = rows.clone();
        reversed.reverse();
        assert_eq!(
            ordered("playercreateinfo_spell", &reversed).unwrap(),
            expected
        );
        let mut missing = rows;
        missing[0].as_object_mut().unwrap().remove("Spell");
        assert_eq!(
            ordered("playercreateinfo_spell", &missing).unwrap_err().id,
            "canonical_order.missing_key"
        );
    }
    #[test]
    fn unknown_coverage_does_not_discharge_order() {
        assert_eq!(learning(&json!({}), false)["status"], "blocked");
        assert_eq!(relationships(&json!({}))["status"], "blocked");
    }
}
