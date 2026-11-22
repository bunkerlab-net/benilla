// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;
use crate::native::source_execution::{
    player::SourcePlayer,
    unit_vitals::{live_player, UNIT_STAND_STATE_STAND},
};

const UNIT_DYNAMIC_FLAGS: usize = 143; // UpdateFields.h:131.
const PLAYER_CONTROLLED: u32 = 8; // Unit.h:472.
const STUNNED: u32 = 0x0004_0000; // Unit.h:484.
const MULTITAP: u32 = 0x0080_0000; // CreatureDefines.h:128.
const RAGE: u8 = 1; // SharedDefines.h:152.
const DAMAGE: u32 = 0x0000_0002; // SpellDefines.h:363.
const NON_PERIODIC_DAMAGE: u32 = 0x0100_0000; // SpellDefines.h:386.

/// Borrow the live stores for one damage call. No Unit borrow crosses an owner call.
pub struct SourceLiveDamage<'a> {
    pub objects: &'a SourceObjects,
    pub units: &'a mut SourceUnits,
    pub spells: &'a SourceSpellStores,
    pub players: &'a [&'a SourcePlayer],
}

impl SourceLiveDamage<'_> {
    fn rage_user(&self, unit: ObjectHandle, missing_owner: &'static str) -> Result<bool> {
        if self.objects.get(unit)?.kind != SourceObjectKind::Player {
            return Ok(false);
        }
        let power = self
            .units
            .get(self.objects, unit)?
            .power_type
            .ok_or_else(|| refuse(missing_owner))?;
        Ok(power == RAGE)
    }

    fn prelude(&mut self, damage: &SourceDamage) -> Result<()> {
        let victim = self.objects.get(damage.victim)?;
        if let Some(dealer) = damage.dealer {
            require(
                self.objects.get(dealer)?.map == victim.map,
                "execution.combat.Unit_902.cross_map_damage_owner",
            )?;
        }
        // Unit.cpp:905-910. Standing up is independent of the landed amount.
        if !matches!(
            damage.kind,
            SourceDamageType::Dot | SourceDamageType::Instakill
        ) {
            let fields = self.units.get_mut(self.objects, damage.victim)?;
            if !fields.is_standing()
                && (victim.kind == SourceObjectKind::Player || fields.flags & STUNNED == 0)
            {
                fields.set_stand_state(
                    self.objects,
                    damage.victim,
                    UNIT_STAND_STATE_STAND,
                    false,
                )?;
            }
        }
        // Unit.cpp:912-917. Rage done precedes aura interruption.
        if let (Some(dealer), Some(clean)) = (damage.dealer, damage.clean_damage) {
            if damage.kind == SourceDamageType::Direct
                && dealer != damage.victim
                && self.rage_user(dealer, "execution.combat.Unit_915.GetPowerType_owner")?
                && clean.attack != SourceDamageAttack::Ranged
            {
                return Err(refuse("execution.combat.Unit_916.RewardRage_owner"));
            }
        }
        if let Some(clean) = damage.clean_damage {
            self.damage_interrupts(damage, clean)?;
            // Unit.cpp:957-964. Absorbed physical damage can grant rage at zero.
            if damage.amount == 0
                && clean.damage != 0
                && damage.school & 1 != 0
                && self.rage_user(
                    damage.victim,
                    "execution.combat.Unit_960.GetPowerType_owner",
                )?
                && !matches!(
                    clean.outcome,
                    SourceDamageOutcome::Dodge | SourceDamageOutcome::Parry
                )
            {
                return Err(refuse("execution.combat.Unit_962.RewardRage_owner"));
            }
        }
        Ok(())
    }

    // Unit.cpp:919-955. Finish the selection walk before the first removal call.
    fn damage_interrupts(&self, damage: &SourceDamage, clean: SourceCleanDamage) -> Result<()> {
        if !clean.taken_or_absorbed
            || matches!(
                damage.kind,
                SourceDamageType::Instakill | SourceDamageType::NoDamage
            )
        {
            return Ok(());
        }
        let flags = if damage.kind == SourceDamageType::Dot {
            DAMAGE
        } else {
            DAMAGE | NON_PERIODIC_DAMAGE
        };
        let victim = self.units.get(self.objects, damage.victim)?;
        let mut remove = false;
        for (id, holder) in victim.auras.aura_holders() {
            if damage.spell.is_some_and(|spell| spell.id == id.spell()) {
                continue;
            }
            let spell = self
                .spells
                .template(id.spell())
                .ok_or_else(|| refuse("execution.combat.Unit_935.GetSpellProto_owner"))?;
            if damage
                .travel_started_ms
                .is_some_and(|start| start <= holder.apply_ms)
            {
                continue;
            }
            remove |= spell.aurainterruptflags & flags != 0;
        }
        require(
            !remove,
            "execution.combat.Unit_950.RemoveAurasDueToSpell_owner",
        )?;
        if let Some(id) = victim.spell_slots.current(SpellSlot::Channel) {
            let spell = victim
                .spell_starts
                .spells
                .get(&id)
                .and_then(|spell| self.spells.template(spell.spell))
                .ok_or_else(|| refuse("execution.combat.Unit_953.channel_spell_owner"))?;
            require(
                spell.channelinterruptflags & flags == 0,
                "execution.combat.Unit_954.InterruptSpell_owner",
            )?;
        }
        Ok(())
    }

    fn nonzero_prelude(&mut self, damage: &SourceDamage) -> Result<()> {
        let Some(dealer) = damage.dealer else {
            return Ok(());
        };
        let victim_kind = self.objects.get(damage.victim)?.kind;
        // Unit.cpp:972-984. A live null duel pointer is the ordinary solo branch.
        if self.units.get(self.objects, dealer)?.flags & PLAYER_CONTROLLED != 0
            && victim_kind == SourceObjectKind::Player
        {
            let player = live_player(
                self.players,
                damage.victim,
                "execution.combat.Unit_976.duel_owner",
            )?;
            if player.duel_opponent.is_some()
                && damage.amount >= self.combat_health(damage.victim)?.wrapping_sub(1)
            {
                return Err(refuse("execution.combat.Unit_980.duel_opponent_owner"));
            }
        }
        // Unit.cpp:986-1000. Pets can read a recipient but cannot acquire this tap.
        if !matches!(
            victim_kind,
            SourceObjectKind::Creature | SourceObjectKind::Pet
        ) {
            return Ok(());
        }
        let fields = self.units.get(self.objects, damage.victim)?;
        if victim_kind != SourceObjectKind::Pet
            && fields.loot_recipient_guid.raw() == 0
            && fields.loot_group_recipient_id == 0
        {
            self.set_loot_recipient(damage.victim, dealer)?;
        }
        let fields = self.units.get(self.objects, damage.victim)?;
        require(
            fields.loot_group_recipient_id == 0,
            "execution.combat.Creature_1112.GetGroupById_owner",
        )?;
        let guid = fields.loot_recipient_guid;
        if guid.raw() != 0 {
            // Creature.cpp:1101-1132; ObjectAccessor.cpp:102-111.
            let player = self
                .objects
                .find_player(guid)?
                .ok_or_else(|| refuse("execution.combat.Creature_1103.FindPlayer_owner"))?;
            if !self.in_recipient_group(dealer, player)? {
                let fields = self.units.get_mut(self.objects, damage.victim)?;
                fields.damage_by_others = fields.damage_by_others.wrapping_add(damage.amount);
            }
        }
        Ok(())
    }

    // Creature.cpp:1151-1179. Unit.cpp:6482-6494 resolves the beneficiary through GetMaster.
    fn set_loot_recipient(&mut self, victim: ObjectHandle, dealer: ObjectHandle) -> Result<()> {
        if self.units.get(self.objects, victim)?.static_flags[2] & MULTITAP != 0 {
            return Ok(());
        }
        let beneficiary = self
            .units
            .get(self.objects, dealer)?
            .control_guids
            .get_master(self.objects, dealer, None, unbound_pet_loading)?
            .unit()
            .unwrap_or(dealer);
        let object = self.objects.get(beneficiary)?;
        if object.kind != SourceObjectKind::Player {
            return Ok(());
        }
        let guid = object
            .guid
            .filter(|guid| guid.is_player())
            .ok_or_else(|| refuse("execution.combat.Creature_1173.GetObjectGuid_owner"))?;
        self.units
            .get_mut(self.objects, victim)?
            .loot_recipient_guid = guid;
        let player = live_player(
            self.players,
            beneficiary,
            "execution.combat.Creature_1176.GetGroup_owner",
        )?;
        require(
            player.group.is_none(),
            "execution.combat.Creature_1176.loot_group_owner",
        )?;
        self.objects.force_update_field(victim, UNIT_DYNAMIC_FLAGS)
    }

    // Relations.cpp:714-739,908-937. Same-unit membership does not read a group.
    fn in_recipient_group(&self, dealer: ObjectHandle, recipient: ObjectHandle) -> Result<bool> {
        if dealer == recipient {
            return Ok(true);
        }
        let player = |unit, missing| {
            let object = self.objects.get(unit)?;
            let fields = self.units.get(self.objects, unit)?;
            require(
                object.kind == SourceObjectKind::Player && !fields.control_guids.has_charmer(),
                missing,
            )?;
            live_player(self.players, unit, missing)
        };
        let dealer_controlled =
            self.units.get(self.objects, dealer)?.flags & PLAYER_CONTROLLED != 0;
        let recipient_controlled =
            self.units.get(self.objects, recipient)?.flags & PLAYER_CONTROLLED != 0;
        if !dealer_controlled
            && !recipient_controlled
            && self.objects.get(dealer)?.kind != SourceObjectKind::Player
        {
            return Err(refuse("execution.combat.Relations_916.IsFriend_owner"));
        }
        if dealer_controlled && recipient_controlled {
            let dealer = player(
                dealer,
                "execution.combat.Relations_728.GetControllingPlayer_owner",
            )?;
            player(
                recipient,
                "execution.combat.Relations_730.GetControllingPlayer_owner",
            )?;
            require(
                dealer.group.is_none(),
                "execution.combat.Relations_733.group_owner",
            )?;
        }
        Ok(false)
    }

    fn suffix(&self, damage: &SourceDamage) -> Result<()> {
        // Unit.cpp:1223-1247. Neither roll depends on the Kill durability argument.
        if let Some(dealer) = damage.dealer {
            if self.objects.get(dealer)?.kind == SourceObjectKind::Player {
                return Err(refuse("execution.combat.Unit_1226.durability_roll_owner"));
            }
        }
        if self.objects.get(damage.victim)?.kind == SourceObjectKind::Player {
            if damage.dealer != Some(damage.victim)
                && self.rage_user(
                    damage.victim,
                    "execution.combat.Unit_1237.GetPowerType_owner",
                )?
                && damage.clean_damage.is_some_and(|clean| {
                    clean.damage != 0
                        && !matches!(
                            clean.outcome,
                            SourceDamageOutcome::Dodge | SourceDamageOutcome::Parry
                        )
                })
            {
                return Err(refuse("execution.combat.Unit_1239.RewardRage_owner"));
            }
            return Err(refuse("execution.combat.Unit_1242.durability_roll_owner"));
        }
        if damage.dealer.is_some()
            && !matches!(
                damage.kind,
                SourceDamageType::NoDamage | SourceDamageType::Shield | SourceDamageType::Split
            )
        {
            return Err(refuse(
                "execution.combat.Unit_1250.InterruptOrDelaySpell_owner",
            ));
        }
        Ok(())
    }
}

