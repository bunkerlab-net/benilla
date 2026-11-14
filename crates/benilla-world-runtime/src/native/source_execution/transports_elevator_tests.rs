// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored elevator tests use real F3 relocation/update sets and chronological model adapters, not F6 geometry.
use super::*;

fn oracle(index: usize) -> Vec<u32> {
    include_str!("../../../../benilla-sim/tests/data/source_elevators.tsv")
        .lines()
        .nth(index)
        .unwrap()
        .split_whitespace()
        .map(|word| u32::from_str_radix(word, 16).unwrap())
        .collect()
}
fn record(id: u32, time: u32, position: [u32; 3]) -> TransportAnimationRecord {
    TransportAnimationRecord {
        id,
        ordinal: id,
        entry: 100,
        time,
        position,
    }
}
fn records() -> Vec<TransportAnimationRecord> {
    let words = oracle(0);
    let a = words[..3].try_into().unwrap();
    let b = words[3..6].try_into().unwrap();
    vec![
        record(1, 0, a),
        record(2, 100, a),
        record(3, 700, b),
        record(4, 1000, b),
    ]
}
fn elevator_fixture(animated: bool, rotated: bool) -> Fixture {
    let words = oracle(if rotated { 7 } else { 0 });
    let mut row = spawn();
    row.position_bits = [words[13], words[14], words[15], 0];
    row.rotation_bits = words[9..13].try_into().unwrap();
    Fixture::with(row, false, |owner| {
        owner.templates.get_mut(&100).unwrap().kind = 11;
        owner.animations = SourceTransportTemplates::default();
        owner
            .animations
            .load_animations(if animated { records() } else { vec![] }, &mut |_| Ok(()))
            .unwrap();
    })
}
fn expected(index: usize) -> SourcePose {
    let row = oracle(index);
    SourcePose([row[16], row[17], row[18], 0])
}
fn state(f: &Fixture) -> &SourceElevator {
    f.go().elevator.as_ref().unwrap()
}

#[test]
fn absent_animation_skips_snapshot_and_every_generic_update_effect() {
    // Elevator Update returns before reading the map clock, never GameObject::Update; Transports.cpp:454-459 at 8ec338a1.
    let mut f = elevator_fixture(false, false);
    let before = f.world.objects.pose(f.object).unwrap();
    f.factory.objects.get_mut(f.object).unwrap().loot_state = 3;
    let draws = f.factory.cursor.raw;
    f.update();
    assert!(f.factory.life().events.is_empty());
    assert_eq!(f.factory.cursor.raw, draws);
    assert_eq!(f.world.objects.pose(f.object).unwrap(), before);
    assert_eq!(state(&f).progress, 0);
}

#[test]
fn intervals_include_next_endpoint_and_wrap_without_wrapping_the_node_lookup() {
    // lower_bound selects exact next and strict previous, zero progress has no previous; TransportMgr.cpp:314-332 at 8ec338a1.
    let mut f = elevator_fixture(true, false);
    for (index, time, segment) in [
        (0, 100, 0),
        (1, 101, 100),
        (2, 333, 100),
        (3, 699, 100),
        (4, 700, 100),
    ] {
        f.factory.life().clock_ms = time;
        f.update();
        assert_eq!(f.world.objects.pose(f.object).unwrap(), expected(index));
        assert_eq!(
            (state(&f).progress, state(&f).current_segment),
            (time as u32, segment)
        );
    }
    let before = f.world.objects.pose(f.object).unwrap();
    f.factory.life().events.clear();
    f.factory.life().clock_ms = 1000;
    f.update();
    assert_eq!(state(&f).progress, 0);
    assert_eq!(state(&f).current_segment, 100);
    assert_eq!(f.world.objects.pose(f.object).unwrap(), before);
    assert_eq!(f.factory.life().events, [Event::ElevatorSnapshot(map())]);
}

