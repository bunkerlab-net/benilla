// SPDX-License-Identifier: GPL-3.0-or-later
//! CMaNGOS Classic 8ec338a1 Player.cpp:1483–1489,6069–6075,19113–19175 and
//! Player.h:1997. GPL-2.0-or-later upstream; bounded native cache ownership only.
use super::*;
use benilla_sim::creature_motion as motion;
use environment::{
    BoundLiquidResult, EnvironmentPhase, EnvironmentPose, EnvironmentQuery,
    EnvironmentSnapshotBinding, EnvironmentState, EnvironmentStateSnapshot, LiquidCatalog,
};
use serde::{Deserialize, Serialize};

pub const PLAYER_ENVIRONMENT_POLICY: &str = "cmangos-8ec338a1-native-player-environment-v1";

/// Explicit selected source and complete DBC. Neither hashes nor this selection admit
/// a native game; the loaded terrain provider still has to answer every due query.
/// ```compile_fail
/// use benilla_world_runtime::native::PlayerEnvironmentSource;
/// let _: PlayerEnvironmentSource = serde_json::from_str("{}").unwrap();
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlayerEnvironmentSource {
    policy: &'static str,
    snapshot: EnvironmentSnapshotBinding,
    height_source: crate::Digest,
    // The complete catalog's intrinsic digest is in snapshot. It is not caller rows.
    #[serde(skip)]
    catalog: LiquidCatalog,
}
impl PlayerEnvironmentSource {
    pub fn snapshot(&self) -> EnvironmentSnapshotBinding {
        self.snapshot
    }
    pub fn snapshot_binding(&self) -> EnvironmentSnapshotBinding {
        self.snapshot
    }
    pub fn catalog(&self) -> &LiquidCatalog {
        &self.catalog
    }
    pub fn binding(&self) -> crate::Digest {
        crate::digest(b"benilla-player-environment-source-v1\0", self)
    }
    pub(super) fn validate(&self, profile: &Profile) -> Result<(), NativeError> {
        let height =
            profile
                .player_collision_height
                .as_ref()
                .ok_or(NativeError::AuthorityUnavailable(
                    "owned player environment collision height",
                ))?;
        height.validate(profile)?;
        if self.policy != PLAYER_ENVIRONMENT_POLICY
            || self.snapshot.map != profile.map_id
            || self.snapshot.content != profile.content_digest
            || self.height_source != height.binding()
        {
            return Err(NativeError::InvalidProfile);
        }
        EnvironmentState::new(self.snapshot, &self.catalog)?;
        Ok(())
    }
}
impl Profile {
    pub fn initialize_player_environment(
        &mut self,
        snapshot: EnvironmentSnapshotBinding,
        catalog: LiquidCatalog,
    ) -> Result<(), NativeError> {
        if self.player_environment.is_some() {
            self.validate()?;
        }
        let height_source = self
            .player_collision_height
            .as_ref()
            .ok_or(NativeError::AuthorityUnavailable(
                "owned player environment collision height",
            ))?
            .binding();
        let source = PlayerEnvironmentSource {
            policy: PLAYER_ENVIRONMENT_POLICY,
            snapshot,
            height_source,
            catalog,
        };
        source.validate(self)?;
        if self
            .player_environment
            .as_ref()
            .is_some_and(|old| old != &source)
        {
            return Err(NativeError::InvalidProfile);
        }
        let mut next = self.clone();
        next.player_environment = Some(source);
        next.validate()?;
        *self = next;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvironmentOwner {
    height: PlayerCollisionHeightWitness,
    source: crate::Digest,
}
impl EnvironmentOwner {
    fn new(
        profile: &Profile,
        world: crate::WorldId,
        player: crate::PlayerId,
    ) -> Result<Self, NativeError> {
        let source = profile
            .player_environment
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        source.validate(profile)?;
        let height = PlayerCollisionHeightState::new(profile, world, player)?
            .ok_or(NativeError::InvalidState)?
            .witness()
            .clone();
        Ok(Self {
            height,
            source: source.binding(),
        })
    }
    fn validate(
        &self,
        profile: &Profile,
        world: crate::WorldId,
        player: crate::PlayerId,
    ) -> Result<(), NativeError> {
        if self != &Self::new(profile, world, player)? {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
}

/// Read-only cache and phase sequence, with no nested journal. The native frame owns
/// transaction staging and replay; timer expiry alone never refreshes terrain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerEnvironmentState {
    owner: EnvironmentOwner,
    sequence: u64,
    updated_tick: u64,
    state: EnvironmentState,
    last_pose: Option<motion::Pose>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlayerEnvironmentSnapshot {
    owner: EnvironmentOwner,
    sequence: u64,
    updated_tick: u64,
    state: EnvironmentStateSnapshot,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    last_pose: Option<motion::Pose>,
}
impl PlayerEnvironmentState {
    pub fn environment(&self) -> &EnvironmentState {
        &self.state
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn digest(&self) -> crate::Digest {
        crate::digest(b"benilla-player-environment-state-v1\0", &self.snapshot())
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn new(
        profile: &Profile,
        world: crate::WorldId,
        player: crate::PlayerId,
    ) -> Result<Option<Self>, NativeError> {
        let Some(source) = &profile.player_environment else {
            return Ok(None);
        };
        Ok(Some(Self {
            owner: EnvironmentOwner::new(profile, world, player)?,
            sequence: 0,
            updated_tick: 0,
            last_pose: None,
            state: EnvironmentState::new(source.snapshot, &source.catalog)?,
        }))
    }
    pub(super) fn snapshot(&self) -> PlayerEnvironmentSnapshot {
        PlayerEnvironmentSnapshot {
            owner: self.owner.clone(),
            sequence: self.sequence,
            updated_tick: self.updated_tick,
            state: self.state.snapshot(),
            last_pose: self.last_pose,
        }
    }
    fn validate(
        &self,
        profile: &Profile,
        world: crate::WorldId,
        player: crate::PlayerId,
    ) -> Result<(), NativeError> {
        self.owner.validate(profile, world, player)?;
        let source = profile
            .player_environment
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        if self.state
            != self
                .state
                .snapshot()
                .restore(source.snapshot, &source.catalog, self.sequence)?
        {
            return Err(NativeError::InvalidState);
        }
        self.state.validate_native_lifecycle(&source.catalog)?;
        if self.updated_tick > crate::MAX_TICK || self.sequence < self.updated_tick {
            return Err(NativeError::InvalidState);
        }
        let height = self.owner.height.height().bits();
        if self.last_pose.map(|pose| EnvironmentPose {
            position: point_bits(pose.position),
            collision_height: height,
        }) != self.state.last_position()
        {
            return Err(NativeError::InvalidState);
        }
        if let Some(pose) = self.last_pose {
            pose.position.distance_squared(pose.position)?;
            Orientation::from_bits(pose.orientation.bits())?;
        }
        if self
            .state
            .last_position()
            .is_some_and(|p| p.collision_height != height)
            || self
                .state
                .cached_query()
                .is_some_and(|q| q.collision_height != height)
        {
            return Err(NativeError::InvalidState);
        }
        Ok(())
    }
    pub(super) fn validate_optional(
        state: Option<&Self>,
        profile: &Profile,
        owner: &State,
    ) -> Result<(), NativeError> {
        match (state, profile.player_environment.as_ref()) {
            (None, None) => Ok(()),
            (Some(cache), Some(_)) => {
                cache.validate(profile, owner.world, owner.player)?;
                // Exactly one countdown per committed frame; at most one phase per input.
                if cache.updated_tick != owner.tick
                    || cache.sequence < owner.tick
                    || cache.sequence > owner.tick.saturating_add(owner.next_sequence)
                    || (cache.last_pose.is_none() && cache.sequence != owner.tick)
                    || (cache.last_pose.is_some() && cache.sequence == owner.tick)
                    || cache.state.timer_ms() > 100_u32.saturating_sub(profile.tick_ms)
                {
                    return Err(NativeError::InvalidState);
                }
                let pose = cache.last_pose.unwrap_or(motion::Pose {
                    position: profile.player.start,
                    orientation: ai::Real::from_bits(profile.player.orientation.bits())?,
                });
                if pose.position != owner.position
                    || pose.orientation.bits() != owner.orientation.bits()
                {
                    return Err(NativeError::InvalidState);
                }
                Ok(())
            }
            _ => Err(NativeError::InvalidState),
        }
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn restore(
        snapshot: Option<PlayerEnvironmentSnapshot>,
        profile: &Profile,
    ) -> Result<Option<Self>, NativeError> {
        match (snapshot, profile.player_environment.as_ref()) {
            (None, None) => Ok(None),
            (Some(s), Some(source)) => {
                let state = s
                    .state
                    .restore(source.snapshot, &source.catalog, s.sequence)?;
                state.validate_native_lifecycle(&source.catalog)?;
                Ok(Some(Self {
                    owner: s.owner,
                    sequence: s.sequence,
                    updated_tick: s.updated_tick,
                    state,
                    last_pose: s.last_pose,
                }))
            }
            _ => Err(NativeError::InvalidCheckpoint),
        }
    }
    pub(super) fn query_for_position(
        &self,
        profile: &Profile,
        pose: motion::Pose,
    ) -> Result<Option<PlayerEnvironmentQuery>, NativeError> {
        let source = profile
            .player_environment
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let environment_pose = EnvironmentPose {
            position: point_bits(pose.position),
            collision_height: self.owner.height.height().bits(),
        };
        Ok(self
            .state
            .query_for_position(source.snapshot, self.sequence, environment_pose)?
            .map(|query| PlayerEnvironmentQuery {
                owner: self.owner.clone(),
                sequence: query.ticket,
                before: self.digest(),
                pose,
                query,
            }))
    }
    pub(super) fn apply(
        &mut self,
        profile: &Profile,
        tick: u64,
        phase: EnvironmentPhase,
        kind: PlayerEnvironmentPhase,
    ) -> Result<PlayerEnvironmentReceipt, NativeError> {
        let source = profile
            .player_environment
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        if self.updated_tick.checked_add(1) != Some(tick) || tick > crate::MAX_TICK {
            return Err(NativeError::InvalidState);
        }
        phase.validate_native_lifecycle(&source.catalog)?;
        let before = self.digest();
        let (state, sequence) =
            self.state
                .reduce(source.snapshot, &source.catalog, self.sequence, &phase)?;
        state.validate_native_lifecycle(&source.catalog)?;
        match kind {
            PlayerEnvironmentPhase::Position { pose, .. } => self.last_pose = Some(pose),
            PlayerEnvironmentPhase::Update { .. } => self.updated_tick = tick,
        }
        self.state = state;
        self.sequence = sequence;
        Ok(PlayerEnvironmentReceipt {
            sequence,
            before,
            after: self.digest(),
            phase: kind,
        })
    }
}
impl State {
    pub fn player_environment(&self) -> Option<&PlayerEnvironmentState> {
        self.source_player_environment.as_ref()
    }
}
fn point_bits(point: ai::Point) -> [u32; 3] {
    [point.x.bits(), point.y.bits(), point.z.bits()]
}

/// Exact post-relocation query, including owned height and the before-phase cache.
/// It is a serialized transcript, never proof that a caller owns the current frame.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerEnvironmentQuery {
    owner: EnvironmentOwner,
    sequence: u64,
    before: crate::Digest,
    pose: motion::Pose,
    query: EnvironmentQuery,
}
impl PlayerEnvironmentQuery {
    pub fn liquid_query(&self) -> EnvironmentQuery {
        self.query
    }
    pub fn snapshot_binding(&self) -> EnvironmentSnapshotBinding {
        self.query.snapshot
    }
    pub fn height_witness(&self) -> &PlayerCollisionHeightWitness {
        &self.owner.height
    }
    pub fn pose(&self) -> motion::Pose {
        self.pose
    }
    pub fn phase_sequence(&self) -> u64 {
        self.sequence
    }
    pub fn before_digest(&self) -> crate::Digest {
        self.before
    }
    pub fn validate_for(
        &self,
        profile: &Profile,
        world: crate::WorldId,
        player: crate::PlayerId,
    ) -> Result<(), NativeError> {
        profile.validate()?;
        self.owner.validate(profile, world, player)?;
        let source = profile
            .player_environment
            .as_ref()
            .ok_or(NativeError::AuthorityBinding)?;
        self.pose.position.distance_squared(self.pose.position)?;
        Orientation::from_bits(self.pose.orientation.bits())?;
        if self.sequence == 0
            || self.before == [0; 32]
            || self.query.ticket != self.sequence
            || self.query.snapshot != source.snapshot
            || self.query.required_type != 0x0f
            || self.query.position != point_bits(self.pose.position)
            || self.query.collision_height != self.owner.height.height().bits()
        {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerEnvironmentFacts {
    query: Box<PlayerEnvironmentQuery>,
    result: BoundLiquidResult,
}
impl PlayerEnvironmentFacts {
    pub fn new(
        query: &PlayerEnvironmentQuery,
        result: BoundLiquidResult,
    ) -> Result<Self, NativeError> {
        let facts = Self {
            query: Box::new(query.clone()),
            result,
        };
        facts.validate_for(query)?;
        Ok(facts)
    }
    pub fn query(&self) -> &PlayerEnvironmentQuery {
        &self.query
    }
    pub fn result(&self) -> BoundLiquidResult {
        self.result
    }
    pub fn validate_for(&self, query: &PlayerEnvironmentQuery) -> Result<(), NativeError> {
        if self.query.as_ref() != query || self.result.query != query.query {
            return Err(NativeError::AuthorityBinding);
        }
        self.result.validate(query.query)?;
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum PlayerEnvironmentPhase {
    Position {
        input_sequence: u64,
        pose: motion::Pose,
        refreshed: bool,
    },
    Update {
        diff_ms: u32,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerEnvironmentReceipt {
    pub sequence: u64,
    pub before: crate::Digest,
    pub after: crate::Digest,
    pub phase: PlayerEnvironmentPhase,
}

/// Synchronous player callbacks can start Chase before Player::Update's countdown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerEnvironmentMoment {
    BeforeCountdown,
    AfterCountdown,
}

/// Frozen source Player::IsInWater cache at a route callback. cached_query may name
/// an older position by design; never substitute a fresh liquid query at target.pose.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerEnvironmentTarget {
    tick: u64,
    moment: PlayerEnvironmentMoment,
    snapshot: PlayerEnvironmentSnapshot,
    digest: crate::Digest,
    in_water: bool,
    target: motion::Target,
}
impl PlayerEnvironmentTarget {
    pub(super) fn new(
        profile: &Profile,
        owner: &State,
        target: motion::Target,
    ) -> Result<Option<Self>, NativeError> {
        let state = match (
            &owner.source_player_environment,
            &profile.player_environment,
        ) {
            (None, None) => return Ok(None),
            (Some(state), Some(_)) => state,
            _ => return Err(NativeError::InvalidState),
        };
        let tick = owner.tick;
        let moment = if state.updated_tick == tick {
            PlayerEnvironmentMoment::AfterCountdown
        } else if state.updated_tick.checked_add(1) == Some(tick) {
            PlayerEnvironmentMoment::BeforeCountdown
        } else {
            return Err(NativeError::InvalidState);
        };
        let witness = Self {
            tick,
            moment,
            snapshot: state.snapshot(),
            digest: state.digest(),
            in_water: state.state.flags() & 0x01 != 0,
            target,
        };
        witness.validate(profile, owner.world, owner.player, &target, owner.now_ms)?;
        Ok(Some(witness))
    }
    pub fn moment(&self) -> PlayerEnvironmentMoment {
        self.moment
    }
    pub fn tick(&self) -> u64 {
        self.tick
    }
    pub fn in_water(&self) -> bool {
        self.in_water
    }
    pub fn phase_sequence(&self) -> u64 {
        self.snapshot.sequence
    }
    pub fn cache_digest(&self) -> crate::Digest {
        self.digest
    }
    pub fn target(&self) -> &motion::Target {
        &self.target
    }
    fn validate(
        &self,
        profile: &Profile,
        world: crate::WorldId,
        player: crate::PlayerId,
        target: &motion::Target,
        now_ms: u64,
    ) -> Result<(), NativeError> {
        let source =
            profile
                .player_environment
                .as_ref()
                .ok_or(NativeError::AuthorityUnavailable(
                    "cached player water state",
                ))?;
        let state = PlayerEnvironmentState {
            owner: self.snapshot.owner.clone(),
            sequence: self.snapshot.sequence,
            updated_tick: self.snapshot.updated_tick,
            last_pose: self.snapshot.last_pose,
            state: self.snapshot.state.clone().restore(
                source.snapshot,
                &source.catalog,
                self.snapshot.sequence,
            )?,
        };
        state.validate(profile, world, player)?;
        let expected_ms = self
            .tick
            .checked_mul(u64::from(profile.tick_ms))
            .and_then(|elapsed| profile.clock_origin_ms.checked_add(elapsed))
            .ok_or(NativeError::AuthorityBinding)?;
        let countdowns = match self.moment {
            PlayerEnvironmentMoment::BeforeCountdown => self
                .tick
                .checked_sub(1)
                .ok_or(NativeError::AuthorityBinding)?,
            PlayerEnvironmentMoment::AfterCountdown => self.tick,
        };
        let max_sequence = self
            .tick
            .saturating_mul(MAX_INPUTS_PER_TICK as u64)
            .saturating_add(countdowns);
        let max_timer = match self.moment {
            PlayerEnvironmentMoment::BeforeCountdown => 100,
            PlayerEnvironmentMoment::AfterCountdown => 100_u32.saturating_sub(profile.tick_ms),
        };
        if self.tick == 0
            || self.tick > crate::MAX_TICK
            || expected_ms != now_ms
            || state.updated_tick != countdowns
            || state.sequence < countdowns
            || state.sequence > max_sequence
            || (state.last_pose.is_none() && state.sequence != countdowns)
            || (state.last_pose.is_some() && state.sequence == countdowns)
            || state.state.timer_ms() > max_timer
        {
            return Err(NativeError::AuthorityBinding);
        }
        let last_pose = state.last_pose.unwrap_or(motion::Pose {
            position: profile.player.start,
            orientation: ai::Real::from_bits(profile.player.orientation.bits())?,
        });
        if last_pose != target.pose {
            return Err(NativeError::AuthorityBinding);
        }
        if self.digest != state.digest()
            || self.in_water != (state.state.flags() & 0x01 != 0)
            || &self.target != target
            || target.id != SOLO_UNIT
            || target.collision_height.bits() != state.owner.height.height().bits()
        {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
}
impl CreatureRouteRequest {
    /// Dependency for Chase's final target-water branch; does not implement its full gate.
    pub fn require_target_water(
        &self,
        profile: &Profile,
        world: crate::WorldId,
        player: crate::PlayerId,
    ) -> Result<&PlayerEnvironmentTarget, NativeError> {
        self.owner.validate_for(profile, self.owner.npc())?;
        if !matches!(
            self.request.kind,
            motion::RouteKind::ChaseInitial | motion::RouteKind::ChaseReplan
        ) {
            return Err(NativeError::AuthorityUnavailable(
                "route has no Chase target-water branch",
            ));
        }
        self.require_target_collision_height(profile, world, player)?;
        let target = self
            .request
            .target
            .as_ref()
            .ok_or(NativeError::AuthorityBinding)?;
        let cache = self
            .target_player_water
            .as_ref()
            .ok_or(NativeError::AuthorityUnavailable(
                "cached player water state",
            ))?;
        cache.validate(profile, world, player, target, self.request.binding.now_ms)?;
        Ok(cache)
    }
}
impl From<environment::EnvironmentError> for NativeError {
    fn from(error: environment::EnvironmentError) -> Self {
        Self::PlayerEnvironment(error)
    }
}
