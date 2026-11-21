// SPDX-License-Identifier: GPL-3.0-or-later
//! Equal item_template identities must agree across retained source consumers.
use super::*;
use benilla_sim::{creature_init as init, starter_init as starter};

pub(super) fn validate_item_overlaps(
    profile: &Profile,
    input: &init::Input,
) -> Result<(), NativeError> {
    let init::Rows::Complete(items) = &input.item_templates else {
        return Err(NativeError::AuthorityUnavailable(
            "creature equipment item source rows",
        ));
    };
    let items = items
        .iter()
        .map(|item| (item.entry, item))
        .collect::<BTreeMap<_, _>>();
    if let Some(combat) = &profile.combat {
        for weapon in &combat.equipment_weapons {
            let Some(item) = items.get(&weapon.item_id) else {
                continue;
            };
            if weapon.class != item.class
                || weapon.subclass != item.subclass
                || weapon.inventory_type != item.inventory_type
                || weapon.delay_ms != item.delay_ms
                || weapon.damage.len() != item.damage.len()
                || weapon
                    .damage
                    .iter()
                    .zip(item.damage)
                    .any(|(old, raw)| *old != (raw.minimum.bits(), raw.maximum.bits(), raw.school))
            {
                return Err(NativeError::AuthorityBinding);
            }
        }
    }
    if let Some(quests) = &profile.quest_progression {
        for starter in quests.retained_starter_items() {
            if let Some(item) = items.get(&starter.id) {
                validate_starter(item, starter)?;
            }
        }
        if let Some(loot) = &quests.loot {
            // A virtual-only item need not be owned, lootable or in inventory.
            for inventory in loot.inventory.items() {
                let Some(item) = items.get(&inventory.entry) else {
                    continue;
                };
                if inventory.class != item.class
                    || inventory.inventory_type != item.inventory_type
                    || inventory.display_id != item.display_id
                {
                    return Err(NativeError::AuthorityBinding);
                }
            }
            for starter in loot.starter_items.values() {
                if let Some(item) = items.get(&starter.id) {
                    validate_starter(item, starter)?;
                }
            }
        }
    }
    Ok(())
}
fn validate_starter(
    item: &init::ItemTemplateRow,
    old: &starter::ItemRow,
) -> Result<(), NativeError> {
    if old.class != item.class
        || old.subclass != item.subclass
        || old.inventory_type != item.inventory_type
        || old.delay_ms != item.delay_ms
        || old.damage.iter().zip(item.damage).any(|(old, raw)| {
            (old.min_bits, old.max_bits, old.school)
                != (raw.minimum.bits(), raw.maximum.bits(), raw.school)
        })
    {
        return Err(NativeError::AuthorityBinding);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn raw(old: &starter::ItemRow, display_id: u32) -> init::ItemTemplateRow {
        init::ItemTemplateRow {
            entry: old.id,
            display_id,
            class: old.class,
            subclass: old.subclass,
            material: -1,
            inventory_type: old.inventory_type,
            sheath: 1,
            delay_ms: old.delay_ms,
            damage: old.damage.map(|d| init::ItemDamage {
                minimum: spell::SpellFloat::from_bits(d.min_bits).unwrap(),
                maximum: spell::SpellFloat::from_bits(d.max_bits).unwrap(),
                school: d.school,
            }),
        }
    }
    fn change(item: &mut init::ItemTemplateRow, index: usize) {
        match index {
            0 => item.class += 1,
            1 => item.subclass += 1,
            2 => item.inventory_type += 1,
            3 => item.delay_ms += 1,
            4 => item.damage[4].minimum = synthetic_fixture::float(1.0),
            5 => item.damage[4].maximum = synthetic_fixture::float(1.0),
            _ => item.damage[4].school += 1,
        }
    }
    fn rejects_changes(profile: &Profile, input: &init::Input, display: bool) {
        validate_item_overlaps(profile, input).unwrap();
        for index in 0..if display { 8 } else { 7 } {
            let mut invalid = input.clone();
            let init::Rows::Complete(items) = &mut invalid.item_templates else {
                unreachable!()
            };
            if index == 7 {
                items[0].display_id += 1;
            } else {
                change(&mut items[0], index);
            }
            assert_eq!(
                validate_item_overlaps(profile, &invalid),
                Err(NativeError::AuthorityBinding)
            );
        }
    }
    #[test]
    fn equipped_starter_overlap_binds_shared_raw_columns_without_quest_loot() {
        let profile = synthetic_fixture::initialized_profile();
        let starter = synthetic_fixture::initialized_starter_input();
        let weapon = &profile.combat.as_ref().unwrap().equipment_weapons[0];
        let old = starter
            .items
            .iter()
            .find(|item| item.id == weapon.item_id)
            .unwrap();
        let mut input = synthetic_creature_model_fixture::input();
        input.item_templates = init::Rows::Complete(vec![raw(old, 90001)]);
        rejects_changes(&profile, &input, false);
    }
    #[test]
    fn quest_loot_inventory_and_starter_overlaps_bind_only_shared_columns() {
        let (mut profile, starter, quest) = synthetic_loot_fixture::setup();
        profile
            .initialize_quest_progression(&starter, quest)
            .unwrap();
        let loot = profile
            .quest_progression
            .as_ref()
            .unwrap()
            .loot
            .as_ref()
            .unwrap();
        let old = &starter.items[0];
        let display = loot.inventory.item(old.id).unwrap().display_id;
        let mut input = synthetic_creature_model_fixture::input();
        input.item_templates = init::Rows::Complete(vec![raw(old, display)]);
        rejects_changes(&profile, &input, true);
        let init::Rows::Complete(items) = &mut input.item_templates else {
            unreachable!()
        };
        items[0].entry = 90009;
        items[0].display_id = 90009;
        validate_item_overlaps(&profile, &input).unwrap();
    }
    #[test]
    fn retained_ungranted_starter_rows_bind_without_loot_or_equipment_membership() {
        let (mut profile, mut starter, quest) = synthetic_quest_fixture::setup();
        assert!(quest.loot.is_none());
        let mut ungranted = starter.items[0].clone();
        ungranted.id = 900009;
        starter.items.push(ungranted.clone());
        profile
            .initialize_player(&starter, synthetic_fixture::float(100.0), 1)
            .unwrap();
        profile
            .initialize_quest_progression(&starter, quest)
            .unwrap();
        assert!(profile
            .combat
            .as_ref()
            .unwrap()
            .equipment_weapons
            .iter()
            .all(|weapon| weapon.item_id != ungranted.id));
        let mut input = synthetic_creature_model_fixture::input();
        input.item_templates = init::Rows::Complete(vec![raw(&ungranted, 90009)]);
        rejects_changes(&profile, &input, false);
    }
}
