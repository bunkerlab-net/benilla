// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;
use crate::native::source_execution::{
    player::SourcePlayer,
    unit_vitals::{live_player, UNIT_STAND_STATE_STAND},
};
use std::collections::BTreeMap;

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
    pub ais: &'a SourceCreatureAis,
    pub event_ais: &'a mut BTreeMap<ObjectHandle, SourceEventAi>,
    pub factions: &'a CreatureAiFactions,
}

impl SourceLiveDamage<'_> {
    fn unit_queries(&mut self) -> SourceUnitQueries<'_> {
        SourceUnitQueries {
            objects: self.objects,
            units: self.units,
        }
    }

    fn relations(&self) -> SourceRelations<'_> {
        SourceRelations {
            objects: self.objects,
            units: self.units,
            factions: self.factions,
        }
    }

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

impl SourceRelationOwner for SourceLiveDamage<'_> {
    fn relation_kind(&self, unit: ObjectHandle) -> Result<SourceObjectKind> {
        Ok(self.objects.get(unit)?.kind)
    }

    fn relation_player_flags(&self, player: ObjectHandle, read: &'static str) -> Result<u32> {
        Ok(live_player(self.players, player, read)?.player_flags())
    }

    fn relation_flags(&self, unit: ObjectHandle) -> Result<u32> {
        self.relations().relation_flags(unit)
    }

    fn relation_master(&self, unit: ObjectHandle) -> Result<Option<ObjectHandle>> {
        self.relations().relation_master(unit)
    }

    fn relation_faction(&self, unit: ObjectHandle) -> Result<Option<&SourceFactionTemplate>> {
        Ok(self
            .factions
            .relation_template(self.units.get(self.objects, unit)?.faction))
    }

    fn relation_persuaded(&self, unit: ObjectHandle) -> Result<Option<ObjectHandle>> {
        self.relations().relation_persuaded(unit)
    }

    fn relation_has_reputation(&self, faction: u32) -> Result<bool> {
        self.relations().relation_has_reputation(faction)
    }

    fn relation_faction_player(&self, unit: ObjectHandle) -> Result<Option<ObjectHandle>> {
        // Relations.cpp:72-90. An empty master GUID makes a player their own controller.
        require(
            self.objects.get(unit)?.kind == SourceObjectKind::Player
                && self
                    .units
                    .get(self.objects, unit)?
                    .control_guids
                    .master_guid()
                    .raw()
                    == 0,
            "execution.relations.GetFactionReaction_166_GetControllingPlayer_owner",
        )?;
        Ok(Some(unit))
    }

    fn relation_forced_rank(&self, player: ObjectHandle, _faction: u32) -> Result<Option<u32>> {
        let read = "execution.relations.Relations_172.GetForcedRankIfAny_owner";
        // ReputationMgr.cpp:128-129. The forced-reaction writer has not been ported.
        require(
            live_player(self.players, player, read)?
                .forced_reactions
                .is_empty(),
            read,
        )?;
        Ok(None)
    }
}

struct CombatEntry<'a> {
    spells: &'a SourceSpellStores,
}

impl UnitAiExecutionOwner for CombatEntry<'_> {
    fn call(
        &mut self,
        unit: ObjectHandle,
        call: UnitAiCall,
        units: &mut SourceUnits,
        objects: &SourceObjects,
    ) -> Result<()> {
        match call {
            UnitAiCall::PropagateCombat(_) => {
                // Unit.cpp:7930-7932. A missing controller does not propagate.
                if units
                    .get(objects, unit)?
                    .control_guids
                    .get_master(objects, unit, None, unbound_pet_loading)?
                    .unit()
                    .is_some()
                {
                    return Err(refuse(
                        "execution.combat.Unit_7932.CanJoinInAttacking_owner",
                    ));
                }
                Ok(())
            }
            UnitAiCall::InterruptNonCombatCasts => {
                units.interrupt_non_combat_spells(objects, self.spells, unit)
            }
            UnitAiCall::CreatureCombatStart(_) => {
                Err(refuse("execution.combat.Unit_7973.GetCreatureAddon_owner"))
            }
            _ => Err(call.missing_owner()),
        }
    }
}

