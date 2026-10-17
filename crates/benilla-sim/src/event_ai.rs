// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f.
// Copyright CMaNGOS Project contributors; see ../licenses/AUTHORS.md.
// Source GPL-2.0-or-later; Rust translation GPL-3.0-or-later. WITHOUT ANY WARRANTY.
//! Bounded ordinary EventAI AGGRO -> TEXT holder and random-word reducer.
//!
//! The host proves type AGGRO, zero phase mask/flags/event parameters, exactly
//! one TEXT action followed by two NONE actions, complete ordered source rows,
//! and a genuine EnterCombat callback. This module neither admits source rows
//! nor detects combat transitions. Owner/generation, catalog identity, callback
//! order, target resolution and broadcast delivery remain host responsibilities.
//! See the EventAI section of ../PROVENANCE.md for source hashes and exclusions.

use serde::{Deserialize, Serialize};

pub const AGGRO_TEXT_POLICY_ID: &str = "cmangos-8ec338a1-aggro-text-full32-v1";
pub const AGGRO_TEXT_SNAPSHOT_VERSION: u32 = 1;

/// Validated parameters of the one supported TEXT action; no database content.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct AggroText {
    event_id: u32,
    chance: u8,
    text_ids: [i32; 3],
    text_count: u8,
}

impl AggroText {
    pub fn new(event_id: u32, chance: u8, text_ids: [i32; 3]) -> Result<Self, Error> {
        if event_id == 0 {
            return Err(Error::Invalid("zero event identity"));
        }
        if !(1..=100).contains(&chance) {
            return Err(Error::Invalid("chance outside 1..=100"));
        }
        let mut text_count = 0;
        let mut ended = false;
        for text_id in text_ids {
            if text_id < 0 || (ended && text_id != 0) {
                return Err(Error::Invalid(
                    "text IDs must be contiguous positive int32 values",
                ));
            }
            if text_id == 0 {
                ended = true;
            } else {
                text_count += 1;
            }
        }
        if text_count == 0 {
            return Err(Error::Invalid("TEXT requires a positive first ID"));
        }
        Ok(Self {
            event_id,
            chance,
            text_ids,
            text_count,
        })
    }

    pub fn event_id(&self) -> u32 {
        self.event_id
    }

    pub fn chance(&self) -> u8 {
        self.chance
    }

    pub fn text_ids(&self) -> [i32; 3] {
        self.text_ids
    }
}

/// Each request consumes one independent, full unsigned 32-bit source word.
/// Never substitute a bounded 0..99 draw or reuse the chance word for Action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawRequest {
    Chance,
    Action,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChanceResult {
    Failed,
    ActionRequired,
}

/// Selection occurs even when the host subsequently finds zero listeners.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct TextSelection {
    pub event_id: u32,
    pub text_id: u32,
    /// Zero-based parameter index of the TEXT action, before broadcast lookup.
    pub text_slot: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid(&'static str),
    WrongDrawStage {
        expected: Option<DrawRequest>,
        supplied: DrawRequest,
    },
    InProgress,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "source AGGRO TEXT: {self:?}")
    }
}

impl std::error::Error for Error {}

/// Explicit source holder fields at a quiescent boundary. The outer snapshot
/// must also bind the exact immutable catalog and NPC generation before restore.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AggroTextHolderSnapshot {
    pub version: u32,
    pub event_id: u32,
    pub timer_ms: u32,
    pub enabled: bool,
    pub in_progress: bool,
    /// Always null for AGGRO. A supplied value, including zero, is invalid.
    #[serde(deserialize_with = "required_event_target")]
    pub event_target: Option<u64>,
}

// Without an explicit deserializer serde accepts an absent Option as None,
// which would silently reconstruct an omitted source holder member.
fn required_event_target<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u64>, D::Error> {
    Option::<u64>::deserialize(deserializer)
}

/// Compact, private holder representation: timer=0, enabled=true and
/// eventTarget=null are invariants of this bounded action/lifecycle domain.
/// pending_draw is exactly the source inProgress guard plus its local RNG stage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AggroTextHolder {
    event: AggroText,
    pending_draw: Option<DrawRequest>,
}

impl AggroTextHolder {
    /// InitAI constructs holders but does not execute AGGRO.
    pub fn new(event: AggroText) -> Self {
        Self {
            event,
            pending_draw: None,
        }
    }

