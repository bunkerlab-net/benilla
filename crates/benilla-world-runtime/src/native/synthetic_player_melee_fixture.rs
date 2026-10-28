// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored source-shaped mainhand profiles for exact controller tests only.
//! No private inputs, production capabilities or native admission.
use super::*;
use super::{
    synthetic_combat_fixture, synthetic_creature_generation_fixture, synthetic_fixture as fixture,
    synthetic_motion_fixture, synthetic_quest_fixture,
};
use benilla_sim::{player_progression as pg, starter_init as init};

pub fn profile() -> Profile {
    profile_with_health(1)
}

pub(in crate::native) fn profile_with_health(health: u32) -> Profile {
    profile_with_parameters(health, false)
}

#[cfg(test)]
pub(in crate::native) fn fragile_profile() -> Profile {
    profile_with_parameters(60, true)
}

pub(in crate::native) fn profile_with_parameters(health: u32, fragile_player: bool) -> Profile {
    profile_with_home_case(health, fragile_player, None)
}

pub(in crate::native) fn profile_with_home_case(
    health: u32,
    fragile_player: bool,
    home_spawn: Option<f32>,
) -> Profile {
    profile_with_options(health, fragile_player, home_spawn, false)
}

pub(in crate::native) fn profile_with_options(
    health: u32,
    fragile_player: bool,
    home_spawn: Option<f32>,
    fragile_weapon: bool,
) -> Profile {
    profile_with_options_and_spell(health, fragile_player, home_spawn, fragile_weapon, false)
}

pub fn source_fireball_profile(health: u32) -> Profile {
    profile_with_options_and_spell(health, false, None, false, true)
}

