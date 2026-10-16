// SPDX-License-Identifier: GPL-3.0-or-later
//! Shared cached player environment reducer and standalone journal. See ../ENVIRONMENT.md.
//! CMaNGOS Classic 8ec338a1 Player.cpp:1105–1140,1483–1489,6069–6075,19113–19175,
//! copyright CMaNGOS contributors, upstream GPL-2.0-or-later.
//! This is the environment phase AFTER accepted relocation, not full SetPosition.
//! NativeWorld selects its narrower callback-free owner explicitly; gameplay admission stays closed.
mod catalog;
pub mod mirror;
mod persistence;
mod reducer;
pub use catalog::{LiquidCatalog, LiquidCatalogError, LiquidTypeRow};
use mirror::{MirrorTimer, MirrorTimerType};
pub use persistence::{EnvironmentCheckpoint, EnvironmentRecord};
pub(crate) use reducer::{EnvironmentPhase, EnvironmentStateSnapshot};
use serde::{Deserialize, Serialize};

pub const ENVIRONMENT_POLICY: &str = "cmangos-8ec338a1-player-environment-component-mirror-v2";
pub const MAX_ENVIRONMENT_RECORDS: usize = 2048;
pub const MAX_ENVIRONMENT_BYTES: usize = 4 * 1024 * 1024;
const REFRESH_MS: u32 = 100;
const ALL_LIQUIDS: u8 = 0x0f;
const IN_WATER: u8 = 0x01;
const IN_MAGMA: u8 = 0x02;
const IN_SLIME: u8 = 0x04;
const HIGH_SEA: u8 = 0x08;
const UNDERWATER: u8 = 0x10;
const HIGH_LIQUID: u8 = 0x20;
const LIQUID: u8 = 0x40;
const HAZARD: u8 = IN_MAGMA | IN_SLIME;
const IN_LIQUID: u8 = IN_WATER | HAZARD;
const LIQUID_FLAGS: u8 = 0x7f;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnvironmentError {
    InvalidInput,
    Binding,
    InvalidCheckpoint,
    ReplayMismatch,
    TooLarge,
    JournalFull,
    Limit,
    /// No production executor is present for this exact source-ordered operation.
    CallbacksUnavailable(EnvironmentCallback),
}
impl std::fmt::Display for EnvironmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "player environment component: {self:?}")
    }
}
impl std::error::Error for EnvironmentError {}

/// Inert equality binding to an already-published TerrainInfo result and complete DBC.
/// Supplied hashes establish consistency only, never authenticity or admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentSnapshotBinding {
    pub map: u32,
    pub content: crate::Digest,
    pub source_epoch: crate::Digest,
    pub inventory: crate::Digest,
    pub policy: crate::Digest,
    pub liquid_catalog: crate::Digest,
}
impl EnvironmentSnapshotBinding {
    fn validate(self, catalog: &LiquidCatalog) -> Result<(), EnvironmentError> {
        if self.map > 999
            || [
                self.content,
                self.source_epoch,
                self.inventory,
                self.policy,
                self.liquid_catalog,
            ]
            .contains(&[0; 32])
            || self.liquid_catalog != catalog.digest()
            || !catalog.is_complete()
        {
            return Err(EnvironmentError::Binding);
        }
        Ok(())
    }
}