impl ThreatExecutionOwner for SourceLiveDamage<'_> {
    fn can_attack(&mut self, source: ObjectHandle, target: ObjectHandle) -> Result<bool> {
        unit_can_attack(source, target, self)
    }

    fn can_enter_combat(&mut self, unit: ObjectHandle) -> Result<bool> {
        self.unit_queries().can_enter_combat(unit)
    }

    fn threat_unit(&mut self, unit: ObjectHandle) -> Result<ThreatUnit> {
        self.unit_queries().threat_unit(unit)
    }

    fn threat_master(&mut self, unit: ObjectHandle) -> Result<SourceMasterLookup> {
        self.unit_queries().threat_master(unit)
    }

    fn threat_lookup(&mut self, source: ObjectHandle, guid: u64) -> Result<Option<ObjectHandle>> {
        self.unit_queries().threat_lookup(source, guid)
    }

    fn combat_timer(&mut self, source: ObjectHandle, target: ObjectHandle) -> Result<()> {
        self.unit_queries().combat_timer(source, target)
    }

    fn spell_threat_mod(
        &mut self,
        unit: ObjectHandle,
        spell: u32,
        value: ThreatValue,
    ) -> Result<ThreatValue> {
        // ThreatManager.cpp:46-49; Object.h:1156, Player.h:990, Pet.h:187, Totem.h:46.
        let kind = self.objects.get(unit)?.kind;
        if kind != SourceObjectKind::Player {
            require(
                kind != SourceObjectKind::Pet && !self.units.get(self.objects, unit)?.totem,
                "execution.threat.ThreatManager_48.GetSpellModOwner_owner",
            )?;
            return Ok(value);
        }
        let player = live_player(
            self.players,
            unit,
            "execution.threat.ThreatManager_48.GetSpellModOwner_owner",
        )?;
        let template = self
            .spells
            .template(spell)
            .ok_or_else(|| refuse("execution.threat.ThreatManager_49.spell_template_owner"))?;
        // SpellDefines.h:398, SPELLMOD_THREAT.
        let amount =
            player.apply_spell_mod(self.spells, template, 2, f32::from_bits(value.bits()))?;
        Ok(ThreatValue::from_bits(amount.to_bits()))
    }

    fn critical_threat_mod(&mut self, unit: ObjectHandle, school: u32) -> Result<ThreatValue> {
        // ThreatManager.cpp:51-52; Unit.cpp:4558-4572; SpellAuraDefines.h:318.
        let auras = &self.units.get(self.objects, unit)?.auras;
        let mut multiplier = ThreatValue::from_u32(1);
        if school == 0 {
            return Ok(multiplier);
        }
        let hundred = ThreatValue::from_u32(100);
        for effect in auras.auras_of_type(183) {
            let aura = auras
                .aura_holder(effect.holder)
                .and_then(|holder| holder.effects.get(usize::from(effect.effect)))
                .and_then(Option::as_ref)
                .ok_or_else(|| refuse("execution.threat.Unit_4568.GetModifier_owner"))?;
            if aura.misc as u32 & school != 0 {
                let factor = hundred
                    .add_value(ThreatValue::from_i32(aura.amount))
                    .divide(hundred);
                multiplier = multiplier.multiply(factor);
            }
        }
        Ok(multiplier)
    }

    fn total_threat_mod(
        &mut self,
        unit: ObjectHandle,
        school: u32,
        value: ThreatValue,
    ) -> Result<ThreatValue> {
        // Unit.cpp:8638-8649; SpellAuraDefines.h:81.
        let fields = self.units.get(self.objects, unit)?;
        if fields.auras.auras_of_type(10).is_empty() || school == 0 {
            return Ok(value);
        }
        // SharedDefines.h:238-244 falls back to the physical school if no bit matches.
        let index = (school & 0x7f).trailing_zeros();
        let modifier = fields.threat_modifier[if index < 7 { index as usize } else { 0 }];
        Ok(value.multiply(ThreatValue::from_bits(modifier.to_bits())))
    }

    fn aggro_linking(
        &mut self,
        _threats: &mut SourceThreats,
        _source: ObjectHandle,
        _target: ObjectHandle,
    ) -> Result<()> {
        Err(refuse(
            "execution.threat.ThreatManager_483.TriggerAggroLinkingEvent_owner",
        ))
    }

    fn offline_target(&mut self, source: ObjectHandle, target: ObjectHandle) -> Result<bool> {
        // Unit.cpp:8944-8949, Creature.cpp:2962-2965, Unit.h:2298,2300.
        let kind = self.objects.get(source)?.kind;
        let ignores_feign = matches!(kind, SourceObjectKind::Creature | SourceObjectKind::Pet)
            && self.units.get(self.objects, source)?.static_flags[1] & 0x100 != 0;
        Ok(!ignores_feign && self.units.get(self.objects, target)?.state & 4 != 0)
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

    fn can_apply_damage_mods(&mut self, unit: ObjectHandle) -> Result<bool> {
        // Unit.cpp:867. A dead victim returns before any other Unit field read.
        if !self.objects.get(unit)?.alive {
            return Ok(false);
        }
        let fields = self.units.get(self.objects, unit)?;
        Ok(fields.state & 0x80 == 0 && !fields.in_evade())
    }

    fn has_damage_ai(&mut self, unit: ObjectHandle) -> Result<bool> {
        // Unit.cpp:880,884 only checks the AI pointer.
        Ok(self.units.get(self.objects, unit)?.has_ai)
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
            CombatCall::DamageDeal => self.ais.require_base_damage_ai(
                self.objects,
                damage
                    .dealer
                    .ok_or_else(|| refuse("execution.combat.Unit_881.DamageDeal_owner"))?,
                "execution.combat.Unit_881.DamageDeal_owner",
            ),
            CombatCall::DamageTaken => Err(refuse("execution.combat.Unit_885.DamageTaken_owner")),
            CombatCall::JustPreventedDeath => {
                self.ais.require_base_damage_ai(
                    self.objects,
                    damage.victim,
                    "execution.combat.CreatureAI_111.JustPreventedDeath_owner",
                )?;
                if let Some(event_ai) = self.event_ais.get_mut(&damage.victim) {
                    event_ai.just_prevented_death(self.objects)?;
                }
                Ok(())
            }
        }
    }

    fn damage_taken(&mut self, combat: &mut SourceCombat, damage: &mut SourceDamage) -> Result<()> {
        self.ais.require_base_damage_ai(
            self.objects,
            damage.victim,
            "execution.combat.Unit_885.DamageTaken_owner",
        )?;
        combat.creature_damage_taken(damage, self)?;
        if let Some(event_ai) = self.event_ais.get_mut(&damage.victim) {
            let vitals = self
                .units
                .get(self.objects, damage.victim)?
                .vitals
                .as_ref()
                .ok_or_else(|| refuse("execution.event_ai.CreatureEventAI_1855.GetHealth_owner"))?;
            event_ai.damage_taken(self.objects, vitals, damage.amount)?;
        }
        Ok(())
    }

    fn damage_threat_multiplier(&mut self, spell: Option<ThreatSpell>) -> Result<ThreatValue> {
        // Unit.cpp:1213; SpellMgr.h:2088-2096.
        Ok(ThreatValue::from_bits(
            self.spells.threat_multiplier(spell.map(|s| s.id)).0,
        ))
    }

    fn set_in_combat(
        &mut self,
        unit: ObjectHandle,
        enemy: ObjectHandle,
        _threats: &mut SourceThreats,
    ) -> Result<()> {
        self.units.set_in_combat_with(
            unit,
            enemy,
            self.objects,
            &mut CombatEntry {
                spells: self.spells,
            },
        )
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