pub(in crate::native) fn profile_with_options_and_spell(
    health: u32,
    fragile_player: bool,
    home_spawn: Option<f32>,
    fragile_weapon: bool,
    source_fireball: bool,
) -> Profile {
    let (mut profile, mut input, mut rows) =
        synthetic_quest_fixture::setup_with_levels(3, 60, false, 0x3f800000);
    if source_fireball {
        crate::native::synthetic_spell_fixture::rekey(&mut profile, &mut input);
        rows.abilities[0].ability = profile.abilities[0].clone();
        rows.abilities[0].cast_time.as_mut().unwrap().id = 901;
        let mut source = crate::native::synthetic_spell_fixture::source(&profile);
        source.spells[0].words[21] |= 8; // source SPELL_INTERRUPT_FLAG_COMBAT
        input
            .spells
            .iter_mut()
            .find(|row| row.id == 133)
            .unwrap()
            .words = source.spells[0].words.clone().try_into().unwrap();
    }
    if fragile_player {
        input.create_health = 1;
        input.create_attributes.0[2] = 1;
    }
    let init::Rows::Complete(skills) = &mut input.default_skills else {
        unreachable!()
    };
    for id in [136, 162] {
        skills.push(init::DefaultSkill { id, step: 0 });
        input
            .skill_lines
            .push(init::SkillLineRow { id, category: 6 });
    }
    let mut weapon = input.items[0].clone();
    weapon.id = 900002;
    weapon.class = 2;
    weapon.subclass = 10;
    weapon.inventory_type = 17;
    weapon.armor = 0;
    weapon.stats = [init::ItemStat { kind: 0, value: 0 }; 10];
    weapon.stats[0] = init::ItemStat { kind: 3, value: 3 };
    weapon.damage[0] = init::ItemDamage {
        min_bits: 2.0f32.to_bits(),
        max_bits: 4.0f32.to_bits(),
        school: 0,
    };
    weapon.delay_ms = 2500;
    weapon.max_durability = if fragile_weapon { 1 } else { 1000 };
    input.items.push(weapon);
    let init::Rows::Complete(grants) = &mut input.item_grants else {
        unreachable!()
    };
    grants.push(init::ItemGrant {
        source: init::GrantSource::Custom,
        source_order: 1,
        item: 900002,
        count: 1,
    });
    profile
        .initialize_player(&input, crate::native::synthetic_fixture::float(100.0), 1)
        .unwrap();
    for (position, id) in [(2, 136), (3, 162)] {
        let skill = &profile.combat.as_ref().unwrap().skills[&id];
        rows.skills.push(pg::SkillState {
            id,
            position,
            value: skill.value,
            maximum: skill.maximum,
            step: skill.step,
            permanent_bonus: skill.permanent_bonus,
            temporary_bonus: skill.temporary_bonus,
            status: pg::SkillStatus::Unchanged,
        });
    }
    // The existing fixture's authored level rows, extended with an actual
    // staff SkillLine lookup. These are test rows, never native admission.
    rows.levels = pg::Catalog::new(
        1,
        8,
        rows.levels.rules(),
        (1..=3)
            .map(|level| pg::LevelStatsRow {
                race: 1,
                class: 8,
                level,
                attributes: init::Attributes(
                    [17, 23, if fragile_player { 1 } else { 29 }, 31, 37]
                        .map(|v| v + level as i32 - 1),
                ),
            })
            .collect(),
        (1..=3)
            .map(|level| pg::ClassLevelStatsRow {
                class: 8,
                level,
                base_health: (if fragile_player { 1 } else { 5000 }) + (level - 1) * 10,
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
    // Preserve distinct owners; this fixture deliberately has no passive script writer.
    for npc in &mut profile.npcs {
        npc.ai.reaction = ai::Reaction::Defensive;
        npc.faction_template = if npc.ai.creature_id == 2 {
            2
        } else if source_fireball {
            3
        } else {
            1
        };
        npc.ai.neutral_to_all = npc.ai.creature_id == 3;
        npc.ai.spawn = fixture::point(if npc.ai.creature_id == 2 { 2.0 } else { 4.0 });
    }
    if let Some(spawn) = home_spawn {
        // Author the source profile before motion, relations, and raw creature
        // initialization. The real AI must enter Home; no phase is injected.
        let target = profile
            .npcs
            .iter_mut()
            .find(|npc| npc.ai.creature_id == 2)
            .unwrap();
        target.ai.reaction = ai::Reaction::Aggressive;
        target.ai.spawn = fixture::point(spawn);
        target.ai.hard_leash_radius = Some(fixture::real(0.1));
        profile.npcs[1].faction_template = 3;
        profile.player.orientation = Orientation::new(std::f32::consts::PI).unwrap();
    }
    let mut relations = synthetic_combat_fixture::source_rows(&profile);
    if home_spawn.is_some() || source_fireball {
        // A neutral second target is attackable by the player but is not a
        // friendly assistance peer of the hostile victim. This is raw relation
        // input, not suppression of an actual assistance callback.
        // The Home test observes player pursuit, not the unsupported peer attack
        // branch. Its separate quest giver has genuinely neutral source masks.
        relations.factions.push(InteractionFactionSource {
            template_id: 3,
            faction: 0,
            template_flags: 0,
            group_mask: 0,
            friend_group_mask: 0,
            enemy_group_mask: 0,
            enemies: [0; 4],
            friends: [0; 4],
            reputation_list_id: None,
        });
    }
    for npc in &mut relations.npcs {
        npc.npc_flags = u32::from(npc.entry == 60003) * 2;
    }
    profile.initialize_interactions(relations).unwrap();
    let mut single = profile.clone();
    single.npcs = vec![profile.npcs[0].clone()];
    let mut motion = synthetic_motion_fixture::genesis(&single, 2);
    single.npcs = vec![profile.npcs[1].clone()];
    motion
        .npcs
        .extend(synthetic_motion_fixture::genesis(&single, 3).npcs);
    // Explicit authored facing: the victim initially faces the player, so
    // parry/block are reachable rather than silently restricted to rear dodge.
    motion.npcs.get_mut(&2).unwrap().npc_spawn_orientation =
        Orientation::new(std::f32::consts::PI).unwrap();
    profile.source_motion = Some(motion);
    let mut capabilities = synthetic_motion_fixture::creature_capability_sources(&profile);
    let CreatureCapabilityRows::Complete(capability_rows) = &mut capabilities else {
        unreachable!()
    };
    for source in capability_rows {
        source.requested_spawn_template_entry = source.spawn_entry;
        let CreatureCapabilityRows::Complete(spawns) = &mut source.spawn_templates else {
            unreachable!()
        };
        spawns[0].entry = source.spawn_entry;
    }
    profile.creature_capabilities =
        Some(CreatureCapabilityCatalog::validate_sources(capabilities).unwrap());
    profile.source_visitors = Some(
        SourceVisitorCatalog::validate_sources(
            crate::native::synthetic_two_owner_fixture::visitor_input(&profile, vec![2, 3]),
        )
        .unwrap(),
    );
    profile
        .initialize_combat_authority(
            profile
                .npcs
                .iter()
                .map(|npc| NpcCombatAuthoritySource {
                    entry: npc.entry,
                    extra_flags: match npc.ai.reaction {
                        ai::Reaction::Defensive => 2,
                        ai::Reaction::Aggressive => 0,
                        ai::Reaction::Passive => unreachable!(),
                    },
                    static_flags1: 0,
                    static_flags2: 0,
                    static_flags3: 0,
                    static_flags4: 0,
                })
                .collect(),
        )
        .unwrap();
    let mut sources = synthetic_creature_generation_fixture::complete_scene_sources(&profile);
    // Distinct authored spawn-template rows may retain different current health.
    // The capability capture above names these same rows, not a shared entry zero.
    for source in &mut sources {
        source.requested_spawn_template_entry = source.spawn_entry;
        let benilla_sim::creature_init::Rows::Complete(spawns) = &mut source.input.spawn_template
        else {
            unreachable!()
        };
        spawns[0].entry = source.spawn_entry;
        if source_fireball && source.spawn_id == 3 {
            spawns[0].current_health = 1;
        }
    }
    let target = sources
        .iter_mut()
        .find(|source| source.spawn_id == 2)
        .unwrap();
    let benilla_sim::creature_init::Rows::Complete(spawns) = &mut target.input.spawn_template
    else {
        unreachable!()
    };
    spawns[0].current_health = health;
    profile = synthetic_creature_generation_fixture::install(profile, sources, vec![2, 3]);
    profile.initialize_quest_progression(&input, rows).unwrap();
    profile.initialize_facing_authority().unwrap();
    if fragile_player {
        use benilla_sim::player_death as death;
        profile
            .configure_player_death(
                death::DeathConfig {
                    map_instanceable: false,
                    pve_reclaim_delay_enabled: true,
                },
                death::DeathSupport::checked(
                    &death::REQUIRED_ABSENCES.map(|entry| (entry, Some(false))),
                )
                .unwrap(),
            )
            .unwrap();
    }
    if source_fireball {
        let mut source = crate::native::synthetic_spell_fixture::source(&profile);
        source.spells[0].words[21] |= 8;
        profile.initialize_spell_authority(&input, source).unwrap();
        crate::native::synthetic_spell_fixture::initialize_authored_location(&mut profile);
    }
    profile.initialize_player_melee(1).unwrap();
    profile
}
