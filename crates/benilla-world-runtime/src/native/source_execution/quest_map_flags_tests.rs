use super::*;
use quest_map_test_inputs::{load_script_quest_catalog, script_quest_recording};

const DELIVER: u32 = 0x08;
const KILL_SPEAK: u32 = 0x30;
const TIMED: u32 = 0x40;

fn flags(map: &SourceQuestMap) -> u32 {
    map.template(7).unwrap().special_flags
}

fn flag_mutations(before: u32, after: u32) -> [usize; 4] {
    [DELIVER, EXPLORATION, KILL_SPEAK, TIMED].map(|mask| usize::from(before & mask != after & mask))
}

#[test]
fn quest_activity_starts_active_and_refuses_missing_quest_without_inserting_it() {
    let mut map = load([row(7), row(8)], &mut Inputs::default());
    assert!(map.templates().all(SourceQuestTemplate::is_active));
    map.set_active(7, false).unwrap();
    assert!(!map.template(7).unwrap().is_active());
    assert_eq!(
        map.set_active(9, true).unwrap_err().id,
        "execution.event.quest_definition_missing"
    );
    assert!(map.template(9).is_none());
    assert!(!map.template(7).unwrap().is_active());
    assert!(map.template(8).unwrap().is_active());
    map.set_active(7, true).unwrap();
    assert!(map.templates().all(SourceQuestTemplate::is_active));
}

#[test]
fn deliver_sets_before_item_validation_and_preserves_already_set_flags() {
    for initial in [0, DELIVER] {
        let map = load(
            [SourceQuestTemplate {
                special_flags: initial,
                required_items: [999999, 1, 0, 0],
                ..row(7)
            }],
            &mut Inputs::default(),
        );
        assert_eq!(flags(&map), 8);
        assert_eq!(
            flag_mutations(initial, flags(&map)),
            [usize::from(initial == 0), 0, 0, 0]
        );
    }
    assert_eq!(flags(&load([row(7)], &mut Inputs::default())), 0);
}

#[test]
fn required_spell_effects_set_only_matching_quest_complete_or_send_event() {
    for effect in [[(16, 7), (0, 0), (0, 0)], [(0, 0), (0, 0), (61, -1)]] {
        for initial in [0, EXPLORATION] {
            let mut owner = Inputs::default();
            owner.spells.insert(99, effect);
            let map = load(
                [SourceQuestTemplate {
                    special_flags: initial,
                    required_spells: [99, 99, 0, 0],
                    ..row(7)
                }],
                &mut owner,
            );
            assert_eq!(flags(&map), 2);
            assert_eq!(
                flag_mutations(initial, flags(&map)),
                [0, usize::from(initial == 0), 0, 0]
            );
            assert_eq!(owner.spell_reads, [99, 99]);
        }
    }
    for effects in [None, Some([(16, 8), (16, -1), (0, 0)])] {
        let mut owner = Inputs::default();
        if let Some(effects) = effects {
            owner.spells.insert(99, effects);
        }
        let mut quest = row(7);
        quest.required_spells[0] = 99;
        assert_eq!(flags(&load([quest], &mut owner)), 0);
    }
    let mut quest = row(7);
    quest.required_spells[0] = 99;
    quest.required_targets[0] = 123;
    let mut owner = Inputs::default();
    owner.spells.insert(99, [(61, 0); 3]);
    assert_eq!(flags(&load([quest], &mut owner)), 48);
    assert_eq!(owner.spell_reads, [99]);
}

#[test]
fn cached_nonzero_target_sets_both_flags_even_when_target_would_be_missing() {
    for target in [-999999, 999999] {
        for initial in [0, 0x10, 0x20, KILL_SPEAK] {
            let mut quest = row(7);
            quest.special_flags = initial;
            quest.required_targets = [target; 4];
            let map = load([quest], &mut Inputs::default());
            assert_eq!(map.template(7).unwrap().required_targets, [0; 4]);
            assert_eq!(flags(&map), 48);
            assert_eq!(
                flag_mutations(initial, flags(&map)),
                [0, 0, usize::from(initial != KILL_SPEAK), 0]
            );
        }
    }
}

#[test]
fn timed_sets_for_any_nonzero_limit_and_preserves_already_set_flag() {
    for limit_time in [1, u32::MAX] {
        for initial in [0, TIMED] {
            let mut quest = row(7);
            quest.special_flags = initial;
            quest.limit_time = limit_time;
            let map = load([quest], &mut Inputs::default());
            assert_eq!(flags(&map), 64);
            assert_eq!(
                flag_mutations(initial, flags(&map)),
                [0, 0, 0, usize::from(initial == 0)]
            );
        }
    }
}

