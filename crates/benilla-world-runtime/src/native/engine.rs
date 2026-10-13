// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::NpcId;
use ai::movement::{
    GeometryAuthority, MovementOutcome, MovementPurpose, MovementRequest, Revision,
};

impl NativeWorld {
    /// Whole-frame staging includes inputs, all component states, RNG, facts, events and size
    /// limits. A missing navigation/melee adapter cannot partially advance the native world.
    pub fn step(
        &mut self,
        tick: u64,
        input: Option<OwnedInput>,
        authority: &mut impl Authority,
    ) -> Result<&TickRecord, NativeError> {
        if tick != self.state.tick.checked_add(1).ok_or(NativeError::Limit)?
            || tick > crate::MAX_TICK
        {
            return Err(NativeError::InvalidInput);
        }
        if self.journal.len() >= MAX_RECORDS {
            return Err(NativeError::Limit);
        }
        let input = input
            .map(|owned| {
                let i = owned.input;
                if owned.authenticated_owner != self.state.player.owner
                    || i.player != self.state.player
                    || i.world != self.state.world
                    || i.tick != tick
                    || i.sequence != self.state.next_sequence
                {
                    return Err(NativeError::InvalidInput);
                }
                Ok(i)
            })
            .transpose()?;
        let before = self.state_digest();
        let mut next = self.state.clone();
        next.tick = tick;
        next.now_ms = next
            .now_ms
            .checked_add(u64::from(self.profile.tick_ms))
            .ok_or(NativeError::Limit)?;
        let mut frame = Frame {
            profile: &self.profile,
            profile_digest: self.profile_digest,
            state: &mut next,
            authority,
            replies: vec![],
            events: vec![],
            draws: vec![],
            receipts: BTreeMap::new(),
            moved: false,
        };
        if let Some(i) = &input {
            frame.command(&i.command)?;
            frame.state.next_sequence = frame
                .state
                .next_sequence
                .checked_add(1)
                .ok_or(NativeError::Limit)?;
        }
        frame.casts()?;
        frame.auras()?;
        frame.player_reactives();
        frame.npcs()?;
        frame.player_combat_update();
        frame.mana()?;
        if frame
            .state
            .rng_counter
            .checked_sub(self.state.rng_counter)
            .ok_or(NativeError::InvalidState)?
            > MAX_RNG_WORDS_PER_TICK
        {
            return Err(NativeError::Limit);
        }
        let replies = frame.replies;
        let events = frame.events;
        let draws = frame.draws;
        let after = crate::digest(b"benilla-native-state-v2\0", &self.snapshot_of(&next));
        let record = TickRecord {
            version: NATIVE_FORMAT_VERSION,
            profile: self.profile_digest,
            tick,
            input,
            before,
            after,
            previous_record: self.journal_tip,
            replies,
            events,
            draws,
        };
        let size = serde_json::to_vec(&record)
            .map_err(|_| NativeError::InvalidState)?
            .len();
        let bytes = self
            .journal_bytes
            .checked_add(size + usize::from(!self.journal.is_empty()))
            .ok_or(NativeError::TooLarge)?;
        if bytes > MAX_BYTES {
            return Err(NativeError::TooLarge);
        }
        let tip = record.digest();
        self.make_checkpoint(&next, tip)?;
        self.state = next;
        self.journal_tip = tip;
        self.journal.push(record);
        self.journal_bytes = bytes;
        Ok(self.journal.last().unwrap())
    }
}
struct Frame<'a, A> {
    profile: &'a Profile,
    profile_digest: crate::Digest,
    state: &'a mut State,
    authority: &'a mut A,
    replies: Vec<AuthorityReply>,
    events: Vec<Event>,
    draws: Vec<Draw>,
    receipts: BTreeMap<u64, Vec<ai::Receipt>>,
    moved: bool,
}
impl<A: Authority> Frame<'_, A> {
    fn ask(&mut self, npc: Option<NpcId>, query: Query) -> Result<Facts, NativeError> {
        if self.replies.len() >= 20_000 {
            return Err(NativeError::TooLarge);
        }
        let request = AuthorityRequest {
            binding: Binding {
                world: self.state.world,
                tick: self.state.tick,
                profile: self.profile_digest,
                authority_policy: self.profile.authority_policy,
                geometry: self.profile.geometry_revision,
                npc,
            },
            query,
        };
        let reply = self.authority.resolve(&request)?;
        if reply.request != request {
            return Err(NativeError::AuthorityBinding);
        }
        self.replies.push(reply.clone());
        Ok(reply.facts)
    }
    fn facts(
        &mut self,
        cast_id: u64,
        ability: u32,
        target: NpcId,
        phase: CastPhase,
    ) -> Result<SpellFacts, NativeError> {
        let n = self
            .state
            .npcs
            .get(&target.spawn)
            .filter(|n| n.id == target)
            .ok_or(NativeError::UnknownTarget)?;
        let q = Query::Spell {
            cast_id,
            ability,
            phase,
            player_position: self.state.position,
            target_position: n.ai.position(),
        };
        let Facts::Spell(mut f) = self.ask(Some(target), q)? else {
            return Err(NativeError::AuthorityBinding);
        };
        if f.resist_chance > 10000 || f.critical_chance > 10000 {
            return Err(NativeError::InvalidState);
        }
        if let Some(c) = &self.state.combat {
            f.critical_chance = spell::combat_chance(c.stats.crit_percent)?;
        }
        Ok(f)
    }
    fn spell_events(
        &mut self,
        cast_id: u64,
        ability: u32,
        target: NpcId,
        events: Vec<spell::SpellEvent>,
    ) -> Result<(), NativeError> {
        for event in events {
            // ThreatManager::addThreat (pinned 8ec338a1, lines 446-463) rejects
            // a dead threat owner or dead hated caster; DOT damage itself persists.
            let can_add_threat = self.state.health > 0
                && self
                    .state
                    .npcs
                    .get(&target.spawn)
                    .is_some_and(|n| n.id == target && n.health > 0);
            match event {
                spell::SpellEvent::DirectDamage { effective, .. }
                    if effective > 0 && can_add_threat =>
                {
                    self.receipt(target, ai::CombatEvent::AttackedBy { target: SOLO_UNIT })?;
                    self.receipt(
                        target,
                        ai::CombatEvent::Threat {
                            target: SOLO_UNIT,
                            amount: ai::Real::new(effective as f32)?,
                            kind: ai::ThreatKind::DirectDamage,
                        },
                    )?;
                }
                spell::SpellEvent::PeriodicDamage { effective, .. }
                    if effective > 0 && can_add_threat =>
                {
                    self.receipt(
                        target,
                        ai::CombatEvent::Threat {
                            target: SOLO_UNIT,
                            amount: ai::Real::new(effective as f32)?,
                            kind: ai::ThreatKind::PeriodicDamage,
                        },
                    )?
                }
                _ => {}
            }
            self.events.push(Event::Spell {
                cast_id,
                ability,
                target,
                event,
            });
        }
        Ok(())
    }
    fn receipt(&mut self, target: NpcId, event: ai::CombatEvent) -> Result<(), NativeError> {
        let npc = self
            .state
            .npcs
            .get(&target.spawn)
            .filter(|n| n.id == target)
            .ok_or(NativeError::UnknownTarget)?;
        let pending = self.receipts.entry(target.spawn).or_default();
        if pending.len() >= 64 {
            return Err(NativeError::Limit);
        }
        let id = npc
            .ai
            .last_receipt_id()
            .checked_add(pending.len() as u64 + 1)
            .ok_or(NativeError::Limit)?;
        pending.push(ai::Receipt { id, event });
        Ok(())
    }
    fn command(&mut self, command: &Command) -> Result<(), NativeError> {
        let rejection = match command {
            Command::Cast { ability, target } => {
                let Some(a) = self.profile.ability(*ability) else {
                    self.events.push(Event::Rejected(Rejection::UnknownAbility));
                    return Ok(());
                };
                let spec = match &a.admission {
                    AbilityAdmission::Unavailable { reason } => {
                        self.events
                            .push(Event::Rejected(Rejection::UnavailableAbility {
                                ability: *ability,
                                reason: reason.clone(),
                            }));
                        return Ok(());
                    }
                    AbilityAdmission::Available(s) => s.clone(),
                };
                if self.state.health == 0 {
                    Some(Rejection::Dead)
                } else if self
                    .state
                    .casts
                    .values()
                    .any(|c| matches!(c.execution.state(), spell::CastState::Casting { .. }))
                {
                    Some(Rejection::AlreadyCasting)
                } else if !self.state.npcs.get(&target.spawn).is_some_and(|n| {
                    n.id == *target
                        && n.health > 0
                        && !matches!(
                            n.ai.phase(),
                            ai::Phase::Despawned | ai::Phase::ReturningHome
                        )
                }) {
                    Some(Rejection::UnknownTarget)
                } else {
                    if self.state.casts.len() >= MAX_CASTS {
                        return Err(NativeError::Limit);
                    }
                    let id = self.state.next_cast_id;
                    let facts = self.facts(id, *ability, *target, CastPhase::Prepare)?;
                    let checks = spell::CastChecks {
                        caster_alive: true,
                        target_exists: true,
                        target_alive: true,
                        position_changed: false,
                        other_failure: facts.other_failure,
                    };
                    let (execution, events) = spell::MageProjectileCast::prepare(
                        spec,
                        SOLO_UNIT,
                        target.spawn,
                        self.state.now_ms,
                        &mut self.state.resources.caster,
                        checks,
                    )?;
                    let failed = match execution.state() {
                        spell::CastState::Failed(f) => Some(f),
                        _ => None,
                    };
                    self.spell_events(id, *ability, *target, events)?;
                    if failed.is_none() {
                        self.state.next_cast_id = id.checked_add(1).ok_or(NativeError::Limit)?;
                        self.state.casts.insert(
                            id,
                            Cast {
                                id,
                                ability: *ability,
                                target: *target,
                                origin: self.state.position,
                                execution,
                            },
                        );
                    }
                    failed.map(Rejection::CastFailed)
                }
            }
            Command::CancelCast => {
                let current = self
                    .state
                    .casts
                    .iter()
                    .find(|(_, c)| matches!(c.execution.state(), spell::CastState::Casting { .. }))
                    .map(|(id, _)| *id);
                if let Some(id) = current {
                    let mut c = self.state.casts.remove(&id).unwrap();
                    let events = c.execution.cancel(
                        &mut self.state.resources.caster,
                        spell::CastFailure::Interrupted,
                    );
                    self.spell_events(id, c.ability, c.target, events)?;
                    None
                } else {
                    Some(Rejection::NoCast)
                }
            }
            Command::Move { destination } => {
                destination.distance_squared(*destination)?;
                if self.state.health == 0 {
                    Some(Rejection::Dead)
                } else {
                    let Facts::PlayerMovement { accepted } = self.ask(
                        None,
                        Query::PlayerMovement {
                            origin: self.state.position,
                            destination: *destination,
                            elapsed_ms: self.profile.tick_ms,
                        },
                    )?
                    else {
                        return Err(NativeError::AuthorityBinding);
                    };
                    if accepted {
                        self.moved = *destination != self.state.position;
                        self.state.position = *destination;
                        self.events.push(Event::PlayerMoved(*destination));
                        None
                    } else {
                        Some(Rejection::MovementBlocked)
                    }
                }
            }
        };
        if let Some(r) = rejection {
            self.events.push(Event::Rejected(r));
        }
        Ok(())
    }
    fn rolls(
        &mut self,
        id: u64,
        spec: &spell::MageProjectileSpec,
        facts: &SpellFacts,
    ) -> Result<spell::LaunchRolls, NativeError> {
        let hit = self
            .state
            .draw(id, DrawPurpose::Hit, 1, 10000, &mut self.draws)? as u16;
        let mut direct = [None; 3];
        let mut damage = 0;
        if hit > facts.resist_chance {
            for (slot, effect) in spec.effects.iter().enumerate() {
                if let spell::MageEffect::SchoolDamage(value) = effect {
                    if let Some((lo, hi)) = value.roll_range(spec.spell_rank)? {
                        direct[slot] = Some(self.state.draw(
                            id,
                            DrawPurpose::Direct { slot: slot as u8 },
                            lo,
                            hi,
                            &mut self.draws,
                        )?);
                    }
                    damage = value.evaluate(spec.spell_rank, direct[slot])?.max(0);
                }
            }
        }
        let critical = if damage > 0 && facts.critical_chance != 0 {
            Some(
                self.state
                    .draw(id, DrawPurpose::Critical, 1, 10000, &mut self.draws)?
                    as u16,
            )
        } else {
            None
        };
        Ok(spell::LaunchRolls {
            hit,
            direct,
            critical,
        })
    }
    fn casts(&mut self) -> Result<(), NativeError> {
        let casts = std::mem::take(&mut self.state.casts);
        for (id, mut c) in casts {
            let exists = self
                .state
                .npcs
                .get(&c.target.spawn)
                .is_some_and(|n| n.id == c.target && n.ai.phase() != ai::Phase::Despawned);
            let alive = exists && self.state.npcs[&c.target.spawn].health > 0;
            let moved = c.origin != self.state.position;
            let spec = match &self
                .profile
                .ability(c.ability)
                .ok_or(NativeError::InvalidState)?
                .admission
            {
                AbilityAdmission::Available(s) => s.clone(),
                _ => return Err(NativeError::InvalidState),
            };
            let mut checks = spell::CastChecks {
                caster_alive: self.state.health > 0,
                target_exists: exists,
                target_alive: alive,
                position_changed: moved,
                other_failure: None,
            };
            let launch = if c
                .execution
                .release_due(self.state.now_ms, self.profile.tick_ms)
                && exists
                && alive
                && !(moved && spec.movement_interrupts)
            {
                let facts = self.facts(id, c.ability, c.target, CastPhase::Release)?;
                checks.other_failure = facts.other_failure;
                if c.execution
                    .release_failure(&self.state.resources.caster, checks, self.state.now_ms)
                    .is_none()
                {
                    let distance = self
                        .state
                        .position
                        .distance_squared(self.state.npcs[&c.target.spawn].ai.position())?;
                    Some(spell::LaunchContext {
                        distance_squared: spell::SpellFloat::from_bits(distance.bits())?,
                        resist_chance: facts.resist_chance,
                        critical_chance: facts.critical_chance,
                        rolls: self.rolls(id, &spec, &facts)?,
                    })
                } else {
                    None
                }
            } else {
                None
            };
            let events = c.execution.update(
                self.state.now_ms,
                self.profile.tick_ms,
                &mut self.state.resources.caster,
                checks,
                launch,
            )?;
            self.spell_events(id, c.ability, c.target, events)?;
            if matches!(c.execution.state(),spell::CastState::Traveling{impact_at_ms} if impact_at_ms<=self.state.now_ms)
            {
                let facts = self.facts(id, c.ability, c.target, CastPhase::Impact)?;
                let npc = self.state.npcs.get_mut(&c.target.spawn).unwrap();
                let mut target = npc
                    .auras
                    .remove(&c.ability)
                    .unwrap_or(spell::DamageTarget::new(c.target.spawn, npc.health)?);
                target.health = npc.health;
                let mut aura_rolls = [None; 3];
                let hit = c
                    .execution
                    .launch_context()
                    .is_some_and(|l| l.rolls.hit > l.resist_chance);
                if hit && facts.allowed && !facts.immune && target.health > 0 {
                    for (slot, effect) in spec.effects.iter().enumerate() {
                        if let spell::MageEffect::PeriodicDamage { value, .. } = effect {
                            if let Some((lo, hi)) = value.roll_range(spec.spell_rank)? {
                                aura_rolls[slot] = Some(self.state.draw(
                                    id,
                                    DrawPurpose::Periodic { slot: slot as u8 },
                                    lo,
                                    hi,
                                    &mut self.draws,
                                )?);
                            }
                        }
                    }
                }
                let events = c.execution.impact(
                    self.state.now_ms,
                    &mut target,
                    facts.allowed,
                    facts.immune,
                    aura_rolls,
                )?;
                let npc = self.state.npcs.get_mut(&c.target.spawn).unwrap();
                npc.health = target.health;
                if target.aura().is_some() {
                    if npc.auras.len() >= MAX_AURAS {
                        return Err(NativeError::Limit);
                    }
                    npc.auras.insert(c.ability, target);
                }
                self.spell_events(id, c.ability, c.target, events)?;
            }
            if matches!(
                c.execution.state(),
                spell::CastState::Casting { .. } | spell::CastState::Traveling { .. }
            ) {
                self.state.casts.insert(id, c);
            }
        }
        Ok(())
    }
    fn auras(&mut self) -> Result<(), NativeError> {
        let ids = self.state.npcs.keys().copied().collect::<Vec<_>>();
        for id in ids {
            let spells = self.state.npcs[&id]
                .auras
                .keys()
                .copied()
                .collect::<Vec<_>>();
            for ability in spells {
                let target_id = self.state.npcs[&id].id;
                let facts = self.facts(0, ability, target_id, CastPhase::Aura)?;
                let npc = self.state.npcs.get_mut(&id).unwrap();
                let mut target = npc.auras.remove(&ability).unwrap();
                target.health = npc.health;
                let events =
                    target.update_aura(self.state.now_ms, self.profile.tick_ms, facts.immune)?;
                npc.health = target.health;
                if target.aura().is_some() {
                    npc.auras.insert(ability, target);
                }
                self.spell_events(0, ability, target_id, events)?;
            }
        }
        Ok(())
    }
    fn npcs(&mut self) -> Result<(), NativeError> {
        let ids = self.state.npcs.keys().copied().collect::<Vec<_>>();
        let mut swings = Vec::new();
        for id in ids {
            let mut npc = self.state.npcs.remove(&id).unwrap();
            let p = self.profile.npc(id).unwrap();
            if npc.health == 0
                && !matches!(npc.ai.phase(), ai::Phase::Corpse | ai::Phase::Despawned)
            {
                // Damage receipts before death need a live authoritative target and are preserved.
                let pending = self.receipts.entry(id).or_default();
                pending.push(ai::Receipt {
                    id: npc
                        .ai
                        .last_receipt_id()
                        .checked_add(pending.len() as u64 + 1)
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
            let query = Query::Npc {
                position: npc.ai.position(),
                player_position: self.state.position,
                motion: npc.ai.motion().cloned(),
            };
            let Facts::Npc(facts) = self.ask(Some(npc.id), query)? else {
                return Err(NativeError::AuthorityBinding);
            };
            let movement = if let Some(motion) = npc.ai.motion().cloned().filter(|_| {
                !matches!(npc.ai.phase(), ai::Phase::Corpse | ai::Phase::Despawned)
                    && npc.health > 0
            }) {
                let goal = match motion.purpose {
                    MovementPurpose::Home => motion.home.ok_or(NativeError::InvalidState)?,
                    MovementPurpose::Chase { .. } => facts.chase_goal.ok_or(
                        NativeError::AuthorityUnavailable("native chase near-point/navigation"),
                    )?,
                };
                let request = MovementRequest {
                    request_id: npc
                        .ai
                        .last_movement_request_id()
                        .checked_add(1)
                        .ok_or(NativeError::Limit)?,
                    command_id: motion.command_id,
                    creature_id: id,
                    map_id: self.profile.map_id,
                    origin: npc.ai.position(),
                    goal,
                    speed: if motion.purpose == MovementPurpose::Home {
                        p.ai.home_speed
                    } else {
                        p.ai.run_speed
                    },
                    elapsed_ms: self.profile.tick_ms,
                    content_revision: Revision(self.profile.content_digest),
                    geometry_revision: Revision(self.profile.geometry_revision),
                    purpose: motion.purpose,
                };
                let Facts::Movement(outcome) =
                    self.ask(Some(npc.id), Query::Movement(request.clone()))?
                else {
                    return Err(NativeError::AuthorityBinding);
                };
                Some(ai::movement::resolve_movement(
                    &mut MovementAdapter(outcome),
                    request,
                )?)
            } else {
                None
            };
            let observation = ai::Observation {
                sequence: self.state.tick,
                elapsed_ms: self.profile.tick_ms,
                geometry_revision: Revision(self.profile.geometry_revision),
                target: Some(ai::Target {
                    id: SOLO_UNIT,
                    position: self.state.position,
                    level: self.profile.player.level,
                    combat_reach: self.profile.player.combat_reach,
                    bounding_radius: self.profile.player.bounding_radius,
                    moving_and_running: self.moved,
                    alive: self.state.health > 0,
                    online: true,
                    can_attack_in_combat: facts.can_attack_in_combat,
                    can_attack_on_sight: facts.can_attack_on_sight,
                    visible: facts.visible,
                    accessible: facts.accessible,
                    line_of_sight: facts.line_of_sight,
                }),
                can_initiate_attack: facts.can_initiate_attack,
                casting_non_melee: false,
                stationary_melee_refresh_eligible: facts.stationary_melee_refresh_eligible,
                creature_moving_and_running: facts.creature_moving_and_running,
                movement,
                receipts: self.receipts.remove(&id).unwrap_or_default(),
            };
            let mut melee_events = Vec::new();
            let effects = if self.state.combat.is_some() {
                let start = self.events.len();
                let mut failure = None;
                let npc_id = npc.id;
                let health = npc.health;
                let result = npc.ai.step_with_mainhand(
                    &mut ObservationAdapter(Some(observation)),
                    &mut |_, position, started| {
                        if started {
                            self.enter_player_combat();
                        }
                        match self.native_melee(npc_id, position, health) {
                            Ok(()) => Ok(()),
                            Err(error) => {
                                failure = Some(error);
                                Err(ai::AiError::Invalid("native melee callback"))
                            }
                        }
                    },
                );
                if let Some(error) = failure {
                    return Err(error);
                }
                melee_events = self.events.split_off(start);
                result?
            } else {
                npc.ai.step(&mut ObservationAdapter(Some(observation)))?
            };
            for effect in effects {
                match &effect {
                    ai::Effect::MainhandSwing { .. } if self.state.combat.is_none() => {
                        swings.push(npc.id)
                    }
                    ai::Effect::EvadeStarted(_) => {
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
                    ai::Effect::Respawned { .. } => {
                        npc.id.generation =
                            npc.id.generation.checked_add(1).ok_or(NativeError::Limit)?;
                        npc.health = p.max_health;
                        npc.auras.clear();
                    }
                    _ => {}
                }
                let started = matches!(effect, ai::Effect::CombatStarted { .. });
                let swung = matches!(effect, ai::Effect::MainhandSwing { .. });
                self.events.push(Event::Npc { id: npc.id, effect });
                if started {
                    if matches!(
                        melee_events.first(),
                        Some(Event::Combat(CombatEvent::EnteredCombat))
                    ) {
                        self.events.push(melee_events.remove(0));
                    } else {
                        self.enter_player_combat();
                    }
                }
                if swung {
                    self.events.append(&mut melee_events);
                }
            }
            self.state.npcs.insert(id, npc);
        }
        // Legacy synthetic fixed-outcome mode only: observations share pre-melee
        // actor state. Initialized mode already resolved callbacks inside AI. Legacy swings
        // apply in spawn order; a prior lethal swing suppresses later swing intents.
        // This explicit bounded phase ordering avoids validating earlier spell receipts
        // against a player killed later in the same frame.
        for id in swings {
            if self.state.health == 0 {
                break;
            }
            let health = self.state.npcs[&id.spawn].health;
            let Facts::Melee(outcome) = self.ask(
                Some(id),
                Query::Melee {
                    player_health: self.state.health,
                    npc_health: health,
                },
            )?
            else {
                return Err(NativeError::AuthorityBinding);
            };
            if outcome.damage > 1_000_000
                || outcome.school > 6
                || outcome.absorb > 1_000_000
                || outcome.blocked > 1_000_000
                || outcome.resist.unsigned_abs() > 1_000_000
            {
                return Err(NativeError::InvalidState);
            }
            let effective = self.state.health.min(outcome.damage);
            self.state.health -= effective;
            if effective > 0 && outcome.pushback {
                self.pushback()?;
            }
            self.events.push(Event::Melee {
                id,
                outcome,
                effective,
            });
        }
        if self.state.health == 0 {
            let ids = self.state.casts.keys().copied().collect::<Vec<_>>();
            for id in ids {
                let mut c = self.state.casts.remove(&id).unwrap();
                let events = c.execution.cancel(
                    &mut self.state.resources.caster,
                    spell::CastFailure::CasterDead,
                );
                self.spell_events(id, c.ability, c.target, events)?;
            }
        }
        Ok(())
    }
    fn player_reactives(&mut self) {
        if let Some(c) = self.state.combat.as_mut() {
            if c.defense_reactive_ms > 0 {
                c.defense_reactive_ms = c.defense_reactive_ms.saturating_sub(self.profile.tick_ms);
                if c.defense_reactive_ms == 0 {
                    c.defense_reactive_started_ms = None;
                    self.events
                        .push(Event::Combat(CombatEvent::DefenseReactive {
                            remaining_ms: 0,
                        }));
                }
            }
        }
    }
    fn enter_player_combat(&mut self) {
        if let Some(c) = self.state.combat.as_mut() {
            if !c.in_combat {
                c.in_combat = true;
                c.combat_tick_ms = 1200;
                self.events.push(Event::Combat(CombatEvent::EnteredCombat));
            }
        }
    }
    fn player_combat_update(&mut self) {
        let hostile = self
            .state
            .npcs
            .values()
            .any(|n| n.ai.phase() == ai::Phase::Combat);
        if let Some(c) = self.state.combat.as_mut() {
            if c.in_combat {
                c.combat_tick_ms += self.profile.tick_ms;
                if c.combat_tick_ms >= 1200 {
                    c.combat_tick_ms -= 1200;
                    if !hostile {
                        c.in_combat = false;
                        self.events.push(Event::Combat(CombatEvent::LeftCombat));
                    }
                }
            }
        }
    }
    fn native_melee(
        &mut self,
        id: NpcId,
        position: ai::Point,
        npc_health: u32,
    ) -> Result<(), NativeError> {
        use benilla_sim::{npc_melee as m, starter_init as init};
        let p = self
            .profile
            .npc(id.spawn)
            .ok_or(NativeError::UnknownTarget)?;
        let attack = p
            .combat
            .as_ref()
            .ok_or(NativeError::UnsupportedMechanic(
                "native creature damage fields",
            ))?
            .attacker(p.ai.level);
        let c = self
            .state
            .combat
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let action = c.next_action;
        // Binds every mutable defense/cast/resource field, identity, source generation and
        // geometry revision. An unchanged HP miss cannot authorize a repeated action.
        let cast_states = self
            .state
            .casts
            .iter()
            .map(|(id, c)| (*id, c.execution.checkpoint()))
            .collect::<Vec<_>>();
        let binding = crate::digest(
            b"benilla-native-melee-action-v2\0",
            &(
                self.state.world,
                self.state.player,
                self.state.tick,
                self.profile_digest,
                self.profile.geometry_revision,
                id,
                position,
                npc_health,
                self.state.position,
                self.state.health,
                self.state.resources.caster.checkpoint(),
                c.snapshot(),
                cast_states,
            ),
        );
        let Facts::NativeMelee(facts) = self.ask(
            Some(id),
            Query::NativeMelee {
                action,
                state_binding: binding,
                player_in_combat: c.in_combat,
                player_position: self.state.position,
                npc_position: position,
            },
        )?
        else {
            return Err(NativeError::AuthorityBinding);
        };
        let c = self.state.combat.as_ref().unwrap();
        let defense = c.skills.get(&combat::DEFENSE_SKILL);
        let plan = m::prepare(m::MeleeInput {
            attacker: attack,
            defender: m::MageDefense {
                level: self.profile.player.level,
                agility: u32::try_from(c.stats.attributes[1])
                    .map_err(|_| NativeError::InvalidState)?,
                defense_skill: defense.map(SkillState::current_value).unwrap_or(0),
                armor: c.stats.resistances[0],
                standing: c.standing,
                casting_non_melee: self
                    .state
                    .casts
                    .values()
                    .any(|c| matches!(c.execution.state(), spell::CastState::Casting { .. })),
                has_channeled_spell: c.has_channeled_spell,
                has_auto_repeat_spell: c.has_auto_repeat_spell,
            },
            health: self.state.health,
            front_facing: facts.front_facing,
        })?;
        let skill = defense.map(|s| m::DefenseSkill {
            value: s.value,
            maximum: s.maximum,
            gain: self.profile.combat.as_ref().unwrap().defense_gain,
        });
        // Establish unsupported skill callback before draws; even a miss can require it.
        let skill_chance = if !attack.creature_is_critter {
            skill
                .map(|s| m::defense_gain_chance(self.profile.player.level, s))
                .transpose()?
                .flatten()
        } else {
            None
        };
        let (low, high) = plan.damage_range();
        let damage = self.state.draw_range(
            0,
            DrawPurpose::MeleeDamage { action, npc: id },
            i64::from(low),
            i64::from(high),
            &mut self.draws,
        )? as u32;
        let outcome = self.state.draw(
            0,
            DrawPurpose::MeleeOutcome { action, npc: id },
            1,
            10000,
            &mut self.draws,
        )? as u16;
        let result = m::resolve(plan, m::MeleeRolls { damage, outcome })?;
        let callbacks = result.callbacks();
        if callbacks.killed {
            return Err(NativeError::UnsupportedMechanic(
                "Player::Kill: death procs, corpse, aura and resurrection lifecycle",
            ));
        }
        let (hit_info, victim_state) = match result.outcome() {
            m::MeleeOutcome::Miss => (2 | 0x10, 0),
            m::MeleeOutcome::Dodge => (2, 2),
            m::MeleeOutcome::Critical => (2 | 0x80, 1),
            m::MeleeOutcome::Crushing => (2 | 0x8000, 1),
            m::MeleeOutcome::Normal => (2, 1),
        };
        self.events.push(Event::Melee {
            id,
            outcome: MeleeOutcome {
                damage: result.total_damage(),
                hit_info,
                victim_state,
                blocked: 0,
                absorb: 0,
                resist: 0,
                school: 0,
                pushback: callbacks.interrupt_or_delay_spell,
            },
            effective: result.effective_damage(),
        });
        if callbacks.critical_wound_emote {
            self.events.push(Event::Combat(CombatEvent::CriticalWound));
        }
        if callbacks.stand_up {
            self.state.combat.as_mut().unwrap().standing = true;
            self.events.push(Event::Combat(CombatEvent::StoodUp));
        }
        m::apply_health(&mut self.state.health, result)?;
        if callbacks.surviving_damage_combat
            && facts.player_can_attack_npc
            && facts.player_can_enter_combat
            && facts.npc_can_enter_combat
        {
            self.enter_player_combat();
        }
        if callbacks.durability_hit_attempt {
            // Source rand_chance runs even when configured chance is exactly zero.
            let fraction = self.state.draw_range(
                0,
                DrawPurpose::DurabilityChance { action, npc: id },
                0,
                (1i64 << 53) - 1,
                &mut self.draws,
            )? as u64;
            if m::PercentRoll::from_fraction53(fraction)?
                .succeeds(self.profile.combat.as_ref().unwrap().durability_hit_chance)?
            {
                let slot = self.state.draw(
                    0,
                    DrawPurpose::DurabilitySlot { action, npc: id },
                    0,
                    18,
                    &mut self.draws,
                )? as u8;
                let mut c = self.state.combat.take().unwrap();
                let mut equipment = c.equipment.clone();
                let changes =
                    equipment.apply_hit_loss::<NativeError, _>(slot, |_, _, changes| {
                        for transition in &changes.stat_transitions {
                            let equipment::EquipmentStatTransition::Broken { slot, .. } =
                                *transition;
                            let live = init::Resources {
                                health: self.state.health,
                                max_health: c.stats.max_health,
                                mana: self.state.resources.caster.mana,
                                max_mana: c.stats.max_mana,
                            };
                            let result = c.apply_item_break(slot, live)?;
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
            }
        }
        if callbacks.interrupt_or_delay_spell {
            self.native_spell_damage()?;
        }
        // Post-health CanEnterCombat/AttackedBy applies even when the outcome was miss.
        let engage = callbacks.attacked_by_and_combat
            && facts.player_can_enter_combat
            && facts.npc_can_enter_combat;
        if engage {
            self.enter_player_combat();
        }
        if callbacks.defense_skill_attempt {
            if let Some(skill) = skill {
                let roll = if skill_chance.is_some() {
                    Some(m::PercentRoll::from_fraction53(self.state.draw_range(
                        0,
                        DrawPurpose::DefenseGain { action, npc: id },
                        0,
                        (1i64 << 53) - 1,
                        &mut self.draws,
                    )?
                        as u64)?)
                } else {
                    None
                };
                let value = m::resolve_defense_gain(self.profile.player.level, skill, roll)?;
                self.state
                    .combat
                    .as_mut()
                    .unwrap()
                    .skills
                    .get_mut(&combat::DEFENSE_SKILL)
                    .unwrap()
                    .value = value;
                if value != skill.value {
                    self.events.push(Event::Combat(CombatEvent::DefenseSkill {
                        before: skill.value,
                        after: value,
                    }));
                }
            }
        }
        if callbacks.defense_reactive {
            let c = self.state.combat.as_mut().unwrap();
            c.defense_reactive_ms = m::DEFENSE_REACTIVE_DURATION_MS;
            c.defense_reactive_started_ms = Some(self.state.now_ms);
            self.events
                .push(Event::Combat(CombatEvent::DefenseReactive {
                    remaining_ms: c.defense_reactive_ms,
                }));
        }
        let c = self.state.combat.as_mut().unwrap();
        c.next_action = c.next_action.checked_add(1).ok_or(NativeError::Limit)?;
        c.action_tip = crate::digest(
            b"benilla-native-melee-chain-v2\0",
            &(c.action_tip, action, binding, self.state.rng_counter),
        );
        self.events
            .push(Event::Combat(CombatEvent::ActionCommitted {
                action,
                binding,
                tip: c.action_tip,
            }));
        Ok(())
    }
    fn native_spell_damage(&mut self) -> Result<(), NativeError> {
        let cancels = &self.profile.combat.as_ref().unwrap().damage_cancels;
        let ids = self
            .state
            .casts
            .iter()
            .filter(|(_, c)| {
                matches!(c.execution.state(), spell::CastState::Casting { .. })
                    && cancels.contains(&c.ability)
            })
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for id in ids {
            let mut c = self.state.casts.remove(&id).unwrap();
            let events = c.execution.cancel(
                &mut self.state.resources.caster,
                spell::CastFailure::Interrupted,
            );
            self.spell_events(id, c.ability, c.target, events)?;
        }
        self.pushback()
    }

    fn pushback(&mut self) -> Result<(), NativeError> {
        let ids = self
            .state
            .casts
            .iter()
            .filter(|(_, c)| matches!(c.execution.state(), spell::CastState::Casting { .. }))
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for id in ids {
            let mut c = self.state.casts.remove(&id).unwrap();
            let eligible = matches!(&self.profile.ability(c.ability).unwrap().admission,AbilityAdmission::Available(s) if s.damage_pushback);
            if eligible {
                let die = self
                    .state
                    .draw(id, DrawPurpose::Pushback, 0, 99, &mut self.draws)?
                    as u8;
                let events = c.execution.damage_pushback(die)?;
                self.spell_events(id, c.ability, c.target, events)?;
            }
            self.state.casts.insert(id, c);
        }
        Ok(())
    }
    fn mana(&mut self) -> Result<(), NativeError> {
        let events = self
            .events
            .iter()
            .filter_map(|e| match e {
                Event::Spell { event, .. } => Some(*event),
                _ => None,
            })
            .collect();
        let result = self.state.resources.after_spell_and_aura_events(
            mana::ManaFrame {
                diff_ms: self.profile.tick_ms,
                in_world: true,
                alive_after_events: self.state.health > 0,
                world_mana_rate: self.profile.player.world_mana_rate,
                exclusions: self.profile.player.mana_exclusions.into(),
            },
            events,
        )?;
        if result.mana_before_regeneration != result.mana_after_regeneration {
            self.events.push(Event::Mana {
                before: result.mana_before_regeneration,
                after: result.mana_after_regeneration,
            });
        }
        Ok(())
    }
}
struct MovementAdapter(MovementOutcome);
impl GeometryAuthority for MovementAdapter {
    fn resolve(&mut self, _: &MovementRequest) -> Result<MovementOutcome, ai::AiError> {
        Ok(self.0.clone())
    }
}
struct ObservationAdapter(Option<ai::Observation>);
impl ai::CombatAuthority for ObservationAdapter {
    fn observe(&mut self, _: u64) -> Result<ai::Observation, ai::AiError> {
        self.0
            .take()
            .ok_or(ai::AiError::Invalid("duplicate observation"))
    }
}
