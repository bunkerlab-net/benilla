// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
fn target(kind: u32) -> CreatureSpellTargetRow {
    SqlTemplateRow {
        ordinal: 0,
        words: [1, kind, 0, 0, 0, u32::MAX],
        strings: [b"current".to_vec()],
    }
}
fn entry(id: u32) -> CreatureSpellEntryRow {
    SqlTemplateRow {
        ordinal: 0,
        words: [id, 0, 0],
        strings: [b"list".to_vec()],
    }
}
fn spell(id: u32, position: u32, spell: u32) -> CreatureSpellRow {
    SqlTemplateRow {
        ordinal: 0,
        words: [id, position, spell, 0, u32::MAX, 1, 0, 100, 0, 0, 0, 0, 0],
        strings: [],
    }
}
fn metadata() -> BTreeMap<u32, u32> {
    BTreeMap::from([(10, 0), (11, 0x20000), (12, 0)])
}
#[test]
fn targeting_accepts_max_and_entries_drop_empty_names() {
    let (store, census) =
        LoadedCreatureSpellLists::load(vec![target(3)], vec![entry(1)], vec![], &metadata());
    assert_eq!(store.targeting[&1].words[1], 3);
    assert_eq!(census.loaded, [1, 1, 0]);
    let mut empty = entry(2);
    empty.strings[0].clear();
    let (store, census) =
        LoadedCreatureSpellLists::load(vec![target(4)], vec![empty], vec![], &metadata());
    assert!(store.targeting.is_empty() && store.lists.is_empty());
    assert_eq!(census.dropped, BTreeMap::from([(995, 1), (1019, 1)]));
    assert_eq!(census.source_logged_count, 0);
}
#[test]
fn every_modern_validation_runs_in_source_order() {
    let cases = [
        (1046, spell(2, 0, 999)),
        (1053, spell(1, 0, 999)),
        (1067, {
            let mut s = spell(1, 0, 10);
            s.words[5] = 9;
            s
        }),
        (1073, {
            let mut s = spell(1, 0, 10);
            s.words[3] = 3;
            s
        }),
        (1079, {
            let mut s = spell(1, 0, 10);
            s.words[3] = 1;
            s
        }),
        (1095, {
            let mut s = spell(1, 0, 10);
            s.words[9] = 2;
            s.words[10] = 1;
            s
        }),
        (1101, {
            let mut s = spell(1, 0, 10);
            s.words[11] = 2;
            s.words[12] = 1;
            s
        }),
    ];
    for (reason, row) in cases {
        let (_, census) =
            LoadedCreatureSpellLists::load(vec![target(0)], vec![entry(1)], vec![row], &metadata());
        assert_eq!(census.dropped, BTreeMap::from([(reason, 1)]));
    }
    let (store, census) = LoadedCreatureSpellLists::load(
        vec![target(0)],
        vec![entry(1)],
        vec![spell(1, 0, 2), spell(1, 1, 11)],
        &metadata(),
    );
    assert_eq!(census.loaded, [1, 1, 2]);
    assert!(store.lists[&1].spells[&0].disabled_for_ai);
    assert!(store.lists[&1].spells[&1].disabled_for_ai);
    assert_eq!(store.lists[&1].spells[&1].combat_condition().unwrap(), -1);
}
#[test]
fn duplicate_spell_lookup_uses_spell_id_as_position_then_emplace_keeps_first() {
    let (store, census) = LoadedCreatureSpellLists::load(
        vec![target(0)],
        vec![entry(1)],
        vec![spell(1, 11, 10), spell(1, 20, 11), spell(1, 11, 12)],
        &metadata(),
    );
    assert_eq!(census.dropped, BTreeMap::from([(1059, 1), (1109, 1)]));
    assert_eq!(store.lists[&1].spells.len(), 1);
    assert_eq!(store.lists[&1].spells[&11].spell_id, 10);
}
#[derive(Default)]
struct Rolls {
    draws: Vec<(u32, u32)>,
    values: std::collections::VecDeque<u32>,
}
impl CreatureRandom for Rolls {
    fn creature_irand(&mut self, _: i32, _: i32) -> Result<i32> {
        panic!("unexpected signed draw")
    }
    fn creature_urand(&mut self, a: u32, b: u32) -> Result<u32> {
        self.draws.push((a, b));
        Ok(self.values.pop_front().expect("unexpected draw"))
    }
}
#[test]
fn set_spell_list_zero_and_missing_keep_source_state_without_draws() {
    let mut random = Rolls::default();
    let mut list = CreatureSpellList {
        id: 7,
        name: b"old".to_vec(),
        ..Default::default()
    };
    assert!(list.set(0, None, &mut random).unwrap());
    assert!(list.disabled && list.spells.is_empty());
    assert_eq!(list.id, 7);
    assert_eq!(
        list.set(1, None, &mut random).unwrap_err().id,
        "execution.creature.loaded_spell_list_store_owner"
    );
    let map = CreatureSpellMapData(Arc::new(LoadedCreatureSpellLists::default()));
    let before = list.clone();
    assert!(!list.set(99, Some(&map), &mut random).unwrap());
    assert_eq!(list, before);
    assert!(random.draws.is_empty());
}
#[test]
fn availability_uses_inclusive_0_100_and_keeps_equality_in_position_order() {
    let mut rows = vec![
        spell(1, 0, 10),
        spell(1, 1, 11),
        spell(1, 2, 12),
        spell(1, 3, 10),
    ];
    rows[0].words[7] = 0;
    rows[1].words[7] = 99;
    rows[2].words[7] = 0;
    let (store, _) =
        LoadedCreatureSpellLists::load(vec![target(0)], vec![entry(1)], rows, &metadata());
    let map = CreatureSpellMapData(Arc::new(store));
    let mut random = Rolls {
        values: [0, 100, 1].into(),
        ..Default::default()
    };
    let mut list = CreatureSpellList::default();
    assert!(list.set(1, Some(&map), &mut random).unwrap());
    assert_eq!(random.draws, [(0, 100); 3]);
    assert_eq!(list.spells.keys().copied().collect::<Vec<_>>(), [0, 3]);
    assert_eq!(map.get(1).unwrap().spells.len(), 4);
}
#[test]
fn legacy_unwritten_condition_survives_copies_and_refuses_only_at_read() {
    let mut store = LoadedCreatureSpellLists::default();
    let creatures = BTreeSet::from([1]);
    let spells = metadata();
    let mut cooldowns = BTreeMap::from([((1, 10), [7, 3])]);
    assert_eq!(
        LoadedCreatureSpellLists::load_cooldowns(
            &[[2, 10, 1, 2], [1, 99, 1, 2], [1, 10, 0, 0], [1, 10, 9, 4]],
            &creatures,
            &spells,
            &mut cooldowns
        ),
        BTreeMap::from([(9837, 1), (9843, 1), (9850, 1)])
    );
    assert_eq!(cooldowns[&(1, 10)], [9, 4]);
    let rows = [
        [1, 0, 10, 2, 999, 0, 0, 0, 0, 0, 0, 0],
        [2, 0, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        [1, 0, 11, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    ];
    let census = store.load_legacy(&rows, &creatures, &spells, &cooldowns);
    assert_eq!((census.lists, census.unwritten_combat_conditions), (1, 2));
    assert_eq!(
        census.dropped,
        BTreeMap::from([(9768, 1), (9774, 1), (9791, 1)])
    );
    let map = CreatureSpellMapData(Arc::new(store));
    let mut list = CreatureSpellList::default();
    let mut random = Rolls::default();
    assert!(list.set(100, Some(&map), &mut random).unwrap());
    assert_eq!(list.spells[&0].repeat, [9, 4]);
    assert_eq!(list.id, 0);
    assert_eq!(list.chance_support, 0);
    assert_eq!(list.chance_ranged, 0);
    assert!(list.disabled && !list.ai_has_spells());
    assert_eq!(
        list.spells[&0].combat_condition().unwrap_err().id,
        "execution.creature_spell_lists.UnitAI_1213.unwritten_combat_condition"
    );
    assert!(random.draws.is_empty());
    assert_eq!(
        map.0.targeting[&1].as_ref(),
        &CreatureSpellTarget::default()
    );
}
#[test]
fn map_snapshot_retains_loaded_owner_across_reload() {
    let (store, _) =
        LoadedCreatureSpellLists::load(vec![target(0)], vec![entry(1)], vec![], &metadata());
    let mut world = Arc::new(store);
    let map = CreatureSpellMapData(Arc::clone(&world));
    world = Arc::new(LoadedCreatureSpellLists::default());
    assert!(world.get(1).is_none());
    assert!(map.get(1).is_some());
}

#[test]
fn unit_ai_reads_bound_list_but_unbound_owner_retains_its_refusal() {
    struct Bound(CreatureSpellList);
    impl UnitAiExecutionOwner for Bound {
        fn creature_spell_list(&mut self, _: ObjectHandle) -> Result<&CreatureSpellList> {
            Ok(&self.0)
        }
    }
    struct Unbound;
    impl UnitAiExecutionOwner for Unbound {}
    let mut objects = SourceObjects::default();
    let unit = objects.construct(7, SourceObjectKind::Creature).unwrap();
    let (store, _) = LoadedCreatureSpellLists::load(
        vec![target(0)],
        vec![entry(1)],
        vec![spell(1, 0, 10)],
        &metadata(),
    );
    let mut bound = Bound(store.get(1).unwrap().clone());
    assert!(bound.spell_list_has_spells(unit).unwrap());
    bound.0.disabled = true;
    assert!(!bound.spell_list_has_spells(unit).unwrap());
    assert_eq!(
        Unbound.spell_list_has_spells(unit).unwrap_err().id,
        "execution.unit_ai.spell_list_owner"
    );
}
#[test]
fn event_ai_resolves_loaded_lists_and_retains_unbound_store_refusal() {
    let Some(root) = std::env::var_os("BENILLA_F2_CATALOG") else {
        eprintln!("SKIP event_ai_resolves_loaded_lists_and_retains_unbound_store_refusal: BENILLA_F2_CATALOG absent");
        return;
    };
    let mut catalog =
        super::super::super::production_compiler::VerifiedCatalog::open(Path::new(&root), false)
            .unwrap();
    let config = EventAiLoaderConfig::parse(
        b"LogFilter_EventAiDev=1",
        Sha256Id::of(b"LogFilter_EventAiDev=1"),
    )
    .unwrap();
    let (store, _) =
        LoadedCreatureSpellLists::load(vec![target(0)], vec![entry(1)], vec![], &metadata());
    let quests = SourceQuestMap::default();
    let mut owner = EventAiCatalog::new(&mut catalog, config, &quests);
    assert_eq!(
        owner
            .contains(EventAiLookup::SpellList, 1, 0)
            .unwrap_err()
            .id,
        "execution.event_ai.loader.spell_list_store_owner"
    );
    owner.bind_spell_lists(&store);
    assert!(owner.contains(EventAiLookup::SpellList, 1, 0).unwrap());
    assert!(!owner.contains(EventAiLookup::SpellList, 2, 0).unwrap());
}
