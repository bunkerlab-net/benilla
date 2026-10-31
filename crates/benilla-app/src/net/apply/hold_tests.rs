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
        &std::env::temp_dir(),
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
