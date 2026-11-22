use super::*;
#[path = "quest_map_evidence_tests.rs"]
mod evidence;
#[path = "quest_map_flags_tests.rs"]
mod flags;

use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
struct Inputs {
    present: BTreeSet<(QuestLookup, u32)>,
    gameobjects: BTreeMap<u32, u32>,
    spells: BTreeMap<u32, [(u32, i32); 3]>,
    spell_reads: Vec<u32>,
    invalid: BTreeSet<u32>,
    reads: Vec<(QuestLookup, u32)>,
}

impl QuestLoaderOwner for Inputs {
    fn contains(&mut self, kind: QuestLookup, id: u32) -> Result<bool> {
        self.reads.push((kind, id));
        Ok(self.present.contains(&(kind, id)))
    }
    fn gameobject_type(&mut self, id: u32) -> Result<Option<u32>> {
        Ok(self.gameobjects.get(&id).copied())
    }
    fn spell_effects(&mut self, id: u32) -> Result<Option<[(u32, i32); 3]>> {
        self.spell_reads.push(id);
        Ok(self.spells.get(&id).copied())
    }
    fn spell_valid(&mut self, id: u32, _: &mut Vec<SpellValidityNotice>) -> Result<bool> {
        Ok(!self.invalid.contains(&id))
    }
    fn max_skill(&mut self) -> Result<u16> {
        Ok(300)
    }
}

fn row(id: u32) -> SourceQuestTemplate {
    SourceQuestTemplate {
        id,
        ..Default::default()
    }
}

fn load(rows: impl IntoIterator<Item = SourceQuestTemplate>, input: &mut Inputs) -> SourceQuestMap {
    SourceQuestMap::load_quests(rows, input).unwrap()
}

#[test]
fn relation_load_preserves_duplicates_and_bad_givers_but_skips_missing_quests() {
    use QuestRelationKind::*;
    let mut input = Inputs::default();
    input.present.insert((QuestLookup::Creature, 20));
    input.gameobjects.extend([(30, 2), (31, 3)]);
    let mut quests = load([row(1), row(2)], &mut input);
    let mut calls = Vec::new();
    quests
        .load_quest_relations(
            |kind| {
                calls.push(kind);
                Ok(match kind {
                    GameObjectStart => vec![[31, 1], [30, 2], [30, 1], [30, 2], [32, 99]],
                    GameObjectEnd => vec![[32, 2], [31, 1]],
                    CreatureStart => vec![[20, 1], [21, 2]],
                    CreatureEnd => vec![[22, 1], [20, 2]],
                })
            },
            &mut input,
        )
        .unwrap();
    assert_eq!(
        calls,
        [GameObjectStart, GameObjectEnd, CreatureStart, CreatureEnd]
    );
    assert_eq!(quests.relations(GameObjectStart, 30), &[2, 1, 2]);
    assert_eq!(quests.relations(GameObjectStart, 31), &[1]);
    assert_eq!(quests.relations(GameObjectStart, 32), &[] as &[u32]);
    assert_eq!(quests.relations(GameObjectEnd, 32), &[2]);
    assert_eq!(quests.relations(CreatureStart, 21), &[2]);
    assert_eq!(quests.relations(CreatureEnd, 22), &[1]);
    assert_eq!(quests.relations(CreatureEnd, 20), &[2]);
    assert_eq!(
        quests.notices().iter().map(|n| n.line).collect::<Vec<_>>(),
        [7971, 7995, 8009, 8007, 8021, 8033]
    );
    quests
        .load_quest_relations(|_| Ok(Vec::new()), &mut input)
        .unwrap();
    assert!(quests.relations(GameObjectStart, 30).is_empty());
    assert!(quests.relations(CreatureEnd, 20).is_empty());
}