#[test]
fn unsigned_snapshot_subtraction_wraps_and_does_not_use_supplied_diff() {
    // GenericTransport starts movementStarted at zero and subtracts uint32; Transports.h:33, Transports.cpp:454-459 at 8ec338a1.
    let mut f = elevator_fixture(true, false);
    assert_eq!(state(&f).movement_started, 0);
    f.factory
        .objects
        .get_mut(f.object)
        .unwrap()
        .elevator
        .as_mut()
        .unwrap()
        .movement_started = u32::MAX - 99;
    f.factory.life().clock_ms = 1;
    f.apply(|o, h, c| update_source_gameobject(o, h, u32::MAX, c))
        .unwrap();
    assert_eq!(state(&f).progress, 101);
    assert_eq!(f.world.objects.pose(f.object).unwrap(), expected(1));
}

#[test]
fn quaternion_left_product_y_flip_and_stationary_origin_do_not_accumulate() {
    // currentPos * rotation normalizes and uses the original stationary offset; Transports.cpp:477-481 at 8ec338a1.
    let mut f = elevator_fixture(true, true);
    f.factory.life().clock_ms = 333;
    let origin = state(&f).stationary;
    f.update();
    assert_eq!(f.world.objects.pose(f.object).unwrap(), expected(9));
    f.update();
    assert_eq!(f.world.objects.pose(f.object).unwrap(), expected(9));
    assert_eq!(state(&f).stationary, origin);
    assert_eq!(
        f.factory
            .life()
            .events
            .iter()
            .filter(|e| **e == Event::Model(f.object, Op::Relocate))
            .count(),
        2
    );
}

#[test]
fn single_nonzero_time_and_beyond_final_node_have_no_pair() {
    // GetPrev returns null for both begin and end, not a cyclic predecessor; TransportMgr.cpp:314-323 at 8ec338a1.
    let mut value = SourceElevator::new(
        Some(Arc::new(SourceTransportAnimation {
            total_time: 1000,
            nodes: BTreeMap::from([(100, record(1, 100, [0; 3]))]),
        })),
        SourcePose([0; 4]),
    );
    let q = [0, 0, 0, 1.0_f32.to_bits()];
    assert_eq!(value.position(100, q, 0).unwrap(), None);
    assert_eq!(value.position(101, q, 0).unwrap(), None);
    assert_eq!(value.current_segment, 0);
}

#[test]
fn zero_total_time_is_typed_undefined_and_one_is_defined() {
    // Unsigned modulo zero is undefined before node selection; Transports.cpp:459 at 8ec338a1.
    for total in [0, 1] {
        let mut f = elevator_fixture(true, false);
        f.factory
            .objects
            .get_mut(f.object)
            .unwrap()
            .elevator
            .as_mut()
            .unwrap()
            .animation = Some(Arc::new(SourceTransportAnimation {
            total_time: total,
            nodes: BTreeMap::from([(total, record(1, total, [0; 3]))]),
        }));
        let result = f.apply(|o, h, c| update_source_gameobject(o, h, 1, c));
        if total == 0 {
            assert_eq!(
                result.unwrap_err().id,
                "execution.elevator.period_modulo_zero_undefined"
            );
            assert!(f.go().lifecycle_stopped);
        } else {
            result.unwrap();
            assert_eq!(state(&f).progress, 0);
        }
    }
}

#[test]
fn same_time_animation_overwrite_uses_effective_dbc_id_order() {
    // Effective ID iteration, not physical ordinal, overwrites equal times; TransportMgr.cpp:28-32,305-311 at 8ec338a1.
    let mut f = elevator_fixture(true, false);
    let mut rows = records();
    let mut loser = record(5, 700, [0; 3]);
    loser.ordinal = 100;
    let mut winner = record(6, 700, oracle(0)[3..6].try_into().unwrap());
    winner.ordinal = 1;
    rows.extend([winner, loser]);
    f.factory.animations = SourceTransportTemplates::default();
    f.factory
        .animations
        .load_animations(rows, &mut |_| Ok(()))
        .unwrap();
    let animation = f.factory.animations.animation(100).unwrap().unwrap();
    assert_eq!(animation.nodes[&700].id, 6);
    f.factory
        .objects
        .get_mut(f.object)
        .unwrap()
        .elevator
        .as_mut()
        .unwrap()
        .animation = Some(Arc::new(animation.clone()));
    f.factory.life().clock_ms = 333;
    f.update();
    assert_eq!(f.world.objects.pose(f.object).unwrap(), expected(2));
}

