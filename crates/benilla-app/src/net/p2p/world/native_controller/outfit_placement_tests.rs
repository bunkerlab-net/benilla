// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored outfit rows through the actual controller, projection and checkpoint store.
use super::*;

struct OutfitCatalog {
    items: Vec<init::ItemRow>,
}
impl ClientCatalog for OutfitCatalog {
    fn build(&self) -> u32 {
        LootCatalog.build()
    }
    fn manifest_digest(&self) -> runtime::Digest {
        LootCatalog.manifest_digest()
    }
    fn contains_map(&self, id: u32) -> bool {
        LootCatalog.contains_map(id)
    }
    fn contains_creature_display(&self, id: u32) -> bool {
        LootCatalog.contains_creature_display(id)
    }
    fn contains_item_display(&self, id: u32) -> bool {
        LootCatalog.contains_item_display(id)
    }
    fn contains_spell(&self, id: u32) -> bool {
        LootCatalog.contains_spell(id)
    }
    fn player_power_type(&self, id: PlayerId) -> Option<u8> {
        LootCatalog.player_power_type(id)
    }
    fn player_bytes_2(&self, id: PlayerId) -> Option<u32> {
        LootCatalog.player_bytes_2(id)
    }
    fn item_metadata(&self, id: u32) -> Option<benilla_protocol::messages::ItemInfo> {
        let Some(row) = self.items.iter().find(|row| row.id == id) else {
            return LootCatalog.item_metadata(id);
        };
        let mut info = LootCatalog.item_metadata(900001)?;
        info.class = row.class;
        info.subclass = row.subclass;
        info.inventory_type = row.inventory_type;
        info.stackable = row.stackable;
        info.stats.clear();
        info.armor = 0;
        info.max_durability = row.max_durability;
        if row.class == 2 {
            info.damages = vec![benilla_protocol::messages::ItemDamage {
                min: f32::from_bits(row.damage[0].min_bits),
                max: f32::from_bits(row.damage[0].max_bits),
                school: row.damage[0].school,
            }];
            info.dmg_min = info.damages[0].min;
            info.dmg_max = info.damages[0].max;
            info.delay_ms = row.delay_ms;
        }
        Some(info)
    }
    fn quest_metadata(&self, id: u32) -> Option<benilla_world_client::QuestMetadata> {
        LootCatalog.quest_metadata(id)
    }
    fn quest_greeting(&self, id: u32) -> Option<String> {
        LootCatalog.quest_greeting(id)
    }
    fn native_quest_greeting(&self, id: u32) -> Option<benilla_world_client::NativeQuestGreeting> {
        LootCatalog.native_quest_greeting(id)
    }
    fn native_quest_presentation(
        &self,
        id: u32,
    ) -> Option<benilla_world_client::NativeQuestPresentation> {
        LootCatalog.native_quest_presentation(id)
    }
}