#[test]
fn quest_links_follow_reference_iteration_and_keep_signs_and_duplicate_predecessors() {
    let mut a = row(1);
    a.next_quest_id = -2;
    a.next_quest_in_chain = 2;
    a.exclusive_group = -40;
    let mut b = row(2);
    b.prev_quest_id = -1;
    b.exclusive_group = -40;
    let mut c = row(3);
    c.breadcrumb_for_quest_id = 2;
    let mut d = row(4);
    d.breadcrumb_for_quest_id = 3;
    let mut duplicate = row(2);
    duplicate.next_quest_id = 3;
    let map = load([a, b, c, d, duplicate], &mut Inputs::default());
    assert_eq!(map.quest_count(), 4);
    let b = map.template(2).unwrap();
    assert_eq!(b.prev_quests(), &[-1, -1]);
    assert_eq!(b.prev_chain_quests(), &[1]);
    assert_eq!(b.dependent_breadcrumbs(), &[4, 3]);
    assert_eq!(map.template(3).unwrap().dependent_breadcrumbs(), &[4]);
    assert_eq!(map.exclusive_group(-40), &[2, 1]);
    assert!(map.template(99).is_none());
    assert!(map.exclusive_group(40).is_empty());
}

#[test]
fn source_lookup_refuses_at_the_first_reached_call() {
    struct Unbound;
    impl QuestLoaderOwner for Unbound {}
    assert!(SourceQuestMap::load_quests([row(1)], &mut Unbound).is_ok());
    for (line, field, changed) in [
        (
            4336,
            "area_dbc",
            SourceQuestTemplate {
                zone_or_sort: 9,
                ..row(1)
            },
        ),
        (
            4346,
            "quest_sort_dbc",
            SourceQuestTemplate {
                zone_or_sort: -9,
                ..row(1)
            },
        ),
        (
            4389,
            "skill_line_dbc",
            SourceQuestTemplate {
                required_skill: 1,
                ..row(1)
            },
        ),
        (
            4398,
            "max_skill_config",
            SourceQuestTemplate {
                required_skill_value: 1,
                ..row(1)
            },
        ),
        (
            4407,
            "faction_dbc",
            SourceQuestTemplate {
                rep_objective_faction: 1,
                ..row(1)
            },
        ),
        (
            4414,
            "faction_dbc",
            SourceQuestTemplate {
                required_min_rep_faction: 1,
                ..row(1)
            },
        ),
        (
            4421,
            "faction_dbc",
            SourceQuestTemplate {
                required_max_rep_faction: 1,
                ..row(1)
            },
        ),
        (
            4465,
            "item_template",
            SourceQuestTemplate {
                source_item: 1,
                ..row(1)
            },
        ),
        (
            4487,
            "spell_template",
            SourceQuestTemplate {
                source_spell: 1,
                ..row(1)
            },
        ),
        (
            4515,
            "item_template",
            SourceQuestTemplate {
                required_items: [1; 4],
                ..row(1)
            },
        ),
        (
            4534,
            "item_template",
            SourceQuestTemplate {
                required_source_items: [1; 4],
                ..row(1)
            },
        ),
        (
            4556,
            "spell_template",
            SourceQuestTemplate {
                required_spells: [1; 4],
                ..row(1)
            },
        ),
        (
            4600,
            "gameobject_template",
            SourceQuestTemplate {
                required_targets: [-1; 4],
                ..row(1)
            },
        ),
        (
            4607,
            "creature_template",
            SourceQuestTemplate {
                required_targets: [1; 4],
                ..row(1)
            },
        ),
        (
            4640,
            "item_template",
            SourceQuestTemplate {
                choice_items: [1; 6],
                ..row(1)
            },
        ),
        (
            4676,
            "item_template",
            SourceQuestTemplate {
                reward_items: [1; 4],
                ..row(1)
            },
        ),
        (
            4706,
            "faction_dbc",
            SourceQuestTemplate {
                reward_factions: [1; 5],
                ..row(1)
            },
        ),
        (
            4723,
            "spell_template",
            SourceQuestTemplate {
                reward_spell: 1,
                ..row(1)
            },
        ),
        (
            4747,
            "spell_template",
            SourceQuestTemplate {
                reward_spell_cast: 1,
                ..row(1)
            },
        ),
        (
            4771,
            "mail_template_dbc",
            SourceQuestTemplate {
                reward_mail_template: 1,
                ..row(1)
            },
        ),
    ] {
        let err = SourceQuestMap::load_quests([changed], &mut Unbound).unwrap_err();
        assert_eq!(
            err.id,
            format!("execution.quests.ObjectMgr_{line}.{field}_owner")
        );
    }
}