#[test]
fn generic_constructor_callbacks_precede_animation_and_world_registration() {
    // Generic Create finishes callbacks before the virtual animation lookup and Map::Add; Transports.cpp:444-449, GameObject.cpp:230-300 at 8ec338a1.
    let mut f = elevator_fixture(true, false);
    let actions = &f.factory.actions;
    let model = actions
        .iter()
        .position(|a| *a == Action::Model(f.object, Op::ConstructAndInsert));
    // The lifecycle model adapter records model history separately from constructor actions.
    assert_eq!(model, None);
    let visibility = actions
        .iter()
        .position(|a| *a == Action::Callback(f.object, GameObjectCallback::LargeVisibility))
        .unwrap();
    let animation = actions
        .iter()
        .position(|a| *a == Action::Animation(100))
        .unwrap();
    let viewpoint = actions
        .iter()
        .position(|a| *a == Action::Callback(f.object, GameObjectCallback::MapViewpoint))
        .unwrap();
    assert!(visibility < animation && animation < viewpoint);
    assert_eq!(
        &f.factory.life().tree[..3],
        [
            Tree::Construct(f.object, 1, 123),
            Tree::Insert(f.object, 1),
            Tree::Insert(f.object, 1)
        ]
    );
    let value = f.world.objects.get(f.object).unwrap();
    assert!(value.in_world);
    assert_eq!(value.kind, SourceObjectKind::GameObject);
    assert_eq!(value.guid.unwrap().high(), HighGuid::GameObject as u16);
    assert_eq!(f.go().flags & 0x28, 0x28);
    assert_eq!((state(&f).progress, state(&f).current_segment), (0, 0));
}
#[test]
fn model_constructor_refuses_after_guid_rotation_and_transport_flags_before_animation() {
    // SetDisplayId reaches F6 before initial GO state and derived animation; GameObject.cpp:223-232, Transports.cpp:444-449 at 8ec338a1.
    let (mut world, mut factory) = (World::new(), Factory::new());
    factory.templates.get_mut(&100).unwrap().kind = 11;
    factory.collision_available = false;
    assert_eq!(
        world.load(&mut factory, spawn()).unwrap_err().id,
        "execution.dynamic_collision.gameobject_world_geometry_owner"
    );
    let h = ObjectHandle {
        thread: 1,
        construction: 1,
    };
    let go = factory.objects.get(h).unwrap();
    assert_eq!(go.flags & 0x28, 0x28);
    assert_eq!(go.rotation_bits, spawn().rotation_bits);
    assert_eq!((go.animation, go.state), (0, 0));
    assert!(go.elevator.is_none());
    assert_eq!(world.objects.get(h).unwrap().guid.unwrap().counter(), 7);
    assert!(!world.objects.get(h).unwrap().in_world);
    assert!(factory.actions.is_empty());
    assert_eq!(factory.cursor.raw, 0);
}

