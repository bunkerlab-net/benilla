// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored rows only; no native content, capability, or client install.
use super::synthetic_fixture::{
    float, initialized_profile, initialized_starter_input, player, point, KernelAuthority,
};
use super::*;
use benilla_sim::{
    player_progression as pg, quest as arithmetic, quest_lifecycle as q, starter_init as init,
};
use pg::{derived::*, kill_xp as kill};
fn excluded() -> q::ExcludedFields {
    q::ExcludedFields {
        required_skill: 0,
        required_skill_value: 0,
        required_condition: 0,
        rep_objective_faction: 0,
        rep_objective_value: 0,
        required_min_rep_faction: 0,
        required_min_rep_value: 0,
        required_max_rep_faction: 0,
        required_max_rep_value: 0,
        quest_type: 0,
        suggested_players: 0,
        limit_time: 0,
        exclusive_group: 0,
        breadcrumb_for_quest: 0,
        source_item: 0,
        source_item_count: 0,
        source_spell: 0,
        required_source: [0; 4],
        required_source_count: [0; 4],
        required_spell: [0; 4],
        reward_choice_item: [0; 6],
        reward_choice_count: [0; 6],
        reward_item: [0; 4],
        reward_item_count: [0; 4],
        reward_rep_faction: [0; 5],
        reward_rep_value: [0; 5],
        reward_faction_flags: 0,
        reward_spell: 0,
        reward_spell_cast: 0,
        reward_mail_template: 0,
        reward_mail_delay: 0,
        start_script: 0,
        complete_script: 0,
    }
}
fn progression_audit() -> pg::Audit {
    pg::Audit {
        group_or_raid: pg::Evidence::Absent,
        pet_or_controlled_unit: pg::Evidence::Absent,
        used_talents: pg::Evidence::Absent,
        shapeshift: pg::Evidence::Absent,
        level_change_scripts: pg::Evidence::Absent,
        dynamic_or_level_scaling_modifiers: pg::Evidence::Absent,
        resource_or_stat_aura_hooks: pg::Evidence::Absent,
        unsupported_item_effects: pg::Evidence::Absent,
        non_mana_power_modifiers: pg::Evidence::Absent,
        extra_spell_crit_or_regen_modifiers: pg::Evidence::Absent,
    }
}
fn derived_audit() -> DerivedAudit {
    DerivedAudit {
        attack_power_or_damage_modifiers: DerivedEvidence::Absent,
        crit_or_avoidance_modifiers: DerivedEvidence::Absent,
        spell_damage_or_hit_modifiers: DerivedEvidence::Absent,
        regeneration_modifiers: DerivedEvidence::Absent,
        weapon_enchantments: DerivedEvidence::Absent,
        ammo: DerivedEvidence::Absent,
        shapeshift_or_disarm: DerivedEvidence::Absent,
        parry_block_or_dual_wield: DerivedEvidence::Absent,
    }
}
pub fn setup() -> (Profile, init::StarterInput, QuestProgressionInput) {
    let mut input = initialized_starter_input();
    for row in &mut input.spells {
        row.words[1] = if row.id == 60001 { 2 } else { 4 };
        row.words[18] = 77;
        row.words[28] = 1;
        row.words[32] = 10;
    }
    let mut p = initialized_profile();
    p.initialize_player(&input, float(100.0), 1).unwrap();
    p.npcs[0].max_health = 10;
    let mut giver = p.npcs[0].clone();
    giver.ai.creature_id = 3;
    giver.entry = 60003;
    giver.ai.spawn = point(2.0);
    giver.ai.reaction = ai::Reaction::Passive;
    giver.max_health = 100;
    giver.name = "Authored quest giver".into();
    p.npcs.push(giver);
    let levels = pg::Catalog::new(
        1,
        8,
        pg::Rules::from_classic_config(60, true).unwrap(),
        (1..=3)
            .map(|level| pg::LevelStatsRow {
                race: 1,
                class: 8,
                level,
                attributes: init::Attributes([17, 23, 29, 31, 37].map(|v| v + level as i32 - 1)),
            })
            .collect(),
        (1..=3)
            .map(|level| pg::ClassLevelStatsRow {
                class: 8,
                level,
                base_health: 5000 + (level - 1) * 10,
                base_mana: 59 + (level - 1) * 6,
            })
            .collect(),
        (1..=3)
            .map(|level| pg::XpRow {
                level,
                xp_for_next_level: level * 40,
            })
            .collect(),
        input
            .skill_lines
            .iter()
            .map(|line| pg::SkillDependency {
                skill: line.id,
                line: Some(*line),
                race_class: init::Rows::Complete(vec![]),
            })
            .collect(),
    )
    .unwrap();
    let skills = [95, 8]
        .into_iter()
        .enumerate()
        .map(|(position, id)| {
            let x = &p.combat.as_ref().unwrap().skills[&id];
            pg::SkillState {
                id,
                position: position as u8,
                value: x.value,
                maximum: x.maximum,
                step: x.step,
                permanent_bonus: x.permanent_bonus,
                temporary_bonus: x.temporary_bonus,
                status: pg::SkillStatus::Unchanged,
            }
        })
        .collect();
    let row = q::QuestRow {
        id: 70001,
        method: 2,
        active: true,
        eligibility: arithmetic::QuestEligibilityInput {
            minimum_level: 1,
            maximum_level: 60,
            required_classes: 0,
            required_races: 0,
        },
        reward: arithmetic::QuestRewardInput {
            xp: arithmetic::QuestXpInput {
                quest_level: 1,
                reward_money_max_level: 60,
            },
            reward_or_required_money: 7,
            quest_flags: 0,
        },
        special_flags: q::KILL | q::SPEAK,
        previous_quest: 0,
        next_quest: 0,
        next_in_chain: 0,
        required_item: [0; 4],
        required_item_count: [0; 4],
        required_creature: [60001, 0, 0, 0],
        required_creature_count: [1, 0, 0, 0],
        excluded: excluded(),
    };
    let relation = q::CreatureQuestRelation {
        creature: 60003,
        quest: 70001,
    };
    let data = QuestProgressionInput {
        dialog_rules: QuestDialogRules {
            always_show_quest_greeting: true,
            low_level_hide_diff: 5,
            high_level_hide_diff: 7,
        },
        quests: vec![row],
        relations: q::QuestRelations {
            starters: vec![relation.clone()],
            finishers: vec![relation],
        },
        quest_audit: q::DependencyAudit {
            complete_relation_scan: true,
            questgiver_hooks: q::Evidence::Absent,
            spell_area: q::Evidence::Absent,
            collection_item_effects: q::Evidence::Absent,
        },
        levels,
        progression_audit: progression_audit(),
        derived_audit: derived_audit(),
        skills,
        initial_next_level_xp: 40,
        start_player_money: 11,
        quest_xp_rate: arithmetic::Binary32Rate::ONE,
        money_rate: arithmetic::Binary32Rate::ONE,
        npc_rows: vec![QuestNpcRow {
            entry: 60003,
            npc_flags: 2,
            unit_flags: 0,
        }],
        abilities: p
            .abilities
            .iter()
            .cloned()
            .map(|ability| AbilityRecipe {
                ability,
                cast_time: Some(CastTimeRow {
                    id: 77,
                    base_ms: 200,
                    per_level_ms: 0,
                    minimum_ms: 200,
                }),
            })
            .collect(),
        kill: NativeKillPolicy {
            rules: kill::Rules {
                kill_rate_bits: 0x3f800000,
            },
            clock: kill::OnlineRestContract {
                rule: kill::OnlineRestRule::SimulationContinuationWithoutSourceLogout,
                offline_intervals: pg::Evidence::Absent,
                entered_rest_area: pg::Evidence::Absent,
            },
            audit: KillDependencyAudit {
                kill_procs: pg::Evidence::Absent,
                kill_scripts: pg::Evidence::Absent,
                other_damage_sources: pg::Evidence::Absent,
                tap_overrides: pg::Evidence::Absent,
                native_loot_or_money: pg::Evidence::Absent,
            },
            creatures: p
                .npcs
                .iter()
                .map(|n| KillCreatureRow {
                    entry: n.entry,
                    experience_multiplier_bits: 0x3f800000,
                    is_totem: false,
                    is_pet: false,
                    is_no_xp: false,
                    is_critter: false,
                    is_elite: false,
                    player_controlled: false,
                    civilian_for_target: pg::Evidence::Absent,
                    racial_leader: pg::Evidence::Absent,
                    reputation: kill::ReputationAdmission::ProvenAbsentOnKillRow { entry: n.entry },
                    kill_credit: [0; 2],
                })
                .collect(),
        },
    };
    (p, input, data)
}
pub fn profile() -> Profile {
    let (mut p, input, data) = setup();
    p.initialize_quest_progression(&input, data).unwrap();
    p
}
#[derive(Default)]
pub struct QuestAuthority {
    pub base: KernelAuthority,
    pub deny: bool,
    pub wrong: bool,
}
impl Authority for QuestAuthority {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if let Query::QuestStatus { .. } = request.query {
            let mut echo = request.clone();
            if self.wrong {
                echo.binding.tick += 1;
            }
            return Ok(AuthorityReply {
                request: echo,
                facts: Facts::QuestStatus {
                    player_reaction: if self.deny { 2 } else { 4 },
                    npc_reaction: 4,
                },
            });
        }
        if let Query::QuestInteraction { .. } = request.query {
            let mut echo = request.clone();
            if self.wrong {
                echo.binding.tick += 1;
            }
            return Ok(AuthorityReply {
                request: echo,
                facts: Facts::QuestInteraction(QuestInteractionFacts {
                    player_in_world: true,
                    player_taxi_flying: false,
                    player_cannot_react_or_lost_control: false,
                    player_charmed: false,
                    npc_charmed: false,
                    npc_pet_in_combat: false,
                    npc_invisible_to_alive: false,
                    player_reaction: if self.deny { 2 } else { 4 },
                    npc_reaction: 4,
                    line_of_sight: false,
                }),
            });
        }
        self.base.resolve(request)
    }
}
pub fn world() -> NativeWorld {
    NativeWorld::synthetic_component_test(profile(), [1; 16], player(), 42).unwrap()
}
pub fn command(w: &NativeWorld, c: Command) -> OwnedInput {
    OwnedInput {
        authenticated_owner: w.state.player.owner,
        input: Input {
            world: w.state.world,
            player: w.state.player,
            tick: w.state.tick + 1,
            sequence: w.state.next_sequence,
            command: c,
        },
    }
}
pub fn accept(w: &mut NativeWorld, a: &mut QuestAuthority) {
    let i = command(
        w,
        Command::AcceptQuest {
            quest: 70001,
            giver: w.state.npcs[&3].id,
        },
    );
    w.step(w.state.tick + 1, Some(i), a).unwrap();
}
pub fn kill_target(w: &mut NativeWorld, a: &mut QuestAuthority) {
    let i = command(
        w,
        Command::Cast {
            ability: 60001,
            target: w.state.npcs[&2].id,
        },
    );
    w.step(w.state.tick + 1, Some(i), a).unwrap();
    for _ in 0..20 {
        if w.state.npcs[&2].health == 0 {
            return;
        }
        w.step(w.state.tick + 1, None, a).unwrap();
    }
    panic!("authored target did not die");
}

