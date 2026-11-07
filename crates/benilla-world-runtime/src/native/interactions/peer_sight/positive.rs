// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 UnitAI.cpp:495-523, Unit.cpp:4460-4465,
// Player.h:1997, Object.cpp:1252-1263/1361-1395, Relations.cpp:458-513/1420-1437.
//! Predicate dependencies remain separate from the runtime-owned admission decision.
use super::*;
use benilla_sim::creature_ai::movement::Check;
use benilla_sim::creature_motion as motion;

/// Complete bound CheckForHelp call, including the resolved player's cached water
/// state. The helper/peer segment is deliberately distinct from the victim endpoint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeerHelpRequest {
    pub binding: Binding,
    pub source: crate::Digest,
    pub content: crate::Digest,
    pub map: u32,
    pub now_ms: u64,
    pub sight: VisitorSightRequest,
    pub helper: NpcId,
    pub peer: NpcId,
    pub victim: crate::PlayerId,
    pub helper_pose: motion::Pose,
    pub peer_pose: motion::Pose,
    pub helper_capability: CreatureCapabilityWitness,
    pub peer_capability: CreatureCapabilityWitness,
    pub helper_width: CreatureCollisionWidthWitness,
    pub peer_width: CreatureCollisionWidthWitness,
    pub helper_height: CreatureCollisionHeightWitness,
    pub peer_height: CreatureCollisionHeightWitness,
    pub helper_combat_reach: ai::Real,
    pub peer_combat_reach: ai::Real,
    /// CONFIG_FLOAT_CREATURE_CHECK_FOR_HELP_RADIUS, never template CallForHelp.
    pub radius: ai::Real,
    pub victim_water: PlayerEnvironmentTarget,
}
impl PeerHelpRequest {
    pub fn sight(&self) -> &VisitorSightRequest {
        &self.sight
    }
    pub fn accessible(&self) -> Result<bool, NativeError> {
        let capabilities = self.helper_capability.capabilities()?;
        Ok(source_accessible(
            self.victim_water.in_water(),
            capabilities.can_swim(),
            capabilities.can_walk(),
            capabilities.can_fly(),
        ))
    }
    pub fn within_range(&self) -> Result<bool, NativeError> {
        source_within_distance(
            self.helper_pose.position,
            self.peer_pose.position,
            self.radius,
            self.helper_combat_reach,
            self.peer_combat_reach,
        )
    }
    pub fn validate_for(
        &self,
        profile: &Profile,
        player: crate::PlayerId,
        sight: &VisitorSightRequest,
    ) -> Result<(), NativeError> {
        let source = profile
            .combat_assistance()
            .ok_or(NativeError::AuthorityUnavailable(
                "peer help selected assistance owners",
            ))?;
        source.validate(profile)?;
        if &self.sight != sight
            || self.source != source.digest()
            || self.victim != player
            || self.sight.observer != VisitorParticipant::Creature(self.helper)
            || self.sight.target != VisitorParticipant::Creature(self.peer)
            || self.helper == self.peer
            || self.binding.npc != Some(self.helper)
            || self.binding.profile != profile.digest()
            || self.binding.authority_policy != profile.authority_policy
            || self.binding.geometry != profile.geometry_revision
            || self.content != profile.content_digest
            || self.map != profile.map_id
            || profile.peer_help_radius() != Some(self.radius)
            || self.now_ms
                != profile
                    .clock_origin_ms
                    .checked_add(
                        self.binding
                            .tick
                            .checked_mul(u64::from(profile.tick_ms))
                            .ok_or(NativeError::Limit)?,
                    )
                    .ok_or(NativeError::Limit)?
        {
            return Err(NativeError::AuthorityBinding);
        }
        self.helper_pose.validate()?;
        self.peer_pose.validate()?;
        for (id, capability, width, height, reach) in [
            (
                self.helper,
                &self.helper_capability,
                &self.helper_width,
                &self.helper_height,
                self.helper_combat_reach,
            ),
            (
                self.peer,
                &self.peer_capability,
                &self.peer_width,
                &self.peer_height,
                self.peer_combat_reach,
            ),
        ] {
            let mut binding = self.binding.clone();
            binding.npc = Some(id);
            capability.validate_context(profile, &binding)?;
            width.validate_context(profile, &binding)?;
            let model =
                width.validated_model_selection(profile, id, capability.initialization())?;
            height.validate_for(profile, id)?;
            height.validate_width_selection(width)?;
            if model.combat_reach().bits() != reach.bits()
                || height.width_source_binding() != width.source_binding()
                || height.model_id() != width.model_id()
                || height.native_display_id() != width.native_display_id()
                || height.object_scale() != width.object_scale()
            {
                return Err(NativeError::AuthorityBinding);
            }
        }
        self.victim_water.validate(
            profile,
            self.binding.world,
            player,
            self.victim_water.target(),
            self.now_ms,
        )?;
        if self.victim_water.tick() != self.binding.tick {
            return Err(NativeError::AuthorityBinding);
        }
        self.within_range()?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum PeerHelpQuery {
    /// player.IsVisibleForOrDetect(helper, helper, false), not AI visibility or LOS.
    VictimVisibility(PeerHelpRequest),
    /// Explicitly authored same-map component membership only. It is not loaded
    /// Map coverage. The runtime computes the strict binary32 3D reach predicate.
    PeerRange(PeerHelpRequest),
    /// helper.IsWithinLOSInMap(peer, true): peer heights and ignore-M2 policy.
    PeerLineOfSight(PeerHelpRequest),
}
impl PeerHelpQuery {
    pub fn request(&self) -> &PeerHelpRequest {
        match self {
            Self::VictimVisibility(r) | Self::PeerRange(r) | Self::PeerLineOfSight(r) => r,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeerHelpFacts {
    pub query: Box<PeerHelpQuery>,
    pub check: Check,
}
impl PeerHelpFacts {
    pub fn validate_for(&self, query: &PeerHelpQuery) -> Result<Check, NativeError> {
        if self.query.as_ref() != query {
            return Err(NativeError::AuthorityBinding);
        }
        match self.check {
            Check::Unknown => Err(NativeError::AuthorityUnavailable(
                "peer help predicate authority",
            )),
            value => Ok(value),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeerHelpContinuation {
    pub request: PeerHelpRequest,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub visibility: Option<PeerHelpFacts>,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub same_map: Option<PeerHelpFacts>,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub line_of_sight: Option<PeerHelpFacts>,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub can_assist: Option<bool>,
}
impl PeerHelpContinuation {
    pub(super) fn denial(&self) -> Result<Option<PeerHelpDenial>, NativeError> {
        let denial = if !self.request.accessible()? {
            Some(PeerHelpDenial::VictimInaccessible)
        } else if self
            .visibility
            .as_ref()
            .ok_or(NativeError::AuthorityBinding)?
            .validate_for(&PeerHelpQuery::VictimVisibility(self.request.clone()))?
            == Check::Failed
        {
            Some(PeerHelpDenial::VictimNotVisible)
        } else if self
            .same_map
            .as_ref()
            .ok_or(NativeError::AuthorityBinding)?
            .validate_for(&PeerHelpQuery::PeerRange(self.request.clone()))?
            == Check::Failed
        {
            Some(PeerHelpDenial::PeerDifferentMap)
        } else if !self.request.within_range()? {
            Some(PeerHelpDenial::PeerOutOfRange)
        } else if self
            .line_of_sight
            .as_ref()
            .ok_or(NativeError::AuthorityBinding)?
            .validate_for(&PeerHelpQuery::PeerLineOfSight(self.request.clone()))?
            == Check::Failed
        {
            Some(PeerHelpDenial::PeerLineOfSightBlocked)
        } else if !self.can_assist.ok_or(NativeError::AuthorityBinding)? {
            Some(PeerHelpDenial::CannotAssistPeer)
        } else {
            None
        };
        let forbidden = match denial {
            Some(PeerHelpDenial::VictimInaccessible) => {
                self.visibility.is_some()
                    || self.same_map.is_some()
                    || self.line_of_sight.is_some()
                    || self.can_assist.is_some()
            }
            Some(PeerHelpDenial::VictimNotVisible) => {
                self.same_map.is_some() || self.line_of_sight.is_some() || self.can_assist.is_some()
            }
            Some(PeerHelpDenial::PeerDifferentMap | PeerHelpDenial::PeerOutOfRange) => {
                self.line_of_sight.is_some() || self.can_assist.is_some()
            }
            Some(PeerHelpDenial::PeerLineOfSightBlocked) => self.can_assist.is_some(),
            _ => false,
        };
        if forbidden {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(denial)
    }
}
/// The live prefix has established every early condition. A transcript is checked
/// for consistency only; it cannot install an assistance owner or mutate an NPC.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeerHelpReceipt {
    pub continuation: Box<PeerHelpContinuation>,
    pub observer_in_combat: bool,
    pub observer_can_call: bool,
    pub target_can_call: bool,
    pub observer_can_check: bool,
    pub target_can_check: bool,
    pub target_unit_state: u32,
    pub target_unit_flags: u32,
    pub observer_can_initiate: bool,
    pub observer_can_attack_victim: bool,
}
impl PeerHelpReceipt {
    pub(super) fn new(
        prefix: PeerHelpDenialReceipt,
        continuation: PeerHelpContinuation,
    ) -> Result<Self, NativeError> {
        let receipt = Self {
            continuation: Box::new(continuation),
            observer_in_combat: prefix.observer_in_combat,
            observer_can_call: prefix.observer_can_call,
            target_can_call: prefix.target_can_call,
            observer_can_check: prefix.observer_can_check,
            target_can_check: prefix.target_can_check,
            target_unit_state: prefix.target_unit_state,
            target_unit_flags: prefix.target_unit_flags,
            observer_can_initiate: prefix.observer_can_initiate == Some(true),
            observer_can_attack_victim: prefix.observer_can_attack_victim == Some(true),
        };
        receipt.validate_predicates()?;
        Ok(receipt)
    }
    pub fn request(&self) -> &PeerHelpRequest {
        &self.continuation.request
    }
    fn validate_predicates(&self) -> Result<(), NativeError> {
        if self.observer_in_combat
            || !self.observer_can_call
            || !self.target_can_call
            || !self.observer_can_check
            || !self.target_can_check
            || self.target_unit_state & 0x400 != 0
            || self.target_unit_flags & (0x0040_0000 | STUNNED) != 0
            || !self.observer_can_initiate
            || !self.observer_can_attack_victim
            || self.continuation.denial()?.is_some()
        {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
    pub(in crate::native) fn validate_for(
        &self,
        profile: &Profile,
        player: crate::PlayerId,
        request: &VisitorSightRequest,
    ) -> Result<(), NativeError> {
        self.validate_predicates()?;
        self.request().validate_for(profile, player, request)?;
        // The final CanAssist branch uses source-owned flags and directed faction
        // reaction. Its second CanAttackOnSight sees the unchanged resolved victim.
        if !can_assist_peer(profile, self.request().helper, self.request().peer)? {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
}

pub(super) fn source_accessible(
    in_water: bool,
    can_swim: bool,
    can_walk: bool,
    can_fly: bool,
) -> bool {
    if in_water {
        can_swim
    } else {
        can_walk || can_fly
    }
}

pub(in crate::native::interactions) fn source_within_distance(
    from: ai::Point,
    to: ai::Point,
    radius: ai::Real,
    from_reach: ai::Real,
    to_reach: ai::Real,
) -> Result<bool, NativeError> {
    if [radius, from_reach, to_reach]
        .iter()
        .any(|v| v.as_f32() < 0.0)
    {
        return Err(NativeError::AuthorityBinding);
    }
    let size = from_reach.as_f32() + to_reach.as_f32();
    let maxdist = radius.as_f32() + size;
    let squared = ai::Real::new(maxdist * maxdist)?;
    Ok(from.distance_squared(to)?.as_f32() < squared.as_f32())
}

pub(super) fn can_assist_peer(
    profile: &Profile,
    helper: NpcId,
    peer: NpcId,
) -> Result<bool, NativeError> {
    let visitors = profile
        .source_visitors
        .as_ref()
        .ok_or(NativeError::AuthorityUnavailable("peer help outdoor map"))?;
    if !visitors.outdoor() {
        return Err(NativeError::UnsupportedMechanic("instanceable peer help"));
    }
    let catalog = profile
        .interactions
        .as_ref()
        .ok_or(NativeError::InvalidProfile)?;
    let h = profile.npc(helper.spawn).ok_or(NativeError::InvalidState)?;
    let p = profile.npc(peer.spawn).ok_or(NativeError::InvalidState)?;
    let hs = &catalog.npcs[&h.entry];
    let ps = &catalog.npcs[&p.entry];
    if ps.unit_flags & unit_flags::UNINTERACTIBLE != 0 {
        return Ok(false);
    }
    let friendly = relations::faction_reaction(
        &catalog.factions[&h.faction_template].template(),
        &catalog.factions[&p.faction_template].template(),
    ) >= Reaction::Friendly;
    if !friendly {
        return Ok(false);
    }
    if (hs.unit_flags | ps.unit_flags) & unit_flags::PLAYER_CONTROLLED != 0 {
        return Err(NativeError::UnsupportedMechanic("controlled peer help"));
    }
    Ok(true)
}

pub(super) fn evaluate(
    profile: &Profile,
    state: &State,
    helper: &Npc,
    peer: &Npc,
    sight: &VisitorSightRequest,
    ask: &mut impl FnMut(PeerHelpQuery) -> Result<PeerHelpFacts, NativeError>,
) -> Result<PeerHelpContinuation, NativeError> {
    let motion_source = profile
        .source_motion
        .as_ref()
        .ok_or(NativeError::InvalidProfile)?;
    let victim = motion::Target {
        id: SOLO_UNIT,
        pose: motion::Pose {
            position: state.position,
            orientation: ai::Real::from_bits(state.orientation.bits())?,
        },
        movement_flags: state
            .player_movement_flags
            .ok_or(NativeError::InvalidState)?,
        combat_reach: profile.player.combat_reach,
        bounding_radius: profile.player.bounding_radius,
        collision_height: state
            .source_player_collision_height
            .as_ref()
            .map(|s| ai::Real::from_bits(s.witness().height().bits()))
            .transpose()?
            .unwrap_or(motion_source.player_collision_height),
    };
    let request = PeerHelpRequest {
        binding: Binding {
            world: state.world,
            tick: state.tick,
            profile: profile.digest(),
            authority_policy: profile.authority_policy,
            geometry: profile.geometry_revision,
            npc: Some(helper.id),
        },
        source: profile
            .combat_assistance()
            .ok_or(NativeError::InvalidProfile)?
            .digest(),
        content: profile.content_digest,
        map: profile.map_id,
        now_ms: state.now_ms,
        sight: sight.clone(),
        helper: helper.id,
        peer: peer.id,
        victim: state.player,
        helper_pose: helper
            .source_motion
            .as_ref()
            .ok_or(NativeError::InvalidState)?
            .motion
            .pose(),
        peer_pose: peer
            .source_motion
            .as_ref()
            .ok_or(NativeError::InvalidState)?
            .motion
            .pose(),
        helper_capability: helper
            .creature_capabilities()
            .ok_or(NativeError::InvalidState)?
            .witness()
            .clone(),
        peer_capability: peer
            .creature_capabilities()
            .ok_or(NativeError::InvalidState)?
            .witness()
            .clone(),
        helper_width: helper
            .creature_collision_width()
            .ok_or(NativeError::AuthorityUnavailable("peer help helper width"))?
            .witness()
            .clone(),
        peer_width: peer
            .creature_collision_width()
            .ok_or(NativeError::AuthorityUnavailable("peer help peer width"))?
            .witness()
            .clone(),
        helper_height: helper
            .creature_collision_height()
            .ok_or(NativeError::AuthorityUnavailable("peer help helper height"))?
            .witness()
            .clone(),
        peer_height: peer
            .creature_collision_height()
            .ok_or(NativeError::AuthorityUnavailable("peer help peer height"))?
            .witness()
            .clone(),
        helper_combat_reach: helper.effective_profile(profile)?.ai.combat_reach,
        peer_combat_reach: peer.effective_profile(profile)?.ai.combat_reach,
        radius: profile
            .peer_help_radius()
            .ok_or(NativeError::AuthorityUnavailable(
                "source CheckForHelp config radius",
            ))?,
        victim_water: PlayerEnvironmentTarget::new(profile, state, victim)?.ok_or(
            NativeError::AuthorityUnavailable("cached player water state"),
        )?,
    };
    request.validate_for(profile, state.player, sight)?;
    let mut result = PeerHelpContinuation {
        request,
        visibility: None,
        same_map: None,
        line_of_sight: None,
        can_assist: None,
    };
    if !result.request.accessible()? {
        return Ok(result);
    }
    let query = PeerHelpQuery::VictimVisibility(result.request.clone());
    let facts = ask(query.clone())?;
    let check = facts.validate_for(&query)?;
    result.visibility = Some(facts);
    if check == Check::Failed {
        return Ok(result);
    }
    let query = PeerHelpQuery::PeerRange(result.request.clone());
    let facts = ask(query.clone())?;
    let check = facts.validate_for(&query)?;
    result.same_map = Some(facts);
    if check == Check::Failed || !result.request.within_range()? {
        return Ok(result);
    }
    let query = PeerHelpQuery::PeerLineOfSight(result.request.clone());
    let facts = ask(query.clone())?;
    let check = facts.validate_for(&query)?;
    result.line_of_sight = Some(facts);
    if check == Check::Failed {
        return Ok(result);
    }
    result.can_assist = Some(can_assist_peer(profile, helper.id, peer.id)?);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_player_water_and_helper_capabilities_match_exact_source_oracle() {
        let vectors: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../../scripts/native_assistance_source_oracle/accessibility_vectors.json"
        ))
        .unwrap();
        assert_eq!(
            vectors["source_revision"],
            "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f"
        );
        let cases = vectors["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 32);
        for case in cases {
            let input = &case["input"];
            assert_eq!(
                source_accessible(
                    input["environment_flags"].as_u64().unwrap() & 1 != 0,
                    input["can_swim"].as_bool().unwrap(),
                    input["can_walk"].as_bool().unwrap(),
                    input["can_fly"].as_bool().unwrap()
                ),
                case["observation"]["accessible"].as_bool().unwrap(),
                "{}",
                case["name"]
            );
        }
    }
    #[test]
    fn helper_peer_distance_is_strict_three_dimensional_and_adds_both_combat_reaches() {
        let point = |x, y, z| ai::Point::new(x, y, z).unwrap();
        let real = |v| ai::Real::new(v).unwrap();
        let from = point(0.0, 0.0, 0.0);
        let within = |to, radius| {
            source_within_distance(from, to, real(radius), real(1.5), real(2.25)).unwrap()
        };
        let threshold = 8.75_f32;
        assert!(within(
            point(f32::from_bits(threshold.to_bits() - 1), 0.0, 0.0),
            5.0
        ));
        assert!(!within(point(threshold, 0.0, 0.0), 5.0));
        assert!(!within(
            point(f32::from_bits(threshold.to_bits() + 1), 0.0, 0.0),
            5.0
        ));
        assert!(!within(point(0.0, 0.0, threshold), 5.0));
        assert!(within(point(5.5, 0.0, 0.0), 5.0));
        assert!(!within(point(5.5, 0.0, 0.0), 0.01));
        assert!(within(point(0.0, 0.0, 0.0), 0.0));
        assert!(source_within_distance(from, from, real(-1.0), real(1.5), real(2.25)).is_err());
    }
}
