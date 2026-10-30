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
mod hold_tests {
    use benilla_protocol::{SessionEnd, SessionEvent};
    use bevy::prelude::*;

    use super::{
        apply_net_updates, discard_retired_inbound, InboundHold, NetEvents, P2pInboundOverflow,
    };

    fn connected() -> SessionEvent {
        SessionEvent::Connected {
            self_guid: 0x2a,
            name: "Probeseven".into(),
            billing: Default::default(),
            tutorial_flags: None,
            addon_info: None,
        }
    }

    /// What `events` leaves on the drain's dispatch, by kind.
    fn admitted(hold: &mut InboundHold, events: Vec<SessionEvent>) -> Vec<String> {
        let mut out = Vec::new();
        for ev in events {
            hold.admit(ev, &mut out);
        }
        out.iter()
            .map(|ev| format!("{:?}", benilla_protocol::SessionEventKind::from(ev)))
            .collect()
    }

    #[test]
    fn p2p_overflow_retires_transport_and_never_dispatches_its_tail() {
        use super::super::{NetHandlerApp, NetStatus, P2pEndpointTicket, P2pSettings};
        #[derive(Resource, Default)]
        struct Seen {
            ended: usize,
            played: usize,
        }
        fn ended(In(_): In<SessionEvent>, mut seen: ResMut<Seen>) {
            seen.ended += 1;
        }
        fn played(In(_): In<SessionEvent>, mut seen: ResMut<Seen>) {
            seen.played += 1;
        }
        let config = benilla_config::AppConfig::parse(
            "[p2p]\npsk = \"public bounded queue fixture only\"\nlisten = \"127.0.0.1:31337\"\n",
            std::path::Path::new("/tmp"),
        )
        .unwrap();
        let settings = P2pSettings(config.p2p.unwrap(), config.ruleset);
        let (_handles, start) =
            super::super::p2p::prepare(settings.clone(), P2pEndpointTicket::default());
        let mut app = App::new();
        app.insert_resource(settings)
            .insert_resource(start)
            .init_resource::<NetStatus>()
            .init_resource::<Seen>()
            .net_handler(benilla_protocol::SessionEventKind::Disconnected, ended)
            .net_handler(benilla_protocol::SessionEventKind::PlayedTime, played);
        let (tx, rx) = crossbeam_channel::unbounded();
        app.insert_resource(NetEvents(rx));
        app.insert_resource(NextState::Pending(
            crate::client_state::ClientState::InWorld,
        ));
        app.insert_resource(InboundHold {
            holding: true,
            armed: false,
            held: vec![SessionEvent::PlayedTime { total: 1, level: 1 }; 1024],
        });
        tx.send(SessionEvent::PlayedTime { total: 2, level: 2 })
            .unwrap();
        apply_net_updates(app.world_mut());
        assert!(app.world().contains_resource::<P2pInboundOverflow>());
        assert!(!app.world().resource::<super::super::P2pStart>().can_start());
        assert!(app.world().resource::<InboundHold>().held.is_empty());
        assert_eq!(app.world().resource::<Seen>().ended, 1);
        assert_eq!(app.world().resource::<Seen>().played, 0);
        tx.send(SessionEvent::PlayedTime { total: 3, level: 3 })
            .unwrap();
        apply_net_updates(app.world_mut());
        assert_eq!(app.world().resource::<Seen>().ended, 1);
        assert_eq!(app.world().resource::<Seen>().played, 0);
        // Exercise the same reset path called after reconfiguration retirement. An old
        // queued event is discarded, while a new session's event can run normally.
        tx.send(SessionEvent::PlayedTime { total: 4, level: 4 })
            .unwrap();
        discard_retired_inbound(app.world_mut());
        assert!(!app.world().contains_resource::<P2pInboundOverflow>());
        assert!(!app.world().contains_resource::<InboundHold>());
        tx.send(SessionEvent::PlayedTime { total: 5, level: 5 })
            .unwrap();
        apply_net_updates(app.world_mut());
        assert_eq!(app.world().resource::<Seen>().played, 1);
    }

    #[test]
    fn p2p_hold_limit_accepts_exact_capacity_and_exempts_terminal_and_pong() {
        let mut hold = InboundHold::default();
        let mut out = Vec::new();
        hold.admit(connected(), &mut out);
        for _ in 0..1024 {
            let event = SessionEvent::PlayedTime { total: 1, level: 1 };
            assert!(!hold.would_overflow(&event));
            hold.admit(event, &mut out);
        }
        assert!(hold.would_overflow(&SessionEvent::PlayedTime { total: 1, level: 1 }));
        for event in [
            SessionEvent::Pong { sequence: 1 },
            SessionEvent::LoggedOut,
            SessionEvent::CharacterLoginFailed { result: 1 },
            SessionEvent::Disconnected {
                reason: "end".into(),
                end: SessionEnd::Lost,
            },
        ] {
            assert!(!hold.would_overflow(&event));
        }
        hold.admit(SessionEvent::Pong { sequence: 1 }, &mut out);
        assert_eq!(hold.held.len(), 1024);
        hold.admit(SessionEvent::LoggedOut, &mut out);
        assert!(hold.held.is_empty());
        assert!(!hold.holding);
        assert!(matches!(out.last(), Some(SessionEvent::LoggedOut)));
    }