/// Authored future active abilities, learned in source row order rather than ID order.
pub fn learned_setup() -> (Profile, init::StarterInput, QuestProgressionInput) {
    let (mut profile, mut input, mut data) = setup();
    for (index, id) in [60005, 60004].into_iter().enumerate() {
        let mut raw = input.spells[0].clone();
        raw.id = id;
        raw.words[0] = id;
        raw.words[18] = 78;
        raw.words[21] = 2;
        input.spells.push(raw);
        let init::Rows::Complete(rows) = &mut input.skill_abilities else {
            unreachable!()
        };
        rows.push(init::SkillAbilityRow {
            row_id: 3 + index as u32,
            skill: 8,
            spell: id,
            race_mask: 0,
            class_mask: 0,
            required_value: 6,
            learn_on_get_skill: 1,
            forward_spell_id: 0,
            max_value: 0,
        });
        input.active_support.push(init::ActiveAbilitySupport {
            spell_id: id,
            unavailable_reason: None,
        });
        let mut recipe = data.abilities[0].clone();
        recipe.ability.id = id;
        if let AbilityAdmission::Available(spec) = &mut recipe.ability.admission {
            spec.spell_id = id;
            spec.cast_ms = 3000;
            spec.damage_pushback = true;
        }
        recipe.cast_time = Some(CastTimeRow {
            id: 78,
            base_ms: 3000,
            per_level_ms: 0,
            minimum_ms: 3000,
        });
        data.abilities.push(recipe);
    }
    profile.initialize_player(&input, float(100.0), 1).unwrap();
    (profile, input, data)
}
