use super::*;
use std::collections::BTreeSet;

#[derive(Default)]
struct Owner {
    spells: BTreeMap<u32, [(u32, i32); 3]>,
    triggers: BTreeSet<u32>,
    reads: Vec<u32>,
}
impl QuestFlagLoaderOwner for Owner {
    fn spell_effects(&mut self, id: u32) -> Result<Option<[(u32, i32); 3]>> {
        self.reads.push(id);
        Ok(self.spells.get(&id).copied())
    }
    fn area_trigger(&mut self, id: u32) -> Result<bool> {
        Ok(self.triggers.contains(&id))
    }
}
fn quest() -> QuestFlagRow {
    QuestFlagRow {
        id: 7,
        ..Default::default()
    }
}
fn load(row: QuestFlagRow, owner: &mut impl QuestFlagLoaderOwner) -> SourceQuestFlags {
    SourceQuestFlags::load_quests([row], owner).unwrap()
}

#[test]
fn deliver_sets_before_item_validation_and_preserves_already_set_flags() {
    for initial in [0, DELIVER] {
        let mut row = quest();
        row.special_flags = initial as u16;
        row.required_items = [999999, 1, 0, 0];
        let flags = load(row, &mut Owner::default());
        assert_eq!(flags.flags(7), Some(8));
        assert_eq!(
            flags.mutation_counts(),
            [usize::from(initial == 0), 0, 0, 0, 0]
        );
    }
    assert_eq!(load(quest(), &mut Owner::default()).flags(7), Some(0));
}

#[test]
fn required_spell_effects_set_only_matching_quest_complete_or_send_event() {
    for effect in [[(16, 7), (0, 0), (0, 0)], [(0, 0), (0, 0), (61, -1)]] {
        for initial in [0, EXPLORATION] {
            let mut row = quest();
            row.special_flags = initial as u16;
            row.required_spells = [99, 99, 0, 0];
            let mut owner = Owner::default();
            owner.spells.insert(99, effect);
            let flags = load(row, &mut owner);
            assert_eq!(flags.flags(7), Some(2));
            assert_eq!(
                flags.mutation_counts(),
                [0, usize::from(initial == 0), 0, 0, 0]
            );
            assert_eq!(owner.reads, [99, 99]);
        }
    }
    for effects in [None, Some([(16, 8), (16, -1), (0, 0)])] {
        let mut owner = Owner::default();
        if let Some(effects) = effects {
            owner.spells.insert(99, effects);
        }
        let mut row = quest();
        row.required_spells[0] = 99;
        assert_eq!(load(row, &mut owner).flags(7), Some(0));
    }
    let mut row = quest();
    row.required_spells[0] = 99;
    row.required_targets[0] = 123;
    let mut owner = Owner::default();
    owner.spells.insert(99, [(61, 0); 3]);
    assert_eq!(load(row, &mut owner).flags(7), Some(48));
    assert_eq!(owner.reads, [99]);
}

#[test]
fn cached_nonzero_target_sets_both_flags_even_when_target_would_be_missing() {
    for target in [-999999, 999999] {
        for initial in [0, 0x10, 0x20, KILL_SPEAK] {
            let mut row = quest();
            row.special_flags = initial as u16;
            row.required_targets = [target; 4];
            let flags = load(row, &mut Owner::default());
            assert_eq!(flags.flags(7), Some(48));
            assert_eq!(
                flags.mutation_counts(),
                [0, 0, usize::from(initial != KILL_SPEAK), 0, 0]
            );
        }
    }
}

#[test]
fn timed_sets_for_any_nonzero_limit_and_preserves_already_set_flag() {
    for limit_time in [1, u32::MAX] {
        for initial in [0, TIMED] {
            let mut row = quest();
            row.special_flags = initial as u16;
            row.limit_time = limit_time;
            let flags = load(row, &mut Owner::default());
            assert_eq!(flags.flags(7), Some(64));
            assert_eq!(
                flags.mutation_counts(),
                [0, 0, 0, usize::from(initial == 0), 0]
            );
        }
    }
}

#[test]
fn area_triggers_follow_quests_and_precede_scriptmgr_reads_and_mutations() {
    for spell_sets in [false, true] {
        for initial in [0, EXPLORATION] {
            let mut row = quest();
            row.special_flags = initial as u16;
            let mut owner = Owner::default();
            owner.triggers.insert(10);
            if spell_sets {
                row.required_spells[0] = 99;
                owner.spells.insert(99, [(61, 0); 3]);
            }
            let mut flags = load(row, &mut owner);
            assert_eq!(
                flags.quest_exploration(7).unwrap_err().id,
                "execution.quest_flags.quest_area_triggers_owner"
            );
            assert_eq!(
                flags.set_quest_exploration(7).unwrap_err().id,
                "execution.quest_flags.quest_area_triggers_owner"
            );
            flags
                .load_area_triggers([[999, 7], [10, 999]], &mut owner)
                .unwrap();
            assert_eq!(
                flags.flags(7),
                Some(if spell_sets || initial != 0 { 2 } else { 0 })
            );
            flags
                .load_area_triggers([[10, 7], [10, 7]], &mut owner)
                .unwrap();
            assert_eq!(flags.quest_exploration(7).unwrap(), Some(true));
            assert_eq!(flags.quest_exploration(999).unwrap(), None);
            assert_eq!(
                flags.mutation_counts()[4],
                usize::from(!spell_sets && initial == 0)
            );
        }
    }
    let mut owner = Owner::default();
    let mut flags = load(quest(), &mut owner);
    flags.load_area_triggers([], &mut owner).unwrap();
    assert_eq!(flags.quest_exploration(7).unwrap(), Some(false));
    flags.set_quest_exploration(7).unwrap();
    assert_eq!(flags.flags(7), Some(2));
}

