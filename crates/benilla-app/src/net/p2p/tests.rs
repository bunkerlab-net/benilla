use super::super::CharRequest;
use super::*;
use benilla_profile::{Appearance, NewCharacter, SpawnPolicy};
use std::path::{Path, PathBuf};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "benilla-profile-bridge-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn profile(&self) -> PathBuf {
        self.0.join("profile")
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub(super) fn settings(
    listen: std::net::SocketAddr,
    peers: &[std::net::SocketAddr],
) -> P2pSettings {
    let peers = peers
        .iter()
        .map(|p| format!("\"{p}\""))
        .collect::<Vec<_>>()
        .join(",");
    let text = format!("[p2p]\npsk = \"public app integration test phrase only\"\nlisten = \"{listen}\"\npeers = [{peers}]\n");
    let config = benilla_config::AppConfig::parse(&text, Path::new("/tmp")).unwrap();
    P2pSettings(config.p2p.unwrap(), config.ruleset)
}
fn request(root: &Path) -> StartRequest {
    StartRequest {
        root: Some(root.to_path_buf()),
        catalog: Some(AvatarCatalog::fixture()),
        generation: 0,
    }
}
fn open(root: &Path, settings: P2pSettings) -> (io::NetHandles, P2pStart) {
    let (handles, mut start) = prepare(settings, P2pEndpointTicket::default());
    start.start_request(request(root)).unwrap();
    (handles, start)
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
            .expect("expected backend event before deadline");
        if predicate(&event) {
            return event;
        }
        if let SessionEvent::Disconnected {
            reason,
            end: SessionEnd::Lost,
        } = &event
        {
            panic!("unexpected disconnect: {reason}");
        }
    }
}
fn listed(handles: &io::NetHandles) -> Vec<benilla_protocol::Character> {
    match next_matching(handles, |e| matches!(e, SessionEvent::CharacterList { .. })) {
        SessionEvent::CharacterList { characters, .. } => characters,
        _ => unreachable!(),
    }
}
fn create(handles: &io::NetHandles, name: &str, race: u8, class: u8, gender: u8) -> u64 {
    handles
        .pick
        .send(CharRequest::Create(benilla_protocol::CharCreateReq {
            name: name.into(),
            race,
            class,
            gender,
            skin: 1,
            face: 2,
            hair_style: 3,
            hair_color: 1,
            facial_hair: 2,
        }))
        .unwrap();
    let chars = listed(handles);
    next_matching(handles, |e| {
        matches!(
            e,
            SessionEvent::CharActionResult {
                action: benilla_protocol::CharAction::Create,
                code: benilla_protocol::messages::CHAR_CREATE_SUCCESS
            }
        )
    });
    chars
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(name))
        .unwrap()
        .guid
}
fn enter(handles: &io::NetHandles, guid: u64) {
    handles.pick.send(CharRequest::Enter(guid)).unwrap();
    next_matching(
        handles,
        |e| matches!(e, SessionEvent::Connected { self_guid, .. } if *self_guid == guid),
    );
}
fn free_endpoint() -> std::net::SocketAddr {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
}
pub(super) fn fixture(owner: u8, id: u8, name: &str) -> Character {
    Character {
        id: [id; 16],
        owner_id: [owner; 16],
        name: name.into(),
        race: 1,
        class: 1,
        gender: 0,
        appearance: Appearance::default(),
        level: 1,
        spawn_policy: SpawnPolicy::NorthshireMeetingV1,
        position: SpawnPolicy::NorthshireMeetingV1.position(),
    }
}

