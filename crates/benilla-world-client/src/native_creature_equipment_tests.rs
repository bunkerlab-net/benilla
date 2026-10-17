// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored source rows and actual unit packets, never native admission or assets.
use super::*;
use benilla_sim::creature_init as init;
use n::{synthetic_creature_model_fixture as models, synthetic_fixture as f};
fn item(weapon: bool) -> init::ItemTemplateRow {
    let mut damage = [init::ItemDamage {
        minimum: f::float(0.0),
        maximum: f::float(0.0),
        school: 0,
    }; 5];
    damage[0] = init::ItemDamage {
        minimum: f::float(900.0),
        maximum: f::float(1200.0),
        school: 2,
    };
    init::ItemTemplateRow {
        entry: 80001,
        display_id: 81,
        class: if weapon { 2 } else { 4 },
        subclass: if weapon { 10 } else { 6 },
        material: -1,
        inventory_type: if weapon { 17 } else { 14 },
        sheath: 2,
        delay_ms: 9900,
        damage,
    }
}
fn metadata(item: &init::ItemTemplateRow) -> p::ItemInfo {
    p::ItemInfo {
        class: item.class,
        subclass: item.subclass,
        name: "Authored virtual mainhand".into(),
        display_info_id: item.display_id,
        quality: 1,
        flags: 0,
        buy_price: 0,
        sell_price: 0,
        inventory_type: item.inventory_type,
        allowable_class: -1,
        allowable_race: -1,
        item_level: 1,
        required_level: 0,
        required_skill: 0,
        required_skill_rank: 0,
        required_spell: 0,
        required_honor_rank: 0,
        required_city_rank: 0,
        required_rep_faction: 0,
        required_rep_rank: 0,
        max_count: 0,
        stackable: 1,
        container_slots: 0,
        stats: vec![],
        damages: item
            .damage
            .iter()
            .filter(|d| f32::from_bits(d.maximum.bits()) > 0.0)
            .map(|d| p::ItemDamage {
                min: f32::from_bits(d.minimum.bits()),
                max: f32::from_bits(d.maximum.bits()),
                school: d.school,
            })
            .collect(),
        dmg_min: f32::from_bits(item.damage[0].minimum.bits()),
        dmg_max: f32::from_bits(item.damage[0].maximum.bits()),
        dmg_type: item.damage[0].school,
        armor: 0,
        resistances: [0; 6],
        delay_ms: item.delay_ms,
        ammo_type: 0,
        ranged_mod_range: 0.0,
        spells: vec![],
        spell_charges_0: 0,
        use_spell: None,
        bonding: 0,
        description: String::new(),
        page_text: 0,
        language_id: 0,
        page_material: 0,
        start_quest: 0,
        lock_id: 0,
        material: item.material as u32,
        sheath: item.sheath,
        random_property: 0,
        block: 0,
        item_set: 0,
        max_durability: 0,
        area: 0,
        map: 0,
        bag_family: 0,
    }
}
struct Catalog {
    info: Option<p::ItemInfo>,
    display: bool,
}
impl ClientCatalog for Catalog {
    fn build(&self) -> u32 {
        5875
    }
    fn manifest_digest(&self) -> Digest {
        [9; 32]
    }
    fn contains_map(&self, id: u32) -> bool {
        id == 7
    }
    fn contains_creature_display(&self, id: u32) -> bool {
        matches!(id, 31 | 34)
    }
    fn contains_item_display(&self, id: u32) -> bool {
        self.display && id == 81
    }
    fn contains_spell(&self, id: u32) -> bool {
        matches!(id, 60001 | 60002)
    }
    fn player_power_type(&self, _: PlayerId) -> Option<u8> {
        Some(0)
    }
    fn item_metadata(&self, id: u32) -> Option<p::ItemInfo> {
        (id == 80001).then(|| self.info.clone()).flatten()
    }
    fn quest_metadata(&self, _: u32) -> Option<crate::QuestMetadata> {
        None
    }
    fn quest_greeting(&self, _: u32) -> Option<String> {
        None
    }
}
fn world(
    mainhand: Option<init::ItemTemplateRow>,
    disarmed: bool,
    spawn_zero: bool,
) -> n::NativeWorld {
    let mut input = models::input();
    input.template.extra_flags = 2;
    input.template.regenerate_stats = 0;
    input.template.corpse_delay_seconds = 1;
    input.template.unit_flags = if disarmed { 0x0020_0000 } else { 0 };
    if let Some(item) = mainhand {
        input.template.equipment_template_id = 12;
        input.equipment_templates = init::Rows::Complete(vec![init::EquipmentTemplateRow {
            entry: 12,
            item_entries: [item.entry, 0, 0],
        }]);
        input.item_templates = init::Rows::Complete(vec![item]);
    }
    let init::Rows::Complete(spawn) = &mut input.spawn_template else {
        unreachable!()
    };
    spawn[0].current_health = 1;
    if spawn_zero {
        spawn[0].equipment_id = 0;
    }
    let mut profile = f::profile();
    profile.npcs[0].ai.spawn = f::point(3.0);
    profile.npcs[0].faction_template = input.template.faction;
    profile.npcs[0].health_regeneration.regenerate_stats = 0;
    profile.npcs[0].health_regeneration.unit_flags = input.template.unit_flags;
    profile
        .initialize_creature_generations(
            init::Rows::Complete(vec![n::CreatureGenerationSource {
                spawn_id: 2,
                spawn_entry: input.template.entry,
                requested_spawn_template_entry: 0,
                respawn_delay_min_seconds: 2,
                respawn_delay_max_seconds: 2,
                input,
            }]),
            vec![2],
        )
        .unwrap();
    n::NativeWorld::synthetic_component_test(profile, [8; 16], f::player(), 1234).unwrap()
}
fn assert_fields(world: &n::NativeWorld, fields: &ObjectFields) {
    let selected = world.state().npcs[&2].initialized_creature().unwrap();
    for slot in 0..3 {
        let item = (slot == 0).then_some(selected.equipment.mainhand).flatten();
        assert_eq!(
            fields.unit_virtual_item_display(slot),
            Some(item.map_or(0, |item| item.display_id))
        );
        let word = item.map_or(0, |item| item.info[0]);
        assert_eq!(
            fields.unit_virtual_item_info(slot),
            Some((
                word as u8,
                (word >> 8) as u8,
                (word >> 16) as u8,
                (word >> 24) as u8
            ))
        );
        assert_eq!(
            fields.unit_virtual_item_sheath(slot),
            Some(item.map_or(0, |item| item.info[1] as u8))
        );
    }
    assert_eq!(fields.unit_sheath_state(), Some(1));
    assert!(creature_equipment::fields(selected).contains(&(164, 0x1001)));
    assert_eq!(
        fields.unit_flags() & 0x0020_0000,
        selected.unit_flags & 0x0020_0000
    );
    assert_eq!(
        fields.unit_min_damage().unwrap().to_bits(),
        selected.stats.damage.minimum.bits()
    );
    assert_eq!(
        fields.unit_max_damage().unwrap().to_bits(),
        selected.stats.damage.maximum.bits()
    );
    assert_eq!(fields.unit_base_attack_time(0), Some(2300));
    assert!(f32::from_bits(selected.stats.damage.maximum.bits()) < 900.0);
}
fn assert_create(world: &n::NativeWorld, projection: &NativeProjection, events: &[SessionEvent]) {
    let target = projection.npc_guid(world.state().npcs[&2].id).unwrap();
    let fields = events
        .iter()
        .find_map(|event| match event {
            SessionEvent::ObjectCreate { guid, fields, .. } if *guid == target => Some(fields),
            _ => None,
        })
        .unwrap();
    assert_fields(world, fields);
    assert!(!events
        .iter()
        .any(|event| matches!(event, SessionEvent::ItemCreate { .. })));
}
#[test]
fn virtual_mainhand_create_values_death_respawn_restore_keep_source_fields() {
    for (mainhand, disarmed, spawn_zero) in [
        (Some(item(true)), false, false),
        (Some(item(true)), true, false),
        (Some(item(false)), true, false),
        (None, true, false),
        (Some(item(true)), false, true),
    ] {
        let catalog = Catalog {
            info: mainhand.as_ref().map(metadata),
            display: true,
        };
        let mut world = world(mainhand, disarmed, spawn_zero);
        let mut projection = NativeProjection::new(&world, world.state().player, &catalog).unwrap();
        assert_create(
            &world,
            &projection,
            &projection.initial_events(&world).unwrap(),
        );
        let old_id = world.state().npcs[&2].id;
        let old_guid = projection.npc_guid(old_id).unwrap();
        let initial = world.state().npcs[&2]
            .initialized_creature()
            .unwrap()
            .equipment;
        let mut authority = f::SyntheticAuthority::default();
        let cast = f::cast(&world, 60001);
        world
            .step(world.state().tick + 1, Some(cast), &mut authority)
            .unwrap();
        let mut saw_dead = false;
        let mut destroyed = false;
        let mut respawned = false;
        for _ in 0..100 {
            let events = projection.advance(&world).unwrap();
            for event in &events {
                if let SessionEvent::ObjectValues { guid, fields } = event {
                    if *guid == old_guid {
                        assert_fields(&world, fields);
                        saw_dead |= fields.unit_health() == Some(0);
                    }
                }
                destroyed |=
                    matches!(event, SessionEvent::ObjectDestroyed(guid) if *guid == old_guid);
            }
            if world.state().npcs[&2].id != old_id {
                assert!(destroyed);
                assert_create(&world, &projection, &events);
                assert_eq!(
                    world.state().npcs[&2]
                        .initialized_creature()
                        .unwrap()
                        .equipment,
                    initial
                );
                respawned = true;
                break;
            }
            world
                .step(world.state().tick + 1, None, &mut authority)
                .unwrap();
        }
        assert!(saw_dead && respawned);
        let restored = n::NativeWorld::restore_synthetic_component_test(
            world.profile().clone(),
            world.checkpoint().unwrap(),
        )
        .unwrap();
        let resumed = NativeProjection::new(&restored, restored.state().player, &catalog).unwrap();
        assert_create(
            &restored,
            &resumed,
            &resumed.initial_events(&restored).unwrap(),
        );
    }
}
#[test]
fn selected_item_catalog_rejects_missing_display_and_every_shared_metadata_mismatch() {
    let raw = item(true);
    let world = world(Some(raw), false, false);
    let good = metadata(&raw);
    for index in 0..13 {
        let mut info = good.clone();
        match index {
            0 => info.display_info_id += 1,
            1 => info.class += 1,
            2 => info.subclass += 1,
            3 => info.material = 255, // Same packed byte, different raw Material.
            4 => info.inventory_type += 1,
            5 => info.sheath += 1,
            6 => info.delay_ms += 1,
            7 => info.dmg_min += 1.0,
            8 => info.dmg_max += 1.0,
            9 => info.dmg_type += 1,
            10 => info.damages[0].min += 1.0,
            11 => info.damages[0].school += 1,
            _ => info.damages.clear(),
        }
        let catalog = Catalog {
            info: Some(info),
            display: true,
        };
        assert_eq!(
            NativeProjection::new(&world, world.state().player, &catalog).err(),
            Some(Error::InvalidItemMetadata(raw.entry))
        );
    }
    let missing = Catalog {
        info: None,
        display: true,
    };
    assert_eq!(
        NativeProjection::new(&world, world.state().player, &missing).err(),
        Some(Error::MissingItemMetadata(raw.entry))
    );
    let unbound = Catalog {
        info: Some(good),
        display: false,
    };
    assert_eq!(
        NativeProjection::new(&world, world.state().player, &unbound).err(),
        Some(Error::UnboundItemDisplay(raw.entry))
    );
}
#[test]
fn unselected_retained_item_needs_no_client_display_or_item_metadata() {
    let world = world(Some(item(true)), false, true);
    let catalog = Catalog {
        info: None,
        display: false,
    };
    let projection = NativeProjection::new(&world, world.state().player, &catalog).unwrap();
    assert_create(
        &world,
        &projection,
        &projection.initial_events(&world).unwrap(),
    );
}
#[test]
fn zero_raw_display_keeps_weapon_semantics_but_requires_client_render_binding() {
    let mut raw = item(true);
    raw.display_id = 0;
    let world = world(Some(raw), true, false);
    let selected = world.state().npcs[&2].initialized_creature().unwrap();
    assert!(selected.equipment.mainhand.unwrap().has_weapon);
    assert_eq!(selected.equipment.mainhand.unwrap().display_id, 0);
    let catalog = Catalog {
        info: Some(metadata(&raw)),
        display: true,
    };
    // Source hasWeapon depends on class; display availability is a separate
    // client-install boundary. This catalog cannot render display zero.
    assert_eq!(
        NativeProjection::new(&world, world.state().player, &catalog).err(),
        Some(Error::UnboundItemDisplay(raw.entry))
    );
}
