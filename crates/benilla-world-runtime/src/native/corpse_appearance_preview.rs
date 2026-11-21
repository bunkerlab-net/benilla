// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only current-owner appearance preview, never a CreateCorpse operation.
//! CMaNGOS Classic 8ec338a1 Player::CreateCorpse appearance assignments.
use super::*;
use benilla_sim::corpse_appearance as source;

/// An observation of one validated native checkpoint, not a corpse or capability.
/// No deserializer or public operand constructor can mint this observation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorpseAppearancePreview {
    identity: CheckpointIdentity,
    words: source::Snapshot,
}
impl CorpseAppearancePreview {
    pub fn identity(&self) -> &CheckpointIdentity {
        &self.identity
    }
    pub fn words(&self) -> &source::Snapshot {
        &self.words
    }
    /// Equality to current owned observations, not authentication of past chronology.
    pub fn validate_for(&self, world: &NativeWorld) -> Result<(), NativeError> {
        if self != &world.corpse_appearance_preview()? {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
}
impl NativeWorld {
    /// Preview the ordinary source appearance words from actual current owners.
    /// This neither runs ghost casts nor allocates, saves, registers or publishes
    /// a corpse. It is valid while alive too; death is not evidence of CreateCorpse.
    pub fn corpse_appearance_preview(&self) -> Result<CorpseAppearancePreview, NativeError> {
        player_hide_flags::validate_profile(&self.profile)?;
        let map =
            self.profile
                .map_classification
                .as_ref()
                .ok_or(NativeError::AuthorityUnavailable(
                    "corpse preview exact map classification",
                ))?;
        map.validate(&self.profile)?;
        if map.is_battleground() {
            return Err(NativeError::UnsupportedMechanic(
                "corpse preview battleground",
            ));
        }
        // Revalidate retained appearance, collision, hide, equipment custody and
        // source history before observing any words; this does not commit a save.
        let identity = self.checkpoint()?.identity();
        let appearance = self
            .state
            .player_appearance()
            .ok_or(NativeError::InvalidState)?;
        let request = self
            .profile
            .player_appearance
            .as_ref()
            .ok_or(NativeError::InvalidProfile)?
            .request();
        let choices = appearance.choices();
        let model =
            self.state
                .player_collision_height()
                .ok_or(NativeError::AuthorityUnavailable(
                    "corpse preview native display owner",
                ))?;
        let loot = self
            .profile
            .quest_progression
            .as_ref()
            .and_then(|q| q.loot.as_ref())
            .ok_or(NativeError::AuthorityUnavailable(
                "corpse preview equipment prototype owner",
            ))?;
        let combat = self
            .state
            .combat
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let mut equipment = [None; 19];
        // Source uses occupied m_items, including broken and hidden objects. A
        // genesis registry entry or an active modifier alone is not worn custody.
        for item in combat.equipment.items() {
            let handle = loot
                .equipment_instance(item.origin)
                .ok_or(NativeError::AuthorityBinding)?;
            if loot.equipment_origin(handle) != Some(item.origin) {
                return Err(NativeError::AuthorityBinding);
            }
            let row = loot
                .inventory
                .item(item.item_id)
                .map_err(NativeError::Inventory)?;
            let slot = equipment
                .get_mut(usize::from(item.slot))
                .ok_or(NativeError::InvalidState)?;
            if slot.is_some() {
                return Err(NativeError::InvalidState);
            }
            *slot = Some(source::EquippedItem {
                display_info_id: row.display_id,
                inventory_type: row.inventory_type,
            });
        }
        let flags = self.state.player_hide_flags();
        // Fresh Player::Create starts guild ID 0; BGData starts membership ID 0.
        // These remain derived facts ONLY in the current fresh script-free host:
        // no native guild/BG join, teleport/import, GM or scripted setter exists.
        // Map classification above is separate from Player::InBattleGround.
        let words = source::capture_ordinary(&source::OwnerAppearance {
            race: request.race,
            gender: request.gender,
            skin: choices.skin,
            face: choices.face,
            hair_style: choices.hair_style,
            hair_color: choices.hair_color,
            facial_hair: choices.facial_hair,
            native_display_id: model.witness().native_display_id(),
            guild_id: 0,
            hide_helm: flags & 0x400 != 0,
            hide_cloak: flags & 0x800 != 0,
            equipment,
        });
        Ok(CorpseAppearancePreview { identity, words })
    }
}
