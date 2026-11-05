// SPDX-License-Identifier: GPL-3.0-or-later
use super::super::dbc::Tables;
use super::*;

fn blocked(mut result: Value, reason: &str) -> Value {
    result["reason"] = json!(reason);
    result
}

pub(in crate::native::production_compiler) fn coverage(
    source: &Value,
    template: Option<&Value>,
    dbcs: &Tables,
) -> Value {
    let mut result = json!({"entry":template.map(|t| &t["entry"]),
        "taxi_path_id":template.map(|t| &t["data0"]),
        "selector_map":source["selection"]["map_id"],"status":"blocked"});
    let Some(template) = template else {
        return blocked(result, "template_unknown");
    };
    if template["type"] != 15 {
        return blocked(result, "mo_transport_template_required");
    }
    let (Some(paths), Some(table)) = (dbcs.get("TaxiPath"), dbcs.get("TaxiPathNode")) else {
        return blocked(result, "reextract_v5_required");
    };
    let path_id = int(&template["data0"]) as u32;
    if path_id == 0 || paths.get(path_id).is_none() {
        return blocked(result, "taxi_path_unknown");
    }
    // DBCStores.cpp:485-508 uses path/node indices, not physical record order.
    let mut nodes: Vec<_> = (0..table.count)
        .map(|i| table.row(i))
        .filter(|r| r.word(1) == path_id)
        .collect();
    nodes.sort_by_key(|r| r.word(2));
    result["raw_node_count"] = json!(nodes.len());
    result["nodes_sha256"] = json!(digest(&json!(nodes
        .iter()
        .map(|r| r.raw())
        .collect::<Vec<_>>())));
    if nodes.len() < 4
        || nodes
            .iter()
            .enumerate()
            .any(|(i, r)| r.word(0) == 0 || r.word(2) as usize != i)
    {
        return blocked(result, "path_node_order_or_coverage_invalid");
    }
    if nodes
        .iter()
        .any(|r| (4..7).any(|i| r.word(i) & 0x7f800000 == 0x7f800000))
    {
        return blocked(result, "path_coordinate_nonfinite");
    }
    if int(&template["data1"]) == 0 || int(&template["data2"]) == 0 {
        return blocked(result, "path_speed_or_acceleration_invalid");
    }
    if dbcs
        .get("Map")
        .is_none_or(|maps| nodes.iter().any(|r| maps.get(r.word(3)).is_none()))
    {
        return blocked(result, "path_map_unknown");
    }
    // TransportMgr.cpp:100-124 discards endpoints and the pair at each teleport.
    let (mut frames, mut teleports, mut changing) = (Vec::new(), Vec::new(), false);
    for i in 1..nodes.len() - 1 {
        if changing {
            changing = false;
        } else if nodes[i].word(7) & 1 != 0 || nodes[i].word(3) != nodes[i + 1].word(3) {
            let Some(last) = frames.last() else {
                return blocked(result, "path_initial_teleport_invalid");
            };
            teleports.push(*last);
            changing = true;
        } else {
            frames.push(i);
        }
    }
    if frames.len() < 2 || matches!(path_id, 293 | 303) && frames.len() <= 12 {
        return blocked(result, "path_keyframes_invalid");
    }
    teleports.push(*frames.last().unwrap()); // Last-to-first always teleports, line 139.
    let mut legs = Vec::new();
    let mut start = 0;
    for i in 1..frames.len() {
        let teleport = teleports.contains(&frames[i - 1]);
        if teleport || i + 1 == frames.len() {
            let leg = &frames[start..if teleport { i } else { i + 1 }];
            if leg.len() < 2
                || leg
                    .iter()
                    .any(|j| nodes[*j].word(3) != nodes[leg[0]].word(3))
            {
                return blocked(result, "path_spline_leg_invalid");
            }
            legs.push(leg);
            start = i;
        }
    }
    let maps: BTreeSet<_> = frames.iter().map(|i| nodes[*i].word(3)).collect();
    let disjoint = !maps.contains(&(int(&source["selection"]["map_id"]) as u32));
    result["keyframe_node_indices"] = json!(frames);
    result["teleport_after_node_indices"] = json!(teleports);
    result["spline_legs"] = json!(legs);
    result["maps"] = json!(maps);
    if disjoint {
        result["status"] = json!("excluded_other_maps");
        result["reason"] = json!("map_disjoint");
        result["margin"] = json!("not_applicable_map_identity");
        result
    } else {
        // Transports.cpp:402-439 passes an unclamped t to Catmull-Rom evaluation.
        blocked(result, "interpolation_parameter_unproved")
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::dbc::Table;
    use super::*;

    #[test]
    fn malformed_paths_never_become_map_exclusions() {
        let corpus: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../scripts/native_starter/production_verifier_cases.json"
        )))
        .unwrap();
        let fixture = corpus["fixtures"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == "transport-other-map")
            .unwrap();
        let source = &fixture["source"];
        let template = &source["participants_capture"]["rows"]["gameobject_template"][0];
        let mut tables = Tables::new();
        for (name, hex) in fixture["dbc_hex"].as_object().unwrap() {
            let bytes = hex
                .as_str()
                .unwrap()
                .as_bytes()
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
                .collect();
            tables.insert(name.clone(), Table::parse(name, bytes).unwrap());
        }
        let original = tables["TaxiPathNode"].data.clone();
        for (kind, expected) in [
            ("hole", "path_node_order_or_coverage_invalid"),
            ("duplicate", "path_node_order_or_coverage_invalid"),
            ("zero_id", "path_node_order_or_coverage_invalid"),
            ("nan", "path_coordinate_nonfinite"),
            ("infinity", "path_coordinate_nonfinite"),
            ("map", "path_map_unknown"),
            ("initial_teleport", "path_initial_teleport_invalid"),
            ("short_leg", "path_spline_leg_invalid"),
        ] {
            let mut bytes = original.clone();
            let (node, column, value) = match kind {
                "duplicate" => (2, 2, 1u32),
                "zero_id" => (0, 0, 0),
                "nan" => (2, 4, 0x7fc00000),
                "infinity" => (2, 4, 0x7f800000),
                "map" => (2, 3, 99999),
                "initial_teleport" => (1, 7, 1),
                "short_leg" => (2, 7, 1),
                _ => (0, 0, 100),
            };
            let offset = 20 + node * 36 + column * 4;
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            if kind == "hole" {
                bytes.drain(20 + 2 * 36..20 + 3 * 36);
                bytes[4..8].copy_from_slice(&5u32.to_le_bytes());
            }
            tables.insert(
                "TaxiPathNode".into(),
                Table::parse("TaxiPathNode", bytes).unwrap(),
            );
            let result = coverage(source, Some(template), &tables);
            assert_eq!(result["status"], "blocked", "{kind}");
            assert_eq!(result["reason"], expected, "{kind}");
        }
    }
}