impl ThreatExecutionOwner for SourceLiveDamage<'_> {
    fn can_attack(&mut self, _source: ObjectHandle, _target: ObjectHandle) -> Result<bool> {
        Err(refuse("execution.combat.Unit_1211.CanAttack_owner"))
    }
}

impl CombatExecutionOwner for SourceLiveDamage<'_> {
    fn combat_unit(&mut self, unit: ObjectHandle) -> Result<CombatUnit> {
        let health = self.combat_health(unit)?;
        let preventing_death = self.preventing_death(unit)?;
        let fields = self.units.get(self.objects, unit)?;
        Ok(CombatUnit {
            health,
            alive: self.objects.get(unit)?.alive,
            taxi: fields.state & 0x80 != 0,
            evade: fields.in_evade(),
            has_ai: fields.has_ai,
            unkillable: fields.static_flags[0] & 8 != 0,
            preventing_death,
        })
    }

    // Unit.cpp:969. Read the same health field that ModifyHealth writes.
    fn combat_health(&mut self, unit: ObjectHandle) -> Result<u32> {
        self.units
            .get(self.objects, unit)?
            .vitals
            .as_ref()
            .map(|vitals| vitals.health)
            .ok_or_else(|| refuse("execution.combat.Unit_969.GetHealth_owner"))
    }

    // Unit.cpp:1006; Player.cpp:20132-20135; Creature.cpp:3006-3009.
    fn preventing_death(&mut self, unit: ObjectHandle) -> Result<bool> {
        if self.objects.get(unit)?.kind == SourceObjectKind::Player {
            let player = live_player(
                self.players,
                unit,
                "execution.combat.Unit_1006.IsPreventingDeath_owner",
            )?;
            return Ok(player.extra_flags & 0x0400 != 0);
        }
        Ok(self.units.get(self.objects, unit)?.static_flags[0] & 8 != 0)
    }

    fn combat_call(&mut self, call: CombatCall, damage: &mut SourceDamage) -> Result<()> {
        match call {
            CombatCall::DealDamagePrelude => self.prelude(damage),
            CombatCall::DealDamageNonzeroPrelude => self.nonzero_prelude(damage),
            CombatCall::DamageDealtSuffix => self.suffix(damage),
            CombatCall::Kill => Err(refuse("execution.combat.Unit_1010.Kill_owner")),
            CombatCall::DamageDeal => Err(refuse("execution.combat.Unit_881.DamageDeal_owner")),
            CombatCall::DamageTaken => Err(refuse("execution.combat.Unit_885.DamageTaken_owner")),
            CombatCall::JustPreventedDeath => Err(refuse(
                "execution.combat.CreatureAI_111.JustPreventedDeath_owner",
            )),
        }
    }

    fn modify_health(&mut self, unit: ObjectHandle, delta: i32) -> Result<()> {
        self.units.get_mut(self.objects, unit)?.modify_health(
            self.objects,
            unit,
            delta,
            self.players,
        )?;
        Ok(())
    }
}
