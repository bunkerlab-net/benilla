// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored F4.20 component executions, not sealed MC/MPR evidence.
use super::*;
#[derive(Debug, PartialEq, Eq)]
enum Step {
    Event(CorpseEvent),
    Sql(CorpseSql),
    Phase(ObjectHandle, CorpseMapPhase),
    Sweep,
}
struct ConversionWork {
    now: i64,
    constructor_now: i64,
    bg: bool,
    steps: Vec<Step>,
    fail_sql: bool,
}
impl Default for ConversionWork {
    fn default() -> Self {
        Self {
            now: 259251,
            constructor_now: 9000000,
            bg: false,
            steps: Vec::new(),
            fail_sql: false,
        }
    }
}
impl CorpseWork for ConversionWork {
    fn constructor_clock(
        &mut self,
        object: ObjectHandle,
        objects: &mut SourceObjects,
    ) -> Result<ClockSample> {
        assert!(objects.get(object)?.guid.is_none());
        self.constructor_now += 1;
        Ok(ClockSample::authored(
            ClockKind::WallSeconds,
            self.constructor_now,
        ))
    }
    fn observe(&mut self, event: CorpseEvent) -> Result<()> {
        self.steps.push(Step::Event(event));
        Ok(())
    }
    fn query_corpses(&mut self) -> Result<CharacterQueryStatus> {
        panic!("conversion does not query")
    }
    fn sql(&mut self, operation: CorpseSql) -> Result<()> {
        self.steps.push(Step::Sql(operation));
        if self.fail_sql {
            return Err(refuse("authored.expiry_sql"));
        }
        Ok(())
    }
}
impl CorpseConversionWork for ConversionWork {
    fn sweep_clock(&mut self) -> Result<ClockSample> {
        self.steps.push(Step::Sweep);
        Ok(ClockSample::authored(ClockKind::WallSeconds, self.now))
    }
    fn is_battleground(&mut self, _: MapIdentity) -> Result<bool> {
        Ok(self.bg)
    }
    fn map_phase(
        &mut self,
        object: ObjectHandle,
        phase: CorpseMapPhase,
        objects: &mut SourceObjects,
        cells: &mut SourceCells,
    ) -> Result<()> {
        match phase {
            CorpseMapPhase::RemoveVisibility => {
                assert!(!objects.get(object)?.in_world);
                assert!(objects.get(object)?.corpse_grid_reference.is_some());
            }
            CorpseMapPhase::AddedViewpoint | CorpseMapPhase::AddedVisibility => {
                assert!(objects.get(object)?.in_world);
                assert!(cells.registered_cell(object).is_some());
            }
        }
        self.steps.push(Step::Phase(object, phase));
        Ok(())
    }
}
fn owner(player: u32) -> SourceGuid {
    SourceGuid::without_entry(HighGuid::Player, player)
}
fn conversion(maps: &mut BTreeMap<MapIdentity, SourceCells>) -> CorpseConversion<'_> {
    CorpseConversion {
        thread: 1,
        bones_world: true,
        bones_battleground: true,
        maps,
    }
}
fn resident(registry: &mut SourceCorpseRegistry, objects: &mut SourceObjects) -> SourceCells {
    let mut cells = SourceCells::new(map(0, 0)).unwrap();
    cells.configure_grids(60000, true).unwrap();
    let grid = cells.ensure_grid(pair().grid_x(), pair().grid_y()).unwrap();
    cells.set_object_data_loaded(grid).unwrap();
    registry
        .load_cell(pair(), objects, &mut cells, &mut NoOtherMaps)
        .unwrap();
    cells
}
#[test]
fn corpse_expiry_equality_retains_and_next_second_expires() {
    let (mut registry, mut objects, _, _) = initial(CORPSE, CHARACTER, "");
    let old = handle(&registry, 1);
    let mut maps = BTreeMap::new();
    let mut work = ConversionWork {
        now: 259250,
        ..Default::default()
    };
    registry
        .remove_old_corpses(&mut objects, &mut conversion(&mut maps), &mut work)
        .unwrap();
    assert_eq!(registry.for_player(owner(1)).unwrap(), Some(old));
    assert_eq!(work.steps, [Step::Sweep]);
    work.now = 259251;
    registry
        .remove_old_corpses(&mut objects, &mut conversion(&mut maps), &mut work)
        .unwrap();
    assert!(registry.for_player(owner(1)).unwrap().is_none());
    assert!(objects.get(old).is_err());
    assert_eq!(
        work.steps,
        [
            Step::Sweep,
            Step::Sweep,
            Step::Event(CorpseEvent::Remove(old)),
            Step::Sql(CorpseSql::DeleteOwner(1)),
            Step::Event(CorpseEvent::Destroy(old))
        ]
    );
}
#[test]
fn corpse_conversion_absence_is_benign_without_clocks_sql_or_map_access() {
    let mut registry = SourceCorpseRegistry::default();
    let mut objects = SourceObjects::default();
    let mut maps = BTreeMap::new();
    let mut work = ConversionWork::default();
    assert_eq!(
        registry
            .convert_for_player(
                owner(1),
                true,
                &mut objects,
                &mut conversion(&mut maps),
                &mut work
            )
            .unwrap(),
        None
    );
    assert!(work.steps.is_empty());
}
#[test]
fn corpse_sweep_builds_same_guid_fresh_bones_and_deletes_sql_first() {
    let (mut registry, mut objects, _, _) = initial(CORPSE, CHARACTER, "");
    let old = handle(&registry, 1);
    registry.corpse_mut(old).unwrap().rank_snapshot = 9;
    registry
        .corpse_mut(old)
        .unwrap()
        .set_field(3, 0x12345678)
        .unwrap();
    registry
        .corpse_mut(old)
        .unwrap()
        .set_field(37, 0x87654321)
        .unwrap();
    let old_fields = *registry.corpse(old).unwrap().fields();
    let cells = resident(&mut registry, &mut objects);
    let mut maps = BTreeMap::from([(map(0, 0), cells)]);
    let mut work = ConversionWork::default();
    registry
        .remove_old_corpses(&mut objects, &mut conversion(&mut maps), &mut work)
        .unwrap();
    let state = registry.inventory(&objects).unwrap();
    assert!(state.owners_in_source_order.is_empty());
    let bones = &state.corpses[0];
    assert_ne!(bones.object, old);
    assert_eq!(bones.object.construction, old.construction + 1);
    assert_eq!(
        objects.get(bones.object).unwrap().guid.unwrap().counter(),
        77
    );
    assert_eq!(objects.get(bones.object).unwrap().database_guid, 77);
    assert_eq!(bones.corpse_type, 0);
    assert_eq!(bones.created_seconds, 9000001);
    assert_eq!(bones.rank_snapshot, 0);
    assert_eq!(bones.owner, owner(1));
    assert_eq!(bones.fields[35], 5);
    assert_eq!(&bones.fields[0..13], &old_fields[0..13]);
    assert!(old_fields[13] != 0);
    assert_eq!(&bones.fields[13..32], &[0; 19]);
    assert_eq!(&bones.fields[32..35], &old_fields[32..35]);
    assert_eq!(&bones.fields[36..38], &old_fields[36..38]);
    assert!(bones.in_world);
    assert_eq!(
        bones.position,
        [0x41200000, 0x41a00000, 0x41f00000, 0x3f000000]
    );
    assert!(objects.get(old).is_err());
    assert!(registry.corpse(old).is_err());
    assert_eq!(
        work.steps,
        [
            Step::Sweep,
            Step::Event(CorpseEvent::Remove(old)),
            Step::Phase(old, CorpseMapPhase::RemoveVisibility),
            Step::Sql(CorpseSql::DeleteOwner(1)),
            Step::Event(CorpseEvent::Construct(bones.object)),
            Step::Phase(bones.object, CorpseMapPhase::AddedViewpoint),
            Step::Phase(bones.object, CorpseMapPhase::AddedVisibility),
            Step::Event(CorpseEvent::Destroy(old))
        ]
    );
    assert_eq!(
        maps[&map(0, 0)]
            .removals()
            .removed_guids()
            .collect::<Vec<_>>(),
        [SourceGuid::without_entry(HighGuid::Corpse, 77)]
    );
    // Advancing the corpse sweep never visits bones; ObjectAccessor.cpp:293-301.
    work.now = 10000000;
    registry
        .remove_old_corpses(&mut objects, &mut conversion(&mut maps), &mut work)
        .unwrap();
    assert!(registry.corpse(bones.object).is_ok());
}
#[test]
fn corpse_conversion_cold_map_cold_grid_and_removal_grid_never_create_bones() {
    for branch in 0..3 {
        let (mut registry, mut objects, _, _) = initial(CORPSE, CHARACTER, "");
        let old = handle(&registry, 1);
        let mut maps = BTreeMap::new();
        if branch != 0 {
            let mut cells = SourceCells::new(map(0, 0)).unwrap();
            if branch == 2 {
                cells = resident(&mut registry, &mut objects);
                let grid = cells
                    .grids()
                    .identity(pair().grid_x(), pair().grid_y())
                    .unwrap();
                cells.grids.set_phase(grid, NGridPhase::Removal).unwrap();
            }
            maps.insert(map(0, 0), cells);
        }
        let mut work = ConversionWork::default();
        assert!(registry
            .convert_for_player(
                owner(1),
                true,
                &mut objects,
                &mut conversion(&mut maps),
                &mut work
            )
            .unwrap()
            .is_none());
        assert!(objects.get(old).is_err());
        assert!(!work
            .steps
            .iter()
            .any(|step| matches!(step, Step::Event(CorpseEvent::Construct(_)))));
        assert!(registry.inventory(&objects).unwrap().corpses.is_empty());
    }
}
#[test]
fn corpse_bones_options_choose_map_kind_and_insignia_overrides_both() {
    for (bg, world, battleground, insignia, expected) in [
        (false, false, true, false, false),
        (false, true, false, false, true),
        (true, true, false, false, false),
        (true, false, true, false, true),
        (false, false, false, true, true),
        (true, false, false, true, true),
    ] {
        let (mut registry, mut objects, _, _) = initial(CORPSE, CHARACTER, "");
        let cells = resident(&mut registry, &mut objects);
        let mut maps = BTreeMap::from([(map(0, 0), cells)]);
        let mut context = conversion(&mut maps);
        context.bones_world = world;
        context.bones_battleground = battleground;
        let mut work = ConversionWork {
            bg,
            ..Default::default()
        };
        assert_eq!(
            registry
                .convert_for_player(owner(1), insignia, &mut objects, &mut context, &mut work)
                .unwrap()
                .is_some(),
            expected
        );
    }
}
#[test]
fn corpse_conversion_created_but_not_object_loaded_grid_still_gets_bones() {
    let (mut registry, mut objects, _, _) = initial(CORPSE, CHARACTER, "");
    let cells = loaded_cells(map(0, 0), pair());
    let mut maps = BTreeMap::from([(map(0, 0), cells)]);
    let bones = registry
        .convert_for_player(
            owner(1),
            false,
            &mut objects,
            &mut conversion(&mut maps),
            &mut ConversionWork::default(),
        )
        .unwrap()
        .unwrap();
    assert!(objects.get(bones).unwrap().in_world);
    assert!(!maps[&map(0, 0)].object_data_loaded(pair().grid_x(), pair().grid_y()));
}
#[test]
fn corpse_sweep_mutation_preserves_source_unordered_chain_and_one_clock() {
    let corpses = "(77,1,10,20,30,0.5,0,50,1,0),(78,14,10,20,30,0.5,0,50,1,0),(79,2,10,20,30,0.5,0,51,1,0),(80,27,10,20,30,0.5,0,50,1,0)";
    let chars = "(1,0,1,8,0,0,'',0),(14,0,1,8,0,0,'',0),(2,0,1,8,0,0,'',0),(27,0,1,8,0,0,'',0)";
    let (mut registry, mut objects, _, _) = initial(corpses, chars, "");
    assert_eq!(
        registry
            .inventory(&objects)
            .unwrap()
            .owners_in_source_order
            .iter()
            .map(|(owner, _)| owner.counter())
            .collect::<Vec<_>>(),
        [2, 27, 14, 1]
    );
    let keep = handle(&registry, 2);
    let mut work = ConversionWork::default();
    registry
        .remove_old_corpses(
            &mut objects,
            &mut conversion(&mut BTreeMap::new()),
            &mut work,
        )
        .unwrap();
    assert_eq!(
        work.steps
            .iter()
            .filter_map(
                |step| if let Step::Sql(CorpseSql::DeleteOwner(owner)) = step {
                    Some(*owner)
                } else {
                    None
                }
            )
            .collect::<Vec<_>>(),
        [27, 14, 1]
    );
    assert_eq!(
        work.steps
            .iter()
            .filter(|step| **step == Step::Sweep)
            .count(),
        1
    );
    assert_eq!(registry.for_player(owner(2)).unwrap(), Some(keep));
}
#[test]
fn corpse_sql_refusal_stops_before_bones_construction_and_retry() {
    let (mut registry, mut objects, _, _) = initial(CORPSE, CHARACTER, "");
    let cells = resident(&mut registry, &mut objects);
    let mut maps = BTreeMap::from([(map(0, 0), cells)]);
    let mut work = ConversionWork {
        fail_sql: true,
        ..Default::default()
    };
    assert_eq!(
        registry
            .convert_for_player(
                owner(1),
                false,
                &mut objects,
                &mut conversion(&mut maps),
                &mut work
            )
            .unwrap_err()
            .id,
        "authored.expiry_sql"
    );
    assert!(!work
        .steps
        .iter()
        .any(|step| matches!(step, Step::Event(CorpseEvent::Construct(_)))));
    assert_eq!(
        registry
            .convert_for_player(
                owner(1),
                false,
                &mut objects,
                &mut conversion(&mut maps),
                &mut work
            )
            .unwrap_err()
            .id,
        "execution.corpse.stopped"
    );
}
#[test]
fn corpse_expiry_signed_subtraction_refuses_only_below_defined_boundary() {
    let (registry, objects, _, _) = initial(CORPSE, CHARACTER, "");
    let corpse = registry.corpse(handle(&registry, 1)).unwrap();
    assert!(!corpse.is_expired(i64::MIN + 259200).unwrap());
    assert_eq!(
        corpse.is_expired(i64::MIN + 259199).unwrap_err().id,
        "execution.corpse.expiry_subtraction_undefined"
    );
    assert_eq!(registry.inventory(&objects).unwrap().corpses.len(), 1);
}
struct WorldHost {
    phases: Vec<WorldPhase>,
    sweeps: usize,
}
impl WorldWork for WorldHost {
    fn phase(&mut self, phase: WorldPhase, _: u64, _: u32) -> Result<()> {
        self.phases.push(phase);
        Ok(())
    }
    fn run_map_jobs(&mut self, _: u64, _: &[MapJob], _: &mut SourceScheduler) -> Result<()> {
        panic!("no maps")
    }
    fn run_map_inline(&mut self, _: u64, _: MapJob) -> Result<()> {
        panic!("no maps")
    }
    fn unload_maps(&mut self, _: u32, _: &mut SourceMapManager) -> Result<()> {
        Ok(())
    }
    fn remove_old_corpses(&mut self, thread: u64) -> Result<()> {
        assert_eq!(thread, 1);
        assert_eq!(self.phases.last(), Some(&WorldPhase::Corpses));
        self.sweeps += 1;
        Ok(())
    }
}
#[test]
fn corpse_world_timer_is_twenty_minutes_and_preserves_one_interval_overshoot() {
    let mut world = SourceWorld::new(1, 100).unwrap();
    let mut work = WorldHost {
        phases: Vec::new(),
        sweeps: 0,
    };
    world.update(1199999, &mut work).unwrap();
    assert_eq!(work.sweeps, 0);
    world.update(1, &mut work).unwrap();
    assert_eq!(work.sweeps, 1);
    assert_eq!(world.corpse_timer_current(), 0);
    world.update(2400007, &mut work).unwrap();
    assert_eq!(work.sweeps, 2);
    assert_eq!(world.corpse_timer_current(), 1200007);
    world.update(0, &mut work).unwrap();
    assert_eq!(work.sweeps, 3);
    assert_eq!(world.corpse_timer_current(), 7);
    let tail = &work.phases[work.phases.len() - 23..];
    assert!(
        tail.iter()
            .position(|p| *p == WorldPhase::SqlCallbacks)
            .unwrap()
            < tail.iter().position(|p| *p == WorldPhase::Corpses).unwrap()
    );
    assert!(
        tail.iter().position(|p| *p == WorldPhase::Corpses).unwrap()
            < tail
                .iter()
                .position(|p| *p == WorldPhase::GameEvents)
                .unwrap()
    );
}
#[test]
fn corpse_bones_explicit_queue_misses_after_cleanup_then_real_grid_unload_deletes() {
    let (mut registry, mut objects, _, _) = initial(CORPSE, CHARACTER, "");
    let cells = resident(&mut registry, &mut objects);
    let mut maps = BTreeMap::from([(map(0, 0), cells)]);
    let bones = registry
        .convert_for_player(
            owner(1),
            false,
            &mut objects,
            &mut conversion(&mut maps),
            &mut ConversionWork::default(),
        )
        .unwrap()
        .unwrap();
    let mut cells = maps.remove(&map(0, 0)).unwrap();
    objects
        .bind_source_allocation(bones, 0x8000, 0x100, 0)
        .unwrap();
    let grid = cells.registered_cell(bones).unwrap().grid;
    let mut host = Host {
        registry: SourceCorpseRegistry::default(),
        other: NoOtherMaps,
        events: Vec::new(),
    };
    let mut definitions = Definitions;
    let mut products = Products::default();
    let mut sets = GridSpawnSets::new(map(0, 0));
    let residency = SourceResidency::new(
        map(0, 0),
        ResidencyConfig {
            visibility: 0.0,
            save_respawn_immediately: false,
            battleground: false,
        },
    )
    .unwrap();
    {
        let mut context = GridFactoryContext {
            definitions: &mut definitions,
            products: &mut products,
            sets: &mut sets,
            objects: &mut objects,
            cells: &mut cells,
        };
        assert!(registry
            .delete_bones_from_world(bones, &mut context, &mut host)
            .unwrap());
        assert!(!registry
            .delete_bones_from_world(bones, &mut context, &mut host)
            .unwrap());
        host.registry = registry;
        residency.drain(&mut context, &mut host).unwrap();
        assert!(host.events.contains(&ResidencyEvent::MissingCorpse(bones)));
        assert!(context.objects.get(bones).is_ok());
        residency.update(0, &mut context, &mut host).unwrap();
        assert_eq!(
            context.cells.grids().phase(grid).unwrap(),
            NGridPhase::Removal
        );
        residency.update(60000, &mut context, &mut host).unwrap();
    }
    assert!(objects.get(bones).is_err());
    assert!(host.registry.corpse(bones).is_err());
    assert!(host
        .registry
        .inventory(&objects)
        .unwrap()
        .corpses
        .is_empty());
    assert!(cells.grids().identity(grid.x, grid.y).is_none());
}

