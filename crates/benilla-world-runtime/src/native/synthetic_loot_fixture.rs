// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored loot/storage placeholders, never native-content admission.
use super::*;
use benilla_sim::{
    inventory as inv, loot as l, player_progression::Evidence, starter_init as init,
};
pub fn item(entry: u32) -> inv::Item {
    inv::Item {
        entry,
        class: 15,
        inventory_type: 0,
        display_id: 70002,
        quality: 1,
        flags: 0,
        extra_flags: 0,
        stackable: 5,
        maximum_count: 0,
        bonding: 0,
        maximum_durability: 0,
        bag_family: 0,
        container_slots: 0,
        random_property: 0,
        start_quest: 0,
        duration: 0,
        item_set: 0,
        area: 0,
        map: 0,
        lock_id: 0,
        script_id: 0,
        spells: [inv::Spell {
            id: 0,
            trigger: 0,
            charges: 0,
            ppm_rate_bits: 0,
            cooldown: 0,
            category: 0,
            category_cooldown: 0,
        }; 5],
    }
}
pub fn inventory_catalog(items: Vec<inv::Item>) -> inv::Catalog {
    inv::Catalog::new(
        items,
        inv::Audit {
            item_creation_and_loot_scripts: inv::Evidence::Absent,
            equipped_bags: inv::Evidence::Absent,
            bank_and_keyring_items: inv::Evidence::Absent,
            trade_or_temporary_item_loot: inv::Evidence::Absent,
            additional_item_mutations: inv::Evidence::Absent,
        },
    )
    .unwrap()
}
pub fn setup() -> (Profile, init::StarterInput, QuestProgressionInput) {
    let (p, input, mut q) = synthetic_quest_fixture::setup();
    let mut items = vec![item(70001)];
    for old in &input.items {
        let mut row = item(old.id);
        row.class = old.class;
        row.inventory_type = old.inventory_type;
        row.display_id = 900002;
        row.stackable = old.stackable as i32;
        row.maximum_count = old.unique_max_count as i32;
        row.maximum_durability = old.max_durability;
        row.spells = std::array::from_fn(|index| inv::Spell {
            id: old.spells[index].id,
            trigger: old.spells[index].trigger,
            charges: 0,
            ppm_rate_bits: 0,
            cooldown: 0,
            category: 0,
            category_cooldown: 0,
        });
        items.push(row);
    }
    let capture = l::Capture {
        upstream_commit: l::UPSTREAM_COMMIT.into(),
        source_fingerprint: [0x5a; 32],
        loot_id: 99,
        complete_selected_projection: true,
        loader_order: l::LoaderOrder {
            origin: l::OrderOrigin::WholeTableLoaderResult,
            complete_result_observed: true,
            ordered_result_fingerprint: [0x6b; 32],
            result_row_count: 1,
            selected_row_positions: vec![0],
        },
        rows: vec![l::Row {
            item: 70001,
            chance_or_quest_chance_bits: 75f32.to_bits(),
            group: 0,
            min_count_or_reference: 1,
            max_count: 8,
            condition_id: 0,
        }],
        items: BTreeMap::from([(
            70001,
            l::ItemLookup::Present(l::Item {
                entry: 70001,
                quality: 1,
                display_id: 70002,
                flags: 0,
                start_quest: 0,
                extra_flags: 0,
                random_property: 0,
            }),
        )]),
    };
    q.kill.audit.native_loot_or_money = Evidence::Present;
    q.loot = Some(NativeLootInput {
        rules: l::Rules {
            quality_rate_bits: [1f32.to_bits(); 7],
            money_rate_bits: 1f32.to_bits(),
            corpse_empty_loot_show: false,
        },
        captures: vec![capture],
        creatures: p
            .npcs
            .iter()
            .map(|n| LootCreatureRow {
                entry: n.entry,
                loot_id: if n.ai.creature_id == 2 { 99 } else { 0 },
                minimum_money: 3,
                maximum_money: 9,
                no_loot: false,
                skinning_loot_id: 0,
                can_wield_loot: false,
                can_be_multitapped: false,
                is_pet: false,
                despawn_instantly: false,
                forever_corpse_duration: false,
                corpse_acceleration_decay_ms: 120_000,
            })
            .collect(),
        inventory: inventory_catalog(items),
        audit: LootAudit {
            just_died_and_owner_hooks: Evidence::Absent,
            instance_battleground_outdoor_pvp_hooks: Evidence::Absent,
            death_scripts_linking_formation: Evidence::Absent,
            corpse_timer_overrides: Evidence::Absent,
            skinning_and_pickpocket: Evidence::Absent,
            looting_stealth_aura_callbacks: Evidence::Absent,
            group_and_controlled_units: Evidence::Absent,
        },
    });
    (p, input, q)
}
pub fn profile() -> Profile {
    let (mut p, input, q) = setup();
    p.initialize_quest_progression(&input, q).unwrap();
    p
}
pub fn world() -> NativeWorld {
    NativeWorld::synthetic_component_test(profile(), [1; 16], synthetic_fixture::player(), 42)
        .unwrap()
}
#[derive(Default)]
pub struct LootAuthority {
    pub base: synthetic_quest_fixture::QuestAuthority,
    pub deny: bool,
    pub wrong: bool,
}
impl Authority for LootAuthority {
    fn resolve(&mut self, r: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if matches!(r.query, Query::LootInteraction { .. }) {
            let mut echo = r.clone();
            if self.wrong {
                echo.binding.tick += 1;
            }
            return Ok(AuthorityReply {
                request: echo,
                facts: Facts::LootInteraction {
                    player_in_world: true,
                    player_stunned: self.deny,
                },
            });
        }
        self.base.resolve(r)
    }
}

/// Original grant-only rows for source-shaped component tests. No production admission.
pub fn item_grant_setup() -> (Profile, init::StarterInput, QuestProgressionInput) {
    let (p, input, mut q) = setup();
    let loot = q.loot.as_mut().unwrap();
    let mut items: Vec<_> = loot.inventory.items().cloned().collect();
    items
        .iter_mut()
        .find(|i| i.entry == 70001)
        .unwrap()
        .maximum_count = 7;
    let mut bag = item(70003);
    bag.class = 1;
    bag.inventory_type = 18;
    bag.container_slots = 16;
    bag.stackable = 1;
    bag.bonding = 1;
    bag.spells[0] = inv::Spell {
        id: 80001,
        trigger: 0,
        charges: -5,
        ppm_rate_bits: 1.25f32.to_bits(),
        cooldown: 10,
        category: 4,
        category_cooldown: 20,
    };
    let mut charged = item(70004);
    charged.spells = bag.spells;
    charged.start_quest = 70002;
    charged.item_set = 80002;
    let mut random = item(70005);
    random.random_property = 80003;
    items.extend([bag, charged, random]);
    loot.inventory = inventory_catalog(items);
    (p, input, q)
}
pub fn item_grant_profile() -> Profile {
    let (mut p, input, q) = item_grant_setup();
    p.initialize_quest_progression(&input, q).unwrap();
    p
}