#[test]
fn missing_dependency_owners_refuse_without_inventing_negative_results() {
    struct Missing;
    impl QuestFlagLoaderOwner for Missing {}
    let mut row = quest();
    row.required_spells[0] = 99;
    assert_eq!(
        SourceQuestFlags::load_quests([row], &mut Missing)
            .unwrap_err()
            .id,
        "execution.quest_flags.spell_template_owner"
    );
    let mut flags = load(quest(), &mut Missing);
    assert_eq!(
        flags
            .load_area_triggers([[10, 7]], &mut Missing)
            .unwrap_err()
            .id,
        "execution.quest_flags.area_trigger_dbc_owner"
    );
    assert_eq!(
        flags
            .load_area_triggers([[10, 999]], &mut Missing)
            .unwrap_err()
            .id,
        "execution.quest_flags.area_trigger_dbc_owner"
    );
}

#[test]
fn signed_target_negation_refuses_minimum_but_accepts_its_neighbor() {
    let mut row = quest();
    row.required_targets[0] = i32::MIN;
    assert_eq!(
        SourceQuestFlags::load_quests([row.clone()], &mut Owner::default())
            .unwrap_err()
            .id,
        "execution.quest_flags.target_negation_undefined"
    );
    row.required_targets[0] += 1;
    assert_eq!(load(row, &mut Owner::default()).flags(7), Some(48));
}

#[test]
fn quest_map_uses_first_duplicate_and_retains_high_database_flags() {
    let mut first = quest();
    first.special_flags = 0x8001;
    let mut second = quest();
    second.limit_time = 1;
    let flags = SourceQuestFlags::load_quests([first, second], &mut Owner::default()).unwrap();
    assert_eq!(flags.flags(7), Some(0x8001));
    assert_eq!(flags.quest_count(), 1);
}

#[test]
fn catalog04_quest_flag_mutation_census_or_explicit_absence() {
    use super::super::super::super::production_compiler::VerifiedCatalog;
    let Some(root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        println!("SKIP catalog-04 quest SpecialFlags census: BENILLA_F5_AI_CATALOG absent");
        return;
    };
    let mut catalog = VerifiedCatalog::open(Path::new(&root), false).unwrap();
    struct Missing;
    impl SpellLoaderOwner for Missing {}
    let spells = SourceSpellStores::load(&mut catalog, &mut Missing).unwrap();
    let raw = catalog
        .execution_quest_flag_operands(
            "quest_template",
            [
                "ReqCreatureOrGOId1",
                "ReqCreatureOrGOId2",
                "ReqCreatureOrGOId3",
                "ReqCreatureOrGOId4",
            ],
        )
        .unwrap();
    let undefined = raw
        .iter()
        .filter(|v| v.contains(&(i32::MIN as u32)))
        .count();
    assert_eq!(
        undefined, 0,
        "stock undefined target negation must stop before commit"
    );
    let flags = SourceQuestFlags::load_catalog(&mut catalog, Some(&spells)).unwrap();
    assert_eq!(flags.quest_count(), 4245);
    assert_eq!(flags.mutation_counts(), [2835, 0, 411, 29, 0]);
    println!("F5_QUEST_FLAGS_CENSUS quests={} changes_4513_4584_4618_4833_6290={:?} undefined_target_negation_quests={undefined}",
        flags.quest_count(), flags.mutation_counts());
    let occurrences = std::env::var_os("BENILLA_F5_DB_SCRIPT_OCCURRENCES")
        .expect("explicit catalog requires retained DB script cursor");
    let rows =
        catalog_db_script_rows(&mut catalog, Path::new(&occurrences), DbScriptType::Relay).unwrap();
    let relay = rows
        .into_iter()
        .filter(|r| r.id == 720702)
        .collect::<Vec<_>>();
    assert!(!relay.is_empty());
    let expected = relay.len();
    assert_eq!(relay.iter().filter(|r| r.command == 7).count(), 1);
    impl DbScriptLoaderOwner for Missing {}
    let mut external = Missing;
    let mut owner = DbScriptCatalog::new(&mut catalog, Some(&spells), &mut external, None);
    let report = LoadedDbScriptMap::load(
        DbScriptType::Relay,
        relay,
        &SourceDbScriptTemplates::default(),
        None,
        &mut owner,
    )
    .unwrap();
    assert_eq!(report.loaded_count(), expected);
    println!("Relay 720702 validated against post-area-trigger quest flags");
}
