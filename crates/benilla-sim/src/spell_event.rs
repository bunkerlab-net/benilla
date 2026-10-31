// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded scheduling decisions from CMaNGOS Classic SpellEvent::Execute.
//!
//! Source: `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, Spell.cpp:7149-7233.
//! Call after the host's Spell::update with the entire Unit diff. This module
//! neither updates a spell nor implements its effects, cancellation, ownership,
//! Unit phases, or EventProcessor dispatch. See tools/spell_event_source/README.md.

/// State observed by SpellEvent after the host's spell update.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpellEventState {
    Finished {
        deletable: bool,
    },
    Casting,
    /// Only nonchanneled travel belongs to this bounded helper.
    Traveling {
        /// None and Some(0) both denote the source's zero/unstarted sentinel.
        delay_start_ms: Option<u64>,
        delay_moment_ms: u64,
    },
    /// Channeled travel and channeling require unrepresented host behavior.
    Channeled,
}

/// Decision to apply to the currently detached event in its queue transaction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpellEventPlan {
    Delete,
    ReAdd {
        deadline_ms: u64,
        /// Assign the source delay-start member only when Some; otherwise retain it.
        /// Some(0) deliberately preserves the source's unstarted sentinel.
        set_delay_start_ms: Option<u64>,
    },
    /// Invoke the host delayed handler once, then call [`plan_after_delayed`].
    HandleDelayed {
        elapsed_ms: u64,
    },
}

/// Fail-closed restrictions rather than wrapped arithmetic or missing host code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpellEventError {
    TimeOverflow,
    DelayStartInFuture,
    DelayNotStarted,
    ChanneledUnsupported,
}

impl std::fmt::Display for SpellEventError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::TimeOverflow => "spell event clock arithmetic overflow",
            Self::DelayStartInFuture => "spell event delay start is in the future",
            Self::DelayNotStarted => "spell event delayed handler requires a nonzero delay start",
            Self::ChanneledUnsupported => "channeled spell event host is unsupported",
        })
    }
}

impl std::error::Error for SpellEventError {}

fn next_update(now_ms: u64) -> Result<SpellEventPlan, SpellEventError> {
    Ok(SpellEventPlan::ReAdd {
        deadline_ms: now_ms.checked_add(1).ok_or(SpellEventError::TimeOverflow)?,
        set_delay_start_ms: None,
    })
}

/// Plan the source branch after Spell::update; do not call that host update here.
///
/// Re-adds must preserve BasicEvent::m_addTime (`set_addtime = false`). A deadline
/// at or before now remains eligible in the same EventProcessor update. Neither
/// the callback's scheduled deadline nor lateness replaces `now_ms`.
pub fn plan_after_update(
    now_ms: u64,
    state: SpellEventState,
) -> Result<SpellEventPlan, SpellEventError> {
    match state {
        SpellEventState::Finished { deletable: true } => Ok(SpellEventPlan::Delete),
        SpellEventState::Finished { deletable: false } | SpellEventState::Casting => {
            next_update(now_ms)
        }
        SpellEventState::Traveling {
            delay_start_ms,
            delay_moment_ms,
        } => {
            if let Some(start) = delay_start_ms.filter(|start| *start != 0) {
                Ok(SpellEventPlan::HandleDelayed {
                    elapsed_ms: now_ms
                        .checked_sub(start)
                        .ok_or(SpellEventError::DelayStartInFuture)?,
                })
            } else {
                Ok(SpellEventPlan::ReAdd {
                    deadline_ms: now_ms
                        .checked_add(delay_moment_ms)
                        .ok_or(SpellEventError::TimeOverflow)?,
                    set_delay_start_ms: Some(now_ms),
                })
            }
        }
        SpellEventState::Channeled => Err(SpellEventError::ChanneledUnsupported),
    }
}

/// Plan after exactly one nonchanneled `handle_delayed(elapsed_ms)` call.
///
/// A nonzero returned offset is relative to the first callback's delay start,
/// even when the resulting deadline is already due. Zero schedules now+1 even
/// when the handler just finished the spell; deletion is a later callback.
pub fn plan_after_delayed(
    now_ms: u64,
    delay_start_ms: u64,
    next_offset_ms: u64,
) -> Result<SpellEventPlan, SpellEventError> {
    if delay_start_ms == 0 {
        return Err(SpellEventError::DelayNotStarted);
    }
    if delay_start_ms > now_ms {
        return Err(SpellEventError::DelayStartInFuture);
    }
    if next_offset_ms == 0 {
        next_update(now_ms)
    } else {
        Ok(SpellEventPlan::ReAdd {
            deadline_ms: delay_start_ms
                .checked_add(next_offset_ms)
                .ok_or(SpellEventError::TimeOverflow)?,
            set_delay_start_ms: None,
        })
    }
}
