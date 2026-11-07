// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, Creature.cpp:506-513/2747-2778; GPL-2.0-or-later.
//! Source-owned virtual items are unit fields, not player-owned Item objects.
use super::*;
use benilla_sim::creature_init as init;

pub(super) fn validate(
    input: &init::Input,
    selected: &init::InitializedCreature,
    catalog: &impl ClientCatalog,
) -> Result<(), Error> {
    let Some(virtual_item) = selected.equipment.mainhand else {
        return Ok(());
    };
    let init::Rows::Complete(items) = &input.item_templates else {
        return Err(Error::Unrepresentable);
    };
    let raw = items
        .iter()
        .find(|row| row.entry == virtual_item.item_entry)
        .ok_or(Error::Unrepresentable)?;
    let info = catalog
        .item_metadata(raw.entry)
        .ok_or(Error::MissingItemMetadata(raw.entry))?;
    // Compare raw shared columns, including signed Material's GetUInt32 wire
    // representation. The packed virtual field uses only that value's low byte.
    let damage = raw
        .damage
        .iter()
        .filter(|line| f32::from_bits(line.maximum.bits()) > 0.0)
        .collect::<Vec<_>>();
    if info.display_info_id != raw.display_id
        || info.class != raw.class
        || info.subclass != raw.subclass
        || info.material != raw.material as u32
        || info.inventory_type != raw.inventory_type
        || info.sheath != raw.sheath
        || info.delay_ms != raw.delay_ms
        || info.dmg_min.to_bits() != raw.damage[0].minimum.bits()
        || info.dmg_max.to_bits() != raw.damage[0].maximum.bits()
        || info.dmg_type != raw.damage[0].school
        || info.damages.len() != damage.len()
        || info.damages.iter().zip(damage).any(|(actual, raw)| {
            actual.min.to_bits() != raw.minimum.bits()
                || actual.max.to_bits() != raw.maximum.bits()
                || actual.school != raw.school
        })
    {
        return Err(Error::InvalidItemMetadata(raw.entry));
    }
    if !catalog.contains_item_display(raw.display_id) {
        return Err(Error::UnboundItemDisplay(raw.entry));
    }
    Ok(())
}

pub(super) fn fields(selected: &init::InitializedCreature) -> [(u16, u32); 10] {
    let (display, info) = selected
        .equipment
        .mainhand
        .map_or((0, [0; 2]), |item| (item.display_id, item.info));
    // LoadEquipment writes all slots; closed offhand/ranged slots stay explicit
    // zero. UpdateEntry sets melee sheath and UNIT_BYTE2_FLAG_AURAS; the selected
    // GUID/template addon then replaces only the sheath byte on each life.
    [
        (37, display),
        (38, 0),
        (39, 0),
        (40, info[0]),
        (41, info[1]),
        (42, 0),
        (43, 0),
        (44, 0),
        (45, 0),
        (164, selected.unit_bytes_2),
    ]
}
