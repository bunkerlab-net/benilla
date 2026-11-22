// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored closed short chain, not the real 15-to-script21 Northshire catalog.
use super::{synthetic_distinct_quest_fixture as f, synthetic_fixture as base, *};
use benilla_sim::{quest_lifecycle as q, starter_init as init};

pub const TARGETS: [u64; 10] = [2, 10, 11, 12, 13, 14, 15, 16, 17, 18];
pub const MECHANIC_TARGETS: [u64; 32] = [
    2, 10, 11, 12, 13, 14, 15, 16, 17, 18, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33,
    34, 35, 36, 37, 38, 39, 40, 41,
];
pub fn setup() -> (
    Profile,
    init::StarterInput,
    QuestProgressionInput,
    q::CompiledOrdinaryQuests,
) {
    setup_with_mechanics(false)
}
pub fn setup_with_mechanics(
    enabled: bool,
) -> (
    Profile,
    init::StarterInput,
    QuestProgressionInput,
    q::CompiledOrdinaryQuests,
) {
    let targets: &[u64] = if enabled { &MECHANIC_TARGETS } else { &TARGETS };
    let (mut profile, initial, mut progression) = f::setup();
    profile.interactions = None;
    profile.source_visitors = None;
    profile.creature_capabilities = None;
    let giver = profile
        .npcs
        .iter_mut()
        .find(|n| n.ai.creature_id == f::GIVER)
        .unwrap();
    giver.entry = 197;
    let mut second = giver.clone();
    second.entry = 823;
    second.ai.creature_id = 4;
    second.ai.spawn = base::point(-3.0);
    profile.npcs.push(second);
    let target = profile
        .npcs
        .iter_mut()
        .find(|n| n.ai.creature_id == 2)
        .unwrap();
    target.ai.neutral_to_all = true;
    target.faction_template = 3;
    target.ai.spawn = base::point(3.0);
    let target = target.clone();
    for &spawn in targets.iter().skip(1) {
        let mut next = target.clone();
        next.entry = 60000 + spawn as u32;
        next.ai.creature_id = spawn;
        profile.npcs.push(next);
    }
    let mut motion = profile.source_motion.take().unwrap();
    motion.npcs.insert(4, motion.npcs[&f::GIVER].clone());
    for &spawn in targets.iter().skip(1) {
        motion.npcs.insert(spawn, motion.npcs[&2].clone());
    }
    let mut interactions = f::interaction_rows(&profile);
    for row in &mut interactions.npcs {
        row.npc_flags = if row.entry == 197 && enabled {
            3
        } else if matches!(row.entry, 197 | 823) {
            2
        } else {
            0
        };
    }
    profile.initialize_interactions(interactions).unwrap();
    profile.source_motion = Some(motion);
    profile.creature_capabilities = Some(
        CreatureCapabilityCatalog::validate_sources(
            synthetic_motion_fixture::creature_capability_sources(&profile),
        )
        .unwrap(),
    );
    profile.source_visitors = Some(
        SourceVisitorCatalog::validate_sources(synthetic_two_owner_fixture::visitor_input(
            &profile,
            profile.npcs.iter().map(|n| n.ai.creature_id).collect(),
        ))
        .unwrap(),
    );
    profile
        .initialize_combat_authority(
            profile
                .npcs
                .iter()
                .map(|npc| NpcCombatAuthoritySource {
                    entry: npc.entry,
                    extra_flags: 0,
                    static_flags1: 0,
                    static_flags2: 0,
                    static_flags3: 0,
                    static_flags4: 0,
                })
                .collect(),
        )
        .unwrap();
    profile
        .initialize_spell_authority(&initial, synthetic_spell_fixture::source(&profile))
        .unwrap();
    profile.initialize_facing_authority().unwrap();
    let mut rows = vec![];
    let chain = if enabled {
        vec![
            (783, 0, 7),
            (7, 783, 15),
            (15, 7, 21),
            (21, 15, 0),
            (3104, 7, 0),
        ]
    } else {
        vec![(783, 0, 7), (7, 783, 15), (15, 7, 0)]
    };
    for (id, previous, next) in chain {
        let mut row = progression.quests[0].clone();
        row.id = id;
        row.previous_quest = previous;
        row.next_in_chain = next;
        row.special_flags = 0;
        row.required_item = [0; 4];
        row.required_item_count = [0; 4];
        row.required_creature = [
            if id == 783 {
                0
            } else {
                f::HOSTILE_ENTRY as i32
            },
            0,
            0,
            0,
        ];
        row.required_creature_count = [if id == 783 { 0 } else { 10 }, 0, 0, 0];
        row.reward.xp.reward_money_max_level = if id == 783 { 24 } else { 102 };
        row.reward.reward_or_required_money = if id == 783 { 0 } else { 25 };
        if id == 21 {
            row.required_creature_count[0] = 12;
            row.excluded.complete_script = 71021;
        } else if id == 3104 {
            row.required_creature = [0; 4];
            row.required_creature_count = [0; 4];
            row.required_item = [9571, 0, 0, 0];
            row.required_item_count = [1, 0, 0, 0];
            row.excluded.source_item = 9571;
            row.excluded.source_item_count = 1;
        }
        rows.push(row);
    }
    let mut starters = vec![
        q::CreatureQuestRelation {
            creature: 823,
            quest: 783,
        },
        q::CreatureQuestRelation {
            creature: 197,
            quest: 7,
        },
        q::CreatureQuestRelation {
            creature: 197,
            quest: 15,
        },
    ];
    let mut finishers = [783, 7, 15]
        .map(|quest| q::CreatureQuestRelation {
            creature: 197,
            quest,
        })
        .to_vec();
    if enabled {
        starters.extend([21, 3104].map(|quest| q::CreatureQuestRelation {
            creature: 197,
            quest,
        }));
        finishers.extend([21, 3104].map(|quest| q::CreatureQuestRelation {
            creature: 197,
            quest,
        }));
    }
    let captured = q::CapturedOrdinaryQuests {
        source: [37; 32],
        quests: Some(rows.clone()),
        starters: Some(starters),
        finishers: Some(finishers),
        givers: Some(
            [(3, 197), (4, 823)]
                .map(|(spawn, entry)| q::CapturedQuestGiver {
                    spawn,
                    database_guid: spawn as u32,
                    generation: 0,
                    entry,
                    npc_flags: if entry == 197 && enabled { 3 } else { 2 },
                    unit_flags: 0,
                    script_name: String::new(),
                    loaded_script: 0,
                })
                .to_vec(),
        ),
    };
    let dependencies = q::QuestDependencySource {
        source: [37; 32],
        mechanics: enabled.then(authored_mechanics),
        scripts: Some(vec![]),
        gossip: Some(
            [197, 823]
                .map(|entry| q::CapturedGossipScope {
                    entry,
                    menu: if entry == 197 && enabled { 44001 } else { 0 },
                    options: if enabled { vec![0] } else { vec![] },
                })
                .to_vec(),
        ),
        allowed_areas: vec![9],
        global_spell_areas: Some(vec![]),
        item_keys: if enabled { vec![9571] } else { vec![] },
        item_deletion_effects: Some(vec![]),
    }
    .resolve(&captured)
    .unwrap();
    let compiled = captured
        .compile(
            &synthetic_quest_fixture::relationship_capture(&rows),
            &dependencies,
        )
        .unwrap();
    let kill = progression.kill.creatures[1].clone();
    progression
        .kill
        .creatures
        .retain(|row| row.entry == f::HOSTILE_ENTRY);
    progression
        .kill
        .creatures
        .extend([197, 823].map(|entry| {
            KillCreatureRow {
        entry,
        reputation:
            benilla_sim::player_progression::kill_xp::ReputationAdmission::ProvenAbsentOnKillRow {
                entry,
            },
        ..kill.clone()
    }
        }));
    let loot = progression.loot.as_mut().unwrap();
    if enabled {
        let mut items = loot.inventory.items().cloned().collect::<Vec<_>>();
        items.push(synthetic_loot_fixture::item(9571));
        loot.inventory = synthetic_loot_fixture::inventory_catalog(items);
        progression.quest_script_rules = Some(QuestScriptRules {
            say_range: ai::Real::new(25.0).unwrap(),
            visibility_distance: ai::Real::new(100.0).unwrap(),
        });
    }
    let giver = loot.creatures[1].clone();
    loot.creatures.retain(|row| row.entry == f::HOSTILE_ENTRY);
    loot.creatures
        .extend([197, 823].map(|entry| LootCreatureRow {
            entry,
            ..giver.clone()
        }));
    let target_kill = progression.kill.creatures[0].clone();
    let target_loot = loot.creatures[0].clone();
    for &spawn in targets.iter().skip(1) {
        let entry = 60000 + spawn as u32;
        progression.kill.creatures.push(KillCreatureRow {
            entry, kill_credit: [f::HOSTILE_ENTRY, 0],
            reputation: benilla_sim::player_progression::kill_xp::ReputationAdmission::ProvenAbsentOnKillRow { entry },
            ..target_kill.clone()
        });
        loot.creatures.push(LootCreatureRow {
            entry,
            ..target_loot.clone()
        });
    }
    progression.kill.creatures.sort_by_key(|row| row.entry);
    loot.creatures.sort_by_key(|row| row.entry);
    (profile, initial, progression, compiled)
}
pub fn profile() -> (Profile, q::CompiledOrdinaryQuests) {
    let (mut profile, starter, progression, captured) = setup();
    profile
        .initialize_captured_quest_progression(&starter, &captured, progression)
        .unwrap();
    (profile, captured)
}

