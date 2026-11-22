// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::collections::BTreeMap;

fn definition(name: &str, ordinal: u64, go: bool) -> SourceScriptDefinition {
    SourceScriptDefinition {
        name: name.as_bytes().to_vec(),
        registration: ordinal,
        operation: ordinal.to_string(),
        source_file: "authored.cpp".into(),
        source_line: 1,
        source_function: "AddSC_authored".into(),
        slots: BTreeMap::from([("GetGameObjectAI".into(), go)]),
    }
}

#[test]
fn script_registry_zero_lookup_refuses_only_before_resize() {
    let mut registry = SourceScriptRegistry::default();
    assert_eq!(
        registry.get_script(0).unwrap_err().id,
        "execution.script_registry.ScriptDevAIMgr_439.zero_index_undefined"
    );
    assert!(registry.get_script(1).unwrap().is_none());
    registry.load_script_names(|| Ok(None)).unwrap();
    registry.initialize_registry([]).unwrap();
    assert_eq!(registry.size(), 1);
    assert!(registry.get_script(0).unwrap().is_none());
    assert!(registry.get_script(1).unwrap().is_none());
}

#[test]
fn script_registry_names_sort_registration_order_overwrites_and_count() {
    let mut registry = SourceScriptRegistry::default();
    registry
        .load_script_names(|| Ok(Some(vec![b"z".to_vec(), b"a".to_vec()])))
        .unwrap();
    assert_eq!(registry.get_script_id(None), 0);
    assert_eq!(registry.get_script_id(Some(b"")), 0);
    assert_eq!(registry.get_script_id(Some(b"absent")), 0);
    assert_eq!(registry.get_script_id(Some(b"a")), 1);
    assert_eq!(registry.get_script_id(Some(b"z")), 2);
    registry
        .initialize_registry([
            definition("z", 0, false),
            definition("absent", 1, true),
            definition("a", 2, false),
            definition("z", 3, true),
        ])
        .unwrap();
    assert_eq!(
        (
            registry.size(),
            registry.script_count(),
            registry.null_slots()
        ),
        (3, 3, 1)
    );
    assert_eq!(registry.get_script(2).unwrap().unwrap().registration, 3);
    assert!(registry.check_script_names().unwrap().is_empty());
    assert_eq!(
        registry.gameobject_ai(2).unwrap_err().id,
        format!("{}.z", script_registry::SCRIPTDEV_GET_GAMEOBJECT_AI_OWNER)
    );
    registry.gameobject_ai(0).unwrap();
    registry.gameobject_ai(1).unwrap();
    registry.gameobject_ai(3).unwrap();
}

#[test]
fn script_registry_query_refusal_keeps_preceding_empty_name() {
    let mut registry = SourceScriptRegistry::default();
    assert_eq!(
        registry
            .load_script_names(|| Err(refuse("authored.query")))
            .unwrap_err()
            .id,
        "authored.query"
    );
    assert_eq!(registry.names(), &[Vec::<u8>::new()]);
}

#[test]
fn script_registry_add_script_zero_and_nonzero_index_boundaries() {
    let mut registry = SourceScriptRegistry::default();
    registry.add_script(0, definition("a", 0, false)).unwrap();
    assert_eq!(registry.script_count(), 0);
    assert_eq!(
        registry
            .add_script(1, definition("a", 0, false))
            .unwrap_err()
            .id,
        "execution.script_registry.ScriptDevAIMgr_431.index_undefined"
    );
    registry
        .load_script_names(|| Ok(Some(vec![b"a".to_vec(), b"z".to_vec()])))
        .unwrap();
    registry.initialize_registry([]).unwrap();
    registry.add_script(2, definition("z", 0, false)).unwrap();
    assert_eq!(registry.check_script_names().unwrap(), vec![1]);
    assert_eq!(
        registry
            .add_script(3, definition("absent", 1, false))
            .unwrap_err()
            .id,
        "execution.script_registry.ScriptDevAIMgr_431.index_undefined"
    );
    assert_eq!(registry.script_count(), 1);
}

#[test]
fn script_registry_unexecuted_outputs_never_become_empty_results() {
    let mut registry = SourceScriptRegistry::default();
    registry.load_script_names(|| Ok(None)).unwrap();
    registry.initialize_registry([]).unwrap();
    assert_eq!(registry.unexecuted_prerequisites().len(), 5);
    for prerequisite in registry.unexecuted_prerequisites() {
        assert_eq!(
            registry.require_prerequisite(*prerequisite).unwrap_err().id,
            prerequisite.owner()
        );
        assert!(!prerequisite.source().is_empty());
    }
}

#[test]
fn script_registry_count_overflow_refuses_after_the_preceding_store() {
    let mut registry = SourceScriptRegistry::default();
    registry
        .load_script_names(|| Ok(Some(vec![b"a".to_vec()])))
        .unwrap();
    assert_eq!(
        registry.check_script_names().unwrap_err().id,
        "execution.script_registry.ScriptDevAIMgr_526.index_undefined"
    );
    registry.initialize_registry([]).unwrap();
    assert_eq!(registry.check_script_names().unwrap(), [1]);
    registry.script_count = i32::MAX - 1;
    registry.add_script(1, definition("a", 0, false)).unwrap();
    assert_eq!(registry.script_count(), i32::MAX);
    assert_eq!(
        registry
            .add_script(1, definition("a", 1, false))
            .unwrap_err()
            .id,
        "execution.script_registry.ScriptDevAIMgr_432.count_overflow_undefined"
    );
    assert_eq!(registry.get_script(1).unwrap().unwrap().registration, 1);
    assert_eq!(registry.script_count(), i32::MAX);
}

#[test]
fn script_registry_resize_preserves_existing_slots_and_does_not_reset_count() {
    let mut registry = SourceScriptRegistry::default();
    registry
        .load_script_names(|| Ok(Some(vec![b"a".to_vec()])))
        .unwrap();
    registry
        .initialize_registry([definition("a", 0, false)])
        .unwrap();
    registry.initialize_registry([]).unwrap();
    assert_eq!(registry.script_count(), 1);
    assert_eq!(registry.get_script(1).unwrap().unwrap().registration, 0);
    registry.register_self(definition("a", 1, true)).unwrap();
    assert_eq!(registry.script_count(), 2);
    assert_eq!(registry.get_script(1).unwrap().unwrap().registration, 1);
}

#[test]
fn gameobject_script_factory_names_each_unported_callback() {
    let mut registry = SourceScriptRegistry::default();
    let names = ["go_bells", "go_covert_ops", "go_lightning"];
    registry
        .load_script_names(|| Ok(Some(names.map(|name| name.as_bytes().to_vec()).into())))
        .unwrap();
    registry
        .initialize_registry([
            definition(names[0], 0, true),
            definition(names[1], 1, true),
            definition(names[2], 2, true),
        ])
        .unwrap();
    let select = |name: &str| registry.gameobject_ai(registry.get_script_id(Some(name.as_bytes())));
    assert_eq!(
        select(names[0]).unwrap(),
        Some(gameobjects::GameObjectScript::Bells)
    );
    for name in &names[1..] {
        assert_eq!(
            select(name).unwrap_err().id,
            format!(
                "{}.{name}",
                script_registry::SCRIPTDEV_GET_GAMEOBJECT_AI_OWNER
            )
        );
    }
}
