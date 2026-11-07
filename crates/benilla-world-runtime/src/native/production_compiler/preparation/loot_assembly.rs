// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 LootMgr.cpp:125-185,2619-2669.
//! Checked raw composition. Host audits and NativeCapability remain separate.
use super::*;
use benilla_sim::{inventory, loot};
use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct CheckedLootDraft {
    items: inventory::captured::CheckedItems,
    body: Vec<loot::Capture>,
    references: Vec<loot::Capture>,
    client_inputs: BTreeMap<u32, Value>,
}
impl CheckedLootDraft {
    pub fn items(&self) -> &inventory::captured::CheckedItems {
        &self.items
    }
    pub fn body(&self) -> &[loot::Capture] {
        &self.body
    }
    pub fn references(&self) -> &[loot::Capture] {
        &self.references
    }
    /// Complete source query operands, not renderability or SpellTemplate query proof.
    pub fn client_inputs(&self) -> &BTreeMap<u32, Value> {
        &self.client_inputs
    }
}

pub(super) fn plan(source: &Value, ordered: &Map<String, Value>, raw_refusals: &[Value]) -> Value {
    let mut refusals = raw_refusals.to_vec();
    let mut inputs = Vec::new();
    let mut metadata = Vec::new();
    let ids = source["loot_capture"]["leaf_item_ids"].as_array().unwrap();
    for id in ids {
        let entry = int(id);
        let Some(item) = rows(source, "item_template")
            .iter()
            .find(|r| r["entry"] == *id)
        else {
            refusals.push(json!({"table":"item_template","key":entry.to_string(),"reason":"loot.item_template_missing"}));
            continue;
        };
        let checked = super::plans::item::item_plan(item);
        for refusal in checked["refusals"].as_array().unwrap() {
            refusals.push(json!({"table":"item_template","key":entry.to_string(),"reason":format!("loot.{}", refusal["reason"].as_str().unwrap())}));
        }
        if !checked["captured_input"].is_null() {
            inputs.push(checked["captured_input"].clone());
            metadata.push(json!({"entry":id,"item_template_sha256":checked["item_template_sha256"],"row":item}));
        }
    }
    json!({"status":if refusals.is_empty(){"checked_projection"}else{"blocked"},
        "runtime_admitted":false,"creatures":source["loot_capture"]["creatures"],
        "body_row_count":ordered.get("creature_loot_template").and_then(Value::as_array).map_or(0,Vec::len),
        "reference_row_count":ordered.get("reference_loot_template").and_then(Value::as_array).map_or(0,Vec::len),
        "leaf_item_count":ids.len(),"item_inputs":inputs,"client_metadata_inputs":metadata,
        "remaining_dependencies":["independent_inventory_audit","client_item_query_binding","ordinary_corpse_host_audit_and_creature_rows"],"refusals":refusals})
}

fn fingerprint(value: &Value) -> Result<[u8; 32]> {
    let text = value
        .as_str()
        .ok_or_else(|| fail("loot.assembly_fingerprint"))?;
    let mut bytes = [0; 32];
    check(text.len() == 64, "loot.assembly_fingerprint")?;
    for (byte, pair) in bytes.iter_mut().zip(text.as_bytes().as_chunks::<2>().0) {
        *byte = std::str::from_utf8(pair)
            .ok()
            .and_then(|s| u8::from_str_radix(s, 16).ok())
            .ok_or_else(|| fail("loot.assembly_fingerprint"))?;
    }
    Ok(bytes)
}
fn table(plan: &Value, name: &str) -> Result<Option<loot::captured::Table>> {
    let loader = &plan["input_bindings"]["loader"]["tables"][name];
    let Some(rows) = plan["ordered_rows"][name].as_array() else {
        return Ok(None);
    };
    let mut captured = Vec::with_capacity(rows.len());
    let mut positions = Vec::with_capacity(rows.len());
    for row in rows {
        let entry = row["entry"]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| fail("loot.assembly_entry"))?;
        let projected = json!({"item":row["item"],"chance_or_quest_chance_bits":row["chance_or_quest_chance_bits"],
            "group":row["groupid"],"min_count_or_reference":row["mincountOrRef"],"max_count":row["maxcount"],"condition_id":row["condition_id"]});
        captured.push(loot::captured::TemplateRow {
            entry,
            row: serde_json::from_value(projected).map_err(|_| fail("loot.assembly_row"))?,
        });
        positions.push(loot::captured::SelectedPosition {
            entry,
            item: row["item"]
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(|| fail("loot.assembly_item"))?,
            position: row["source_position"]
                .as_u64()
                .ok_or_else(|| fail("loot.assembly_position"))?,
        });
    }
    Ok(Some(loot::captured::Table {
        source_fingerprint: fingerprint(&plan["input_bindings"]["source_sha256"])?,
        ordered_result_fingerprint: fingerprint(&loader["ordered_result_sha256"])?,
        result_row_count: loader["result_row_count"]
            .as_u64()
            .ok_or_else(|| fail("loot.assembly_count"))?,
        rows: captured,
        selected_positions: positions,
    }))
}

pub(in crate::native) fn assemble(plan: &Value) -> Result<Option<CheckedLootDraft>> {
    let raw = &plan["raw_assembly"];
    if raw["status"] != "checked_projection" {
        return Ok(None);
    }
    let rows: Vec<inventory::captured::CapturedItem> =
        Vec::deserialize(&raw["item_inputs"]).map_err(|_| fail("loot.assembly_item_projection"))?;
    let items = inventory::captured::compile_items(rows).map_err(|errors| {
        fail(&format!(
            "loot.assembly_item_refused:{}:{}:{}",
            errors[0].item, errors[0].field, errors[0].reason
        ))
    })?;
    let catalog = loot::captured::CapturedLoot {
        body: table(plan, "creature_loot_template")?,
        references: table(plan, "reference_loot_template")?,
    }
    .compile_checked(&items)
    .map_err(|error| {
        fail(&format!(
            "loot.assembly_catalog_refused:{}:{}:{}:{}",
            error.table, error.entry, error.item, error.reason
        ))
    })?;
    let (body, references) = catalog.into_captures();
    let client_inputs = raw["client_metadata_inputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|input| {
            (
                input["entry"].as_u64().unwrap() as u32,
                input["row"].clone(),
            )
        })
        .collect();
    Ok(Some(CheckedLootDraft {
        items,
        body,
        references,
        client_inputs,
    }))
}
