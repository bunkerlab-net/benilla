// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored learning and durable controller corridor, never production admission.
use super::*;
use benilla_sim::creature_init as creature;
use benilla_sim::{player_progression as pg, reputation as rep, starter_init as init};

// One melee kill trains the starting weapon; one later spell kill tests cast resume.
// The captured quest-giver corridor separately proves the ten-kill quest threshold.
const LEARNING_KILLS: u32 = 2;

macro_rules! assert_events_eq {
    ($actual:expr, $expected:expr) => {
        assert_eq!(format!("{:?}", $actual), format!("{:?}", $expected))
    };
}

fn mandatory(id: u32) -> init::SpellRow {
    if id == 81 {
        return n::synthetic_initial_cast_fixture::row(81);
    }
    let effects: &[(u32, u32, u32)] = match id {
        227 => &[(25, 0, 0), (60, 0, 0)],
        20597 => &[(6, 98, 43), (6, 98, 55)],
        20598 => &[(6, 137, 4)],
        20599 => &[(6, 156, 0)],
        20864 => &[(6, 98, 54), (6, 98, 160)],
        _ => unreachable!(),
    };
    let holder = id != 227;
    let mut w = [0; 173];
    w[0] = id;
    w[6] = if holder { 0x50 } else { 0xc0 };
    w[18] = 1;
    w[25] = 101;
    w[30] = if holder { 21 } else { 0 };
    w[36] = 1;
    w[58] = if holder { u32::MAX } else { 2 };
    w[59] = if holder { 0 } else { 1024 };
    w[164] = u32::from(!holder);
    w[165] = u32::from(!holder);
    w[166] = if holder { 0 } else { u32::MAX };
    if !holder {
        w[167..170].fill(0x3f80_0000);
    }
    for (slot, &(effect, aura, misc)) in effects.iter().enumerate() {
        w[61 + slot] = effect;
        w[82 + slot] = u32::from(holder);
        w[91 + slot] = aura;
        w[106 + slot] = misc;
        if holder {
            w[64 + slot] = 1;
            w[67 + slot] = 1;
            w[76 + slot] = if id == 20599 { 9 } else { 4 };
            w[167 + slot] = 0x3f80_0000;
        }
    }
    init::SpellRow {
        id,
        words: w,
        duration_ms: Some(if holder { -1 } else { 0 }),
        previous_rank: None,
        talent: false,
        learned_spells: init::Rows::Complete(vec![]),
        external_initial_hooks: false,
    }
}
fn captured(
    input: &init::StarterInput,
    source_identity: [u8; 32],
) -> init::CapturedStarterLearning {
    let init::Rows::Complete(skills) = &input.default_skills else {
        unreachable!()
    };
    let init::Rows::Complete(spells) = &input.default_spells else {
        unreachable!()
    };
    let init::Rows::Complete(abilities) = &input.skill_abilities else {
        unreachable!()
    };
    init::CapturedStarterLearning {
        source_identity,
        selected_spell_ids: input.spells.iter().map(|r| r.id).collect(),
        skill_training: init::Rows::Complete(vec![]),
        creation_skills: init::Rows::Complete(
            skills
                .iter()
                .map(|r| init::CreationSkill {
                    race_mask: 0,
                    class_mask: 0,
                    skill: r.id.into(),
                    step: r.step.into(),
                })
                .collect(),
        ),
        creation_spells: init::Rows::Complete(
            spells
                .iter()
                .map(|id| init::CreationSpell {
                    race: 1,
                    class: 8,
                    spell: *id,
                })
                .collect(),
        ),
        skill_lines: init::Rows::Complete(
            input
                .skill_lines
                .iter()
                .map(|r| {
                    let mut w = [0; 22];
                    w[0] = r.id.into();
                    w[1] = r.category as u32;
                    w
                })
                .collect(),
        ),
        skill_race_class: init::Rows::Complete(
            input
                .skill_lines
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    [
                        i as u32 + 1,
                        r.id.into(),
                        1,
                        128,
                        if r.category == 7 { 16 } else { 0 },
                        0,
                        0,
                        0,
                    ]
                })
                .collect(),
        ),
        skill_tiers: init::Rows::Complete(vec![]),
        skill_abilities: init::Rows::Complete(
            abilities
                .iter()
                .map(|r| {
                    let mut w = [0; 15];
                    w[0] = r.row_id;
                    w[1] = r.skill.into();
                    w[2] = r.spell;
                    w[3] = r.race_mask;
                    w[4] = r.class_mask;
                    w[7] = r.required_value;
                    w[8] = r.forward_spell_id;
                    w[9] = r.learn_on_get_skill;
                    w[10] = r.max_value;
                    w
                })
                .collect(),
        ),
        talents: init::Rows::Complete(vec![]),
        chains: init::Rows::Complete(vec![]),
        learn_spells: init::Rows::Complete(vec![]),
        global_chain_probe: init::Rows::Complete(vec![]),
        global_learn_probe: init::Rows::Complete(vec![]),
        spell_existence: init::Rows::Complete(input.spells.iter().map(|r| (r.id, true)).collect()),
        numeric: init::Rows::Complete(
            input
                .spells
                .iter()
                .map(|r| init::LearningSpellNumeric {
                    source_identity,
                    id: r.id,
                    client_words: r.words,
                    effective_words: r.words,
                    duration_ms: r.duration_ms,
                })
                .collect(),
        ),
    }
}
fn profile(order_identity: u8) -> n::Profile {
    profile_with_bound_change(order_identity, 0)
}
fn profile_with_bound_change(order_identity: u8, change: u8) -> n::Profile {
    let (profile, mut input, mut progression) = loop_fixture::setup_with_unbound_spells();
    for id in [136, 162] {
        if let init::Rows::Complete(rows) = &mut input.default_skills {
            rows.push(init::DefaultSkill { id, step: 0 });
        }
        input
            .skill_lines
            .push(init::SkillLineRow { id, category: 6 });
    }
    let mut weapon = input.items[0].clone();
    weapon.id = 900003;
    weapon.class = 2;
    weapon.subclass = 10;
    weapon.inventory_type = 17;
    weapon.armor = 0;
    weapon.stats = [init::ItemStat { kind: 0, value: 0 }; 10];
    weapon.damage[0] = init::ItemDamage {
        min_bits: 2.0f32.to_bits(),
        max_bits: 4.0f32.to_bits(),
        school: 0,
    };
    weapon.delay_ms = 2500;
    weapon.max_durability = 0;
    input.items.push(weapon);
    if let init::Rows::Complete(rows) = &mut input.item_grants {
        rows.push(init::ItemGrant {
            source: init::GrantSource::Custom,
            source_order: 1,
            item: 900003,
            count: 1,
        });
    }
    for id in [81, 227, 20597, 20598, 20599, 20864] {
        input.spells.push(mandatory(id));
        if let init::Rows::Complete(rows) = &mut input.default_spells {
            rows.push(id);
        }
    }
    let mut capture = captured(&input, [86; 32]);
    if change == 1 {
        let init::Rows::Complete(rows) = &mut capture.skill_abilities else {
            unreachable!()
        };
        rows[0][7] += 1;
    }
    if change == 2 {
        capture.global_chain_probe = init::Rows::Complete(vec![init::CapturedSpellChain {
            spell: 1,
            previous: 0,
            first: 1,
            rank: 1,
            required: 0,
        }]);
        capture.chains = init::Rows::Complete(vec![init::CapturedSpellChain {
            spell: 133,
            previous: 0,
            first: 133,
            rank: 1,
            required: 0,
        }]);
    }
    let learning = capture.compile(1, 8).unwrap();
    let init::Rows::Complete(numeric) = &capture.numeric else {
        unreachable!()
    };
    let mut queried: Vec<_> = numeric.iter().map(|r| r.id).collect();
    queried.sort_unstable();
    let dependencies: Vec<_> = [81, 227, 20597, 20598, 20599, 20864]
        .into_iter()
        .map(|id| {
            let row = input.spells.iter().find(|r| r.id == id).unwrap();
            n::InitialCastDependencies {
                spell_id: id,
                cast_time: init::InitialSpellLookup::Missing {
                    index: row.words[18],
                },
                duration: if row.words[30] == 0 {
                    init::InitialSpellLookup::Missing { index: 0 }
                } else {
                    init::InitialSpellLookup::Found(n::SpellDurationSource {
                        id: 21,
                        base_ms: -1,
                        per_level_ms: 0,
                        maximum_ms: -1,
                    })
                },
                aura_script_names: n::SpellSourceRows::Complete(vec![]),
                server_side: 0,
                server_attributes: 0,
                bonus_coefficients: if id == 81 {
                    [0x3f80_0000, 0, 0]
                } else {
                    std::array::from_fn(|slot| {
                        if row.words[61 + slot] == 6 {
                            0x3f80_0000
                        } else {
                            0
                        }
                    })
                },
                attack_power_coefficients: [0; 3],
            }
        })
        .collect();
    let mut source = n::NativeInitialCastSourceInput {
        source_identity: [0; 32],
        entry_location: None,
        store_identity: [87; 32],
        dependencies: n::SpellSourceRows::Complete(dependencies.clone()),
        hooks: n::synthetic_spell_fixture::hooks(),
        queried_spell_ids: n::SpellSourceRows::Complete(queried),
    };
    if change == 2 {
        source.hooks.spell_rank_chain = n::SpellSourceRows::Complete(vec![n::SpellRankSource {
            spell_id: 133,
            previous_spell: 0,
            first_spell: 133,
            rank: 1,
            required_spell: 0,
        }]);
    }
    let bound = source.bind_learning_spell_rows(&learning, numeric).unwrap();
    input = learning.populate(input, bound).unwrap();
    let mut outfit_items = [0; 12];
    outfit_items[0] = 900001;
    let items = init::CapturedStarterItems {
        outfits: init::Rows::Complete(vec![init::CharStartOutfitRow {
            id: 1,
            race_class_gender: 0x0801,
            items: outfit_items,
        }]),
        custom: init::Rows::Complete(vec![init::CustomStartingItem {
            race: 1,
            class: 8,
            item: 900003,
            amount: 1,
        }]),
        items: input
            .items
            .iter()
            .cloned()
            .map(|item| init::StartingItemRow {
                item,
                buy_count: 1,
                first_spell_category: 0,
            })
            .collect(),
    };
    input = items.compile(input).unwrap();
    source.source_identity = input.source_identity();
    let deps = init::StarterInitialDependencies {
        source_identity: input.source_identity(),
        store_identity: source.store_identity,
        hook_scope_identity: source.hook_scope_identity(),
        queried_spell_ids: dependencies.iter().map(|r| r.spell_id).collect(),
        rows: init::Rows::Complete(
            dependencies
                .iter()
                .map(|d| init::StarterInitialDependency {
                    spell_id: d.spell_id,
                    source_hash: input
                        .spells
                        .iter()
                        .find(|r| r.id == d.spell_id)
                        .unwrap()
                        .source_hash(),
                    cast_time: d.cast_time.clone(),
                    duration: d.duration.clone(),
                    server_side: d.server_side,
                    server_attributes: d.server_attributes,
                    bonus_coefficients: d.bonus_coefficients,
                    attack_power_coefficients: d.attack_power_coefficients,
                })
                .collect(),
        ),
    };
    let prepared = init::prepare_starter(
        &input,
        &deps,
        &init::StarterCastOrder {
            creation: init::InitialCastOrderEvidence::Authored {
                identity: [order_identity; 32],
            },
            first_entry: init::InitialCastOrderEvidence::Authored { identity: [89; 32] },
            entry_skill_order: Some(vec![95, 8, 136, 162]),
        },
    )
    .unwrap();
    let mut scene = profile;
    // The selected periodic spell permits at most 500 ms per simulated tick.
    // Keep all spell inputs and the 120-second corpse timer unchanged.
    scene.tick_ms = 500;
    let mut profile = n::synthetic_fixture::profile();
    profile.content_digest = scene.content_digest;
    profile.authority_policy = scene.authority_policy;
    profile.geometry_revision = scene.geometry_revision;
    profile.tick_ms = scene.tick_ms;
    profile.clock_origin_ms = scene.clock_origin_ms;
    profile.map_id = scene.map_id;
    profile.player = scene.player.clone();
    profile.abilities = std::mem::take(&mut scene.abilities);
    profile.npcs = std::mem::take(&mut scene.npcs);
    profile
        .initialize_player_prepared(&input, &prepared, n::synthetic_fixture::float(100.0), 1)
        .unwrap();
    profile
        .initialize_initial_cast_source(&input, source)
        .unwrap();
    progression.skills = prepared
        .first_entry()
        .skills
        .values()
        .enumerate()
        .map(|(i, r)| pg::SkillState {
            id: r.id,
            position: i as u8,
            value: r.value,
            maximum: r.maximum,
            step: r.step,
            permanent_bonus: r.permanent_bonus,
            temporary_bonus: r.temporary_bonus,
            status: pg::SkillStatus::Unchanged,
        })
        .collect();
    progression.levels = pg::Catalog::new(
        1,
        8,
        pg::Rules::from_classic_config(
            10,
            progression.levels.rules().always_max_skill_for_level,
            progression.levels.rules().talent_rate_bits,
        )
        .unwrap(),
        (1..=10)
            .map(|level| pg::LevelStatsRow {
                race: 1,
                class: 8,
                level,
                attributes: init::Attributes([17, 23, 29, 31, 37].map(|v| v + level as i32 - 1)),
            })
            .collect(),
        (1..=10)
            .map(|level| pg::ClassLevelStatsRow {
                class: 8,
                level,
                base_health: 5000 + (level - 1) * 10,
                base_mana: 59 + (level - 1) * 6,
            })
            .collect(),
        (1..=10)
            .map(|level| pg::XpRow {
                level,
                xp_for_next_level: if level == 10 { 0 } else { level * 40 },
            })
            .collect(),
        input
            .skill_lines
            .iter()
            .map(|r| pg::SkillDependency {
                skill: r.id,
                line: Some(*r),
                race_class: init::Rows::Complete(vec![]),
            })
            .collect(),
    )
    .unwrap();
    progression.world_rules.progression = progression.levels.rules();
    let diplomacy = input.spells.iter().find(|r| r.id == 20599).unwrap();
    progression.reputation = Some(
        rep::Catalog::new_with_general_reputation_auras(
            vec![
                rep::Row {
                    faction: rep::Faction {
                        id: 99001,
                        reputation_list_id: 0,
                        race_masks: [0; 4],
                        class_masks: [0; 4],
                        base_values: [0; 4],
                        flags: [1, 0, 0, 0],
                        team: 0,
                    },
                    reward_rates: rep::Resolved::Absent,
                    spillover: rep::Resolved::Absent,
                },
                rep::Row {
                    faction: rep::Faction {
                        id: 96002,
                        reputation_list_id: -1,
                        race_masks: [0; 4],
                        class_masks: [0; 4],
                        base_values: [0; 4],
                        flags: [0; 4],
                        team: 0,
                    },
                    reward_rates: rep::Resolved::Absent,
                    spillover: rep::Resolved::Absent,
                },
            ],
            vec![],
            rep::Config {
                gain_rate: benilla_sim::classic_spell::SpellFloat::ONE,
                low_level_kill_rate: benilla_sim::classic_spell::SpellFloat::ONE,
                low_level_quest_rate: benilla_sim::classic_spell::SpellFloat::ONE,
            },
            rep::Audit {
                general_reputation_auras: rep::Evidence::Present,
                faction_reputation_auras: rep::Evidence::Absent,
                forced_reactions: rep::Evidence::Absent,
                reputation_scripts: rep::Evidence::Absent,
            },
            vec![rep::GeneralReputationAura {
                spell_id: 20599,
                effect_index: 0,
                aura_type: 156,
                misc_value: 0,
                amount: 10,
                attributes: 0x50,
                attributes_ex3: 0,
                caster_aura_state: 0,
                aura_interrupt_flags: 0,
                duration_ms: -1,
                source_spell_hash: diplomacy.source_hash(),
            }],
        )
        .unwrap(),
    );
    progression.world_rules.reputation = progression.reputation.as_ref().unwrap().config();
    let mut intro = progression.quests[0].clone();
    intro.id = 783;
    intro.special_flags = 0;
    intro.required_creature = [0; 4];
    intro.required_creature_count = [0; 4];
    intro.required_item = [0; 4];
    intro.required_item_count = [0; 4];
    intro.next_in_chain = 7;
    intro.reward.xp.reward_money_max_level = 6;
    intro.excluded.reward_rep_faction[0] = 99001;
    intro.excluded.reward_rep_value[0] = 13;
    let mut cleanup = intro.clone();
    cleanup.id = 7;
    cleanup.previous_quest = 783;
    cleanup.next_in_chain = 15;
    cleanup.special_flags = q::KILL | q::SPEAK;
    cleanup.required_creature = [loop_fixture::HOSTILE_ENTRY as i32, 0, 0, 0];
    cleanup.required_creature_count = [LEARNING_KILLS, 0, 0, 0];
    let mut next = intro.clone();
    next.id = 15;
    next.previous_quest = 7;
    next.next_in_chain = 0;
    progression.quests = vec![intro, cleanup, next];
    progression.quest_relationships =
        n::synthetic_quest_fixture::relationship_capture(&progression.quests);
    let relations: Vec<_> = [783, 7, 15]
        .into_iter()
        .map(|quest| q::CreatureQuestRelation {
            creature: loop_fixture::GIVER_ENTRY,
            quest,
        })
        .collect();
    progression.relations = q::QuestRelations {
        starters: relations.clone(),
        finishers: relations,
    };
    let loot = progression.loot.as_mut().unwrap();
    let mut items: Vec<_> = loot.inventory.items().cloned().collect();
    items
        .iter_mut()
        .find(|item| item.entry == loop_fixture::ITEM)
        .unwrap()
        .maximum_count = 0;
    let mut staff = items.iter().find(|r| r.entry == 900001).unwrap().clone();
    staff.entry = 900003;
    staff.class = 2;
    staff.inventory_type = 17;
    items.push(staff);
    loot.inventory = n::synthetic_loot_fixture::inventory_catalog(items);
    for npc in &mut profile.npcs {
        npc.ai.corpse_decay_ms = 2000;
        npc.ai.respawn_delay_seconds = 1;
    }
    scene.player = profile.player;
    scene.abilities = profile.abilities;
    scene.npcs = profile.npcs;
    scene.combat = profile.combat;
    scene
        .initialize_quest_progression(&input, progression)
        .unwrap();
    let mut profile = scene;
    profile.validate().expect("learning scene composition");
    profile = n::synthetic_creature_model_fixture::with_complete_scene_generations_and_source_edit(
        profile,
        |source| {
            let creature::Rows::Complete(levels) = &mut source.input.class_levels else {
                unreachable!()
            };
            for level in levels {
                level.base_health = 10;
            }
            source.input.template.corpse_delay_seconds = 2;
            source.respawn_delay_min_seconds = 3;
            source.respawn_delay_max_seconds = 3;
            source.input.template.displays[1..].fill(creature::DisplayChoice {
                id: 0,
                probability: 0,
            });
            let creature::Rows::Complete(models) = &mut source.input.models else {
                unreachable!()
            };
            for model in models {
                model.other_gender_id = 0;
            }
        },
    );
    let mut spell = n::synthetic_spell_fixture::source(&profile);
    spell.initial_cast_dependencies = n::SpellSourceRows::Complete(dependencies);
    let n::SpellSourceRows::Complete(modifier_ids) = &mut spell.modifier_spell_ids else {
        unreachable!()
    };
    let mut scope: std::collections::BTreeSet<_> = modifier_ids.iter().copied().collect();
    scope.extend(
        prepared
            .creation_plan()
            .occurrences()
            .iter()
            .chain(prepared.first_entry_plan().occurrences())
            .map(|cast| cast.row().id),
    );
    *modifier_ids = scope.into_iter().collect();
    profile.initialize_spell_authority(&input, spell).unwrap();
    profile.initialize_player_melee(1).unwrap();
    profile
}