#[test]
fn update_set_dispatch_enters_virtual_elevator_not_generic_go_lifecycle() {
    // Object updater collection and final dispatch call the virtual GO Update; Map.cpp:995-1000, Transports.cpp:454 at 8ec338a1.
    let mut f = elevator_fixture(true, false);
    f.factory.life().clock_ms = 333;
    let mut set = SourceUpdateSet::new(map()).unwrap();
    set.insert(&f.world.objects, f.object, UpdateInsertion::Generic)
        .unwrap();
    set.dispatch(
        9999,
        &mut f.world.objects,
        &mut f.world.cells,
        &mut GameObjectLifecycleDispatch {
            definitions: &mut f.world.definitions,
            products: &mut f.world.products,
            sets: &mut f.world.sets,
            persistent: &mut f.persistent,
            spawns: &mut f.spawns,
            owner: &mut f.factory,
        },
        &mut |_| Ok(()),
    )
    .unwrap();
    assert_eq!(f.world.objects.pose(f.object).unwrap(), expected(2));
    assert!(!f
        .factory
        .life()
        .events
        .iter()
        .any(|e| matches!(e, Event::Phase(..))));
}

struct Boarding;
impl MoTransportOwner for Boarding {
    fn transport_template(&mut self, _: u32) -> Result<Option<GameObjectTemplate>> {
        panic!("boarding does not create MO transports")
    }
    fn transport_allocation(&mut self, _: ObjectHandle, _: &mut SourceObjects) -> Result<()> {
        panic!("boarding does not allocate")
    }
}
fn passenger(
    f: &mut Fixture,
    kind: SourceObjectKind,
    in_world: bool,
    mismatch: bool,
) -> ObjectHandle {
    let h = f.world.objects.construct(1, kind).unwrap();
    f.world
        .objects
        .bind_source_allocation(h, 0x1000, 64, 0)
        .unwrap();
    f.world.objects.bind_map(h, map(), &f.world.cells).unwrap();
    f.world.objects.set_pose(h, SourcePose([0; 4])).unwrap();
    let transport = PassengerTransport {
        handle: f.object,
        guid: f.world.objects.get(f.object).unwrap().guid.unwrap().raw(),
        progress: 0,
        pose: f.world.objects.pose(f.object).unwrap(),
        map: Some(map()),
    };
    f.factory
        .passengers
        .bind_unit(
            h,
            PassengerMovement {
                transport: None,
                guid: 0,
                time: 77,
                offset: SourcePose([0; 4]),
                on_transport: false,
                movement_pose: SourcePose([0; 4]),
                ghost: false,
                pet: None,
                mini_pet: None,
            },
            &f.world.objects,
        )
        .unwrap();
    f.factory
        .passengers
        .add(transport, h, false, &mut f.world.objects, &mut Boarding)
        .unwrap();
    let value = f.world.objects.get_mut(h).unwrap();
    value.in_world = in_world;
    if mismatch {
        value.map = Some(MapIdentity { map: 1, ..map() });
    }
    h
}

#[test]
fn passenger_map_mismatch_skips_before_relocation_and_out_of_world_passenger_moves() {
    // Map mismatch skips only in-world passengers; Transports.cpp:506-508,534-549 at 8ec338a1.
    for in_world in [false, true] {
        let mut f = elevator_fixture(true, false);
        let h = passenger(&mut f, SourceObjectKind::Player, in_world, true);
        f.factory.life().clock_ms = 333;
        f.update();
        let movement = f.factory.passengers.movement(h).unwrap();
        assert_eq!(movement.time, if in_world { 0 } else { 333 });
        assert_eq!(
            f.world.objects.pose(h).unwrap(),
            if in_world {
                SourcePose([0; 4])
            } else {
                expected(2)
            }
        );
        assert!(!f
            .factory
            .life()
            .events
            .iter()
            .any(|e| matches!(e, Event::Passenger(..))));
    }
}

