// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Spell.cpp:6404-6801. Item identities are not inventory or equipment state.
use super::*;
use SpellCastResult as R;

impl SourceSpell {
    /// `player` is the true caster's type, already read by CheckCast.
    pub(in super::super) fn check_items(
        &mut self,
        t: &SpellTemplate,
        player: bool,
        owner: &mut (impl SpellExecutionOwner + ?Sized),
    ) -> Result<R> {
        // Spell.cpp:6438-6445. Creatures and gameobjects skip every item check.
        if !player {
            return Ok(R::Ok);
        }
        if let Some(target) = self.targets.unit {
            let _ = owner.unit(target)?.player;
        }
        // :6450 is the first live cast-item read, before inventory or charges.
        if self.item_cast {
            return Err(refuse("execution.spells.f5_25c_Item_IsInTrade_owner"));
        }
        // :6519-6543. A GUID and a resolved item pointer are distinct operands.
        if self.targets.item_guid != 0 {
            if self.targets.item.is_none() {
                return Ok(
                    if self.flags.old_triggered() && self.targets.mask & 0x1000 == 0 {
                        R::DontReport
                    } else {
                        R::ItemGone
                    },
                );
            }
            return Err(refuse(
                "execution.spells.f5_25a_Item_IsFitToSpellRequirements_GetProto_owner",
            ));
        }
        // Player.cpp:18645-18699. Negative classes skip even hand requirements.
        if t.equippeditemclass >= 0 {
            if t.attributesex3 & 0x400 != 0 {
                return Err(refuse(
                    "execution.spells.f5_25b_Player_hasMainhandWeaponForAttack_owner",
                ));
            }
            if t.attributesex3 & 0x01000000 != 0 {
                return Err(refuse(
                    "execution.spells.f5_25b_Player_hasOffhandWeaponForAttack_owner",
                ));
            }
            if matches!(t.equippeditemclass, 2 | 4) {
                return Err(refuse(
                    "execution.spells.f5_25b_Player_GetItemByPos_equipment_owner",
                ));
            }
            // Unsupported positive classes fail without scanning equipment.
            return Ok(if self.flags.old_triggered() {
                R::DontReport
            } else {
                R::EquippedItemClass
            });
        }
        // Spell.cpp:6546-6556. This focus search precedes reagents and is repeated
        // by CheckCast after CheckItems, including passive/non-player spells.
        if t.requiresspellfocus != 0 {
            return Err(SpellCheck::Focus.missing_owner());
        }
        if !self.ignore_item_requirements(t)? {
            // Player::CanNoReagentCast is unconditionally false, :18702-18709.
            for reagent in [
                t.reagent1, t.reagent2, t.reagent3, t.reagent4, t.reagent5, t.reagent6, t.reagent7,
                t.reagent8,
            ] {
                if reagent <= 0 {
                    continue;
                }
                // No CastItem survives :6450 above. Even a zero reagent count
                // enters HasItemCount's live inventory scan, Player.cpp:8462-8464.
                return Err(refuse(
                    "execution.spells.f5_25a_Player_HasItemCount_reagent_owner",
                ));
            }
            for totem in [t.totem1, t.totem2] {
                if totem != 0 {
                    return Err(refuse(
                        "execution.spells.f5_25a_Player_HasItemCount_totem_owner",
                    ));
                }
            }
        }
        // Spell.cpp:6638-6798. Preserve effect-slot order and the targeting skip.
        for (effect, item) in [
            (t.effect1, t.effectitemtype1),
            (t.effect2, t.effectitemtype2),
            (t.effect3, t.effectitemtype3),
        ] {
            match effect {
                24 if self.state != SourceSpellState::Targeting
                    && !self.flags.old_triggered()
                    && item != 0 =>
                {
                    return Err(refuse(
                        "execution.spells.f5_25a_ObjectMgr_GetItemPrototype_owner",
                    ));
                }
                53 | 54 => {
                    if self.targets.item.is_none() {
                        return Ok(R::ItemGone);
                    }
                    return Err(refuse(if effect == 53 {
                        "execution.spells.f5_25a_Item_GetProto_enchant_owner"
                    } else {
                        "execution.spells.f5_25a_Item_GetOwner_temporary_enchant_owner"
                    }));
                }
                99 => {
                    if self.targets.item.is_none() {
                        return Ok(R::CantBeDisenchanted);
                    }
                    return Err(refuse(
                        "execution.spells.f5_25a_Item_GetOwnerGuid_disenchant_owner",
                    ));
                }
                17 | 58 if self.attack_type == 2 => {
                    return Err(refuse(
                        "execution.spells.f5_25b_Player_GetWeaponForAttack_ranged_owner",
                    ));
                }
                _ => {}
            }
        }
        Ok(R::Ok)
    }

    // Spell.cpp:6404-6433, without the unconfigured ENABLE_PLAYERBOTS branch.
    fn ignore_item_requirements(&self, t: &SpellTemplate) -> Result<bool> {
        if self.flags.channel_only() || self.flags.ignore_costs() {
            return Ok(true);
        }
        if self.flags.old_triggered() || t.id == 46546 {
            if self.targets.item.is_some() {
                return Err(refuse(
                    "execution.spells.f5_25a_Item_GetOwnerGuid_ignore_requirements_owner",
                ));
            }
            return Ok(self.triggered_by_spell_reagent != Some(0));
        }
        Ok(false)
    }
}