    /// A `SMSG_PONG` passes the hold (`0x537b56`); a session's end lets the held events through
    /// first, in wire order, then itself, and holds nothing after.
    #[test]
    fn a_pong_passes_and_an_end_releases_in_wire_order() {
        let mut hold = InboundHold::default();
        let time = || SessionEvent::PlayedTime { total: 1, level: 1 };
        assert_eq!(
            admitted(
                &mut hold,
                vec![connected(), time(), SessionEvent::Pong { sequence: 1 }]
            ),
            ["Connected", "Pong"],
            "the world packet held, the pong through"
        );
        let end = SessionEvent::Disconnected {
            reason: "gone".into(),
            end: SessionEnd::Lost,
        };
        assert_eq!(
            admitted(&mut hold, vec![time(), end, time()]),
            ["PlayedTime", "PlayedTime", "Disconnected", "PlayedTime"]
        );
    }

    /// No load coming (no `InWorld` transition owed, the arm never seen): the hold lets go at the
    /// next drain; one owed waits for the arm.
    #[test]
    fn the_hold_lets_go_when_no_load_is_coming() {
        let mut world = World::new();
        let mut hold = InboundHold::default();
        let held = vec![connected(), SessionEvent::Pong { sequence: 1 }];
        admitted(&mut hold, held);
        hold.held.push(SessionEvent::LoggedOut);
        world.insert_resource(NextState::Pending(
            crate::client_state::ClientState::InWorld,
        ));
        assert!(
            hold.release(&world).is_empty(),
            "the entry is owed: still held"
        );
        world.remove_resource::<NextState<crate::client_state::ClientState>>();
        assert_eq!(hold.release(&world).len(), 1, "no load coming: released");
        assert!(!hold.holding);
    }

    use crate::local_state::test_env::{EnvGuard, ENV_LOCK};

    /// The digests come in the login's own frame, behind `Connected`: they wait for the entry
    /// load, so the load finds every account-data section pending and its readers are held for
    /// them (`0x5afe50`), as the reference's load precedes the login (`0x46c272`). A `SMSG_PONG`
    /// passes the hold (`0x537b56`).
    #[test]
    fn the_login_burst_waits_for_the_entry_load() {
        let _l = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let tmp = std::env::temp_dir().join(format!("benilla-net-hold-{}", std::process::id()));
        std::fs::remove_dir_all(&tmp).ok();
        let _c = EnvGuard::unset("WOW_CAPTURE");
        let _h = EnvGuard::set("BENILLA_HOME", tmp.to_str().unwrap());
        let mut app = crate::game_plugins::schedule_tests::headless_client();
        app.insert_resource(bevy::ecs::error::DefaultErrorHandler(
            bevy::ecs::error::ignore,
        ));
        let (tx, _sent) = crossbeam_channel::unbounded();
        app.insert_resource(crate::net::NetCommands(tx));
        let (inbound, rx) = crossbeam_channel::unbounded();
        app.insert_resource(crate::net::NetEvents::for_test(rx));
        let pending = |app: &bevy::prelude::App| {
            app.world()
                .resource::<crate::account_data::AccountData>()
                .pending(crate::account_data::CHAT)
        };
        for ev in [connected(), SessionEvent::AccountDataMd5([[0; 16]; 8])] {
            inbound.send(ev).unwrap();
        }
        app.update();
        assert!(pending(&app), "held behind Connected");
        app.update();
        assert!(
            app.world()
                .contains_resource::<crate::ui_script::PendingEntryUiLoad>(),
            "the entry load is armed"
        );
        assert!(pending(&app), "still held while the load is armed");
        inbound.send(SessionEvent::Pong { sequence: 1 }).unwrap();
        let mut frames = 0;
        while app
            .world()
            .contains_resource::<crate::ui_script::PendingEntryUiLoad>()
        {
            app.update();
            frames += 1;
            assert!(frames < 10, "the entry load ran");
        }
        assert!(!pending(&app), "released after the load, in wire order");
        drop(app);
        std::fs::remove_dir_all(&tmp).ok();
    }
}