#[test]
fn relocation_model_refusal_retains_earlier_pose_and_progress_without_moving_passengers() {
    // GameObjectRelocation changes pose before UpdateModelPosition and passenger iteration; Map.cpp:1214-1216, Transports.cpp:483-486 at 8ec338a1.
    let mut f = elevator_fixture(true, false);
    let h = passenger(&mut f, SourceObjectKind::Creature, false, false);
    f.factory.life().clock_ms = 333;
    f.factory.life().refuse_model = Some(Op::Relocate);
    assert_eq!(
        f.apply(|o, h, c| update_source_gameobject(o, h, 100, c))
            .unwrap_err()
            .id,
        "execution.dynamic_collision.gameobject_world_geometry_owner"
    );
    assert_eq!(state(&f).progress, 333);
    assert_eq!(f.world.objects.pose(f.object).unwrap(), expected(2));
    assert_eq!(f.world.objects.pose(h).unwrap(), SourcePose([0; 4]));
    assert_eq!(f.factory.passengers.movement(h).unwrap().time, 0);
    assert_eq!(
        f.factory.life().events,
        [
            Event::ElevatorSnapshot(map()),
            Event::Model(f.object, Op::Contains),
            Event::Model(f.object, Op::Remove),
            Event::Model(f.object, Op::Relocate)
        ]
    );
}

#[test]
fn f5_player_refusal_occurs_after_model_relocation_and_visibility() {
    // PlayerRelocation is reached after the elevator's completed relocation; Transports.cpp:483-486,540-549 at 8ec338a1.
    let mut f = elevator_fixture(true, false);
    let player = passenger(&mut f, SourceObjectKind::Player, true, false);
    f.factory.life().clock_ms = 333;
    assert_eq!(
        f.apply(|o, h, c| update_source_gameobject(o, h, 100, c))
            .unwrap_err()
            .id,
        "execution.transport.f5_player_relocation_owner"
    );
    assert_eq!(
        f.factory.life().events,
        [
            Event::ElevatorSnapshot(map()),
            Event::Model(f.object, Op::Contains),
            Event::Model(f.object, Op::Remove),
            Event::Model(f.object, Op::Relocate),
            Event::Model(f.object, Op::Insert),
            Event::Relocation(RelocationPhase::GameObjectVisibility),
            Event::Passenger(PassengerEffect::PlayerRelocate {
                player,
                pose: expected(2)
            })
        ]
    );
    assert_eq!(f.factory.passengers.members(f.object).count(), 1);
    assert_eq!(f.factory.passengers.movement(player).unwrap().time, 0);
}

#[test]
fn attached_in_world_creature_uses_actual_f3_relocation_after_model() {
    // In-world creatures relocate through Map, then update t_time; Transports.cpp:532-538 at 8ec338a1.
    let mut f = elevator_fixture(true, false);
    let creature = passenger(&mut f, SourceObjectKind::Creature, false, false);
    let origin = state(&f).stationary;
    f.world.objects.set_pose(creature, origin).unwrap();
    f.world
        .objects
        .assign_guid(
            creature,
            99,
            SourceGuid::with_entry(HighGuid::Creature, 1, 99),
        )
        .unwrap();
    let cell = f.world.cells.cell(origin.cell().unwrap()).unwrap();
    f.world
        .objects
        .link_cell(creature, cell, &mut f.world.cells)
        .unwrap();
    f.world.objects.register_world_lookup(creature).unwrap();
    f.world.objects.mark_in_world(creature).unwrap();
    f.factory.life().clock_ms = 333;
    f.update();
    assert_eq!(f.world.objects.pose(creature).unwrap(), expected(2));
    assert_eq!(f.factory.passengers.movement(creature).unwrap().time, 333);
    let events = &f.factory.life().events;
    let model = events
        .iter()
        .position(|e| *e == Event::Model(f.object, Op::Insert))
        .unwrap();
    let passenger = events
        .iter()
        .position(|e| {
            matches!(
                e,
                Event::Passenger(PassengerEffect::CreatureRelocate { .. })
            )
        })
        .unwrap();
    assert!(model < passenger);
    assert_eq!(
        f.world.cells.registered_cell(creature),
        Some(
            f.world
                .objects
                .pose(creature)
                .unwrap()
                .cell()
                .and_then(|pair| f.world.cells.cell(pair))
                .unwrap()
        )
    );
}

