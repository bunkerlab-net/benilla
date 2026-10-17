// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_sim::creature_motion as m;

struct Spatial<'a, 'b, A: Authority> {
    frame: &'a mut Frame<'b, A>,
    npc: NpcId,
    capabilities: Option<CreatureCapabilityWitness>,
    owner_ignore_pathfinding: Option<OwnerIgnorePathfindingReceipt>,
    collision_width: Option<CreatureCollisionWidthWitness>,
    collision_height: Option<CreatureCollisionHeightWitness>,
    failure: Option<NativeError>,
}
impl<A: Authority> m::Authority for Spatial<'_, '_, A> {
    fn resolve(&mut self, query: &m::Query) -> Result<m::Reply, m::MotionError> {
        if let m::Query::Route(request) = query {
            if let Err(error) = request.validate_pathfinder_dispatch() {
                self.failure = Some(NativeError::AuthorityBinding);
                return Err(error);
            }
            if matches!(request.kind, m::RouteKind::HomeForceDestination)
                != request.target.is_none()
            {
                self.failure = Some(NativeError::AuthorityBinding);
                return Err(m::MotionError::Invalid("route player target presence"));
            }
            if let Some(cache) = &request.prior_cache {
                if let Err(error) = self.validate_cache_owner(cache) {
                    self.failure = Some(error);
                    return Err(m::MotionError::Invalid("native pathfinder cache owner"));
                }
            }
        }
        let capability_binding = match query {
            m::Query::NearPoint(request) => Some(request.binding),
            m::Query::Route(request) => Some(request.binding),
            _ => None,
        };
        if let (Some(binding), Some(owner)) = (capability_binding, &self.capabilities) {
            if binding.identity.creature != self.npc.spawn
                || binding.identity.map != self.frame.profile.map_id
                || binding.identity.content.0 != self.frame.profile.content_digest
                || binding.identity.geometry.0 != self.frame.profile.geometry_revision
                || binding.now_ms != self.frame.state.now_ms
                || owner.npc() != self.npc
            {
                self.failure = Some(NativeError::AuthorityBinding);
                return Err(m::MotionError::Invalid(
                    "creature capability request binding",
                ));
            }
        }
        let target = match query {
            m::Query::NearPoint(q) => Some(&q.target),
            m::Query::Route(q) => q.target.as_ref(),
            _ => None,
        };
        let target_player_height = match target
            .map(|target| self.frame.player_target_height(target))
            .transpose()
        {
            Ok(witness) => witness.flatten(),
            Err(error) => {
                self.failure = Some(error);
                return Err(m::MotionError::Invalid("player target model binding"));
            }
        };
        let query = match (query, &self.capabilities) {
            (m::Query::NearPoint(request), Some(searcher)) => {
                MotionQuery::NearPoint(CreatureNearPointRequest {
                    request: request.clone(),
                    searcher: searcher.clone(),
                    target_player_height: target_player_height.clone(),
                })
            }
            (m::Query::Route(request), Some(owner)) => {
                let pathfinder_dispatch = match PathfinderDispatchReceipt::mint(
                    self.frame.profile,
                    self.frame.state.world,
                    self.frame.state.tick,
                    self.npc,
                    owner,
                    request,
                ) {
                    Ok(receipt) => receipt,
                    Err(error) => {
                        self.failure = Some(error);
                        return Err(m::MotionError::Invalid("native pathfinder dispatch"));
                    }
                };
                MotionQuery::Route(Box::new(CreatureRouteRequest {
                    pathfinder_dispatch,
                    request: request.clone(),
                    owner: owner.clone(),
                    owner_ignore_pathfinding: self.owner_ignore_pathfinding.clone(),
                    target_player_water: match self.frame.player_target_water(request) {
                        Ok(witness) => witness,
                        Err(error) => {
                            self.failure = Some(error);
                            return Err(m::MotionError::Invalid("player target water binding"));
                        }
                    },
                    target_player_height: target_player_height.clone(),
                    collision_width: self.collision_width.clone(),
                    collision_height: self.collision_height.clone(),
                    // Motion dispatch builds origin from its executing map_pose after
                    // any forced relocation; Home alone may have a different launch_origin.
                    owner_position: request.origin,
                }))
            }
            _ => MotionQuery::Spatial(query.clone()),
        };
        // Keep the actual runtime-minted event, not evidence supplied on a reply.
        // Replay regenerates this event but still cannot load a native query.
        let pathfinder_dispatch = match &query {
            MotionQuery::Route(request) => request.pathfinder_dispatch.clone(),
            _ => None,
        };
        match self.frame.ask(Some(self.npc), Query::SourceMotion(query)) {
            Ok(Facts::SourceMotion(MotionFacts::Spatial(reply))) => {
                if let m::Fact::RouteCalculation(calculation) = &reply.fact {
                    if calculation.source == m::RouteSource::OwnerPathFinder {
                        let valid = pathfinder_dispatch
                            .as_ref()
                            .ok_or(NativeError::AuthorityUnavailable(
                                "owned pathfinder constructor dispatch",
                            ))
                            .and_then(|event| event.validate_for(self.frame.profile, self.npc));
                        if let Err(error) = valid {
                            self.failure = Some(error);
                            return Err(m::MotionError::Invalid("native source cache dispatch"));
                        }
                    }
                    if let Err(error) = self.validate_cache_owner(&calculation.after_cache) {
                        self.failure = Some(error);
                        return Err(m::MotionError::Invalid("native pathfinder cache owner"));
                    }
                }
                Ok(*reply)
            }
            Ok(_) => Err(m::MotionError::Invalid("native motion reply kind")),
            Err(error) => {
                self.failure = Some(error);
                Err(m::MotionError::Missing("native source spatial authority"))
            }
        }
    }
}
impl<A: Authority> Spatial<'_, '_, A> {
    fn validate_cache_owner(&self, cache: &m::PathFinderCache) -> Result<(), NativeError> {
        let owner = self
            .capabilities
            .as_ref()
            .ok_or(NativeError::AuthorityBinding)?;
        owner.validate_for(self.frame.profile, self.npc)?;
        let include_flags = benilla_sim::route_filter::initial_include_flags(owner.capabilities()?)
            .ok_or(NativeError::AuthorityBinding)?;
        if cache.include_flags != include_flags
            || cache.exclude_flags != 0
            || cache.binding.owner_generation != self.npc.generation
            || cache.binding.identity.creature != self.npc.spawn
            || cache.binding.identity.map != self.frame.profile.map_id
            || cache.binding.identity.content.0 != self.frame.profile.content_digest
            || cache.binding.identity.geometry.0 != self.frame.profile.geometry_revision
        {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
}
impl<A: Authority> Frame<'_, A> {
    fn source_spatial<T>(
        &mut self,
        id: NpcId,
        generation: Option<&CreatureGenerationOwner>,
        capabilities: Option<&CreatureCapabilityState>,
        collision_width: Option<&CreatureCollisionWidthState>,
        collision_height: Option<&CreatureCollisionHeightState>,
        f: impl FnOnce(&mut Spatial<'_, '_, A>) -> Result<T, m::MotionError>,
    ) -> Result<T, NativeError> {
        if capabilities.is_some() != self.profile.creature_capabilities.is_some() {
            return Err(NativeError::InvalidState);
        }
        if let Some(capabilities) = capabilities {
            capabilities.validate(self.profile, id)?;
        }
        match (&self.profile.creature_generations, generation) {
            (Some(_), Some(owner)) => {
                CreatureCollisionWidthState::validate_generation_optional(
                    collision_width,
                    self.profile,
                    id,
                    owner,
                )?;
                CreatureCollisionHeightState::validate_generation_optional(
                    collision_height,
                    self.profile,
                    id,
                    owner,
                )?;
            }
            (None, None) => {
                CreatureCollisionWidthState::validate_optional(collision_width, self.profile, id)?;
                CreatureCollisionHeightState::validate_optional(
                    collision_height,
                    self.profile,
                    id,
                )?;
            }
            _ => return Err(NativeError::AuthorityBinding),
        }
        let owner_ignore_pathfinding =
            OwnerIgnorePathfindingReceipt::mint(self.profile, self.state, id, capabilities)?;
        let mut adapter = Spatial {
            frame: self,
            npc: id,
            owner_ignore_pathfinding,
            capabilities: capabilities.map(|state| state.witness().clone()),
            collision_width: collision_width.map(|state| state.witness().clone()),
            collision_height: collision_height.map(|state| state.witness().clone()),
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
            collision_height: match self.state.source_player_collision_height.as_ref() {
                Some(source) => {
                    PlayerCollisionHeightState::validate_optional(
                        Some(source),
                        self.profile,
                        self.state.world,
                        self.state.player,
                    )?;
                    ai::Real::from_bits(source.witness().height().bits())?
                }
                None => {
                    PlayerCollisionHeightState::validate_optional(
                        None,
                        self.profile,
                        self.state.world,
                        self.state.player,
                    )?;
                    g.player_collision_height
                }
            },
        })
    }
    fn player_target_height(
        &self,
        target: &m::Target,
    ) -> Result<Option<PlayerCollisionHeightTarget>, NativeError> {
        if target != &self.source_target()? {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(self
            .state
            .source_player_collision_height
            .as_ref()
            .map(|source| PlayerCollisionHeightTarget::new(source, *target)))
    }
    fn player_target_water(
        &self,
        request: &m::RouteRequest,
    ) -> Result<Option<PlayerEnvironmentTarget>, NativeError> {
        if !matches!(
            request.kind,
            m::RouteKind::ChaseInitial | m::RouteKind::ChaseReplan
        ) {
            return Ok(None);
        }
        let target = request
            .target
            .as_ref()
            .ok_or(NativeError::AuthorityBinding)?;
        if target != &self.source_target()? {
            return Err(NativeError::AuthorityBinding);
        }
        PlayerEnvironmentTarget::new(self.profile, self.state, *target)
    }

    fn source_input(&self, npc: &Npc) -> Result<m::UnitInputs, NativeError> {
        let p = npc.effective_profile(self.profile)?;
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
        let mut policy = self.profile.npc_motion(npc.id.spawn)?.npc_policy.clone();
        if let Some(selected) = npc.initialized_creature() {
            policy.walk_speed = ai::Real::from_bits(selected.model.walk_speed.bits())?;
            policy.run_speed = p.ai.run_speed;
            policy.settings.bounding_radius = p.ai.bounding_radius;
            policy.settings.combat_reach = p.ai.combat_reach;
            if let Some(height) = npc.creature_collision_height() {
                policy.settings.collision_height =
                    ai::Real::from_bits(height.witness().height().bits())?;
            }
        }
        let input = policy.input(npc.health, p.max_health);
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
    pub(super) fn source_observation(
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
                // Once linked, only the owned HostileReference callback changes online state.
                // Before acquisition, the selected source catalogs independently resolve it.
                online: match npc.ai.hostile_reference() {
                    Some(reference) => reference.online(),
                    None if self.profile.source_combat_authority() => {
                        !self.profile.source_player_offline_target(npc.id)?
                    }
                    None => true, // legacy source-motion adapter without source combat authority
                },
                can_attack_in_combat: facts.can_attack_in_combat,
                can_attack_on_sight: facts.can_attack_on_sight,
                visible: facts.visible,
                accessible: facts.accessible,
                line_of_sight: facts.line_of_sight,
            }),
            can_initiate_attack: facts.can_initiate_attack && npc.events.can_aggro(),
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
        callback: &benilla_sim::event_processor::EventCallbackSnapshot,
    ) -> Result<bool, NativeError> {
        let source = npc
            .source_motion
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let request = VisitorRequest {
            owner,
            callback: callback.clone(),
            event_clock_ms: callback.due.now_ms,
            now_ms: self.state.now_ms,
            radius: m::visitor_radius(
                match owner {
                    VisitorOwner::Player => 18,
                    VisitorOwner::Creature(_) => {
                        self.profile
                            .npc_motion(npc.id.spawn)?
                            .creature_detection_range
                    }
                },
                self.profile
                    .source_motion
                    .as_ref()
                    .ok_or(NativeError::InvalidState)?
                    .aggro_rate,
            )?,
            player_pose: self.source_target()?.pose,
            player_alive: self.state.health > 0,
            creature: npc.id,
            creature_pose: source.motion.pose(),
            creature_phase: npc.ai.phase(),
            creature_can_aggro: npc.events.can_aggro(),
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
        let timing = self.creature_death_timing(&mut npc, &receipt)?;
        let facts = self.source_facts(&npc, CombatPhase::CombatReceipt)?;
        let observation = self.source_observation(&npc, facts, None, vec![receipt.clone()])?;
        let effects = npc.ai.source_combat_event_with_death_timing(
            &observation,
            self.source_alive_unit == Some(id),
            timing,
        )?;
        self.source_prefix_effects(
            &mut npc,
            &effects,
            &EventAiCallbackKind::CombatReceipt { receipt },
        )?;
        self.state.npcs.insert(id.spawn, npc);
        Ok(())
    }
    pub(super) fn source_player_notify_callback(
        &mut self,
        callback: &benilla_sim::event_processor::EventCallbackSnapshot,
    ) -> Result<(), NativeError> {
        if self.profile.source_motion.is_none() {
            return Err(NativeError::InvalidState);
        }
        if self.profile.source_visitors.is_some() {
            return self.source_selected_player_notify(callback);
        }
        let id = self.profile.npcs[0].ai.creature_id;
        let mut npc = self
            .state
            .npcs
            .remove(&id)
            .ok_or(NativeError::InvalidState)?;
        if self.source_visitor(&npc, VisitorOwner::Player, callback)? {
            let facts = self.source_facts(&npc, CombatPhase::Sight)?;
            let observation = self.source_observation(&npc, facts, None, Vec::new())?;
            let effects = npc.ai.source_sight_event(&observation)?;
            self.source_prefix_effects(
                &mut npc,
                &effects,
                &EventAiCallbackKind::Sight {
                    notification_owner: VisitorParticipant::Player(self.state.player),
                    callback: callback.clone(),
                },
            )?;
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
                } => npc
                    .events
                    .schedule_notify(g.relocation_aggro_delay_ms, false)?,
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
        callback: Option<&EventAiCallbackKind>,
    ) -> Result<(), NativeError> {
        if npc.source_motion.is_none() {
            return Ok(());
        }
        let shared = self
            .profile
            .source_motion
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let g = self.profile.npc_motion(npc.id.spawn)?;
        match effect {
            ai::Effect::CombatStarted { target } => {
                if self.profile.combat_assistance().is_some() && npc.events.callback.is_some() {
                    return Err(NativeError::UnsupportedMechanic(
                        "assistance entry inside owning NPC callback",
                    ));
                }
                let source = npc.source_motion.as_mut().unwrap();
                source.combat_start = Some(source.motion.pose());
                npc.events.entered_combat()?;
                self.event_ai_enter_combat(npc, *target, callback)?;
                self.combat_entry_assistance(npc, *target, callback)?;
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
                        self.source_spatial(
                            npc.id,
                            npc.source_generation.as_ref(),
                            npc.source_capabilities.as_ref(),
                            npc.source_collision_width.as_ref(),
                            npc.source_collision_height.as_ref(),
                            |a| source.motion.start_chase(target, g.chase_walking, input, a),
                        )?
                    }
                    MovementPurpose::Home => {
                        let destination = source.combat_start.unwrap_or(m::Pose {
                            position: self.profile.npc(npc.id.spawn).unwrap().ai.spawn,
                            orientation: ai::Real::from_bits(g.npc_spawn_orientation.bits())?,
                        });
                        if command.home != Some(destination.position) {
                            return Err(NativeError::InvalidState);
                        }
                        self.source_spatial(
                            npc.id,
                            npc.source_generation.as_ref(),
                            npc.source_capabilities.as_ref(),
                            npc.source_collision_width.as_ref(),
                            npc.source_collision_height.as_ref(),
                            |a| {
                                source.motion.start_home(
                                    destination,
                                    g.home_walking,
                                    source.active,
                                    input,
                                    a,
                                )
                            },
                        )?
                    }
                    _ => return Err(NativeError::InvalidState),
                };
                npc.source_motion = Some(source);
                self.source_motion_events(npc, events)?;
            }
            ai::Effect::EvadeStarted(_) => {
                // UnitAI::EnterEvadeMode calls CombatStop before returning Home.
                npc.events
                    .assistance_combat_stop(self.state.tick, AssistanceStopReason::Evade)?;
            }
            ai::Effect::Died => {
                // Unit::SetDeathState(JUST_DIED) calls CombatStop, Unit.cpp:8562.
                npc.events
                    .assistance_combat_stop(self.state.tick, AssistanceStopReason::Death)?;
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
                npc.events
                    .schedule_notify(shared.relocation_aggro_delay_ms, false)?;
            }
            ai::Effect::Respawned { .. } => {
                let source = npc.source_motion.as_mut().unwrap();
                source.active = g.npc_initial_active;
                source.combat_start = None;
                npc.events
                    .respawned(shared.creature_initial_aggro_delay_ms)?;
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn source_prefix_effects(
        &mut self,
        npc: &mut Npc,
        effects: &[ai::Effect],
        callback: &EventAiCallbackKind,
    ) -> Result<(), NativeError> {
        for effect in effects {
            self.events.push(Event::Npc {
                id: npc.id,
                effect: effect.clone(),
            });
            self.source_ai_effect(npc, effect, Some(callback))?;
        }
        Ok(())
    }
    pub(super) fn npc_event_update(&mut self, npc: &mut Npc) -> Result<(), NativeError> {
        use benilla_sim::event_processor::{CallbackKind, EventKind};
        if npc.ai.phase() == ai::Phase::Despawned {
            npc.events.skipped_updates = npc
                .events
                .skipped_updates
                .checked_add(1)
                .ok_or(NativeError::Limit)?;
            return Ok(());
        }
        // The enclosing Frame owns atomic world/RNG/output rollback. Only this
        // transaction owns the mutable queue while callbacks borrow the NPC.
        let mut queue = std::mem::take(&mut npc.events.queue);
        let mut transaction = queue.begin_update(self.profile.tick_ms)?;
        while let Some(due) = transaction.pop_due()? {
            if matches!(
                due.kind,
                EventKind::ReenableHelp { .. } | EventKind::AggroLinking { .. }
            ) {
                let in_combat = npc.ai.phase() == ai::Phase::Combat;
                // The bounded direct enemy is the same retained player Unit, even dead.
                npc.events.dispatch_assistance(
                    self.state.tick,
                    due,
                    Some(self.state.player),
                    in_combat,
                )?;
                if due.callback == CallbackKind::Execute {
                    transaction.finish_execute(true)?;
                } else {
                    transaction.finish_abort(true)?;
                }
                continue;
            }
            if let EventKind::SpellUpdate { cast_id } = due.kind {
                let lifetime = npc
                    .events
                    .spells
                    .remove(&cast_id)
                    .ok_or(NativeError::InvalidState)?;
                if lifetime.event_id != due.event_id
                    || lifetime.caster != npc.id
                    || lifetime.terminal.is_none()
                {
                    return Err(NativeError::InvalidState);
                }
                // Instant nonchanneled triggered spells have no current-slot reference.
                // Both failed and successful SpellStart objects are already finished.
                if due.callback == CallbackKind::Execute {
                    transaction.finish_execute(true)?;
                } else {
                    transaction.finish_abort(true)?;
                }
                self.events.push(Event::NpcSpell(NpcSpellEvent::Destroyed {
                    lifetime,
                    aborted: due.callback == CallbackKind::Abort,
                }));
                continue;
            }
            if npc.events.notify != Some(due.event_id) {
                return Err(NativeError::InvalidState);
            }
            let callback = transaction.callback_snapshot()?;
            npc.events.callback = Some(callback.clone());
            if due.callback == CallbackKind::Execute {
                npc.events.can_aggro = npc.health > 0;
                if self.profile.source_visitors.is_some() {
                    self.source_selected_npc_notify(npc, &callback)?;
                } else if self.source_visitor(npc, VisitorOwner::Creature(npc.id), &callback)? {
                    let facts = self.source_facts(npc, CombatPhase::Sight)?;
                    let observation = self.source_observation(npc, facts, None, Vec::new())?;
                    let effects = npc.ai.source_sight_event(&observation)?;
                    self.source_prefix_effects(
                        npc,
                        &effects,
                        &EventAiCallbackKind::Sight {
                            notification_owner: VisitorParticipant::Creature(npc.id),
                            callback: callback.clone(),
                        },
                    )?;
                }
                transaction.finish_execute(true)?;
            } else {
                transaction.finish_abort(true)?;
            }
            // FinalizeAINotifyEvent is after the visitor, not before it.
            npc.events.notify = None;
            npc.events.callback = None;
        }
        transaction.commit()?;
        npc.events.queue = queue;
        Ok(())
    }
    pub(super) fn source_npc_update(
        &mut self,
        id: NpcId,
        swings: &mut Vec<NpcId>,
    ) -> Result<(), NativeError> {
        let result = (|| {
            if self.source_alive_unit.is_some() {
                return Err(NativeError::InvalidState);
            }
            let receipt = self.prepare_source_receipt(id)?;
            self.npc_update(id, Some(receipt), swings)
        })();
        // Keep the ALIVE branch local even when an authority rejects the staged
        // update. The enclosing world transaction owns all state/output rollback.
        self.source_alive_unit = None;
        result
    }
    fn prepare_source_receipt(
        &mut self,
        id: NpcId,
    ) -> Result<(ai::ReceiptStep, Option<ai::MotionCommand>), NativeError> {
        let npc = self
            .state
            .npcs
            .get(&id.spawn)
            .filter(|npc| npc.id == id)
            .ok_or(NativeError::UnknownTarget)?;
        if self.receipts.get(&id.spawn).is_some_and(|r| !r.is_empty()) {
            return Err(NativeError::InvalidState);
        }
        // Source direct damage has already executed synchronous receipt callbacks.
        // The fallback represents an externally staged death before Unit::Update.
        if npc.health == 0 && !matches!(npc.ai.phase(), ai::Phase::Corpse | ai::Phase::Despawned) {
            self.source_combat_receipt(id, ai::CombatEvent::Died)?;
        }
        let mut npc = self
            .state
            .npcs
            .remove(&id.spawn)
            .ok_or(NativeError::InvalidState)?;
        let entered_alive =
            npc.health > 0 && !matches!(npc.ai.phase(), ai::Phase::Corpse | ai::Phase::Despawned);
        self.source_alive_unit = entered_alive.then_some(id);
        self.npc_event_update(&mut npc)?;
        self.state.npcs.insert(id.spawn, npc);
        // Unit::_UpdateSpells follows this owner's events and precedes its motion.
        // Its damage callbacks also execute synchronously before the receipt token.
        self.npc_auras(id)?;
        let mut npc = self
            .state
            .npcs
            .remove(&id.spawn)
            .ok_or(NativeError::InvalidState)?;
        let prior = npc.ai.motion().cloned();
        let facts = self.source_facts(&npc, CombatPhase::UnitPrelude)?;
        let observation = self.source_observation(&npc, facts, None, Vec::new())?;
        let token = npc
            .ai
            .begin_source_receipts_in_phase(&observation, entered_alive)?;
        self.state.npcs.insert(id.spawn, npc);
        Ok((token, prior))
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
                let update = self.source_spatial(
                    npc.id,
                    npc.source_generation.as_ref(),
                    npc.source_capabilities.as_ref(),
                    npc.source_collision_width.as_ref(),
                    npc.source_collision_height.as_ref(),
                    |a| {
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
                    },
                )?;
                npc.source_motion = Some(source);
                self.source_motion_events(npc, update.events)?;
                source = npc.source_motion.take().unwrap();
                Some(update.movement)
            } else {
                let events = self.source_spatial(
                    npc.id,
                    npc.source_generation.as_ref(),
                    npc.source_capabilities.as_ref(),
                    npc.source_collision_width.as_ref(),
                    npc.source_collision_height.as_ref(),
                    |a| source.motion.advance(diff, Some(target), input, a),
                )?;
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
        let effective = npc.effective_profile(self.profile)?;
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
                            let event = self.source_spatial(
                                npc.id,
                                npc.source_generation.as_ref(),
                                npc.source_capabilities.as_ref(),
                                npc.source_collision_width.as_ref(),
                                npc.source_collision_height.as_ref(),
                                |a| source.motion.face_target(player_position, a),
                            )?;
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
                                    &effective,
                                    Some(Orientation::from_bits(
                                        source.motion.pose().orientation.bits(),
                                    )?),
                                    Some(&interactions::WorkingNpc {
                                        id: npc.id,
                                        health: npc.health,
                                        generation: npc.source_generation.as_ref(),
                                        ai: working_ai,
                                        motion: &source,
                                        events: &npc.events,
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
