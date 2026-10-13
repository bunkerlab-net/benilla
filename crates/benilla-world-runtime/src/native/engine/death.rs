// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_sim::{player_death as d, starter_init as init};
impl<A: Authority> Frame<'_, A> {
    pub(super) fn death_enabled(&self) -> bool {
        self.state
            .combat
            .as_ref()
            .is_some_and(|c| c.death.is_some())
    }
    fn death_spells(&self) -> (Vec<d::CurrentSpell>, Vec<u64>) {
        let mut current = Vec::new();
        let mut released = Vec::new();
        for (id, c) in &self.state.casts {
            match c.execution.state() {
                spell::CastState::Casting { .. } => current.push(d::CurrentSpell {
                    slot: d::CurrentSpellSlot::Generic,
                    cast_id: *id,
                    state: d::CurrentSpellState::Casting,
                    triggered: false,
                    allow_cast_while_casting: false,
                }),
                spell::CastState::Traveling { .. } => released.push(*id),
                _ => (),
            }
        }
        (current, released)
    }
    pub(super) fn player_death_update(&mut self) -> Result<(), NativeError> {
        if !self.death_enabled() {
            return Ok(());
        }
        let (current, _) = self.death_spells();
        let plan = self
            .state
            .combat
            .as_ref()
            .unwrap()
            .death
            .as_ref()
            .unwrap()
            .prepare_update(
                d::DeathUpdate {
                    tick: self.state.tick,
                    now_seconds: self.state.now_ms / 1000,
                    diff_ms: self.profile.tick_ms,
                    in_world: true,
                },
                &current,
            )?;
        self.death_actions(plan.actions)?;
        self.state.combat.as_mut().unwrap().death = Some(plan.next);
        Ok(())
    }
    pub(super) fn native_kill(
        &mut self,
        killer: NpcId,
        result: benilla_sim::npc_melee::MeleeResolution,
    ) -> Result<(), NativeError> {
        let g = self.profile.combat.as_ref().unwrap();
        let p = g.death.as_ref().ok_or(NativeError::UnsupportedMechanic(
            "Player::Kill map/config and callback closure",
        ))?;
        let mut holders = BTreeMap::new();
        for a in &g.initial_auras {
            holders.entry(a.spell_id).or_insert(d::DeathAura {
                holder_id: u64::from(a.spell_id),
                spell_id: a.spell_id,
                attributes: a.attributes,
                attributes_ex3: a.attributes_ex3,
                caster_aura_state: a.caster_aura_state,
            });
        }
        let holders = holders.into_values().collect::<Vec<_>>();
        let (current, released) = self.death_spells();
        let damage = result.total_damage();
        let mut checked_health = self.state.health;
        benilla_sim::npc_melee::apply_health(&mut checked_health, result)?;
        if checked_health != 0 {
            return Err(NativeError::InvalidState);
        }
        let plan = self
            .state
            .combat
            .as_ref()
            .unwrap()
            .death
            .as_ref()
            .ok_or(NativeError::InvalidState)?
            .prepare_kill(d::DeathInput {
                tick: self.state.tick,
                now_seconds: self.state.now_ms / 1000,
                player_id: SOLO_UNIT,
                class: 8,
                killer_npc_id: killer.spawn,
                health_before: self.state.health,
                lethal_damage: damage,
                mana_before: self.state.resources.caster.mana,
                durability_loss: true,
                killing_spell_attributes_ex3: None,
                support: &p.support,
                auras: &holders,
                current_spells: &current,
                released_flight_ids: &released,
            })?;
        // Full owner/character plus killer generation remain in state/action/journal binding.
        self.state.combat.as_mut().unwrap().death_killer = Some(killer);
        self.death_actions(plan.actions)?;
        self.state.combat.as_mut().unwrap().death = Some(plan.next);
        Ok(())
    }
    fn death_actions(&mut self, actions: Vec<d::DeathAction>) -> Result<(), NativeError> {
        for action in actions {
            self.events.push(Event::Death(action.clone()));
            match action {
                d::DeathAction::SetHealthZero => self.state.health = 0,
                d::DeathAction::NpcDeathDurabilityLoss => self.death_equipment_loss()?,
                d::DeathAction::ClearAllReactives => {
                    let c = self.state.combat.as_mut().unwrap();
                    c.defense_reactive_started_ms = None;
                    c.defense_reactive_ms = 0;
                }
                d::DeathAction::CombatStop => {
                    let c = self.state.combat.as_mut().unwrap();
                    if c.in_combat {
                        self.events.push(Event::Combat(CombatEvent::LeftCombat));
                    }
                    c.in_combat = false;
                    // Current melee NPC is temporarily owned by its AI callback and receives
                    // TargetDied on return. All other live AI states detach synchronously.
                    for n in self.state.npcs.values_mut() {
                        if n.ai.detach_dead_victim(SOLO_UNIT, self.state.now_ms)? {
                            self.events.push(Event::Npc {
                                id: n.id,
                                effect: ai::Effect::AttackStopped { target: SOLO_UNIT },
                            });
                        }
                    }
                    if let Some(id) = self.state.combat.as_ref().unwrap().death_killer {
                        if !self.state.npcs.contains_key(&id.spawn) && !self.events.iter().any(|e| matches!(e, Event::Npc { id: seen, effect: ai::Effect::AttackStopped { .. } } if *seen == id)) {
                            self.events.push(Event::Npc { id, effect: ai::Effect::AttackStopped { target: SOLO_UNIT } });
                        }
                    }
                }
                d::DeathAction::InterruptCurrentSpell { cast_id, .. } => {
                    let mut cast = self
                        .state
                        .casts
                        .remove(&cast_id)
                        .ok_or(NativeError::InvalidState)?;
                    let events = cast.execution.cancel(
                        &mut self.state.resources.caster,
                        spell::CastFailure::CasterDead,
                    );
                    self.spell_events(cast_id, cast.ability, cast.target, events)?;
                }
                d::DeathAction::RemoveAuraByDeath { .. } => {
                    return Err(NativeError::UnsupportedMechanic(
                        "active player aura removal",
                    ))
                }
                // All other mutable dependencies are absent in the checked bounded policy.
                // Body/root/timer/release facts are committed by the checked lifecycle and
                // projected explicitly; no ghost, corpse object or graveyard is fabricated.
                _ => (),
            }
        }
        Ok(())
    }
    fn death_equipment_loss(&mut self) -> Result<(), NativeError> {
        let mut c = self.state.combat.take().unwrap();
        let mut equipment = c.equipment.clone();
        let changes = equipment.apply_npc_death_loss::<NativeError, _>(|_, _, changes| {
            for transition in &changes.stat_transitions {
                let equipment::EquipmentStatTransition::Broken { slot, .. } = *transition;
                let result = c.apply_item_break(
                    slot,
                    init::Resources {
                        health: self.state.health,
                        max_health: c.stats.max_health,
                        mana: self.state.resources.caster.mana,
                        max_mana: c.stats.max_mana,
                    },
                )?;
                self.state.health = result.resources.health;
                self.state.resources.caster.mana = result.resources.mana;
                self.state.resources = mana::MageManaState::from_state(
                    self.state.resources.caster.clone(),
                    result.resources.max_mana,
                    result.mana_regeneration,
                    self.state.resources.regen_elapsed_ms(),
                    self.state.resources.last_mana_use_timer_ms(),
                )?;
                self.events.push(Event::Combat(CombatEvent::ItemStats {
                    slot,
                    stats: c.stats.clone(),
                    resources: result
                        .modifier_steps
                        .iter()
                        .map(|r| (r.health, r.max_health, r.mana, r.max_mana))
                        .collect(),
                }));
            }
            Ok(())
        })?;
        c.equipment = equipment;
        self.state.combat = Some(c);
        self.events
            .push(Event::Combat(CombatEvent::Durability(changes)));
        Ok(())
    }
}
impl<A: Authority> Frame<'_, A> {
    pub(super) fn prepare_npc_receipts(
        &mut self,
    ) -> Result<BTreeMap<u64, (ai::ReceiptStep, Option<ai::MotionCommand>)>, NativeError> {
        let mut prepared = BTreeMap::new();
        if !self.death_enabled() {
            return Ok(prepared);
        }
        let ids = self.state.npcs.keys().copied().collect::<Vec<_>>();
        for id in ids {
            let mut npc = self.state.npcs.remove(&id).unwrap();
            let prior_motion = npc.ai.motion().cloned();
            let mut receipts = self.receipts.remove(&id).unwrap_or_default();
            if npc.health == 0
                && !matches!(npc.ai.phase(), ai::Phase::Corpse | ai::Phase::Despawned)
            {
                receipts.push(ai::Receipt {
                    id: npc
                        .ai
                        .last_receipt_id()
                        .checked_add(receipts.len() as u64 + 1)
                        .ok_or(NativeError::Limit)?,
                    event: ai::CombatEvent::Died,
                });
                for ability in npc.auras.keys() {
                    self.events.push(Event::Spell {
                        cast_id: 0,
                        ability: *ability,
                        target: npc.id,
                        event: spell::SpellEvent::AuraRemoved,
                    });
                }
                npc.auras.clear();
            }
            // Only receipt-bearing NPCs require event-time relationship predicates.
            let target = if receipts
                .iter()
                .any(|r| !matches!(r.event, ai::CombatEvent::Died))
            {
                let Facts::Npc(f) = self.ask(
                    Some(npc.id),
                    Query::Npc {
                        position: npc.ai.position(),
                        player_position: self.state.position,
                        motion: prior_motion.clone(),
                    },
                )?
                else {
                    return Err(NativeError::AuthorityBinding);
                };
                Some(ai::Target {
                    id: SOLO_UNIT,
                    position: self.state.position,
                    level: self.profile.player.level,
                    combat_reach: self.profile.player.combat_reach,
                    bounding_radius: self.profile.player.bounding_radius,
                    moving_and_running: self.moved,
                    alive: self.state.health > 0,
                    online: true,
                    can_attack_in_combat: f.can_attack_in_combat,
                    can_attack_on_sight: f.can_attack_on_sight,
                    visible: f.visible,
                    accessible: f.accessible,
                    line_of_sight: f.line_of_sight,
                })
            } else {
                None
            };
            let observation = ai::Observation {
                sequence: self.state.tick,
                elapsed_ms: self.profile.tick_ms,
                geometry_revision: Revision(self.profile.geometry_revision),
                target,
                can_initiate_attack: false,
                casting_non_melee: false,
                stationary_melee_refresh_eligible: false,
                creature_moving_and_running: false,
                movement: None,
                receipts,
            };
            let token = npc.ai.begin_receipts(&observation)?;
            for effect in token.effects() {
                self.events.push(Event::Npc {
                    id: npc.id,
                    effect: effect.clone(),
                });
                if matches!(effect, ai::Effect::CombatStarted { .. }) {
                    self.enter_player_combat();
                }
            }
            prepared.insert(id, (token, prior_motion));
            self.state.npcs.insert(id, npc);
        }
        Ok(prepared)
    }
}
