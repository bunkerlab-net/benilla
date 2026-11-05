// SPDX-License-Identifier: GPL-3.0-or-later
//! Complete schema and loader identities, not gameplay admission.
use super::{check, fail, json, Result};
use serde_json::Value;
use std::{collections::BTreeSet, path::Path, sync::LazyLock};

pub(super) static POLICY: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../../scripts/classic_export/catalog_coverage.json"
    ))
    .expect("reviewed coverage schema metadata")
});

pub(super) fn fields(value: &Value, expected: &[&str], id: &str) -> Result<()> {
    let object = value.as_object().ok_or_else(|| fail(id))?;
    check(
        object.len() == expected.len() && expected.iter().all(|k| object.contains_key(*k)),
        id,
    )
}

pub(super) fn verify_schema(schema: &Value, coverage: &Value) -> Result<Value> {
    verify_with_policy(schema, coverage, &POLICY)
}

pub(super) fn verify_with_policy(
    schema: &Value,
    coverage: &Value,
    policy: &Value,
) -> Result<Value> {
    fields(
        schema,
        &["format", "core_revision", "database_revision", "tables"],
        "catalog.schema",
    )?;
    check(
        schema["format"] == "benilla-classic-catalog-schema-v3"
            && schema["core_revision"] == policy["core_revision"]
            && schema["database_revision"] == policy["database_revision"],
        "catalog.version",
    )?;
    let tables = schema["tables"]
        .as_object()
        .ok_or_else(|| fail("catalog.table"))?;
    let expected = policy["tables"].as_object().expect("policy tables");
    check(
        tables.keys().collect::<BTreeSet<_>>() == expected.keys().collect(),
        "catalog.table",
    )?;
    for (name, table) in tables {
        check(
            json::digest(table) == expected[name]["schema_sha256"],
            "catalog.schema_mismatch",
        )?;
    }
    fields(
        coverage,
        &["policy_sha256", "tables", "complete"],
        "catalog.coverage",
    )?;
    check(
        coverage["policy_sha256"] == json::digest(policy),
        "catalog.coverage",
    )?;
    let ledger = coverage["tables"]
        .as_object()
        .ok_or_else(|| fail("catalog.coverage"))?;
    check(
        ledger.keys().collect::<BTreeSet<_>>() == expected.keys().collect(),
        "catalog.coverage",
    )?;
    for (name, entry) in ledger {
        fields(entry, &["status", "loaders"], "catalog.coverage")?;
        let ids = entry["loaders"]
            .as_array()
            .ok_or_else(|| fail("catalog.loader"))?;
        check(
            ids.iter().all(|id| {
                id.as_str()
                    .is_some_and(|s| policy["loaders"].get(s).is_some())
            }),
            "catalog.loader",
        )?;
        check(
            entry
                == &json!({"status":expected[name]["status"],"loaders":expected[name]["loaders"]}),
            "catalog.coverage",
        )?;
    }
    check(coverage["complete"] == false, "catalog.incomplete")?;
    let columns: usize = tables
        .values()
        .map(|t| t["columns"].as_array().expect("checked schema").len())
        .sum();
    Ok(json!({"tables":tables.len(),"columns":columns,"schema_sha256":json::digest(schema)}))
}