fn lines(map: &SourceQuestMap) -> Vec<u32> {
    map.notices().iter().map(|notice| notice.line).collect()
}

#[test]
fn quest_constructor_reads_source_types_and_cached_counts() {
    let mut cells: [Option<&[u8]>; 131] = [None; 131];
    for (column, value) in [
        (0, &b"41"[..]),
        (12, b"-3"),
        (19, b"65545"),
        (20, b"65536"),
        (23, b"-2"),
        (28, b"title\0ignored"),
        (30, b"\x80"),
        (34, b"first"),
        (38, b"3"),
        (42, b"2"),
        (54, b"-4"),
        (58, b"2"),
        (66, b"3"),
        (72, b"2"),
        (78, b"3"),
        (82, b"2"),
        (96, b"-11"),
        (103, b"0.1"),
        (104, b"-2.5"),
        (108, b"5"),
        (118, b"-1"),
        (122, b"-2"),
        (126, b"33"),
        (127, b"34"),
        (128, b"35"),
        (130, b"7"),
    ] {
        cells[column] = Some(value);
    }
    let mut input = Inputs::default();
    input.gameobjects.insert(4, 2);
    let map = load(
        [SourceQuestTemplate::from_sql_fields(cells).unwrap()],
        &mut input,
    );
    let q = map.template(41).unwrap();
    assert_eq!((q.quest_flags, q.special_flags), (9, 0x38));
    assert_eq!(
        (
            q.rep_objective_value,
            q.exclusive_group,
            q.reward_or_required_money
        ),
        (-3, -2, -11)
    );
    assert_eq!(
        (&*q.title, &*q.objectives, &*q.objective_text[0]),
        (&b"title"[..], &b"\x80"[..], &b"first"[..])
    );
    assert!(q.details.is_empty());
    assert_eq!(q.point_x.to_bits(), 0.1_f32.to_bits());
    assert_eq!(q.point_y, -2.5);
    assert_eq!(q.offer_reward_emotes[0], u32::MAX);
    assert_eq!(q.offer_reward_emote_delays[0], u32::MAX - 1);
    assert_eq!(
        (
            q.start_script,
            q.complete_script,
            q.required_condition,
            q.reward_faction_flags
        ),
        (33, 34, 35, 7)
    );
    assert_eq!(
        (
            q.required_items_count(),
            q.required_targets_count(),
            q.choice_items_count(),
            q.reward_items_count(),
            q.details_emotes_count()
        ),
        (1, 1, 1, 1, 3)
    );
    assert_eq!(q.choice_items[0], 0);
    assert_eq!(q.reward_items[0], 0);
    assert_eq!(q.required_item_counts[0], 0);
}