#[test]
fn area_triggers_follow_quests_and_precede_scriptmgr_reads_and_mutations() {
    const PHASE: &str = "execution.quests.World_1146.quest_area_triggers_owner";
    for spell_sets in [false, true] {
        for initial in [0, EXPLORATION] {
            let mut quest = row(7);
            quest.special_flags = initial;
            let mut owner = Inputs::default();
            if spell_sets {
                quest.required_spells[0] = 99;
                owner.spells.insert(99, [(61, 0); 3]);
            }
            let mut map = load([quest, row(8)], &mut owner);
            assert_eq!(map.area_trigger_quest(10).unwrap_err().id, PHASE);
            assert_eq!(map.quest_exploration(7).unwrap_err().id, PHASE);
            assert_eq!(map.set_quest_exploration(7).unwrap_err().id, PHASE);
            assert!(DbScriptLoaderOwner::contains(&mut map, DbScriptLookup::Quest, 7).unwrap());
            assert!(!DbScriptLoaderOwner::contains(&mut map, DbScriptLookup::Quest, 999).unwrap());
            map.load_area_triggers([[999, 999], [10, 999]], |trigger| Ok(trigger == 10))
                .unwrap();
            assert!(lines(&map).ends_with(&[6274, 6281]));
            assert_eq!(map.area_trigger_quest(10).unwrap(), None);
            assert_eq!(flags(&map), if spell_sets || initial != 0 { 2 } else { 0 });
            let before = flags(&map);
            map.load_area_triggers([[10, 7], [10, 7], [10, 8], [10, 999]], |_| Ok(true))
                .unwrap();
            assert_eq!(map.area_trigger_quest(10).unwrap(), Some(8));
            assert_eq!(map.quest_exploration(7).unwrap(), Some(true));
            assert_eq!(map.quest_exploration(8).unwrap(), Some(true));
            assert_eq!(map.quest_exploration(999).unwrap(), None);
            assert_eq!(
                map.notices()
                    .iter()
                    .filter(|n| n.line == 6287 && n.quest == 7)
                    .count(),
                usize::from(!spell_sets && initial == 0)
            );
            assert_eq!(before != flags(&map), !spell_sets && initial == 0);
            map.load_area_triggers([], |_| panic!("empty query must not read the DBC"))
                .unwrap();
            assert_eq!(map.area_trigger_quest(10).unwrap(), None);
            assert_eq!(map.quest_exploration(8).unwrap(), Some(true));
        }
    }
    let mut map = load([row(7)], &mut Inputs::default());
    map.load_area_triggers([], |_| panic!("empty query must not read the DBC"))
        .unwrap();
    assert_eq!(map.quest_exploration(7).unwrap(), Some(false));
    let command = DbScriptRow {
        id: 1,
        command: 7,
        data: [7, 101, 0],
        ..Default::default()
    };
    let script = LoadedDbScriptMap::load(
        DbScriptType::Relay,
        [command],
        &SourceDbScriptTemplates::default(),
        None,
        &mut map,
    )
    .unwrap();
    assert_eq!(script.loaded_count(), 0);
    assert_eq!(
        script.rows()[0]
            .logs
            .iter()
            .map(|log| log.reason)
            .collect::<Vec<_>>(),
        ["quest_exploration_flag_set", "quest_distance_too_large"]
    );
    assert_eq!(flags(&map), 2);
}

#[test]
fn missing_dependency_owners_refuse_without_inventing_negative_results() {
    struct Missing;
    impl QuestLoaderOwner for Missing {}
    let mut quest = row(7);
    quest.required_spells[0] = 99;
    assert_eq!(
        SourceQuestMap::load_quests([quest], &mut Missing)
            .unwrap_err()
            .id,
        "execution.quests.ObjectMgr_4556.spell_template_owner"
    );
    let mut map = load([row(7)], &mut Inputs::default());
    for quest in [7, 999] {
        assert_eq!(
            map.load_area_triggers([[10, quest]], |_| Err(unbound()))
                .unwrap_err()
                .id,
            "execution.quests.ObjectMgr_6271.area_trigger_dbc_owner"
        );
    }
    assert_eq!(
        map.area_trigger_quest(10).unwrap_err().id,
        "execution.quests.World_1146.quest_area_triggers_owner"
    );
    assert_eq!(flags(&map), 0);
}

#[test]
fn signed_target_negation_refuses_minimum_but_accepts_its_neighbor() {
    let quest = |target| SourceQuestTemplate {
        required_targets: [target, 0, 0, 0],
        ..row(7)
    };
    assert_eq!(
        SourceQuestMap::load_quests([quest(i32::MIN)], &mut Inputs::default())
            .unwrap_err()
            .id,
        "execution.quests.ObjectMgr_4600.target_negation_undefined"
    );
    assert_eq!(
        flags(&load([quest(i32::MIN + 1)], &mut Inputs::default())),
        48
    );
}

