//! Fork-only direct-peer backend. Each member owns only its own avatar's pose; these are
//! trusted-friends exploration sessions, not authoritative combat or a complete WoW realm.
//! The transport proves group-key possession before any avatar, pose or chat is accepted.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use benilla_p2p::{Command, Event, Hello, Network, PeerId, TransportConfig};
use benilla_protocol::{EntityKind, JumpInfo, MoveSpeeds, ObjectFields, SessionEnd, SessionEvent};
use crossbeam_channel::{Receiver, Sender};
use serde::{Deserialize, Serialize};

use super::{io, ChatKind, ClientCommand, P2pSettings, ParkRequest, ParkSender};

// Version of this fixed exploration world contract, not an attestation of local WoW assets.
const CONTENT_VERSION: &str = "benilla-exploration-northshire-v1";
const MAX_CHAT: usize = 255;
const SPAWN: [f32; 3] = [-8949.95, -132.49, 83.9];
const HELP: &str = "P2P exploration: direct peers, movement and /say. No realm server. Combat, quests, XP, loot and character saves are not implemented yet. Restart to start a new session.";

/// TOML keeps this small initial protocol inspectable, but never shares the config or its PSK.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum Frame {
    Avatar {
        position: [f32; 3],
        orientation: f32,
    },
    Move {
        position: [f32; 3],
        orientation: f32,
        flags: u32,
        pitch: f32,
        fall_time: u32,
        jump: Option<[f32; 4]>,
        heartbeat: bool,
        time: u32,
    },
    Say {
        text: String,
    },
}

impl Frame {
    fn encode(&self) -> Result<Vec<u8>> {
        let bytes = toml::to_string(self)?.into_bytes();
        if bytes.len() > benilla_p2p::MAX_PAYLOAD_SIZE {
            bail!("P2P message exceeds the frame limit");
        }
        Ok(bytes)
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > benilla_p2p::MAX_PAYLOAD_SIZE {
            bail!("P2P message exceeds the frame limit");
        }
        // Do not propagate parser errors: they can echo private chat payloads.
        let value: Self = std::str::from_utf8(bytes)
            .ok()
            .and_then(|text| toml::from_str(text).ok())
            .context("invalid P2P message")?;
        match &value {
            Self::Avatar {
                position,
                orientation,
            } => validate_pose(position, *orientation)?,
            Self::Move {
                position,
                orientation,
                pitch,
                jump,
                flags,
                ..
            } => {
                validate_pose(position, *orientation)?;
                if !pitch.is_finite() || pitch.abs() > std::f32::consts::PI
                    || jump.is_some_and(|values| values.iter().any(|x| !x.is_finite() || x.abs() > 100.0))
                    // No transport exists in this slice; accepting its flag without a pose is invalid.
                    || flags & 0x200 != 0
                {
                    bail!("invalid P2P movement");
                }
            }
            Self::Say { text } => {
                if text.is_empty() || text.len() > MAX_CHAT || text.chars().any(char::is_control) {
                    bail!("invalid P2P chat");
                }
            }
        }
        Ok(value)
    }
}

fn validate_pose(position: &[f32; 3], orientation: f32) -> Result<()> {
    if position
        .iter()
        .any(|x| !x.is_finite() || x.abs() > 20_000.0)
        || !orientation.is_finite()
        || orientation.abs() > 100.0
    {
        bail!("invalid P2P pose");
    }
    Ok(())
}

fn guid(id: PeerId) -> u64 {
    // WoW's high GUID bits remain the player type. Collisions are checked before admission.
    let mut bytes = [0; 8];
    bytes[..6].copy_from_slice(&id[..6]);
    u64::from_le_bytes(bytes).max(1)
}

fn name(id: PeerId) -> String {
    format!("Peer{:02x}{:02x}{:02x}", id[0], id[1], id[2])
}

fn player_name(id: PeerId) -> SessionEvent {
    SessionEvent::PlayerName {
        guid: guid(id),
        name: name(id),
        race: 1,
        gender: 0,
        class: 1,
    }
}

