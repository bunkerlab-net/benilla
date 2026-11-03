// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from CMaNGOS Classic 8ec338a1 CreatureEventAI.cpp:35-48,293-295,
// 575-662,1382-1452,1597-1632,1929-1975. Copyright CMaNGOS contributors.
//! OOC timer holders and the shared 500 ms EventAI update clock.
use super::*;

pub const EVENT_UPDATE_MS: u32 = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct OocText {
    text: AggroText,
    initial: [u32; 2],
    repeat: [u32; 2],
    repeatable: bool,
}
impl OocText {
    pub fn new(
        text: AggroText,
        initial: [u32; 2],
        repeat: [u32; 2],
        repeatable: bool,
    ) -> Result<Self, Error> {
        if initial[0] > initial[1] || repeat[0] > repeat[1] {
            return Err(Error::Invalid("timer maximum below minimum"));
        }
        Ok(Self {
            text,
            initial,
            repeat,
            repeatable,
        })
    }
    pub fn text(&self) -> &AggroText {
        &self.text
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OocHolderSnapshot {
    pub event_id: u32,
    pub timer_ms: u32,
    pub enabled: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OocSnapshot {
    pub update_remaining_ms: u32,
    pub debt_ms: u32,
    pub holders: Vec<OocHolderSnapshot>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OocDraw {
    Initial { event_id: u32, min: u32, max: u32 },
    Chance { event_id: u32 },
    Action { event_id: u32 },
    Repeat { event_id: u32, min: u32, max: u32 },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OocTimers<'a> {
    events: &'a [OocText],
    state: OocSnapshot,
}
impl<'a> OocTimers<'a> {
    // CreatureEventAI.cpp:37-40 draws only for unequal endpoints.
    fn timer<E>(
        event_id: u32,
        range: [u32; 2],
        initial: bool,
        draw: &mut impl FnMut(OocDraw) -> Result<u32, E>,
    ) -> Result<u32, E> {
        if range[0] == range[1] {
            return Ok(range[0]);
        }
        draw(if initial {
            OocDraw::Initial {
                event_id,
                min: range[0],
                max: range[1],
            }
        } else {
            OocDraw::Repeat {
                event_id,
                min: range[0],
                max: range[1],
            }
        })
    }
    pub fn new<E>(
        events: &'a [OocText],
        draw: &mut impl FnMut(OocDraw) -> Result<u32, E>,
    ) -> Result<Self, E> {
        let mut holders = Vec::with_capacity(events.len());
        for event in events {
            holders.push(OocHolderSnapshot {
                event_id: event.text.event_id(),
                timer_ms: Self::timer(event.text.event_id(), event.initial, true, draw)?,
                enabled: true,
            });
        }
        Ok(Self {
            events,
            state: OocSnapshot {
                update_remaining_ms: EVENT_UPDATE_MS,
                debt_ms: 0,
                holders,
            },
        })
    }
    pub fn restore(events: &'a [OocText], state: OocSnapshot) -> Result<Self, Error> {
        if state.update_remaining_ms > EVENT_UPDATE_MS
            || state.debt_ms != EVENT_UPDATE_MS - state.update_remaining_ms
            || state.holders.len() != events.len()
        {
            return Err(Error::Invalid("OOC clock or holder count"));
        }
        for (event, holder) in events.iter().zip(&state.holders) {
            if holder.event_id != event.text.event_id()
                || holder.timer_ms > event.initial[1].max(event.repeat[1])
                || (!holder.enabled && event.repeatable)
            {
                return Err(Error::Invalid("OOC holder state"));
            }
        }
        Ok(Self { events, state })
    }
    pub fn snapshot(&self) -> OocSnapshot {
        self.state.clone()
    }
    pub fn into_snapshot(self) -> OocSnapshot {
        self.state
    }
    // EnterCombat resets only the shared clock, not OOC holders:1597-1632.
    pub fn enter_combat(&mut self) {
        self.state.update_remaining_ms = EVENT_UPDATE_MS;
        self.state.debt_ms = 0;
    }
    // JustReachedHome and JustDied call Reset:1425-1452,1472,1495.
    pub fn reset<E>(&mut self, draw: &mut impl FnMut(OocDraw) -> Result<u32, E>) -> Result<(), E> {
        self.enter_combat();
        for (event, holder) in self.events.iter().zip(&mut self.state.holders) {
            holder.timer_ms = Self::timer(event.text.event_id(), event.initial, true, draw)?;
            holder.enabled = true;
        }
        Ok(())
    }
    /// Timer subtraction precedes combat/evade eligibility. All ready holders
    /// are collected before actions, as CheckAndReady/ProcessEvents do upstream.
    pub fn update<E: From<Error>>(
        &mut self,
        diff: u32,
        combat: bool,
        evade: bool,
        draw: &mut impl FnMut(OocDraw) -> Result<u32, E>,
    ) -> Result<Vec<TextSelection>, E> {
        let elapsed = self
            .state
            .debt_ms
            .checked_add(diff)
            .ok_or(Error::Invalid("OOC update debt overflow"))?;
        if self.state.update_remaining_ms >= diff {
            self.state.debt_ms = elapsed;
            self.state.update_remaining_ms -= diff;
            return Ok(Vec::new());
        }
        for holder in &mut self.state.holders {
            holder.timer_ms = holder.timer_ms.saturating_sub(elapsed);
        }
        let mut selections = Vec::new();
        for (event, holder) in self.events.iter().zip(&mut self.state.holders) {
            if !holder.enabled || holder.timer_ms != 0 || combat || evade {
                continue;
            }
            let event_id = event.text.event_id();
            let chance = draw(OocDraw::Chance { event_id })?;
            if u32::from(event.text.chance()) > chance % 100 {
                let mut action = AggroTextHolder::new(event.text);
                action.begin_enter_combat();
                action.consume_chance(chance)?;
                selections.push(action.consume_action(draw(OocDraw::Action { event_id })?)?);
            }
            // ResetEvent runs after success AND failure, drawing before disable.
            holder.timer_ms = Self::timer(event_id, event.repeat, false, draw)?;
            if !event.repeatable {
                holder.enabled = false;
            }
        }
        self.enter_combat();
        Ok(selections)
    }
}
