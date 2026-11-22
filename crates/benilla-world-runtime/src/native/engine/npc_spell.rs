// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 SpellStart/Prepare/AddUnitTarget/SpellHitResult.
// Copyright CMaNGOS contributors; upstream GPL-2.0-or-later. No warranty.
//! Private source entry; no owner command installs or casts a player slow aura.
#[cfg(any(test, feature = "synthetic-native-tests"))]
use super::*;

#[cfg(any(test, feature = "synthetic-native-tests"))]
impl<A: Authority> Frame<'_, A> {
    fn npc_spell_request(
        &self,
        caster: NpcId,
        cast_id: u64,
    ) -> Result<NpcSpellRequest, NativeError> {
        let npc = self
            .state
            .npcs
            .get(&caster.spawn)
            .filter(|n| n.id == caster)
            .ok_or(NativeError::UnknownTarget)?;
        Ok(NpcSpellRequest {
            witness: self.interaction_witness()?,
            caster,
            target: self.state.player,
            lifetime: npc
                .events
                .spells
                .get(&cast_id)
                .ok_or(NativeError::InvalidState)?
                .clone(),
            queue: npc.events.queue_snapshot(),
            caster_position: npc.ai.position(),
            target_position: self.state.position,
            caster_health: npc.health,
            target_health: self.state.health,
            caster_level: npc.effective_profile(self.profile)?.ai.level,
            target_level: self.state.level(self.profile),
        })
    }
    fn npc_spell_check(
        &mut self,
        caster: NpcId,
        cast_id: u64,
        check: NpcSpellCheck,
    ) -> Result<bool, NativeError> {
        let context = self.npc_spell_request(caster, cast_id)?;
        match self.ask(
            Some(caster),
            Query::NpcSpell(NpcSpellQuery::Check { context, check }),
        )? {
            Facts::NpcSpell(NpcSpellFacts::Check(ai::movement::Check::Passed)) => Ok(true),
            Facts::NpcSpell(NpcSpellFacts::Check(ai::movement::Check::Failed)) => Ok(false),
            Facts::NpcSpell(NpcSpellFacts::Check(ai::movement::Check::Unknown)) => Err(
                NativeError::AuthorityUnavailable("complete NPC spell predicate"),
            ),
            _ => Err(NativeError::AuthorityBinding),
        }
    }
    fn finish_npc_spell(
        &mut self,
        caster: NpcId,
        cast_id: u64,
        terminal: NpcSpellTerminal,
    ) -> Result<(), NativeError> {
        let applied_holder = if terminal
            == (NpcSpellTerminal::Hit {
                effect_applied: true,
            }) {
            let spell_id = self.state.npcs[&caster.spawn].events.spells[&cast_id].spell_id;
            Some(
                self.state
                    .player_slow
                    .as_ref()
                    .ok_or(NativeError::InvalidState)?
                    .holders()
                    .find(|h| h.spell_id == spell_id && h.caster == caster)
                    .ok_or(NativeError::InvalidState)?
                    .id,
            )
        } else {
            None
        };
        let lifetime = self
            .state
            .npcs
            .get_mut(&caster.spawn)
            .filter(|n| n.id == caster)
            .and_then(|n| n.events.spells.get_mut(&cast_id))
            .ok_or(NativeError::InvalidState)?;
        if lifetime.terminal.replace(terminal).is_some() {
            return Err(NativeError::InvalidState);
        }
        lifetime.applied_holder = applied_holder;
        self.events.push(Event::NpcSpell(NpcSpellEvent::Finished {
            lifetime: lifetime.clone(),
        }));
        Ok(())
    }
    /// Only the source's already-hostile, standing, ordinary player interaction
    /// is represented. New combat entry/assistance is rejected rather than invented.
    fn npc_spell_hostile_hit(&mut self, caster: NpcId, cast_id: u64) -> Result<(), NativeError> {
        if !self.npc_spell_check(caster, cast_id, NpcSpellCheck::CanAttack)? {
            if self.npc_spell_check(caster, cast_id, NpcSpellCheck::CanAssist)? {
                return Err(NativeError::UnsupportedMechanic(
                    "NPC triggered assisting spell combat",
                ));
            }
            return Ok(());
        }
        // Unit::CasterHitTargetWithSpell evaluates detection before CanEnterCombat.
        if !self.npc_spell_check(caster, cast_id, NpcSpellCheck::CasterDetectedByTarget)? {
            return Ok(());
        }
        let g = self
            .profile
            .source_motion
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let npc = self
            .state
            .npcs
            .get(&caster.spawn)
            .ok_or(NativeError::InvalidState)?;
        if !g.player_can_enter_combat
            || !self.profile.npc_motion(caster.spawn)?.npc_can_enter_combat
            || npc.ai.phase() == ai::Phase::ReturningHome
        {
            return Ok(());
        }
        if self.state.health == 0
            || self.state.combat.as_ref().is_none_or(|c| {
                !c.in_combat || !c.standing || c.has_channeled_spell || c.has_auto_repeat_spell
            })
            || !self.state.casts.is_empty()
            || npc.ai.phase() != ai::Phase::Combat
            || npc.ai.target() != Some(SOLO_UNIT)
        {
            return Err(NativeError::UnsupportedMechanic(
                "NPC triggered spell outside existing player combat",
            ));
        }
        // The same victim already has the source threat/combat reference. AddThreat(0)
        // and SetInCombatWith wrappers do not change it. TriggerCombatTimer(player)
        // is a no-op: the ordinary player cannot own a threat list (CombatManager:184).
        Ok(())
    }
    fn npc_spell_start(&mut self, trigger: &npc_spell::FixtureTrigger) -> Result<(), NativeError> {
        player_melee::refuse_spell_concurrency(self.profile, self.state)?;
        let caster = trigger.caster;
        let npc = self
            .state
            .npcs
            .get(&caster.spawn)
            .filter(|n| n.id == caster)
            .ok_or(NativeError::UnknownTarget)?;
        // This is a host admission bound, not an invented CheckCast CASTER_DEAD result.
        if self.state.health == 0
            || npc.health == 0
            || matches!(npc.ai.phase(), ai::Phase::Corpse | ai::Phase::Despawned)
        {
            return Err(NativeError::UnsupportedMechanic(
                "living NPC and player triggered spell host",
            ));
        }
        let genesis = self
            .profile
            .player_slow
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let recipe = genesis.triggered_recipe(
            trigger.spell_id,
            npc.effective_profile(self.profile)?.ai.level,
        )?;
        let lifetime = self
            .state
            .npcs
            .get_mut(&caster.spawn)
            .unwrap()
            .events
            .start_spell(
                self.state.player,
                trigger.spell_id,
                recipe.binding,
                self.state.tick,
                self.state.now_ms,
                trigger.after_npc_update,
            )?;
        let id = lifetime.cast_id;
        self.events
            .push(Event::NpcSpell(NpcSpellEvent::Started { lifetime }));
        let context = self.npc_spell_request(caster, id)?;
        match self.ask(
            Some(caster),
            Query::NpcSpell(NpcSpellQuery::Closure(context)),
        )? {
            Facts::NpcSpell(NpcSpellFacts::Closure(closure)) => closure.validate()?,
            _ => return Err(NativeError::AuthorityBinding),
        }
        // Strict CheckCast still runs under OLD_TRIGGERED. Target6 has no source
        // singleton range/LOS check here; its target enumerator is CHAIN.
        for check in [
            NpcSpellCheck::TargetNotTaxiFlying,
            NpcSpellCheck::TargetNotUntargetable,
            NpcSpellCheck::TargetNotSpiritOfRedemption,
        ] {
            if !self.npc_spell_check(caster, id, check)? {
                return self.finish_npc_spell(caster, id, NpcSpellTerminal::BadTargets);
            }
        }
        // Same-ID/same-caster rows have an identical deterministic amount in this
        // immutable-level host. Distinct IDs are stackable under the source catalog.
        // Cross-caster replacement remains excluded, including at PreCastCheck.
        if self
            .state
            .player_slow
            .as_ref()
            .unwrap()
            .holders()
            .any(|h| h.spell_id == trigger.spell_id && h.caster.spawn != caster.spawn)
        {
            return Err(NativeError::UnsupportedMechanic(
                "NPC slow cross-caster stacking check",
            ));
        }
        if !self.npc_spell_check(caster, id, NpcSpellCheck::CanAttackSpell)? {
            return self.finish_npc_spell(caster, id, NpcSpellTerminal::BadTargets);
        }
        if !self.npc_spell_check(caster, id, NpcSpellCheck::LocationAllowed)? {
            return self.finish_npc_spell(caster, id, NpcSpellTerminal::LocationDenied);
        }
        // Prepare -> cast(true), no current generic slot and no second CheckCast.
        // Closed source hooks exclude magnet redirection, script target mutation,
        // cast/hit procs and nested spells. CheckTarget may filter the sole target
        // without turning the successful spell into a failed cast.
        for check in [
            NpcSpellCheck::TargetGmVisible,
            NpcSpellCheck::TargetNotGm,
            NpcSpellCheck::FilterTargetNotUntargetable,
        ] {
            if !self.npc_spell_check(caster, id, check)? {
                return self.finish_npc_spell(caster, id, NpcSpellTerminal::TargetFiltered);
            }
        }
        let effect_applicable = self.npc_spell_check(caster, id, NpcSpellCheck::EffectNotImmune)?;
        // SpellHitResult still receives the original effect mask, even when
        // AddUnitTarget's separate effectHitMask was reduced to zero.
        let terminal = if self.state.health == 0 {
            NpcSpellTerminal::Hit {
                effect_applied: false,
            }
        } else if !self.npc_spell_check(caster, id, NpcSpellCheck::TargetNotEvading)? {
            NpcSpellTerminal::Evade
        } else if !self.npc_spell_check(caster, id, NpcSpellCheck::SpellNotImmune)? {
            NpcSpellTerminal::Immune
        } else {
            // IsReflectableSpell is false for source DmgClass NONE; that damage
            // class has no avoidance die. Neither IGNORE_HIT nor DO_NOT_PROC is set.
            NpcSpellTerminal::Hit {
                effect_applied: effect_applicable,
            }
        };
        if terminal != NpcSpellTerminal::Evade {
            self.npc_spell_hostile_hit(caster, id)?;
        }
        if matches!(
            terminal,
            NpcSpellTerminal::Hit {
                effect_applied: true
            }
        ) {
            self.apply_player_slow(caster, trigger.spell_id)?;
        }
        self.finish_npc_spell(caster, id, terminal)
    }
    pub(super) fn synthetic_npc_spell_starts(
        &mut self,
        after_npc_update: bool,
    ) -> Result<(), NativeError> {
        let triggers: Vec<_> = self
            .profile
            .player_slow
            .as_ref()
            .map(|g| {
                g.fixture_triggers
                    .iter()
                    .filter(|t| t.tick == self.state.tick && t.after_npc_update == after_npc_update)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        for trigger in triggers {
            self.npc_spell_start(&trigger)?;
        }
        Ok(())
    }
}
