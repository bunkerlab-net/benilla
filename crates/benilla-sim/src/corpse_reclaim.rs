// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
//! Ordinary PvE reclaim-handler guards and delegated callback order only.
//! This does not create ghosts/corpses, resurrect a player, or grant native admission.

use crate::creature_ai::{Point, Real};

pub const CORPSE_RECLAIM_POLICY: &str = "cmangos-8ec338a1-pve-reclaim-call-plan-v1";
pub const RECLAIM_RADIUS: f32 = 39.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Location {
    /// Host-owned identity of the actual map instance, not merely a map ID.
    pub map_instance: u64,
    pub in_world: bool,
    pub position: [f32; 3],
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Player {
    pub id: u64,
    pub alive: bool,
    pub ghost: bool,
    pub location: Location,
    pub combat_reach: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RegisteredCorpse {
    pub id: u64,
    pub owner: u64,
    pub location: Location,
    pub ghost_time_seconds: u64,
    /// Only ordinary resurrectable PvE corpses are supported by this policy.
    pub kind: CorpseKind,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CorpseKind {
    ResurrectablePve,
    ResurrectablePvp,
    Bones,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clocks {
    /// The time sampled inside Player::GetCorpseReclaimDelay.
    pub delay_now_seconds: u64,
    /// The separate time sampled by the handler's deadline comparison.
    pub comparison_now_seconds: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    pve_delay_enabled: bool,
}
impl Policy {
    /// Explicit ordinary-world evidence. This validates operands, not their origin.
    pub fn checked(
        battleground: Option<bool>,
        pve_delay_enabled: Option<bool>,
    ) -> Result<Self, Error> {
        match (battleground, pve_delay_enabled) {
            (Some(false), Some(pve_delay_enabled)) => Ok(Self { pve_delay_enabled }),
            (Some(true), _) => Err(Error::UnsupportedBattleground),
            _ => Err(Error::MissingPolicyEvidence),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Input {
    pub policy: Policy,
    pub player: Player,
    /// The player's registered corpse, never resolved from the packet's ignored GUID.
    pub corpse: Option<RegisteredCorpse>,
    pub death_expire_time_seconds: u64,
    pub clocks: Clocks,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    Alive,
    NotGhost,
    MissingCorpse,
    Delay,
    MapOrDistance,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    MissingPolicyEvidence,
    UnsupportedBattleground,
    UnsupportedCorpse,
    InvalidIdentity,
    WrongCorpseOwner,
    InvalidLocation,
    InvalidReach,
    ClockDomain,
    ClockOverflow,
    ArithmeticDomain,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Delegate the WHOLE source callback, including aura/zone/visibility work.
    /// It is not permission to set health alone. No callback executes here.
    ResurrectPlayer {
        restore_percent_bits: u32,
        apply_sickness: bool,
    },
    /// Delegate registered-corpse retirement and conditional visual bones/save.
    SpawnCorpseBones,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    player: u64,
    corpse: u64,
    actions: [Action; 2],
}
impl Plan {
    pub fn player(&self) -> u64 {
        self.player
    }
    pub fn corpse(&self) -> u64 {
        self.corpse
    }
    pub fn actions(&self) -> &[Action; 2] {
        &self.actions
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Rejected(Rejection),
    /// Guard success only. No recovery or durable transition has happened.
    Calls(Plan),
}

fn location(value: Location) -> Result<Point, Error> {
    if value.map_instance == 0 {
        return Err(Error::InvalidLocation);
    }
    let [x, y, z] = value.position;
    Point::new(x, y, z).map_err(|_| Error::InvalidLocation)
}

/// Mirrors uint32(expiration - min(now, expiration)) before division, not after.
fn delay(expiration: u64, now: u64, enabled: bool) -> u32 {
    let count = if enabled {
        (expiration.saturating_sub(now) as u32) / 300
    } else {
        0
    };
    [30, 60, 120][count.min(2) as usize]
}

/// Evaluate a source-shaped observation; this is not a persistent world reducer.
/// Later operands are inspected only if their source guard is reached. A future host must authenticate the
/// registry, freshness and all delegated callbacks; this module cannot do so.
pub fn prepare(input: Input) -> Result<Decision, Error> {
    if input.player.id == 0 {
        return Err(Error::InvalidIdentity);
    }
    let rejected = |reason| Ok(Decision::Rejected(reason));
    if input.player.alive {
        return rejected(Rejection::Alive);
    }
    if !input.player.ghost {
        return rejected(Rejection::NotGhost);
    }
    let Some(corpse) = input.corpse else {
        return rejected(Rejection::MissingCorpse);
    };
    if corpse.id == 0 {
        return Err(Error::InvalidIdentity);
    }
    if corpse.owner != input.player.id {
        return Err(Error::WrongCorpseOwner);
    }
    if corpse.kind != CorpseKind::ResurrectablePve {
        return Err(Error::UnsupportedCorpse);
    }
    let Clocks {
        delay_now_seconds,
        comparison_now_seconds,
    } = input.clocks;
    if [
        delay_now_seconds,
        comparison_now_seconds,
        input.death_expire_time_seconds,
        corpse.ghost_time_seconds,
    ]
    .into_iter()
    .any(|time| time > i64::MAX as u64)
    {
        return Err(Error::ClockDomain);
    }
    let deadline = corpse
        .ghost_time_seconds
        .checked_add(u64::from(delay(
            input.death_expire_time_seconds,
            delay_now_seconds,
            input.policy.pve_delay_enabled,
        )))
        .filter(|time| *time <= i64::MAX as u64)
        .ok_or(Error::ClockOverflow)?;
    if deadline > comparison_now_seconds {
        return rejected(Rejection::Delay);
    }
    if !corpse.location.in_world
        || !input.player.location.in_world
        || corpse.location.map_instance != input.player.location.map_instance
    {
        return rejected(Rejection::MapOrDistance);
    }
    let player_position = location(input.player.location)?;
    let corpse_position = location(corpse.location)?;
    let player_reach = Real::new(input.player.combat_reach).map_err(|_| Error::InvalidReach)?;
    player_reach
        .nonnegative("player combat reach")
        .map_err(|_| Error::InvalidReach)?;
    let in_range = || -> Result<bool, crate::creature_ai::AiError> {
        // Corpse inherits WorldObject::GetCombatReach() == 0.f. Keep grouping.
        let size_factor = Real::ZERO.add(player_reach)?;
        let maximum_distance = Real::new(RECLAIM_RADIUS)?.add(size_factor)?;
        Ok(corpse_position
            .distance_squared(player_position)?
            .less(maximum_distance.mul(maximum_distance)?))
    };
    if !in_range().map_err(|_| Error::ArithmeticDomain)? {
        return rejected(Rejection::MapOrDistance);
    }
    Ok(Decision::Calls(Plan {
        player: input.player.id,
        corpse: corpse.id,
        actions: [
            Action::ResurrectPlayer {
                restore_percent_bits: 0.5f32.to_bits(),
                apply_sickness: false,
            },
            Action::SpawnCorpseBones,
        ],
    }))
}
