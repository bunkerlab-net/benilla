// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, UnitAI.cpp:68-101/495-523, Unit.h:1239-1248, Relations.cpp:351-416/1144-1161,
// Creature.cpp:2271 and Unit.cpp:6228; upstream GPL-2.0-or-later.
//! Source-owned ordinary NPC/NPC sight predicates after the notifier visibility gate.
use super::*;
use benilla_formats::Reaction;
mod positive;
pub(super) use positive::source_within_distance;
pub use positive::PeerHelpContinuation;
pub use positive::{PeerHelpFacts, PeerHelpQuery, PeerHelpReceipt, PeerHelpRequest};

pub(in crate::native) enum PeerSightPreparation {
    Complete(PeerSightReceipt),
    Assist(PeerHelpReceipt),
}

/// Runtime-derived no-op receipts, never externally supplied AI decisions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PeerSightDisposition {
    ExistingOutdoorVictim,
    NeutralToAll,
    NonAggressive,
    CannotInitiateAttack,
    CannotAttackOnSight,
}

/// The first source predicate that denies CheckForHelp. It does not itself
/// terminate MoveInLineOfSight, whose ordinary continuation is recorded alongside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PeerHelpDenial {
    ObserverInCombat,
    ObserverCannotCall,
    TargetCannotCall,
    ObserverCannotCheck,
    TargetCannotCheck,
    TargetRetreating,
    TargetConfused,
    TargetStunned,
    CannotInitiateAttack,
    CannotAttackVictimOnSight,
    VictimInaccessible,
    VictimNotVisible,
    PeerDifferentMap,
    PeerOutOfRange,
    PeerLineOfSightBlocked,
    CannotAssistPeer,
}
/// Current Unit-owner facts, derived only while the directional callback owns its
/// complete scene. The enclosing VisitorSightRequest binds the callback and scene.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeerHelpDenialReceipt {
    pub source: crate::Digest,
    pub observer: NpcId,
    pub target: NpcId,
    pub helper_victim: crate::PlayerId,
    pub observer_in_combat: bool,
    pub observer_can_call: bool,
    pub target_can_call: bool,
    pub observer_can_check: bool,
    pub target_can_check: bool,
    pub target_unit_state: u32,
    pub target_unit_flags: u32,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub observer_can_initiate: Option<bool>,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub observer_can_attack_victim: Option<bool>,
    pub denial: PeerHelpDenial,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub continuation: Option<Box<PeerHelpContinuation>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeerSightReceipt {
    pub disposition: PeerSightDisposition,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub help: Option<PeerHelpDenialReceipt>,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub assistance: Option<Box<PeerHelpReceipt>>,
}
impl From<PeerSightDisposition> for PeerSightReceipt {
    fn from(disposition: PeerSightDisposition) -> Self {
        Self {
            disposition,
            help: None,
            assistance: None,
        }
    }
}
impl PeerHelpDenialReceipt {
    fn early_denial(&self) -> Option<PeerHelpDenial> {
        use PeerHelpDenial::*;
        if self.observer_in_combat {
            Some(ObserverInCombat)
        } else if !self.observer_can_call {
            Some(ObserverCannotCall)
        } else if !self.target_can_call {
            Some(TargetCannotCall)
        } else if !self.observer_can_check {
            Some(ObserverCannotCheck)
        } else if !self.target_can_check {
            Some(TargetCannotCheck)
        } else if self.target_unit_state & 0x0000_0400 != 0 {
            Some(TargetRetreating)
        } else if self.target_unit_flags & 0x0040_0000 != 0 {
            Some(TargetConfused)
        } else if self.target_unit_flags & STUNNED != 0 {
            Some(TargetStunned)
        } else {
            None
        }
    }
    fn validate(&self) -> Result<(), NativeError> {
        let valid = if let Some(denial) = self.early_denial() {
            self.denial == denial
                && self.observer_can_initiate.is_none()
                && self.observer_can_attack_victim.is_none()
        } else {
            match (self.observer_can_initiate, self.observer_can_attack_victim) {
                (Some(false), None) => self.denial == PeerHelpDenial::CannotInitiateAttack,
                (Some(true), Some(false)) => {
                    self.denial == PeerHelpDenial::CannotAttackVictimOnSight
                }
                (Some(true), Some(true)) => {
                    self.continuation.as_ref().is_some_and(|continuation| {
                        continuation
                            .denial()
                            .is_ok_and(|denial| denial == Some(self.denial))
                    })
                }
                _ => false,
            }
        };
        if !valid
            || self.observer == self.target
            || ((self.observer_can_attack_victim != Some(true)) && self.continuation.is_some())
        {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
}
impl PeerSightReceipt {
    pub fn joined_combat(&self) -> bool {
        self.assistance.is_some()
    }
    pub(in crate::native) fn validate_for(
        &self,
        profile: &Profile,
        player: crate::PlayerId,
        request: &VisitorSightRequest,
    ) -> Result<(), NativeError> {
        if let Some(assistance) = &self.assistance {
            assistance.validate_for(profile, player, request)?;
            if self.help.is_some()
                || matches!(
                    self.disposition,
                    PeerSightDisposition::ExistingOutdoorVictim
                        | PeerSightDisposition::NeutralToAll
                )
            {
                return Err(NativeError::AuthorityBinding);
            }
        }
        if let Some(help) = &self.help {
            help.validate()?;
            if let Some(continuation) = &help.continuation {
                continuation
                    .request
                    .validate_for(profile, player, request)?;
                if continuation.can_assist.is_some_and(|can_assist| {
                    positive::can_assist_peer(profile, help.observer, help.target) != Ok(can_assist)
                }) {
                    return Err(NativeError::AuthorityBinding);
                }
            }
            if request.observer != VisitorParticipant::Creature(help.observer)
                || request.target != VisitorParticipant::Creature(help.target)
                || help.helper_victim != player
                || matches!(
                    self.disposition,
                    PeerSightDisposition::ExistingOutdoorVictim
                        | PeerSightDisposition::NeutralToAll
                )
                || profile
                    .combat_assistance()
                    .is_none_or(|catalog| catalog.digest() != help.source)
            {
                return Err(NativeError::AuthorityBinding);
            }
        }
        Ok(())
    }
}

/// The notifying owner may already be removed by its consecutive Unit update.
/// Both callback directions borrow this complete overlay without removing a peer,
/// advancing its observation sequence, or reconstructing any pre-frame actor.
#[cfg(test)]
pub(in crate::native) fn source_peer_sight(
    profile: &Profile,
    state: &State,
    removed_owner: Option<&Npc>,
    observer: NpcId,
    target: NpcId,
) -> Result<PeerSightReceipt, NativeError> {
    match peer_sight_prepare(
        profile,
        state,
        removed_owner,
        observer,
        target,
        None,
        &mut |_| {
            Err(NativeError::AuthorityUnavailable(
                "peer help predicate authority",
            ))
        },
    )? {
        PeerSightPreparation::Complete(receipt) => Ok(receipt),
        PeerSightPreparation::Assist(_) => Err(NativeError::AuthorityUnavailable(
            "peer help AttackStart owner",
        )),
    }
}
pub(in crate::native) fn source_peer_sight_prepare(
    profile: &Profile,
    state: &State,
    removed_owner: Option<&Npc>,
    observer: NpcId,
    target: NpcId,
    request: &VisitorSightRequest,
    ask: &mut impl FnMut(PeerHelpQuery) -> Result<PeerHelpFacts, NativeError>,
) -> Result<PeerSightPreparation, NativeError> {
    peer_sight_prepare(
        profile,
        state,
        removed_owner,
        observer,
        target,
        Some(request),
        ask,
    )
}
fn peer_sight_prepare(
    profile: &Profile,
    state: &State,
    removed_owner: Option<&Npc>,
    observer: NpcId,
    target: NpcId,
    request: Option<&VisitorSightRequest>,
    ask: &mut impl FnMut(PeerHelpQuery) -> Result<PeerHelpFacts, NativeError>,
) -> Result<PeerSightPreparation, NativeError> {
    let visitors = profile
        .source_visitors
        .as_ref()
        .ok_or(NativeError::UnsupportedMechanic(
            "peer sight requires complete source visitors",
        ))?;
    visitors.validate(profile)?;
    let catalog = profile
        .interactions
        .as_ref()
        .ok_or(NativeError::InvalidProfile)?;
    validate_abilities(state.abilities(profile))?;
    validate_scene(profile, state, removed_owner)?;
    let observer = current_npc(state, removed_owner, observer)?;
    let target = current_npc(state, removed_owner, target)?;
    if observer.id == target.id
        || !alive(observer)
        || !alive(target)
        || observer.ai.phase() == ai::Phase::ReturningHome
    {
        return Err(NativeError::AuthorityBinding);
    }
    let observer_profile = profile
        .npc(observer.id.spawn)
        .ok_or(NativeError::InvalidState)?;
    let observer_faction = &catalog.factions[&observer_profile.faction_template];

    // The selected closure derives ordinary Aggressive/Defensive EventAI from
    // raw ExtraFlags and proves ground actors. Passive/flying branches cannot run.
    if active_attack_victim(observer)? {
        return Ok(PeerSightPreparation::Complete(
            PeerSightDisposition::ExistingOutdoorVictim.into(),
        ));
    }
    // Unit::IsNeutralToAll short-circuits faction zero before masks/reputation.
    let neutral = observer_faction.faction == 0
        || (!observer_faction
            .reputation_list_id
            .is_some_and(|id| id >= 0)
            && observer_faction.enemies.iter().all(|enemy| *enemy == 0)
            && observer_faction.enemy_group_mask == 0
            && observer_faction.friend_group_mask == 0);
    if neutral {
        return Ok(PeerSightPreparation::Complete(
            PeerSightDisposition::NeutralToAll.into(),
        ));
    }
    // UnitAI::CheckForHelp precedes reaction and the ordinary sight chain.
    // A denied help prefix resumes that chain; a positive branch is not a no-op.
    let help = if target.ai.phase() == ai::Phase::Combat {
        let mut prefix = source_check_for_help(profile, state, removed_owner, observer, target)?;
        if prefix.observer_can_attack_victim == Some(true) {
            let request = request.ok_or(NativeError::AuthorityUnavailable(
                "peer help callback witness",
            ))?;
            let continuation = positive::evaluate(profile, state, observer, target, request, ask)?;
            if let Some(denial) = continuation.denial()? {
                prefix.denial = denial;
                prefix.continuation = Some(Box::new(continuation));
            } else {
                return Ok(PeerSightPreparation::Assist(PeerHelpReceipt::new(
                    prefix,
                    continuation,
                )?));
            }
        }
        Some(prefix)
    } else {
        None
    };
    ordinary_sight(profile, observer, target, help, None).map(PeerSightPreparation::Complete)
}

pub(in crate::native) fn source_peer_sight_after_help(
    profile: &Profile,
    state: &State,
    removed_owner: Option<&Npc>,
    observer: NpcId,
    target: NpcId,
    proof: PeerHelpReceipt,
) -> Result<PeerSightReceipt, NativeError> {
    validate_scene(profile, state, removed_owner)?;
    let observer = current_npc(state, removed_owner, observer)?;
    let target = current_npc(state, removed_owner, target)?;
    proof.validate_for(profile, state.player, proof.request().sight())?;
    if proof.request().helper != observer.id
        || proof.request().peer != target.id
        || !active_attack_victim(observer)?
    {
        return Err(NativeError::AuthorityBinding);
    }
    ordinary_sight(profile, observer, target, None, Some(Box::new(proof)))
}

fn ordinary_sight(
    profile: &Profile,
    observer: &Npc,
    target: &Npc,
    help: Option<PeerHelpDenialReceipt>,
    assistance: Option<Box<PeerHelpReceipt>>,
) -> Result<PeerSightReceipt, NativeError> {
    let catalog = profile
        .interactions
        .as_ref()
        .ok_or(NativeError::InvalidProfile)?;
    let observer_profile = profile
        .npc(observer.id.spawn)
        .ok_or(NativeError::InvalidState)?;
    let target_profile = profile
        .npc(target.id.spawn)
        .ok_or(NativeError::InvalidState)?;
    let observer_source = &catalog.npcs[&observer_profile.entry];
    let target_source = &catalog.npcs[&target_profile.entry];
    let observer_faction = &catalog.factions[&observer_profile.faction_template];
    let target_faction = &catalog.factions[&target_profile.faction_template];
    let receipt = |disposition| {
        Ok(PeerSightReceipt {
            disposition,
            help,
            assistance,
        })
    };
    // UnitAI.cpp:86: Defensive actors can reach help, but never initiate here.
    if observer_profile.ai.reaction != ai::Reaction::Aggressive {
        return receipt(PeerSightDisposition::NonAggressive);
    }
    let observer_flags = current_flags(observer_source, observer);
    if !relations::can_initiate_attack(relations::CreatureInitiateAttackInput {
        unit_state: observer_source.control.unit_state,
        unit_flags: observer_flags,
        force_attacking_capability: profile.source_pathfinding_extra_flags(observer.id.spawn)?
            & 0x0008_0000
            != 0,
        can_aggro: observer.events.can_aggro(),
    }) {
        return receipt(PeerSightDisposition::CannotInitiateAttack);
    }

    // UnitAI::AssistPlayerInCombat's base implementation is false. The complete
    // ordinary-AI closure excludes overrides, pets and successful feign death.
    if target.ai.phase() == ai::Phase::ReturningHome {
        return receipt(PeerSightDisposition::CannotAttackOnSight);
    }
    let target_flags = current_flags(target_source, target);
    if target_flags & relations::ATTACK_BLOCKING_FLAGS != 0
        || target_flags & unit_flags::IMMUNE_TO_NPC != 0
        || observer_flags & unit_flags::IMMUNE_TO_NPC != 0
    {
        return receipt(PeerSightDisposition::CannotAttackOnSight);
    }
    let observer_template = observer_faction.template();
    let target_template = target_faction.template();
    let observer_enemy =
        relations::faction_reaction(&observer_template, &target_template) == Reaction::Hostile;
    let target_enemy =
        relations::faction_reaction(&target_template, &observer_template) == Reaction::Hostile;
    let can_attack = observer_enemy || target_enemy;
    if !can_attack || !observer_enemy {
        return receipt(PeerSightDisposition::CannotAttackOnSight);
    }
    Err(NativeError::UnsupportedMechanic(
        "peer sight attack/accessibility continuation",
    ))
}

fn source_check_for_help(
    profile: &Profile,
    state: &State,
    removed_owner: Option<&Npc>,
    observer: &Npc,
    target: &Npc,
) -> Result<PeerHelpDenialReceipt, NativeError> {
    // Unit.h:1239 returns the active GetVictim before inspecting m_attackers.
    // TargetGuid/current_victim alone cannot establish that first branch.
    if !active_attack_victim(target)? {
        return Err(NativeError::UnsupportedMechanic(
            "peer help attacker fallback",
        ));
    }
    let assistance_catalog =
        profile
            .combat_assistance()
            .ok_or(NativeError::AuthorityUnavailable(
                "peer help selected assistance owners",
            ))?;
    assistance_catalog.validate(profile)?;
    let source = assistance_catalog.digest();
    let observer_owner = observer
        .events
        .assistance()
        .ok_or(NativeError::AuthorityUnavailable(
            "peer help observer assistance owner",
        ))?;
    let target_owner = target
        .events
        .assistance()
        .ok_or(NativeError::AuthorityUnavailable(
            "peer help target assistance owner",
        ))?;
    if observer_owner.source_identity() != source || target_owner.source_identity() != source {
        return Err(NativeError::AuthorityBinding);
    }
    let catalog = profile
        .interactions
        .as_ref()
        .ok_or(NativeError::InvalidProfile)?;
    let observer_profile = profile
        .npc(observer.id.spawn)
        .ok_or(NativeError::InvalidState)?;
    let target_profile = profile
        .npc(target.id.spawn)
        .ok_or(NativeError::InvalidState)?;
    let observer_source = &catalog.npcs[&observer_profile.entry];
    let target_source = &catalog.npcs[&target_profile.entry];
    let mut receipt = PeerHelpDenialReceipt {
        source,
        observer: observer.id,
        target: target.id,
        helper_victim: state.player,
        observer_in_combat: observer.ai.phase() == ai::Phase::Combat,
        observer_can_call: profile.assistance_extra_flags(observer.id.spawn)?
            & combat_assistance::NO_CALL_ASSIST
            == 0,
        target_can_call: profile.assistance_extra_flags(target.id.spawn)?
            & combat_assistance::NO_CALL_ASSIST
            == 0,
        observer_can_check: observer_owner.can_check_for_help(),
        target_can_check: target_owner.can_check_for_help(),
        target_unit_state: target_source.control.unit_state,
        target_unit_flags: current_flags(target_source, target),
        observer_can_initiate: None,
        observer_can_attack_victim: None,
        denial: PeerHelpDenial::CannotInitiateAttack,
        continuation: None,
    };
    if let Some(denial) = receipt.early_denial() {
        receipt.denial = denial;
        return Ok(receipt);
    }
    let can_initiate = relations::can_initiate_attack(relations::CreatureInitiateAttackInput {
        unit_state: observer_source.control.unit_state,
        unit_flags: current_flags(observer_source, observer),
        force_attacking_capability: profile.assistance_extra_flags(observer.id.spawn)?
            & 0x0008_0000
            != 0,
        can_aggro: observer.events.can_aggro(),
    });
    receipt.observer_can_initiate = Some(can_initiate);
    if !can_initiate {
        return Ok(receipt);
    }
    // Reconstitute the exact overlay and detach only the observing Unit for the
    // existing NPC/player source-relation host. No victim state is synthesized.
    let mut overlay = state.clone();
    if let Some(owner) = removed_owner {
        if overlay.npcs.insert(owner.id.spawn, owner.clone()).is_some() {
            return Err(NativeError::AuthorityBinding);
        }
    }
    let current_observer = overlay
        .npcs
        .remove(&observer.id.spawn)
        .ok_or(NativeError::AuthorityBinding)?;
    let can_attack =
        WorkingNpc::from_npc(&current_observer)?.can_attack_player_on_sight(profile, &overlay)?;
    receipt.observer_can_attack_victim = Some(can_attack);
    if !can_attack {
        receipt.denial = PeerHelpDenial::CannotAttackVictimOnSight;
        return Ok(receipt);
    }
    Ok(receipt)
}

fn alive(npc: &Npc) -> bool {
    npc.health != 0 && !matches!(npc.ai.phase(), ai::Phase::Corpse | ai::Phase::Despawned)
}

fn current_flags(source: &NpcInteractionSource, npc: &Npc) -> u32 {
    source.unit_flags
        | if npc.ai.phase() == ai::Phase::Combat {
            unit_flags::IN_COMBAT
        } else {
            0
        }
}

fn current_npc<'a>(
    state: &'a State,
    removed_owner: Option<&'a Npc>,
    id: NpcId,
) -> Result<&'a Npc, NativeError> {
    removed_owner
        .filter(|npc| npc.id.spawn == id.spawn)
        .or_else(|| state.npcs.get(&id.spawn))
        .filter(|npc| npc.id == id)
        .ok_or(NativeError::AuthorityBinding)
}