fn avatar(id: PeerId, position: [f32; 3], orientation: f32) -> SessionEvent {
    let guid = guid(id);
    SessionEvent::ObjectCreate {
        guid,
        kind: EntityKind::Player,
        display_id: Some(49), // ChrRaces' human male display, from the player's own client data.
        position,
        orientation,
        scale: 1.0,
        speeds: Some(MoveSpeeds {
            walk: 2.5,
            run: 7.0,
            run_back: 4.5,
            swim: 4.722222,
            swim_back: 2.5,
            turn_rate: std::f32::consts::PI,
        }),
        mover: None,
        transport_progress: None,
        transport: None,
        spline: None,
        fields: ObjectFields::from_pairs(&[
            (0, guid as u32),
            (1, (guid >> 32) as u32),
            (2, 25),
            (4, 1.0f32.to_bits()),
            (22, 100),
            (28, 100),
            (30, 1000),
            (34, 1),
            (35, 1),
            (36, 1 | (1 << 8) | (1 << 24)), // human warrior, male, rage
            (126, 2000),
            (131, 49),
            (132, 49),
            (716, 0),
            (717, 400),
        ])
        .into_created(benilla_protocol::messages::ObjectType::Player),
    }
}

fn chat(sender_guid: u64, text: String) -> SessionEvent {
    SessionEvent::Chat(benilla_protocol::messages::ChatMessage {
        chat_type: if sender_guid == 0 { 0x0a } else { 0x00 },
        language: 0,
        sender_guid,
        target_guid: 0,
        sender_name: None,
        channel: None,
        text,
        chat_tag: 0,
    })
}

fn emit(events: &Sender<SessionEvent>, event: SessionEvent) -> Result<()> {
    events
        .try_send(event)
        .map_err(|_| anyhow::anyhow!("P2P application event queue unavailable"))
}

/// Same application channels as the realm backend; none of the realm network code is started.
pub(super) fn spawn(settings: P2pSettings) -> io::NetHandles {
    let (events_tx, events) = crossbeam_channel::bounded(1024);
    let (commands, commands_rx) = crossbeam_channel::unbounded();
    let (park_tx, park_rx) = crossbeam_channel::unbounded::<ParkRequest>();
    let (login, login_rx) = crossbeam_channel::unbounded();
    thread::Builder::new()
        .name("benilla-peer-world".into())
        .spawn(move || {
            // Keep legacy UI senders valid, without ever submitting credentials to another peer.
            let _ui_receivers = (park_rx, login_rx);
            if let Err(error) = run(settings, &events_tx, &commands_rx) {
                bevy::log::error!("p2p: {error:#}");
                let _ = events_tx.try_send(SessionEvent::Disconnected {
                    reason: format!("P2P session ended: {error}"),
                    end: SessionEnd::Lost,
                });
            }
        })
        .expect("spawn benilla-peer-world thread");
    io::NetHandles {
        events,
        commands,
        pick: ParkSender::park(park_tx.clone()),
        realm: ParkSender::park(park_tx),
        screen_asks_roster: Arc::new(AtomicBool::new(false)),
        login,
        login_abandon: Arc::new(AtomicU64::new(0)),
        ping: Arc::new(Mutex::new(io::PingClock::default())),
    }
}