#[test]
fn quest_header_validates_masks_skills_and_reputation_in_source_order() {
    use QuestLookup::*;
    let mut owner = Inputs::default();
    let mut q = row(1);
    q.method = 3;
    q.special_flags = 4;
    q.quest_flags = 0x400;
    q.choice_items[1] = 20;
    q.choice_item_counts[1] = 1;
    q.zone_or_sort = -24;
    q.required_classes = 0x20;
    q.required_races = 0x100;
    q.required_skill = 100;
    q.required_skill_value = 301;
    q.rep_objective_faction = 10;
    q.required_min_rep_faction = 11;
    q.required_max_rep_faction = 12;
    q.required_min_rep_value = 43_000;
    q.required_max_rep_value = 43_000;
    let map = load([q], &mut owner);
    assert_eq!(
        lines(&map),
        [
            4311, 4316, 4326, 4349, 4359, 4371, 4381, 4391, 4400, 4409, 4416, 4423, 4430, 4437,
            4642
        ]
    );
    assert_eq!(
        owner.reads,
        [
            (QuestSort, 24),
            (SkillLine, 100),
            (Faction, 10),
            (Faction, 11),
            (Faction, 12),
            (Item, 20)
        ]
    );
    let q = map.template(1).unwrap();
    assert_eq!(
        (
            q.method,
            q.special_flags,
            q.required_classes,
            q.required_races
        ),
        (3, 4, 0, 0)
    );
    assert_eq!(
        (
            q.required_skill,
            q.required_skill_value,
            q.required_min_rep_value
        ),
        (100, 301, 43_000)
    );

    owner.present.extend([
        (QuestSort, 24),
        (SkillLine, 182),
        (Faction, 10),
        (Faction, 11),
        (Faction, 12),
        (Area, 9),
    ]);
    let mut q = row(1);
    q.method = 2;
    q.special_flags = 3;
    q.quest_flags = 0x400;
    q.zone_or_sort = -24;
    q.required_classes = 0x60;
    q.required_races = 0x101;
    q.required_skill = 182;
    q.required_skill_value = 300;
    q.rep_objective_faction = 10;
    q.required_min_rep_faction = 11;
    q.required_max_rep_faction = 12;
    q.required_min_rep_value = 42_998;
    q.required_max_rep_value = 42_999;
    let mut no_factions = row(2);
    no_factions.zone_or_sort = 9;
    no_factions.rep_objective_value = 1;
    no_factions.required_min_rep_value = 1;
    no_factions.required_max_rep_value = 2;
    let map = load([q, no_factions], &mut owner);
    assert_eq!(lines(&map), [4444, 4451, 4458]);
    let q = map.template(1).unwrap();
    assert_eq!((q.required_classes, q.required_races), (0x60, 0x101));
    let map = load(
        [SourceQuestTemplate {
            zone_or_sort: 10,
            ..row(3)
        }],
        &mut owner,
    );
    assert_eq!(lines(&map), [4338]);
}

#[test]
fn quest_sources_and_objectives_use_cached_ids_and_preserve_log_only_fields() {
    use QuestLookup::*;
    let mut owner = Inputs::default();
    owner.present.extend([(Item, 11), (Creature, 22)]);
    owner.gameobjects.insert(33, 2);
    owner.spells.extend([
        (11, [(16, 7), (0, 0), (0, 0)]),
        (12, [(16, 8), (0, 0), (0, 0)]),
        (13, [(0, 0), (61, -1), (0, 0)]),
    ]);
    owner.invalid.insert(12);
    let q = SourceQuestTemplate {
        source_item: 11,
        source_spell: 12,
        required_items: [11, 12, 0, 11],
        required_item_counts: [0, 2, 3, 4],
        required_source_items: [12, 0, 11, 0],
        required_source_item_counts: [1, 2, 2, 0],
        required_spells: [11, 99, 12, 13],
        required_targets: [0, -44, 23, -33],
        required_target_counts: [7, 0, 1, 2],
        ..row(7)
    };
    let map = load([q], &mut owner);
    assert_eq!(
        lines(&map),
        [4473, 4496, 4508, 4517, 4524, 4536, 4545, 4581, 4559, 4629, 4602, 4622, 4609]
    );
    let q = map.template(7).unwrap();
    assert_eq!(
        (q.source_item, q.source_item_count, q.source_spell),
        (11, 1, 0)
    );
    assert_eq!(q.required_item_counts, [0, 0, 0, 4]);
    assert_eq!(q.required_source_item_counts, [1, 2, 2, 0]);
    assert_eq!(q.required_spells, [11, 99, 12, 13]);
    assert_eq!(q.required_targets, [0, 0, 0, -33]);
    assert_eq!(q.required_target_counts, [7, 0, 1, 2]);
    assert_eq!(q.required_targets_count(), 3);
    assert_eq!(q.special_flags, 0x3a);

    let map = load(
        [SourceQuestTemplate {
            source_item: 12,
            source_item_count: 8,
            source_spell: 99,
            required_targets: [22, 0, 0, 0],
            required_target_counts: [1, 0, 0, 0],
            required_spells: [0, 12, 13, 11],
            ..row(7)
        }],
        &mut owner,
    );
    assert_eq!(lines(&map), [4467, 4490, 4589, 4581]);
    let q = map.template(7).unwrap();
    assert_eq!(
        (q.source_item, q.source_item_count, q.required_targets[0]),
        (0, 8, 22)
    );

    let map = load(
        [SourceQuestTemplate {
            source_item_count: 8,
            source_spell: 11,
            special_flags: 2,
            required_spells: [13, 0, 0, 0],
            ..row(7)
        }],
        &mut owner,
    );
    assert_eq!(lines(&map), [4480]);
    let q = map.template(7).unwrap();
    assert_eq!(
        (q.source_item_count, q.source_spell, q.special_flags),
        (0, 11, 2)
    );
    let map = load(
        [SourceQuestTemplate {
            source_item: 11,
            source_item_count: 2,
            ..row(7)
        }],
        &mut owner,
    );
    assert!(map.notices().is_empty());
}

