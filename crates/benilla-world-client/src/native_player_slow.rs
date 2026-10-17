// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-ordered player harmful slots and forced speeds. This projects committed state;
//! neither an aura field nor a speed ACK grants a client-side gameplay mutation.
use super::*;
use benilla_sim::player_slow::{MoveType, SpeedRates};

fn pairs<'a>(
    holders: impl Iterator<Item = &'a n::PlayerSlowHolder>,
    slot_levels: [u8; 16],
) -> Vec<(u16, u32)> {
    let mut ids = [0; 16];
    let mut flags = [0; 2];
    let mut levels = [0; 4];
    for (offset, level) in slot_levels.into_iter().enumerate() {
        levels[offset / 4] |= u32::from(level) << ((offset % 4) * 8);
    }
    for holder in holders {
        let offset = usize::from(holder.slot - 32);
        ids[offset] = holder.spell_id;
        // SpellAuraHolder::SetAuraFlag uses AFLAG_UNK4 for a harmful holder.
        flags[offset / 8] |= 0x8 << ((offset % 8) * 4);
    }
    let mut out = ids
        .into_iter()
        .enumerate()
        .map(|(offset, id)| (79 + offset as u16, id))
        .collect::<Vec<_>>();
    out.extend([(99, flags[0]), (100, flags[1])]);
    for (offset, level) in levels.into_iter().enumerate() {
        out.push((109 + offset as u16, level));
        // The wire application count is stack count minus one: these holders have one stack.
        out.push((121 + offset as u16, 0));
    }
    out
}

pub(super) fn player_pairs(w: &n::NativeWorld) -> Vec<(u16, u32)> {
    w.state()
        .player_slow
        .as_ref()
        .map_or_else(Vec::new, |state| {
            pairs(state.holders(), state.slot_levels())
        })
}

fn speed_order(
    guid: u64,
    kind: MoveType,
    speed: ai::Real,
    next: &mut Option<u32>,
) -> Result<SessionEvent, Error> {
    let kind = match kind {
        MoveType::Walk => p::SpeedKind::Walk,
        MoveType::Run => p::SpeedKind::Run,
        MoveType::RunBack => p::SpeedKind::RunBack,
        MoveType::Swim => p::SpeedKind::Swim,
        MoveType::SwimBack => p::SpeedKind::SwimBack,
    };
    Ok(SessionEvent::ForceSpeedChange {
        guid,
        kind,
        counter: movement_counter(next)?,
        speed: speed.as_f32(),
    })
}

fn speed_orders(
    guid: u64,
    rates: SpeedRates,
    next: &mut Option<u32>,
) -> Result<Vec<SessionEvent>, Error> {
    [MoveType::Run, MoveType::RunBack, MoveType::Swim]
        .into_iter()
        .map(|kind| {
            speed_order(
                guid,
                kind,
                rates.speed(kind).map_err(|_| Error::Unrepresentable)?,
                next,
            )
        })
        .collect()
}

pub(super) fn initial(
    w: &n::NativeWorld,
    guid: u64,
    next: &mut Option<u32>,
) -> Result<Vec<SessionEvent>, Error> {
    let Some(state) = &w.state().player_slow else {
        return Ok(vec![]);
    };
    let mut events = state
        .holders()
        .map(|holder| SessionEvent::AuraDuration {
            slot: holder.slot,
            remaining_ms: holder.remaining_ms,
        })
        .collect::<Vec<_>>();
    // A new session never inherits another session's ACKs, even at normal speed.
    events.extend(speed_orders(guid, state.speeds(), next)?);
    Ok(events)
}

pub(super) struct Frame {
    holders: BTreeMap<u8, n::PlayerSlowHolder>,
    slot_levels: [u8; 16],
}
impl Frame {
    pub(super) fn new(previous: &n::State) -> Self {
        Self {
            slot_levels: previous
                .player_slow
                .as_ref()
                .map_or([0; 16], |state| state.slot_levels()),
            holders: previous
                .player_slow
                .iter()
                .flat_map(|state| state.holders())
                .map(|holder| (holder.slot, holder.clone()))
                .collect(),
        }
    }
    fn fields(&self, guid: u64) -> SessionEvent {
        SessionEvent::ObjectValues {
            guid,
            fields: ObjectFields::from_pairs(&pairs(self.holders.values(), self.slot_levels)),
        }
    }
    pub(super) fn project(
        &mut self,
        event: &n::PlayerSlowEvent,
        guid: u64,
        next: &mut Option<u32>,
    ) -> Result<Vec<SessionEvent>, Error> {
        match event {
            n::PlayerSlowEvent::Applied { holder } => {
                if !(32..48).contains(&holder.slot)
                    || holder.effect_slot > 2
                    || self.holders.insert(holder.slot, holder.clone()).is_some()
                {
                    return Err(Error::Unrepresentable);
                }
                self.slot_levels[usize::from(holder.slot - 32)] = holder.caster_level;
                // SpellAuraHolder::SetAura sends the slot/duration before HandleModDecreaseSpeed.
                Ok(vec![
                    self.fields(guid),
                    SessionEvent::AuraDuration {
                        slot: holder.slot,
                        remaining_ms: holder.remaining_ms,
                    },
                ])
            }
            n::PlayerSlowEvent::Removed {
                holder, slot_level, ..
            } => {
                if self.holders.remove(&holder.slot).map(|old| old.id) != Some(holder.id) {
                    return Err(Error::Unrepresentable);
                }
                self.slot_levels[usize::from(holder.slot - 32)] = *slot_level;
                // The runtime orders restored speeds before this clear, including replacement.
                Ok(vec![self.fields(guid)])
            }
            n::PlayerSlowEvent::SpeedChanged { kind, speed, .. } => {
                Ok(vec![speed_order(guid, *kind, *speed, next)?])
            }
        }
    }
    pub(super) fn finish(&self, w: &n::NativeWorld) -> Result<(), Error> {
        let expected: BTreeMap<_, _> = w
            .state()
            .player_slow
            .iter()
            .flat_map(|state| state.holders())
            .map(|holder| (holder.slot, holder.id))
            .collect();
        let actual: BTreeMap<_, _> = self.holders.iter().map(|(slot, h)| (*slot, h.id)).collect();
        if actual != expected
            || (w.state().player_slow.is_some()
                && pairs(self.holders.values(), self.slot_levels) != player_pairs(w))
        {
            return Err(Error::Unrepresentable);
        }
        Ok(())
    }
}
