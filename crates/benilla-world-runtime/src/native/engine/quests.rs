// SPDX-License-Identifier: GPL-3.0-or-later
//! Owner-only ordinary quest commands; no client-authored kill credit.
use super::*;
use benilla_sim::{player_progression as pg, quest_lifecycle as q, starter_init as init};

impl<A: Authority> Frame<'_, A> {
    fn quest_interaction_gate(
        &mut self,
        quest: u32,
        giver: NpcId,
        reward: bool,
    ) -> Result<Option<Rejection>, NativeError> {
        let Some(g) = &self.profile.quest_progression else {
            return Err(NativeError::UnsupportedMechanic(
                "native quest/progression catalog",
            ));
        };
        if self.state.health == 0 {
            return Ok(Some(Rejection::Dead));
        }
        let Some(npc) = self.state.npcs.get(&giver.spawn).filter(|n| {
            n.id == giver
                && n.health > 0
                && !matches!(n.ai.phase(), ai::Phase::Corpse | ai::Phase::Despawned)
        }) else {
            return Ok(Some(Rejection::UnknownTarget));
        };
        let profile = npc.effective_profile(self.profile)?;
        let (npc_flags, unit_flags) =
            match interactions::current_npc_flags(self.profile, self.state, npc) {
                Ok(flags) => flags,
                Err(_)
                    if self.profile.interactions.is_none()
                        && !g.npc_rows.contains_key(&profile.entry) =>
                {
                    return Ok(Some(Rejection::QuestInteraction))
                }
                Err(error) => return Err(error),
            };
        // Hello uses UNIT_NPC_FLAG_NONE; CanInteractWithQuestGiver uses QUESTGIVER.
        let required_flags = if quest == 0 && !reward {
            npc_flags != 0
        } else {
            npc_flags & 2 != 0
        };
        if !required_flags
            || unit_flags & 0x0200_0000 != 0
            || (npc.ai.target().is_some() && npc.ai.victim_detached_at_ms().is_none())
        {
            return Ok(Some(Rejection::QuestInteraction));
        }
        let position = npc.ai.position();
        if !ai::within_interaction_distance(
            self.state.position,
            position,
            self.profile.player.combat_reach,
            profile.ai.combat_reach,
        )? {
            return Ok(Some(Rejection::QuestInteraction));
        }
        let query = Query::QuestInteraction {
            witness: self.interaction_witness()?,
            quest,
            reward,
            player_position: self.state.position,
            npc_position: position,
            player_health: self.state.health,
            npc_health: npc.health,
            npc_entry: profile.entry,
            npc_flags,
            unit_flags,
        };
        let Facts::QuestInteraction(facts) = self.ask(Some(giver), query)? else {
            return Err(NativeError::AuthorityBinding);
        };
        // Player::GetNPCIfCanInteractWith + Relations::CanInteractNow. The source
        // does not gate this path on LOS; this semantic query requests no geometry.
        if !facts.player_in_world
            || facts.player_taxi_flying
            || facts.player_cannot_react_or_lost_control
            || facts.player_charmed
            || facts.npc_charmed
            || facts.npc_pet_in_combat
            || facts.npc_invisible_to_alive
            || facts.player_reaction <= 2
            || facts.npc_reaction <= 2
            || facts.player_reaction > 7
            || facts.npc_reaction > 7
        {
            return Ok(Some(Rejection::QuestInteraction));
        }
        Ok(None)
    }
    pub(super) fn quest_command(
        &mut self,
        quest: u32,
        giver: NpcId,
        reward: bool,
        choice: u32,
    ) -> Result<Option<Rejection>, NativeError> {
        if choice >= 6 {
            return Ok(Some(Rejection::Quest(q::Gate::RewardChoice)));
        }
        if let Some(rejected) = self.quest_interaction_gate(quest, giver, reward)? {
            return Ok(Some(rejected));
        }
        let g = self
            .profile
            .quest_progression
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let profile = self
            .profile
            .npc(giver.spawn)
            .ok_or(NativeError::InvalidState)?;
        self.sync_quest_progression()?;
        let current = self
            .state
            .quest_progression
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let combat = self
            .state
            .combat
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let command = if reward {
            q::Command::Reward {
                quest,
                giver: profile.entry,
                announce: true,
                choice,
            }
        } else {
            q::Command::Accept {
                quest,
                giver: profile.entry,
            }
        };
        let (next, events) = match current.reduce(g, combat, command) {
            Ok(tx) => tx,
            Err(NativeError::Quest(q::Error::Rejected(gate))) => {
                let refusal = match gate {
                    q::Gate::RewardStorage { result, .. } => Some(Some(result)),
                    q::Gate::Items => {
                        Some(Some(benilla_sim::inventory::InventoryResult::ItemNotFound))
                    }
                    q::Gate::NotComplete | q::Gate::AlreadyRewarded | q::Gate::Money => Some(None),
                    _ => None,
                };
                // QuestHandler::HandleQuestgiverChooseRewardOpcode reoffers after
                // every CanRewardQuest refusal, after any source equipment error.
                if let Some(result) = refusal.filter(|_| reward) {
                    self.events.push(Event::QuestProgression(
                        QuestProgressionEvent::RewardRefused {
                            quest,
                            giver,
                            level: current.player.view().level as u8,
                            result,
                        },
                    ));
                }
                return Ok(Some(Rejection::Quest(gate)));
            }
            Err(e) => return Err(e),
        };
        let next_quest = if reward {
            let id = g.quest(quest)?.next_in_chain;
            (id != 0
                && g.quest_relations()
                    .starters
                    .iter()
                    .any(|r| r.creature == profile.entry && r.quest == id))
            .then_some(id)
        } else {
            None
        };
        self.commit_quest_progression(next)?;
        self.events
            .extend(events.into_iter().map(Event::QuestProgression));
        if let Some(quest) = next_quest {
            // QuestHandler opens same-giver chain details after RewardQuest,
            // without applying CanTakeQuest to this presentation callback.
            self.events.push(Event::QuestProgression(
                QuestProgressionEvent::ChainDetails {
                    quest,
                    giver,
                    level: self
                        .state
                        .quest_progression
                        .as_ref()
                        .ok_or(NativeError::InvalidState)?
                        .player
                        .view()
                        .level as u8,
                },
            ));
        }
        Ok(None)
    }
    pub(super) fn sync_quest_progression(&mut self) -> Result<(), NativeError> {
        let (Some(g), Some(current), Some(c)) = (
            &self.profile.quest_progression,
            &self.state.quest_progression,
            &self.state.combat,
        ) else {
            return if self.profile.quest_progression.is_none()
                && self.state.quest_progression.is_none()
            {
                Ok(())
            } else {
                Err(NativeError::InvalidState)
            };
        };
        let mut next = current.clone();
        next.synchronize(g, c, self.state.health, self.state.resources.caster.mana)?;
        self.commit_quest_progression(next)
    }
    pub(super) fn commit_quest_progression(
        &mut self,
        next: QuestProgressionState,
    ) -> Result<(), NativeError> {
        if self.state.quest_progression.is_none() {
            return Err(NativeError::InvalidState);
        }
        if self
            .state
            .casts
            .values()
            .any(|cast| matches!(cast.execution.state(), spell::CastState::Casting { .. }))
        {
            let g = self
                .profile
                .quest_progression
                .as_ref()
                .ok_or(NativeError::InvalidState)?;
            // Prepare retains SpellEntry and its timer. Preflight the original spell
            // under the new source closure without changing any live cast witness.
            for cast in self
                .state
                .casts
                .values()
                .filter(|c| matches!(c.execution.state(), spell::CastState::Casting { .. }))
            {
                g.validate_prepare_transition(&next, &cast.binding)?;
            }
        }
        let view = next.player.view();
        let c = self
            .state
            .combat
            .as_mut()
            .ok_or(NativeError::InvalidState)?;
        let recipe = c.recipe.rebase(
            view.level,
            view.modifiers.create,
            view.modifiers.create_health,
            view.modifiers.create_mana,
        )?;
        if recipe.snapshot().modifiers != view.modifiers {
            return Err(NativeError::InvalidState);
        }
        let live = init::Resources {
            health: view.numeric.health,
            max_health: view.numeric.max_health,
            mana: view.numeric.powers[0],
            max_mana: view.numeric.max_powers[0],
        };
        let result = recipe.current(live)?;
        c.recipe = recipe;
        c.stats = combat::CharacterStats::from_result(&result);
        c.skills = view
            .skills
            .iter()
            .filter(|s| s.status != pg::SkillStatus::Deleted)
            .map(|s| {
                (
                    s.id,
                    SkillState {
                        id: s.id,
                        value: s.value,
                        maximum: s.maximum,
                        step: s.step,
                        permanent_bonus: s.permanent_bonus,
                        temporary_bonus: s.temporary_bonus,
                    },
                )
            })
            .collect();
        self.state.health = view.numeric.health;
        self.state.resources.caster.mana = view.numeric.powers[0];
        self.state.resources = mana::MageManaState::from_state(
            self.state.resources.caster.clone(),
            view.numeric.max_powers[0],
            view.numeric.mana_regeneration,
            self.state.resources.regen_elapsed_ms(),
            self.state.resources.last_mana_use_timer_ms(),
        )?;
        self.state.quest_progression = Some(next);
        Ok(())
    }
}

