// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored source-manager tests, not sealed MC or normal-player lifecycle acceptance.
use super::*;
use benilla_sim::{
    corpse_appearance::EquippedItem,
    resident_map::{compute_cell_pair, CellPair},
};
use std::collections::BTreeMap;
fn map(id: u32, instance: u32) -> MapIdentity {
    MapIdentity {
        map: id,
        instance,
        creation_generation: 1,
    }
}
fn snapshot(corpses: &str, characters: &str, guild: &str) -> Vec<u8> {
    let specs = [
        (
            "corpse",
            &[
                "guid",
                "player",
                "position_x",
                "position_y",
                "position_z",
                "orientation",
                "map",
                "time",
                "corpse_type",
                "instance",
            ][..],
            corpses,
        ),
        (
            "characters",
            &[
                "guid",
                "gender",
                "race",
                "class",
                "playerBytes",
                "playerBytes2",
                "equipmentCache",
                "playerFlags",
            ][..],
            characters,
        ),
        ("guild_member", &["guid", "guildid"][..], guild),
    ];
    let mut out = String::new();
    for (name, columns, rows) in specs {
        out.push_str(&format!("CREATE TABLE `{name}` (\n"));
        for column in columns {
            out.push_str(&format!("  `{column}` int,\n"));
        }
        out.push_str(") ENGINE=InnoDB;\n");
        out.push_str(&format!("LOCK TABLES `{name}` WRITE;\n"));
        if !rows.is_empty() {
            out.push_str(&format!("INSERT INTO `{name}` VALUES {rows};\n"));
        }
        out.push_str("UNLOCK TABLES;\n");
    }
    out.into_bytes()
}
const CHARACTER: &str = "(1,1,1,8,67305985,5,'7 4294967295 8 0',3072)";
const CORPSE: &str = "(77,1,10,20,30,0.5,0,50,1,0)";
struct Work {
    clocks: usize,
    events: Vec<CorpseEvent>,
    status: CharacterQueryStatus,
    sql: Vec<CorpseSql>,
    fail_sql_at: Option<usize>,
}
impl Work {
    fn new(status: CharacterQueryStatus) -> Self {
        Self {
            clocks: 0,
            events: Vec::new(),
            status,
            sql: Vec::new(),
            fail_sql_at: None,
        }
    }
}
impl CorpseWork for Work {
    fn constructor_clock(
        &mut self,
        object: ObjectHandle,
        objects: &mut SourceObjects,
    ) -> Result<ClockSample> {
        assert!(objects.get(object)?.guid.is_none());
        self.clocks += 1;
        Ok(ClockSample::authored(
            ClockKind::WallSeconds,
            100 + self.clocks as i64,
        ))
    }
    fn observe(&mut self, event: CorpseEvent) -> Result<()> {
        self.events.push(event);
        Ok(())
    }
    fn query_corpses(&mut self) -> Result<CharacterQueryStatus> {
        Ok(self.status)
    }
    fn player_rank(&mut self, _: ObjectHandle) -> Result<u8> {
        Ok(4)
    }
    fn sql(&mut self, op: CorpseSql) -> Result<()> {
        self.sql.push(op);
        if self.fail_sql_at == Some(self.sql.len()) {
            return Err(refuse("authored.sql_owner"));
        }
        Ok(())
    }
}
#[derive(Default)]
struct Appearance {
    items: Vec<u32>,
}
impl CorpseAppearanceOwner for Appearance {
    fn player_displays(&mut self, race: u8, class: u8) -> Result<Option<[u32; 2]>> {
        Ok((race == 1 && class == 8).then_some([49, 50]))
    }
    fn item(&mut self, id: u32) -> Result<Option<EquippedItem>> {
        self.items.push(id);
        Ok((id == 7).then_some(EquippedItem {
            display_info_id: 0x123456,
            inventory_type: 5,
        }))
    }
}
#[derive(Default)]
struct NoOtherMaps;
impl CorpseGridOwner for NoOtherMaps {}
fn initial(c: &str, ch: &str, g: &str) -> (SourceCorpseRegistry, SourceObjects, Work, Appearance) {
    let inputs = CorpseInitialInputs::authored(&snapshot(c, ch, g), None).unwrap();
    let mut work = Work::new(if inputs.rows().next().is_some() {
        CharacterQueryStatus::Rows
    } else {
        CharacterQueryStatus::Empty
    });
    let mut registry = SourceCorpseRegistry::default();
    let mut objects = SourceObjects::default();
    let mut appearance = Appearance::default();
    registry
        .load_initial(&inputs, 1, &mut objects, &mut appearance, &mut work)
        .unwrap();
    (registry, objects, work, appearance)
}
fn handle(registry: &SourceCorpseRegistry, player: u32) -> ObjectHandle {
    registry
        .for_player(SourceGuid::without_entry(HighGuid::Player, player))
        .unwrap()
        .unwrap()
}
fn pair() -> CellPair {
    compute_cell_pair(10.0f32.to_bits(), 20.0f32.to_bits()).unwrap()
}
fn loaded_cells(id: MapIdentity, entry: CellPair) -> SourceCells {
    let mut cells = SourceCells::new(id).unwrap();
    cells.ensure_grid(entry.grid_x(), entry.grid_y()).unwrap();
    cells
}