#[test]
fn captured_giver_profile_reuses_unchanged_runtime_genesis() {
    let (mut profile, input, rows, compiled) = setup();
    let mut explicit = profile.clone();
    let mut direct = rows.clone();
    direct.quests = compiled.quests().to_vec();
    direct.relations = compiled.relations().clone();
    direct.quest_relationships = compiled.relationships().clone();
    direct.quest_audit = compiled.audit().clone();
    direct.npc_rows = compiled
        .givers()
        .iter()
        .map(|giver| QuestNpcRow {
            entry: giver.entry,
            npc_flags: giver.npc_flags,
            unit_flags: giver.unit_flags,
        })
        .collect();
    explicit
        .initialize_quest_progression(&input, direct)
        .unwrap();
    profile
        .initialize_captured_quest_progression(&input, &compiled, rows)
        .unwrap();
    assert_eq!(explicit, profile);
}

fn authored_mechanics() -> q::Mechanics {
    q::Mechanics::compile(q::MechanicsInput {
        script_keys: vec![71021],
        scripts: vec![
            q::ScriptRow {
                id: 71021,
                delay: 2000,
                priority: 0,
                position: 0,
                command: 0,
                datalong: [0; 3],
                buddy_entry: 0,
                search_radius: 0,
                data_flags: 0,
                dataint: [72001, 0, 0, 0],
                float_bits: [0; 6],
                condition: 0,
            },
            q::ScriptRow {
                id: 71021,
                delay: 2000,
                priority: 0,
                position: 1,
                command: 1,
                datalong: [71, 0, 0],
                buddy_entry: 0,
                search_radius: 0,
                data_flags: 0,
                dataint: [0; 4],
                float_bits: [0; 6],
                condition: 0,
            },
        ],
        broadcasts: vec![q::ScriptBroadcast {
            id: 72001,
            chat_type: 0,
            language: 0,
            sounds: [0; 2],
            emotes: [0; 3],
            delays: [0; 3],
            condition: 0,
            emotes_id: 0,
            flags: 1,
        }],
        emotes: vec![(71, 0)],
        givers: vec![(197, 44001, 3), (823, 0, 2)],
        menus: vec![q::GossipMenuRow {
            entry: 44001,
            text: 75001,
            script: 0,
            condition: 0,
        }],
        options: vec![q::GossipOptionRow {
            menu: 0,
            id: 0,
            option: 2,
            npc_flags: 2,
            condition: 0,
            icon: 0,
            text: "Authored hidden quest option".into(),
            broadcast: 0,
            action_menu: 0,
            action_poi: 0,
            action_script: 0,
            box_coded: false,
            box_money: 0,
            box_text: String::new(),
            box_broadcast: 0,
        }],
        conditions: vec![],
    })
    .unwrap()
}

pub fn profile_with_mechanics() -> (Profile, q::CompiledOrdinaryQuests) {
    let (mut profile, starter, progression, captured) = setup_with_mechanics(true);
    profile
        .initialize_captured_quest_progression(&starter, &captured, progression)
        .unwrap();
    (profile, captured)
}