impl<A: Authority> Frame<'_, A> {
    pub(super) fn native_kill_rewards(&mut self, id: NpcId) -> Result<(), NativeError> {
        let Some(g) = &self.profile.quest_progression else {
            return Ok(());
        };
        if !self
            .state
            .npcs
            .get(&id.spawn)
            .is_some_and(|n| n.id == id && n.health == 0)
        {
            return Err(NativeError::InvalidState);
        }
        if self
            .state
            .quest_progression
            .as_ref()
            .is_some_and(|q| q.credited_generations.get(&id.spawn) == Some(&id.generation))
        {
            return Ok(());
        }
        if self.state.npcs[&id.spawn].original_loot_recipient != Some(self.state.player) {
            return Err(NativeError::InvalidState);
        }
        self.sync_quest_progression()?;
        let profile = self.state.npcs[&id.spawn].effective_profile(self.profile)?;
        let (next, events) = self
            .state
            .quest_progression
            .as_ref()
            .ok_or(NativeError::InvalidState)?
            .killed(
                g,
                self.state
                    .combat
                    .as_ref()
                    .ok_or(NativeError::InvalidState)?,
                self.state.world,
                &profile,
                id,
            )?;
        self.commit_quest_progression(next)?;
        self.events
            .extend(events.into_iter().map(Event::QuestProgression));
        Ok(())
    }
    pub(super) fn progression_item_break(
        &mut self,
        c: &mut PlayerCombatState,
        slot: u8,
        live: init::Resources,
    ) -> Result<init::EquipmentRecompute, NativeError> {
        let before = c.clone();
        let result = c.apply_item_break(slot, live)?;
        if let (Some(g), Some(s)) = (
            &self.profile.quest_progression,
            &mut self.state.quest_progression,
        ) {
            s.item_break(g, &before, c, slot, &result, live.health, live.mana)?;
        }
        Ok(result)
    }
    pub(super) fn progression_defense_change(&mut self) -> Result<(), NativeError> {
        if let (Some(g), Some(s), Some(c)) = (
            &self.profile.quest_progression,
            &mut self.state.quest_progression,
            &self.state.combat,
        ) {
            s.defense_change(g, c, self.state.health, self.state.resources.caster.mana)?;
        }
        Ok(())
    }
}