#[test]
fn quest_rewards_clear_or_repair_only_the_fields_changed_by_source() {
    use QuestLookup::*;
    let mut owner = Inputs::default();
    owner.present.extend([
        (Item, 11),
        (Item, 12),
        (Faction, 13),
        (Talent, 22),
        (MailTemplate, 31),
    ]);
    owner
        .spells
        .extend([(21, [(0, 0); 3]), (22, [(0, 0); 3]), (23, [(0, 0); 3])]);
    owner.invalid.insert(23);
    let mut low = row(1);
    low.reward_mail_template = 31;
    low.reward_mail_delay = 80;
    low.reward_spell = 99;
    low.reward_spell_cast = 23;
    let mut high = row(2);
    high.choice_items = [11, 0, 12, 0, 99, 0];
    high.choice_item_counts = [0, 0, 2, 0, 1, 5];
    high.reward_items = [99, 11, 0, 12];
    high.reward_item_counts = [2, 0, 3, 2];
    high.reward_factions = [99, 13, 0, 13, 0];
    high.reward_rep_values = [0, 0, -1, 25, 0];
    high.reward_spell = 22;
    high.reward_spell_cast = 21;
    high.reward_mail_template = 31;
    high.reward_mail_delay = 45;
    let map = load([low, high], &mut owner);
    assert_eq!(
        lines(&map),
        [
            4666, 4642, 4658, 4651, 4678, 4685, 4692, 4703, 4708, 4703, 4715, 4739, 4727, 4757,
            4781
        ]
    );
    let q = map.template(2).unwrap();
    assert_eq!(q.choice_items, [11, 12, 12, 0, 0, 0]);
    assert_eq!(q.choice_item_counts, [0, 2, 2, 0, 1, 5]);
    assert_eq!(q.reward_items, [0, 11, 0, 12]);
    assert_eq!(q.reward_item_counts, [2, 0, 3, 2]);
    assert_eq!(q.reward_factions, [0, 13, 0, 13, 0]);
    assert_eq!(q.reward_rep_values, [0, 0, -1, 25, 0]);
    assert_eq!(
        (
            q.reward_spell,
            q.reward_spell_cast,
            q.reward_mail_template,
            q.reward_mail_delay
        ),
        (0, 21, 31, 45)
    );
    let q = map.template(1).unwrap();
    assert_eq!(
        (
            q.reward_spell,
            q.reward_spell_cast,
            q.reward_mail_template,
            q.reward_mail_delay
        ),
        (0, 0, 0, 0)
    );
    for (reward_spell, reward_spell_cast, expected) in
        [(23, 22, vec![4733, 4763, 4773]), (21, 99, vec![4751, 4773])]
    {
        let map = load(
            [SourceQuestTemplate {
                reward_spell,
                reward_spell_cast,
                reward_mail_template: 32,
                reward_mail_delay: 45,
                ..row(1)
            }],
            &mut owner,
        );
        assert_eq!(lines(&map), expected);
        let q = map.template(1).unwrap();
        assert_eq!((q.reward_mail_template, q.reward_mail_delay), (0, 0));
    }
}