#[test]
fn corpse_conversion_unlinks_the_actual_foreign_grid_reference() {
    for supplied in [false, true] {
        let (mut registry, mut objects, _, _) = initial(CORPSE, CHARACTER, "");
        let old = handle(&registry, 1);
        let first = resident(&mut registry, &mut objects);
        let mut maps = BTreeMap::from([(map(0, 0), first)]);
        let mut second = loaded_cells(map(1, 0), pair());
        registry
            .add_corpses_to_grid(
                pair(),
                false,
                &mut objects,
                &mut second,
                &mut CorpseOtherMaps { maps: &mut maps },
            )
            .unwrap();
        assert!(second.registered_cell(old).is_some());
        if supplied {
            maps.insert(map(1, 0), second);
        }
        let mut work = ConversionWork::default();
        let result = registry.convert_for_player(
            owner(1),
            false,
            &mut objects,
            &mut conversion(&mut maps),
            &mut work,
        );
        if supplied {
            assert!(result.unwrap().is_some());
            assert!(objects.get(old).is_err());
            assert!(maps[&map(1, 0)].registered_cell(old).is_none());
        } else {
            assert_eq!(
                result.unwrap_err().id,
                "execution.corpse.cross_map_cell_owner"
            );
            assert!(!work.steps.iter().any(|step| matches!(step, Step::Sql(_))));
        }
    }
}

#[test]
fn corpse_empty_sweep_still_samples_its_own_clock_once() {
    let mut registry = SourceCorpseRegistry::default();
    let mut objects = SourceObjects::default();
    let mut work = ConversionWork::default();
    registry
        .remove_old_corpses(
            &mut objects,
            &mut conversion(&mut BTreeMap::new()),
            &mut work,
        )
        .unwrap();
    assert_eq!(work.steps, [Step::Sweep]);
}