/// Verifies the complete schema and exactly the map-0/map-1 pilot, never F1 binding.
pub fn diagnose_catalog_schema(path: &Path) -> Result<Value> {
    let value = json::decode(&json::read_regular(path, 1_048_576)?, 1_048_576)?;
    fields(
        &value,
        &[
            "format",
            "schema",
            "coverage",
            "rows",
            "source_snapshot",
            "complete",
        ],
        "catalog.pilot",
    )?;
    check(
        value["format"] == "benilla-classic-catalog-pilot-v3"
            && value["source_snapshot"] == "phase0-sequential-diagnostic-not-F1"
            && value["complete"] == false,
        "catalog.incomplete",
    )?;
    let mut result = verify_schema(&value["schema"], &value["coverage"])?;
    let rows = value["rows"]
        .as_array()
        .ok_or_else(|| fail("catalog.pilot"))?;
    check(rows.len() == 2, "catalog.pilot")?;
    let mut keys = Vec::new();
    for row in rows {
        let cells = row.as_array().ok_or_else(|| fail("catalog.pilot"))?;
        check(cells.len() == 2, "catalog.pilot")?;
        for cell in cells {
            let text = cell.as_str().ok_or_else(|| fail("catalog.pilot"))?;
            check(
                text.len() % 2 == 0
                    && text
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "catalog.pilot",
            )?;
        }
        keys.push(cells[0].as_str().expect("checked cell"));
    }
    keys.sort_unstable();
    check(keys == ["30", "31"], "catalog.pilot")?;
    result["pilot_sha256"] = json!(json::digest(rows));
    result["complete"] = json!(false);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn authored() -> (Value, Value, Value) {
        let table = json!({"columns":[{"ordinal":1,"name":"map","type":"uint32","nullable":false,"default":"0","charset":null}],"indexes":[{"columns":["map"],"unique":true}]});
        let policy = json!({"core_revision":"authored","database_revision":"authored","loaders":{"world":{}},"tables":{"world_template":{"schema_sha256":json::digest(&table),"status":"consumed","loaders":["world"]}}});
        let schema = json!({"format":"benilla-classic-catalog-schema-v3","core_revision":"authored","database_revision":"authored","tables":{"world_template":table}});
        let coverage = json!({"policy_sha256":json::digest(&policy),"complete":false,"tables":{"world_template":{"status":"consumed","loaders":["world"]}}});
        (schema, coverage, policy)
    }
    #[test]
    fn catalog_authored_schema_refusals() {
        let (schema, coverage, policy) = authored();
        assert!(verify_with_policy(&schema, &coverage, &policy).is_ok());
        for (field, forged) in [
            ("ordinal", json!(2)),
            ("type", json!("int32")),
            ("nullable", json!(true)),
            ("default", json!("1")),
            ("charset", json!("latin1")),
        ] {
            let mut bad = schema.clone();
            bad["tables"]["world_template"]["columns"][0][field] = forged;
            assert_eq!(
                verify_with_policy(&bad, &coverage, &policy).unwrap_err().id,
                "catalog.schema_mismatch"
            );
        }
        for name in ["", "unknown"] {
            let mut bad = schema.clone();
            let table = bad["tables"]
                .as_object_mut()
                .unwrap()
                .remove("world_template")
                .unwrap();
            bad["tables"][name] = table;
            assert_eq!(
                verify_with_policy(&bad, &coverage, &policy).unwrap_err().id,
                "catalog.table"
            );
        }
        let mut bad = coverage.clone();
        bad["tables"]["world_template"]["loaders"] = json!(["unknown"]);
        assert_eq!(
            verify_with_policy(&schema, &bad, &policy).unwrap_err().id,
            "catalog.loader"
        );
        let mut bad = coverage.clone();
        bad["complete"] = json!(true);
        assert_eq!(
            verify_with_policy(&schema, &bad, &policy).unwrap_err().id,
            "catalog.incomplete"
        );
    }
    #[test]
    fn catalog_policy_is_exhaustive_and_non_admitting() {
        assert_eq!(POLICY["tables"].as_object().unwrap().len(), 194);
        assert_eq!(POLICY["loaders"].as_object().unwrap().len(), 182);
        let tables = POLICY["tables"].as_object().unwrap();
        assert_eq!(
            tables
                .values()
                .filter(|t| t["status"] == "consumed")
                .count(),
            173
        );
        assert_eq!(
            tables
                .values()
                .filter(|t| t["status"] == "archive-only")
                .count(),
            21
        );
        for table in tables.values().filter(|t| t["status"] == "archive-only") {
            assert_eq!(table["loaders"], json!([]));
            assert!(table["evidence"]["search"]
                .as_str()
                .is_some_and(|s| !s.is_empty()));
            assert!(table["evidence"]["result"]
                .as_str()
                .is_some_and(|s| !s.is_empty()));
        }
        assert_eq!(POLICY["review"], "pending-owner-content-compiler-review");
    }
    #[test]
    fn catalog_pilot_private_parity() {
        let Some(path) = std::env::var_os("BENILLA_CATALOG_PILOT") else {
            return;
        };
        let report = diagnose_catalog_schema(Path::new(&path)).unwrap();
        assert_eq!(report["tables"], 194);
        assert_eq!(report["columns"], 2287);
        println!("{}", json::canonical(&report).escape_ascii());
    }
}
