// SPDX-License-Identifier: GPL-3.0-or-later
// Source phase order: pinned Spell.cpp::CheckCast, AddUnitTarget,
// DoAllEffectOnTarget/DoSpellHitOnUnit and SpellAuras.cpp::PeriodicTick.
use super::*;
use interactions::spell_authority::CurrentSpell;

/// Borrowed from the actual frame owner. A receipt cannot construct this value.
#[derive(Clone, Copy)]
pub(super) enum SpellHistory<'a> {
    Prepare(&'a ExecutionBinding),
    Cast(
        &'a Cast,
        &'a ExecutionBinding,
        &'a benilla_sim::event_processor::EventCallbackSnapshot,
    ),
    Aura,
}
impl<'a> SpellHistory<'a> {
    fn digest(
        self,
        state: &State,
        target: NpcId,
        ability: u32,
        phase: SourceSpellPhase,
    ) -> Result<crate::Digest, NativeError> {
        let bytes = match self {
            Self::Prepare(binding)
                if phase == SourceSpellPhase::Prepare && binding.ability.id == ability =>
            {
                serde_json::to_vec(&(binding, &state.player_events.snapshot()))
            }
            Self::Cast(c, current, scheduler)
                if c.target == target
                    && c.ability == ability
                    && current.ability.id == ability
                    && scheduler.due.kind
                        == benilla_sim::event_processor::EventKind::SpellUpdate {
                            cast_id: c.id,
                        }
                    && scheduler.due.callback
                        == benilla_sim::event_processor::CallbackKind::Execute
                    && scheduler.due.diff_ms.is_some()
                    && matches!(
                        phase,
                        SourceSpellPhase::ReleaseCheck
                            | SourceSpellPhase::ReleaseHit
                            | SourceSpellPhase::Impact
                    ) =>
            {
                serde_json::to_vec(&(
                    &c.binding,
                    &c.release_binding,
                    c.id,
                    c.ability,
                    c.target,
                    c.origin,
                    c.execution.checkpoint(),
                    current,
                    scheduler,
                ))
            }
            Self::Aura if phase == SourceSpellPhase::Aura => {
                let n = state
                    .npcs
                    .get(&target.spawn)
                    .filter(|n| n.id == target)
                    .ok_or(NativeError::InvalidState)?;
                let holder = n.auras.get(&ability).ok_or(NativeError::InvalidState)?;
                let binding = n
                    .aura_bindings
                    .get(&ability)
                    .ok_or(NativeError::InvalidState)?;
                serde_json::to_vec(&(binding, holder.checkpoint()))
            }
            _ => return Err(NativeError::AuthorityBinding),
        }
        .map_err(|_| NativeError::InvalidState)?;
        if bytes.len() > MAX_BYTES {
            return Err(NativeError::TooLarge);
        }
        Ok(crate::digest(
            b"benilla-source-spell-phase-history-v1\0",
            &bytes,
        ))
    }
    fn current(self) -> Option<&'a ExecutionBinding> {
        match self {
            Self::Prepare(b) | Self::Cast(_, b, _) => Some(b),
            Self::Aura => None,
        }
    }
}
impl<A: Authority> Frame<'_, A> {
    fn source_spell_request(
        &self,
        cast_id: u64,
        ability: u32,
        target: NpcId,
        phase: SourceSpellPhase,
        history: SpellHistory<'_>,
    ) -> Result<SourceSpellRequest, NativeError> {
        let n = self
            .state
            .npcs
            .get(&target.spawn)
            .filter(|n| n.id == target)
            .ok_or(NativeError::UnknownTarget)?;
        Ok(SourceSpellRequest {
            witness: self.interaction_witness()?,
            scheduler: match history {
                SpellHistory::Cast(_, _, s) => Some(s.clone()),
                _ => None,
            },
            history: history.digest(self.state, target, ability, phase)?,
            cast_id,
            ability,
            phase,
            player_orientation: self.state.orientation,
            player_position: self.state.position,
            target_position: n.ai.position(),
            player_level: self.state.level(self.profile),
            target_level: n.effective_profile(self.profile)?.ai.level,
        })
    }
    fn record_spell(
        &mut self,
        target: NpcId,
        request: SourceSpellRequest,
        facts: SpellFacts,
    ) -> Result<SpellFacts, NativeError> {
        if self.replies.len() >= 20_000 {
            return Err(NativeError::TooLarge);
        }
        let reply = AuthorityReply {
            request: AuthorityRequest {
                binding: Binding {
                    world: self.state.world,
                    tick: self.state.tick,
                    profile: self.profile_digest,
                    authority_policy: self.profile.authority_policy,
                    geometry: self.profile.geometry_revision,
                    npc: Some(target),
                },
                query: Query::SourceSpell(SourceSpellQuery::Semantics(request)),
            },
            facts: Facts::SourceSpell(SourceSpellFacts::Semantics(facts.clone())),
        };
        self.authority.observe_derived(&reply)?;
        self.replies.push(reply);
        Ok(facts)
    }
    /// The selected source path returns before the legacy unconditional crit
    /// assignment. No external SpellFacts can replace the result of this host.
    pub(super) fn source_spell_facts(
        &mut self,
        cast_id: u64,
        ability: u32,
        target: NpcId,
        phase: SourceSpellPhase,
        history: SpellHistory<'_>,
    ) -> Result<SpellFacts, NativeError> {
        let view = CurrentSpell::new(self.profile, self.state, target, ability)?;
        if let Some(current) = history.current() {
            view.validate_context(current)?;
        }
        let mut facts = SpellFacts {
            other_failure: None,
            allowed: true,
            immune: false,
            resist_chance: 0,
            critical_chance: 0,
        };
        match phase {
            SourceSpellPhase::Prepare | SourceSpellPhase::ReleaseCheck => {
                // Readiness precedes all spatial/target checks. Probe the same
                // pure component check against a resource clone; no timer/RNG or
                // GCD on the real transaction is changed here. Power is checked later.
                let checks = spell::CastChecks {
                    caster_alive: self.state.health > 0,
                    target_exists: true,
                    target_alive: true,
                    position_changed: false,
                    other_failure: None,
                };
                let early = match history {
                    SpellHistory::Prepare(b) => {
                        let mut resources = self.state.resources.caster.clone();
                        let (cast, _) = spell::MageProjectileCast::prepare(
                            b.spec()?.clone(),
                            SOLO_UNIT,
                            target.spawn,
                            self.state.now_ms,
                            &mut resources,
                            checks,
                        )?;
                        matches!(
                            cast.state(),
                            spell::CastState::Failed(
                                spell::CastFailure::NotReady | spell::CastFailure::CasterDead
                            )
                        )
                    }
                    SpellHistory::Cast(c, b, _) => matches!(
                        c.execution.release_failure_with_context(
                            &self.state.resources.caster,
                            checks,
                            self.state.now_ms,
                            b.release_context(&c.binding)?
                        )?,
                        Some(spell::CastFailure::NotReady | spell::CastFailure::CasterDead)
                    ),
                    _ => return Err(NativeError::AuthorityBinding),
                };
                if !early {
                    // Spell.cpp:4849 precedes untargetable/creature constraints,
                    // TARGET_HARMFUL CanAttackSpell (5100), then CheckRange (5145).
                    let request =
                        self.source_spell_request(cast_id, ability, target, phase, history)?;
                    let Facts::SourceSpell(SourceSpellFacts::LineOfSight(los)) = self.ask(
                        Some(target),
                        Query::SourceSpell(SourceSpellQuery::LineOfSight(request)),
                    )?
                    else {
                        return Err(NativeError::AuthorityBinding);
                    };
                    if !spatial(los, "spell CheckCast line of sight")? {
                        facts.other_failure = Some(0x2a);
                    } else {
                        let view = CurrentSpell::new(self.profile, self.state, target, ability)?;
                        if view.untargetable() || !view.can_attack_spell()? {
                            facts.other_failure = Some(0x0a);
                        } else {
                            let source = view.range();
                            let facing = view.needs_facing();
                            let request = self
                                .source_spell_request(cast_id, ability, target, phase, history)?;
                            let query = SourceSpellQuery::Range {
                                context: request,
                                source,
                                player_reach: self.profile.player.combat_reach,
                                target_reach: self
                                    .state
                                    .npcs
                                    .get(&target.spawn)
                                    .filter(|npc| npc.id == target)
                                    .ok_or(NativeError::InvalidState)?
                                    .effective_profile(self.profile)?
                                    .ai
                                    .combat_reach,
                            };
                            let Facts::SourceSpell(SourceSpellFacts::Range(range)) =
                                self.ask(Some(target), Query::SourceSpell(query))?
                            else {
                                return Err(NativeError::AuthorityBinding);
                            };
                            facts.other_failure = match range {
                                SpellRangeResult::Within => None,
                                SpellRangeResult::TooFar => Some(0x59),
                                SpellRangeResult::TooClose => Some(0x76),
                                SpellRangeResult::Unknown => {
                                    return Err(NativeError::AuthorityUnavailable(
                                        "spell CheckRange",
                                    ))
                                }
                            };
                            if facts.other_failure.is_none() && facing {
                                let request = self.source_spell_request(
                                    cast_id, ability, target, phase, history,
                                )?;
                                let Facts::SourceSpell(SourceSpellFacts::Facing(front)) = self
                                    .ask(
                                        Some(target),
                                        Query::SourceSpell(SourceSpellQuery::Facing(request)),
                                    )?
                                else {
                                    return Err(NativeError::AuthorityBinding);
                                };
                                if !spatial(front, "spell CheckRange facing")? {
                                    facts.other_failure = Some(0x7c);
                                }
                            }
                        }
                    }
                }
            }
            SourceSpellPhase::ReleaseHit => {
                // This is reached only after release CheckCast/CheckPower succeeds.
                // TARGET_UNIT_ENEMY is CHAIN in SpellTargets.cpp:39. With <=1
                // targets, CheckTarget skips caster LOS and no chain jump runs.
                // Raw immunity/reflection/effect-mask variants were rejected by
                // the catalog, before a single hit/direct/critical die is drawn.
                (facts.resist_chance, facts.critical_chance) = view.launch_chances()?;
            }
            SourceSpellPhase::Impact => {
                let SpellHistory::Cast(c, _, _) = history else {
                    return Err(NativeError::AuthorityBinding);
                };
                let hit = c
                    .execution
                    .launch_context()
                    .is_some_and(|l| l.rolls.hit > l.resist_chance);
                // A resisted launch never enters DoSpellHitOnUnit. The existing
                // failed-hostile-hit callback is independent of this predicate.
                if !hit {
                    facts.allowed = false;
                } else {
                    if self.state.npcs[&target.spawn].ai.is_evading() {
                        return Err(NativeError::UnsupportedMechanic(
                            "source direct impact evade absorption",
                        ));
                    }
                    // Immunities precede CanAttackSpell and visibility. The raw
                    // closure plus damage-only live holders proves absence here.
                    facts.allowed = view.can_attack_spell()?;
                    if facts.allowed {
                        let request =
                            self.source_spell_request(cast_id, ability, target, phase, history)?;
                        let Facts::SourceSpell(SourceSpellFacts::ImpactVisibility(visible)) = self
                            .ask(
                                Some(target),
                                Query::SourceSpell(SourceSpellQuery::ImpactVisibility(request)),
                            )?
                        else {
                            return Err(NativeError::AuthorityBinding);
                        };
                        facts.allowed = spatial(visible, "spell delayed impact visibility")?;
                    } else {
                        facts.immune = true;
                    }
                }
            }
            SourceSpellPhase::Aura => {
                // PeriodicTick checks damage immunity only. No new hit, crit,
                // relationship, range, facing or visibility check is performed.
                // Source DealDamageMods evade absorption remains in update_aura.
            }
        }
        let request = self.source_spell_request(cast_id, ability, target, phase, history)?;
        self.record_spell(target, request, facts)
    }
}
fn spatial(check: ai::movement::Check, dependency: &'static str) -> Result<bool, NativeError> {
    match check {
        ai::movement::Check::Passed => Ok(true),
        ai::movement::Check::Failed => Ok(false),
        ai::movement::Check::Unknown => Err(NativeError::AuthorityUnavailable(dependency)),
    }
}
