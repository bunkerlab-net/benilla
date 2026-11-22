// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Spell.cpp:3442-3453; Unit.cpp:2659-2661;
// UnitAuraProcHandler.cpp:343-406,514-605; SpellAuras.cpp:395-407.
use super::*;
use crate::native::frost_armor as owner;
impl<A: Authority> Frame<'_, A> {
    pub(super) fn cast_frost_armor(
        &mut self,
        ability: u32,
    ) -> Result<Option<Rejection>, NativeError> {
        let Some(known) = self.state.ability(self.profile, ability) else {
            return Ok(Some(Rejection::UnknownAbility));
        };
        if known.admission != AbilityAdmission::FrostArmor {
            return Ok(Some(match &known.admission {
                AbilityAdmission::Unavailable { reason } => Rejection::UnavailableAbility {
                    ability,
                    reason: reason.clone(),
                },
                _ => Rejection::UnknownTarget,
            }));
        }
        if !self.state.player_alive() {
            return Ok(Some(Rejection::Dead));
        }
        if self
            .state
            .casts
            .values()
            .any(|c| matches!(c.execution.state(), spell::CastState::Casting { .. }))
        {
            return Ok(Some(Rejection::AlreadyCasting));
        }
        let raw = owner::source(self.profile, 168)?;
        if !self
            .state
            .combat
            .as_ref()
            .ok_or(NativeError::InvalidState)?
            .standing
        {
            return Ok(Some(Rejection::CastFailed(spell::CastFailure::Rejected(
                0x3e,
            ))));
        }
        let cast_id = self.state.next_cast_id;
        self.state.next_cast_id = cast_id.checked_add(1).ok_or(NativeError::Limit)?;
        // Spell.cpp:3007-3016 creates an event even when preparation fails.
        // The existing Unit callback deletes this finished spell in this frame.
        self.state
            .player_events
            .prepare(cast_id, self.state.now_ms)?;
        let result = self.state.resources.caster.cast_instant_aura(
            owner::resources(raw),
            self.state.now_ms,
            spell::CastChecks {
                caster_alive: true,
                target_exists: true,
                target_alive: true,
                position_changed: false,
                other_failure: None,
            },
        )?;
        if let Err(failure) = result {
            self.state.player_events.finish(
                cast_id,
                self.state.now_ms,
                FinishedSpellReason::PrepareFailed,
            )?;
            self.events.push(Event::FrostArmor(FrostArmorEvent::Cast {
                cast_id,
                failure: Some(failure),
                mana_spent: 0,
            }));
            return Ok(Some(Rejection::CastFailed(failure)));
        }
        self.state.player_events.finish(
            cast_id,
            self.state.now_ms,
            FinishedSpellReason::UpdateFinished,
        )?;
        if interactions::spell_authority::resets_melee_timers(self.profile, ability)? {
            let combat = self
                .state
                .combat
                .as_mut()
                .ok_or(NativeError::InvalidState)?;
            let genesis = self
                .profile
                .combat
                .as_ref()
                .ok_or(NativeError::InvalidState)?;
            let intervals = if self.profile.player_melee.is_some() {
                combat.reset_selected_melee_timer(genesis)?
            } else {
                combat.reset_melee_timers(genesis)?
            };
            let witness = melee_reset::MeleeResetWitness::capture_frost(
                self.profile,
                self.state,
                cast_id,
                intervals,
            )?;
            self.state
                .combat
                .as_mut()
                .ok_or(NativeError::InvalidState)?
                .melee_reset = Some(witness);
            player_melee::record_instant_spell_reset(self.profile, self.state)?;
        }
        self.events.push(Event::FrostArmor(FrostArmorEvent::Cast {
            cast_id,
            failure: None,
            mana_spent: raw.words[32],
        }));
        if self.state.frost_armor.take().is_some() {
            self.events
                .push(Event::FrostArmor(FrostArmorEvent::Removed));
        }
        self.state.frost_armor = Some(FrostArmorHolder {
            aura: spell::TimedAura::new(
                owner::spec(raw, self.state.level(self.profile))?,
                owner::duration(raw)?,
            )?,
            applied_tick: self.state.tick,
            last_update_tick: self.state.tick - 1,
            cast_id,
            level: self.state.level(self.profile),
        });
        self.events
            .push(Event::FrostArmor(FrostArmorEvent::Applied));
        Ok(None)
    }
    pub(super) fn player_frost_armor_update(&mut self) -> Result<(), NativeError> {
        if let Some(holder) = &mut self.state.frost_armor {
            let expired = holder.aura.update(self.profile.tick_ms)?.expired;
            holder.last_update_tick = self.state.tick;
            if expired || !self.state.player_alive() {
                self.state.frost_armor = None;
                self.events
                    .push(Event::FrostArmor(FrostArmorEvent::Removed));
            }
        }
        Ok(())
    }
    pub(super) fn npc_chilled_update(&mut self, id: NpcId) -> Result<(), NativeError> {
        let npc = self
            .state
            .npcs
            .get_mut(&id.spawn)
            .filter(|n| n.id == id)
            .ok_or(NativeError::UnknownTarget)?;
        if let Some(holder) = &mut npc.chilled {
            if holder.applied_tick == self.state.tick {
                return Ok(());
            }
            let expired = holder.aura.update(self.profile.tick_ms)?.expired;
            holder.last_update_tick = self.state.tick;
            if expired || npc.health == 0 {
                npc.chilled = None;
                npc.ai
                    .apply_aura_profile(npc.effective_profile(self.profile)?.ai)?;
                if let Some(source) = &mut npc.source_motion {
                    source.motion.speed_changed();
                }
                self.events
                    .push(Event::FrostArmor(FrostArmorEvent::NpcSpeeds {
                        target: id,
                        speeds: npc.chilled_speeds(self.profile)?,
                    }));
                self.events
                    .push(Event::FrostArmor(FrostArmorEvent::ChilledRemoved {
                        target: id,
                    }));
            }
        }
        Ok(())
    }
    pub(super) fn frost_armor_proc(
        &mut self,
        id: NpcId,
        action: u64,
        target_level: u8,
        outcome: benilla_sim::npc_melee::MeleeOutcome,
    ) -> Result<Option<ChilledHolder>, NativeError> {
        use benilla_sim::npc_melee::MeleeOutcome as O;
        if !self.state.player_alive()
            || !self.state.combat.as_ref().is_some_and(|c| c.standing)
            || !matches!(outcome, O::Normal | O::Critical | O::Crushing)
        {
            return Ok(None);
        }
        let Some(armor) = self.state.frost_armor.clone() else {
            return Ok(None);
        };
        let raw = owner::source(self.profile, 6136)?;
        let caster_level = self.state.level(self.profile);
        let miss = spell::zero_resistance_binary_npc_miss(
            caster_level,
            target_level,
            spell::SpellFloat::from_bits(0x4080_0000)?,
        )?;
        let roll = self.state.draw(
            0,
            DrawPurpose::FrostArmorHit { action, npc: id },
            0,
            9999,
            &mut self.draws,
        )?;
        let missed = roll < i32::from(miss);
        self.events
            .push(Event::FrostArmor(FrostArmorEvent::Chilled {
                target: id,
                action,
                missed,
            }));
        if missed {
            return Ok(None);
        }
        Ok(Some(ChilledHolder {
            slot: 32,
            aura: spell::TimedAura::new(owner::spec(raw, caster_level)?, owner::duration(raw)?)?,
            applied_tick: self.state.tick,
            last_update_tick: self.state.tick,
            action,
            target: id,
            caster_level,
            target_level,
            armor,
            draw: self.draws.last().ok_or(NativeError::InvalidState)?.clone(),
        }))
    }
    pub(super) fn sync_chilled(
        &mut self,
        npc: &mut Npc,
        holder: Option<ChilledHolder>,
    ) -> Result<(), NativeError> {
        if let Some(mut holder) = holder {
            self.clear_chilled(npc)?;
            holder.slot = if npc.auras.contains_key(&133) && npc.fireball_aura_slot == 32 {
                33
            } else {
                32
            };
            npc.chilled = Some(holder);
            npc.ai
                .apply_aura_profile(npc.effective_profile(self.profile)?.ai)?;
            if let Some(source) = &mut npc.source_motion {
                source.motion.speed_changed();
            }
            self.events
                .push(Event::FrostArmor(FrostArmorEvent::NpcSpeeds {
                    target: npc.id,
                    speeds: npc.chilled_speeds(self.profile)?,
                }));
        }
        Ok(())
    }
    pub(super) fn clear_chilled(&mut self, npc: &mut Npc) -> Result<(), NativeError> {
        if npc.chilled.take().is_some() {
            npc.ai
                .apply_aura_profile(npc.effective_profile(self.profile)?.ai)?;
            if let Some(source) = &mut npc.source_motion {
                source.motion.speed_changed();
            }
            self.events
                .push(Event::FrostArmor(FrostArmorEvent::NpcSpeeds {
                    target: npc.id,
                    speeds: npc.chilled_speeds(self.profile)?,
                }));
            self.events
                .push(Event::FrostArmor(FrostArmorEvent::ChilledRemoved {
                    target: npc.id,
                }));
        }
        Ok(())
    }
}