fn validate_scene(
    profile: &Profile,
    state: &State,
    removed_owner: Option<&Npc>,
) -> Result<(), NativeError> {
    if state.npcs.len() + usize::from(removed_owner.is_some()) != profile.npcs.len()
        || removed_owner.is_some_and(|npc| state.npcs.contains_key(&npc.id.spawn))
        || state.npcs.iter().any(|(spawn, npc)| *spawn != npc.id.spawn)
    {
        return Err(NativeError::AuthorityBinding);
    }
    for row in &profile.npcs {
        let npc = removed_owner
            .filter(|npc| npc.id.spawn == row.ai.creature_id)
            .or_else(|| state.npcs.get(&row.ai.creature_id))
            .ok_or(NativeError::AuthorityBinding)?;
        // A live capability owner also binds the current generation. A public
        // receipt or a stale NpcId cannot construct or replace this live state.
        npc.creature_capabilities()
            .ok_or(NativeError::InvalidState)?
            .witness()
            .validate_for(profile, npc.id)?;
        if npc.source_motion.is_none() || npc.health > npc.effective_profile(profile)?.max_health {
            return Err(NativeError::InvalidState);
        }
    }
    Ok(())
}

fn active_attack_victim(npc: &Npc) -> Result<bool, NativeError> {
    if npc.ai.phase() != ai::Phase::Combat {
        return Ok(false);
    }
    // AttackStart installs Unit's victim before ThreatManager chooses its cached
    // victim. AttackStop(false) after lethal player damage retains TargetGuid.
    let detached = npc.ai.victim_detached_at_ms().is_some();
    if npc.ai.target() != Some(SOLO_UNIT)
        || npc.ai.hostile_reference().is_some() == detached
        || npc
            .ai
            .hostile_reference()
            .is_some_and(|reference| reference.target_guid() != SOLO_UNIT)
        || npc.ai.current_victim().is_some_and(|victim| {
            victim != SOLO_UNIT
                || npc
                    .ai
                    .hostile_reference()
                    .is_none_or(|reference| !reference.valid() || !reference.online())
        })
    {
        return Err(NativeError::InvalidState);
    }
    Ok(!detached)
}

#[cfg(test)]
mod tests;
