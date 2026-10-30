//! The per-frame wire→ECS bridge: [`apply_net_updates`] drains the inbound [`SessionEvent`]
//! channel through the handler table. Nothing is applied here: every packet's handler lives with
//! its subsystem, our own player's create and its arrival with the update packet's
//! ([`super::objects`]).

use benilla_protocol::SessionEvent;
use bevy::prelude::*;

use super::NetEvents;

#[cfg(test)]
mod seam_tests;
#[cfg(test)]
mod world_enter_tests;

/// The retiring P2P session exceeded its bounded entry queue; ignore its remaining tail.
#[derive(Resource)]
pub(crate) struct P2pInboundOverflow;

// ── The per-frame bridge systems ─────────────────────────────────────────────────────────────────

/// Runs this frame's events through the handler table ([`super::handlers`]) in wire order, each
/// handler's commands applied before the next, before anything else in
/// [`benilla_world::schedule::WorldStage::Net`] runs, the world session's held behind its entry
/// load ([`InboundHold`]).
pub(crate) fn apply_net_updates(world: &mut World) {
    if world.contains_resource::<P2pInboundOverflow>() {
        // Never dispatch the tail of a retired session on later frames.
        drain(&world.resource::<NetEvents>().0, 1024);
        return;
    }
    let p2p = world.contains_resource::<super::P2pSettings>()
        && !world.contains_resource::<super::NetOffline>();
    let receiver = &world.resource::<NetEvents>().0;
    let (mut events, closed) = drain(receiver, if p2p { 1024 } else { usize::MAX });
    // A bounded P2P queue closes on overload. Its disconnect cannot depend on fitting one
    // more message into that same queue; synthesize the final edge after draining it.
    if closed
        && p2p
        && (world.resource::<super::NetStatus>().connected
            || events
                .iter()
                .any(|event| matches!(event, SessionEvent::Connected { .. })))
        && !events
            .iter()
            .any(|event| matches!(event, SessionEvent::Disconnected { .. }))
    {
        events.push(SessionEvent::Disconnected {
            reason: "P2P transport stopped; restart to rejoin.".into(),
            end: benilla_protocol::SessionEnd::Lost,
        });
    }
    // `drn`: where the frame's packets land, against the input records' `prs` and `snd` lines.
    if !events.is_empty() && benilla_assets::trace::enabled_for("drn") {
        benilla_assets::trace::line("drn", &format!("events={}", events.len()));
    }
    let mut hold = world.remove_resource::<InboundHold>().unwrap_or_default();
    let mut out = hold.release(world);
    for ev in events {
        // The entry-load hold must not turn the bounded P2P transport into an unbounded
        // application queue. Retire before accepting a burst beyond the same hard cap.
        if p2p && hold.would_overflow(&ev) {
            world.insert_resource(P2pInboundOverflow);
            hold = InboundHold::default();
            out.clear();
            if let Some(mut start) = world.get_resource_mut::<super::P2pStart>() {
                start.begin_retirement();
            }
            out.push(SessionEvent::Disconnected {
                reason: "P2P entry queue overflow; restart to rejoin.".into(),
                end: benilla_protocol::SessionEnd::Lost,
            });
            break;
        }
        hold.admit(ev, &mut out);
    }
    world.insert_resource(hold);
    super::handlers::dispatch(world, out);
}

/// Called only after retirement finishes, before installing the replacement transport.
pub(super) fn discard_retired_inbound(world: &mut World) {
    world.resource::<NetEvents>().0.try_iter().for_each(drop);
    world.remove_resource::<InboundHold>();
    world.remove_resource::<P2pInboundOverflow>();
}

/// A bounded queue can be refilled while drained, so P2P also has a per-frame work bound.
fn drain(
    receiver: &crossbeam_channel::Receiver<SessionEvent>,
    limit: usize,
) -> (Vec<SessionEvent>, bool) {
    let mut events = Vec::new();
    loop {
        if events.len() == limit {
            return (events, false);
        }
        match receiver.try_recv() {
            Ok(event) => events.push(event),
            Err(crossbeam_channel::TryRecvError::Empty) => return (events, false),
            Err(crossbeam_channel::TryRecvError::Disconnected) => return (events, true),
        }
    }
}

/// The world session's inbound events behind its entry load. The reference sends the login only
/// once the interface is up (`0x46c272`), so no world packet is handled before it; here the login
/// goes out early (`net::io`'s deviation), and what it brings waits, in wire order, from
/// `Connected` until the load has run.
#[derive(Resource, Default)]
pub(crate) struct InboundHold {
    holding: bool,
    /// The entry load was seen armed since `Connected`.
    armed: bool,
    held: Vec<SessionEvent>,
}

impl InboundHold {
    fn would_overflow(&self, event: &SessionEvent) -> bool {
        self.holding
            && self.held.len() >= 1024
            && !matches!(
                event,
                SessionEvent::Pong { .. }
                    | SessionEvent::Disconnected { .. }
                    | SessionEvent::LoggedOut
                    | SessionEvent::CharacterLoginFailed { .. }
            )
    }

    /// One event off the wire: onto `out`, the drain's dispatch in order, or held.
    fn admit(&mut self, ev: SessionEvent, out: &mut Vec<SessionEvent>) {
        match &ev {
            // `SMSG_PONG` is the one packet the socket thread answers itself (`0x537b56`).
            SessionEvent::Pong { .. } if self.holding => out.push(ev),
            // A session's end lets everything before it through, in order: no load is coming.
            SessionEvent::Disconnected { .. }
            | SessionEvent::LoggedOut
            | SessionEvent::CharacterLoginFailed { .. }
                if self.holding =>
            {
                out.append(&mut self.held);
                out.push(ev);
                *self = Self::default();
            }
            _ if self.holding => self.held.push(ev),
            // The world session starts: what follows waits for the entry load.
            SessionEvent::Connected { .. } => {
                out.push(ev);
                self.holding = true;
            }
            _ => out.push(ev),
        }
    }

    /// The held events once the load has run, or once none is coming: the world was not entered
    /// (no `InWorld` transition owed or made, the arm never seen).
    fn release(&mut self, world: &World) -> Vec<SessionEvent> {
        if !self.holding {
            return Vec::new();
        }
        if world.contains_resource::<crate::ui_script::PendingEntryUiLoad>() {
            self.armed = true;
            return Vec::new();
        }
        let entering = world
            .get_resource::<NextState<crate::client_state::ClientState>>()
            .is_some_and(|n| {
                matches!(
                    n,
                    NextState::Pending(crate::client_state::ClientState::InWorld)
                )
            });
        if entering && !self.armed {
            return Vec::new();
        }
        std::mem::take(self).held
    }
}

#[cfg(test)]
mod hold_tests;
