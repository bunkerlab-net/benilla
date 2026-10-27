// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Unit::UpdateMeleeAttackingState/DealMeleeDamage.
use super::*;
use benilla_sim::player_melee as m;

impl<A: Authority> Frame<'_, A> {
    pub(super) fn player_mainhand_update(&mut self) -> Result<(), NativeError> {
        if self.profile.player_melee.is_none() {
            return Ok(());
        }
        self.sync_quest_progression()?;
        let (pending, events) = player_melee::prepare_tick(self.profile, self.state)?;
        self.events.extend(events);
        let Some(pending) = pending else {
            return Ok(());
        };
        let target = pending.target();
        let action = pending.action();
        let first_draw = self.draws.len();
        let plan = m::prepare(pending.input())?;
        let (low, high) = plan.damage_range();
        let damage = self.state.draw_range(
            0,
            DrawPurpose::PlayerMeleeDamage {
                action,
                npc: target,
            },
            i64::from(low),
            i64::from(high),
            &mut self.draws,
        )? as u32;
        let outcome = self.state.draw(
            0,
            DrawPurpose::PlayerMeleeOutcome {
                action,
                npc: target,
            },
            1,
            10000,
            &mut self.draws,
        )? as u16;
        let glance = plan
            .glance_range(outcome)?
            .map(|(low, high)| {
                self.state
                    .draw_range(
                        0,
                        DrawPurpose::PlayerMeleeGlance {
                            action,
                            npc: target,
                        },
                        i64::from(low),
                        i64::from(high),
                        &mut self.draws,
                    )
                    .map(|n| n as u32)
            })
            .transpose()?;
        let result = m::resolve(
            plan,
            m::MeleeRolls {
                damage,
                outcome,
                glance,
            },
        )?;
        let numeric_draws = self.draws[first_draw..].to_vec();
        let effective = result.total_damage().min(pending.target_health());
        let killed = effective != 0 && effective == pending.target_health();
        let (hit_info, victim_state) = match result.outcome() {
            m::MeleeOutcome::Miss => (0x12, 0),
            m::MeleeOutcome::Dodge => (2, 2),
            m::MeleeOutcome::Parry => (2, 3),
            m::MeleeOutcome::Block => (0x802, if result.total_damage() == 0 { 5 } else { 1 }),
            m::MeleeOutcome::Glancing => (0x4002, 1),
            m::MeleeOutcome::Critical => (0x82, 1),
            m::MeleeOutcome::Normal => (2, 1),
        };
        // SendAttackStateUpdate precedes DealMeleeDamage and all death callbacks.
        self.events.push(Event::PlayerMelee {
            target,
            action,
            effective,
            outcome: MeleeOutcome {
                damage: result.total_damage(),
                hit_info,
                victim_state,
                blocked: result.blocked_amount(),
                absorb: 0,
                resist: 0,
                school: 0,
                pushback: false,
            },
        });
        if result.outcome() == m::MeleeOutcome::Critical {
            self.events.push(Event::PlayerMeleeCriticalWound { target });
        }
        if result.outcome() == m::MeleeOutcome::Parry {
            let raw = &self
                .profile
                .creature_generations
                .as_ref()
                .ok_or(NativeError::InvalidProfile)?
                .source(target.spawn)
                .ok_or(NativeError::InvalidProfile)?
                .input
                .template;
            if raw.extra_flags & 0x8 == 0 {
                self.state
                    .npcs
                    .get_mut(&target.spawn)
                    .ok_or(NativeError::InvalidState)?
                    .ai
                    .apply_player_mainhand_parry_haste()?;
            }
        }
        let mut loot = None;
        if effective != 0 {
            let npc = self
                .state
                .npcs
                .get_mut(&target.spawn)
                .filter(|npc| npc.id == target)
                .ok_or(NativeError::InvalidState)?;
            if npc.original_loot_recipient.is_none() {
                npc.original_loot_recipient = Some(self.state.player);
            }
            npc.health = npc
                .health
                .checked_sub(effective)
                .ok_or(NativeError::InvalidState)?;
            if killed {
                self.native_kill_rewards(target)?;
                self.prepare_body_loot(target)?;
                loot = self.state.npcs[&target.spawn].corpse_loot.clone();
                self.source_combat_receipt(target, ai::CombatEvent::Died)?;
                // The target's CombatStop removes its attackers before the
                // attacker later executes the unconditional BASE reset.
                let timers = &mut self
                    .state
                    .combat
                    .as_mut()
                    .ok_or(NativeError::InvalidState)?
                    .melee_timers;
                timers.write_source_fields(None, timers.remaining_ms())?;
                self.events.push(Event::PlayerAttackCancel);
                self.events.push(Event::PlayerAttackStop { target });
            } else {
                if pending.victim_can_attack() && self.player_melee_can_enter(target)? {
                    self.source_combat_receipt(
                        target,
                        ai::CombatEvent::Threat {
                            target: SOLO_UNIT,
                            amount: ai::Real::new(effective as f32)?,
                            kind: ai::ThreatKind::DirectDamage,
                        },
                    )?;
                    self.enter_player_combat();
                }
                self.player_hit_durability(
                    DrawPurpose::PlayerDurabilityChance {
                        action,
                        npc: target,
                    },
                    DrawPurpose::PlayerDurabilitySlot {
                        action,
                        npc: target,
                    },
                )?;
            }
        }
        // Unit::AttackedBy itself returns for dead targets; CanEnterCombat
        // remains a separate predicate and has no implied alive check.
        if self.player_melee_can_enter(target)? {
            if !killed {
                self.source_combat_receipt(
                    target,
                    ai::CombatEvent::AttackedBy { target: SOLO_UNIT },
                )?;
            }
            self.enter_player_combat();
        }
        self.sync_quest_progression()?;
        let after_damage = MeleeExecutionBinding::capture(self.profile, self.state)?;
        let operands = after_damage.current_operands(self.profile)?;
        let critter = self.state.npcs[&target.spawn]
            .effective_profile(self.profile)?
            .combat
            .as_ref()
            .ok_or(NativeError::InvalidProfile)?
            .creature_is_critter;
        if !critter {
            let gain = self
                .profile
                .player_melee
                .as_ref()
                .ok_or(NativeError::InvalidProfile)?
                .weapon_skill_gain();
            let skill = m::WeaponSkill {
                value: operands.pure_skill,
                maximum: operands.maximum_skill,
                gain,
            };
            let roll = m::weapon_gain_chance(operands.level, operands.intellect, skill)?
                .map(|_| {
                    self.state
                        .draw_range(
                            0,
                            DrawPurpose::PlayerWeaponGain {
                                action,
                                npc: target,
                            },
                            0,
                            (1_i64 << 53) - 1,
                            &mut self.draws,
                        )
                        .and_then(|value| {
                            Ok(benilla_sim::npc_melee::PercentRoll::from_fraction53(
                                value as u64,
                            )?)
                        })
                })
                .transpose()?;
            let resolution =
                m::resolve_weapon_gain(operands.level, operands.intellect, skill, roll)?;
            if resolution.update_weapon_skill() {
                self.state
                    .combat
                    .as_mut()
                    .ok_or(NativeError::InvalidState)?
                    .skills
                    .get_mut(&operands.skill_id)
                    .ok_or(NativeError::InvalidState)?
                    .value = resolution.value();
                self.progression_weapon_skill_change(operands.skill_id, gain)?;
            }
        }
        player_melee::record_npc_reactive(
            self.profile,
            self.state,
            &pending,
            numeric_draws,
            &result,
        )?;
        let draws = self.draws[first_draw..].to_vec();
        self.events.extend(player_melee::finish_swing(
            self.profile,
            self.state,
            pending,
            draws,
            after_damage,
            loot,
        )?);
        Ok(())
    }
    fn player_melee_can_enter(&self, target: NpcId) -> Result<bool, NativeError> {
        use benilla_sim::classic_targets::{can_enter_combat, CombatEntryInput};
        let npc = self
            .state
            .npcs
            .get(&target.spawn)
            .filter(|npc| npc.id == target)
            .ok_or(NativeError::InvalidState)?;
        Ok(can_enter_combat(CombatEntryInput {
            can_enter_combat: self
                .profile
                .source_motion
                .as_ref()
                .ok_or(NativeError::InvalidProfile)?
                .player_can_enter_combat,
            evading_home: false,
        }) && can_enter_combat(CombatEntryInput {
            can_enter_combat: self.profile.npc_motion(target.spawn)?.npc_can_enter_combat,
            evading_home: npc.ai.phase() == ai::Phase::ReturningHome,
        }))
    }
}