#[test]
fn preparing_and_dropping_does_not_write_a_profile_or_start_transport() {
    let dir = Temp::new();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let peer = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    peer.set_nonblocking(true).unwrap();
    let (handles, start) = prepare(
        settings(
            listener.local_addr().unwrap(),
            &[peer.local_addr().unwrap()],
        ),
        P2pEndpointTicket::default(),
    );
    assert!(start.can_start());
    assert!(!dir.profile().exists());
    assert!(handles
        .events
        .recv_timeout(Duration::from_millis(100))
        .is_err());
    assert_eq!(
        peer.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    drop(start);
    assert!(matches!(
        handles.events.try_recv(),
        Err(crossbeam_channel::TryRecvError::Disconnected)
    ));
}

#[test]
fn real_create_screen_commands_save_and_list_without_opening_a_socket() {
    let dir = Temp::new();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let (handles, mut start) = open(
        &dir.profile(),
        settings(listener.local_addr().unwrap(), &[]),
    );
    assert!(listed(&handles).is_empty());
    let guid = create(&handles, "Thrallina", 2, 7, 1);
    assert!(!start.can_start());
    assert!(start.start_request(request(&dir.profile())).is_err());
    assert!(handles
        .events
        .try_iter()
        .all(|event| !matches!(event, SessionEvent::Connected { .. })));
    // Held port proves create/select neither bound nor tried to enter the world.
    drop(start);
    let store = ProfileStore::open(dir.profile()).unwrap();
    let saved = store.selected_character().unwrap();
    assert_eq!(character_guid(saved), guid);
    assert_eq!(
        (&saved.name, saved.race, saved.class, saved.gender),
        (&"Thrallina".into(), 2, 7, 1)
    );
    assert_eq!(saved.appearance.face, 2);
}

#[test]
fn solo_create_join_logout_restart_reuses_identity_and_position_with_offline_peer() {
    let dir = Temp::new();
    let endpoint = free_endpoint();
    let (handles, start) = open(
        &dir.profile(),
        settings(endpoint, &["127.0.0.1:1".parse().unwrap()]),
    );
    assert!(listed(&handles).is_empty());
    let guid = create(&handles, "Solotest", 1, 4, 0);
    enter(&handles, guid);
    next_matching(
        &handles,
        |e| matches!(e, SessionEvent::ObjectCreate { guid: actual, .. } if *actual == guid),
    );
    let moved = [SPAWN[0] + 7.0, SPAWN[1] + 3.0, SPAWN[2]];
    handles
        .commands
        .send(ClientCommand::Move {
            kind: super::super::MoveKind::Heartbeat,
            flags: 0,
            pos: moved,
            orientation: 1.0,
            pitch: 0.0,
            fall_time: 0,
            jump: None,
            transport: None,
        })
        .unwrap();
    handles.commands.send(ClientCommand::Logout).unwrap();
    next_matching(&handles, |e| matches!(e, SessionEvent::LoggedOut));
    let chars = listed(&handles);
    assert_eq!(chars[0].guid, guid);
    assert_eq!(
        [
            chars[0].position.x,
            chars[0].position.y,
            chars[0].position.z
        ],
        moved
    );
    drop(start);
    let owner = {
        let store = ProfileStore::open(dir.profile()).unwrap();
        store.owner_id()
    };
    let (handles2, start2) = open(&dir.profile(), settings(endpoint, &[]));
    assert_eq!(listed(&handles2)[0].guid, guid);
    enter(&handles2, guid);
    next_matching(
        &handles2,
        |e| matches!(e, SessionEvent::Worldport { position, orientation, .. } if *position == moved && *orientation == 1.0),
    );
    drop(start2);
    assert_eq!(ProfileStore::open(dir.profile()).unwrap().owner_id(), owner);
}

#[test]
fn back_then_start_reopens_saved_roster_without_network_or_duplicate_worker() {
    let dir = Temp::new();
    let (handles, mut start) = open(&dir.profile(), settings(free_endpoint(), &[]));
    assert!(listed(&handles).is_empty());
    let guid = create(&handles, "Backtest", 1, 1, 0);
    handles.pick.send(CharRequest::Abandon).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !start.can_start() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    start.start_request(request(&dir.profile())).unwrap();
    assert_eq!(listed(&handles)[0].guid, guid);
}

#[test]
fn normal_application_shutdown_checkpoints_last_local_movement() {
    let dir = Temp::new();
    let (handles, mut start) = open(&dir.profile(), settings(free_endpoint(), &[]));
    listed(&handles);
    let guid = create(&handles, "Exitpose", 1, 1, 0);
    enter(&handles, guid);
    let moved = [SPAWN[0] + 5.0, SPAWN[1], SPAWN[2]];
    handles
        .commands
        .send(ClientCommand::Move {
            kind: super::super::MoveKind::Heartbeat,
            flags: 0,
            pos: moved,
            orientation: 0.7,
            pitch: 0.0,
            fall_time: 0,
            jump: None,
            transport: None,
        })
        .unwrap();
    // Query follows movement on the same FIFO and confirms the worker consumed the movement.
    handles.commands.send(ClientCommand::QueryTime).unwrap();
    next_matching(&handles, |e| {
        matches!(e, SessionEvent::ServerUnixTime { .. })
    });
    start.shutdown();
    let store = ProfileStore::open(dir.profile()).unwrap();
    assert_eq!(store.selected_character().unwrap().position.position, moved);
}

#[test]
fn invalid_create_and_save_failure_are_visible_and_never_claim_success() {
    let dir = Temp::new();
    let (handles, start) = open(&dir.profile(), settings(free_endpoint(), &[]));
    listed(&handles);
    handles
        .pick
        .send(CharRequest::Create(benilla_protocol::CharCreateReq {
            name: "Invalid".into(),
            race: 3,
            class: 8,
            gender: 0,
            skin: 0,
            face: 0,
            hair_style: 0,
            hair_color: 0,
            facial_hair: 0,
        }))
        .unwrap();
    next_matching(&handles, |e| {
        matches!(e, SessionEvent::CharActionResult { code: 0x30, .. })
    });
    handles.pick.send(CharRequest::Enumerate).unwrap();
    assert!(listed(&handles).is_empty());
    // Simulate an external disk mutation, not user state. The store must never overwrite it.
    std::fs::write(
        dir.profile().join("profile.json"),
        b"fictional corrupt test fixture",
    )
    .unwrap();
    handles
        .pick
        .send(CharRequest::Create(benilla_protocol::CharCreateReq {
            name: "Savefail".into(),
            race: 1,
            class: 1,
            gender: 0,
            skin: 0,
            face: 0,
            hair_style: 0,
            hair_color: 0,
            facial_hair: 0,
        }))
        .unwrap();
    next_matching(
        &handles,
        |e| matches!(e, SessionEvent::Disconnected { reason, end: SessionEnd::Lost } if reason.contains("could not save character")),
    );
    drop(start);
    assert_eq!(
        std::fs::read(dir.profile().join("profile.json")).unwrap(),
        b"fictional corrupt test fixture"
    );
}

#[test]
fn character_descriptor_uses_saved_name_body_appearance_class_and_valid_rest() {
    let mut c = fixture(1, 2, "Orctest");
    c.race = 2;
    c.gender = 1;
    c.class = 7;
    c.appearance = Appearance {
        skin: 1,
        face: 2,
        hair_style: 3,
        hair_color: 1,
        facial_hair: 2,
    };
    assert!(
        matches!(character_name(&c), SessionEvent::PlayerName { name, race: 2, class: 7, gender: 1, .. } if name == "Orctest")
    );
    let SessionEvent::ObjectCreate {
        guid,
        fields,
        display_id,
        ..
    } = character_avatar(&c, &AvatarCatalog::fixture()).unwrap()
    else {
        panic!("avatar");
    };
    assert_eq!(guid, character_guid(&c));
    assert_eq!(display_id, Some(52));
    assert_eq!(fields.unit_race(), Some(2));
    assert_eq!(fields.unit_class(), Some(7));
    assert_eq!(fields.unit_gender(), Some(1));
    assert_eq!(fields.unit_power_type(), 0);
    assert_eq!(fields.player_face(), Some(2));
    assert_eq!(fields.player_hair_style(), Some(3));
    assert_eq!(fields.player_facial_hair(), Some(2));
    assert_eq!(fields.player_rest_state(), Some(2));
    assert_eq!(fields.player_rest_state_experience(), Some(0));
}

#[test]
fn owner_bound_character_guid_prevents_offline_id_copy_impersonation() {
    let a = fixture(1, 7, "Samechar");
    let b = fixture(2, 7, "Samechar");
    assert_ne!(character_guid(&a), character_guid(&b));
    assert_eq!(character_guid(&a) >> 48, 0);
    assert_ne!(character_guid(&a), 0);
    let peers = [(b.owner_id, Some(b.clone()))].into();
    assert!(validate_peer_character(
        a.owner_id,
        &a,
        &fixture(3, 9, "Localtest"),
        &peers,
        &AvatarCatalog::fixture()
    )
    .is_ok());
    assert!(validate_peer_character(
        b.owner_id,
        &a,
        &b,
        &BTreeMap::new(),
        &AvatarCatalog::fixture()
    )
    .is_err());
}

#[test]
fn peer_intro_requires_authenticated_owner_valid_traits_and_unique_game_identity() {
    let own = fixture(1, 1, "Ownchar");
    let other = fixture(2, 2, "Otherchar");
    let cat = AvatarCatalog::fixture();
    assert!(validate_peer_character([3; 16], &other, &own, &BTreeMap::new(), &cat).is_err());
    let peers = [([2; 16], Some(other.clone()))].into();
    assert!(validate_peer_character([2; 16], &other, &own, &peers, &cat).is_err());
    for change in [0, 1, 2, 3, 4] {
        let mut bad = other.clone();
        match change {
            0 => bad.level = 60,
            1 => bad.class = 11,
            2 => bad.appearance.face = 14,
            3 => bad.position.map_id = 1,
            _ => bad.name = "Bad;name".into(),
        }
        assert!(validate_peer_character([2; 16], &bad, &own, &BTreeMap::new(), &cat).is_err());
    }
}

#[test]
fn absent_and_preintro_peers_cannot_move_chat_or_overwrite_an_entity() {
    let own = fixture(1, 1, "Ownchar");
    let other = fixture(2, 2, "Otherchar");
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut peers = BTreeMap::new();
    let say = Frame::Say { text: "hi".into() }.encode().unwrap();
    apply_peer_frame(
        &tx,
        other.owner_id,
        &say,
        &mut peers,
        &own,
        &AvatarCatalog::fixture(),
    )
    .unwrap();
    peers.insert(other.owner_id, None);
    apply_peer_frame(
        &tx,
        other.owner_id,
        &say,
        &mut peers,
        &own,
        &AvatarCatalog::fixture(),
    )
    .unwrap();
    assert!(rx.is_empty());
    let avatar = Frame::Avatar {
        version: AVATAR_VERSION,
        character: other.clone(),
    }
    .encode()
    .unwrap();
    apply_peer_frame(
        &tx,
        other.owner_id,
        &avatar,
        &mut peers,
        &own,
        &AvatarCatalog::fixture(),
    )
    .unwrap();
    assert_eq!(rx.try_iter().count(), 2);
    assert!(apply_peer_frame(
        &tx,
        other.owner_id,
        &avatar,
        &mut peers,
        &own,
        &AvatarCatalog::fixture()
    )
    .is_err());
    assert!(rx.is_empty());
    assert_eq!(peers[&other.owner_id], Some(other));
}

#[test]
fn frame_codec_is_versioned_bounded_and_does_not_echo_payload_errors() {
    let avatar = Frame::Avatar {
        version: AVATAR_VERSION,
        character: fixture(1, 2, "Codecname"),
    };
    assert!(matches!(
        Frame::decode(&avatar.encode().unwrap()).unwrap(),
        Frame::Avatar { version: 1, .. }
    ));
    let bad_version = Frame::Avatar {
        version: 99,
        character: fixture(1, 2, "Codecname"),
    };
    assert!(Frame::decode(&bad_version.encode().unwrap()).is_err());
    assert!(Frame::decode(&vec![0; benilla_p2p::MAX_PAYLOAD_SIZE + 1]).is_err());
    assert!(!Frame::decode(b"private malformed text")
        .unwrap_err()
        .to_string()
        .contains("private"));
    assert!(Frame::decode(
        &Frame::Say {
            text: "x".repeat(MAX_CHAT + 1)
        }
        .encode()
        .unwrap()
    )
    .is_err());
    assert!(validate_pose(&[f32::NAN, 0.0, 0.0], 0.0).is_err());
}

#[test]
fn two_saved_owner_bound_peers_enter_move_chat_and_only_save_their_own_characters() {
    let da = Temp::new();
    let db = Temp::new();
    let endpoint = free_endpoint();
    let (a, sa) = open(&da.profile(), settings(endpoint, &[]));
    listed(&a);
    let a_id = create(&a, "Peerone", 1, 1, 0);
    enter(&a, a_id);
    let (b, sb) = open(&db.profile(), settings(free_endpoint(), &[endpoint]));
    listed(&b);
    let b_id = create(&b, "Peertwo", 2, 7, 1);
    enter(&b, b_id);
    assert_ne!(a_id, b_id);
    next_matching(
        &a,
        |e| matches!(e, SessionEvent::ObjectCreate { guid, display_id: Some(52), .. } if *guid == b_id),
    );
    next_matching(
        &b,
        |e| matches!(e, SessionEvent::ObjectCreate { guid, .. } if *guid == a_id),
    );
    let moved = [SPAWN[0] + 2.0, SPAWN[1], SPAWN[2]];
    a.commands
        .send(ClientCommand::Move {
            kind: super::super::MoveKind::Heartbeat,
            flags: 0,
            pos: moved,
            orientation: 0.5,
            pitch: 0.0,
            fall_time: 0,
            jump: None,
            transport: None,
        })
        .unwrap();
    next_matching(
        &b,
        |e| matches!(e, SessionEvent::UnitMove { guid, position, .. } if *guid == a_id && *position == moved),
    );
    a.commands
        .send(ClientCommand::Chat {
            kind: ChatKind::Say,
            target: None,
            text: "hello saved peer".into(),
            language: None,
        })
        .unwrap();
    next_matching(
        &b,
        |e| matches!(e, SessionEvent::Chat(m) if m.sender_guid == a_id && m.text == "hello saved peer"),
    );
    drop(sa);
    next_matching(
        &b,
        |e| matches!(e, SessionEvent::ObjectDestroyed(guid) if *guid == a_id),
    );
    drop(sb);
    let a = ProfileStore::open(da.profile()).unwrap();
    let b = ProfileStore::open(db.profile()).unwrap();
    assert_eq!(a.characters().len(), 1);
    assert_eq!(b.characters().len(), 1);
    assert_eq!(a.characters()[0].position.position, moved);
    assert_eq!(b.characters()[0].position.position, SPAWN);
    assert_ne!(a.owner_id(), b.owner_id());
}

#[test]
fn malformed_and_forged_peer_frames_disconnect_only_the_offender() {
    let da = Temp::new();
    let db = Temp::new();
    let endpoint = free_endpoint();
    let (a, sa) = open(&da.profile(), settings(endpoint, &[]));
    listed(&a);
    let ag = create(&a, "Honestone", 1, 1, 0);
    enter(&a, ag);
    let (b, sb) = open(&db.profile(), settings(free_endpoint(), &[endpoint]));
    listed(&b);
    let bg = create(&b, "Honesttwo", 1, 1, 0);
    enter(&b, bg);
    next_matching(
        &a,
        |e| matches!(e, SessionEvent::ObjectCreate { guid, .. } if *guid == bg),
    );
    next_matching(
        &b,
        |e| matches!(e, SessionEvent::ObjectCreate { guid, .. } if *guid == ag),
    );
    for case in 0..4 {
        let dx = Temp::new();
        let mut attacker = ProfileStore::create(dx.profile()).unwrap();
        let xid = attacker
            .create_character(NewCharacter {
                name: "Badfixture".into(),
                race: 1,
                class: 1,
                gender: 0,
                appearance: Appearance::default(),
            })
            .unwrap();
        let config = settings(free_endpoint(), &[endpoint]);
        let signer = attacker.identity();
        let identity = OwnerIdentity::new(signer.public_key(), move |challenge| {
            signer.sign_transport_challenge(challenge)
        })
        .unwrap();
        let network = Network::start_owned(
            TransportConfig {
                listen: config.0.listen,
                peers: config.0.peers.clone(),
                handshake_timeout: Duration::from_secs(2),
                max_peers: 4,
            },
            config.0.psk_bytes(),
            Hello {
                protocol_version: benilla_p2p::PROTOCOL_VERSION,
                network: config.0.network.clone(),
                peer_id: identity.owner_id(),
                ruleset_hash: config.1.fingerprint(),
                content_version: CONTENT_VERSION.into(),
            },
            identity,
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(Instant::now() < deadline, "attacker fixture handshake");
            if matches!(network.try_recv(), Ok(Event::PeerJoined(_))) {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        let mut character = attacker.character(xid).unwrap().clone();
        let payload = match case {
            0 => b"invalid TOML private fixture".to_vec(),
            1 => {
                character.owner_id = [99; 16];
                Frame::Avatar {
                    version: AVATAR_VERSION,
                    character,
                }
                .encode()
                .unwrap()
            }
            2 => {
                character.position.map_id = 1;
                Frame::Avatar {
                    version: AVATAR_VERSION,
                    character,
                }
                .encode()
                .unwrap()
            }
            _ => {
                let payload = Frame::Avatar {
                    version: AVATAR_VERSION,
                    character,
                }
                .encode()
                .unwrap();
                network
                    .try_send(Command::Broadcast(payload.clone()))
                    .unwrap();
                payload
            }
        };
        network.try_send(Command::Broadcast(payload)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(Instant::now() < deadline, "invalid peer must be removed");
            if matches!(network.try_recv(), Ok(Event::PeerLeft(_))) {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        drop(network);
        b.commands
            .send(ClientCommand::Chat {
                kind: ChatKind::Say,
                target: None,
                text: format!("still connected {case}"),
                language: None,
            })
            .unwrap();
        next_matching(
            &a,
            |e| matches!(e, SessionEvent::Chat(m) if m.sender_guid == bg && m.text == format!("still connected {case}")),
        );
    }
    drop(sa);
    drop(sb);
    assert_eq!(
        ProfileStore::open(da.profile()).unwrap().characters().len(),
        1
    );
    assert_eq!(
        ProfileStore::open(db.profile()).unwrap().characters().len(),
        1
    );
}

#[test]
fn periodic_checkpoint_advances_only_after_local_movement() {
    let dir = Temp::new();
    let (handles, start) = open(&dir.profile(), settings(free_endpoint(), &[]));
    listed(&handles);
    let guid = create(&handles, "Autopose", 1, 1, 0);
    enter(&handles, guid);
    let before = std::fs::read(dir.profile().join("profile.json")).unwrap();
    let moved = [SPAWN[0] + 4.0, SPAWN[1], SPAWN[2]];
    handles
        .commands
        .send(ClientCommand::Move {
            kind: super::super::MoveKind::Heartbeat,
            flags: 0,
            pos: moved,
            orientation: 0.4,
            pitch: 0.0,
            fall_time: 0,
            jump: None,
            transport: None,
        })
        .unwrap();
    let deadline = Instant::now() + CHECKPOINT_INTERVAL + Duration::from_secs(3);
    while std::fs::read(dir.profile().join("profile.json")).unwrap() == before {
        assert!(
            Instant::now() < deadline,
            "periodic checkpoint did not commit"
        );
        thread::sleep(Duration::from_millis(100));
    }
    drop(start);
    assert_eq!(
        ProfileStore::open(dir.profile())
            .unwrap()
            .selected_character()
            .unwrap()
            .position
            .position,
        moved
    );
}

#[test]
fn connection_setup_failure_does_not_erase_created_characters_or_regenerate_identity() {
    let dir = Temp::new();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let (handles, start) = open(
        &dir.profile(),
        settings(listener.local_addr().unwrap(), &[]),
    );
    listed(&handles);
    let guid = create(&handles, "Bindfail", 1, 1, 0);
    handles.pick.send(CharRequest::Enter(guid)).unwrap();
    next_matching(&handles, |e| {
        matches!(
            e,
            SessionEvent::Disconnected {
                end: SessionEnd::Lost,
                ..
            }
        )
    });
    drop(start);
    let store = ProfileStore::open(dir.profile()).unwrap();
    assert_eq!(store.characters().len(), 1);
    assert_eq!(character_guid(&store.characters()[0]), guid);
}

#[test]
fn a_truncated_guid_collision_is_refused_even_when_character_records_differ() {
    // Inject the truncated hash result, rather than attempting a SHA-256 birthday attack.
    assert!(validate_character_guid(42, 42, [7, 8].into_iter()).is_err());
    assert!(validate_character_guid(42, 9, [7, 42].into_iter()).is_err());
    assert!(validate_character_guid(42, 9, [7, 8].into_iter()).is_ok());
}

#[test]
fn entering_a_different_saved_character_persists_selection_without_overwriting_the_first() {
    let dir = Temp::new();
    let endpoint = free_endpoint();
    let (handles, start) = open(&dir.profile(), settings(endpoint, &[]));
    listed(&handles);
    let one = create(&handles, "Firstsaved", 1, 1, 0);
    let two = create(&handles, "Othersaved", 2, 7, 1);
    enter(&handles, one);
    handles.commands.send(ClientCommand::Logout).unwrap();
    next_matching(&handles, |e| matches!(e, SessionEvent::LoggedOut));
    assert_eq!(listed(&handles).len(), 2);
    enter(&handles, two);
    next_matching(
        &handles,
        |e| matches!(e, SessionEvent::PlayerName { guid, name, .. } if *guid == two && name == "Othersaved"),
    );
    drop(start);
    let store = ProfileStore::open(dir.profile()).unwrap();
    assert_eq!(store.characters().len(), 2);
    assert_eq!(character_guid(store.selected_character().unwrap()), two);
    assert_eq!(character_guid(&store.characters()[0]), one);
}

#[test]
fn full_application_queue_refuses_avatar_without_blocking() {
    let (tx, _rx) = crossbeam_channel::bounded(1);
    let catalog = AvatarCatalog::fixture();
    emit(
        &tx,
        character_avatar(&fixture(1, 1, "Firstchar"), &catalog).unwrap(),
    )
    .unwrap();
    assert!(emit(
        &tx,
        character_avatar(&fixture(2, 2, "Nextchar"), &catalog).unwrap()
    )
    .is_err());
}

#[test]
fn confirmed_delete_refreshes_the_exact_local_roster_and_restart_keeps_the_archive() {
    let dir = Temp::new();
    let (handles, start) = open(&dir.profile(), settings(free_endpoint(), &[]));
    listed(&handles);
    let one = create(&handles, "Firstsaved", 1, 1, 0);
    let two = create(&handles, "Othersaved", 2, 7, 1);
    handles.pick.send(CharRequest::Delete(one)).unwrap();
    let roster = listed(&handles);
    assert_eq!(roster.len(), 1);
    assert_eq!(roster[0].guid, two);
    next_matching(&handles, |event| {
        matches!(
            event,
            SessionEvent::CharActionResult {
                action: benilla_protocol::CharAction::Delete,
                code: benilla_protocol::messages::CHAR_DELETE_SUCCESS,
            }
        )
    });
    handles.pick.send(CharRequest::Delete(one)).unwrap();
    next_matching(&handles, |event| {
        matches!(
            event,
            SessionEvent::CharActionResult {
                action: benilla_protocol::CharAction::Delete,
                code: 0x3a,
            }
        )
    });
    drop(start);
    let store = ProfileStore::open(dir.profile()).unwrap();
    assert_eq!(store.characters().len(), 1);
    assert_eq!(character_guid(store.selected_character().unwrap()), two);
    assert_eq!(character_guid(&store.archived_characters()[0]), one);
    drop(store);
    let (handles, start) = open(&dir.profile(), settings(free_endpoint(), &[]));
    let roster = listed(&handles);
    assert_eq!(roster.len(), 1);
    assert_eq!(roster[0].guid, two);
    drop(start);
}

#[test]
fn exploration_slash_emote_commands_echo_without_a_peer() {
    let temp = Temp::new();
    let (handles, mut start) =
        prepare(settings(free_endpoint(), &[]), P2pEndpointTicket::default());
    let mut request = request(&temp.profile());
    request.catalog.as_mut().unwrap().1 = benilla_formats::EmoteSoundCatalog::default()
        .with_text_row(9001, 8001, 0)
        .with_text_row(9002, 8002, 2);
    start.start_request(request).unwrap();
    listed(&handles);
    let guid = create(&handles, "Emoter", 1, 1, 0);
    enter(&handles, guid);
    for text_id in [9001, 9002] {
        handles
            .commands
            .send(ClientCommand::TextEmote { text_id, target: 0 })
            .unwrap();
        let visual = next_matching(&handles, |e| {
            matches!(
                e,
                SessionEvent::Emote { .. } | SessionEvent::ObjectValues { .. }
            )
        });
        match (text_id, visual) {
            (
                9001,
                SessionEvent::Emote {
                    guid: actor,
                    emote_id: 8001,
                },
            ) => assert_eq!(actor, guid),
            (
                9002,
                SessionEvent::ObjectValues {
                    guid: actor,
                    fields,
                },
            ) => {
                assert_eq!(actor, guid);
                assert_eq!(fields.unit_emote_state(), 8002);
            }
            other => panic!("wrong emote presentation: {other:?}"),
        }
        assert!(
            matches!(next_matching(&handles, |e| matches!(e, SessionEvent::TextEmote { .. })), SessionEvent::TextEmote { guid: actor, text_emote, target_name } if actor == guid && text_emote == text_id && target_name.is_empty())
        );
    }
    for state in [1, 3, 8, 0] {
        handles
            .commands
            .send(ClientCommand::StandStateChange { state })
            .unwrap();
        assert!(
            matches!(next_matching(&handles, |e| matches!(e, SessionEvent::ObjectValues { .. })), SessionEvent::ObjectValues { guid: actor, fields } if actor == guid && fields.unit_stand_state() == state as u8 && fields.unit_emote_state() == 8002)
        );
    }
    handles
        .commands
        .send(ClientCommand::TextEmote {
            text_id: u32::MAX,
            target: 0,
        })
        .unwrap();
    next_matching(
        &handles,
        |e| matches!(e, SessionEvent::Chat(message) if message.text.contains("emote is unavailable")),
    );
}

#[test]
fn a_late_joiner_receives_current_loop_and_posture_without_replayed_text() {
    let first = Temp::new();
    let second = Temp::new();
    let endpoint = free_endpoint();
    let open_with_emote = |root: &Path, settings| {
        let (handles, mut start) = prepare(settings, P2pEndpointTicket::default());
        let mut request = request(root);
        request.catalog.as_mut().unwrap().1 =
            benilla_formats::EmoteSoundCatalog::default().with_text_row(9002, 8002, 2);
        start.start_request(request).unwrap();
        listed(&handles);
        (handles, start)
    };
    let (a, _a_start) = open_with_emote(&first.profile(), settings(endpoint, &[]));
    let actor = create(&a, "Dancer", 1, 1, 0);
    enter(&a, actor);
    a.commands
        .send(ClientCommand::TextEmote {
            text_id: 9002,
            target: 0,
        })
        .unwrap();
    next_matching(&a, |e| matches!(e, SessionEvent::TextEmote { .. }));
    a.commands
        .send(ClientCommand::StandStateChange { state: 8 })
        .unwrap();
    next_matching(
        &a,
        |e| matches!(e, SessionEvent::ObjectValues { fields, .. } if fields.unit_stand_state() == 8),
    );
    let (b, _b_start) = open_with_emote(&second.profile(), settings(free_endpoint(), &[endpoint]));
    let observer = create(&b, "Observer", 1, 1, 0);
    enter(&b, observer);
    next_matching(
        &b,
        |e| matches!(e, SessionEvent::ObjectCreate { guid, .. } if *guid == actor),
    );
    let snapshot = next_matching(
        &b,
        |e| matches!(e, SessionEvent::ObjectValues { guid, .. } | SessionEvent::TextEmote { guid, .. } if *guid == actor),
    );
    assert!(
        matches!(snapshot, SessionEvent::ObjectValues { fields, .. } if fields.unit_stand_state() == 8 && fields.unit_emote_state() == 8002)
    );
    a.commands
        .send(ClientCommand::StandStateChange { state: 0 })
        .unwrap();
    next_matching(
        &b,
        |e| matches!(e, SessionEvent::ObjectValues { guid, fields } if *guid == actor && fields.unit_stand_state() == 0 && fields.unit_emote_state() == 8002),
    );
    // Accepted movement cancels the persistent loop for the actor and its current peers.
    a.commands
        .send(ClientCommand::Move {
            kind: super::super::MoveKind::StartForward,
            flags: 1,
            pos: SPAWN,
            orientation: 0.0,
            pitch: 0.0,
            fall_time: 0,
            jump: None,
            transport: None,
        })
        .unwrap();
    for handles in [&a, &b] {
        next_matching(handles, |e| {
            matches!(e,
            SessionEvent::ObjectValues { guid, fields }
            if *guid == actor && fields.unit_emote_state() == 0)
        });
    }
    next_matching(&b, |e| {
        matches!(e,
        SessionEvent::UnitMove { guid, flags: 1, .. } if *guid == actor)
    });
    // A third peer joining after cancellation receives the cleared state, not the old dance.
    let third = Temp::new();
    let (c, _c_start) = open_with_emote(&third.profile(), settings(free_endpoint(), &[endpoint]));
    let late = create(&c, "Latecomer", 1, 1, 0);
    enter(&c, late);
    next_matching(
        &c,
        |e| matches!(e, SessionEvent::ObjectCreate { guid, .. } if *guid == actor),
    );
    let snapshot = next_matching(&c, |e| {
        matches!(e,
        SessionEvent::ObjectValues { guid, .. } | SessionEvent::TextEmote { guid, .. } if *guid == actor)
    });
    assert!(matches!(snapshot, SessionEvent::ObjectValues { fields, .. }
        if fields.unit_emote_state() == 0 && fields.unit_stand_state() == 0));
}

#[test]
fn exploration_clock_publishes_configured_wall_minute_to_stock_consumers() {
    let (send, receive) = crossbeam_channel::unbounded();
    let sample = benilla_config::world_clock::WorldClockSample {
        hours: 23,
        minutes: 59,
        day_serial: 739_904,
    };
    publish_exploration_clock(&send, sample).unwrap();
    match receive.try_recv().unwrap() {
        SessionEvent::TimeSpeed {
            hours,
            minutes,
            day_serial,
            timescale,
        } => {
            assert_eq!((hours, minutes, day_serial), (23, 59, 739_904));
            assert_eq!(timescale, 1.0 / 60.0);
        }
        other => panic!("unexpected clock event: {other:?}"),
    }
    assert!(receive.try_recv().is_err());
}
