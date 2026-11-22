// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
fn entry(id: u32, index: u32, world_state: u32) -> DungeonEncounterEntry {
    DungeonEncounterEntry {
        id,
        map: 0,
        difficulty: 0,
        data: 0,
        index,
        complete_world_state: world_state,
        names: std::array::from_fn(|_| Vec::new()),
        name_flags: 0,
        icon: 0,
    }
}
#[test]
fn encounter_validation_order_multimap_and_reload() {
    let store = LoadedDungeonEncounters::load([entry(1, 0, 0), entry(2, 1, 0)]);
    let refs = EncounterReferences {
        creatures: BTreeSet::from([8]),
        spells: BTreeSet::from([9]),
    };
    let rows = [
        [99, 99, 0, 0],
        [1, 0, 99, 0],
        [1, 1, 99, 0],
        [1, 3, 0, 0],
        [2, 256, 8, 777],
        [1, 2, 8, 999],
        [2, 1, 9, 0],
    ];
    let (loaded, census) = LoadedInstanceEncounters::load(None, Some(&rows), &store, &refs);
    assert_eq!((census.visited, census.loaded), (7, 3));
    assert_eq!(
        census.skipped,
        BTreeMap::from([
            (EncounterSkip::Encounter, 1),
            (EncounterSkip::Creature, 1),
            (EncounterSkip::Spell, 1),
            (EncounterSkip::CreditType, 1)
        ])
    );
    assert_eq!(
        loaded
            .by_map(0)
            .iter()
            .map(|e| e.entry.id)
            .collect::<Vec<_>>(),
        [2, 1, 2]
    );
    assert_eq!(
        loaded.by_credit()[&8]
            .iter()
            .map(|e| e.last_dungeon)
            .collect::<Vec<_>>(),
        [777, 999]
    );
    let (reloaded, census) =
        LoadedInstanceEncounters::load(Some(&loaded), Some(&rows[4..5]), &store, &refs);
    assert_eq!(census.loaded, 1);
    assert_eq!(reloaded.by_map(0).len(), 4);
    assert_eq!(reloaded.by_credit()[&8].len(), 1);
    let (empty, _) = LoadedInstanceEncounters::load(Some(&reloaded), None, &store, &refs);
    assert!(empty.by_credit().is_empty());
    assert_eq!(empty.by_map(0).len(), 4);
}
#[test]
fn encounter_defaults_fallback_unsigned_arithmetic_and_broadcast() {
    let mut variables = SourceWorldStateVariables::default();
    assert_eq!(variables.get(99), 0);
    assert!(variables.entries().is_empty());
    variables.set(99, 0).unwrap();
    let value = &variables.entries()[&99];
    assert_eq!(
        (
            value.value,
            value.send,
            value.zone,
            value.area,
            value.executors.len()
        ),
        (0, false, 0, 0, 0)
    );
    variables.set_encounter(0x8000_0000, true).unwrap();
    assert_eq!((variables.get(0), variables.get(1)), (1, 0));
    variables.set_encounter(21_474_837, false).unwrap();
    assert_eq!(
        (variables.get(-2_147_483_596), variables.get(-2_147_483_595)),
        (0, 1)
    );
    variables.set_encounter(u32::MAX, true).unwrap();
    assert_eq!((variables.get(-100), variables.get(-99)), (1, 0));
    variables.set_data(99, true, 4, 5);
    variables.set(99, 0).unwrap();
    assert_eq!(
        variables.set(99, 1).unwrap_err().id,
        "execution.world_state.BroadcastVariable_owner"
    );
    assert_eq!(variables.get(99), 1); // Mutation precedes the missing broadcast.
}
#[test]
fn encounter_signed_shift_refuses_at_exact_operation_on_both_branches() {
    let refs = EncounterReferences {
        creatures: BTreeSet::new(),
        spells: BTreeSet::new(),
    };
    for world_state in [0, 900] {
        let store = LoadedDungeonEncounters::load([
            entry(1, 30, world_state),
            entry(2, 31, world_state),
            entry(3, 32, world_state),
        ]);
        let (loaded, _) = LoadedInstanceEncounters::load(
            None,
            Some(&[[1, 2, 0, 0], [2, 2, 0, 0], [3, 2, 0, 0]]),
            &store,
            &refs,
        );
        assert_eq!(loaded.shift_census(), (3, 2));
        let mut variables = SourceWorldStateVariables::default();
        assert_eq!(
            variables
                .initialize(0, 1 << 30, Some(&loaded))
                .unwrap_err()
                .id,
            ENCOUNTER_SHIFT_UB
        );
        assert_eq!(variables.get(if world_state == 0 { 100 } else { 900 }), 1);
        assert!(!variables.entries().contains_key(&200));
        let (only32, _) =
            LoadedInstanceEncounters::load(None, Some(&[[3, 2, 0, 0]]), &store, &refs);
        assert_eq!(
            SourceWorldStateVariables::default()
                .initialize(0, 0, Some(&only32))
                .unwrap_err()
                .id,
            ENCOUNTER_SHIFT_UB
        );
    }
}
#[test]
fn encounter_catalog_census() {
    let Some(root) = std::env::var_os("BENILLA_F2_CATALOG") else {
        eprintln!("SKIP encounter_catalog_census: BENILLA_F2_CATALOG absent");
        return;
    };
    let mut catalog = super::super::super::production_compiler::VerifiedCatalog::open(
        std::path::Path::new(&root),
        false,
    )
    .unwrap();
    let (store, rows, refs) = catalog.execution_encounter_operands().unwrap();
    let (loaded, census) = LoadedInstanceEncounters::load(None, Some(&rows), &store, &refs);
    println!(
        "ENCOUNTERS dungeon={} visited={} loaded={} skipped={:?} shift_census={:?} map0={} map1={}",
        store.rows().len(),
        census.visited,
        census.loaded,
        census.skipped,
        loaded.shift_census(),
        loaded.by_map(0).len(),
        loaded.by_map(1).len()
    );
    assert_eq!(
        loaded.shift_census().1,
        0,
        "stock reaches signed-shift UB; stop before committing"
    );
}

