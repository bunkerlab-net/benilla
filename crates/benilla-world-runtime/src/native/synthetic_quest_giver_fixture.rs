// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored closed short chain, not the real 15-to-script21 Northshire catalog.
use super::{synthetic_distinct_quest_fixture as f, synthetic_fixture as base, *};
use benilla_sim::{quest_lifecycle as q, starter_init as init};

pub const TARGETS: [u64; 10] = [2, 10, 11, 12, 13, 14, 15, 16, 17, 18];
pub fn setup() -> (
    Profile,
    init::StarterInput,
    QuestProgressionInput,
    q::CompiledOrdinaryQuests,
) {
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
    for spawn in TARGETS.into_iter().skip(1) {
        let mut next = target.clone();
        next.entry = 60000 + spawn as u32;
        next.ai.creature_id = spawn;
        profile.npcs.push(next);
    }
    let mut motion = profile.source_motion.take().unwrap();
    motion.npcs.insert(4, motion.npcs[&f::GIVER].clone());
    for spawn in TARGETS.into_iter().skip(1) {
        motion.npcs.insert(spawn, motion.npcs[&2].clone());
    }
    let mut interactions = f::interaction_rows(&profile);
    for row in &mut interactions.npcs {
        row.npc_flags = if matches!(row.entry, 197 | 823) { 2 } else { 0 };
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
    for (id, previous, next) in [(783, 0, 7), (7, 783, 15), (15, 7, 0)] {
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
        rows.push(row);
    }
    let starters = vec![
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
    let finishers = [783, 7, 15]
        .map(|quest| q::CreatureQuestRelation {
            creature: 197,
            quest,
        })
        .to_vec();
    let captured = q::CapturedOrdinaryQuests {
        source: [37; 32],
        quests: Some(rows.clone()),
        starters: Some(starters.clone()),
        finishers: Some(finishers.clone()),
        loaded_starters: Some(starters),
        loaded_finishers: Some(finishers),
        givers: Some(
            [(3, 197), (4, 823)]
                .map(|(spawn, entry)| q::CapturedQuestGiver {
                    spawn,
                    database_guid: spawn as u32,
                    generation: 0,
                    entry,
                    npc_flags: 2,
                    unit_flags: 0,
                    script_name: String::new(),
                    loaded_script: 0,
                })
                .to_vec(),
        ),
    };
    let dependencies = q::QuestDependencySource {
        source: [37; 32],
        scripts: Some(vec![]),
        gossip: Some(
            [197, 823]
                .map(|entry| q::CapturedGossipScope {
                    entry,
                    menu: 0,
                    options: vec![],
                })
                .to_vec(),
        ),
        allowed_areas: vec![9],
        global_spell_areas: Some(vec![]),
        item_keys: vec![],
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
    let giver = loot.creatures[1].clone();
    loot.creatures.retain(|row| row.entry == f::HOSTILE_ENTRY);
    loot.creatures
        .extend([197, 823].map(|entry| LootCreatureRow {
            entry,
            ..giver.clone()
        }));
    let target_kill = progression.kill.creatures[0].clone();
    let target_loot = loot.creatures[0].clone();
    for spawn in TARGETS.into_iter().skip(1) {
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