#[test]
fn quest_link_validation_reports_missing_targets_and_cuts_breadcrumb_cycles() {
    let mut input = Inputs::default();
    let map = load([], &mut input);
    assert_eq!(lines(&map), [4279]);
    let mut map = load(
        [
            SourceQuestTemplate {
                next_quest_id: 2,
                breadcrumb_for_quest_id: 2,
                ..row(1)
            },
            SourceQuestTemplate {
                breadcrumb_for_quest_id: 1,
                ..row(2)
            },
            SourceQuestTemplate {
                prev_quest_id: 1,
                ..row(3)
            },
            SourceQuestTemplate {
                prev_quest_id: 98,
                next_quest_id: -97,
                next_quest_in_chain: 93,
                breadcrumb_for_quest_id: 96,
                limit_time: 3,
                ..row(4)
            },
        ],
        &mut input,
    );
    assert_eq!(lines(&map), [4795, 4808, 4820, 4840, 4810, 4846, 4864]);
    let q = map.template(4).unwrap();
    assert_eq!(
        (
            q.prev_quest_id,
            q.next_quest_id,
            q.next_quest_in_chain,
            q.breadcrumb_for_quest_id,
            q.special_flags
        ),
        (98, -97, 0, 0, 64)
    );
    assert_eq!(map.template(1).unwrap().dependent_breadcrumbs(), &[2]);
    assert_eq!(map.template(2).unwrap().dependent_breadcrumbs(), &[2, 1]);
    assert_eq!(map.template(2).unwrap().breadcrumb_for_quest_id, 0);
    assert_eq!(map.template(2).unwrap().prev_quests(), &[1]);
    assert!(map.template(3).unwrap().prev_quests().is_empty());
    assert!(map.templates().all(SourceQuestTemplate::is_active));
    map.set_active(3, false).unwrap();
    assert!(!map.template(3).unwrap().is_active());
    assert_eq!(
        map.set_active(99, false).unwrap_err().id,
        "execution.event.quest_definition_missing"
    );
    assert!(map.template(99).is_none());
    map.set_active(3, true).unwrap();
    assert!(map.template(3).unwrap().is_active());
}

#[test]
fn quest_signed_overflow_refuses_before_negating() {
    let input = &mut Inputs::default();
    for (q, id, line, tag) in [
        (
            SourceQuestTemplate {
                zone_or_sort: i32::MIN,
                ..row(7)
            },
            7,
            4346,
            "zone_or_sort_negation",
        ),
        (
            SourceQuestTemplate {
                required_targets: [i32::MIN, 0, 0, 0],
                ..row(7)
            },
            7,
            4600,
            "target_negation",
        ),
        (
            SourceQuestTemplate {
                prev_quest_id: i32::MIN,
                ..row(7)
            },
            7,
            4806,
            "prev_quest_abs",
        ),
        (
            SourceQuestTemplate {
                next_quest_id: i32::MIN,
                ..row(7)
            },
            7,
            4817,
            "next_quest_abs",
        ),
        (
            SourceQuestTemplate {
                next_quest_id: -9,
                ..row(i32::MIN as u32)
            },
            i32::MIN as u32,
            4824,
            "quest_id_negation",
        ),
    ] {
        let next = SourceQuestTemplate {
            zone_or_sort: q
                .zone_or_sort
                .saturating_add(i32::from(q.zone_or_sort == i32::MIN)),
            required_targets: q
                .required_targets
                .map(|v| v.saturating_add(i32::from(v == i32::MIN))),
            prev_quest_id: q
                .prev_quest_id
                .saturating_add(i32::from(q.prev_quest_id == i32::MIN)),
            next_quest_id: q
                .next_quest_id
                .saturating_add(i32::from(q.next_quest_id == i32::MIN)),
            ..row(if id == 7 { 7 } else { id + 1 })
        };
        let refusal = SourceQuestMap::load_quests([q, row(9)], input).unwrap_err();
        assert_eq!(
            refusal.id,
            format!("execution.quests.ObjectMgr_{line}.{tag}_undefined")
        );
        assert!(SourceQuestMap::load_quests([next, row(9)], input).is_ok());
    }
}