/// Actual post-relocation source-world binary32 fields. Orientation and relocation
/// effects are outside this component; an unchanged pose is still a valid call.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentPose {
    pub position: [u32; 3],
    pub collision_height: u32,
}
impl EnvironmentPose {
    fn validate(self) -> Result<(), EnvironmentError> {
        if self.position.iter().any(|b| {
            let f = f32::from_bits(*b);
            !f.is_finite()
        }) || !f32::from_bits(self.collision_height).is_finite()
        {
            return Err(EnvironmentError::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentQuery {
    pub snapshot: EnvironmentSnapshotBinding,
    pub ticket: u64,
    pub position: [u32; 3],
    pub required_type: u8,
    pub collision_height: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum LiquidStatus {
    NoWater = 0,
    AboveWater = 1,
    WaterWalk = 2,
    InWater = 4,
    UnderWater = 8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiquidData {
    pub entry: u32,
    pub type_flags: u32,
    pub level: u32,
    pub depth_level: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundLiquidResult {
    pub query: EnvironmentQuery,
    pub status: LiquidStatus,
    pub data: Option<LiquidData>,
}
impl BoundLiquidResult {
    pub(crate) fn validate(self, expected: EnvironmentQuery) -> Result<(), EnvironmentError> {
        if self.query != expected {
            return Err(EnvironmentError::Binding);
        }
        match (self.status, self.data) {
            (LiquidStatus::NoWater, None) => Ok(()),
            (LiquidStatus::NoWater, Some(_)) | (_, None) => Err(EnvironmentError::InvalidInput),
            (_, Some(data))
                if [data.level, data.depth_level]
                    .iter()
                    .all(|b| f32::from_bits(*b).is_finite()) =>
            {
                Ok(())
            }
            _ => Err(EnvironmentError::InvalidInput),
        }
    }
}

/// Source callback intent, not a receipt claiming that an effect happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnvironmentCallback {
    UpdateThreatTables,
    RemoveInterruptAuras {
        mask: u32,
    },
    SetMirrorScale {
        timer: EnvironmentMirror,
        scale: i32,
    },
    RemoveSpellAuras {
        spell_id: u32,
    },
    ReadHasAura {
        spell_id: u32,
    },
    ReadWaterBreathingInterval,
    CastSelf {
        spell_id: u32,
        triggered_flags: u32,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnvironmentMirror {
    Fatigue,
    Breath,
    Environmental,
}

// Private and non-extensible from callers: a Clone promise cannot guarantee rollback
// for an arbitrary external host with shared interior mutable state or side effects.
trait Callbacks {
    fn effect(&mut self, intent: EnvironmentCallback, flags: u8) -> Result<(), EnvironmentError>;
    fn has_aura(&mut self, spell_id: u32) -> Result<bool, EnvironmentError>;
    fn breathing_interval(&mut self) -> Result<u32, EnvironmentError>;
}
struct OwnedMirrorCallbacks<'a> {
    timers: &'a mut [MirrorTimer; 4],
}
impl Callbacks for OwnedMirrorCallbacks<'_> {
    fn effect(&mut self, intent: EnvironmentCallback, _: u8) -> Result<(), EnvironmentError> {
        if let EnvironmentCallback::SetMirrorScale { timer, scale } = intent {
            self.timers[mirror_index(timer)].set_scale(scale);
            Ok(())
        } else {
            Err(EnvironmentError::CallbacksUnavailable(intent))
        }
    }
    fn has_aura(&mut self, spell_id: u32) -> Result<bool, EnvironmentError> {
        Err(EnvironmentError::CallbacksUnavailable(
            EnvironmentCallback::ReadHasAura { spell_id },
        ))
    }
    fn breathing_interval(&mut self) -> Result<u32, EnvironmentError> {
        Err(EnvironmentError::CallbacksUnavailable(
            EnvironmentCallback::ReadWaterBreathingInterval,
        ))
    }
}

/// Read-only cached state. It is not deserializable or externally constructible.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvironmentState {
    flags: u8,
    timer_ms: u32,
    last_liquid: Option<u32>,
    last_position: Option<EnvironmentPose>,
    cached_query: Option<EnvironmentQuery>,
    mirrors: [MirrorTimer; 4],
}
fn initial_mirrors() -> [MirrorTimer; 4] {
    [
        MirrorTimerType::Fatigue,
        MirrorTimerType::Breath,
        MirrorTimerType::FeignDeath,
        MirrorTimerType::Environmental,
    ]
    .map(MirrorTimer::new)
}
fn mirror_index(timer: EnvironmentMirror) -> usize {
    match timer {
        EnvironmentMirror::Fatigue => 0,
        EnvironmentMirror::Breath => 1,
        EnvironmentMirror::Environmental => 3,
    }
}
impl EnvironmentState {
    /// Actual owned source timer states. This component only mutates their scale;
    /// starting/ticking timers and consuming expiration still need the player lifecycle.
    pub fn mirror(&self, timer: EnvironmentMirror) -> &MirrorTimer {
        &self.mirrors[mirror_index(timer)]
    }
    pub fn flags(&self) -> u8 {
        self.flags
    }
    pub fn timer_ms(&self) -> u32 {
        self.timer_ms
    }
    pub fn last_liquid(&self) -> Option<u32> {
        self.last_liquid
    }
    pub fn last_position(&self) -> Option<EnvironmentPose> {
        self.last_position
    }
    pub fn cached_query(&self) -> Option<EnvironmentQuery> {
        self.cached_query
    }

    fn update_timer(&mut self, diff_ms: u32) {
        self.timer_ms = self.timer_ms.saturating_sub(diff_ms);
    }
    fn refresh_if_due(
        &mut self,
        reply: BoundLiquidResult,
        catalog: &LiquidCatalog,
        host: &mut impl Callbacks,
    ) -> Result<bool, EnvironmentError> {
        if self.timer_ms != 0 {
            return Ok(false);
        }
        self.timer_ms = REFRESH_MS;
        self.refresh(reply, catalog, host)?;
        self.cached_query = Some(reply.query);
        Ok(true)
    }
    fn set_flags(
        &mut self,
        mask: u8,
        apply: bool,
        host: &mut impl Callbacks,
    ) -> Result<(), EnvironmentError> {
        if (self.flags & mask != 0) == apply {
            return Ok(());
        }
        if apply {
            self.flags |= mask;
        } else {
            self.flags &= !mask;
        }
        // Dispatch by REQUESTED mask, including all callbacks during combined dry clear.
        if mask & IN_LIQUID != 0 {
            host.effect(EnvironmentCallback::UpdateThreatTables, self.flags)?;
        }
        if mask & HIGH_LIQUID != 0 {
            host.effect(
                EnvironmentCallback::RemoveInterruptAuras {
                    mask: if apply { 0x80 } else { 0x100 },
                },
                self.flags,
            )?;
        }
        if mask & HIGH_SEA != 0 {
            host.effect(
                EnvironmentCallback::SetMirrorScale {
                    timer: EnvironmentMirror::Fatigue,
                    scale: if apply { -1 } else { 10 },
                },
                self.flags,
            )?;
        }
        if mask & UNDERWATER != 0 {
            let decreases = apply && host.breathing_interval()? != 0;
            host.effect(
                EnvironmentCallback::SetMirrorScale {
                    timer: EnvironmentMirror::Breath,
                    scale: if decreases { -1 } else { 10 },
                },
                self.flags,
            )?;
        }
        if mask & HAZARD != 0 {
            host.effect(
                EnvironmentCallback::SetMirrorScale {
                    timer: EnvironmentMirror::Environmental,
                    scale: if self.flags & HAZARD != 0 { -1 } else { 10 },
                },
                self.flags,
            )?;
        }
        Ok(())
    }

    fn refresh(
        &mut self,
        reply: BoundLiquidResult,
        catalog: &LiquidCatalog,
        host: &mut impl Callbacks,
    ) -> Result<(), EnvironmentError> {
        let last = self.last_liquid.and_then(|id| catalog.get(id));
        if reply.status == LiquidStatus::NoWater {
            self.set_flags(LIQUID_FLAGS, false, host)?;
            if let Some(row) = last.filter(|r| r.spell_id != 0) {
                host.effect(
                    EnvironmentCallback::RemoveSpellAuras {
                        spell_id: row.spell_id,
                    },
                    self.flags,
                )?;
            }
            self.last_liquid = None;
            return Ok(());
        }
        let data = reply.data.ok_or(EnvironmentError::InvalidInput)?;
        self.set_flags(LIQUID, true, host)?;
        if data.entry != 0 {
            let liquid = catalog.get(data.entry);
            if let Some(row) = last.filter(|r| r.spell_id != 0 && r.id != data.entry) {
                host.effect(
                    EnvironmentCallback::RemoveSpellAuras {
                        spell_id: row.spell_id,
                    },
                    self.flags,
                )?;
            }
            if let Some(row) = liquid.filter(|r| r.spell_id != 0) {
                if matches!(
                    reply.status,
                    LiquidStatus::UnderWater | LiquidStatus::InWater
                ) {
                    if !host.has_aura(row.spell_id)? {
                        host.effect(
                            EnvironmentCallback::CastSelf {
                                spell_id: row.spell_id,
                                triggered_flags: 1,
                            },
                            self.flags,
                        )?;
                    }
                } else {
                    host.effect(
                        EnvironmentCallback::RemoveSpellAuras {
                            spell_id: row.spell_id,
                        },
                        self.flags,
                    )?;
                }
            }
            self.last_liquid = liquid.map(|row| row.id);
        } else if let Some(row) = last.filter(|r| r.spell_id != 0) {
            host.effect(
                EnvironmentCallback::RemoveSpellAuras {
                    spell_id: row.spell_id,
                },
                self.flags,
            )?;
            self.last_liquid = None;
        }
        // Deliberately selective: a different liquid kind does not clear the old kind's bit.
        if data.type_flags & 0x0f != 0 {
            self.set_flags(UNDERWATER, reply.status == LiquidStatus::UnderWater, host)?;
        }
        if data.type_flags & 0x0a != 0 {
            self.set_flags(
                IN_WATER,
                matches!(
                    reply.status,
                    LiquidStatus::UnderWater | LiquidStatus::InWater
                ),
                host,
            )?;
        }
        let touching = matches!(
            reply.status,
            LiquidStatus::UnderWater | LiquidStatus::InWater | LiquidStatus::WaterWalk
        );
        if data.type_flags & 0x01 != 0 {
            self.set_flags(IN_MAGMA, touching, host)?;
        }
        if data.type_flags & 0x04 != 0 {
            self.set_flags(IN_SLIME, touching, host)?;
        }
        self.set_flags(HIGH_SEA, data.type_flags & 0x10 != 0, host)?;
        // Unsuffixed source 0.75 is DOUBLE: no binary32 intermediate rounding.
        let deep = f64::from(f32::from_bits(data.level))
            > f64::from(f32::from_bits(data.depth_level))
                + f64::from(f32::from_bits(reply.query.collision_height)) * 0.75_f64;
        self.set_flags(
            HIGH_LIQUID,
            matches!(
                reply.status,
                LiquidStatus::UnderWater | LiquidStatus::InWater
            ) && deep,
            host,
        )?;
        Ok(())
    }
}

/// Owned standalone environment state and journal, explicitly unconnected to NativeWorld.
/// Owned mirror scaling executes; other required callbacks fail atomically.
#[derive(Clone, Debug)]
pub struct EnvironmentMachine {
    snapshot: EnvironmentSnapshotBinding,
    world: crate::WorldId,
    player: crate::PlayerId,
    catalog: LiquidCatalog,
    state: EnvironmentState,
    sequence: u64,
    tip: crate::Digest,
    records: Vec<EnvironmentRecord>,
}
impl EnvironmentMachine {
    pub fn new(
        snapshot: EnvironmentSnapshotBinding,
        world: crate::WorldId,
        player: crate::PlayerId,
        catalog: LiquidCatalog,
    ) -> Result<Self, EnvironmentError> {
        let state = EnvironmentState::new(snapshot, &catalog)?;
        if world == [0; 16] || player.owner == [0; 16] || player.character == [0; 16] {
            return Err(EnvironmentError::InvalidInput);
        }
        Ok(Self {
            snapshot,
            world,
            player,
            catalog,
            state,
            sequence: 0,
            tip: [0; 32],
            records: vec![],
        })
    }
    pub fn state(&self) -> &EnvironmentState {
        &self.state
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn records(&self) -> &[EnvironmentRecord] {
        &self.records
    }

    /// A request is produced only for an eligible post-relocation call. Update expiry
    /// does not produce one; identical position does not suppress one.
    pub fn query_for_position(
        &self,
        pose: EnvironmentPose,
    ) -> Result<Option<EnvironmentQuery>, EnvironmentError> {
        self.state
            .query_for_position(self.snapshot, self.sequence, pose)
    }
    /// Player::Update timer fragment only. No terrain query or refresh on expiration.
    pub fn update_phase(&mut self, diff_ms: u32) -> Result<(), EnvironmentError> {
        self.transact(EnvironmentPhase::Update { diff_ms })
    }
    /// Environment-only phase after valid SetPosition relocation has completed.
    /// Cached calls require None; eligible calls require the exactly bound result.
    pub fn set_position_phase(
        &mut self,
        pose: EnvironmentPose,
        result: Option<BoundLiquidResult>,
    ) -> Result<(), EnvironmentError> {
        self.transact(EnvironmentPhase::Position {
            pose,
            result: result.map(Box::new),
        })
    }
    fn transact(&mut self, phase: EnvironmentPhase) -> Result<(), EnvironmentError> {
        if self.records.len() >= MAX_ENVIRONMENT_RECORDS {
            return Err(EnvironmentError::JournalFull);
        }
        let before = self.state_digest();
        let (next, sequence) =
            self.state
                .reduce(self.snapshot, &self.catalog, self.sequence, &phase)?;
        let record = EnvironmentRecord::new(self, sequence, phase, before, &next);
        let prospective_bytes = self
            .records
            .iter()
            .try_fold(2usize, |n, r| {
                n.checked_add(
                    serde_json::to_vec(r)
                        .map_err(|_| EnvironmentError::InvalidCheckpoint)?
                        .len()
                        + 1,
                )
                .ok_or(EnvironmentError::TooLarge)
            })?
            .checked_add(
                serde_json::to_vec(&record)
                    .map_err(|_| EnvironmentError::InvalidCheckpoint)?
                    .len()
                    + 1,
            )
            .ok_or(EnvironmentError::TooLarge)?;
        if prospective_bytes > MAX_ENVIRONMENT_BYTES {
            return Err(EnvironmentError::JournalFull);
        }
        self.state = next;
        self.sequence = sequence;
        self.tip = record.digest();
        self.records.push(record);
        Ok(())
    }
}

#[cfg(test)]
mod oracle_tests;
#[cfg(test)]
mod tests;