#[test]
fn retained_elevator_constructor_and_mt_projections_or_explicit_absence() {
    for (variable, hash) in [
        (
            "BENILLA_F1_SEAL8_FRAMING",
            "097b87217694b01c5a42c114358a7f30b2faf2062095640c0e50a4028e739cfa",
        ),
        (
            "BENILLA_F1_TRANSPORT_FRAMING",
            "cafc1fdd139046836d248f7799e72bcb7708d0fe645d451960945a62fe4214a7",
        ),
    ] {
        let Some(root) = std::env::var_os(variable) else {
            println!("SKIP unsealed P0/MT elevator projection: set {variable}");
            continue;
        };
        let path = std::path::PathBuf::from(root).join("source-sites.json");
        let (held, raw) = metadata(&path, 1_048_576).unwrap();
        assert_eq!(Sha256Id::of(&raw).as_str(), hash);
        let sites: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        let elevator_sites = sites
            .as_object()
            .unwrap()
            .values()
            .filter(|site| {
                site["callsite"]["function"]
                    .as_str()
                    .unwrap_or("")
                    .contains("ElevatorTransport::")
            })
            .count();
        held.check().unwrap();
        assert_eq!(elevator_sites, 0);
        println!("SKIP unsealed P0 elevator constructor and MT animation comparison: metadata={hash}, direct elevator sites={elevator_sites}, independently matched constructor/animation records=0");
    }
}

