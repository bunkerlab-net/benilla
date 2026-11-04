// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[test]
fn reached_active_callback_location_escape_and_missing_manifest_refuse_independently() {
    let mut raw = serde_json::Map::new();
    for table in [
        "spell_chain",
        "spell_proc_event",
        "spell_affect",
        "spell_threat",
        "spell_elixir",
        "spell_group_spell",
        "spell_scripts",
        "spell_area",
    ] {
        raw.insert(table.into(), json!([]));
    }
    raw.insert(
        "spell_scripts".into(),
        json!([{"Id":81,"ScriptName":"spell_gameobject_call_for_help_on_usage"}]),
    );
    let mut source = json!({"selection":{"map_id":0,"min_milliunits":[-1000,-1000,-1000],"max_milliunits":[1000,1000,1000]},
        "tables":{"playercreateinfo":[{"map":0,"position_x":{"decimal":"0"},"position_y":{"decimal":"0"},"position_z":{"decimal":"0"}}]},
        "global_hook_capture":{"registration_manifest_sha256":digest(&CONTRACT["manifest"]),"configuration":CONTRACT["manifest"]["configuration"],"rows":raw}});
    let mut words = vec![0; 173];
    words[0] = 81;
    let loader = json!({"default_spell_candidates":[81],"loaded_learning_maps_resolved":true});
    let numeric = json!({"status":"checked"});
    let dbcs = Tables::new();
    let run = |source: &Value, words: &[u32]| {
        let sha = digest(source);
        let c = Compiler {
            source,
            source_sha: &sha,
            dbcs: &dbcs,
            request: ProductionRequest {
                race: 1,
                class: 8,
                gender: 0,
            },
            issues: vec![],
            dependencies: BTreeSet::new(),
            selected: BTreeMap::new(),
            binding_bytes: 0,
        };
        plan(
            &c,
            &json!([{"id":81,"server_numeric_words":words}]),
            &loader,
            &numeric,
        )
    };
    let absent = run(&source, &words);
    assert_eq!(absent["roots"][0]["status"], "hooks_unreachable");
    assert_eq!(absent["discharged_root_count"], 0);
    assert_eq!(absent["roots"][0]["effective_learning_proven"], false);
    words[6] = 0x40;
    let reached = run(&source, &words);
    assert_eq!(reached["roots"][0]["status"], "blocked");
    assert_eq!(
        reached["roots"][0]["refusals"],
        json!(["reached_callback_unsupported"])
    );
    source["tables"]["playercreateinfo"][0]["position_x"]["decimal"] = json!("2");
    assert!(run(&source, &words)["roots"][0]["refusals"]
        .as_array()
        .unwrap()
        .contains(&json!("first_entry_location_outside_selection")));
    source["global_hook_capture"]
        .as_object_mut()
        .unwrap()
        .remove("registration_manifest_sha256");
    let unknown = run(&source, &words);
    assert_eq!(unknown["roots"][0]["status"], "unknown");
    assert!(unknown["roots"][0]["refusals"]
        .as_array()
        .unwrap()
        .contains(&json!("registration_manifest_unknown")));
}
