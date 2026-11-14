// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored owners exercise chronology only, not F6 geometry or sealed execution.
use super::*;
use benilla_sim::source_transports::{generate_template, TaxiNode};
use std::sync::Arc;
fn map(id: u32) -> MapIdentity {
    MapIdentity {
        map: id,
        instance: 0,
        creation_generation: u64::from(id) + 1,
    }
}
fn templates(entries: &[(u32, u32)]) -> SourceTransportTemplates {
    let mut registry = SourceTransportTemplates {
        templates_loaded: true,
        ..Default::default()
    };
    for &(entry, map) in entries {
        let definition = MoTransport {
            entry,
            path: 1,
            speed: 10,
            acceleration: 2,
        };
        let nodes = (0..7)
            .map(|i| TaxiNode {
                id: i + 1,
                path: 1,
                index: i,
                map,
                position: [(i as f32 * 10.0).to_bits(), 0, 0],
                flags: if i == 1 || i == 5 { 2 } else { 0 },
                delay: 2,
            })
            .collect::<Vec<_>>();
        let path =
            generate_template(definition, Arc::from(nodes), &BTreeMap::from([(map, 0)])).unwrap();
        registry.templates.insert(entry, path);
        registry.by_map.entry(map).or_default().push(entry);
    }
    registry
}
struct AuthoredOwner {
    next: u64,
    model: bool,
    contained: bool,
    fail: Option<GameObjectModelOperation>,
    operations: Vec<(ObjectHandle, GameObjectModelOperation, SourcePose)>,
}
impl Default for AuthoredOwner {
    fn default() -> Self {
        Self {
            next: 0x100000,
            model: true,
            contained: true,
            fail: None,
            operations: Vec::new(),
        }
    }
}
fn definition(entry: u32) -> GameObjectTemplate {
    let mut data = [0; 24];
    data[..3].copy_from_slice(&[1, 10, 2]);
    GameObjectTemplate {
        entry,
        kind: 15,
        display: 77,
        faction: 35,
        flags: 8,
        extra_flags: 0,
        scale_bits: 1.0_f32.to_bits(),
        data,
        string_id: 0,
    }
}
impl MoTransportOwner for AuthoredOwner {
    fn transport_template(&mut self, entry: u32) -> Result<Option<GameObjectTemplate>> {
        Ok(Some(definition(entry)))
    }
    fn transport_allocation(&mut self, h: ObjectHandle, o: &mut SourceObjects) -> Result<()> {
        self.next -= 256;
        o.bind_source_allocation(h, self.next, 128, 0)
    }
    fn transport_model(
        &mut self,
        t: &SourceMoTransport,
        operation: GameObjectModelOperation,
    ) -> Result<bool> {
        self.operations.push((t.handle, operation, t.position));
        if self.fail == Some(operation) {
            return Err(refuse(
                "execution.transport.f6_dynamic_model_relocate_owner",
            ));
        }
        match operation {
            GameObjectModelOperation::ConstructAndInsert => {
                assert_eq!((t.state, t.kind, t.animation), (0, 0, 0));
                Ok(self.model)
            }
            GameObjectModelOperation::Contains => Ok(self.contained),
            _ => Ok(true),
        }
    }
}
fn initialize(
    t: &SourceTransportTemplates,
    m: &mut SourceMoTransports,
    c: &mut SourceCells,
    o: &mut SourceObjects,
    w: &mut impl MoTransportOwner,
    snapshot: u32,
) {
    m.initialize_map(
        t,
        &mut MoTransportContext {
            cells: c,
            objects: o,
            owner: w,
        },
        1,
        snapshot,
    )
    .unwrap();
}
#[test]
fn startup_uses_first_node_entry_guid_and_no_world_or_grid_registration() {
    // Transports.cpp:131-147,161-192 creates before Map player entry and does not AddToWorld.
    let t = templates(&[(42, 0)]);
    let mut m = SourceMoTransports::default();
    let mut c = SourceCells::new(map(0)).unwrap();
    let mut o = SourceObjects::default();
    let mut w = AuthoredOwner::default();
    initialize(&t, &mut m, &mut c, &mut o, &mut w, 123);
    let h = m.members(map(0)).next().unwrap();
    let go = m.get(h).unwrap();
    assert_eq!(
        go.position,
        SourcePose([
            10.0_f32.to_bits(),
            0,
            0,
            t.templates[&42].key_frames[0].orientation
        ])
    );
    assert_eq!(go.motion.progress(), 123);
    assert_eq!(go.motion.current_frame(), 0);
    assert_eq!(go.motion.next_frame(), 1);
    assert_eq!((go.state, go.kind, go.animation), (1, 15, 100));
    assert_eq!(go.template.faction, 35);
    assert_eq!(go.display, 77);
    let object = o.get(h).unwrap();
    assert_eq!(object.guid.unwrap().raw(), 0x1fc000000000002a);
    assert_eq!(object.database_guid, 42);
    assert!(!object.in_world);
    assert!(object.current_cell.is_none());
    let player = o.construct(1, SourceObjectKind::Player).unwrap();
    assert!(player.construction > h.construction);
}
#[test]
fn transport_set_uses_allocation_order_not_sql_guid_or_construction_order() {
    // Map.cpp:119-121,725-729 traverses std::set<Transport*>.
    let t = templates(&[(9, 0), (42, 0)]);
    let mut m = SourceMoTransports::default();
    let mut c = SourceCells::new(map(0)).unwrap();
    let mut o = SourceObjects::default();
    let mut w = AuthoredOwner::default();
    initialize(&t, &mut m, &mut c, &mut o, &mut w, 0);
    let members = m.members(map(0)).collect::<Vec<_>>();
    assert_eq!(o.get(members[0]).unwrap().guid.unwrap().counter(), 42);
    assert_eq!(o.get(members[1]).unwrap().guid.unwrap().counter(), 9);
    w.operations.clear();
    m.update_map(&t, map(0), 2001, 0, &mut o, &mut w).unwrap();
    assert_eq!(w.operations[0].0, members[0]);
    assert_eq!(w.operations[4].0, members[1]);
}
#[test]
fn model_transform_history_uses_new_pose_and_exact_operation_order() {
    // Transports.cpp:490-495 relocates before GameObject.cpp:2134-2138 model operations.
    let t = templates(&[(42, 0)]);
    let mut m = SourceMoTransports::default();
    let mut c = SourceCells::new(map(0)).unwrap();
    let mut o = SourceObjects::default();
    let mut w = AuthoredOwner::default();
    initialize(&t, &mut m, &mut c, &mut o, &mut w, 0);
    let h = m.members(map(0)).next().unwrap();
    let first = m.get(h).unwrap().position;
    w.operations.clear();
    m.update_map(&t, map(0), 2100, u32::MAX, &mut o, &mut w)
        .unwrap();
    assert_eq!(
        w.operations.iter().map(|x| x.1).collect::<Vec<_>>(),
        [
            GameObjectModelOperation::Contains,
            GameObjectModelOperation::Remove,
            GameObjectModelOperation::Relocate,
            GameObjectModelOperation::Insert
        ]
    );
    let now = m.get(h).unwrap().position;
    assert_ne!(first, now);
    assert_eq!(o.pose(h).unwrap(), now);
    assert!(w.operations.iter().all(|x| x.2 == now));
    let count = w.operations.len();
    m.update_map(&t, map(0), 2100, 50, &mut o, &mut w).unwrap();
    assert_eq!(w.operations.len(), count);
}
#[test]
fn missing_model_and_model_not_in_tree_skip_only_source_guarded_operations() {
    // GameObject.cpp:2131-2134 guards null and absent-tree models.
    for model in [false, true] {
        let t = templates(&[(42, 0)]);
        let mut m = SourceMoTransports::default();
        let mut c = SourceCells::new(map(0)).unwrap();
        let mut o = SourceObjects::default();
        let mut w = AuthoredOwner {
            model,
            contained: false,
            ..Default::default()
        };
        initialize(&t, &mut m, &mut c, &mut o, &mut w, 0);
        w.operations.clear();
        m.update_map(&t, map(0), 2100, 0, &mut o, &mut w).unwrap();
        assert_eq!(w.operations.len(), usize::from(model));
    }
}
#[test]
fn unbound_geometry_prerequisite_refuses_before_model_and_membership() {
    struct Missing {
        geometry_calls: usize,
        models: SourceDynamicModels,
        dynamic: SourceDynamicMap,
        vmap: SourceVMapManager,
    }
    impl MoTransportOwner for Missing {
        fn transport_template(&mut self, e: u32) -> Result<Option<GameObjectTemplate>> {
            Ok(Some(definition(e)))
        }
        fn transport_allocation(&mut self, h: ObjectHandle, o: &mut SourceObjects) -> Result<()> {
            o.bind_source_allocation(h, 0x1000, 128, 0)
        }
        fn transport_geometry(
            &mut self,
            _: ObjectHandle,
        ) -> Result<(
            &mut SourceDynamicModels,
            &mut SourceDynamicMap,
            &mut SourceVMapManager,
        )> {
            self.geometry_calls += 1;
            Err(refuse(
                "execution.dynamic_collision.transport_world_geometry_owner",
            ))
        }
    }
    let t = templates(&[(42, 0)]);
    let mut m = SourceMoTransports::default();
    let mut c = SourceCells::new(map(0)).unwrap();
    let mut o = SourceObjects::default();
    let (models, dynamic, vmap) = dynamic_models::tests::fixture();
    let mut owner = Missing {
        geometry_calls: 0,
        models,
        dynamic,
        vmap,
    };
    let e = m
        .initialize_map(
            &t,
            &mut MoTransportContext {
                cells: &mut c,
                objects: &mut o,
                owner: &mut owner,
            },
            1,
            0,
        )
        .unwrap_err();
    assert_eq!(
        e.id,
        "execution.dynamic_collision.transport_world_geometry_owner"
    );
    // Transports.cpp:138,177 binds and uses the map before SetDisplayId at :186.
    assert_eq!(owner.geometry_calls, 1);
    assert!(owner.models.models.inventory().is_empty());
    assert!(owner.models.models.construction_inventory().is_empty());
    assert!(owner.vmap.collision.model_inventory().is_empty());
    assert_eq!(owner.dynamic.tree.size(), 0);
    assert_eq!(m.members(map(0)).count(), 0);
    assert_eq!(
        m.update_map(&t, map(0), 100, 1, &mut o, &mut owner)
            .unwrap_err()
            .id,
        "execution.transport.update_state"
    );
}
#[test]
fn absent_f6_relocation_stops_before_remove_or_insert() {
    let t = templates(&[(42, 0)]);
    let mut m = SourceMoTransports::default();
    let mut c = SourceCells::new(map(0)).unwrap();
    let mut o = SourceObjects::default();
    let mut w = AuthoredOwner::default();
    initialize(&t, &mut m, &mut c, &mut o, &mut w, 0);
    w.operations.clear();
    w.fail = Some(GameObjectModelOperation::Contains);
    assert_eq!(
        m.update_map(&t, map(0), 2100, 0, &mut o, &mut w)
            .unwrap_err()
            .id,
        "execution.transport.f6_dynamic_model_relocate_owner"
    );
    assert_eq!(w.operations.len(), 1);
}
#[test]
fn teleport_refuses_at_model_after_source_relocation() {
    // Transports.cpp:209,268 relocates before the reached UpdateModelPosition owner.
    let t = templates(&[(42, 0)]);
    let mut m = SourceMoTransports::default();
    let mut c = SourceCells::new(map(0)).unwrap();
    let mut o = SourceObjects::default();
    let mut w = AuthoredOwner::default();
    initialize(&t, &mut m, &mut c, &mut o, &mut w, 0);
    let h = m.members(map(0)).next().unwrap();
    let orientation = o.pose(h).unwrap().0[3];
    w.operations.clear();
    w.fail = Some(GameObjectModelOperation::Contains);
    let clock = t.templates[&42].key_frames.last().unwrap().arrive_time;
    assert_eq!(
        m.update_map(&t, map(0), clock, 0, &mut o, &mut w)
            .unwrap_err()
            .id,
        "execution.transport.f6_dynamic_model_relocate_owner"
    );
    assert_eq!(o.pose(h).unwrap().0[3], orientation);
    assert_eq!(w.operations.len(), 1);
}
#[test]
fn two_maps_share_the_calling_stream_without_transport_rng() {
    // Transports.cpp:349-440 has no RNG caller or transport-owned stream.
    let t = templates(&[(42, 0), (43, 1)]);
    let mut m = SourceMoTransports::default();
    let mut o = SourceObjects::default();
    let mut w = AuthoredOwner::default();
    for id in [0, 1] {
        let mut c = SourceCells::new(map(id)).unwrap();
        initialize(&t, &mut m, &mut c, &mut o, &mut w, 0);
    }
    let mut rng = benilla_sim::source_rng::Mt19937::from_seed(123);
    let mut expected = rng.clone();
    m.update_map(&t, map(1), 2100, 17, &mut o, &mut w).unwrap();
    assert_eq!(rng.next_u32(), expected.next_u32());
    m.update_map(&t, map(0), 2100, 91, &mut o, &mut w).unwrap();
    assert_eq!(rng.next_u32(), expected.next_u32());
    let poses = [0, 1].map(|id| m.get(m.members(map(id)).next().unwrap()).unwrap().position);
    assert_eq!(poses[0], poses[1]);
}
#[test]
fn retained_stock_creation_reaches_missing_f6_for_each_transport_or_skips() {
    let (Ok(root), Ok(occurrences)) = (
        std::env::var("BENILLA_F2_CATALOG"),
        std::env::var("BENILLA_F2_DBC_OCCURRENCES"),
    ) else {
        println!("SKIP raw stock MO creation reach: set BENILLA_F2_CATALOG and BENILLA_F2_DBC_OCCURRENCES");
        return;
    };
    let mut catalog = VerifiedCatalog::open(Path::new(&root), false).unwrap();
    // The fixed raw fixture's schema and all column offsets are bound by catalog-04, not production decoding.
    assert_eq!(
        catalog.report()["catalog_sha256"],
        "757dcfb85e2b2a0de6571b0fe981c214fa57a4e2e546968ca61171113f379bd6"
    );
    let inputs = catalog
        .execution_transport_inputs(Path::new(&occurrences))
        .unwrap();
    let mut definitions = BTreeMap::new();
    catalog
        .scan("gameobject_template", 0, 256, |row| {
            let cells = row.cells().collect::<std::result::Result<Vec<_>, _>>()?;
            let text = |i: usize| std::str::from_utf8(cells[i].unwrap()).unwrap();
            let uint = |i: usize| text(i).parse::<u32>().unwrap();
            if uint(1) == 15 {
                assert_eq!(cells.len(), 38);
                definitions.insert(
                    uint(0),
                    GameObjectTemplate {
                        entry: uint(0),
                        kind: uint(1),
                        display: uint(2),
                        faction: uint(5),
                        flags: uint(6),
                        extra_flags: uint(7),
                        scale_bits: text(8).parse::<f32>().unwrap().to_bits(),
                        data: std::array::from_fn(|i| uint(9 + i)),
                        string_id: uint(36),
                    },
                );
            }
            Ok(())
        })
        .unwrap();
    struct RawDefinitions(Arc<BTreeMap<u32, TransportGameObject>>);
    impl TransportDefinitionOwner for RawDefinitions {
        fn gameobject_definitions(&mut self) -> Result<Arc<BTreeMap<u32, TransportGameObject>>> {
            Ok(Arc::clone(&self.0))
        }
    }
    let mut paths = SourceTransportTemplates::default();
    paths
        .load_templates_from_catalog(
            &mut catalog,
            Path::new(&occurrences),
            &mut RawDefinitions(Arc::new(inputs.raw_gameobjects)),
            &mut |_| Ok(()),
        )
        .unwrap();
    struct MissingF6(GameObjectTemplate);
    impl MoTransportOwner for MissingF6 {
        fn transport_template(&mut self, _: u32) -> Result<Option<GameObjectTemplate>> {
            Ok(Some(self.0.clone()))
        }
        fn transport_allocation(&mut self, h: ObjectHandle, o: &mut SourceObjects) -> Result<()> {
            o.bind_source_allocation(h, 0x1000, 128, 0)
        }
    }
    assert_eq!(definitions.len(), 9);
    for (&entry, definition) in &definitions {
        let path = &paths.templates[&entry];
        assert!(path.path_time > 0);
        assert!(path.key_frames.len() > 1);
        let source_map = path.nodes[path.key_frames[0].node].map;
        let one = SourceTransportTemplates {
            templates_loaded: true,
            templates: BTreeMap::from([(entry, path.clone())]),
            by_map: BTreeMap::from([(source_map, vec![entry])]),
            ..Default::default()
        };
        let mut manager = SourceMoTransports::default();
        let mut cells = SourceCells::new(map(source_map)).unwrap();
        let mut objects = SourceObjects::default();
        let mut owner = MissingF6(definition.clone());
        let error = manager
            .initialize_map(
                &one,
                &mut MoTransportContext {
                    cells: &mut cells,
                    objects: &mut objects,
                    owner: &mut owner,
                },
                1,
                0,
            )
            .unwrap_err();
        assert_eq!(
            error.id,
            "execution.dynamic_collision.transport_world_geometry_owner"
        );
        println!(
            "Raw stock transport {entry}: creation={}, movement=not reached, zero-period guard=0",
            error.id
        );
    }
    println!("Unsealed raw catalog component reach: nine F6 refusals, zero admitted creations or movement; no effective ObjectMgr or F1 acceptance");
}

#[path = "transports_passenger_tests.rs"]
mod passenger_tests;
