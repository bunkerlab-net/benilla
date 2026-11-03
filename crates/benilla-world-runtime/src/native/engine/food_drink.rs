// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Player.cpp:7463-7502; Spell.cpp:4314-4375;
// SpellAuras.cpp:4953-4990,5290-5292; Unit.cpp:10176-10191.
use super::*;
use crate::native::food_drink as owner;
impl<A: Authority> Frame<'_, A> {
    pub(super) fn set_standing(
        &mut self,
        standing: bool,
        acknowledge: bool,
    ) -> Result<Option<Rejection>, NativeError> {
        if !owner::selected(self.profile) {
            return Err(NativeError::UnsupportedMechanic("player stand-state owner"));
        }
        let combat = self
            .state
            .combat
            .as_mut()
            .ok_or(NativeError::InvalidState)?;
        if combat.standing == standing {
            return Ok(None);
        }
        combat.standing = standing;
        if standing {
            let food = self
                .state
                .food_drink
                .as_mut()
                .ok_or(NativeError::InvalidState)?;
            for id in [430, 433] {
                if food.holders.remove(&id).is_some() {
                    self.events
                        .push(Event::FoodDrink(FoodDrinkEvent::Removed { spell: id }));
                }
            }
            self.update_drink_regen()?;
        }
        self.events.push(Event::FoodDrink(FoodDrinkEvent::Stand {
            standing,
            acknowledge,
        }));
        Ok(None)
    }
    fn update_drink_regen(&mut self) -> Result<(), NativeError> {
        let combat = self
            .state
            .combat
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let drink = self
            .state
            .food_drink
            .as_ref()
            .is_some_and(|f| f.holders.contains_key(&430));
        let regen = if drink {
            mana::update_mana_regen(
                u32::try_from(combat.stats.attributes[4]).map_err(|_| NativeError::InvalidState)?,
                mana::ManaRegenModifiers {
                    spirit_multiplier: spell::SpellFloat::ONE,
                    flat_mana_per_five: i32::try_from(owner::amount(self.profile, 430)?)
                        .map_err(|_| NativeError::InvalidState)?,
                    interrupted_percent: 0,
                },
            )?
        } else {
            combat.stats.regen()
        };
        self.state.resources.set_regen(regen)?;
        Ok(())
    }
    pub(super) fn use_consumable(
        &mut self,
        slot: u8,
        spell_index: u8,
    ) -> Result<Option<Rejection>, NativeError> {
        if self
            .state
            .quest_progression
            .as_ref()
            .and_then(|state| state.inventory.as_ref())
            .and_then(|inventory| inventory.slots.get(&slot))
            .is_some_and(|item| item.item == 6948)
        {
            return Err(NativeError::UnsupportedMechanic(
                "Hearthstone item use requires spell 8690 teleport and home-bind lifecycle",
            ));
        }
        if !owner::selected(self.profile) {
            return Err(NativeError::UnsupportedMechanic("food/drink item use"));
        }
        let genesis = self
            .profile
            .quest_progression
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let catalog = &genesis
            .loot
            .as_ref()
            .ok_or(NativeError::InvalidState)?
            .inventory;
        let current = self
            .state
            .quest_progression
            .clone()
            .ok_or(NativeError::InvalidState)?;
        let inventory = current
            .inventory
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let Some(item) = inventory.slots.get(&slot) else {
            return Ok(Some(Rejection::Inventory {
                reason: 23,
                source: slot,
                destination: slot,
            }));
        };
        let proto = catalog.item(item.item)?;
        if spell_index != 0 || !matches!((item.item, proto.spells[0].id), (159, 430) | (2070, 433))
        {
            return Err(NativeError::UnsupportedMechanic(
                "starting consumable selected item spell",
            ));
        }
        let id = proto.spells[0].id;
        let cooldown = owner::resources(self.profile, id)?;
        let combat = self
            .state
            .combat
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let moving = self.moved
            || self
                .state
                .player_movement_flags
                .is_some_and(|flags| flags.is_moving());
        let failure = if combat.in_combat {
            Some(0x00)
        } else if moving {
            Some(0x2e)
        } else {
            None
        };
        let alive = self.state.player_alive();
        let cast_id = self.state.next_cast_id;
        self.state.next_cast_id = cast_id.checked_add(1).ok_or(NativeError::Limit)?;
        self.state
            .player_events
            .prepare(cast_id, self.state.now_ms)?;
        let result = self.state.resources.caster.cast_instant_aura(
            cooldown,
            self.state.now_ms,
            spell::CastChecks {
                caster_alive: alive,
                target_exists: true,
                target_alive: alive,
                position_changed: false,
                other_failure: failure,
            },
        )?;
        self.state.player_events.finish(
            cast_id,
            self.state.now_ms,
            if result.is_err() {
                FinishedSpellReason::PrepareFailed
            } else {
                FinishedSpellReason::UpdateFinished
            },
        )?;
        if let Err(failure) = result {
            self.events.push(Event::FoodDrink(FoodDrinkEvent::Cast {
                spell: id,
                cast_id,
                failure: Some(failure),
            }));
            return Ok(Some(Rejection::CastFailed(failure)));
        }
        let receipt = inventory.plan_take_cast_item(catalog, slot)?;
        let (next, events) = current.destroy_selected(genesis, combat, receipt)?;
        // SetCurrentCastedSpell interrupts the previous non-travelling generic
        // spell before the new cast publishes SpellGo (Unit.cpp:4225-4226).
        let old = self
            .state
            .casts
            .iter()
            .filter_map(|(&id, cast)| {
                matches!(cast.execution.state(), spell::CastState::Casting { .. }).then_some(id)
            })
            .collect::<Vec<_>>();
        for id in old {
            self.interrupt_scheduled_cast(id, spell::CastFailure::Interrupted)?;
        }
        self.events.push(Event::FoodDrink(FoodDrinkEvent::Cast {
            spell: id,
            cast_id,
            failure: None,
        }));
        self.commit_quest_progression(next)?;
        self.events
            .extend(events.into_iter().map(Event::QuestProgression));
        let aura = owner::aura(self.profile, id)?;
        let duration_ms = aura.remaining_ms();
        let frost = self.state.frost_armor().is_some();
        let level = self.state.level(self.profile);
        let state = self
            .state
            .food_drink
            .as_mut()
            .ok_or(NativeError::InvalidState)?;
        if state.holders.remove(&id).is_some() {
            self.events
                .push(Event::FoodDrink(FoodDrinkEvent::Removed { spell: id }));
        }
        let aura_slot = (0..3)
            .find(|slot| {
                !(*slot == 0 && frost) && state.holders.values().all(|holder| holder.slot != *slot)
            })
            .ok_or(NativeError::InvalidState)?;
        state.holders.insert(
            id,
            ConsumableHolder {
                spell: id,
                slot: aura_slot,
                level,
                cast_id,
                applied_tick: self.state.tick,
                aura,
            },
        );
        self.events.push(Event::FoodDrink(FoodDrinkEvent::Applied {
            spell: id,
            slot: aura_slot,
            duration_ms,
        }));
        self.set_standing(false, false)?;
        self.update_drink_regen()?;
        Ok(None)
    }
    pub(super) fn player_food_drink_update(&mut self) -> Result<(), NativeError> {
        if self.state.food_drink.is_none() {
            return Ok(());
        }
        for id in [430, 433] {
            let food = self
                .state
                .food_drink
                .as_mut()
                .ok_or(NativeError::InvalidState)?;
            let Some(holder) = food.holders.get_mut(&id) else {
                continue;
            };
            if holder.applied_tick == self.state.tick {
                continue;
            }
            let update = holder.aura.update(self.profile.tick_ms)?;
            if update.periodic && self.state.health > 0 {
                if id == 433 {
                    let before = self.state.health;
                    let maximum = self
                        .state
                        .combat
                        .as_ref()
                        .ok_or(NativeError::InvalidState)?
                        .stats
                        .max_health;
                    self.state.health = before
                        .checked_add(owner::amount(self.profile, 433)?)
                        .ok_or(NativeError::Limit)?
                        .min(maximum);
                    self.events.push(Event::FoodDrink(FoodDrinkEvent::Health {
                        before,
                        after: self.state.health,
                    }));
                    let references =
                        interactions::healing_assist_references(self.profile, self.state)?;
                    if !references.is_empty() {
                        let threat = mana::healing_assist_threat(
                            self.state.health - before,
                            references.len() as u32,
                        )?;
                        for npc in references {
                            self.receipt(
                                npc,
                                ai::CombatEvent::Threat {
                                    target: SOLO_UNIT,
                                    amount: ai::Real::from_bits(threat.bits())?,
                                    kind: ai::ThreatKind::HealingAssist,
                                },
                            )?;
                        }
                    }
                } else {
                    self.events.push(Event::FoodDrink(FoodDrinkEvent::EatEmote));
                }
            }
            if update.expired || self.state.health == 0 {
                self.state
                    .food_drink
                    .as_mut()
                    .ok_or(NativeError::InvalidState)?
                    .holders
                    .remove(&id);
                self.events
                    .push(Event::FoodDrink(FoodDrinkEvent::Removed { spell: id }));
            }
        }
        let food = self
            .state
            .food_drink
            .as_mut()
            .ok_or(NativeError::InvalidState)?;
        food.heartbeat_remaining_ms = food
            .heartbeat_remaining_ms
            .saturating_sub(self.profile.tick_ms);
        if food.heartbeat_remaining_ms == 0 {
            food.heartbeat_remaining_ms = 5000;
            if food.holders.contains_key(&433) {
                self.events
                    .push(Event::FoodDrink(FoodDrinkEvent::PlaySpellVisual {
                        kit: 406,
                    }));
            }
        }
        self.update_drink_regen()?;
        Ok(())
    }
}