#[test]
fn spell_validity_and_talent_are_separate_ordered_owners() {
    struct SpellOwner {
        present: bool,
        validity: Option<bool>,
    }
    impl QuestLoaderOwner for SpellOwner {
        fn spell_effects(&mut self, _: u32) -> Result<Option<[(u32, i32); 3]>> {
            Ok(self.present.then_some([(0, 0); 3]))
        }
        fn spell_valid(&mut self, _: u32, _: &mut Vec<SpellValidityNotice>) -> Result<bool> {
            self.validity.ok_or_else(unbound)
        }
    }
    for (q, spell_validity_line, talent_line) in [
        (
            SourceQuestTemplate {
                source_spell: 5,
                ..row(1)
            },
            4494,
            None,
        ),
        (
            SourceQuestTemplate {
                reward_spell: 5,
                ..row(1)
            },
            4731,
            Some(4737),
        ),
        (
            SourceQuestTemplate {
                reward_spell_cast: 5,
                ..row(1)
            },
            4755,
            Some(4761),
        ),
    ] {
        let make_row = || SourceQuestTemplate {
            source_spell: q.source_spell,
            reward_spell: q.reward_spell,
            reward_spell_cast: q.reward_spell_cast,
            ..row(1)
        };
        assert!(SourceQuestMap::load_quests(
            [make_row()],
            &mut SpellOwner {
                present: false,
                validity: None
            }
        )
        .is_ok());
        assert_eq!(
            SourceQuestMap::load_quests(
                [make_row()],
                &mut SpellOwner {
                    present: true,
                    validity: None
                }
            )
            .unwrap_err()
            .id,
            format!("execution.quests.ObjectMgr_{spell_validity_line}.spell_validity_owner")
        );
        assert!(SourceQuestMap::load_quests(
            [make_row()],
            &mut SpellOwner {
                present: true,
                validity: Some(false)
            }
        )
        .is_ok());
        let checked = SourceQuestMap::load_quests(
            [q],
            &mut SpellOwner {
                present: true,
                validity: Some(true),
            },
        );
        if let Some(line) = talent_line {
            assert_eq!(
                checked.unwrap_err().id,
                format!("execution.quests.ObjectMgr_{line}.talent_dbc_owner")
            );
        } else {
            assert_eq!(checked.unwrap().template(1).unwrap().source_spell, 5);
        }
    }
}

#[test]
fn relation_loader_refuses_only_after_inserting_an_existing_quest() {
    struct Unbound;
    impl QuestLoaderOwner for Unbound {}
    for (kind, line, input) in [
        (
            QuestRelationKind::GameObjectStart,
            7991,
            "gameobject_template",
        ),
        (
            QuestRelationKind::GameObjectEnd,
            8005,
            "gameobject_template",
        ),
        (QuestRelationKind::CreatureStart, 8019, "creature_template"),
        (QuestRelationKind::CreatureEnd, 8031, "creature_template"),
    ] {
        let mut map = SourceQuestMap::load_quests([row(1)], &mut Unbound).unwrap();
        map.load_quest_relations(|_| Ok(vec![[5, 2]]), &mut Unbound)
            .unwrap();
        assert_eq!(lines(&map), [7971; 4]);
        let error = map
            .load_quest_relations(
                |table| {
                    Ok(if table == kind {
                        vec![[5, 1]]
                    } else {
                        Vec::new()
                    })
                },
                &mut Unbound,
            )
            .unwrap_err();
        assert_eq!(
            error.id,
            format!("execution.quests.ObjectMgr_{line}.{input}_owner")
        );
        assert_eq!(map.relations(kind, 5), &[1]);
    }
}
