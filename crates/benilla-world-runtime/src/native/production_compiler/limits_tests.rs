// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
fn rebind_items(source: &mut Value) {
    source["table_sha256"]["item_template"] =
        json!(json::digest(&source["tables"]["item_template"]));
    source["required_ids"]["item_template"] = json!(source["tables"]["item_template"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["entry"].clone())
        .collect::<Vec<_>>());
    let selection = source::Selection::parse(&source["selection"]).unwrap();
    let required = source["tables"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .chain(["db_version"])
        .collect();
    let profiles = source::schema(&source["schema"], &required).unwrap();
    source["report"]["unsupported"] = source::full_report(source, &profiles, &selection).unwrap();
}
#[test]
fn production_verifier_enforces_source_schema_row_and_field_limits() {
    let data = corpus();
    let fixture = &data["fixtures"][0];
    let mut changed = fixture.clone();
    let row = changed["source"]["tables"]["creature"][0].clone();
    changed["source"]["tables"]["creature"] = json!(vec![row; 4097]);
    assert_eq!(
        Inputs::new(&changed).run().unwrap_err().id,
        "rows.limit:creature"
    );
    let mut changed = fixture.clone();
    changed["source"]["schema"]["oversize"] = json!("a".repeat(2 * 1024 * 1024));
    assert_eq!(
        Inputs::new(&changed).run().unwrap_err().id,
        "bundle.schema_byte_limit"
    );
    let mut changed = fixture.clone();
    for field in ["Title", "Details", "Objectives", "OfferRewardText"] {
        changed["source"]["tables"]["quest_template"][0][field] = json!("a".repeat(16384));
    }
    assert_eq!(
        Inputs::new(&changed).run().unwrap_err().id,
        "row.byte_limit:quest_template"
    );
    let mut changed = fixture.clone();
    let source = &mut changed["source"];
    let template = source["tables"]["item_template"][0].clone();
    let items = source["tables"]["item_template"].as_array_mut().unwrap();
    while items.len() < 2048 {
        let mut row = template.clone();
        row["entry"] = json!(1_000_000 + items.len());
        items.push(row);
    }
    rebind_items(source);
    source::validate(source).unwrap();
    assert_eq!(
        Inputs::new(&changed).run().unwrap_err().id,
        "compiler.field_limit"
    );
}
#[test]
fn production_verifier_enforces_regular_file_byte_and_json_node_limits() {
    let data = corpus();
    let fixture = &data["fixtures"][0];
    let inputs = Inputs::new(fixture);
    std::fs::OpenOptions::new()
        .write(true)
        .open(inputs.root.join("source.json"))
        .unwrap()
        .set_len(source::MAX_BUNDLE as u64 + 1)
        .unwrap();
    assert_eq!(inputs.run().unwrap_err().id, "json.byte_limit");
    let inputs = Inputs::new(fixture);
    std::fs::OpenOptions::new()
        .write(true)
        .open(inputs.root.join("dbc/CharBaseInfo.dbc"))
        .unwrap()
        .set_len(dbc::MAX_FILE as u64 + 1)
        .unwrap();
    assert_eq!(inputs.run().unwrap_err().id, "dbc.input_byte_limit");
    let mut nodes = Vec::with_capacity(8_000_001);
    nodes.push(b'[');
    for _ in 0..3_999_999 {
        nodes.extend_from_slice(b"0,");
    }
    nodes.extend_from_slice(b"0]");
    assert_eq!(
        json::decode(&nodes, source::MAX_BUNDLE).unwrap_err().id,
        "json.node_limit"
    );
}
#[test]
fn production_verifier_enforces_whole_dbc_aggregate_limit() {
    let data = corpus();
    let inputs = Inputs::new(&data["fixtures"][0]);
    for (name, layout) in COMPILER_CONTRACT["dbc_layouts"].as_object().unwrap() {
        if name == "CharBaseInfo" {
            continue;
        }
        let size = layout["size"].as_u64().unwrap() as usize;
        let fields = layout["fields"].as_u64().unwrap() as u32;
        let count = 100000.min((dbc::MAX_FILE - 21) / size);
        let mut bytes = vec![0; 20 + count * size + 1];
        bytes[..4].copy_from_slice(b"WDBC");
        bytes[4..8].copy_from_slice(&(count as u32).to_le_bytes());
        bytes[8..12].copy_from_slice(&fields.to_le_bytes());
        bytes[12..16].copy_from_slice(&(size as u32).to_le_bytes());
        bytes[16..20].copy_from_slice(&1u32.to_le_bytes());
        for index in 0..count {
            let start = 20 + index * size;
            bytes[start..start + 4].copy_from_slice(&(index as u32 + 1).to_le_bytes());
        }
        std::fs::write(inputs.root.join("dbc").join(format!("{name}.dbc")), bytes).unwrap();
    }
    assert_eq!(inputs.run().unwrap_err().id, "dbc.aggregate_byte_limit");
}
#[test]
fn production_verifier_enforces_distinct_decoded_dbc_text_budget() {
    let size = 168usize;
    let count = 1025usize;
    let string_bytes = 1 + count * 16385;
    let mut bytes = vec![0; 20 + count * size + string_bytes];
    bytes[..4].copy_from_slice(b"WDBC");
    for (offset, value) in [
        (4, count as u32),
        (8, 42),
        (12, size as u32),
        (16, string_bytes as u32),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    for index in 0..count {
        let start = 20 + index * size;
        bytes[start..start + 4].copy_from_slice(&(index as u32 + 1).to_le_bytes());
        let offset = 1 + index * 16385;
        bytes[start + 4..start + 8].copy_from_slice(&(offset as u32).to_le_bytes());
        let start = 20 + count * size + offset;
        bytes[start..start + 16384].fill(b'a');
    }
    assert_eq!(
        dbc::Table::parse("Map", bytes).err().unwrap().id,
        "dbc.decoded_text_limit"
    );
}

#[test]
fn production_verifier_enforces_selected_dbc_binding_budget() {
    let data = corpus();
    let fixture = &data["fixtures"][0];
    let mut changed = fixture.clone();
    let source = &mut changed["source"];
    let template = source["tables"]["spell_template"][0].clone();
    let spells = source["tables"]["spell_template"].as_array_mut().unwrap();
    while spells.len() < 512 {
        let mut row = template.clone();
        row["Id"] = json!(1_000_000 + spells.len());
        spells.push(row);
    }
    let spell_ids: Vec<_> = spells.iter().map(|r| r["Id"].clone()).collect();
    source["required_ids"]["spell_template"] = json!(spell_ids);
    source["table_sha256"]["spell_template"] =
        json!(json::digest(&source["tables"]["spell_template"]));
    source["learning_capture"]["spell_ids"] = json!(spell_ids);
    source["spell_script_capture"]["selector"]["ids"] = json!(spell_ids);
    source["spell_script_capture"]["snapshot"]["table_sha256"]["spell_template"] =
        source["table_sha256"]["spell_template"].clone();
    let selection = source::Selection::parse(&source["selection"]).unwrap();
    let required = source["tables"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .chain(["db_version"])
        .collect();
    let profiles = source::schema(&source["schema"], &required).unwrap();
    source["report"]["unsupported"] = source::full_report(source, &profiles, &selection).unwrap();
    source::validate(source).unwrap();
    let inputs = Inputs::new(&changed);
    let count = 512usize;
    let size = 692usize;
    let strings = 16386usize;
    let mut bytes = vec![0; 20 + count * size + strings];
    bytes[..4].copy_from_slice(b"WDBC");
    for (offset, value) in [
        (4, count as u32),
        (8, 173),
        (12, size as u32),
        (16, strings as u32),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    for (index, id) in spell_ids.iter().enumerate() {
        let start = 20 + index * size;
        bytes[start..start + 4].copy_from_slice(&(id.as_u64().unwrap() as u32).to_le_bytes());
        for column in COMPILER_CONTRACT["dbc_layouts"]["Spell"]["strings"]
            .as_array()
            .unwrap()
        {
            let start = start + column.as_u64().unwrap() as usize * 4;
            bytes[start..start + 4].copy_from_slice(&1u32.to_le_bytes());
        }
    }
    bytes[20 + count * size + 1..20 + count * size + 16385].fill(b'a');
    let mut manifest: Value =
        serde_json::from_str(fixture["extraction_text"].as_str().unwrap()).unwrap();
    let record = manifest["files"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["file"] == "Spell.dbc")
        .unwrap();
    record["sha256"] = json!(json::hash_bytes(&bytes));
    record["bytes"] = json!(bytes.len());
    record["records"] = json!(count);
    std::fs::write(inputs.root.join("dbc/Spell.dbc"), bytes).unwrap();
    std::fs::write(
        inputs.root.join("dbc/extraction.json"),
        json::canonical(&manifest),
    )
    .unwrap();
    assert_eq!(
        inputs.run().unwrap_err().id,
        "compiler.dbc_binding_byte_limit"
    );
}