    pub fn event(&self) -> &AggroText {
        &self.event
    }

    pub fn pending_draw(&self) -> Option<DrawRequest> {
        self.pending_draw
    }

    /// Call only inside a genuine host EnterCombat callback, after its base AI
    /// work. Already-in-combat attacks and periodic updates must not call this.
    /// EnterCombat enables AGGRO, then CheckEvent rejects an in-progress holder.
    pub fn begin_enter_combat(&mut self) -> Option<DrawRequest> {
        if self.pending_draw.is_some() {
            return None;
        }
        self.pending_draw = Some(DrawRequest::Chance);
        self.pending_draw
    }

    /// ProcessEvent draws before testing `chance <= word % 100`. ResetEvent
    /// neither disables nor adds a timer for AGGRO, including after failure.
    pub fn consume_chance(&mut self, word: u32) -> Result<ChanceResult, Error> {
        self.require_draw(DrawRequest::Chance)?;
        if u32::from(self.event.chance) <= word % 100 {
            self.pending_draw = None;
            Ok(ChanceResult::Failed)
        } else {
            self.pending_draw = Some(DrawRequest::Action);
            Ok(ChanceResult::ActionRequired)
        }
    }

    /// Every passed chance consumes this second word, even with one text ID.
    /// The two trailing NONE actions consume nothing. Audience does not affect
    /// this operation; a lookup/delivery failure must roll back the host frame.
    pub fn consume_action(&mut self, word: u32) -> Result<TextSelection, Error> {
        self.require_draw(DrawRequest::Action)?;
        let text_slot = (word % u32::from(self.event.text_count)) as u8;
        let selection = TextSelection {
            event_id: self.event.event_id,
            text_id: self.event.text_ids[usize::from(text_slot)] as u32,
            text_slot,
        };
        self.pending_draw = None;
        Ok(selection)
    }

    fn require_draw(&self, supplied: DrawRequest) -> Result<(), Error> {
        if self.pending_draw != Some(supplied) {
            return Err(Error::WrongDrawStage {
                expected: self.pending_draw,
                supplied,
            });
        }
        Ok(())
    }

    /// Ordinary Reset explicitly skips AGGRO; it cannot clear inProgress.
    pub fn reset(&mut self) {}

    /// No REACHED_HOME action exists in this domain; its Reset skips AGGRO.
    pub fn reached_home(&mut self) {
        self.reset();
    }

    /// No DEATH action exists in this domain; its Reset skips AGGRO.
    pub fn died(&mut self) {
        self.reset();
    }

    /// JustRespawned calls InitAI and reconstructs this holder. The host must
    /// independently bind the replacement NPC generation and discard old work.
    pub fn respawn(&mut self) {
        *self = Self::new(self.event);
    }

    pub fn validate_quiescent(&self) -> Result<(), Error> {
        if self.pending_draw.is_some() {
            return Err(Error::InProgress);
        }
        Ok(())
    }

    /// Partial callbacks cannot be persisted as committed holder continuation.
    pub fn snapshot(&self) -> Result<AggroTextHolderSnapshot, Error> {
        self.validate_quiescent()?;
        Ok(AggroTextHolderSnapshot {
            version: AGGRO_TEXT_SNAPSHOT_VERSION,
            event_id: self.event.event_id,
            timer_ms: 0,
            enabled: true,
            in_progress: false,
            event_target: None,
        })
    }

    /// Reject corrupt/unsupported source state rather than normalizing it. The
    /// event must come from the host's independently validated immutable catalog.
    pub fn restore(event: AggroText, snapshot: AggroTextHolderSnapshot) -> Result<Self, Error> {
        if snapshot.version != AGGRO_TEXT_SNAPSHOT_VERSION {
            return Err(Error::Invalid("holder snapshot version"));
        }
        if snapshot.event_id != event.event_id {
            return Err(Error::Invalid("holder event identity"));
        }
        if snapshot.timer_ms != 0 || !snapshot.enabled || snapshot.event_target.is_some() {
            return Err(Error::Invalid("noncanonical AGGRO holder"));
        }
        if snapshot.in_progress {
            return Err(Error::InProgress);
        }
        Ok(Self::new(event))
    }
}

#[cfg(test)]
mod tests;
