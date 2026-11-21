// SPDX-License-Identifier: GPL-3.0-or-later
//! All SQL-shaped values below are authored, not copied database rows.
use super::spawn_context::*;
use super::*;
use serde_json::json;

fn capture() -> CapturedSpawnContext {
    CapturedSpawnContext {
        spawns: init::Rows::Complete(vec![serde_json::from_value(json!({
            "guid": 2, "id": 6, "map": 0, "position_x": {"decimal":"11.5"},
            "position_y": {"decimal":"-7"}, "position_z": {"decimal":"3"},
            "orientation": {"decimal":"1.25"}, "spawntimesecsmin": 7,
            "spawntimesecsmax": 9, "spawndist": {"decimal":"5"},
            "MovementType": 1, "spawnMask": 1
        }))
        .unwrap()]),
        spawn_data: init::Rows::Complete(vec![]),
        spawn_templates: init::Rows::Complete(vec![serde_json::from_value(json!({
            "Entry": 0, "NpcFlags": -1, "UnitFlags": -1, "Faction": 0,
            "ModelId": 0, "EquipmentId": -1, "CurHealth": 1, "CurMana": 0,
            "SpawnFlags": 0, "RelayId": 0, "StringId": 0, "Name": "Authored default"
        }))
        .unwrap()]),
        exclusions: EXCLUSION_TABLES
            .iter()
            .map(|&name| (name.to_owned(), init::Rows::Complete(vec![])))
            .collect(),
    }
}

#[test]
fn captured_respawn_position_and_radius_five_bind_existing_owners() {
    let plan = capture().compile(0, &[2]).unwrap();
    let row = plan.spawn(2).unwrap();
    assert_eq!(row.position.x.as_f32(), 11.5);
    assert_eq!(row.position.y.as_f32(), -7.0);
    assert_eq!(row.orientation.bits(), 1.25_f32.to_bits());
    assert!(
        matches!(row.default_movement, benilla_sim::creature_motion::DefaultMovement::Wander { radius, center }
        if radius.as_f32() == 5.0 && center == row.position)
    );
    let mut source = synthetic_creature_generation_fixture::source(2, 1, 2);
    source.spawn_entry = 6;
    source.input.template.entry = 6;
    let bound = plan.bind_generation(source).unwrap();
    assert_eq!(
        (
            bound.respawn_delay_min_seconds,
            bound.respawn_delay_max_seconds
        ),
        (7, 9)
    );
    assert_eq!(
        bound.input.spawn_template,
        init::Rows::Complete(vec![init::SpawnTemplate {
            entry: 0,
            model_id: 0,
            equipment_id: -1,
            npc_flags: -1,
            unit_flags: -1,
            faction: 0,
            current_health: 1,
            current_mana: 0,
            spawn_flags: 0,
            relay_id: 0,
            string_id: 0,
        }])
    );
}

#[test]
fn exclusions_and_unknown_evidence_never_become_empty() {
    for table in EXCLUSION_TABLES {
        let mut input = capture();
        input
            .exclusions
            .insert((*table).into(), init::Rows::Missing);
        assert!(input.compile(0, &[2]).is_err(), "missing {table}");
        let mut input = capture();
        input
            .exclusions
            .insert((*table).into(), init::Rows::Complete(vec![BTreeMap::new()]));
        assert!(input.compile(0, &[2]).is_err(), "present {table}");
    }
    let mut input = capture();
    input.spawn_templates = init::Rows::Complete(vec![]);
    assert!(input.compile(0, &[2]).is_err());
    assert!(capture().compile(0, &[2, 3]).is_err());
    assert!(capture().compile(1, &[2]).is_err());
}

#[test]
fn loader_normalizes_respawn_and_idle_without_inventing_waypoints() {
    let mut input = capture();
    let init::Rows::Complete(rows) = &mut input.spawns else {
        unreachable!()
    };
    rows[0].insert("spawntimesecsmax".into(), json!(2));
    rows[0].insert("MovementType".into(), json!(0));
    let plan = input.compile(0, &[2]).unwrap();
    let row = plan.spawn(2).unwrap();
    assert_eq!(row.respawn_seconds, [7, 7]);
    assert_eq!(
        row.default_movement,
        benilla_sim::creature_motion::DefaultMovement::Idle
    );
    for (key, value) in [
        ("MovementType", json!(2)),
        ("spawnMask", json!(0)),
        ("id", json!(0)),
    ] {
        let mut input = capture();
        let init::Rows::Complete(rows) = &mut input.spawns else {
            unreachable!()
        };
        rows[0].insert(key.into(), value);
        assert!(input.compile(0, &[2]).is_err(), "{key}");
    }
}

