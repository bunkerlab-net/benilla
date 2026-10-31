// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored raw spell/loot/initializer composition, never native content admission.
use super::*;
use synthetic_fixture as f;

pub fn profile() -> Profile {
    profile_with_lethal(false)
}
pub fn profile_with_lethal(lethal: bool) -> Profile {
    build(lethal, false, None, false, false, None)
}
pub fn mainhand_profile() -> Profile {
    build(false, true, None, false, false, None)
}
pub fn profile_with_armor_type(inventory_type: u32) -> Profile {
    build(false, false, Some(inventory_type), false, false, None)
}
pub fn profile_with_duplicate_armor() -> Profile {
    build(false, false, None, true, false, None)
}
/// Two different source-authored original armor slots, sealed by the ordinary initializer.
pub fn profile_with_two_originals() -> Profile {
    build(false, false, None, false, true, None)
}
/// Source-authored original staff, raw spell/loot owners and actual unarmed/staff skills.
pub fn weapon_profile() -> Profile {
    build(false, true, None, false, false, Some(17))
}
pub fn lethal_weapon_profile() -> Profile {
    build(true, true, None, false, false, Some(17))
}
/// Authored inventory type is bound in both initializer and loot rows before sealing.
pub fn weapon_profile_with_type(inventory_type: u32) -> Profile {
    build(false, true, None, false, false, Some(inventory_type))
}
fn build(
    lethal: bool,
    mainhand: bool,
    armor_type: Option<u32>,
    duplicate_armor: bool,
    second_original: bool,
    weapon_type: Option<u32>,
) -> Profile {
    let original_weapon = weapon_type.is_some();
    let (mut p, mut input, mut rows) = synthetic_loot_fixture::setup();
    if original_weapon {
        use benilla_sim::starter_init as init;
        let mut weapon = input.items[0].clone();
        weapon.id = 900002;
        weapon.class = 2;
        weapon.subclass = 10;
        weapon.inventory_type = weapon_type.unwrap();
        weapon.armor = 0;
        weapon.stats = [init::ItemStat { kind: 0, value: 0 }; 10];
        weapon.stats[0] = init::ItemStat { kind: 3, value: 3 };
        weapon.damage[0] = init::ItemDamage {
            min_bits: 2.0f32.to_bits(),
            max_bits: 4.0f32.to_bits(),
            school: 0,
        };
        weapon.delay_ms = 2500;
        weapon.max_durability = 1000;
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
        let init::Rows::Complete(skills) = &mut input.default_skills else {
            unreachable!()
        };
        skills.push(init::DefaultSkill { id: 136, step: 0 });
        input.skill_lines.push(init::SkillLineRow {
            id: 136,
            category: 6,
        });
        let loot = rows.loot.as_mut().unwrap();
        let mut items: Vec<_> = loot.inventory.items().cloned().collect();
        let mut prototype = synthetic_loot_fixture::item(900002);
        prototype.class = 2;
        prototype.inventory_type = weapon_type.unwrap();
        prototype.display_id = 900003;
        prototype.stackable = 1;
        prototype.maximum_durability = 1000;
        items.push(prototype);
        loot.inventory = synthetic_loot_fixture::inventory_catalog(items);
    }
    if second_original {
        use benilla_sim::starter_init as init;
        let mut second = input
            .items
            .iter()
            .find(|item| item.id == 900001)
            .unwrap()
            .clone();
        second.id = 900003;
        second.inventory_type = 6; // Source singleton waist slot 5; chest stays slot 4.
        input.items.push(second);
        let init::Rows::Complete(grants) = &mut input.item_grants else {
            unreachable!()
        };
        grants.push(init::ItemGrant {
            source: init::GrantSource::Custom,
            source_order: 1,
            item: 900003,
            count: 1,
        });
        let loot = rows.loot.as_mut().unwrap();
        let mut items: Vec<_> = loot.inventory.items().cloned().collect();
        let mut prototype = items
            .iter()
            .find(|item| item.entry == 900001)
            .unwrap()
            .clone();
        prototype.entry = 900003;
        prototype.inventory_type = 6;
        items.push(prototype);
        loot.inventory = synthetic_loot_fixture::inventory_catalog(items);
    }
    if duplicate_armor {
        use benilla_sim::starter_init as init;
        let entry = input.items.iter().find(|i| i.class == 4).unwrap().id;
        let init::Rows::Complete(grants) = &mut input.item_grants else {
            unreachable!()
        };
        grants
            .iter_mut()
            .find(|grant| grant.item == entry)
            .unwrap()
            .count = 2;
    }
    if let Some(kind) = armor_type {
        let original = input.items.iter_mut().find(|i| i.class == 4).unwrap();
        original.inventory_type = kind;
        let entry = original.id;
        let loot = rows.loot.as_mut().unwrap();
        let mut items: Vec<_> = loot.inventory.items().cloned().collect();
        items
            .iter_mut()
            .find(|i| i.entry == entry)
            .unwrap()
            .inventory_type = kind;
        loot.inventory = synthetic_loot_fixture::inventory_catalog(items);
    }

    if mainhand {
        use benilla_sim::{player_progression as pg, starter_init as init};
        let init::Rows::Complete(skills) = &mut input.default_skills else {
            unreachable!()
        };
        skills.push(init::DefaultSkill { id: 162, step: 0 });
        input.skill_lines.push(init::SkillLineRow {
            id: 162,
            category: 6,
        });
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
                        [17, 23, 29, 31, 37].map(|v| v + level as i32 - 1),
                    ),
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
        p.npcs[0].ai.spawn = f::point(2.0);
    }
    p.npcs[0].max_health = 1000;
    if lethal {
        p.npcs[0].combat.as_mut().unwrap().minimum_damage = f::float(10000.0);
        p.npcs[0].combat.as_mut().unwrap().maximum_damage = f::float(10000.0);
    }
    p.npcs[0].ai.reaction = ai::Reaction::Defensive;
    p.npcs[1].ai.reaction = ai::Reaction::Defensive;
    p.npcs[1].ai.neutral_to_all = true;
    synthetic_spell_fixture::rekey(&mut p, &mut input);
    if let AbilityAdmission::Available(spec) = &mut p.abilities[0].admission {
        spec.cast_ms = 2000;
    }
    rows.abilities[0].ability = p.abilities[0].clone();
    rows.abilities[0].cast_time.as_mut().unwrap().id = 901;
    rows.abilities[0].cast_time.as_mut().unwrap().base_ms = 2000;
    let mut source = synthetic_spell_fixture::source(&p);
    source.spells[0].words[59] = u32::MAX;
    if original_weapon {
        // Authored source SPELL_INTERRUPT_FLAG_COMBAT: successful release resets BASE/OFF.
        source.spells[0].words[21] |= 8;
    }
    input.spells[0].words = source.spells[0].words.clone().try_into().unwrap();
    p.initialize_player(&input, f::float(100.0), 1).unwrap();
    if mainhand {
        let skill = &p.combat.as_ref().unwrap().skills[&162];
        rows.skills
            .push(benilla_sim::player_progression::SkillState {
                id: 162,
                position: 2,
                value: skill.value,
                maximum: skill.maximum,
                step: skill.step,
                permanent_bonus: skill.permanent_bonus,
                temporary_bonus: skill.temporary_bonus,
                status: benilla_sim::player_progression::SkillStatus::Unchanged,
            });
    }
    if original_weapon {
        let skill = &p.combat.as_ref().unwrap().skills[&136];
        rows.skills
            .push(benilla_sim::player_progression::SkillState {
                id: 136,
                position: 3,
                value: skill.value,
                maximum: skill.maximum,
                step: skill.step,
                permanent_bonus: skill.permanent_bonus,
                temporary_bonus: skill.temporary_bonus,
                status: benilla_sim::player_progression::SkillStatus::Unchanged,
            });
    }
    // A genuinely neutral second owner cannot assist the hostile cast victim.
    p.npcs[0].faction_template = 2;
    p.npcs[1].faction_template = 3;
    let mut relations = synthetic_combat_fixture::source_rows(&p);
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
    for npc in &mut relations.npcs {
        npc.npc_flags = u32::from(npc.entry == 60003) * 2;
    }
    p.initialize_interactions(relations).unwrap();
    let mut one = p.clone();
    one.npcs = vec![p.npcs[0].clone()];
    let mut motion = synthetic_motion_fixture::genesis(&one, 2);
    one.npcs = vec![p.npcs[1].clone()];
    motion
        .npcs
        .extend(synthetic_motion_fixture::genesis(&one, 3).npcs);
    p.source_motion = Some(motion);
    let mut capabilities = synthetic_motion_fixture::creature_capability_sources(&p);
    if mainhand {
        let CreatureCapabilityRows::Complete(sources) = &mut capabilities else {
            unreachable!()
        };
        for source in sources {
            source.requested_spawn_template_entry = source.spawn_entry;
            let CreatureCapabilityRows::Complete(spawns) = &mut source.spawn_templates else {
                unreachable!()
            };
            spawns[0].entry = source.spawn_entry;
        }
    }
    p.creature_capabilities =
        Some(CreatureCapabilityCatalog::validate_sources(capabilities).unwrap());
    p.source_visitors = Some(
        SourceVisitorCatalog::validate_sources(synthetic_two_owner_fixture::visitor_input(
            &p,
            vec![2, 3],
        ))
        .unwrap(),
    );
    p.initialize_combat_authority(
        p.npcs
            .iter()
            .map(|n| NpcCombatAuthoritySource {
                entry: n.entry,
                extra_flags: if n.ai.reaction == ai::Reaction::Defensive {
                    2
                } else {
                    0
                },
                static_flags1: 0,
                static_flags2: 0,
                static_flags3: 0,
                static_flags4: 0,
            })
            .collect(),
    )
    .unwrap();
    if mainhand {
        let mut sources = synthetic_creature_generation_fixture::complete_scene_sources(&p);
        for source in &mut sources {
            source.requested_spawn_template_entry = source.spawn_entry;
            let benilla_sim::creature_init::Rows::Complete(spawns) =
                &mut source.input.spawn_template
            else {
                unreachable!()
            };
            spawns[0].entry = source.spawn_entry;
            if original_weapon {
                // Both creature projections name the same global class/level row.
                // Retain one coherent authored table, with per-spawn current health.
                assert_eq!(source.input.template.health_multiplier, f::float(1.0));
                if source.spawn_id == 2 {
                    spawns[0].current_health = 1000;
                }
                let benilla_sim::creature_init::Rows::Complete(levels) =
                    &mut source.input.class_levels
                else {
                    unreachable!()
                };
                for level in levels {
                    level.base_health = 1000;
                    if lethal {
                        level.base_damage = f::float(10000.0);
                    }
                }
            }
        }
        p = synthetic_creature_generation_fixture::install(p, sources, vec![2, 3]);
        if original_weapon {
            assert_eq!(p.npcs[0].max_health, 1000);
        }
    }
    p.initialize_quest_progression(&input, rows).unwrap();
    if mainhand {
        p.initialize_facing_authority().unwrap();
    }
    p.initialize_spell_authority(&input, source).unwrap();
    synthetic_spell_fixture::initialize_authored_location(&mut p);
    if lethal {
        use benilla_sim::player_death as death;
        p.configure_player_death(
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
    if mainhand {
        p.initialize_player_melee(1).unwrap();
    }
    p
}
pub fn world() -> NativeWorld {
    NativeWorld::synthetic_component_test(profile(), [1; 16], f::player(), 42).unwrap()
}
#[derive(Default)]
pub struct EquipmentAuthority {
    pub scene: synthetic_two_owner_fixture::CompleteAuthority,
    pub spell: synthetic_spell_fixture::SpatialOnly,
    pub loot: synthetic_loot_fixture::LootAuthority,
}
impl Authority for EquipmentAuthority {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        match request.query {
            Query::SourceSpell(_) | Query::SourceLocation(_) => self.spell.resolve(request),
            Query::LootInteraction { .. } => self.loot.resolve(request),
            _ => self.scene.resolve(request),
        }
    }
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        if matches!(reply.request.query, Query::SourceSpell(_)) {
            self.spell.observe_derived(reply)
        } else {
            self.scene.observe_derived(reply)
        }
    }
}