fn run(
    settings: P2pSettings,
    events: &Sender<SessionEvent>,
    commands: &Receiver<ClientCommand>,
) -> Result<()> {
    let id = benilla_p2p::generate_peer_id()?;
    let hello = Hello {
        protocol_version: 1,
        network: settings.0.network.clone(),
        peer_id: id,
        ruleset_hash: settings.1.fingerprint(),
        content_version: CONTENT_VERSION.into(),
    };
    let network = Network::start(
        TransportConfig {
            listen: settings.0.listen,
            peers: settings.0.peers.clone(),
            handshake_timeout: Duration::from_millis(settings.0.timeout_ms),
            max_peers: usize::from(settings.0.max_peers),
        },
        settings.0.psk_bytes(),
        hello,
    )?;
    bevy::log::info!(
        "p2p: exploration session listening on {} as {}",
        network.local_addr(),
        name(id)
    );
    let mut position = SPAWN;
    let mut orientation = 0.0;
    let mut peers = BTreeMap::<PeerId, bool>::new();
    let mut last_rejection_warning: Option<Instant> = None;
    let started = Instant::now();
    emit(
        events,
        SessionEvent::Connected {
            self_guid: guid(id),
            name: name(id),
            billing_time_rested: 0,
            tutorial_flags: None,
            addon_info: Some(Vec::new()),
        },
    )?;
    emit(
        events,
        SessionEvent::Worldport {
            map_id: 0,
            position,
            orientation,
            needs_ack: false,
        },
    )?;
    emit(events, avatar(id, position, orientation))?;
    emit(
        events,
        SessionEvent::TimeSpeed {
            hours: 12,
            minutes: 0,
            day_serial: 0,
            timescale: 0.01666667,
        },
    )?;
    emit(events, chat(0, HELP.into()))?;
    loop {
        for _ in 0..256 {
            let event = match network.try_recv() {
                Ok(event) => event,
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => bail!("peer transport stopped"),
            };
            match event {
                Event::PeerJoined(hello) => {
                    let peer = hello.peer_id;
                    if guid(peer) == guid(id)
                        || peers.keys().any(|other| guid(*other) == guid(peer))
                    {
                        bail!("peer identity collision; restart the session");
                    }
                    peers.insert(peer, false);
                    network
                        .try_send(Command::Send {
                            peer_id: peer,
                            payload: Frame::Avatar {
                                position,
                                orientation,
                            }
                            .encode()?,
                        })
                        .map_err(|_| anyhow::anyhow!("peer send queue unavailable"))?;
                }
                Event::PeerLeft(peer) => {
                    if peers.remove(&peer).is_some() {
                        emit(events, SessionEvent::ObjectDestroyed(guid(peer)))?;
                    }
                }
                Event::Frame { peer_id, payload } => {
                    let Some(joined) = peers.get_mut(&peer_id) else {
                        continue;
                    };
                    let frame = Frame::decode(&payload)?;
                    match frame {
                        Frame::Avatar {
                            position,
                            orientation,
                        } if !*joined => {
                            emit(events, player_name(peer_id))?;
                            emit(events, avatar(peer_id, position, orientation))?;
                            *joined = true;
                        }
                        Frame::Move {
                            position,
                            orientation,
                            flags,
                            pitch,
                            fall_time,
                            jump,
                            heartbeat,
                            time,
                        } if *joined => {
                            emit(
                                events,
                                SessionEvent::UnitMove {
                                    guid: guid(peer_id),
                                    position,
                                    orientation,
                                    flags,
                                    pitch,
                                    time,
                                    fall_time,
                                    transport: None,
                                    jump: jump.map(|j| JumpInfo {
                                        zspeed: j[0],
                                        cos_angle: j[1],
                                        sin_angle: j[2],
                                        xy_speed: j[3],
                                    }),
                                    verb: if heartbeat {
                                        benilla_protocol::RelayVerb::Heartbeat
                                    } else {
                                        benilla_protocol::RelayVerb::Pose
                                    },
                                },
                            )?;
                        }
                        Frame::Say { text } if *joined => emit(events, chat(guid(peer_id), text))?,
                        // A broadcast already queued at the connection edge may beat its avatar.
                        Frame::Move { .. } | Frame::Say { .. } if !*joined => {}
                        _ => bail!("peer sent an invalid session transition"),
                    }
                }
                Event::Rejected { address, reason: _ } => {
                    // Unauthenticated traffic must not turn a reachable listener into a log flood.
                    if last_rejection_warning
                        .is_none_or(|last| last.elapsed() >= Duration::from_secs(10))
                    {
                        bevy::log::warn!("p2p: peer at {address} was rejected; further warnings are rate-limited");
                        last_rejection_warning = Some(Instant::now());
                    }
                }
            }
        }
        match commands.recv_timeout(Duration::from_millis(5)) {
            Ok(command) => {
                let outgoing = match command {
                    ClientCommand::Move {
                        kind,
                        flags,
                        pos,
                        orientation: facing,
                        pitch,
                        fall_time,
                        jump,
                        transport,
                    } => {
                        if transport.is_some() {
                            continue;
                        }
                        position = pos;
                        orientation = facing;
                        Some(Frame::Move {
                            position,
                            orientation,
                            flags,
                            pitch,
                            fall_time,
                            jump: jump.map(|j| [j.zspeed, j.cos_angle, j.sin_angle, j.xy_speed]),
                            heartbeat: matches!(kind, super::MoveKind::Heartbeat),
                            time: started.elapsed().as_millis() as u32,
                        })
                    }
                    ClientCommand::Chat {
                        kind: ChatKind::Say,
                        text,
                        ..
                    } => {
                        if text.len() > MAX_CHAT
                            || text.is_empty()
                            || text.chars().any(char::is_control)
                        {
                            emit(
                                events,
                                chat(
                                    0,
                                    "P2P /say accepts 1–255 bytes without control characters."
                                        .into(),
                                ),
                            )?;
                            continue;
                        }
                        emit(events, chat(guid(id), text.clone()))?;
                        Some(Frame::Say { text })
                    }
                    ClientCommand::Chat { .. } => {
                        emit(
                            events,
                            chat(
                                0,
                                "Only /say is available in this P2P exploration session.".into(),
                            ),
                        )?;
                        None
                    }
                    ClientCommand::NameQuery { guid: query } => {
                        if query == guid(id) {
                            emit(events, player_name(id))?;
                        } else if let Some(peer) = peers.keys().find(|peer| guid(**peer) == query) {
                            emit(events, player_name(*peer))?;
                        }
                        None
                    }
                    ClientCommand::QueryTime => {
                        emit(
                            events,
                            SessionEvent::ServerUnixTime {
                                unix_time: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs() as u32,
                            },
                        )?;
                        None
                    }
                    ClientCommand::Logout | ClientCommand::ForceLogout => {
                        emit(
                            events,
                            SessionEvent::Disconnected {
                                reason: "P2P session closed. Restart to rejoin.".into(),
                                end: SessionEnd::Lost,
                            },
                        )?;
                        return Ok(());
                    }
                    // The remaining realm-only verbs have no simulation backend in this slice.
                    _ => None,
                };
                if let Some(frame) = outgoing {
                    network
                        .try_send(Command::Broadcast(frame.encode()?))
                        .map_err(|_| anyhow::anyhow!("peer send queue unavailable"))?;
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_pose_and_oversized_chat_fail_closed() {
        assert!(validate_pose(&[f32::NAN, 0.0, 0.0], 0.0).is_err());
        assert!(validate_pose(&[0.0; 3], f32::INFINITY).is_err());
        let bad = Frame::Say {
            text: "x".repeat(MAX_CHAT + 1),
        }
        .encode()
        .unwrap();
        assert!(Frame::decode(&bad).is_err());
        assert!(Frame::decode(&vec![0; benilla_p2p::MAX_PAYLOAD_SIZE + 1]).is_err());
    }

    #[test]
    fn chat_parse_errors_never_echo_private_text() {
        let error = Frame::decode(b"secret personal text").unwrap_err();
        assert!(!format!("{error:#}").contains("secret"));
    }

    #[test]
    fn codec_roundtrips_pose_and_chat() {
        let frame = Frame::Avatar {
            position: SPAWN,
            orientation: 1.5,
        };
        assert!(matches!(
            Frame::decode(&frame.encode().unwrap()).unwrap(),
            Frame::Avatar { .. }
        ));
        let frame = Frame::Say {
            text: "hello".into(),
        };
        assert!(
            matches!(Frame::decode(&frame.encode().unwrap()).unwrap(), Frame::Say { text } if text == "hello")
        );
    }

    fn settings(listen: std::net::SocketAddr, peers: &[std::net::SocketAddr]) -> P2pSettings {
        let peers = peers
            .iter()
            .map(|p| format!("\"{p}\""))
            .collect::<Vec<_>>()
            .join(",");
        let text = format!("[p2p]\npsk = \"000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f\"\nlisten = \"{listen}\"\npeers = [{peers}]\n");
        let config = benilla_config::AppConfig::parse(&text, std::path::Path::new("/tmp")).unwrap();
        P2pSettings(config.p2p.unwrap(), config.ruleset)
    }

    fn next_matching(
        handles: &io::NetHandles,
        predicate: impl Fn(&SessionEvent) -> bool,
    ) -> SessionEvent {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let event = handles
                .events
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("expected peer event before deadline");
            if let SessionEvent::Disconnected { reason, .. } = &event {
                panic!("unexpected disconnect: {reason}");
            }
            if predicate(&event) {
                return event;
            }
        }
    }

    #[test]
    fn two_real_peer_backends_enter_move_chat_and_remove_without_a_realm() {
        // Hold the port until immediately before starting A. B only dials A: both are equal peers.
        let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = reservation.local_addr().unwrap();
        drop(reservation);
        let a = spawn(settings(endpoint, &[]));
        let a_id = match next_matching(&a, |ev| matches!(ev, SessionEvent::Connected { .. })) {
            SessionEvent::Connected { self_guid, .. } => self_guid,
            _ => unreachable!(),
        };
        let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let second_endpoint = reservation.local_addr().unwrap();
        drop(reservation);
        let b = spawn(settings(second_endpoint, &[endpoint]));
        let b_id = match next_matching(&b, |ev| matches!(ev, SessionEvent::Connected { .. })) {
            SessionEvent::Connected { self_guid, .. } => self_guid,
            _ => unreachable!(),
        };
        assert_ne!(a_id, b_id);
        next_matching(
            &a,
            |ev| matches!(ev, SessionEvent::ObjectCreate { guid, .. } if *guid == b_id),
        );
        next_matching(
            &b,
            |ev| matches!(ev, SessionEvent::ObjectCreate { guid, .. } if *guid == a_id),
        );
        let position = [SPAWN[0] + 2.0, SPAWN[1], SPAWN[2]];
        a.commands
            .send(ClientCommand::Move {
                kind: super::super::MoveKind::Heartbeat,
                flags: 0,
                pos: position,
                orientation: 0.5,
                pitch: 0.0,
                fall_time: 0,
                jump: None,
                transport: None,
            })
            .unwrap();
        next_matching(
            &b,
            |ev| matches!(ev, SessionEvent::UnitMove { guid, position: actual, .. } if *guid == a_id && *actual == position),
        );
        a.commands
            .send(ClientCommand::Chat {
                kind: ChatKind::Say,
                target: None,
                text: "hello peer".into(),
                language: None,
            })
            .unwrap();
        next_matching(
            &b,
            |ev| matches!(ev, SessionEvent::Chat(message) if message.sender_guid == a_id && message.text == "hello peer"),
        );
        drop(a);
        next_matching(
            &b,
            |ev| matches!(ev, SessionEvent::ObjectDestroyed(guid) if *guid == a_id),
        );
    }

    #[test]
    fn peer_avatar_materializes_through_the_actual_ecs_bridge() {
        let mut app = crate::game_plugins::schedule_tests::headless_client();
        let (tx, rx) = crossbeam_channel::unbounded();
        app.insert_resource(super::super::NetEvents(rx));
        let me = [1; 16];
        let other = [2; 16];
        tx.send(SessionEvent::Connected {
            self_guid: guid(me),
            name: name(me),
            billing_time_rested: 0,
            tutorial_flags: None,
            addon_info: Some(Vec::new()),
        })
        .unwrap();
        tx.send(avatar(me, SPAWN, 0.0)).unwrap();
        tx.send(player_name(other)).unwrap();
        tx.send(avatar(other, SPAWN, 0.0)).unwrap();
        super::super::apply_net_updates(app.world_mut());
        let index = app.world().resource::<super::super::GuidIndex>();
        assert!(index.0.contains_key(&guid(me)));
        assert!(index.0.contains_key(&guid(other)));
        assert_ne!(index.0[&guid(me)], index.0[&guid(other)]);
        assert_eq!(
            app.world().resource::<super::super::SelfGuid>().0,
            Some(guid(me))
        );
    }

    #[test]
    fn a_full_bridge_queue_fails_without_blocking_and_closed_queue_tears_down() {
        let (tx, rx) = crossbeam_channel::bounded(1);
        emit(&tx, avatar([1; 16], SPAWN, 0.0)).unwrap();
        assert!(emit(&tx, avatar([2; 16], SPAWN, 0.0)).is_err());
        drop(tx);
        let mut app = crate::game_plugins::schedule_tests::headless_client();
        app.insert_resource(settings("127.0.0.1:7474".parse().unwrap(), &[]));
        app.world_mut()
            .remove_resource::<super::super::NetOffline>();
        app.insert_resource(super::super::NetEvents(rx));
        app.world_mut()
            .resource_mut::<super::super::NetStatus>()
            .connected = true;
        super::super::apply_net_updates(app.world_mut());
        assert!(!app.world().resource::<super::super::NetStatus>().connected);
        assert!(app
            .world()
            .resource::<super::super::GuidIndex>()
            .0
            .is_empty());
    }

    #[test]
    fn identity_is_bound_to_authenticated_peer_and_player_guid_type() {
        let id = [0x7f; 16];
        assert_eq!(guid(id) >> 48, 0);
        assert_ne!(guid([0; 16]), 0);
        assert!(name(id).len() <= 12);
        if let SessionEvent::ObjectCreate {
            guid: value, kind, ..
        } = avatar(id, SPAWN, 0.0)
        {
            assert_eq!(value, guid(id));
            assert_eq!(kind, EntityKind::Player);
        } else {
            panic!("expected avatar")
        }
    }
}
