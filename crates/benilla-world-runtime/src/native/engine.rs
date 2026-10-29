// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::NpcId;
mod admin;
mod combat_assistance;
mod combat_authority;
mod death;
mod event_ai;
mod event_scheduler;
mod facing;
mod hostile_spell;
mod npc_spell;
mod player_environment;
mod player_slow;
mod source_location;
mod spell_authority;
mod spell_range;
use spell_authority::SpellHistory;
mod inventory;
mod loot;
mod mainhand;
mod motion;
mod quests;
#[cfg(test)]
mod update_phase_tests;
mod visitors;
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
        self.step_batch(tick, input.into_iter().collect(), authority)
    }
    /// All inputs validate and commit atomically. Authority callbacks are read-only proposals.
    /// Empty batches advance one update without consuming an input sequence.
    pub fn step_batch(
        &mut self,
        tick: u64,
        inputs: Vec<OwnedInput>,
        authority: &mut impl Authority,
    ) -> Result<&TickRecord, NativeError> {
        CreaturePoseCellIndex::validate(&self.profile, &self.state)?;
        initial_cast_genesis::validate_state(&self.profile, &self.state)?;
        player_melee::validate_weapon_history(&self.profile, &self.state)?;
        if tick != self.state.tick.checked_add(1).ok_or(NativeError::Limit)?
            || tick > crate::MAX_TICK
        {
            return Err(NativeError::InvalidInput);
        }
        if self.journal.len() >= MAX_RECORDS {
            return Err(NativeError::JournalFull);
        }
        if inputs.len() > MAX_INPUTS_PER_TICK {
            return Err(NativeError::TooLarge);
        }
        let mut sequence = self.state.next_sequence;
        let mut cast_inputs = BTreeMap::new();
        for owned in &inputs {
            let i = &owned.input;
            if owned.authenticated_owner != self.state.player.owner
                || i.player != self.state.player
                || i.world != self.state.world
                || i.tick != tick
                || i.sequence != sequence
            {
                return Err(NativeError::InvalidInput);
            }
            if matches!(i.command, Command::AcknowledgeInitialReputations)
                && sequence != self.state.next_sequence
            {
                return Err(NativeError::InvalidInput);
            }
            if let Command::CancelCast {
                target: CancelCastTarget::PriorInput(prior),
                ..
            } = &i.command
            {
                if !cast_inputs.contains_key(prior) {
                    return Err(NativeError::InvalidInput);
                }
            }
            if let Command::CancelCast {
                target: CancelCastTarget::Instance(id),
                ..
            } = &i.command
            {
                // A session-captured instance cannot name a future cast. New casts in this
                // frame must use their preceding input reference, including rejected prepares.
                if *id >= self.state.next_cast_id {
                    return Err(NativeError::InvalidInput);
                }
            }
            if matches!(i.command, Command::Cast { .. }) {
                cast_inputs.insert(sequence, ());
            }
            sequence = sequence.checked_add(1).ok_or(NativeError::Limit)?;
        }
        let inputs: Vec<_> = inputs.into_iter().map(|owned| owned.input).collect();
        if serde_json::to_vec(&inputs)
            .map_err(|_| NativeError::InvalidInput)?
            .len()
            > MAX_INPUT_BYTES
        {
            return Err(NativeError::TooLarge);
        }
        super::source_location::validate_current(&self.profile, &self.state)?;
        melee_reset::validate_current(&self.profile, &self.state)?;
        let before = self.state_digest();
        let mut next = self.state.clone();
        next.tick = tick;
        next.now_ms = next
            .now_ms
            .checked_add(u64::from(self.profile.tick_ms))
            .ok_or(NativeError::Limit)?;
        let mut frame = Frame {
            read_only: false,
            profile: &self.profile,
            profile_digest: self.profile_digest,
            state: &mut next,
            authority,
            replies: vec![],
            events: vec![],
            draws: vec![],
            receipts: BTreeMap::new(),
            source_alive_unit: None,
            moved: false,
            cast_inputs: BTreeMap::new(),
            movement_origin: self.state.position,
            movement_path: Vec::new(),
            player_auras_updated: false,
            player_melee_unit_updated: false,
            pending_player_melee_death: None,
        };
        frame.source_location(frame.state.position, frame.state.orientation)?;
        for i in &inputs {
            let event_start = frame.events.len();
            let next_cast = frame.state.next_cast_id;
            frame.command(&i.command)?;
            if matches!(i.command, Command::Cast { .. }) {
                let created = (frame.state.next_cast_id != next_cast).then_some(next_cast);
                frame.cast_inputs.insert(i.sequence, created);
            }
            let accepted = !frame.events[event_start..]
                .iter()
                .any(|e| matches!(e, Event::Rejected(_)));
            frame.events.insert(
                event_start,
                Event::InputResult {
                    sequence: i.sequence,
                    accepted,
                },
            );
            frame.state.next_sequence = frame
                .state
                .next_sequence
                .checked_add(1)
                .ok_or(NativeError::Limit)?;
        }
        frame.player_event_update()?;
        #[cfg(any(test, feature = "synthetic-native-tests"))]
        frame.synthetic_player_slow_effects(false)?;
        frame.player_slow_update()?;
        #[cfg(any(test, feature = "synthetic-native-tests"))]
        frame.synthetic_player_slow_effects(true)?;
        if frame.profile.source_motion.is_none() {
            let ids = frame.state.npcs.keys().copied().collect::<Vec<_>>();
            for id in ids {
                let mut npc = frame.state.npcs.remove(&id).unwrap();
                frame.npc_event_update(&mut npc)?;
                frame.state.npcs.insert(id, npc);
            }
            frame.auras()?;
        }
        frame.finish_player_melee_death()?;
        if let Some(combat) = &mut frame.state.combat {
            combat.advance_melee_timers(frame.profile.tick_ms);
        }
        frame.player_melee_unit_updated = true;
        frame.player_reactives();
        frame.player_environment_update()?;
        frame.player_death_update()?;
        frame.finish_player_melee_death()?;
        frame.player_mainhand_update()?;
        #[cfg(any(test, feature = "synthetic-native-tests"))]
        frame.synthetic_npc_spell_starts(false)?;
        frame.npcs()?;
        frame.finish_player_melee_death()?;
        #[cfg(any(test, feature = "synthetic-native-tests"))]
        frame.synthetic_npc_spell_starts(true)?;
        frame.player_combat_update();
        frame.mana()?;
        frame.sync_quest_progression()?;
        if let (Some(g), Some(q)) = (
            &frame.profile.quest_progression,
            &mut frame.state.quest_progression,
        ) {
            q.played_time(g, frame.state.now_ms)?;
        }
        if frame
            .state
            .rng_counter
            .checked_sub(self.state.rng_counter)
            .ok_or(NativeError::InvalidState)?
            > MAX_RNG_WORDS_PER_TICK
        {
            return Err(NativeError::Limit);
        }
        if frame.pending_player_melee_death.is_some() {
            return Err(NativeError::InvalidState);
        }
        melee_reset::validate_current(frame.profile, frame.state)?;
        // Committed-frame pose observations only. This does not perform source
        // CurrentCell relocation, grid creation, or object registration.
        frame.state.creature_pose_cells =
            CreaturePoseCellIndex::capture(frame.profile, frame.state)?;
        let replies = frame.replies;
        let events = frame.events;
        let draws = frame.draws;
        SourceSplineIds::validate_events(&self.state, &next, &events)?;
        let after = crate::digest(b"benilla-native-state-v81\0", &self.snapshot_of(&next));
        let record = TickRecord {
            version: NATIVE_FORMAT_VERSION,
            profile: self.profile_digest,
            tick,
            inputs,
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
        let bytes = journal_size(size, self.journal_bytes, !self.journal.is_empty())?;
        let tip = record.digest();
        self.make_checkpoint(&next, tip)?;
        self.state = next;
        self.journal_tip = tip;
        self.journal.push(record);
        self.journal_bytes = bytes;
        Ok(self.journal.last().unwrap())
    }
}
fn journal_size(record: usize, journal: usize, nonempty: bool) -> Result<usize, NativeError> {
    if record.checked_add(2).ok_or(NativeError::TooLarge)? > MAX_BYTES {
        return Err(NativeError::TooLarge);
    }
    let bytes = journal
        .checked_add(record + usize::from(nonempty))
        .ok_or(NativeError::TooLarge)?;
    if bytes > MAX_BYTES {
        return Err(if nonempty {
            NativeError::JournalFull
        } else {
            NativeError::TooLarge
        });
    }
    Ok(bytes)
}

