// SPDX-License-Identifier: GPL-3.0-or-later
// Independently reconstructs captured_frost_armor_existing_spell_authority_v1.
use super::*;
const FROST_ADAPTER: &str = "captured_frost_armor_existing_spell_authority_v1";
fn frost_refusals(spell: &Value) -> Result<Vec<String>> {
    let id = int(&spell["id"]);
    let armor = id == 168;
    let scope = &spell["numeric_capture"];
    let Some(words) = spell["server_numeric_words"].as_array() else {
        return Ok(vec!["complete_effective_numeric_inputs_required".into()]);
    };
    if scope.is_null() {
        return Ok(vec!["complete_effective_numeric_inputs_required".into()]);
    }
    let w: [u32; 173] = std::array::from_fn(|i| int(&words[i]) as u32);
    let r = &scope["row"];
    let mut reasons = Vec::new();
    let mut refuse = |reason: &str| reasons.push(reason.to_owned());
    if !spell["server_client_differences"]
        .as_array()
        .unwrap()
        .is_empty()
    {
        refuse("server_client_mismatch");
    }
    let registry = &spell["script_registry"];
    let hooks = &spell["initial_hooks"];
    if registry.is_null() || !registry["rows"].as_array().unwrap().is_empty() {
        refuse("selected_registry_absence_required");
    }
    if hooks.is_null()
        || ["spell_area", "spell_proc_event"]
            .iter()
            .any(|t| !hooks[*t].as_array().unwrap().is_empty())
    {
        refuse("selected_hook_absence_required");
    }
    let expected = [
        (0, id as u32),
        (1, 4),
        (4, 1),
        (5, if armor { 0 } else { 11 }),
        (6, if armor { 0x10000 } else { 0 }),
        (7, if armor { 0 } else { 0x488 }),
        (21, if armor { 8 } else { 0 }),
        (24, if armor { 40 } else { 0 }),
        (25, if armor { 100 } else { 101 }),
        (31, 0),
        (36, if armor { 1 } else { 4 }),
        (58, u32::MAX),
        (59, u32::MAX),
        (61, 6),
        (62, 6),
        (63, 0),
        (82, if armor { 1 } else { 6 }),
        (83, if armor { 1 } else { 6 }),
        (84, 0),
        (91, if armor { 22 } else { 138 }),
        (92, if armor { 42 } else { 33 }),
        (93, 0),
        (106, u32::from(armor)),
        (107, 0),
        (108, 0),
        (109, 0),
        (110, if armor { 6136 } else { 0 }),
        (111, 0),
        (160, 3),
        (161, if armor { 0x2080000 } else { 0x100000 }),
        (162, 0),
        (164, 1),
        (165, 1),
    ];
    if expected.into_iter().any(|(i, v)| w[i] != v) {
        refuse("frost_armor_source_morphology");
    }
    if w.iter().enumerate().any(|(i, v)| {
        *v != 0
            && !matches!(i,0..=7|18..=21|24..=25|27..=30|32|36|38|
        58..=59|61..=69|76..=78|82..=84|91..=93|106..=111|115..=158|160..=162|164..=169)
    }) {
        refuse("unexecuted_frost_armor_numeric_field");
    }
    let mut modifier = nonzero(&r["IsServerSide"]) || nonzero(&r["AttributesServerside"]);
    for slot in 1..=3 {
        modifier |= dbc::binary32(
            r[format!("EffectBonusCoefficient{slot}")]["decimal"]
                .as_str()
                .unwrap(),
        )? != 0;
        modifier |= dbc::binary32(
            r[format!("EffectBonusCoefficientFromAP{slot}")]["decimal"]
                .as_str()
                .unwrap(),
        )? != 0;
    }
    if modifier
        || scope["facing"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| nonzero(&r["facingcasterflag"]))
    {
        refuse("frost_armor_source_modifier");
    }
    let cast = &spell["cast_time"];
    let duration = &spell["duration"];
    let range = &spell["range"];
    if w[27] != 0
        || w[28] > 60
        || w[29] > 60
        || w[32] > 1_000_000
        || w[156] != 0
        || !armor && [32, 157, 158].iter().any(|i| w[*i] != 0)
        || armor && w[32] == 0
        || [2, 19, 20].iter().any(|i| w[*i] != 0)
        || w[158] > 3_600_000
        || w[158] != 0 && w[157] == 0
        || cast.is_null()
        || int(&cast["id"]) != i128::from(w[18])
        || w[18] == 0
        || ["base_ms", "per_level_ms", "minimum_ms"]
            .iter()
            .any(|n| nonzero(&cast[*n]))
        || range.is_null()
        || int(&range["id"]) != i128::from(w[36])
        || nonzero(&range["minimum_binary32"])
        || nonzero(&range["flags"])
        || armor && nonzero(&range["maximum_binary32"])
    {
        refuse("frost_armor_cast_and_resource_source");
    }
    if duration.is_null()
        || int(&duration["id"]) != i128::from(w[30])
        || !(1..=3_600_000).contains(&int(&duration["base_ms"]))
        || nonzero(&duration["per_level_ms"])
        || duration["maximum_ms"] != duration["base_ms"]
    {
        refuse("frost_armor_finite_duration");
    }
    for slot in 0..3 {
        let values = (w[64 + slot], w[67 + slot], w[76 + slot]);
        let amount = i64::from(values.2 as i32) + i64::from(values.1);
        if !matches!(values.0, 0 | 1)
            || !matches!(values.1, 0 | 1)
            || slot == 2 && values != (0, 0, 0)
            || armor && slot == 1 && values != (0, 0, 0)
            || !matches!(w[167 + slot], 0 | 0x3f800000)
            || armor && slot == 0 && !(0..=1_000_000).contains(&amount)
            || !armor && slot == 0 && !(-1000..=0).contains(&amount)
            || !armor && slot == 1 && !(-100..=0).contains(&amount)
        {
            refuse(&format!("frost_armor_effect_{}_amount", slot + 1));
        }
    }
    Ok(reasons)
}
pub(super) fn frost_plan(spell: &Value, spells: &[Value]) -> Result<Value> {
    let mut plan = json!({"adapter":FROST_ADAPTER,"status":"blocked","classification":"existing_spell_authority",
        "effective_learning_proven":false,"runtime_export":null,"refusals":[]});
    let mut reasons = frost_refusals(spell)?;
    let armor = int(&spell["id"]) == 168;
    let other = if armor { 6136 } else { 168 };
    match spells.iter().find(|s| int(&s["id"]) == other) {
        Some(s) if frost_refusals(s)?.is_empty() => {}
        _ => reasons.push(
            if armor {
                "triggered_chilled_source_closure_required"
            } else {
                "frost_armor_proc_host_required"
            }
            .into(),
        ),
    }
    if reasons.is_empty() {
        plan["status"] = json!("supported_shape");
        plan["input_bindings"] = json!({"numeric_capture_sha256":spell["numeric_capture"]["coverage_sha256"],
            "effective_words_sha256":digest(&spell["server_numeric_words"]),
            "registry_sha256":spell["script_registry"]["coverage_sha256"],"initial_hooks_sha256":spell["initial_hooks"]["coverage_sha256"]});
    }
    plan["refusals"] = json!(reasons
        .into_iter()
        .map(|r| format!("spell_execution.{r}"))
        .collect::<Vec<_>>());
    Ok(plan)
}