#[test]
fn retained_raw_stock_pre_model_operands_reach_f6_for_every_elevator_spawn() {
    // SetDisplayId is before respawn/animation/world work; GameObject.cpp:230,935-963, Transports.cpp:444-449 at 8ec338a1.
    let Some(path) = std::env::var_os("BENILLA_F4_ELEVATOR_STOCK") else {
        println!("SKIP raw elevator constructor reach: set BENILLA_F4_ELEVATOR_STOCK to the checked stock audit output");
        return;
    };
    let (held, raw) = metadata(Path::new(&path), 2_097_152).unwrap();
    let audit: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(
        audit["catalog_sha256"],
        "757dcfb85e2b2a0de6571b0fe981c214fa57a4e2e546968ca61171113f379bd6"
    );
    assert_eq!(
        audit["animation_dbc_sha256"],
        "4b47398842d741991892fcb9dd732f7811f22fd15571ac1993a025ed87f411df"
    );
    assert_eq!(
        audit["source_snapshot"],
        "phase0-sequential-diagnostic-not-F1"
    );
    assert_eq!(
        (
            audit["templates"].as_u64(),
            audit["templates_with_animation"].as_u64()
        ),
        (Some(35), Some(27))
    );
    assert_eq!(audit["zero_time_template_entries"], serde_json::json!([]));
    let n = |value: &serde_json::Value, field: &str| -> u32 {
        value[field].as_u64().unwrap().try_into().unwrap()
    };
    let mut by_map = BTreeMap::<u32, usize>::new();
    for row in audit["rows"].as_array().unwrap() {
        let identity = MapIdentity {
            map: n(row, "map"),
            ..map()
        };
        let mut world = World::new();
        world.cells = SourceCells::new(identity).unwrap();
        world.cells.configure_grids(300_000, true).unwrap();
        world.sets = GridSpawnSets::new(identity);
        let mut factory = Factory::new();
        factory.collision_available = false;
        let t = &row["template"];
        let entry = n(row, "entry");
        factory.templates.insert(
            entry,
            GameObjectTemplate {
                entry,
                kind: n(t, "kind"),
                display: n(t, "display"),
                faction: n(t, "faction"),
                flags: n(t, "flags"),
                extra_flags: n(t, "extra_flags"),
                scale_bits: n(t, "scale_bits"),
                data: serde_json::from_value(t["data"].clone()).unwrap(),
                string_id: 0,
            },
        );
        // Only raw operands consumed before the model call are supplied; this is not an effective ObjectMgr fixture.
        let input = GameObjectSpawn {
            database_guid: n(row, "guid"),
            entry,
            position_bits: serde_json::from_value(row["position_bits"].clone()).unwrap(),
            rotation_bits: serde_json::from_value(row["rotation_bits"].clone()).unwrap(),
            ..spawn()
        };
        let guid = input.database_guid;
        assert_eq!(
            world.load(&mut factory, input).unwrap_err().id,
            "execution.dynamic_collision.gameobject_world_geometry_owner",
            "map {} guid {guid}",
            identity.map
        );
        let object = ObjectHandle {
            thread: 1,
            construction: 1,
        };
        let go = factory.objects.get(object).unwrap();
        assert_eq!(go.flags & 0x28, 0x28);
        assert_eq!((go.animation, go.state), (0, 0));
        assert!(go.elevator.is_none());
        assert!(!world.objects.get(object).unwrap().in_world);
        assert!(factory.actions.is_empty());
        assert_eq!(factory.cursor.raw, 0);
        assert_eq!(row["animation"], true);
        *by_map.entry(identity.map).or_default() += 1;
    }
    held.check().unwrap();
    assert_eq!(
        by_map,
        BTreeMap::from([
            (0, 14),
            (1, 10),
            (47, 2),
            (90, 1),
            (209, 2),
            (369, 6),
            (469, 2)
        ])
    );
    println!("Unsealed raw pre-model elevator fixture: 35 templates, 27 animated, 37 spawns all animated; F6 constructor refusals by map {by_map:?}, zero period-undefined candidates; no effective ObjectMgr or sealed execution acceptance");
    let Some(root) = std::env::var_os("BENILLA_F1_SEAL8_FRAMING") else {
        println!("SKIP unsealed P0 elevator-entry GUID projection: set BENILLA_F1_SEAL8_FRAMING");
        return;
    };
    let entries: BTreeSet<u32> = audit["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| n(row, "entry"))
        .collect();
    let mut candidates = BTreeMap::<u64, SourceGuid>::new();
    let (mut adds, mut updates) = (0, 0);
    let records = retained_seal8::scan_seal8(Path::new(&root), |context, kind, payload| {
        let mut fields = payload.split_whitespace();
        match kind {
            "object_identity" => {
                let _pointer = fields.next();
                let _database: u32 = retained_seal8::parse(fields.next())?;
                let low: u32 = retained_seal8::parse(fields.next())?;
                let entry: u32 = retained_seal8::parse(fields.next())?;
                let high: u32 = retained_seal8::parse(fields.next())?;
                let generation: u64 = retained_seal8::parse(fields.next())?;
                if high == HighGuid::GameObject as u32 && entries.contains(&entry) {
                    candidates.insert(
                        generation,
                        SourceGuid::with_entry(HighGuid::GameObject, entry, low),
                    );
                }
            }
            "object_add" => {
                let _pointer = fields.next();
                let _map: u32 = retained_seal8::parse(fields.next())?;
                let low: u32 = retained_seal8::parse(fields.next())?;
                let generation: u64 = retained_seal8::parse(fields.next())?;
                if let Some(guid) = candidates.get(&generation) {
                    assert_eq!(low, guid.counter());
                    adds += 1;
                }
            }
            "object_update" => {
                if let Some(guid) = candidates.get(&context.object_generation) {
                    assert_eq!(context.object_guid, guid.raw());
                    updates += 1;
                }
            }
            _ => {}
        }
        Ok(())
    })
    .unwrap();
    println!("Unsealed P0 elevator-entry identity projection: records={records}, candidate identities={}, add-counter matches={adds}, update-GUID matches={updates}; only identity arithmetic, not independently executed constructors", candidates.len());
    if candidates.is_empty() {
        println!("SKIP unsealed P0 reached elevator constructor comparison: no elevator-entry identity candidate in retained projection");
    }
}