#[test]
fn quest_map_uses_first_duplicate_and_retains_high_database_flags() {
    let mut first = row(7);
    first.special_flags = 0x8001;
    let mut second = row(7);
    second.limit_time = 1;
    let map = load([first, second], &mut Inputs::default());
    assert_eq!(flags(&map), 0x8001);
    assert_eq!(map.quest_count(), 1);
}

#[test]
fn catalog04_quest_flag_mutation_census_or_explicit_absence() {
    use crate::native::production_compiler::VerifiedCatalog;
    let (Some(root), Some((config, trace))) = (
        std::env::var_os("BENILLA_F5_AI_CATALOG"),
        script_quest_recording(),
    ) else {
        println!("SKIP catalog-04 quest SpecialFlags census: set BENILLA_F5_AI_CATALOG, BENILLA_F5_EVENT_AI_CONFIG and BENILLA_F5_EVENT_AI_TRACE");
        return;
    };
    let mut catalog = VerifiedCatalog::open(Path::new(&root), false).unwrap();
    struct Missing;
    impl SpellLoaderOwner for Missing {}
    impl DbScriptLoaderOwner for Missing {}
    let spells = SourceSpellStores::load(&mut catalog, &mut Missing).unwrap();
    let mut raw = BTreeMap::new();
    let mut undefined = 0;
    catalog
        .scan_execution_quest_templates(|quest| {
            undefined += usize::from(quest.required_targets.contains(&i32::MIN));
            assert!(raw.insert(quest.id, quest.special_flags).is_none());
            Ok(())
        })
        .unwrap();
    assert_eq!(
        undefined, 0,
        "stock undefined target negation must stop before commit"
    );
    let mut map = load_script_quest_catalog(&mut catalog, &spells, &config, &trace);
    assert_eq!(map.quest_count(), 4245);
    let mut counts = [0; 5];
    for quest in map.templates() {
        let raw = raw.get_mut(&quest.id).unwrap();
        for (sum, change) in counts
            .iter_mut()
            .zip(flag_mutations(*raw, quest.special_flags))
        {
            *sum += change;
        }
        *raw = quest.special_flags;
    }
    assert_eq!(raw.len(), map.quest_count());
    let triggers = quest_map_test_inputs::recorded_area_triggers(&trace);
    let mut catalog_rows = catalog.execution_quest_area_trigger_rows().unwrap();
    let mut recorded_rows = triggers.clone();
    // Stock SQL :19 keys rows by trigger ID. The un-ordered SELECT uses
    // the recorded physical order, which differs from the retained scan.
    catalog_rows.sort_unstable();
    recorded_rows.sort_unstable();
    assert_eq!(catalog_rows, recorded_rows);
    assert!(catalog_rows.windows(2).all(|rows| rows[0][0] < rows[1][0]));
    map.load_catalog_area_triggers(&mut catalog).unwrap();
    counts[4] = map
        .templates()
        .filter(|q| q.special_flags & EXPLORATION != raw[&q.id] & EXPLORATION)
        .count();
    assert_eq!(counts, [2835, 0, 411, 29, 0]);
    assert!(map
        .notices()
        .iter()
        .all(|n| !matches!(n.line, 6274 | 6281 | 6287)));
    for [trigger, quest] in triggers {
        assert_eq!(map.area_trigger_quest(trigger).unwrap(), Some(quest));
    }
    println!("F5_QUEST_FLAGS_CENSUS quests={} changes_4513_4584_4618_4833_6290={counts:?} undefined_target_negation_quests={undefined}", map.quest_count());
    let occurrences = std::env::var_os("BENILLA_F5_DB_SCRIPT_OCCURRENCES")
        .expect("explicit catalog requires retained DB script cursor");
    let relay: Vec<_> =
        catalog_db_script_rows(&mut catalog, Path::new(&occurrences), DbScriptType::Relay)
            .unwrap()
            .into_iter()
            .filter(|r| r.id == 720702)
            .collect();
    assert!(!relay.is_empty());
    let expected = relay.len();
    assert_eq!(relay.iter().filter(|r| r.command == 7).count(), 1);
    assert_eq!(map.quest_exploration(2608).unwrap(), Some(true));
    let report = LoadedDbScriptMap::load(
        DbScriptType::Relay,
        relay,
        &SourceDbScriptTemplates::default(),
        None,
        &mut DbScriptCatalog::new(&mut catalog, Some(&spells), &mut map, &mut Missing, None),
    )
    .unwrap();
    assert_eq!(report.loaded_count(), expected);
    assert_eq!(map.quest_exploration(2608).unwrap(), Some(true));
    println!("Relay 720702 validated against post-area-trigger quest flags");
}