#[test]
fn selected_radius_bounds_feed_the_existing_linear_wander_sampler() {
    use benilla_sim::creature_motion as motion;
    let plan = capture().compile(0, &[2]).unwrap();
    let selected = plan.spawn(2).unwrap();
    let motion::DefaultMovement::Wander { center, radius } = selected.default_movement else {
        panic!()
    };
    let request = motion::WanderRequest {
        prepared: motion::QueryBinding {
            identity: motion::Identity {
                creature: 2,
                map: 0,
                content: ai::movement::Revision([3; 32]),
                geometry: ai::movement::Revision([4; 32]),
            },
            query_id: 1,
            now_ms: 0,
        },
        center,
        radius,
        angle_draw: ai::Real::ZERO,
        radius_draw: ai::Real::new(0.5).unwrap(),
    };
    let midpoint = request.endpoint().unwrap();
    assert_eq!(midpoint.x.as_f32(), 14.0);
    assert_eq!(midpoint.y.as_f32(), -7.0);
    for angle in [0, 1, u32::MAX / 4, u32::MAX / 2, u32::MAX - 1] {
        for distance in [0, 1, u32::MAX / 2, u32::MAX - 1] {
            let point = motion::WanderRequest {
                angle_draw: motion::wander_normalized_word(angle).unwrap(),
                radius_draw: motion::wander_normalized_word(distance).unwrap(),
                ..request
            }
            .endpoint()
            .unwrap();
            let dx = point.x.as_f32() - center.x.as_f32();
            let dy = point.y.as_f32() - center.y.as_f32();
            assert!(dx * dx + dy * dy <= 25.00002);
            assert_eq!(point.z, center.z);
        }
    }
}

#[test]
fn four_quest_giver_entries_use_captured_static_poses_and_idle_movement() {
    for entry in [196, 197, 198, 823] {
        let mut input = capture();
        let init::Rows::Complete(rows) = &mut input.spawns else {
            unreachable!()
        };
        rows[0].insert("id".into(), json!(entry));
        rows[0].insert("MovementType".into(), json!(0));
        let plan = input.compile(0, &[2]).unwrap();
        assert_eq!(plan.spawn(2).unwrap().entry, entry);
        assert_eq!(
            plan.spawn(2).unwrap().default_movement,
            benilla_sim::creature_motion::DefaultMovement::Idle
        );
    }
}

#[test]
fn generation_binding_refuses_unimplemented_override_instead_of_hiding_it() {
    for (field, value) in [
        ("RelayId", 9),
        ("SpawnFlags", 4),
        ("StringId", 8),
        ("NpcFlags", 123),
    ] {
        let mut input = capture();
        let init::Rows::Complete(rows) = &mut input.spawn_templates else {
            unreachable!()
        };
        rows[0].insert(field.into(), json!(value));
        let plan = input.compile(0, &[2]).unwrap();
        let mut source = synthetic_creature_generation_fixture::source(2, 1, 2);
        source.spawn_entry = 6;
        source.input.template.entry = 6;
        assert!(plan.bind_generation(source).is_err(), "{field}");
    }
}

#[test]
fn explicit_template_needs_no_invented_default_and_dangling_override_refuses() {
    let mut input = capture();
    input.spawn_data =
        init::Rows::Complete(vec![
            serde_json::from_value(json!({"Guid": 2, "Id": 19})).unwrap()
        ]);
    let init::Rows::Complete(rows) = &mut input.spawn_templates else {
        unreachable!()
    };
    rows[0].insert("Entry".into(), json!(19));
    let plan = input.compile(0, &[2]).unwrap();
    let mut source = synthetic_creature_generation_fixture::source(2, 1, 2);
    source.spawn_entry = 6;
    source.input.template.entry = 6;
    assert_eq!(
        plan.bind_generation(source)
            .unwrap()
            .requested_spawn_template_entry,
        19
    );
    input.spawn_templates = init::Rows::Complete(vec![]);
    assert!(input.compile(0, &[2]).is_err());
}