#[test]
fn absent_relations_refuse_empty_and_join_missing_owner_construct_nothing() {
    assert_eq!(
        CorpseInitialInputs::authored(b"", None).err().unwrap().id,
        "execution.corpse.snapshot_truncated"
    );
    assert_eq!(
        CorpseInitialInputs::authored(b"-- absent\n", None)
            .err()
            .unwrap()
            .id,
        "execution.corpse.character_relation_absent"
    );
    for (corpses, characters) in [("", ""), (CORPSE, "")] {
        let (registry, objects, work, _) = initial(corpses, characters, "");
        assert_eq!(work.clocks, 0);
        assert!(registry.inventory(&objects).unwrap().corpses.is_empty());
    }
    let (registry, objects, work, _) = initial("(1,1,0,0,0,0,0,0,0,0)", CHARACTER, "");
    assert_eq!(work.clocks, 0);
    assert!(registry.inventory(&objects).unwrap().corpses.is_empty());
}
#[test]
fn invalid_type_race_and_coordinates_fail_only_after_constructor_time_read() {
    for corpse in [
        CORPSE.replace(",1,0)", ",3,0)"),
        CORPSE.replace("10,20,30", "20000,20,30"),
    ] {
        let inputs =
            CorpseInitialInputs::authored(&snapshot(&corpse, CHARACTER, ""), None).unwrap();
        let mut registry = SourceCorpseRegistry::default();
        let mut objects = SourceObjects::default();
        let mut work = Work::new(CharacterQueryStatus::Rows);
        assert_eq!(
            registry
                .load_initial(
                    &inputs,
                    1,
                    &mut objects,
                    &mut Appearance::default(),
                    &mut work
                )
                .unwrap(),
            0
        );
        assert_eq!(work.clocks, 1);
        assert!(objects
            .get(ObjectHandle {
                thread: 1,
                construction: 1
            })
            .is_err());
        assert_eq!(
            work.events,
            [
                CorpseEvent::Construct(ObjectHandle {
                    thread: 1,
                    construction: 1
                }),
                CorpseEvent::FailedLoad(ObjectHandle {
                    thread: 1,
                    construction: 1
                })
            ]
        );
    }
    let (_, objects, work, _) = initial(CORPSE, "(1,0,0,0,0,0,'',0)", "");
    assert_eq!(work.clocks, 1);
    assert!(objects
        .get(ObjectHandle {
            thread: 1,
            construction: 1
        })
        .is_err());
}
#[test]
fn duplicate_owner_hits_source_assertion_after_both_constructors_without_overwriting() {
    let corpse = format!("{CORPSE},{}", CORPSE.replace("77,1", "78,1"));
    let inputs = CorpseInitialInputs::authored(&snapshot(&corpse, CHARACTER, ""), None).unwrap();
    let mut registry = SourceCorpseRegistry::default();
    let mut objects = SourceObjects::default();
    let mut work = Work::new(CharacterQueryStatus::Rows);
    assert_eq!(
        registry
            .load_initial(
                &inputs,
                1,
                &mut objects,
                &mut Appearance::default(),
                &mut work
            )
            .unwrap_err()
            .id,
        "execution.corpse.duplicate_owner_assertion"
    );
    assert_eq!(work.clocks, 2);
    assert_eq!(handle(&registry, 1).construction, 1);
    let state = registry.inventory(&objects).unwrap();
    assert!(state.stopped);
    assert_eq!(state.owners_in_source_order.len(), 1);
    assert_eq!(state.corpses.len(), 2);
}
#[test]
fn loaded_guid_time_fields_and_appearance_match_source_without_publishing_or_loading_maps() {
    let (registry, objects, work, appearance) = initial(CORPSE, CHARACTER, "(1,9)");
    let object = handle(&registry, 1);
    let corpse = registry.corpse(object).unwrap();
    assert_eq!((corpse.created_seconds, corpse.rank_snapshot), (50, 0));
    assert_eq!(work.clocks, 1);
    assert_eq!(
        objects.get(object).unwrap().guid.unwrap().raw(),
        0xf10100000000004d
    );
    assert_eq!(objects.get(object).unwrap().database_guid, 77);
    assert!(objects.get(object).unwrap().map.is_none());
    assert!(!objects.get(object).unwrap().in_world);
    let fields = corpse.fields();
    assert_eq!(&fields[..8], &[77, 0xf1010000, 129, 0, 0x3f800000, 0, 1, 0]);
    // LoadFromDB relocates the WorldObject but leaves the separate corpse position fields zero; Corpse.cpp:193-241.
    assert_eq!(&fields[8..12], &[0; 4]);
    assert_eq!(fields[12], 50);
    assert_eq!(fields[13], 0x05123456);
    assert_eq!(&fields[14..32], &[0; 18]);
    assert_eq!(&fields[32..], &[0x01010100, 0x05040302, 9, 28, 0, 0]);
    assert_eq!(appearance.items.len(), 19);
    assert_eq!(&appearance.items[..3], &[7, 8, 0]);
    assert_eq!(
        registry.inventory(&objects).unwrap().cells,
        [(0, 524800, vec![(1, 0)])]
    );
}
#[test]
fn source_query_order_drives_owner_hashtable_and_cell_index_uses_player_keys() {
    let input = snapshot(
        "(77,1,0,0,0,0,0,1,1,0),(88,2,0,0,0,0,0,1,2,0)",
        &format!("{CHARACTER},(2,0,1,8,0,0,'',0)"),
        "",
    );
    let order = vec![
        CorpseJoinOccurrence {
            corpse: 1,
            character: 1,
            guild: None,
        },
        CorpseJoinOccurrence {
            corpse: 0,
            character: 0,
            guild: None,
        },
    ];
    let inputs = CorpseInitialInputs::authored(&input, Some(order.clone())).unwrap();
    let mut registry = SourceCorpseRegistry::default();
    let mut objects = SourceObjects::default();
    registry
        .load_initial(
            &inputs,
            1,
            &mut objects,
            &mut Appearance::default(),
            &mut Work::new(CharacterQueryStatus::Rows),
        )
        .unwrap();
    let state = registry.inventory(&objects).unwrap();
    assert_eq!(
        state
            .owners_in_source_order
            .iter()
            .map(|(guid, _)| guid.counter())
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(state.cells[0].2, [(1, 0), (2, 0)]);
    assert!(CorpseInitialInputs::authored(&input, Some(vec![order[0], order[0]])).is_err());
}
#[test]
fn per_cell_loader_filters_map_and_instance_but_global_grid_link_omits_map_id() {
    let (mut registry, mut objects, _, _) = initial(CORPSE, CHARACTER, "");
    let corpse = handle(&registry, 1);
    let entry = pair();
    let mut foreign = loaded_cells(map(1, 0), entry);
    let mut owner = NoOtherMaps;
    registry
        .load_cell(entry, &mut objects, &mut foreign, &mut owner)
        .unwrap();
    assert!(foreign.registered_cell(corpse).is_none());
    registry
        .add_corpses_to_grid(entry, false, &mut objects, &mut foreign, &mut owner)
        .unwrap();
    assert_eq!(
        foreign.registered_cell(corpse),
        Some(foreign.cell(entry).unwrap())
    );
    assert!(objects.get(corpse).unwrap().map.is_none());
    assert!(!objects.get(corpse).unwrap().in_world);
    let (mut registry, mut objects, _, _) = initial("(77,1,10,20,30,0,33,1,1,42)", CHARACTER, "");
    let corpse = handle(&registry, 1);
    let mut wrong_instance = loaded_cells(map(33, 41), entry);
    registry
        .load_cell(entry, &mut objects, &mut wrong_instance, &mut owner)
        .unwrap();
    registry
        .add_corpses_to_grid(entry, true, &mut objects, &mut wrong_instance, &mut owner)
        .unwrap();
    assert!(wrong_instance.registered_cell(corpse).is_none());
    let mut correct = loaded_cells(map(33, 42), entry);
    registry
        .load_cell(entry, &mut objects, &mut correct, &mut owner)
        .unwrap();
    assert!(objects.get(corpse).unwrap().in_world);
    assert_eq!(objects.get(corpse).unwrap().map, Some(map(33, 42)));
}
#[test]
fn global_grid_link_uses_entry_cell_not_actual_corpse_cell() {
    let (mut registry, mut objects, _, _) = initial(CORPSE, CHARACTER, "");
    let corpse = handle(&registry, 1);
    let actual = pair();
    let entry =
        CellPair::from_indices(actual.grid_x() * 16 + 15, actual.grid_y() * 16 + 15).unwrap();
    let mut cells = loaded_cells(map(0, 0), entry);
    registry
        .load_cell(actual, &mut objects, &mut cells, &mut NoOtherMaps)
        .unwrap();
    assert_eq!(
        cells.registered_cell(corpse),
        Some(cells.cell(actual).unwrap())
    );
    registry
        .add_corpses_to_grid(entry, false, &mut objects, &mut cells, &mut NoOtherMaps)
        .unwrap();
    assert_eq!(
        cells.registered_cell(corpse),
        Some(cells.cell(entry).unwrap())
    );
    assert_eq!(
        objects.pose(corpse).unwrap().0,
        [
            10.0f32.to_bits(),
            20.0f32.to_bits(),
            30.0f32.to_bits(),
            0.5f32.to_bits()
        ]
    );
}
#[test]
fn cross_map_reference_requires_actual_owner_and_concrete_owner_unlinks_previous_cells() {
    for supplied in [false, true] {
        let (mut registry, mut objects, _, _) = initial(CORPSE, CHARACTER, "");
        let corpse = handle(&registry, 1);
        let entry = pair();
        let mut first = loaded_cells(map(0, 0), entry);
        registry
            .load_cell(entry, &mut objects, &mut first, &mut NoOtherMaps)
            .unwrap();
        let mut second = loaded_cells(map(1, 0), entry);
        let mut maps = BTreeMap::from([(map(0, 0), first)]);
        if supplied {
            registry
                .add_corpses_to_grid(
                    entry,
                    false,
                    &mut objects,
                    &mut second,
                    &mut CorpseOtherMaps { maps: &mut maps },
                )
                .unwrap();
            assert!(maps[&map(0, 0)].registered_cell(corpse).is_none());
            assert!(second.registered_cell(corpse).is_some());
            assert_eq!(objects.get(corpse).unwrap().map, Some(map(0, 0)));
        } else {
            assert_eq!(
                registry
                    .add_corpses_to_grid(entry, false, &mut objects, &mut second, &mut NoOtherMaps)
                    .unwrap_err()
                    .id,
                "execution.corpse.cross_map_cell_owner"
            );
            assert!(maps[&map(0, 0)].registered_cell(corpse).is_some());
            assert!(second.registered_cell(corpse).is_none());
        }
    }
}
#[test]
fn live_constructor_separates_player_guid_corpse_guid_and_construction_identity() {
    let mut objects = SourceObjects::default();
    let cells = SourceCells::new(map(0, 0)).unwrap();
    let player = objects.construct(1, SourceObjectKind::Player).unwrap();
    objects
        .assign_guid(player, 0, SourceGuid::without_entry(HighGuid::Player, 77))
        .unwrap();
    objects.bind_map(player, map(0, 0), &cells).unwrap();
    objects
        .set_pose(
            player,
            SourcePose([0, 0, 30.0f32.to_bits(), 0.5f32.to_bits()]),
        )
        .unwrap();
    let mut registry = SourceCorpseRegistry::default();
    let mut work = Work::new(CharacterQueryStatus::Empty);
    let corpse = registry
        .create_for_player(
            1,
            77,
            SourceCorpseType::Pvp,
            player,
            &mut objects,
            &cells,
            &mut work,
        )
        .unwrap()
        .unwrap();
    assert_eq!(registry.corpse(corpse).unwrap().created_seconds, 101);
    assert_ne!(corpse, player);
    assert_ne!(
        objects.get(corpse).unwrap().guid,
        objects.get(player).unwrap().guid
    );
    assert_eq!(objects.get(corpse).unwrap().database_guid, 0);
    assert_eq!(
        &registry.corpse(corpse).unwrap().fields()[8..12],
        &[0x3f000000, 0, 0, 0x41f00000]
    );
    assert_eq!(registry.corpse(corpse).unwrap().rank_snapshot, 4);
    assert!(registry
        .for_player(SourceGuid::without_entry(HighGuid::Player, 77))
        .unwrap()
        .is_none());
    registry.add(corpse, &mut work).unwrap();
    registry.save_to_db(corpse, &objects, &mut work).unwrap();
    assert!(matches!(
        work.sql.as_slice(),
        [
            CorpseSql::Begin,
            CorpseSql::DeleteOwner(77),
            CorpseSql::Insert {
                guid: 77,
                player: 77,
                corpse_type: 2,
                ..
            },
            CorpseSql::Commit
        ]
    ));
}
#[test]
fn remove_erases_owner_and_cell_entry_but_retains_corpse_and_empty_cell_node() {
    let (mut registry, mut objects, mut work, _) = initial(CORPSE, CHARACTER, "");
    let object = handle(&registry, 1);
    registry.remove(object, &mut objects, &mut work).unwrap();
    registry.remove(object, &mut objects, &mut work).unwrap();
    assert!(registry
        .for_player(SourceGuid::without_entry(HighGuid::Player, 1))
        .unwrap()
        .is_none());
    assert!(registry.corpse(object).is_ok());
    let state = registry.inventory(&objects).unwrap();
    assert_eq!(state.cells, [(0, 524800, vec![])]);
    assert_eq!(state.buckets, 13);
}
#[test]
fn equipment_overflow_is_typed_undefined_refusal_not_a_modulo_repair() {
    let raw = snapshot(CORPSE, "(1,0,1,8,0,0,'2147483648 0',0)", "");
    let inputs = CorpseInitialInputs::authored(&raw, None).unwrap();
    let mut registry = SourceCorpseRegistry::default();
    let mut objects = SourceObjects::default();
    let mut work = Work::new(CharacterQueryStatus::Rows);
    assert_eq!(
        registry
            .load_initial(
                &inputs,
                1,
                &mut objects,
                &mut Appearance::default(),
                &mut work
            )
            .unwrap_err()
            .id,
        "execution.corpse.equipment_atoi_overflow_undefined"
    );
    assert_eq!(work.clocks, 1);
}

struct Definitions;
impl GridDefinitions for Definitions {
    type Row = ();
    fn read(&mut self, _: GridLoadAttempt) -> Result<()> {
        panic!("empty authored spawn set")
    }
}
#[derive(Default)]
struct Products {
    loads: usize,
    releases: usize,
}
impl GridProductOwner for Products {
    fn load(&mut self, _: MapIdentity, _: ProductCoordinates) -> Result<bool> {
        self.loads += 1;
        Ok(true)
    }
}
impl ResidencyProductOwner for Products {
    fn release_grid_products(&mut self, _: MapIdentity, _: ProductCoordinates) -> Result<()> {
        self.releases += 1;
        Ok(())
    }
}
struct Host {
    registry: SourceCorpseRegistry,
    other: NoOtherMaps,
    events: Vec<ResidencyEvent>,
}
impl GridObjectFactory<Definitions, Products> for Host {
    fn load_from_db(
        &mut self,
        _: GridLoadAttempt,
        _: (),
        _: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<GridCandidateOutcome> {
        panic!("empty authored spawn set")
    }
    fn viewpoint_added(
        &mut self,
        _: ObjectHandle,
        _: CellIdentity,
        _: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        panic!("no creature/GO")
    }
    fn battleground_db_load(
        &mut self,
        _: ObjectHandle,
        _: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        panic!("no creature/GO")
    }
    fn corpse_bindings(&mut self) -> Result<CorpseGridBindings<'_>> {
        Ok(CorpseGridBindings {
            registry: &mut self.registry,
            other_maps: &mut self.other,
            instanceable: false,
        })
    }
}
impl ActiveRelocationOwner<Definitions, Products> for Host {
    fn relocation_phase(
        &mut self,
        _: ObjectHandle,
        _: RelocationPhase,
        _: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        panic!("no creature mover")
    }
}
impl ResidencyOwner<Definitions, Products> for Host {
    fn residency_observe(&mut self, event: ResidencyEvent) -> Result<()> {
        self.events.push(event);
        Ok(())
    }
    fn residency_phase(
        &mut self,
        object: ObjectHandle,
        _phase: ResidencyPhase,
        context: &mut GridFactoryContext<'_, Definitions, Products>,
    ) -> Result<()> {
        assert_eq!(context.objects.get(object)?.kind, SourceObjectKind::Bones);
        Ok(())
    }
}
#[test]
fn actual_grid_loader_and_natural_unloader_preserve_global_corpse_and_reload_same_object() {
    let (registry, mut objects, work, _) = initial(CORPSE, CHARACTER, "");
    let object = handle(&registry, 1);
    let mut host = Host {
        registry,
        other: NoOtherMaps,
        events: Vec::new(),
    };
    let mut cells = SourceCells::new(map(0, 0)).unwrap();
    cells.configure_grids(60001, true).unwrap();
    let mut sets = GridSpawnSets::new(map(0, 0));
    let mut definitions = Definitions;
    let mut products = Products::default();
    let entry = pair();
    {
        let mut loader = SourceGridLoader {
            definitions: &mut definitions,
            factory: &mut host,
            products: &mut products,
            sets: &mut sets,
            objects: &mut objects,
        };
        cells
            .trigger_grid_load(entry, GridLoadTrigger::Enter, &mut loader)
            .unwrap();
    }
    let old = cells
        .grids()
        .identity(entry.grid_x(), entry.grid_y())
        .unwrap();
    assert!(objects.get(object).unwrap().in_world);
    let residency = SourceResidency::new(
        map(0, 0),
        ResidencyConfig {
            visibility: 0.0,
            save_respawn_immediately: false,
            battleground: false,
        },
    )
    .unwrap();
    for diff in [6000, 0, 60001] {
        let mut context = GridFactoryContext {
            definitions: &mut definitions,
            products: &mut products,
            sets: &mut sets,
            objects: &mut objects,
            cells: &mut cells,
        };
        residency.update(diff, &mut context, &mut host).unwrap();
    }
    assert!(cells
        .grids()
        .identity(entry.grid_x(), entry.grid_y())
        .is_none());
    assert!(cells.registered_cell(object).is_none());
    assert_eq!(
        host.registry
            .for_player(SourceGuid::without_entry(HighGuid::Player, 1))
            .unwrap(),
        Some(object)
    );
    assert!(objects.get(object).unwrap().in_world);
    assert!(objects.get(object).unwrap().corpse_grid_reference.is_none());
    assert_eq!(
        objects.lookup(
            map(0, 0),
            LiveLookupKind::Corpse,
            SourceGuid::without_entry(HighGuid::Corpse, 77)
        ),
        Some(object)
    );
    assert_eq!(products.releases, 1);
    {
        let mut loader = SourceGridLoader {
            definitions: &mut definitions,
            factory: &mut host,
            products: &mut products,
            sets: &mut sets,
            objects: &mut objects,
        };
        cells
            .trigger_grid_load(entry, GridLoadTrigger::Enter, &mut loader)
            .unwrap();
    }
    assert_ne!(
        cells
            .grids()
            .identity(entry.grid_x(), entry.grid_y())
            .unwrap()
            .generation,
        old.generation
    );
    assert!(cells.registered_cell(object).is_some());
    assert_eq!(work.clocks, 1);
}
#[test]
fn retained_initial_snapshot_reports_zero_stock_duplicate_assertion_reach() {
    let Some(path) = std::env::var_os("BENILLA_F1_INITIAL_CHARACTERS") else {
        println!("SKIP corpse stock snapshot: set BENILLA_F1_INITIAL_CHARACTERS");
        return;
    };
    let hash = Sha256Id::parse("cf538fd2b7bbd96fd7d653d8d2b05f19da49d49989e42ac412aee382bee5a95d")
        .unwrap();
    let inputs = CorpseInitialInputs::inspect_snapshot(Path::new(&path), &hash).unwrap();
    assert_eq!(inputs.raw_counts, [0, 0, 0]);
    assert_eq!(inputs.rows().count(), 0);
    assert!(inputs.require_sealed_execution().is_err());
    println!("Stock initial corpse/character/guild rows=0/0/0; joined corpses=0; duplicate-owner assertion reach=0; atoi overflow reach=0");
}
#[test]
fn retained_f2_corpse_appearance_uses_real_race_class_displays_and_item_operands() {
    let Some(path) = std::env::var_os("BENILLA_F2_CATALOG") else {
        println!("SKIP corpse appearance catalog: set BENILLA_F2_CATALOG");
        return;
    };
    let mut catalog =
        super::super::production_compiler::VerifiedCatalog::open(Path::new(&path), false).unwrap();
    let mut appearance = CorpseCatalogAppearance::load(&mut catalog).unwrap();
    assert_eq!(appearance.player_displays(1, 8).unwrap(), Some([49, 50]));
    assert_eq!(appearance.player_displays(1, 11).unwrap(), None);
    assert_eq!(appearance.item(u32::MAX).unwrap(), None);
    assert_eq!(
        appearance.item(6948).unwrap(),
        Some(EquippedItem {
            display_info_id: 6418,
            inventory_type: 0
        })
    );
}

#[test]
fn undefined_enum_values_refuse_after_constructor_while_three_remains_failed_load() {
    for kind in [4, 255] {
        let body = CORPSE.replace(",1,0)", &format!(",{kind},0)"));
        let inputs = CorpseInitialInputs::authored(&snapshot(&body, CHARACTER, ""), None).unwrap();
        let mut registry = SourceCorpseRegistry::default();
        let mut objects = SourceObjects::default();
        let mut work = Work::new(CharacterQueryStatus::Rows);
        assert_eq!(
            registry
                .load_initial(
                    &inputs,
                    1,
                    &mut objects,
                    &mut Appearance::default(),
                    &mut work
                )
                .unwrap_err()
                .id,
            "execution.corpse.type_conversion_undefined"
        );
        assert_eq!(work.clocks, 1);
        assert!(registry.inventory(&objects).unwrap().stopped);
    }
}

#[test]
fn unavailable_sql_owner_stops_before_later_writes_or_a_retry() {
    let (mut registry, mut objects, mut work, _) = initial(CORPSE, CHARACTER, "");
    let object = handle(&registry, 1);
    let mut cells = loaded_cells(map(0, 0), pair());
    registry
        .load_cell(pair(), &mut objects, &mut cells, &mut NoOtherMaps)
        .unwrap();
    work.fail_sql_at = Some(1);
    assert_eq!(
        registry
            .save_to_db(object, &objects, &mut work)
            .unwrap_err()
            .id,
        "authored.sql_owner"
    );
    assert_eq!(work.sql, [CorpseSql::Begin]);
    assert_eq!(
        registry
            .save_to_db(object, &objects, &mut work)
            .unwrap_err()
            .id,
        "execution.corpse.stopped"
    );
    assert_eq!(work.sql, [CorpseSql::Begin]);
}

#[path = "corpse_expiry_tests.rs"]
mod expiry;