struct Frame<'a, A> {
    read_only: bool,
    profile: &'a Profile,
    profile_digest: crate::Digest,
    state: &'a mut State,
    authority: &'a mut A,
    replies: Vec<AuthorityReply>,
    events: Vec<Event>,
    draws: Vec<Draw>,
    receipts: BTreeMap<u64, Vec<ai::Receipt>>,
    source_alive_unit: Option<NpcId>,
    moved: bool,
    cast_inputs: BTreeMap<u64, Option<u64>>,
    movement_origin: ai::Point,
    movement_path: Vec<ai::Point>,
    player_auras_updated: bool,
    player_melee_unit_updated: bool,
    pending_player_melee_death: Option<player_melee::PendingPlayerMeleeDeath>,
}
impl<A: Authority> Frame<'_, A> {
    fn interaction_moment(&self) -> InteractionMoment {
        if self.read_only {
            InteractionMoment::ReadOnly
        } else {
            InteractionMoment::Frame {
                action_ordinal: self.replies.len() as u32,
            }
        }
    }
    fn interaction_witness(&self) -> Result<InteractionWitness, NativeError> {
        interactions::witness(self.profile_digest, self.state, self.interaction_moment())
    }
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
        // This request was assembled in this frame from the executing callback.
        // Never admit an external pose/request as a current-state authority input.
        let derived = match self
            .spell_range_facts(&request)?
            .or(self.facing_facts(&request)?)
        {
            Some(facts) => Some(facts),
            None => interactions::resolve(
                self.profile,
                self.profile_digest,
                self.state,
                &request,
                self.interaction_moment(),
            )?,
        };
        let reply = if let Some(facts) = derived {
            let reply = AuthorityReply {
                request: request.clone(),
                facts,
            };
            self.authority.observe_derived(&reply)?;
            reply
        } else {
            self.authority.resolve(&request)?
        };
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
        history: SpellHistory<'_>,
    ) -> Result<SpellFacts, NativeError> {
        if interactions::spell_authority::selected(self.profile) {
            let phase = match phase {
                CastPhase::Prepare => SourceSpellPhase::Prepare,
                CastPhase::Release => SourceSpellPhase::ReleaseCheck,
                CastPhase::Impact => SourceSpellPhase::Impact,
                CastPhase::Aura => SourceSpellPhase::Aura,
            };
            return self.source_spell_facts(cast_id, ability, target, phase, history);
        }
        let n = self
            .state
            .npcs
            .get(&target.spawn)
            .filter(|n| n.id == target)
            .ok_or(NativeError::UnknownTarget)?;
        // Release HitResult uses EVADE, while later direct impacts use
        // DealDamageMods absorb. Neither is an external allowed/immune claim.
        if self.profile.source_motion.is_some()
            && n.ai.is_evading()
            && matches!(phase, CastPhase::Release | CastPhase::Impact)
        {
            return Err(NativeError::UnsupportedMechanic(
                "source projectile release/impact during evade",
            ));
        }
        let q = Query::Spell {
            cast_id,
            ability,
            phase,
            player_orientation: self.state.orientation,
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
        scheduler: Option<&benilla_sim::event_processor::EventCallbackSnapshot>,
    ) -> Result<(), NativeError> {
        for event in events {
            if matches!(&event,spell::SpellEvent::DirectDamage{effective,..}|spell::SpellEvent::PeriodicDamage{effective,..} if *effective>0)
            {
                let n = self
                    .state
                    .npcs
                    .get_mut(&target.spawn)
                    .filter(|n| n.id == target)
                    .ok_or(NativeError::InvalidState)?;
                if n.original_loot_recipient.is_none() {
                    n.original_loot_recipient = Some(self.state.player);
                }
            }
            // ThreatManager::addThreat (pinned 8ec338a1, lines 446-463) rejects
            // a dead threat owner or dead hated caster; DOT damage itself persists.
            let can_add_threat = self.state.player_alive()
                && self
                    .state
                    .npcs
                    .get(&target.spawn)
                    .is_some_and(|n| n.id == target && n.health > 0);
            let source_threat = if can_add_threat
                && self.profile.source_motion.is_some()
                && matches!(event, spell::SpellEvent::DirectDamage{effective,..}|spell::SpellEvent::PeriodicDamage{effective,..} if effective > 0)
            {
                self.source_damage_threat_allowed(cast_id, ability, target)?
            } else {
                true
            };
            match event {
                spell::SpellEvent::DirectDamage { effective, .. }
                    if effective > 0 && can_add_threat && source_threat =>
                {
                    if self.profile.source_motion.is_none() {
                        self.receipt(target, ai::CombatEvent::AttackedBy { target: SOLO_UNIT })?;
                    }
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
                    if effective > 0 && can_add_threat && source_threat =>
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
            let killed = matches!(&event,spell::SpellEvent::DirectDamage{effective,..}|spell::SpellEvent::PeriodicDamage{effective,..} if *effective>0)
                && self
                    .state
                    .npcs
                    .get(&target.spawn)
                    .is_some_and(|n| n.id == target && n.health == 0);
            self.events.push(Event::Spell {
                cast_id,
                ability,
                target,
                event,
            });
            if killed {
                if self.profile.creature_generations.is_some() {
                    // Unit::Kill prepares rewards/body loot before JUST_DIED's respawn draw.
                    self.native_kill_rewards(target)?;
                    self.prepare_body_loot(target)?;
                    if self.profile.source_motion.is_some() {
                        self.source_combat_receipt(target, ai::CombatEvent::Died)?;
                    }
                } else {
                    if self.profile.source_motion.is_some() {
                        self.source_combat_receipt(target, ai::CombatEvent::Died)?;
                    }
                    self.native_kill_rewards(target)?;
                    self.prepare_body_loot(target)?;
                }
                self.events
                    .extend(player_melee::target_death::after_spell_kill(
                        self.profile,
                        self.state,
                        target,
                        self.player_melee_unit_updated,
                        scheduler,
                    )?);
            }
        }
        Ok(())
    }
    pub(super) fn creature_death_timing(
        &mut self,
        npc: &mut Npc,
        receipt: &ai::Receipt,
    ) -> Result<Option<ai::DeathTiming>, NativeError> {
        if !matches!(receipt.event, ai::CombatEvent::Died) {
            return Ok(None);
        }
        match (&self.profile.creature_generations, &npc.source_generation) {
            (None, None) => Ok(None),
            (Some(catalog), Some(owner)) => {
                if matches!(npc.ai.phase(), ai::Phase::Corpse | ai::Phase::Despawned) {
                    return Err(NativeError::InvalidState);
                }
                let (next, draws) = owner.redraw_death_delay(
                    catalog,
                    self.state.tick,
                    self.state.seed,
                    &mut self.state.rng_counter,
                )?;
                let delay = next
                    .project_profile(
                        self.profile
                            .npc(npc.id.spawn)
                            .ok_or(NativeError::InvalidState)?,
                    )?
                    .ai
                    .respawn_delay_seconds;
                npc.source_generation = Some(next);
                self.draws.extend(draws);
                Ok(Some(ai::DeathTiming {
                    receipt_id: receipt.id,
                    at_ms: self.state.now_ms,
                    respawn_delay_seconds: delay,
                }))
            }
            _ => Err(NativeError::AuthorityBinding),
        }
    }
    fn receipt(&mut self, target: NpcId, event: ai::CombatEvent) -> Result<(), NativeError> {
        if self.profile.source_motion.is_some() {
            return self.source_combat_receipt(target, event);
        }
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

    fn player_hit_durability(
        &mut self,
        chance_purpose: DrawPurpose,
        slot_purpose: DrawPurpose,
    ) -> Result<(), NativeError> {
        use benilla_sim::{npc_melee as m, starter_init as init};
        // Source rand_chance runs even when configured chance is exactly zero.
        let fraction =
            self.state
                .draw_range(0, chance_purpose, 0, (1i64 << 53) - 1, &mut self.draws)?
                as u64;
        if m::PercentRoll::from_fraction53(fraction)?
            .succeeds(self.profile.combat.as_ref().unwrap().durability_hit_chance)?
        {
            let slot = self.state.draw(0, slot_purpose, 0, 18, &mut self.draws)? as u8;
            let mut c = self.state.combat.take().unwrap();
            let mut equipment = c.equipment.clone();
            let changes = equipment.apply_hit_loss::<NativeError, _>(slot, |_, _, changes| {
                for transition in &changes.stat_transitions {
                    let equipment::EquipmentStatTransition::Broken { slot, .. } = *transition;
                    let live = init::Resources {
                        health: self.state.health,
                        max_health: c.stats.max_health,
                        mana: self.state.resources.caster.mana,
                        max_mana: c.stats.max_mana,
                    };
                    let result = self.progression_item_break(&mut c, slot, live)?;
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
        Ok(())
    }
    fn command(&mut self, command: &Command) -> Result<(), NativeError> {
        let rejection = match command {
            Command::AttackStart { target } => {
                let (events, rejection) = player_melee::start(self.profile, self.state, *target)?;
                self.events.extend(events);
                rejection
            }
            Command::AttackStop => {
                self.events
                    .extend(player_melee::stop(self.profile, self.state)?);
                None
            }
            Command::InventoryMove(request) => self.inventory_move(request)?,
            Command::EquipmentMove(request) => self.equipment_move(request)?,
            Command::EquipmentAutoEquip { source } => self.equipment_auto_equip(*source)?,
            Command::InventorySplit(request) => self.inventory_split(request)?,
            Command::InventoryAutoStore(request) => self.inventory_autostore(request)?,
            Command::InventoryDestroy(request) => self.inventory_destroy(request)?,
            Command::Admin(request) => self.admin_command(request)?,
            Command::AcknowledgeInitialReputations => {
                let g = self
                    .profile
                    .quest_progression
                    .as_ref()
                    .ok_or(NativeError::InvalidState)?;
                self.state
                    .quest_progression
                    .as_mut()
                    .ok_or(NativeError::InvalidState)?
                    .acknowledge_initial_reputations(g)?;
                None
            }
            Command::ToggleHelm | Command::ToggleCloak => {
                player_hide_flags::validate_profile(self.profile)?;
                self.state.player_hide_flags.toggle(
                    matches!(command, Command::ToggleHelm),
                    self.state.next_sequence,
                )?;
                None
            }
            Command::SetSheathed { state } => {
                // The source rejects the raw uint32 before narrowing or callbacks.
                if *state < 3 {
                    player_sheath::validate_profile(self.profile)?;
                    self.state
                        .player_sheath
                        .write(*state as u8, self.state.next_sequence)?;
                }
                None
            }
            Command::SetActionBarToggles { toggles } => {
                action_bar_toggles::validate_profile(self.profile)?;
                self.state
                    .action_bar_toggles
                    .write(*toggles, self.state.next_sequence)?;
                None
            }
            Command::SetWatchedFaction { list_id } => {
                self.state
                    .watched_faction
                    .write(*list_id, self.state.next_sequence)?;
                None
            }
            Command::SetFactionAtWar { list_id, at_war } => {
                let g = self
                    .profile
                    .quest_progression
                    .as_ref()
                    .ok_or(NativeError::InvalidState)?;
                self.state
                    .quest_progression
                    .as_mut()
                    .ok_or(NativeError::InvalidState)?
                    .reputation_control(
                        g,
                        benilla_sim::reputation::Command::AtWar {
                            list_id: *list_id,
                            on: *at_war,
                        },
                    )?;
                None
            }
            Command::SetFactionInactive { list_id, inactive } => {
                let g = self
                    .profile
                    .quest_progression
                    .as_ref()
                    .ok_or(NativeError::InvalidState)?;
                self.state
                    .quest_progression
                    .as_mut()
                    .ok_or(NativeError::InvalidState)?
                    .reputation_control(
                        g,
                        benilla_sim::reputation::Command::Inactive {
                            list_id: *list_id,
                            on: *inactive,
                        },
                    )?;
                None
            }
            Command::ChooseQuestReward {
                quest,
                giver,
                choice,
            } => self.quest_command(*quest, *giver, true, *choice)?,
            Command::OpenLoot { corpse } => self.open_loot(*corpse)?,
            Command::LootItem { corpse, slot } => self.collect_item(*corpse, *slot)?,
            Command::LootMoney { corpse } => self.collect_money(*corpse)?,
            Command::ReleaseLoot { corpse } => self.release_loot(*corpse)?,
            Command::AcceptQuest { quest, giver } => {
                self.quest_command(*quest, *giver, false, 0)?
            }
            Command::TurnInQuest { quest, giver } => self.quest_command(*quest, *giver, true, 0)?,
            Command::AbandonQuest { slot } => self.abandon_quest(*slot)?,
            Command::ReleaseSpirit => Some(Rejection::SpiritReleaseUnavailable {
                phase: self
                    .state
                    .combat
                    .as_ref()
                    .and_then(|c| c.death.as_ref())
                    .map(|d| d.phase()),
            }),
            Command::Cast { ability, target } => {
                player_melee::validate_generic_concurrency(self.profile, self.state)?;
                let Some(a) = self.state.ability(self.profile, *ability) else {
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
                if !self.state.player_alive() {
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
                    let binding = ExecutionBinding::capture(self.profile, self.state, *ability)?;
                    let id = self.state.next_cast_id;
                    self.state.player_events.prepare(id, self.state.now_ms)?;
                    self.state.next_cast_id = id.checked_add(1).ok_or(NativeError::Limit)?;
                    let facts = self.facts(
                        id,
                        *ability,
                        *target,
                        CastPhase::Prepare,
                        SpellHistory::Prepare(&binding),
                    )?;
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
                    self.spell_events(id, *ability, *target, events, None)?;
                    if failed.is_none() {
                        self.state.player_events.current_spell = Some(id);
                        self.state.player_events.last_current_spell = Some(id);
                        self.state.casts.insert(
                            id,
                            Cast {
                                binding,
                                release_binding: None,
                                id,
                                ability: *ability,
                                target: *target,
                                origin: self.state.position,
                                execution,
                            },
                        );
                    }
                    if failed.is_some() {
                        self.state.player_events.finish(
                            id,
                            self.state.now_ms,
                            FinishedSpellReason::PrepareFailed,
                        )?;
                    }
                    failed.map(Rejection::CastFailed)
                }
            }
            Command::CancelCast { spell_id, target } => {
                let id = match target {
                    CancelCastTarget::Instance(id) => Some(*id),
                    CancelCastTarget::PriorInput(sequence) => {
                        self.cast_inputs.get(sequence).copied().flatten()
                    }
                };
                let current = id.filter(|id| {
                    self.state.casts.get(id).is_some_and(|c| {
                        (*spell_id == 0 || c.ability == *spell_id)
                            && matches!(c.execution.state(), spell::CastState::Casting { .. })
                    })
                });
                if let Some(id) = current {
                    self.interrupt_scheduled_cast(id, spell::CastFailure::Interrupted)?;
                }
                None
            }
            Command::MovementPose {
                destination,
                orientation,
                movement_flags,
            } => {
                if self.profile.source_motion.is_none() {
                    return Err(NativeError::InvalidInput);
                }
                let rejection = self.pose(*destination, *orientation, Some(*movement_flags))?;
                if rejection.is_none() {
                    self.state.player_movement_flags = Some(*movement_flags);
                    self.events.push(Event::PlayerMovementState {
                        movement_flags: *movement_flags,
                    });
                }
                rejection
            }
            Command::Move { destination } => {
                if self.profile.source_motion.is_some() {
                    return Err(NativeError::InvalidInput);
                }
                self.pose(*destination, self.state.orientation, None)?
            }
            Command::Pose {
                destination,
                orientation,
            } => {
                if self.profile.source_motion.is_some() {
                    return Err(NativeError::InvalidInput);
                }
                self.pose(*destination, *orientation, None)?
            }
        };
        if let Some(r) = rejection {
            self.events.push(Event::Rejected(r));
        }
        Ok(())
    }
    fn pose(
        &mut self,
        destination: ai::Point,
        orientation: Orientation,
        movement_flags: Option<benilla_sim::creature_motion::GroundMovementFlags>,
    ) -> Result<Option<Rejection>, NativeError> {
        destination.distance_squared(destination)?;
        if !self.state.player_alive() {
            return Ok(Some(Rejection::Dead));
        }
        if destination != self.state.position {
            let Facts::PlayerMovement { accepted } = self.ask(
                None,
                Query::PlayerMovement {
                    speed: self
                        .state
                        .player_slow
                        .as_ref()
                        .map(|slow| {
                            slow.movement_speed(
                                movement_flags
                                    .or(self.state.player_movement_flags)
                                    .ok_or(NativeError::InvalidState)?,
                            )
                        })
                        .transpose()?,
                    frame_origin: self.movement_origin,
                    accepted_path: self.movement_path.clone(),
                    destination,
                    elapsed_ms: self.profile.tick_ms,
                },
            )?
            else {
                return Err(NativeError::AuthorityBinding);
            };
            if !accepted {
                return Ok(Some(Rejection::MovementBlocked));
            }
            self.moved = true;
            self.movement_path.push(destination);
        }
        self.source_location(destination, orientation)?;
        // Pure turns do not fabricate travelled distance, interrupt movement-sensitive
        // casts, append path segments or refresh the frame's immutable elapsed budget.
        if destination != self.state.position || orientation != self.state.orientation {
            if let Some(policy) = &self.profile.source_motion {
                self.state
                    .player_events
                    .schedule_notify(policy.relocation_aggro_delay_ms, false)?;
            }
        }
        self.state.position = destination;
        self.state.orientation = orientation;
        self.events.push(Event::PlayerPose {
            position: destination,
            orientation,
        });
        self.player_environment_position()?;
        Ok(None)
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
    fn casts_one(
        &mut self,
        id: u64,
        diff_ms: u32,
        allow_delayed: bool,
        scheduler: &benilla_sim::event_processor::EventCallbackSnapshot,
    ) -> Result<(spell::CastState, bool), NativeError> {
        let mut c = self
            .state
            .casts
            .remove(&id)
            .ok_or(NativeError::InvalidState)?;
        let exists = self
            .state
            .npcs
            .get(&c.target.spawn)
            .is_some_and(|n| n.id == c.target && n.ai.phase() != ai::Phase::Despawned);
        let alive = exists && self.state.npcs[&c.target.spawn].health > 0;
        let moved = c.origin != self.state.position;
        let spec = c.binding.spec()?.clone();
        let mut checks = spell::CastChecks {
            caster_alive: self.state.player_alive(),
            target_exists: exists,
            target_alive: alive,
            position_changed: moved,
            other_failure: None,
        };
        let mut release_binding = None;
        let mut release_context = None;
        let launch = if c.execution.release_due(self.state.now_ms, diff_ms)
            && exists
            && alive
            && !(moved && spec.movement_interrupts)
        {
            let binding = ExecutionBinding::capture(self.profile, self.state, c.ability)?;
            let context = binding.release_context(&c.binding)?;
            let facts = self.facts(
                id,
                c.ability,
                c.target,
                CastPhase::Release,
                SpellHistory::Cast(&c, &binding, scheduler),
            )?;
            checks.other_failure = facts.other_failure;
            release_context = Some(context);
            let launch = if c
                .execution
                .release_failure_with_context(
                    &self.state.resources.caster,
                    checks,
                    self.state.now_ms,
                    context,
                )?
                .is_none()
            {
                let facts = if interactions::spell_authority::selected(self.profile) {
                    self.source_spell_facts(
                        id,
                        c.ability,
                        c.target,
                        SourceSpellPhase::ReleaseHit,
                        SpellHistory::Cast(&c, &binding, scheduler),
                    )?
                } else {
                    facts
                };
                let distance = self
                    .state
                    .position
                    .distance_squared(self.state.npcs[&c.target.spawn].ai.position())?;
                Some(spell::LaunchContext {
                    distance_squared: spell::SpellFloat::from_bits(distance.bits())?,
                    resist_chance: facts.resist_chance,
                    critical_chance: facts.critical_chance,
                    rolls: self.rolls(id, binding.spec()?, &facts)?,
                })
            } else {
                None
            };
            release_binding = Some(binding);
            launch
        } else {
            None
        };
        let before_execution = c.execution.checkpoint();
        let events = c.execution.update_from_event_with_release(
            self.state.now_ms,
            diff_ms,
            &mut self.state.resources.caster,
            checks,
            launch,
            release_context,
        )?;
        if events
            .iter()
            .any(|event| matches!(event, spell::SpellEvent::Launched { .. }))
        {
            c.release_binding = Some(release_binding.ok_or(NativeError::InvalidState)?);
            if interactions::spell_authority::resets_melee_timers(self.profile, c.ability)? {
                let genesis = self
                    .profile
                    .combat
                    .as_ref()
                    .ok_or(NativeError::InvalidState)?;
                let combat = self
                    .state
                    .combat
                    .as_mut()
                    .ok_or(NativeError::InvalidState)?;
                let intervals = if self.profile.player_melee.is_some() {
                    combat.reset_selected_melee_timer(genesis)?
                } else {
                    combat.reset_melee_timers(genesis)?
                };
                let witness = melee_reset::MeleeResetWitness::capture(
                    self.profile,
                    self.state,
                    &c,
                    before_execution,
                    self.replies.iter().rev().find(|reply| matches!(&reply.request.query, Query::SourceSpell(SourceSpellQuery::Semantics(context)) if context.cast_id == id && context.phase == SourceSpellPhase::ReleaseHit)).ok_or(NativeError::InvalidState)?,
                    intervals,
                )?;
                self.state
                    .combat
                    .as_mut()
                    .ok_or(NativeError::InvalidState)?
                    .melee_reset = Some(witness);
                player_melee::record_spell_reset(self.profile, self.state, scheduler)?;
            }
        }
        self.spell_events(id, c.ability, c.target, events, Some(scheduler))?;
        let after_update = c.execution.state();
        let handled_delayed = allow_delayed
            && matches!(c.execution.state(),spell::CastState::Traveling{impact_at_ms} if impact_at_ms<=self.state.now_ms);
        if handled_delayed {
            let impact_binding = ExecutionBinding::capture(self.profile, self.state, c.ability)?;
            let impact_spec = impact_binding.spec()?.clone();
            let facts = self.facts(
                id,
                c.ability,
                c.target,
                CastPhase::Impact,
                SpellHistory::Cast(&c, &impact_binding, scheduler),
            )?;
            let hit = c
                .execution
                .launch_context()
                .is_some_and(|l| l.rolls.hit > l.resist_chance);
            // Spell.cpp:1451 ExecuteEffects (HandleEffect:4597 calculates aura
            // values) precedes CasterHitTargetWithSpell:1454. Consume only these
            // impact-time value draws here; damage and aura installation remain
            // after the callback, which still observes pre-impact target health.
            let mut aura_rolls = [None; 3];
            if hit && facts.allowed && !facts.immune && self.state.npcs[&c.target.spawn].health > 0
            {
                for (slot, effect) in impact_spec.effects.iter().enumerate() {
                    if let spell::MageEffect::PeriodicDamage { value, .. } = effect {
                        if let Some((lo, hi)) = value.roll_range(impact_spec.spell_rank)? {
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
            if self.profile.source_motion.is_some() && self.state.npcs[&c.target.spawn].health > 0 {
                // A resisted launch calls the failed-hostile-hit branch independently
                // of effective damage. A delayed successful hit rejected by impact
                // visibility/attack or immunity returns before the source callback.
                if !hit || (facts.allowed && !facts.immune) {
                    self.source_hostile_spell_hit(id, c.ability, c.target, hit, &impact_spec)?;
                }
            }
            let npc = self.state.npcs.get_mut(&c.target.spawn).unwrap();
            let mut target = npc
                .auras
                .remove(&c.ability)
                .unwrap_or(spell::DamageTarget::new(c.target.spawn, npc.health)?);
            target.health = npc.health;
            let events = c.execution.impact_at_rank(
                impact_spec.spell_rank,
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
                if events
                    .iter()
                    .any(|e| matches!(e, spell::SpellEvent::AuraApplied { .. }))
                {
                    npc.aura_bindings.insert(c.ability, impact_binding);
                }
            } else {
                npc.aura_bindings.remove(&c.ability);
            }
            self.spell_events(id, c.ability, c.target, events, Some(scheduler))?;
        }
        if matches!(
            c.execution.state(),
            spell::CastState::Casting { .. } | spell::CastState::Traveling { .. }
        ) {
            self.state.casts.insert(id, c);
        }
        Ok((after_update, handled_delayed))
    }
    fn auras(&mut self) -> Result<(), NativeError> {
        let ids = self
            .state
            .npcs
            .values()
            .map(|npc| npc.id)
            .collect::<Vec<_>>();
        for id in ids {
            self.npc_auras(id)?;
        }
        Ok(())
    }
    // Unit::_UpdateSpells advances only its own holders after its event queue.
    fn npc_auras(&mut self, id: NpcId) -> Result<(), NativeError> {
        let spells = self
            .state
            .npcs
            .get(&id.spawn)
            .filter(|npc| npc.id == id)
            .ok_or(NativeError::UnknownTarget)?
            .auras
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for ability in spells {
            // Synchronous source death can remove all remaining holders.
            if !self.state.npcs[&id.spawn].auras.contains_key(&ability) {
                continue;
            }
            let facts = self.facts(0, ability, id, CastPhase::Aura, SpellHistory::Aura)?;
            let npc = self.state.npcs.get_mut(&id.spawn).unwrap();
            let mut target = npc.auras.remove(&ability).unwrap();
            target.health = npc.health;
            // DealDamageMods absorbs the due tick throughout timed/home evade;
            // its aura timer continues until the ordinary EnterEvadeMode cleanup.
            let events = target.update_aura_with_evade(
                self.state.now_ms,
                self.profile.tick_ms,
                facts.immune,
                npc.ai.is_evading(),
            )?;
            npc.health = target.health;
            if target.aura().is_some() {
                npc.auras.insert(ability, target);
            } else {
                npc.aura_bindings.remove(&ability);
            }
            self.spell_events(0, ability, id, events, None)?;
        }
        Ok(())
    }
    fn npcs(&mut self) -> Result<(), NativeError> {
        // Source owners complete events through regeneration in one turn. The
        // selected authored order does not claim Map's unordered object schedule.
        let source = self.profile.source_motion.is_some();
        let mut prepared = if source {
            BTreeMap::new()
        } else {
            self.prepare_npc_receipts()?
        };
        let ids = if let Some(visitors) = &self.profile.source_visitors {
            visitors
                .owner_order()
                .iter()
                .map(|spawn| {
                    self.state
                        .npcs
                        .get(spawn)
                        .map(|npc| npc.id)
                        .ok_or(NativeError::InvalidState)
                })
                .collect::<Result<Vec<_>, _>>()?
        } else {
            self.state
                .npcs
                .values()
                .map(|npc| npc.id)
                .collect::<Vec<_>>()
        };
        let mut swings = Vec::new();
        for id in ids {
            if source {
                self.source_npc_update(id, &mut swings)?;
            } else {
                self.npc_update(id, prepared.remove(&id.spawn), &mut swings)?;
            }
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
        if self.state.health == 0 && !self.death_enabled() {
            let ids = self.state.casts.keys().copied().collect::<Vec<_>>();
            for id in ids {
                self.interrupt_scheduled_cast(id, spell::CastFailure::CasterDead)?;
            }
        }
        Ok(())
    }
    fn npc_update(
        &mut self,
        owner: NpcId,
        receipt_step: Option<(ai::ReceiptStep, Option<ai::MotionCommand>)>,
        swings: &mut Vec<NpcId>,
    ) -> Result<(), NativeError> {
        let mut npc = self
            .state
            .npcs
            .remove(&owner.spawn)
            .filter(|npc| npc.id == owner)
            .ok_or(NativeError::UnknownTarget)?;
        let id = owner.spawn;
        let mut p = npc.effective_profile(self.profile)?;
        // Creature::Update does not regenerate on corpse/dead/respawn updates.
        let alive_update =
            npc.health > 0 && !matches!(npc.ai.phase(), ai::Phase::Corpse | ai::Phase::Despawned);
        let prior_motion = receipt_step
            .as_ref()
            .map(|(_, motion)| motion.clone())
            .unwrap_or_else(|| npc.ai.motion().cloned());
        if npc.health == 0 && !matches!(npc.ai.phase(), ai::Phase::Corpse | ai::Phase::Despawned) {
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
            npc.aura_bindings.clear();
        }
        let (effects, mut melee_events) = if npc.source_motion.is_some() {
            let (token, _) = receipt_step.ok_or(NativeError::InvalidState)?;
            self.advance_source_npc(&mut npc, token)?
        } else {
            let query = Query::Npc {
                position: npc.ai.position(),
                player_position: self.state.position,
                motion: npc.ai.motion().cloned(),
            };
            let Facts::Npc(facts) = self.ask(Some(npc.id), query)? else {
                return Err(NativeError::AuthorityBinding);
            };
            let movement = if let Some(motion) = prior_motion.filter(|_| {
                !matches!(npc.ai.phase(), ai::Phase::Corpse | ai::Phase::Despawned)
                    && npc.health > 0
            }) {
                let goal = match motion.purpose {
                    MovementPurpose::Wander => return Err(NativeError::InvalidState),
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
                    level: self.state.level(self.profile),
                    combat_reach: self.profile.player.combat_reach,
                    bounding_radius: self.profile.player.bounding_radius,
                    moving_and_running: self.moved,
                    alive: self.state.player_alive(),
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
            let death_timing = if receipt_step.is_none() {
                observation
                    .receipts
                    .iter()
                    .find(|r| matches!(r.event, ai::CombatEvent::Died))
                    .map(|receipt| self.creature_death_timing(&mut npc, receipt))
                    .transpose()?
                    .flatten()
            } else {
                None
            };
            // The death draw changes timing, never the selected combat statistics.
            p = npc.effective_profile(self.profile)?;
            let mut melee_events = Vec::new();
            let effects = if self.state.combat.is_some() {
                let start = self.events.len();
                let mut failure = None;
                let npc_id = npc.id;
                let health = npc.health;
                let mut callback = |_, position, started| {
                    if started {
                        self.enter_player_combat();
                    }
                    match self.native_melee(npc_id, position, health, &p, None, None) {
                        Ok(()) => Ok(if !self.state.player_alive() {
                            ai::MainhandResult::TargetDied
                        } else {
                            ai::MainhandResult::Unchanged
                        }),
                        Err(error) => {
                            failure = Some(error);
                            Err(ai::AiError::Invalid("native melee callback"))
                        }
                    }
                };
                let result = if let Some((token, _)) = receipt_step {
                    let prefix = token.effects().len();
                    npc.ai
                        .finish_receipts(observation, token, &mut callback)
                        .map(|effects| effects.into_iter().skip(prefix).collect())
                } else if npc.source_generation.is_some() {
                    let token = npc
                        .ai
                        .begin_receipts_with_death_timing(&observation, death_timing)?;
                    npc.ai.finish_receipts(observation, token, &mut callback)
                } else {
                    npc.ai.step_with_mainhand(
                        &mut ObservationAdapter(Some(observation)),
                        &mut |id, point, started| callback(id, point, started).map(|_| ()),
                    )
                };
                if let Some(error) = failure {
                    return Err(error);
                }
                melee_events = self.events.split_off(start);
                result?
            } else if npc.source_generation.is_some() {
                let token = npc
                    .ai
                    .begin_receipts_with_death_timing(&observation, death_timing)?;
                npc.ai.finish_receipts(observation, token, &mut |_, _, _| {
                    Ok(ai::MainhandResult::Unchanged)
                })?
            } else {
                npc.ai.step(&mut ObservationAdapter(Some(observation)))?
            };
            (effects, melee_events)
        };
        for effect in effects {
            match &effect {
                ai::Effect::MainhandSwing { .. } if self.state.combat.is_none() => {
                    swings.push(npc.id)
                }
                ai::Effect::EvadeStarted(_) => {
                    npc.original_loot_recipient = None;
                    npc.corpse_loot = None;
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
                }
                ai::Effect::CorpseRemoved => {
                    if let Some(owner) = &npc.source_generation {
                        npc.source_generation =
                            Some(owner.record_corpse_removed(self.state.tick, &npc.ai)?);
                    }
                    if self
                        .state
                        .quest_progression
                        .as_ref()
                        .is_some_and(|q| q.loot_target == Some(npc.id))
                    {
                        self.state.quest_progression.as_mut().unwrap().loot_target = None;
                        self.events.push(Event::Loot(LootEvent::Released {
                            corpse: npc.id,
                            send_response: true,
                        }));
                    }
                    npc.corpse_loot = None;
                }
                ai::Effect::Respawned { .. } => {
                    npc.original_loot_recipient = None;
                    npc.corpse_loot = None;
                    let previous_generation = npc.id.generation;
                    npc.id.generation =
                        npc.id.generation.checked_add(1).ok_or(NativeError::Limit)?;
                    match (&self.profile.creature_generations, &npc.source_generation) {
                        (Some(catalog), Some(owner)) => {
                            let profile = self.profile;
                            let tick = self.state.tick;
                            let seed = self.state.seed;
                            let mut cursor = self.state.rng_counter;
                            let (next, draws) = owner.respawn_with_initialization(
                                catalog,
                                npc.id,
                                tick,
                                seed,
                                &mut cursor,
                                Some(profile),
                                &mut |query| match self.ask(
                                    Some(npc.id),
                                    Query::CreatureInitialization(Box::new(query.clone())),
                                )? {
                                    Facts::CreatureInitialization(facts) => Ok(*facts),
                                    _ => Err(NativeError::AuthorityBinding),
                                },
                            )?;
                            self.state.rng_counter = cursor;
                            npc.source_generation = Some(next);
                            self.draws.extend(draws);
                            p = npc.effective_profile(self.profile)?;
                            npc.ai.rebind_after_respawn(
                                p.ai.clone(),
                                previous_generation,
                                npc.id.generation,
                            )?;
                        }
                        (None, None) => {}
                        _ => return Err(NativeError::AuthorityBinding),
                    }
                    if let Some(owner) = &npc.source_generation {
                        npc.source_collision_width = CreatureCollisionWidthState::new_generation(
                            self.profile,
                            npc.id,
                            owner,
                        )?;
                        npc.source_collision_height = CreatureCollisionHeightState::new_generation(
                            self.profile,
                            npc.id,
                            owner,
                        )?;
                    } else {
                        // A later generation never reuses the initial-only geometry receipt.
                        npc.source_collision_width = None;
                        npc.source_collision_height = None;
                    }
                    npc.events.rebind_respawn(npc.id)?;
                    if let Some(events) = &mut self.state.event_ai {
                        events.respawn(self.profile, npc.id, self.state.tick)?;
                    }
                    if let Some(capabilities) = &mut npc.source_capabilities {
                        capabilities.respawn(
                            self.profile,
                            npc.id,
                            npc.source_generation.as_ref(),
                        )?;
                    }
                    npc.health = npc
                        .source_generation
                        .as_ref()
                        .map_or(p.max_health, |owner| owner.result().stats.resources.health);
                    npc.auras.clear();
                    npc.aura_bindings.clear();
                }
                _ => {}
            }
            let started = matches!(effect, ai::Effect::CombatStarted { .. });
            let swung = matches!(effect, ai::Effect::MainhandSwing { .. });
            if npc.source_motion.is_some() && matches!(effect, ai::Effect::FaceTarget { .. }) {
                while matches!(
                    melee_events.first(),
                    Some(Event::NpcMotion {
                        event: benilla_sim::creature_motion::Event::FacingChanged { .. },
                        ..
                    })
                ) {
                    self.events.push(melee_events.remove(0));
                }
            }
            self.events.push(Event::Npc {
                id: npc.id,
                effect: effect.clone(),
            });
            self.source_ai_effect(&mut npc, &effect, None, None)?;
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
        if !melee_events.is_empty() {
            return Err(NativeError::InvalidState);
        }
        // Creature::RegenerateAll follows Unit::Update, including motion, AI and
        // CombatManager. In particular newly entered combat blocks this pulse.
        let before = npc.health;
        let regenerated = benilla_sim::creature_regeneration::update(
            p.health_regeneration,
            p.max_health,
            benilla_sim::creature_regeneration::State {
                elapsed_ms: npc.regen_elapsed_ms,
                health: npc.health,
            },
            benilla_sim::creature_regeneration::Frame {
                diff_ms: self.profile.tick_ms,
                alive_update: alive_update && npc.health > 0,
                in_combat: npc.ai.phase() == ai::Phase::Combat,
                evade_regeneration: npc.ai.is_evade_regeneration(),
            },
        )?;
        npc.regen_elapsed_ms = regenerated.elapsed_ms;
        npc.health = regenerated.health;
        if npc.health != before {
            self.events.push(Event::NpcHealthRegenerated {
                id: npc.id,
                before,
                after: npc.health,
            });
        }
        self.state.npcs.insert(id, npc);
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
        // Unit::SetInCombatState explicitly rejects dead units even though CanEnterCombat does not.
        if !self.state.player_alive() {
            return;
        }
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
        p: &NpcProfile,
        npc_orientation: Option<Orientation>,
        working: Option<&interactions::WorkingNpc<'_>>,
    ) -> Result<(), NativeError> {
        use benilla_sim::npc_melee as m;
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
            b"benilla-native-melee-action-v51\0",
            &(
                self.state.world,
                self.state.player,
                self.state.tick,
                self.profile_digest,
                self.profile.geometry_revision,
                id,
                position,
                npc_health,
                p,
                npc_orientation,
                self.state.position,
                self.state.orientation,
                self.state.health,
                self.state.resources.caster.checkpoint(),
                c.snapshot(),
                cast_states,
            ),
        );
        let legacy = if self.profile.source_combat_authority() {
            None
        } else {
            let Facts::NativeMelee(facts) = self.ask(
                Some(id),
                Query::NativeMelee {
                    npc_orientation,
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
            Some(facts)
        };
        let front_facing = if let Some(facts) = &legacy {
            facts.front_facing
        } else {
            self.source_melee_front(working.ok_or(NativeError::InvalidState)?, action)?
        };
        let c = self.state.combat.as_ref().unwrap();
        let defense = c.skills.get(&combat::DEFENSE_SKILL);
        let plan = m::prepare(m::MeleeInput {
            attacker: attack,
            defender: m::MageDefense {
                level: self.state.level(self.profile),
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
            front_facing,
        })?;
        let skill = defense.map(|s| m::DefenseSkill {
            value: s.value,
            maximum: s.maximum,
            gain: self.profile.combat.as_ref().unwrap().defense_gain,
        });
        // Establish unsupported skill callback before draws; even a miss can require it.
        let skill_chance = if !attack.creature_is_critter {
            skill
                .map(|s| m::defense_gain_chance(self.state.level(self.profile), s))
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
        if callbacks.killed {
            self.native_kill(id, result)?;
        } else {
            m::apply_health(&mut self.state.health, result)?;
        }
        if callbacks.surviving_damage_combat {
            // Unit::HandleDamageDealt tests these after ModifyHealth. Lethal
            // damage takes Unit::Kill and never executes this predicate callback.
            let facts = if let Some(facts) = &legacy {
                MeleeCombatFacts {
                    player_can_attack_npc: facts.player_can_attack_npc,
                    player_can_enter_combat: facts.player_can_enter_combat,
                    npc_can_enter_combat: facts.npc_can_enter_combat,
                }
            } else {
                self.source_melee_combat(working.ok_or(NativeError::InvalidState)?, action)?
            };
            if facts.player_can_attack_npc
                && facts.player_can_enter_combat
                && facts.npc_can_enter_combat
            {
                self.enter_player_combat();
            }
        }
        if callbacks.durability_hit_attempt {
            self.player_hit_durability(
                DrawPurpose::DurabilityChance { action, npc: id },
                DrawPurpose::DurabilitySlot { action, npc: id },
            )?;
        }
        if callbacks.interrupt_or_delay_spell {
            self.native_spell_damage()?;
        }
        // Post-health CanEnterCombat/AttackedBy applies even when the outcome was miss.
        if callbacks.attacked_by_and_combat {
            let facts = if let Some(facts) = &legacy {
                MeleeEntryFacts {
                    player_can_enter_combat: facts.player_can_enter_combat,
                    npc_can_enter_combat: facts.npc_can_enter_combat,
                }
            } else {
                self.source_melee_entry(working.ok_or(NativeError::InvalidState)?, action)?
            };
            if facts.player_can_enter_combat && facts.npc_can_enter_combat {
                self.enter_player_combat();
            }
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
                let value = m::resolve_defense_gain(self.state.level(self.profile), skill, roll)?;
                self.state
                    .combat
                    .as_mut()
                    .unwrap()
                    .skills
                    .get_mut(&combat::DEFENSE_SKILL)
                    .unwrap()
                    .value = value;
                if value != skill.value {
                    self.progression_defense_change()?;
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
            b"benilla-native-melee-chain-v51\0",
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
        let ids = self
            .state
            .casts
            .iter()
            .filter(|(_, c)| {
                matches!(c.execution.state(), spell::CastState::Casting { .. })
                    && self.profile.quest_progression.as_ref().map_or_else(
                        || {
                            self.profile
                                .combat
                                .as_ref()
                                .is_some_and(|g| g.damage_cancels.contains(&c.ability))
                        },
                        |g| g.damage_cancels(c.ability),
                    )
            })
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for id in ids {
            self.interrupt_scheduled_cast(id, spell::CastFailure::Interrupted)?;
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
            let eligible = c.binding.spec()?.damage_pushback;
            if eligible {
                let die = self
                    .state
                    .draw(id, DrawPurpose::Pushback, 0, 99, &mut self.draws)?
                    as u8;
                let events = c.execution.damage_pushback(die)?;
                self.spell_events(id, c.ability, c.target, events, None)?;
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
        // Compute the shared cadence exactly once on a private staged owner.
        // In this closed domain health has no mana dependency or observer hook.
        let mut next_resources = self.state.resources.clone();
        let result = next_resources.after_spell_and_aura_events(
            mana::ManaFrame {
                diff_ms: self.profile.tick_ms,
                in_world: true,
                alive_after_events: self.state.player_alive(),
                world_mana_rate: self.profile.player.world_mana_rate,
                exclusions: self.profile.player.mana_exclusions.into(),
            },
            events,
        )?;
        if let (Some(source), Some(diff_ms)) = (
            &self.profile.player_health_regeneration,
            result.regenerated_diff_ms,
        ) {
            let combat = self
                .state
                .combat
                .as_ref()
                .ok_or(NativeError::InvalidState)?;
            if !combat.standing || !self.state.player_alive() {
                return Err(NativeError::InvalidState);
            }
            let spirit =
                u32::try_from(combat.stats.attributes[4]).map_err(|_| NativeError::InvalidState)?;
            let health = benilla_sim::player_health_regeneration::regenerate(
                benilla_sim::player_health_regeneration::Input {
                    health: self.state.health,
                    max_health: combat.stats.max_health,
                    spirit,
                    diff_ms,
                    health_rate: source.health_rate(),
                    in_combat: combat.in_combat,
                },
            )
            .map_err(|_| NativeError::UnsupportedMechanic("health regeneration numeric domain"))?;
            self.state.health = health.after;
            if health.after != health.before {
                self.events.push(Event::PlayerHealthRegenerated {
                    before: health.before,
                    after: health.after,
                    diff_ms,
                });
            }
        }
        // RegenerateAll observes health before mana; neither callback owns an
        // additional timer. Dead frames retain the prior shared remainder.
        self.state.resources = next_resources;
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

#[cfg(test)]
mod journal_tests {
    use super::*;
    #[test]
    fn single_frame_size_failure_is_never_reclassified_as_journal_capacity() {
        assert_eq!(
            journal_size(MAX_BYTES - 1, 2, false),
            Err(NativeError::TooLarge)
        );
        assert_eq!(
            journal_size(MAX_BYTES - 1, 1000, true),
            Err(NativeError::TooLarge)
        );
        assert_eq!(
            journal_size(100, MAX_BYTES - 50, true),
            Err(NativeError::JournalFull)
        );
        assert_eq!(journal_size(MAX_BYTES - 2, 2, false), Ok(MAX_BYTES));
    }
}
