// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::native::source_execution::authored_effect_holder;
use benilla_sim::source_rng::{DrawCursor, Mt19937, SourceRandom};

fn shirt() -> StartupItemPrototype {
    StartupItemPrototype {
        class: 4,
        inventory_type: 4,
        allowable_class: u32::MAX,
        allowable_race: u32::MAX,
        buy_count: 1,
        stackable: 1,
        ..Default::default()
    }
}

fn fixture(proto: StartupItemPrototype, run: impl FnOnce(&mut SourcePlayer, &mut Outfit<'_>)) {
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let mut player = super::super::tests::construct(&mut objects, &mut units);
    let handle = player.object();
    objects
        .assign_guid(handle, 1, SourceGuid::from_raw(1))
        .unwrap();
    player.race = 1;
    let unit = units.get_mut(&objects, handle).unwrap();
    unit.power_fields = Some(SourceUnitPowerFields {
        level: 1,
        create_health: 31,
        create_mana: 100,
        auxiliary_current: None,
        auxiliary_maximum: None,
    });
    let mut templates = SourceStartupItems::load_enchantments([(1, 1, 100.0)]);
    let mut rng = Mt19937::from_seed(1);
    let mut cursor = DrawCursor::default();
    let rows = BTreeMap::from([(0, proto), (1, proto)]);
    templates
        .validate(
            rows,
            &[1].into(),
            &mut SourceRandom::new(1, &mut rng, &mut cursor, &mut |_| {}).unwrap(),
        )
        .unwrap();
    let mut items = SourceItems::default();
    let mut guids = super::super::item_recording::guid_allocators();
    run(
        &mut player,
        &mut Outfit {
            objects: &mut objects,
            unit,
            spells: &SourceSpellStores::default(),
            templates: &templates,
            items: &mut items,
            guids: &mut guids,
            observe: &mut |_| {},
            class: 8,
            logging_out: false,
        },
    );
}

#[test]
fn probe_dies_after_resolving_the_empty_equipment_slot() {
    fixture(shirt(), |player, live| {
        let probe = ObjectHandle {
            thread: 1,
            construction: 2,
        };
        assert_eq!(player.can_equip_new_item(live, 1).unwrap(), Some(3));
        assert_eq!(live.objects.construction_count(1), 2);
        assert!(live.objects.get(probe).is_err());
        assert!(live.items.get(probe).is_err());
        assert!(player.items.iter().all(Option::is_none));
        assert_eq!(live.guids.allocate(HighGuid::Item).unwrap().counter, 2);
    });
}

#[test]
fn factory_guards_preserve_construction_and_guid_order() {
    fixture(
        StartupItemPrototype {
            stackable: 5,
            spell_charges: [-1, 0, 0, 0, 0],
            max_durability: 25,
            ..shirt()
        },
        |player, live| {
            let mut make = |entry, count| {
                live.items.create(
                    live.objects,
                    live.guids,
                    player.object(),
                    entry,
                    count,
                    live.templates,
                    live.observe,
                )
            };
            assert!(make(0, 0).unwrap().is_none());
            assert!(make(999, 1).unwrap().is_none());
            let handle = make(0, 7).unwrap().unwrap();
            assert_eq!(handle.construction, 2);
            assert_eq!(live.objects.get(handle).unwrap().guid.unwrap().counter(), 1);
            let item = live.items.get(handle).unwrap();
            assert_eq!(
                (
                    item.count,
                    item.durability,
                    item.max_durability,
                    item.charges
                ),
                (5, 25, 25, [-1, 0, 0, 0, 0])
            );
            assert_eq!(item.contained.raw(), 0);
        },
    );
    for (proto, expected, built) in [
        (
            StartupItemPrototype {
                inventory_type: 18,
                ..shirt()
            },
            "execution.item.Bag_77.constructor_owner",
            false,
        ),
        (
            StartupItemPrototype {
                stackable: 0,
                ..shirt()
            },
            "execution.item.Item_968.stackable_assert",
            false,
        ),
        (
            StartupItemPrototype {
                random_property: 1,
                ..shirt()
            },
            "execution.item.Item_649.random_property_owner",
            true,
        ),
    ] {
        fixture(proto, |player, live| {
            let result = player.can_equip_new_item(live, 0);
            assert_eq!(result.unwrap_err().id, expected);
            assert_eq!(live.objects.construction_count(1), 1 + u64::from(built));
            assert!(live
                .objects
                .get(ObjectHandle {
                    thread: 1,
                    construction: 2
                })
                .is_err());
            assert_eq!(
                live.guids.allocate(HighGuid::Item).unwrap().counter,
                1 + u32::from(built)
            );
        });
    }
}

#[test]
fn probe_checks_template_and_live_equip_refusals_before_placement() {
    for (proto, expected) in [
        (
            StartupItemPrototype {
                allowable_class: 0,
                ..shirt()
            },
            "Player_9826.item_race_class",
        ),
        (
            StartupItemPrototype {
                required_skill: 415,
                ..shirt()
            },
            "Player_9830.required_skill_owner",
        ),
        (
            StartupItemPrototype {
                required_spell: 204,
                ..shirt()
            },
            "Player_9837.required_spell",
        ),
        (
            StartupItemPrototype {
                required_honor_rank: 1,
                ..shirt()
            },
            "Player_9839.honor_rank_owner",
        ),
        (
            StartupItemPrototype {
                required_level: 2,
                ..shirt()
            },
            "Player_9843.item_level",
        ),
        (
            StartupItemPrototype {
                inventory_type: 1,
                ..shirt()
            },
            "Player_7932.equip_slot_type_owner",
        ),
        (
            StartupItemPrototype {
                subclass: 1,
                ..shirt()
            },
            "Player_9807.item_proficiency",
        ),
        (
            StartupItemPrototype {
                subclass: 2,
                ..shirt()
            },
            "Item_591.armor_skill_owner",
        ),
        (
            StartupItemPrototype {
                class: 2,
                subclass: 3,
                ..shirt()
            },
            "Item_584.weapon_skill_owner",
        ),
        (
            StartupItemPrototype {
                required_reputation_faction: 72,
                ..shirt()
            },
            "Player_9810.item_reputation_owner",
        ),
        (
            StartupItemPrototype {
                flags: 0x80000,
                ..shirt()
            },
            "Player_19593.unique_equip_owner",
        ),
        (
            StartupItemPrototype {
                class: 11,
                ..shirt()
            },
            "Player_9498.quiver_owner",
        ),
    ] {
        fixture(proto, |player, live| {
            let error = player.can_equip_new_item(live, 0).unwrap_err();
            assert_eq!(
                error.id,
                format!(
                    "execution.{}.{expected}",
                    if expected.starts_with("Item") {
                        "item"
                    } else {
                        "player"
                    }
                )
            );
            assert!(player.items.iter().all(Option::is_none));
            assert_eq!(live.objects.construction_count(1), 2);
            assert!(live
                .objects
                .get(ObjectHandle {
                    thread: 1,
                    construction: 2
                })
                .is_err());
        });
    }
    fixture(shirt(), |player, live| {
        live.unit.flags = 0xc0000;
        assert_eq!(
            player.can_equip_new_item(live, 0).unwrap_err().id,
            "execution.player.Player_9459.stunned"
        );
        live.unit.flags = 0x80000;
        assert_eq!(
            player.can_equip_new_item(live, 0).unwrap_err().id,
            "execution.player.Player_9466.in_combat"
        );
        live.unit.flags = 0;
        live.logging_out = true;
        assert_eq!(
            player.can_equip_new_item(live, 0).unwrap_err().id,
            "execution.player.Player_9471.logging_out"
        );
        live.logging_out = false;
        let template = SpellTemplate {
            id: 44,
            ..Default::default()
        };
        let aura = authored_effect_holder(&template, player.object(), 176);
        live.unit
            .auras
            .store_aura_holder(44, &mut Some(aura))
            .unwrap();
        assert_eq!(
            player.can_equip_new_item(live, 0).unwrap_err().id,
            "execution.player.Player_9478.spirit_of_redemption"
        );
        live.unit
            .spell_slots
            .replace_slot(SpellSlot::AutoRepeat, 1, &mut live.unit.spell_starts.spells)
            .unwrap();
        assert_eq!(
            player.can_equip_new_item(live, 0).unwrap_err().id,
            "execution.player.Player_9474.non_melee_cast"
        );
    });
}

#[test]
fn equip_refuses_the_unreached_item_callbacks_after_placement() {
    let mut cases = vec![
        (
            StartupItemPrototype {
                item_set: 1,
                ..shirt()
            },
            "execution.player.Player_10085.item_set_owner",
        ),
        (
            StartupItemPrototype {
                stat_values: [1; 10],
                ..shirt()
            },
            "execution.player.Player_7063.item_stat_owner",
        ),
        (
            StartupItemPrototype {
                block: 1,
                ..shirt()
            },
            "execution.player.Player_7098.item_block_owner",
        ),
    ];
    for (i, owner) in [
        "execution.player.Player_7101.item_holy_resistance_owner",
        "execution.player.Player_7104.item_fire_resistance_owner",
        "execution.player.Player_7107.item_nature_resistance_owner",
        "execution.player.Player_7110.item_frost_resistance_owner",
        "execution.player.Player_7113.item_shadow_resistance_owner",
        "execution.player.Player_7116.item_arcane_resistance_owner",
    ]
    .into_iter()
    .enumerate()
    {
        let mut proto = shirt();
        proto.resistances[i] = 1;
        cases.push((proto, owner));
    }
    for (proto, expected) in cases {
        fixture(proto, |player, live| {
            let slot = player.can_equip_new_item(live, 1).unwrap().unwrap();
            assert_eq!(
                player.equip_new_item(live, 1, slot).unwrap_err().id,
                expected
            );
            let handle = player.items[slot].unwrap();
            assert_eq!(live.items.get(handle).unwrap().owner.raw(), 1);
            assert_eq!(player.item_update_queue, [Some(handle)]);
        });
    }
}

fn staff() -> StartupItemPrototype {
    StartupItemPrototype {
        class: 2,
        subclass: 10,
        inventory_type: 17,
        first_damage: [3.0f32.to_bits(), 5.0f32.to_bits(), 0],
        delay: 2900,
        ..shirt()
    }
}

#[test]
fn staff_refuses_unbound_damage_or_speed_paths_at_their_source_writes() {
    fixture(
        StartupItemPrototype {
            first_damage: [3.0f32.to_bits(), 5.0f32.to_bits(), 2],
            ..staff()
        },
        |player, live| {
            assert_eq!(
                player.equip_new_item(live, 1, 15).unwrap_err().id,
                "execution.player.Player_7152.weapon_damage_school_owner"
            );
            assert_eq!(live.unit.creature_damage.mainhand_base_weapon(), [3.0, 5.0]);
            assert_eq!(player.attack_times[0], 0.0);
        },
    );
    fixture(
        StartupItemPrototype {
            second_damage_max: 4.0f32.to_bits(),
            ..staff()
        },
        |player, live| {
            assert_eq!(
                player.equip_new_item(live, 1, 15).unwrap_err().id,
                "execution.player.Player_7147.extra_weapon_damage_line_owner"
            );
            assert_eq!(live.unit.creature_damage.mainhand_base_weapon(), [3.0, 5.0]);
            assert_eq!(player.attack_times[0], 0.0);
        },
    );
    fixture(staff(), |player, live| {
        live.unit.loaded_creature.can_modify_stats = true;
        assert_eq!(
            player.equip_new_item(live, 1, 15).unwrap_err().id,
            "execution.player.Player_7172.weapon_damage_update_owner"
        );
        assert_eq!(player.attack_times[0], 2900.0);
    });
}

fn backpack_item() -> StartupItemPrototype {
    StartupItemPrototype {
        class: 15,
        inventory_type: 0,
        max_count: 1,
        bonding: 1,
        ..shirt()
    }
}

#[test]
fn backpack_recheck_excludes_the_moving_bound_item() {
    fixture(backpack_item(), |player, live| {
        player.store_new_item_in_best_slots(live, 1, 1).unwrap();
        let handle = player.items[23].unwrap();
        let guid = live.objects.get(handle).unwrap().guid;
        let item = live.items.get(handle).unwrap();
        assert_eq!(
            (
                item.slot,
                item.flags,
                item.owner.raw(),
                item.contained.raw()
            ),
            (23, 1, 1, 1)
        );
        assert_eq!(item.queue_pos, Some(0));
        assert_eq!(player.item_update_queue, [Some(handle)]);
        assert_eq!(live.objects.construction_count(1), 3);

        let proto = live.templates.item_prototype(1).unwrap().unwrap();
        assert_eq!(
            player
                .can_store_item(live, None, 1, 1, proto)
                .unwrap_err()
                .id,
            "execution.player.Player_8571.item_max_count_exceeded"
        );
        player.recheck_backpack(live).unwrap();
        assert_eq!(live.objects.construction_count(1), 3);
        assert_eq!(player.items[23], Some(handle));
        assert_eq!(player.item_update_queue, [Some(handle)]);
        assert_eq!(live.objects.get(handle).unwrap().guid, guid);
        let item = live.items.get(handle).unwrap();
        assert_eq!(
            (
                item.slot,
                item.flags,
                item.owner.raw(),
                item.contained.raw()
            ),
            (23, 1, 1, 1)
        );
        assert_eq!(item.queue_pos, Some(0));

        assert_eq!(
            player.can_equip_new_item(live, 1).unwrap_err().id,
            "execution.player.Player_8571.item_max_count_exceeded"
        );
        player.remove_backpack_item(live, handle, 23).unwrap();
        let item = live.items.get(handle).unwrap();
        assert_eq!(
            (item.slot, item.owner.raw(), item.contained.raw()),
            (u8::MAX, 1, 0)
        );
        assert!(player.items[23].is_none());
        assert_eq!(player.item_update_queue, [Some(handle)]);
    });
}

#[test]
fn backpack_refuses_unreached_placement_behaviors() {
    fixture(
        StartupItemPrototype {
            bag_family: 9,
            ..backpack_item()
        },
        |player, live| {
            let error = player.store_new_item_in_best_slots(live, 1, 1).unwrap_err();
            assert_eq!(error.id, "execution.player.Player_8964.keyring_owner");
            assert!(player.items.iter().all(Option::is_none));
            assert_eq!(live.objects.construction_count(1), 2);
        },
    );
    fixture(
        StartupItemPrototype {
            max_count: 0,
            stackable: 20,
            ..backpack_item()
        },
        |player, live| {
            player.store_new_item_in_best_slots(live, 1, 5).unwrap();
            let before = player.items[23].unwrap();
            let error = player.store_new_item_in_best_slots(live, 1, 1).unwrap_err();
            assert_eq!(error.id, "execution.player.Player_8798.stack_merge_owner");
            assert_eq!(live.items.get(before).unwrap().count, 5);
            assert_eq!(live.objects.construction_count(1), 4);
        },
    );
}