struct LearningCatalog {
    base: LoopCatalog,
    quests: Vec<q::QuestRow>,
}
impl LearningCatalog {
    fn new(profile: &n::Profile) -> Self {
        let progression = profile.quest_progression.as_ref().unwrap();
        Self {
            quests: [783, 7, 15]
                .into_iter()
                .map(|id| progression.quest(id).unwrap().clone())
                .collect(),
            base: LoopCatalog {
                quest: progression.quest(783).unwrap().clone(),
                map: profile.map_id,
                displays: profile
                    .npcs
                    .iter()
                    .flat_map(|npc| {
                        let source = profile
                            .creature_generations
                            .as_ref()
                            .unwrap()
                            .source(npc.ai.creature_id)
                            .unwrap();
                        let creature::Rows::Complete(displays) = &source.input.displays else {
                            unreachable!()
                        };
                        displays.iter().map(|display| display.id)
                    })
                    .collect(),
                loot_item: progression
                    .loot
                    .as_ref()
                    .unwrap()
                    .inventory
                    .item(loop_fixture::ITEM)
                    .unwrap()
                    .clone(),
            },
        }
    }
}
impl ClientCatalog for LearningCatalog {
    fn build(&self) -> u32 {
        self.base.build()
    }
    fn manifest_digest(&self) -> runtime::Digest {
        self.base.manifest_digest()
    }
    fn contains_map(&self, id: u32) -> bool {
        self.base.contains_map(id)
    }
    fn contains_creature_display(&self, id: u32) -> bool {
        self.base.contains_creature_display(id)
    }
    fn contains_item_display(&self, id: u32) -> bool {
        self.base.contains_item_display(id)
    }
    fn contains_spell(&self, id: u32) -> bool {
        matches!(id, 81 | 227 | 20597 | 20598 | 20599 | 20864) || self.base.contains_spell(id)
    }
    fn player_power_type(&self, id: PlayerId) -> Option<u8> {
        self.base.player_power_type(id)
    }
    fn player_bytes_2(&self, id: PlayerId) -> Option<u32> {
        self.base.player_bytes_2(id)
    }
    fn item_metadata(&self, id: u32) -> Option<benilla_protocol::messages::ItemInfo> {
        if id != 900003 {
            return self.base.item_metadata(id);
        }
        let mut row = self.base.item_metadata(900001)?;
        row.class = 2;
        row.subclass = 10;
        row.inventory_type = 17;
        row.armor = 0;
        row.stats.clear();
        row.damages = vec![benilla_protocol::messages::ItemDamage {
            min: 2.0,
            max: 4.0,
            school: 0,
        }];
        row.dmg_min = 2.0;
        row.dmg_max = 4.0;
        row.delay_ms = 2500;
        Some(row)
    }
    fn quest_metadata(&self, id: u32) -> Option<benilla_world_client::QuestMetadata> {
        let row = self.quests.iter().find(|r| r.id == id)?;
        let mut metadata = Catalog.quest_metadata(70001)?;
        metadata.template.quest_id = id;
        metadata.template.next_quest_in_chain = row.next_in_chain;
        metadata.template.money_max_level = row.reward.xp.reward_money_max_level;
        metadata.template.point_map_id = self.base.map;
        for (i, objective) in metadata.template.objectives.iter_mut().enumerate() {
            objective.creature_or_go = row.required_creature[i] as u32;
            objective.required_count = row.required_creature_count[i];
        }
        Some(metadata)
    }
    fn quest_greeting(&self, entry: u32) -> Option<String> {
        self.base.quest_greeting(entry)
    }
    fn native_quest_greeting(
        &self,
        entry: u32,
    ) -> Option<benilla_world_client::NativeQuestGreeting> {
        self.base.native_quest_greeting(entry)
    }
    fn native_quest_presentation(
        &self,
        id: u32,
    ) -> Option<benilla_world_client::NativeQuestPresentation> {
        self.quests
            .iter()
            .any(|r| r.id == id)
            .then(|| Catalog.native_quest_presentation(70001))
            .flatten()
    }
}
fn enter_learning(h: &Harness, profile: n::Profile, open: bool) -> Result<NativeController, Error> {
    enter_learning_at(&h.root, h, profile, open)
}
fn enter_learning_at(
    root: &std::path::Path,
    h: &Harness,
    profile: n::Profile,
    open: bool,
) -> Result<NativeController, Error> {
    let catalog = LearningCatalog::new(&profile);
    NativeController::enter_component_test(
        root,
        &h.profile,
        &h.character,
        if open {
            Entry::Open { world: [8; 16] }
        } else {
            Entry::Create {
                world: [8; 16],
                seed: 7,
            }
        },
        profile,
        &catalog,
        loop_fixture::CompleteAuthority::default(),
    )
}
fn reopen_learning(h: &Harness, c: NativeController, profile: &n::Profile) -> NativeController {
    let checkpoint = c.world.checkpoint().unwrap();
    drop(c);
    let mut next = enter_learning(h, profile.clone(), true).unwrap();
    assert_eq!(next.world.checkpoint().unwrap(), checkpoint);
    let projection = next.initial_events().unwrap();
    let remaining: Vec<_> = next
        .world
        .state()
        .casts
        .values()
        .filter_map(|cast| match cast.execution.state() {
            benilla_sim::classic_spell::CastState::Casting { remaining_ms } => {
                Some((133, remaining_ms))
            }
            _ => None,
        })
        .collect();
    let projected: Vec<_> = projection
        .iter()
        .filter_map(|event| match event {
            SessionEvent::SpellStart {
                spell_id,
                cast_time_ms,
                ..
            } => Some((*spell_id, *cast_time_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(projected, remaining);
    assert!(!projection.iter().any(|event| matches!(
        event,
        SessionEvent::XpGain(_)
            | SessionEvent::LevelUp(_)
            | SessionEvent::SpellLearned { .. }
            | SessionEvent::SpellGo { .. }
    )));
    next
}
fn quest_command(c: &mut NativeController, quest: u32, reward: bool) -> Pump {
    let npc = guid(c, loop_fixture::GIVER);
    c.receive(if reward {
        ClientCommand::QuestgiverChooseReward {
            npc,
            quest,
            choice: 0,
        }
    } else {
        ClientCommand::QuestgiverAccept { npc, quest }
    })
    .unwrap();
    step(c)
}

fn collect_and_release(c: &mut NativeController) -> Pump {
    c.receive(ClientCommand::LootMoney).unwrap();
    c.receive(ClientCommand::AutostoreLootItem { slot: 0 })
        .unwrap();
    c.receive(ClientCommand::LootRelease {
        guid: guid(c, loop_fixture::HOSTILE),
    })
    .unwrap();
    step(c)
}
#[test]
fn captured_learning_controller_creation_cast_level_rewards_and_reopen_are_identical() {
    let started = std::time::Instant::now();
    let profile = profile(88);
    let h = Harness::new();
    let baseline_h = Harness::new();
    let mut c = enter_learning(&h, profile.clone(), false).unwrap();
    let mut baseline = enter_learning_at(&baseline_h.root, &h, profile.clone(), false).unwrap();
    assert_events_eq!(
        c.initial_events().unwrap(),
        baseline.initial_events().unwrap()
    );
    assert!(c.world.state().rng_counter > 0);
    c = reopen_learning(&h, c, &profile);
    for quest in [783, 7] {
        assert_events_eq!(
            quest_command(&mut c, quest, false).events,
            quest_command(&mut baseline, quest, false).events
        );
        if quest == 7 {
            for kill_index in 0..LEARNING_KILLS {
                // Keep the real 120-second inspection timer and compare every durable tick.
                let wait_started = std::time::Instant::now();
                let wait_tick = c.tick();
                for _ in 0..300 {
                    if c.world.state().npcs[&loop_fixture::HOSTILE].health > 0 {
                        break;
                    }
                    assert_events_eq!(step(&mut c).events, step(&mut baseline).events);
                }
                assert!(c.world.state().npcs[&loop_fixture::HOSTILE].health > 0);
                eprintln!(
                    "starter_learning corpse wait {kill_index}: {} ticks, {:.3}s",
                    c.tick() - wait_tick,
                    wait_started.elapsed().as_secs_f64()
                );
                if kill_index == 0 {
                    c.receive(ClientCommand::AttackSwing {
                        guid: guid(&c, loop_fixture::HOSTILE),
                    })
                    .unwrap();
                    baseline
                        .receive(ClientCommand::AttackSwing {
                            guid: guid(&baseline, loop_fixture::HOSTILE),
                        })
                        .unwrap();
                    for _ in 0..100 {
                        assert_events_eq!(step(&mut c).events, step(&mut baseline).events);
                        if c.world.state().npcs[&loop_fixture::HOSTILE].health == 0 {
                            break;
                        }
                    }
                    c = reopen_learning(&h, c, &profile);
                    assert!(progress(&c)
                        .player
                        .view()
                        .skills
                        .iter()
                        .any(|skill| skill.id == 136 && skill.value > 1));
                    assert!(progress(&c).player.view().level > 1);
                } else {
                    assert_events_eq!(
                        begin_cast(&mut c, loop_fixture::HOSTILE).events,
                        begin_cast(&mut baseline, loop_fixture::HOSTILE).events
                    );
                    c = reopen_learning(&h, c, &profile);
                    for _ in 0..20 {
                        if c.world.state().npcs[&loop_fixture::HOSTILE].health == 0 {
                            break;
                        }
                        assert_events_eq!(step(&mut c).events, step(&mut baseline).events);
                    }
                    c = reopen_learning(&h, c, &profile);
                }
                assert_eq!(c.world.state().npcs[&loop_fixture::HOSTILE].health, 0);
                assert!(
                    c.projection
                        .npc_guid(c.world.state().npcs[&loop_fixture::HOSTILE].id)
                        .is_some(),
                    "missing corpse after kill {kill_index} at tick {}",
                    c.tick(),
                );
                let opened = open_loot(&mut c, loop_fixture::HOSTILE);
                let expected = open_loot(&mut baseline, loop_fixture::HOSTILE);
                assert_events_eq!(opened.events, expected.events);
                assert!(
                    opened
                        .events
                        .iter()
                        .any(|event| matches!(event, SessionEvent::LootResponse { .. })),
                    "missing loot response after kill {kill_index} at tick {}",
                    c.tick(),
                );
                let collected = collect_and_release(&mut c);
                let expected = collect_and_release(&mut baseline);
                assert_events_eq!(collected.events, expected.events);
                assert!(collected
                    .events
                    .iter()
                    .any(|event| matches!(event, SessionEvent::LootReleaseResponse { .. })));
                assert_events_eq!(step(&mut c).events, step(&mut baseline).events);
            }
        }
        assert_events_eq!(
            quest_command(&mut c, quest, true).events,
            quest_command(&mut baseline, quest, true).events
        );
        c = reopen_learning(&h, c, &profile);
    }
    assert_events_eq!(
        quest_command(&mut c, 15, false).events,
        quest_command(&mut baseline, 15, false).events
    );
    assert_eq!(
        c.world.checkpoint().unwrap(),
        baseline.world.checkpoint().unwrap()
    );
    assert!(progress(&c).player.view().level > 1);
    drop(c);
    assert!(enter_learning(&h, self::profile(90), true).is_err());
    assert!(enter_learning(&h, profile_with_bound_change(88, 1), true).is_err());
    assert!(enter_learning(&h, profile_with_bound_change(88, 2), true).is_err());
    eprintln!(
        "starter_learning creation/cast/rewards/reopen: {:.3}s",
        started.elapsed().as_secs_f64()
    );
}

#[test]
fn learning_reward_checkpoint_retry_publishes_once_without_replaying_learning() {
    let started = std::time::Instant::now();
    let data = profile(88);
    let h = Harness::new();
    let baseline_h = Harness::new();
    let mut c = enter_learning(&h, data.clone(), false).unwrap();
    let mut baseline = enter_learning_at(&baseline_h.root, &h, data.clone(), false).unwrap();
    assert_events_eq!(
        c.initial_events().unwrap(),
        baseline.initial_events().unwrap()
    );
    assert_events_eq!(
        quest_command(&mut c, 783, false).events,
        quest_command(&mut baseline, 783, false).events
    );
    let npc = guid(&c, loop_fixture::GIVER);
    c.receive(ClientCommand::QuestgiverChooseReward {
        npc,
        quest: 783,
        choice: 0,
    })
    .unwrap();
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored save fault",
    )
    .unwrap();
    let paused = c.pump(c.tick_duration());
    assert!(paused.pause.is_some());
    assert!(paused.events.is_empty());
    let expected = quest_command(&mut baseline, 783, true);
    let committed = c.world.checkpoint().unwrap();
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    let published = c.pump(Duration::ZERO);
    assert!(published.pause.is_none());
    assert_events_eq!(published.events, expected.events);
    assert!(!published
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::SpellLearned { .. })));
    assert_eq!(c.world.checkpoint().unwrap(), committed);
    assert_eq!(
        c.world.checkpoint().unwrap(),
        baseline.world.checkpoint().unwrap()
    );
    assert!(c.pump(Duration::ZERO).events.is_empty());
    let c = reopen_learning(&h, c, &data);
    assert_eq!(c.world.checkpoint().unwrap(), committed);
    eprintln!(
        "starter_learning checkpoint retry: {:.3}s",
        started.elapsed().as_secs_f64()
    );
}

#[test]
fn learning_hook_binder_refuses_unknown_registry_global_area_and_changed_hooks() {
    let started = std::time::Instant::now();
    let (_, input, _) = loop_fixture::setup();
    let capture = captured(&input, [86; 32]);
    let plan = capture.compile(1, 8).unwrap();
    let init::Rows::Complete(numeric) = &capture.numeric else {
        unreachable!()
    };
    let mut ids: Vec<_> = numeric.iter().map(|r| r.id).collect();
    ids.sort_unstable();
    let source = n::NativeInitialCastSourceInput {
        source_identity: [1; 32],
        entry_location: None,
        store_identity: [2; 32],
        dependencies: n::SpellSourceRows::Complete(vec![]),
        queried_spell_ids: n::SpellSourceRows::Complete(ids),
        hooks: n::synthetic_spell_fixture::hooks(),
    };
    assert!(source.bind_learning_spell_rows(&plan, numeric).is_ok());
    for kind in 0..4 {
        let mut changed = source.clone();
        match kind {
            0 => changed.hooks.spell_script_names = n::SpellSourceRows::Unknown,
            1 => changed.hooks.spell_area = n::SpellAreaSourceRows::Unknown,
            2 => changed.hooks.conditions = n::SpellSourceRows::Complete(vec![1]),
            _ => changed.queried_spell_ids = n::SpellSourceRows::Unknown,
        }
        assert!(changed.bind_learning_spell_rows(&plan, numeric).is_err());
    }
    eprintln!(
        "starter_learning hook binder: {:.3}s",
        started.elapsed().as_secs_f64()
    );
}
