// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored lifecycle operands and source closure, never installation rows.
use super::*;
use benilla_sim::starter_init as init;

pub fn aura_source(id: u32) -> SpellAuthoritySource {
    let armor = id == 168;
    let mut w = vec![0; 173];
    for (i, v) in [
        (0, id),
        (1, 4),
        (4, 1),
        (5, if armor { 0 } else { 11 }),
        (6, if armor { 0x10000 } else { 0 }),
        (7, if armor { 0 } else { 0x488 }),
        (18, 1),
        (21, if armor { 8 } else { 0 }),
        (24, if armor { 40 } else { 0 }),
        (25, if armor { 100 } else { 101 }),
        (28, 1),
        (29, 1),
        (30, if armor { 900 } else { 901 }),
        (32, if armor { 7 } else { 0 }),
        (36, if armor { 1 } else { 4 }),
        (58, u32::MAX),
        (59, u32::MAX),
        (61, 6),
        (62, 6),
        (64, 1),
        (65, u32::from(!armor)),
        (67, 1),
        (68, u32::from(!armor)),
        (76, if armor { 44 } else { (-16i32) as u32 }),
        (77, if armor { 0 } else { (-41i32) as u32 }),
        (82, if armor { 1 } else { 6 }),
        (83, if armor { 1 } else { 6 }),
        (91, if armor { 22 } else { 138 }),
        (92, if armor { 42 } else { 33 }),
        (106, u32::from(armor)),
        (110, if armor { 6136 } else { 0 }),
        (157, if armor { 133 } else { 0 }),
        (158, if armor { 1500 } else { 0 }),
        (160, 3),
        (161, if armor { 0x2080000 } else { 0x100000 }),
        (164, 1),
        (165, 1),
        (166, u32::MAX),
        (167, 0x3f800000),
        (168, 0x3f800000),
        (169, 0x3f800000),
    ] {
        w[i] = v;
    }
    let duration = if armor { 4000 } else { 1200 };
    SpellAuthoritySource {
        words: w,
        server_side: 0,
        server_attributes: 0,
        bonus_coefficients: [0; 3],
        attack_power_coefficients: [0; 3],
        cast_time: SpellCastTimeSource {
            id: 1,
            base_ms: 0,
            per_level_ms: 0,
            minimum_ms: 0,
        },
        duration: Some(SpellDurationSource {
            id: if armor { 900 } else { 901 },
            base_ms: duration,
            per_level_ms: 0,
            maximum_ms: duration,
        }),
        range: SpellRangeSource {
            id: if armor { 1 } else { 4 },
            minimum: spell::SpellFloat::ZERO,
            maximum: if armor {
                spell::SpellFloat::ZERO
            } else {
                synthetic_fixture::float(30.0)
            },
            flags: 0,
        },
        category_flags: 0,
        facing_flags: 0,
        hooks: synthetic_spell_fixture::hooks(),
    }
}
pub fn profile() -> Profile {
    build(false, false)
}
#[cfg(test)]
pub(in crate::native) fn mainhand_profile() -> Profile {
    build(true, false)
}
pub(in crate::native) fn food_drink_profile() -> Profile {
    build(false, true)
}
fn build(mainhand: bool, consumables: bool) -> Profile {
    let (mut profile, mut initial, mut progression) =
        synthetic_distinct_quest_fixture::setup_with_unbound_spells();
    if consumables {
        for npc in &mut profile.npcs {
            npc.ai.spawn = synthetic_fixture::point(100.0 + npc.ai.creature_id as f32);
        }
    }
    if mainhand {
        let init::Rows::Complete(skills) = &mut initial.default_skills else {
            panic!("authored skills")
        };
        skills.push(init::DefaultSkill { id: 162, step: 0 });
        initial.skill_lines.push(init::SkillLineRow {
            id: 162,
            category: 6,
        });
        progression.levels = synthetic_quest_fixture::level_catalog(
            &initial,
            progression.levels.supplied_levels().max().unwrap(),
            progression.levels.rules(),
        );
    }
    let mut fireball = synthetic_spell_fixture::source(&profile).spells.remove(0);
    fireball.words[21] |= 8;
    initial
        .spells
        .iter_mut()
        .find(|s| s.id == 133)
        .unwrap()
        .words = fireball.words.clone().try_into().unwrap();
    let armor = aura_source(168);
    let chilled = aura_source(6136);
    let mut root = initial.spells[0].clone();
    root.id = 168;
    root.words = armor.words.clone().try_into().unwrap();
    root.duration_ms = Some(4000);
    initial.spells.push(root.clone());
    root.id = 6136;
    root.words = chilled.words.clone().try_into().unwrap();
    root.duration_ms = Some(1200);
    initial.spells.push(root);
    if consumables {
        synthetic_food_drink_fixture::augment(&mut initial, &mut progression);
    }
    let init::Rows::Complete(roots) = &mut initial.default_spells else {
        panic!("authored roots")
    };
    roots.push(168);
    initial.active_support.push(init::ActiveAbilitySupport {
        spell_id: 168,
        unavailable_reason: None,
    });
    profile.abilities.push(Ability {
        id: 168,
        school: 4,
        admission: AbilityAdmission::FrostArmor,
    });
    profile.abilities.sort_by_key(|a| a.id);
    profile
        .initialize_player(&initial, synthetic_fixture::float(100.0), 1)
        .unwrap();
    if mainhand {
        let skill = &profile.combat.as_ref().unwrap().skills[&162];
        progression
            .skills
            .push(benilla_sim::player_progression::SkillState {
                id: 162,
                position: progression
                    .skills
                    .iter()
                    .map(|s| s.position)
                    .max()
                    .unwrap_or(0)
                    + 1,
                value: skill.value,
                maximum: skill.maximum,
                step: skill.step,
                permanent_bonus: skill.permanent_bonus,
                temporary_bonus: skill.temporary_bonus,
                status: benilla_sim::player_progression::SkillStatus::Unchanged,
            });
    }
    progression.abilities.push(AbilityRecipe {
        ability: profile.ability(168).unwrap().clone(),
        cast_time: Some(CastTimeRow {
            id: 1,
            base_ms: 0,
            per_level_ms: 0,
            minimum_ms: 0,
        }),
    });
    progression.abilities.sort_by_key(|a| a.ability.id);
    profile
        .initialize_quest_progression(&initial, progression)
        .unwrap();
    profile = synthetic_creature_model_fixture::with_complete_scene_generations(profile);
    let mut spells = vec![fireball, armor, chilled];
    if consumables {
        spells.extend([430, 433].map(synthetic_food_drink_fixture::source));
    }
    spells.sort_by_key(|s| s.words[0]);
    let ids = spells.iter().map(|s| s.words[0]).collect();
    let mut source = synthetic_spell_fixture::context(&profile, spells);
    source.modifier_spell_ids = SpellSourceRows::Complete(ids);
    profile
        .initialize_spell_authority(&initial, source)
        .unwrap();
    synthetic_spell_fixture::initialize_authored_location(&mut profile);
    if mainhand {
        profile.initialize_player_melee(1).unwrap();
    }
    profile
}

#[derive(Default)]
pub struct SpatialOnly {
    scene: synthetic_distinct_quest_fixture::SpatialOnly,
    location: synthetic_spell_fixture::SpatialOnly,
}
impl Authority for SpatialOnly {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if matches!(request.query, Query::SourceLocation(_)) {
            self.location.resolve(request)
        } else {
            self.scene.resolve(request)
        }
    }
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        self.scene.observe_derived(reply)
    }
}