impl NativeWorld {
    /// Read-only current-revision interaction check for local quest panels. This
    /// does not advance an owner sequence, credit a kill, accept, or reward a quest.
    /// Mutating commands repeat the same check in their committed tick.
    pub fn preview_quest_interaction(
        &self,
        giver: NpcId,
        quest: u32,
        reward: bool,
        authority: &mut impl Authority,
    ) -> Result<Option<Rejection>, NativeError> {
        let mut next = self.state.clone();
        let mut frame = Frame {
            read_only: true,
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
            movement_path: vec![],
            player_auras_updated: false,
        };
        frame.quest_interaction_gate(quest, giver, reward)
    }
}

impl NativeWorld {
    /// HandleQuestgiverStatusQuery uses generic CanInteract, not the immediate
    /// panel/command helper: no invented range, alive, taxi, charm or combat gate.
    /// ScriptDevAI dialog callbacks must be proven absent by questgiver_hooks.
    pub fn preview_quest_status(
        &self,
        giver: NpcId,
        authority: &mut impl Authority,
    ) -> Result<bool, NativeError> {
        self.profile
            .quest_progression
            .as_ref()
            .ok_or(NativeError::UnsupportedMechanic(
                "native quest/progression catalog",
            ))?;
        let npc = self
            .state
            .npcs
            .get(&giver.spawn)
            .filter(|n| n.id == giver && n.ai.phase() != ai::Phase::Despawned)
            .ok_or(NativeError::UnknownTarget)?;
        let profile = self
            .profile
            .npc(giver.spawn)
            .ok_or(NativeError::InvalidState)?;
        let (npc_flags, unit_flags) =
            interactions::current_npc_flags(&self.profile, &self.state, npc)?;
        if npc_flags == 0 || unit_flags & 0x0200_0000 != 0 {
            return Ok(false);
        }
        // The native death state stops at Corpse and cannot enter ghost state.
        let request = AuthorityRequest {
            binding: Binding {
                world: self.state.world,
                tick: self.state.tick,
                profile: self.profile_digest,
                authority_policy: self.profile.authority_policy,
                geometry: self.profile.geometry_revision,
                npc: Some(giver),
            },
            query: Query::QuestStatus {
                witness: interactions::witness(
                    self.profile_digest,
                    &self.state,
                    InteractionMoment::ReadOnly,
                )?,
                player_position: self.state.position,
                npc_position: npc.ai.position(),
                npc_entry: profile.entry,
                npc_flags,
                unit_flags,
                player_ghost: false,
            },
        };
        let reply = if let Some(facts) = interactions::resolve(
            &self.profile,
            self.profile_digest,
            &self.state,
            &request,
            InteractionMoment::ReadOnly,
        )? {
            let reply = AuthorityReply {
                request: request.clone(),
                facts,
            };
            authority.observe_derived(&reply)?;
            reply
        } else {
            authority.resolve(&request)?
        };
        if reply.request != request {
            return Err(NativeError::AuthorityBinding);
        }
        let Facts::QuestStatus {
            player_reaction,
            npc_reaction,
        } = reply.facts
        else {
            return Err(NativeError::AuthorityBinding);
        };
        if player_reaction > 7 || npc_reaction > 7 {
            return Err(NativeError::InvalidState);
        }
        Ok(player_reaction > 2 && npc_reaction > 2)
    }
}