fn outfit_profile() -> (n::Profile, OutfitCatalog) {
    let (mut p, mut input, mut quests) = loot::setup();
    input.skill_lines.push(init::SkillLineRow {
        id: 136,
        category: 6,
    });
    let init::Rows::Complete(skills) = &mut input.default_skills else {
        unreachable!()
    };
    skills.push(init::DefaultSkill { id: 136, step: 0 });
    let mut raw = Vec::new();
    for (n, kind) in [20, 7, 8, 17, 4, 0, 0, 0].into_iter().enumerate() {
        let mut item = input.items[0].clone();
        item.id = 900_010 + n as u32;
        item.inventory_type = kind;
        item.stats = [init::ItemStat { kind: 0, value: 0 }; 10];
        item.armor = 0;
        if n == 3 {
            item.class = 2;
            item.subclass = 10;
            item.damage[0] = init::ItemDamage {
                min_bits: 2.0f32.to_bits(),
                max_bits: 4.0f32.to_bits(),
                school: 0,
            };
            item.delay_ms = 2500;
        } else if n >= 5 {
            item.class = if n == 7 { 15 } else { 0 };
            item.stackable = if n == 7 { 1 } else { 20 };
            item.max_durability = 0;
        }
        raw.push(init::StartingItemRow {
            item,
            buy_count: if n == 5 || n == 6 { 5 } else { 1 },
            first_spell_category: if n == 5 {
                11
            } else if n == 6 {
                59
            } else {
                0
            },
        });
    }
    let capture = init::CapturedStarterItems {
        outfits: init::Rows::Complete(vec![init::CharStartOutfitRow {
            id: 900_020,
            race_class_gender: 0x0801,
            items: [
                900010, 900011, 900012, 900013, 900014, 900015, 900016, 900017, 0, -1, 0, 0,
            ],
        }]),
        custom: init::Rows::Complete(vec![]),
        items: raw,
    };
    input = capture.compile(input).unwrap();
    let mut prototypes = vec![loot::item(70001)];
    for row in &input.items {
        let mut item = loot::item(row.id);
        item.class = row.class;
        item.inventory_type = row.inventory_type;
        item.stackable = row.stackable as i32;
        item.maximum_durability = row.max_durability;
        item.display_id = 900002;
        prototypes.push(item);
    }
    quests.loot.as_mut().unwrap().inventory = loot::inventory_catalog(prototypes);
    p.initialize_player(&input, fixture::float(100.0), 1)
        .unwrap();
    let skill = init::initialize_mage(&input).unwrap().skills[&136];
    quests
        .skills
        .push(benilla_sim::player_progression::SkillState {
            id: 136,
            position: 2,
            value: skill.value,
            maximum: skill.maximum,
            step: skill.step,
            permanent_bonus: skill.permanent_bonus,
            temporary_bonus: skill.temporary_bonus,
            status: benilla_sim::player_progression::SkillStatus::Unchanged,
        });
    use benilla_sim::player_progression as pg;
    quests.levels = pg::Catalog::new(
        input.race,
        input.class,
        quests.levels.rules(),
        (1..=3)
            .map(|level| pg::LevelStatsRow {
                race: input.race,
                class: input.class,
                level,
                attributes: init::Attributes(
                    input.create_attributes.0.map(|v| v + level as i32 - 1),
                ),
            })
            .collect(),
        (1..=3)
            .map(|level| pg::ClassLevelStatsRow {
                class: input.class,
                level,
                base_health: input.create_health + (level - 1) * 10,
                base_mana: input.create_mana + (level - 1) * 6,
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
    p.initialize_quest_progression(&input, quests).unwrap();
    (p, OutfitCatalog { items: input.items })
}

fn enter_outfit(h: &Harness, open: bool) -> NativeController {
    let (p, catalog) = outfit_profile();
    NativeController::enter_component_test(
        &h.root,
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
        p,
        &catalog,
        loot::LootAuthority::default(),
    )
    .unwrap()
}

#[test]
fn captured_outfit_controller_starts_with_equipment_and_inventory_and_reopens_exactly() {
    let h = Harness::new();
    let mut c = enter_outfit(&h, false);
    let initial = c.initial_events().unwrap();
    let mut fields = benilla_protocol::ObjectFields::default();
    for event in &initial {
        if let SessionEvent::ObjectValues {
            guid,
            fields: value,
        } = event
        {
            if *guid == c.player_guid() {
                fields.merge(value.clone());
            }
        }
    }
    for (slot, entry) in [
        (4, 900010),
        (6, 900011),
        (7, 900012),
        (15, 900013),
        (3, 900014),
    ] {
        assert_eq!(fields.player_visible_item_entry(slot), Some(entry));
    }
    assert_eq!(
        inventory(&c)
            .slots
            .iter()
            .map(|(&slot, item)| (slot, item.item, item.count))
            .collect::<Vec<_>>(),
        [(23, 900015, 5), (24, 900016, 5), (25, 900017, 1)]
    );
    let items = inventory(&c).clone();
    let gear = c.world.state().combat.as_ref().unwrap().equipment.clone();
    step(&mut c);
    let digest = c.world.state_digest();
    drop(c);
    let mut reopened = enter_outfit(&h, true);
    assert_eq!(reopened.world.state_digest(), digest);
    assert_eq!(inventory(&reopened), &items);
    assert_eq!(
        reopened.world.state().combat.as_ref().unwrap().equipment,
        gear
    );
    let events = reopened.initial_events().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, SessionEvent::ItemCreate { .. }))
            .count(),
        8
    );
    assert!(!events
        .iter()
        .any(|event| matches!(event, SessionEvent::ItemPushResult(_))));
}
