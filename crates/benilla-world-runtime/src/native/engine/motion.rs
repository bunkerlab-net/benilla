// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_sim::creature_motion as m;

struct Spatial<'a, 'b, A: Authority> {
    frame: &'a mut Frame<'b, A>,
    npc: NpcId,
    failure: Option<NativeError>,
}
impl<A: Authority> m::Authority for Spatial<'_, '_, A> {
    fn resolve(&mut self, query: &m::Query) -> Result<m::Reply, m::MotionError> {
        match self.frame.ask(
            Some(self.npc),
            Query::SourceMotion(MotionQuery::Spatial(query.clone())),
        ) {
            Ok(Facts::SourceMotion(MotionFacts::Spatial(reply))) => Ok(*reply),
            Ok(_) => Err(m::MotionError::Invalid("native motion reply kind")),
            Err(error) => {
                self.failure = Some(error);
                Err(m::MotionError::Missing("native source spatial authority"))
            }
        }
    }
}
impl<A: Authority> Frame<'_, A> {
    fn source_spatial<T>(
        &mut self,
        id: NpcId,
        f: impl FnOnce(&mut Spatial<'_, '_, A>) -> Result<T, m::MotionError>,
    ) -> Result<T, NativeError> {
        let mut adapter = Spatial {
            frame: self,
            npc: id,
            failure: None,
        };
        let result = f(&mut adapter);
        if let Some(error) = adapter.failure {
            return Err(error);
        }
        Ok(result?)
    }
    fn source_target(&self) -> Result<m::Target, NativeError> {
        let g = self
            .profile
            .source_motion
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        Ok(m::Target {
            id: SOLO_UNIT,
            pose: m::Pose {
                position: self.state.position,
                orientation: ai::Real::from_bits(self.state.orientation.bits())?,
            },
            movement_flags: self
                .state
                .player_movement_flags
                .ok_or(NativeError::InvalidState)?,
            combat_reach: self.profile.player.combat_reach,
            bounding_radius: self.profile.player.bounding_radius,
            collision_height: g.player_collision_height,
        })
    }
    fn source_input(&self, npc: &Npc) -> Result<m::UnitInputs, NativeError> {
        let p = self
            .profile
            .npc(npc.id.spawn)
            .ok_or(NativeError::InvalidState)?;
        // Available MageEffect admits direct damage and periodic damage only. It
        // cannot create a slow/root/cast/script modifier; future extensions must
        // update this policy, never independently assert current state in geometry.
        for ability in npc.auras.keys() {
            if !self
                .profile
                .ability(*ability)
                .is_some_and(|a| matches!(a.admission, AbilityAdmission::Available(_)))
            {
                return Err(NativeError::InvalidState);
            }
        }
        let input = self
            .profile
            .source_motion
            .as_ref()
            .ok_or(NativeError::InvalidState)?
            .npc_policy
            .input(npc.health, p.max_health);
        input.validate()?;
        Ok(input)
    }
    fn source_facts(&mut self, npc: &Npc, phase: CombatPhase) -> Result<NpcFacts, NativeError> {
        if self.profile.source_combat_authority() {
            return self.source_npc_facts(npc, phase);
        }
        let position = npc
            .source_motion
            .as_ref()
            .ok_or(NativeError::InvalidState)?
            .motion
            .pose()
            .position;
        let Facts::Npc(facts) = self.ask(
            Some(npc.id),
            Query::Npc {
                position,
                player_position: self.state.position,
                motion: npc.ai.motion().cloned(),
            },
        )?
        else {
            return Err(NativeError::AuthorityBinding);
        };
        Ok(facts)
    }
    fn source_observation(
        &self,
        npc: &Npc,
        facts: NpcFacts,
        movement: Option<ai::movement::ValidatedMovement>,
        receipts: Vec<ai::Receipt>,
    ) -> Result<ai::Observation, NativeError> {
        let source = npc
            .source_motion
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let flags = self
            .state
            .player_movement_flags
            .ok_or(NativeError::InvalidState)?;
        Ok(ai::Observation {
            sequence: self.state.tick,
            elapsed_ms: self.profile.tick_ms,
            geometry_revision: Revision(self.profile.geometry_revision),
            target: Some(ai::Target {
                id: SOLO_UNIT,
                position: self.state.position,
                level: self.state.level(self.profile),
                combat_reach: self.profile.player.combat_reach,
                bounding_radius: self.profile.player.bounding_radius,
                moving_and_running: flags.moving_and_running(),
                alive: self.state.health > 0,
                online: true,
                can_attack_in_combat: facts.can_attack_in_combat,
                can_attack_on_sight: facts.can_attack_on_sight,
                visible: facts.visible,
                accessible: facts.accessible,
                line_of_sight: facts.line_of_sight,
            }),
            can_initiate_attack: facts.can_initiate_attack && source.notify.can_aggro(),
            casting_non_melee: false,
            stationary_melee_refresh_eligible: source
                .motion
                .stationary_melee_refresh_eligible(flags.is_moving()),
            creature_moving_and_running: source.motion.moving_and_running(),
            movement,
            receipts,
        })
    }
    fn source_visitor(
        &mut self,
        npc: &Npc,
        owner: VisitorOwner,
        clock: u64,
    ) -> Result<bool, NativeError> {
        let source = npc
            .source_motion
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let request = VisitorRequest {
            owner,
            event_clock_ms: clock,
            now_ms: self.state.now_ms,
            radius: m::visitor_radius(
                match owner {
                    VisitorOwner::Player => 18,
                    VisitorOwner::Creature(_) => {
                        self.profile
                            .source_motion
                            .as_ref()
                            .unwrap()
                            .creature_detection_range
                    }
                },
                self.profile.npcs[0].ai.aggro_rate,
            )?,
            player_pose: self.source_target()?.pose,
            player_alive: self.state.health > 0,
            creature: npc.id,
            creature_pose: source.motion.pose(),
            creature_phase: npc.ai.phase(),
            creature_can_aggro: source.notify.can_aggro(),
        };
        match self.ask(
            Some(npc.id),
            Query::SourceMotion(MotionQuery::Visitor(request)),
        )? {
            Facts::SourceMotion(MotionFacts::Visitor(VisitorFacts::Complete {
                loaded_membership: ai::movement::Check::Passed,
                call_move_in_line_of_sight,
            })) => Ok(call_move_in_line_of_sight),
            Facts::SourceMotion(MotionFacts::Visitor(VisitorFacts::UnrepresentedParticipants)) => {
                Err(NativeError::UnsupportedMechanic(
                    "source visitor participants outside solo scene",
                ))
            }
            Facts::SourceMotion(MotionFacts::Visitor(_)) => Err(NativeError::AuthorityUnavailable(
                "complete source visitor membership",
            )),
            _ => Err(NativeError::AuthorityBinding),
        }
    }
    pub(super) fn source_combat_receipt(
        &mut self,
        id: NpcId,
        event: ai::CombatEvent,
    ) -> Result<(), NativeError> {
        let mut npc = self
            .state
            .npcs
            .remove(&id.spawn)
            .filter(|n| n.id == id)
            .ok_or(NativeError::UnknownTarget)?;
        let receipt = ai::Receipt {
            id: npc
                .ai
                .last_receipt_id()
                .checked_add(1)
                .ok_or(NativeError::Limit)?,
            event,
        };
        let facts = self.source_facts(&npc, CombatPhase::CombatReceipt)?;
        let observation = self.source_observation(&npc, facts, None, vec![receipt])?;
        let effects = npc
            .ai
            .source_combat_event_with_phase(&observation, self.source_inside_alive_unit)?;
        self.source_prefix_effects(&mut npc, &effects)?;
        self.state.npcs.insert(id.spawn, npc);
        Ok(())
    }
    pub(super) fn source_player_notify_callback(&mut self, clock: u64) -> Result<(), NativeError> {
        if self.profile.source_motion.is_none() {
            return Err(NativeError::InvalidState);
        }
        let id = self.profile.npcs[0].ai.creature_id;
        let mut npc = self
            .state
            .npcs
            .remove(&id)
            .ok_or(NativeError::InvalidState)?;
        if self.source_visitor(&npc, VisitorOwner::Player, clock)? {
            let facts = self.source_facts(&npc, CombatPhase::Sight)?;
            let observation = self.source_observation(&npc, facts, None, Vec::new())?;
            let effects = npc.ai.source_sight_event(&observation)?;
            self.source_prefix_effects(&mut npc, &effects)?;
        }
        self.state.npcs.insert(id, npc);
        Ok(())
    }
    fn source_motion_events(
        &mut self,
        npc: &mut Npc,
        events: Vec<m::Event>,
    ) -> Result<(), NativeError> {
        let g = self
            .profile
            .source_motion
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let source = npc
            .source_motion
            .as_mut()
            .ok_or(NativeError::InvalidState)?;
        for event in events {
            match event {
                m::Event::Relocated {
                    notify_map: true, ..
                } => source.notify.relocated(g.relocation_aggro_delay_ms)?,
                m::Event::HomeStarted { .. } => source.active = true,
                m::Event::HomeReached { restore_inactive } => {
                    if restore_inactive {
                        source.active = false;
                    }
                    source.combat_start = None;
                }
                _ => {}
            }
            self.events.push(Event::NpcMotion { id: npc.id, event });
        }
        Ok(())
    }
    pub(super) fn source_ai_effect(
        &mut self,
        npc: &mut Npc,
        effect: &ai::Effect,
    ) -> Result<(), NativeError> {
        if npc.source_motion.is_none() {
            return Ok(());
        }
        let g = self.profile.source_motion.as_ref().unwrap();
        match effect {
            ai::Effect::CombatStarted { .. } => {
                let source = npc.source_motion.as_mut().unwrap();
                source.combat_start = Some(source.motion.pose());
                source.notify.entered_combat();
                self.enter_player_combat();
            }
            ai::Effect::Motion(command) => {
                if npc.ai.motion() != Some(command) {
                    return Err(NativeError::InvalidState);
                }
                let input = self.source_input(npc)?;
                let target = self.source_target()?;
                let mut source = npc.source_motion.take().unwrap();
                source.motion.begin_map_frame(self.state.now_ms)?;
                let events = match command.purpose {
                    MovementPurpose::Chase { target: id }
                        if npc.ai.target() == Some(id) && id == target.id =>
                    {
                        self.source_spatial(npc.id, |a| {
                            source.motion.start_chase(target, g.chase_walking, input, a)
                        })?
                    }
                    MovementPurpose::Home => {
                        let destination = source.combat_start.unwrap_or(m::Pose {
                            position: self.profile.npc(npc.id.spawn).unwrap().ai.spawn,
                            orientation: ai::Real::from_bits(g.npc_spawn_orientation.bits())?,
                        });
                        if command.home != Some(destination.position) {
                            return Err(NativeError::InvalidState);
                        }
                        self.source_spatial(npc.id, |a| {
                            source.motion.start_home(
                                destination,
                                g.home_walking,
                                source.active,
                                input,
                                a,
                            )
                        })?
                    }
                    _ => return Err(NativeError::InvalidState),
                };
                npc.source_motion = Some(source);
                self.source_motion_events(npc, events)?;
            }
            ai::Effect::Died => {
                for ability in npc.auras.keys() {
                    self.events.push(Event::Spell {
                        cast_id: 0,
                        ability: *ability,
                        target: npc.id,
                        event: spell::SpellEvent::AuraRemoved,
                    });
                }
                npc.auras.clear();
                npc.aura_bindings.clear();
                let source = npc.source_motion.as_mut().unwrap();
                source.motion.begin_map_frame(self.state.now_ms)?;
                let event = source.motion.stop_for_death()?;
                self.events.push(Event::NpcMotion { id: npc.id, event });
            }
            ai::Effect::CorpseRemoved => {
                let source = npc.source_motion.as_mut().unwrap();
                source.motion.clear();
                source.motion.lifecycle_relocate(m::Pose {
                    position: npc.ai.position(),
                    orientation: ai::Real::from_bits(g.npc_spawn_orientation.bits())?,
                })?;
                source.notify.relocated(g.relocation_aggro_delay_ms)?;
            }
            ai::Effect::Respawned { .. } => {
                let source = npc.source_motion.as_mut().unwrap();
                source.active = g.npc_initial_active;
                source.combat_start = None;
                source.notify.respawned(g.creature_initial_aggro_delay_ms)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn source_prefix_effects(
        &mut self,
        npc: &mut Npc,
        effects: &[ai::Effect],
    ) -> Result<(), NativeError> {
        for effect in effects {
            self.events.push(Event::Npc {
                id: npc.id,
                effect: effect.clone(),
            });
            self.source_ai_effect(npc, effect)?;
        }
        Ok(())
    }
    pub(super) fn prepare_source_receipts(
        &mut self,
    ) -> Result<BTreeMap<u64, (ai::ReceiptStep, Option<ai::MotionCommand>)>, NativeError> {
        let id = self.profile.npcs[0].ai.creature_id;
        if self.receipts.get(&id).is_some_and(|r| !r.is_empty()) {
            return Err(NativeError::InvalidState);
        }
        // Source direct damage has already executed synchronous receipt callbacks.
        // The fallback represents an externally staged death before Unit::Update.
        if self.state.npcs[&id].health == 0
            && !matches!(
                self.state.npcs[&id].ai.phase(),
                ai::Phase::Corpse | ai::Phase::Despawned
            )
        {
            let target = self.state.npcs[&id].id;
            self.source_combat_receipt(target, ai::CombatEvent::Died)?;
        }
        let mut npc = self
            .state
            .npcs
            .remove(&id)
            .ok_or(NativeError::InvalidState)?;
        let entered_alive =
            npc.health > 0 && !matches!(npc.ai.phase(), ai::Phase::Corpse | ai::Phase::Despawned);
        self.source_inside_alive_unit = entered_alive;
        if npc.ai.phase() != ai::Phase::Despawned {
            let source = npc
                .source_motion
                .as_mut()
                .ok_or(NativeError::InvalidState)?;
            let due = source
                .notify
                .advance_events(self.profile.tick_ms, npc.health > 0)?;
            let clock = source.notify.snapshot().event_clock_ms;
            if due && self.source_visitor(&npc, VisitorOwner::Creature(npc.id), clock)? {
                let facts = self.source_facts(&npc, CombatPhase::Sight)?;
                let observation = self.source_observation(&npc, facts, None, Vec::new())?;
                let effects = npc.ai.source_sight_event(&observation)?;
                self.source_prefix_effects(&mut npc, &effects)?;
            }
        }
        self.state.npcs.insert(id, npc);
        // Unit::_UpdateSpells follows m_events.Update and precedes spline/motion.
        // Its damage callbacks also execute synchronously before the receipt token.
        self.auras()?;
        let mut npc = self
            .state
            .npcs
            .remove(&id)
            .ok_or(NativeError::InvalidState)?;
        let prior = npc.ai.motion().cloned();
        let facts = self.source_facts(&npc, CombatPhase::UnitPrelude)?;
        let observation = self.source_observation(&npc, facts, None, Vec::new())?;
        let token = npc
            .ai
            .begin_source_receipts_in_phase(&observation, entered_alive)?;
        self.state.npcs.insert(id, npc);
        Ok(BTreeMap::from([(id, (token, prior))]))
    }
    pub(super) fn advance_source_npc(
        &mut self,
        npc: &mut Npc,
        token: ai::ReceiptStep,
    ) -> Result<(Vec<ai::Effect>, Vec<Event>), NativeError> {
        let diff = self.profile.tick_ms;
        let player_position = self.state.position;
        let mut source = npc.source_motion.take().ok_or(NativeError::InvalidState)?;
        let target = self.source_target()?;
        let movement = if npc.health > 0
            && !matches!(npc.ai.phase(), ai::Phase::Corpse | ai::Phase::Despawned)
        {
            // Temporarily restore the single authoritative motion owner for current input construction.
            npc.source_motion = Some(source);
            let input = self.source_input(npc)?;
            source = npc.source_motion.take().unwrap();
            if let Some(command) = npc.ai.motion() {
                let update = self.source_spatial(npc.id, |a| {
                    source.motion.advance_for_command(
                        command,
                        npc.ai
                            .last_movement_request_id()
                            .checked_add(1)
                            .ok_or(m::MotionError::Invalid("movement request identity"))?,
                        diff,
                        Some(target),
                        input,
                        a,
                    )
                })?;
                npc.source_motion = Some(source);
                self.source_motion_events(npc, update.events)?;
                source = npc.source_motion.take().unwrap();
                Some(update.movement)
            } else {
                let events = self.source_spatial(npc.id, |a| {
                    source.motion.advance(diff, Some(target), input, a)
                })?;
                npc.source_motion = Some(source);
                self.source_motion_events(npc, events)?;
                source = npc.source_motion.take().unwrap();
                None
            }
        } else {
            source.motion.advance_without_unit(diff)?;
            None
        };
        npc.source_motion = Some(source);
        let facts = self.source_facts(npc, CombatPhase::AfterMovement)?;
        let observation = self.source_observation(npc, facts, movement, Vec::new())?;
        let prefix = token.effects().len();
        let start = self.events.len();
        let mut source = npc.source_motion.take().unwrap();
        let mut failure = None;
        let result =
            npc.ai
                .finish_receipts_with_current(observation, token, &mut |hook, working_ai| {
                    let action: Result<ai::MainhandResult, NativeError> = (|| match hook {
                        ai::OrderedHook::FaceTarget {
                            target: id,
                            position,
                        } => {
                            if id != SOLO_UNIT || position != source.motion.pose().position {
                                return Err(NativeError::InvalidState);
                            }
                            let event = self.source_spatial(npc.id, |a| {
                                source.motion.face_target(player_position, a)
                            })?;
                            self.events.push(Event::NpcMotion { id: npc.id, event });
                            Ok(ai::MainhandResult::Unchanged)
                        }
                        ai::OrderedHook::Mainhand {
                            target: id,
                            position,
                            combat_started,
                        } => {
                            if id != SOLO_UNIT {
                                return Err(NativeError::InvalidState);
                            }
                            if combat_started {
                                self.enter_player_combat();
                            }
                            if self.state.combat.is_some() {
                                self.native_melee(
                                    npc.id,
                                    position,
                                    npc.health,
                                    Some(Orientation::from_bits(
                                        source.motion.pose().orientation.bits(),
                                    )?),
                                    Some(&interactions::WorkingNpc {
                                        id: npc.id,
                                        health: npc.health,
                                        ai: working_ai,
                                        motion: &source,
                                    }),
                                )?;
                            }
                            Ok(if self.state.health == 0 {
                                ai::MainhandResult::TargetDied
                            } else {
                                ai::MainhandResult::Unchanged
                            })
                        }
                    })();
                    action.map_err(|error| {
                        failure = Some(error);
                        ai::AiError::Invalid("source native AI hook")
                    })
                });
        npc.source_motion = Some(source);
        if let Some(error) = failure {
            return Err(error);
        }
        let callbacks = self.events.split_off(start);
        Ok((result?.into_iter().skip(prefix).collect(), callbacks))
    }
}
