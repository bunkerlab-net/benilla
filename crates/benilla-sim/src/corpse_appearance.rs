// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
//! The ordinary non-battleground CreateCorpse appearance assignments only.
//! Caller observations are not authenticated source evidence or corpse admission.

/// Item prototype operands for one occupied equipment slot, including broken or hidden items.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EquippedItem {
    pub display_info_id: u32,
    pub inventory_type: u32,
}

/// Complete caller-supplied observations at CreateCorpse's appearance assignment stage.
/// `None` means a *known empty* slot, never unavailable metadata. This type must not be
/// populated by defaulting unknown source data. Current transformed display is not an operand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnerAppearance {
    pub race: u8,
    pub gender: u8,
    pub skin: u8,
    pub face: u8,
    pub hair_style: u8,
    pub hair_color: u8,
    pub facial_hair: u8,
    pub native_display_id: u32,
    pub guild_id: u32,
    pub hide_helm: bool,
    pub hide_cloak: bool,
    pub equipment: [Option<EquippedItem>; 19],
}

/// Appearance words only: no identity, location, lifetime, world membership or authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub native_display_id: u32,
    pub equipment: [u32; 19],
    pub bytes_1: u32,
    pub bytes_2: u32,
    pub guild_id: u32,
    pub flags: u32,
}

/// Does not execute CreateCorpse, retire old corpses, save, register, or publish anything.
/// The host must separately prove ordinary non-BG scope and the complete observation inputs.
pub fn capture_ordinary(owner: &OwnerAppearance) -> Snapshot {
    Snapshot {
        native_display_id: owner.native_display_id,
        equipment: owner
            .equipment
            .map(|item| item.map_or(0, |item| item.display_info_id | (item.inventory_type << 24))),
        bytes_1: (u32::from(owner.race) << 8)
            | (u32::from(owner.gender) << 16)
            | (u32::from(owner.skin) << 24),
        bytes_2: u32::from(owner.face)
            | (u32::from(owner.hair_style) << 8)
            | (u32::from(owner.hair_color) << 16)
            | (u32::from(owner.facial_hair) << 24),
        guild_id: owner.guild_id,
        flags: 4 | if owner.hide_helm { 8 } else { 0 } | if owner.hide_cloak { 16 } else { 0 },
    }
}
