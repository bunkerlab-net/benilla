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
        position: SpawnPolicy::NorthshireMeetingV1.position().unwrap(),
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
fn asynchronous_retirement_releases_profile_and_preserves_character() {
    let dir = Temp::new();
    let (handles, mut start) = open(&dir.profile(), settings(free_endpoint(), &[]));
    listed(&handles);
    let guid = create(&handles, "Reloadpose", 1, 1, 0);
    enter(&handles, guid);
    let moved = [SPAWN[0] + 7.0, SPAWN[1], SPAWN[2]];
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
    start.begin_retirement();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(result) = start.finish_retirement() {
            result.unwrap();
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    let store = ProfileStore::open(dir.profile()).unwrap();
    assert_eq!(store.selected_character().unwrap().position.position, moved);
    drop(store);
    let (new_handles, _new_start) = open(&dir.profile(), settings(free_endpoint(), &[]));
    assert_eq!(listed(&new_handles)[0].guid, guid);
}

fn retire(start: &mut P2pStart) -> std::result::Result<(), String> {
    start.begin_retirement();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(result) = start.finish_retirement() {
            return result;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn bind_failure_does_not_block_settings_retirement() {
    let dir = Temp::new();
    let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let (handles, mut start) = open(
        &dir.profile(),
        settings(occupied.local_addr().unwrap(), &[]),
    );
    listed(&handles);
    let guid = create(&handles, "Bindretry", 1, 1, 0);
    handles.pick.send(CharRequest::Enter(guid)).unwrap();
    next_matching(&handles, |event| {
        matches!(
            event,
            SessionEvent::Disconnected {
                end: SessionEnd::Lost,
                ..
            }
        )
    });
    assert!(retire(&mut start).is_ok());
    let (handles, _start) = open(&dir.profile(), settings(free_endpoint(), &[]));
    assert_eq!(listed(&handles)[0].guid, guid);
}

#[test]
fn failed_final_checkpoint_blocks_replacement_and_preserves_disk() {
    let dir = Temp::new();
    let (handles, mut start) = open(&dir.profile(), settings(free_endpoint(), &[]));
    listed(&handles);
    let guid = create(&handles, "Saveguard", 1, 1, 0);
    enter(&handles, guid);
    let path = dir.profile().join("profile.json");
    let external = b"external mutation must not be overwritten";
    std::fs::write(&path, external).unwrap();
    let error = retire(&mut start).unwrap_err();
    assert!(error.contains("could not save local position"));
    assert_eq!(std::fs::read(&path).unwrap(), external);
    assert!(start.finish_retirement().unwrap().is_err());
}

#[test]
fn retirement_reports_worker_failure_instead_of_claiming_checkpoint() {
    let (_, mut start) = prepare(settings(free_endpoint(), &[]), P2pEndpointTicket::default());
    *start.failure.lock().unwrap() = Some("checkpoint denied".into());
    start.begin_retirement();
    let error = start.finish_retirement().unwrap().unwrap_err();
    assert!(error.contains("checkpoint denied"));
    assert!(!start.can_start());
}

#[test]
fn unchanged_settings_cannot_hide_a_checkpoint_failure() {
    use crate::net::reconfigure::{ApplyStatus, NetworkReconfigure};
    let settings = settings(free_endpoint(), &[]);
    let config = benilla_config::AppConfig {
        p2p: Some(settings.0.clone()),
        wow: Default::default(),
        ruleset: settings.1,
        native_regeneration: None,
    };
    let (_, start) = prepare(settings.clone(), P2pEndpointTicket::default());
    *start.failure.lock().unwrap() = Some("unverified final checkpoint".into());
    let mut reload = NetworkReconfigure::default();
    reload.request(config, false);
    let mut world = bevy::prelude::World::new();
    world.insert_resource(settings);
    world.insert_resource(start);
    world.insert_resource(reload);
    crate::net::reconfigure::drive(&mut world);
    assert!(matches!(
        world.resource::<NetworkReconfigure>().status,
        ApplyStatus::Failed(_)
    ));
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
    let mut dwarf = other.clone();
    dwarf.race = 3;
    dwarf.appearance.facial_hair = 9;
    assert!(validate_peer_character([2; 16], &dwarf, &own, &BTreeMap::new(), &cat).is_ok());
    dwarf.appearance.facial_hair = 10;
    assert!(validate_peer_character([2; 16], &dwarf, &own, &BTreeMap::new(), &cat).is_err());
    let peers = [([2; 16], Some(other.clone()))].into();
    assert!(validate_peer_character([2; 16], &other, &own, &peers, &cat).is_err());
    for change in [0, 1, 2, 3, 4] {
        let mut bad = other.clone();
        match change {
            0 => bad.level = 60,
            1 => bad.class = 11,
            2 => bad.appearance.face = 14,
            3 => bad.position.map_id = 999,
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
    let say = Frame::Say {
        language: 7,
        text: "hi".into(),
    }
    .encode()
    .unwrap();
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
fn different_map_peer_stays_admitted_without_local_events_and_rejoin_can_be_visible() {
    let own = fixture(1, 1, "Local");
    let mut other = fixture(2, 2, "Remote");
    other.position.map_id = 1;
    let cat = AvatarCatalog::fixture();
    let mut peers = [(other.owner_id, None)].into();
    let (tx, rx) = crossbeam_channel::unbounded();
    let avatar = Frame::Avatar {
        version: AVATAR_VERSION,
        character: other.clone(),
    }
    .encode()
    .unwrap();
    apply_peer_frame(&tx, other.owner_id, &avatar, &mut peers, &own, &cat).unwrap();
    assert_eq!(
        peers[&other.owner_id],
        Some(other.clone()),
        "off-map admission must not request transport rejection"
    );
    assert!(rx.is_empty(), "no off-map name or avatar is published");
    for frame in [
        Frame::Move {
            position: SPAWN,
            orientation: 0.0,
            flags: 0,
            pitch: 0.0,
            fall_time: 0,
            jump: None,
            heartbeat: true,
            time: 1,
        },
        Frame::Say {
            language: 7,
            text: "not local chat".into(),
        },
        Frame::TextEmote {
            text_id: 1,
            target: character_guid(&own),
        },
        Frame::Presentation {
            stand_state: 1,
            state_emote: None,
        },
        Frame::EmoteClock {
            text_id: 1,
            generation: 1,
            elapsed_ms: 1000,
        },
    ] {
        apply_peer_frame(
            &tx,
            other.owner_id,
            &frame.encode().unwrap(),
            &mut peers,
            &own,
            &cat,
        )
        .unwrap();
        assert!(
            rx.is_empty(),
            "off-map movement, say and emotes stay map-local"
        );
        assert!(peers[&other.owner_id].is_some());
    }
    assert!(
        apply_peer_frame(&tx, other.owner_id, b"malformed", &mut peers, &own, &cat).is_err(),
        "off-map peers still undergo frame validation"
    );
    // Maps change only between sessions in this protocol. A fresh same-map introduction
    // after leaving/rejoining publishes the remote avatar normally.
    peers.remove(&other.owner_id);
    other.position.map_id = own.position.map_id;
    peers.insert(other.owner_id, None);
    let avatar = Frame::Avatar {
        version: AVATAR_VERSION,
        character: other.clone(),
    }
    .encode()
    .unwrap();
    apply_peer_frame(&tx, other.owner_id, &avatar, &mut peers, &own, &cat).unwrap();
    let events: Vec<_> = rx.try_iter().collect();
    assert!(events.iter().any(|event| matches!(event, SessionEvent::ObjectCreate { guid, .. } if *guid == character_guid(&other))));
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
            language: 7,
            text: "x".repeat(MAX_CHAT + 1)
        }
        .encode()
        .unwrap()
    )
    .is_err());
    assert!(validate_pose(&[f32::NAN, 0.0, 0.0], 0.0).is_err());
}

#[test]
fn spoken_language_frames_reject_legacy_and_unlearned_language_without_emission() {
    assert!(Frame::decode(b"kind = 'Say'\ntext = 'old universal speech'").is_err());
    assert_eq!(CONTENT_VERSION, "benilla-saved-exploration-languages-v8");
    let own = fixture(1, 1, "Listener");
    let mut other = fixture(2, 2, "Speaker");
    other.race = 5;
    // Peer admission owns this immutable race. The frame cannot substitute a different race.
    let mut peers = [(other.owner_id, Some(other.clone()))].into();
    let (tx, rx) = crossbeam_channel::unbounded();
    for language in [0, 7, u32::MAX, 999] {
        let frame = Frame::Say {
            language,
            text: "forged fluent speech".into(),
        }
        .encode()
        .unwrap();
        assert!(apply_peer_frame(
            &tx,
            other.owner_id,
            &frame,
            &mut peers,
            &own,
            &AvatarCatalog::fixture()
        )
        .is_err());
        assert!(rx.is_empty());
    }
    for language in [1, 33] {
        let frame = Frame::Say {
            language,
            text: "native tongue".into(),
        }
        .encode()
        .unwrap();
        apply_peer_frame(
            &tx,
            other.owner_id,
            &frame,
            &mut peers,
            &own,
            &AvatarCatalog::fixture(),
        )
        .unwrap();
        let SessionEvent::Chat(chat) = rx.try_recv().unwrap() else {
            panic!("speech")
        };
        assert_eq!(chat.sender_guid, character_guid(&other));
        assert_eq!(chat.language, language);
        assert_eq!(chat.text, "native tongue");
        assert!(rx.is_empty());
    }
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
    // Racial defaults put the Orc on Kalimdor. This ownership/movement/chat fixture
    // deliberately saves both peers on one map; cross-map avatars must stay invisible.
    drop(sb);
    drop(b);
    {
        let mut store = ProfileStore::open(db.profile()).unwrap();
        let id = store.characters()[0].id;
        assert_eq!(store.characters()[0].position.map_id, 1);
        store
            .save_position(id, SpawnPolicy::NorthshireMeetingV1.position().unwrap())
            .unwrap();
    }
    let (b, sb) = open(&db.profile(), settings(free_endpoint(), &[endpoint]));
    assert_eq!(listed(&b)[0].guid, b_id);
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
                ruleset_hash: config.0.timezone.exploration_fingerprint(config.1),
                content_version: CONTENT_VERSION.into(),
            },
            identity,
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(Instant::now() < deadline, "attacker fixture handshake");
            match network.try_recv() {
                Ok(Event::PeerJoined(_)) => break,
                Ok(Event::Rejected { reason, .. }) => {
                    panic!("attacker fixture handshake rejected: {reason}");
                }
                _ => {}
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
                character.position.map_id = 999;
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

#[test]
fn private_whisper_frame_codec_rejects_invalid_text_and_forged_fields() {
    let packet = Frame::Whisper {
        message: whispers::Packet::Tell {
            sender: [1; 16],
            recipient: [2; 16],
            sequence: 1,
            text: "private".into(),
        },
    };
    let bytes = packet.encode().unwrap();
    assert!(matches!(
        Frame::decode(&bytes).unwrap(),
        Frame::Whisper { .. }
    ));
    let encoded = String::from_utf8(bytes).unwrap();
    assert!(Frame::decode(
        format!("{encoded}\nspoofed_sender_name = \"Someoneelse\"\n").as_bytes()
    )
    .is_err());
    for text in [String::new(), "x".repeat(256), "private\ntext".into()] {
        let bytes = Frame::Whisper {
            message: whispers::Packet::Tell {
                sender: [1; 16],
                recipient: [2; 16],
                sequence: 1,
                text,
            },
        }
        .encode()
        .unwrap();
        assert!(Frame::decode(&bytes).is_err());
    }
}

#[test]
fn three_exploration_peers_whisper_and_reply_without_bystander_delivery() {
    let da = Temp::new();
    let db = Temp::new();
    let dc = Temp::new();
    let endpoint = free_endpoint();
    let (a, sa) = open(&da.profile(), settings(endpoint, &[]));
    listed(&a);
    let a_id = create(&a, "Whisperone", 1, 1, 0);
    enter(&a, a_id);
    let (b, sb) = open(&db.profile(), settings(free_endpoint(), &[endpoint]));
    listed(&b);
    let b_id = create(&b, "Whispertwo", 1, 1, 0);
    enter(&b, b_id);
    next_matching(
        &a,
        |e| matches!(e, SessionEvent::ObjectCreate { guid, .. } if *guid == b_id),
    );
    next_matching(
        &b,
        |e| matches!(e, SessionEvent::ObjectCreate { guid, .. } if *guid == a_id),
    );
    let (c, sc) = open(&dc.profile(), settings(free_endpoint(), &[endpoint]));
    listed(&c);
    let c_id = create(&c, "Whisperthree", 1, 1, 0);
    enter(&c, c_id);
    next_matching(
        &a,
        |e| matches!(e, SessionEvent::ObjectCreate { guid, .. } if *guid == c_id),
    );
    next_matching(
        &c,
        |e| matches!(e, SessionEvent::ObjectCreate { guid, .. } if *guid == a_id),
    );
    a.commands
        .send(ClientCommand::Chat {
            kind: ChatKind::Whisper,
            target: Some("Whispertwo".into()),
            text: "private one".into(),
            language: None,
        })
        .unwrap();
    next_matching(
        &b,
        |e| matches!(e, SessionEvent::Chat(m) if m.chat_type == benilla_protocol::messages::CHAT_MSG_WHISPER && m.sender_guid == a_id && m.text == "private one"),
    );
    next_matching(
        &a,
        |e| matches!(e, SessionEvent::Chat(m) if m.chat_type == benilla_protocol::messages::CHAT_MSG_WHISPER_INFORM && m.sender_guid == b_id && m.text == "private one"),
    );
    // Stock /r resolves the last incoming name and emits this same WHISPER command.
    b.commands
        .send(ClientCommand::Chat {
            kind: ChatKind::Whisper,
            target: Some("Whisperone".into()),
            text: "private reply".into(),
            language: None,
        })
        .unwrap();
    next_matching(
        &a,
        |e| matches!(e, SessionEvent::Chat(m) if m.chat_type == benilla_protocol::messages::CHAT_MSG_WHISPER && m.sender_guid == b_id && m.text == "private reply"),
    );
    a.commands
        .send(ClientCommand::Chat {
            kind: ChatKind::Say,
            target: None,
            text: "public fence".into(),
            language: None,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let event = c
            .events
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
        if let SessionEvent::Chat(m) = event {
            assert!(!m.text.starts_with("private"), "bystander received whisper");
            if m.text == "public fence" {
                break;
            }
        }
    }
    drop((sa, sb, sc));
}

#[test]
fn dwarf_catalog_beard_creates_reopens_and_invalid_options_never_write() {
    let dir = Temp::new();
    let (handles, mut start) = open(&dir.profile(), settings(free_endpoint(), &[]));
    assert!(listed(&handles).is_empty());
    let request = benilla_protocol::CharCreateReq {
        name: "Dwarflook".into(),
        race: 3,
        class: 1,
        gender: 0,
        skin: 0,
        face: 0,
        hair_style: 0,
        hair_color: 0,
        facial_hair: 9,
    };
    handles.pick.send(CharRequest::Create(request)).unwrap();
    let characters = listed(&handles);
    assert_eq!(characters.len(), 1);
    assert_eq!(characters[0].facial_hair, 9);
    let guid = characters[0].guid;
    next_matching(&handles, |event| {
        matches!(
            event,
            SessionEvent::CharActionResult {
                action: benilla_protocol::CharAction::Create,
                code: benilla_protocol::messages::CHAR_CREATE_SUCCESS
            }
        )
    });
    let before = std::fs::read(dir.profile().join("profile.json")).unwrap();
    // Both the former profile-level skin rejection and the next beard outside this
    // synthetic catalog must still fail at the actual app acceptance boundary.
    for (skin, facial_hair) in [(16, 9), (0, 10)] {
        handles
            .pick
            .send(CharRequest::Create(benilla_protocol::CharCreateReq {
                name: "Invalidlook".into(),
                race: 3,
                class: 1,
                gender: 0,
                skin,
                face: 0,
                hair_style: 0,
                hair_color: 0,
                facial_hair,
            }))
            .unwrap();
        next_matching(&handles, |event| {
            matches!(
                event,
                SessionEvent::CharActionResult {
                    action: benilla_protocol::CharAction::Create,
                    code: 0x30
                }
            )
        });
        assert_eq!(
            std::fs::read(dir.profile().join("profile.json")).unwrap(),
            before
        );
    }
    retire(&mut start).unwrap();
    let (handles, mut start) = open(&dir.profile(), settings(free_endpoint(), &[]));
    let reopened = listed(&handles);
    assert_eq!(reopened.len(), 1);
    assert_eq!(reopened[0].guid, guid);
    assert_eq!(reopened[0].facial_hair, 9);
    retire(&mut start).unwrap();
}

#[test]
fn installed_creation_dials_fit_profile_and_avatar_validation() {
    let data = benilla_formats::wow_data_or_skip!();
    let mut chain = benilla_formats::open_chain(&data).expect("open chain");
    let catalog = benilla_formats::CharCreateCatalog::load(&mut chain).expect("creation catalog");
    let avatars = AvatarCatalog::from_client(&catalog, None);
    for race in 1..=8 {
        for gender in [0, 1] {
            let ranges = catalog.ranges(race, gender).expect("playable body");
            let counts = [
                ranges.skin,
                ranges.face,
                ranges.hair_style,
                ranges.hair_color,
                ranges.facial_hair,
            ];
            for class in catalog.classes_for_race(race) {
                for (axis, count) in counts.into_iter().enumerate() {
                    for value in 0..count {
                        let mut dials = [0; 5];
                        dials[axis] = value;
                        // Colours are style-dependent just as on the stock create screen.
                        if dials[3] >= catalog.hair_colors(race, gender, dials[2]) {
                            continue;
                        }
                        let input = NewCharacter {
                            name: "Cataloglook".into(),
                            race,
                            class,
                            gender,
                            appearance: Appearance {
                                skin: dials[0],
                                face: dials[1],
                                hair_style: dials[2],
                                hair_color: dials[3],
                                facial_hair: dials[4],
                            },
                        };
                        input
                            .validate()
                            .expect("catalog choice must fit profile schema");
                        avatars
                            .validate(race, class, gender, input.appearance)
                            .expect("catalog choice must fit avatar");
                    }
                }
            }
        }
    }
}

#[test]
fn human_and_forsaken_peers_refuse_whispers_both_ways_and_preserve_say_languages() {
    use benilla_protocol::messages::{CHAT_MSG_SAY, CHAT_MSG_WHISPER, CHAT_MSG_WHISPER_INFORM};
    let human_dir = Temp::new();
    let undead_dir = Temp::new();
    let endpoint = free_endpoint();
    let (human, human_start) = open(&human_dir.profile(), settings(endpoint, &[]));
    listed(&human);
    let human_id = create(&human, "Humanpeer", 1, 1, 0);
    enter(&human, human_id);
    let (undead, undead_start) = open(
        &undead_dir.profile(),
        settings(free_endpoint(), &[endpoint]),
    );
    listed(&undead);
    let undead_id = create(&undead, "Forsakenpeer", 5, 1, 0);
    enter(&undead, undead_id);
    next_matching(
        &human,
        |event| matches!(event, SessionEvent::ObjectCreate { guid, .. } if *guid == undead_id),
    );
    next_matching(
        &undead,
        |event| matches!(event, SessionEvent::ObjectCreate { guid, .. } if *guid == human_id),
    );

    for (sender, receiver, target, sender_id, language) in [
        (&human, &undead, "Forsakenpeer", human_id, 7),
        (&undead, &human, "Humanpeer", undead_id, 1),
    ] {
        sender
            .commands
            .send(ClientCommand::Chat {
                kind: ChatKind::Whisper,
                target: Some(target.into()),
                text: "forbidden private line".into(),
                language: None,
            })
            .unwrap();
        next_matching(sender, |event| {
            if let SessionEvent::Chat(chat) = event {
                assert!(!matches!(
                    chat.chat_type,
                    CHAT_MSG_WHISPER | CHAT_MSG_WHISPER_INFORM
                ));
            }
            matches!(event, SessionEvent::ChatWrongFaction)
        });
        // A subsequent public line fences the peer transport without assuming a sleep duration.
        sender
            .commands
            .send(ClientCommand::Chat {
                kind: ChatKind::Say,
                target: None,
                text: "language fence".into(),
                language: None,
            })
            .unwrap();
        next_matching(receiver, |event| {
            let SessionEvent::Chat(chat) = event else {
                return false;
            };
            assert!(!matches!(
                chat.chat_type,
                CHAT_MSG_WHISPER | CHAT_MSG_WHISPER_INFORM
            ));
            if chat.chat_type == CHAT_MSG_SAY
                && chat.sender_guid == sender_id
                && chat.text == "language fence"
            {
                assert_eq!(chat.language, language);
                return true;
            }
            false
        });
    }
    drop((human_start, undead_start));
}