#[test]
fn encounter_initialize_uses_official_ids_and_fallback_in_query_order() {
    // Equivalent map keys keep insertion order; the later encounter overwrites ID 700.
    let mut other_map = entry(4, 32, 800);
    other_map.map = 1;
    let store = LoadedDungeonEncounters::load([
        entry(1, 0, 700),
        entry(2, 1, 0),
        entry(3, 2, 700),
        other_map,
    ]);
    let refs = EncounterReferences {
        creatures: BTreeSet::new(),
        spells: BTreeSet::new(),
    };
    let (loaded, _) = LoadedInstanceEncounters::load(
        None,
        Some(&[[1, 2, 0, 0], [2, 2, 0, 0], [3, 2, 0, 0], [4, 2, 0, 0]]),
        &store,
        &refs,
    );
    let mut variables = SourceWorldStateVariables::default();
    variables.initialize(0, 3, Some(&loaded)).unwrap();
    assert_eq!(
        variables.values(),
        BTreeMap::from([(200, 1), (201, 0), (700, 0)])
    );
    assert_eq!(variables.entries()[&700].executors.len(), 0);
    assert!(variables
        .entries()
        .values()
        .all(|v| !v.send && v.zone == 0 && v.area == 0));
    variables.initialize(99, 0, Some(&loaded)).unwrap();
    assert_eq!(
        variables.values(),
        BTreeMap::from([(200, 1), (201, 0), (700, 0)])
    );
    assert_eq!(
        variables.initialize(0, 0, None).unwrap_err().id,
        DUNGEON_ENCOUNTER_BOUNDS_OWNER
    );
}
