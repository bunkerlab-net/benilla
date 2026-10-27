//! Fork-only direct-peer backend. Each member owns only its own avatar's pose; these are
//! trusted-friends exploration sessions, not authoritative combat or a complete WoW realm.
//! Saved-character sessions bind group admission to the profile owner's signing identity.

pub(crate) use native_geometry::run_collision_diagnostics;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use benilla_p2p::{Command, Event, Hello, Network, OwnerIdentity, PeerId, TransportConfig};
use benilla_profile::{Character, ProfileStore};
use benilla_protocol::{EntityKind, JumpInfo, MoveSpeeds, ObjectFields, SessionEnd, SessionEvent};
use crossbeam_channel::{Receiver, Sender};
use serde::{Deserialize, Serialize};

use super::{io, ChatKind, ClientCommand, P2pSettings, ParkRequest, ParkSender};

mod character_actions;
mod creation_start;
mod emote_motion;
mod emotes;
mod encounter;
#[path = "p2p/world/native_geometry/mod.rs"]
mod native_geometry;
mod recovery_actions;
use character_actions::character_guid;
mod profile;
mod speech;
mod whispers;
mod world_setup;
// Native entry is deliberately closed until a supported compiler/runtime profile exists.
mod world;
pub(super) use native_geometry::plugin as native_geometry_plugin;
use profile::AvatarCatalog;

// Version of this fixed exploration world contract, not an attestation of local WoW assets.
const CONTENT_VERSION: &str = "benilla-saved-exploration-languages-v8";
const MAX_CHAT: usize = 255;
const SPAWN: [f32; 3] = [-8949.95, -132.49, 83.9];
/// Public connection metadata plus the local setup mailbox; no signing keys or PSKs.
/// Connection metadata is revealed only by deliberate UI action, never persisted or logged.
#[derive(bevy::prelude::Resource, Clone, Default)]
pub(crate) struct P2pEndpointTicket(
    Arc<Mutex<EndpointState>>,
    benilla_ui::script::world_setup::WorldSetupMailbox,
);
#[derive(Default)]
struct EndpointState {
    ticket: Option<String>,
    listen: Option<std::net::SocketAddr>,
}
impl P2pEndpointTicket {
    pub(crate) fn world_setup(&self) -> benilla_ui::script::world_setup::WorldSetupMailbox {
        self.1.clone()
    }
    pub(crate) fn get(&self) -> Option<String> {
        self.0.lock().ok().and_then(|value| value.ticket.clone())
    }
    fn set(&self, value: Option<String>) {
        if let Ok(mut current) = self.0.lock() {
            current.ticket = value;
        }
    }
    pub(crate) fn local_addr(&self) -> Option<std::net::SocketAddr> {
        self.0.lock().ok().and_then(|state| state.listen)
    }
    fn set_listen(&self, listen: std::net::SocketAddr) {
        if let Ok(mut state) = self.0.lock() {
            state.listen = Some(listen);
        }
    }
    pub(super) fn clear(&self) {
        if let Ok(mut state) = self.0.lock() {
            *state = EndpointState::default();
        }
    }
}
struct ClearTicket(P2pEndpointTicket);
impl Drop for ClearTicket {
    fn drop(&mut self) {
        self.0.clear();
    }
}

const HELP: &str = "P2P exploration: saved local characters, movement, /say and private /w (or /whisper, /r). New characters use their racial starting locations by default; optional private captures override new starts. Existing characters keep their saved positions. Position saves every 10 seconds and on logout. Combat, quests, XP, loot, spells and inventory progression are not implemented.";
const AVATAR_VERSION: u16 = 1;
const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(10);

/// TOML keeps this small initial protocol inspectable, but never shares the config or its PSK.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum Frame {
    Avatar {
        version: u16,
        character: Character,
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
        language: u32,
        text: String,
    },
    Whisper {
        message: whispers::Packet,
    },
    TextEmote {
        text_id: u32,
        target: u64,
    },
    EmoteClock {
        text_id: u32,
        generation: u64,
        elapsed_ms: u32,
    },
    Presentation {
        stand_state: u8,
        state_emote: Option<u32>,
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
            Self::Avatar { version, character } => {
                if *version != AVATAR_VERSION {
                    bail!("unsupported P2P character version");
                }
                character.validate()?;
            }
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
            Self::Whisper { message } => message.validate()?,
            Self::TextEmote { .. } => {}
            Self::EmoteClock {
                generation,
                elapsed_ms,
                ..
            } => {
                if *generation == 0 || *elapsed_ms > emotes::MAX_CLOCK_MS {
                    bail!("invalid P2P emote clock");
                }
            }
            Self::Presentation { stand_state, .. } => {
                if !emotes::valid_stand_state(u32::from(*stand_state)) {
                    bail!("invalid P2P stand state");
                }
            }
            Self::Say { text, .. } => {
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
            // PLAYER_BYTES_2 byte 3: normal rest state, never invalid zero.
            (194, 2 << 24),
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

/// A final checkpoint failed; unlike a transport error, replacement cannot prove profile safety.
#[derive(Debug)]
struct CheckpointFailed;
impl std::fmt::Display for CheckpointFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "could not save local position; session stopped, check local profile before rejoining",
        )
    }
}
impl std::error::Error for CheckpointFailed {}

/// An inert local-character gate. Opening options cannot create a profile or a transport.
#[derive(bevy::prelude::Resource)]
pub(crate) struct P2pStart {
    pending: Option<PendingStart>,
    requests: Sender<StartRequest>,
    ready: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    generation: Arc<AtomicU64>,
    local_zone: Arc<AtomicU32>,
    worker: Option<thread::JoinHandle<()>>,
    failure: Arc<Mutex<Option<String>>>,
}
struct StartRequest {
    root: Option<std::path::PathBuf>,
    catalog: Option<AvatarCatalog>,
    generation: u64,
}
struct PendingStart {
    settings: P2pSettings,
    ticket: P2pEndpointTicket,
    events: Sender<SessionEvent>,
    commands: Receiver<ClientCommand>,
    park: Receiver<ParkRequest>,
    login: Receiver<io::LoginRequest>,
    requests: Receiver<StartRequest>,
}
impl P2pStart {
    pub(crate) fn can_start(&self) -> bool {
        self.ready.load(Ordering::SeqCst) && !self.stop.load(Ordering::SeqCst)
    }
    pub(crate) fn start(
        &mut self,
        catalog: Option<&benilla_formats::CharCreateCatalog>,
        emotes: Option<&benilla_formats::EmoteSoundCatalog>,
    ) -> std::result::Result<(), String> {
        let synthetic = self
            .pending
            .as_ref()
            .is_some_and(|p| p.settings.0.encounter.is_some());
        let request = if synthetic {
            StartRequest {
                root: None,
                catalog: None,
                generation: self.generation.load(Ordering::SeqCst),
            }
        } else {
            let catalog = catalog.ok_or(
                "Character data is unavailable. Check your game data in Connection setup.",
            )?;
            let root = crate::local_state::home()
                .ok_or("No local profile directory is available.")?
                .join("p2p-profile");
            StartRequest {
                root: Some(root),
                catalog: Some(AvatarCatalog::from_client(catalog, emotes)),
                generation: self.generation.load(Ordering::SeqCst),
            }
        };
        self.start_request(request)
    }
    fn start_request(&mut self, request: StartRequest) -> std::result::Result<(), String> {
        if let Some(error) = self.retirement_failure() {
            return Err(format!(
                "Saved profile needs attention before reopening: {error}"
            ));
        }
        if self.stop.load(Ordering::SeqCst)
            || self
                .ready
                .compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
        {
            return Err("P2P characters are already open or the session is closing.".into());
        }
        if let Some(pending) = self.pending.take() {
            let (ready, stop, zone) = (
                self.ready.clone(),
                self.stop.clone(),
                self.local_zone.clone(),
            );
            let failure = self.failure.clone();
            self.worker = Some(
                thread::Builder::new()
                    .name("benilla-peer-world".into())
                    .spawn(move || {
                        let PendingStart {
                            settings,
                            ticket,
                            events,
                            commands,
                            park,
                            login,
                            requests,
                        } = pending;
                        // Keep the realm UI channel alive, but never submit or forward its credentials.
                        let _login = login;
                        let _clear_ticket = ClearTicket(ticket.clone());
                        while !stop.load(Ordering::SeqCst) {
                            let request = match requests.recv_timeout(Duration::from_millis(20)) {
                                Ok(request) => request,
                                Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                            };
                            let result = if settings.0.encounter.is_some() {
                                encounter::run(settings.clone(), &events, &commands, &stop)
                            } else if let (Some(root), Some(catalog)) =
                                (request.root, request.catalog)
                            {
                                profile::browse(
                                    settings.clone(),
                                    &root,
                                    &catalog,
                                    request.generation,
                                    &events,
                                    &commands,
                                    &park,
                                    &ticket,
                                    &stop,
                                    &zone,
                                )
                            } else {
                                Err(anyhow::anyhow!("saved character setup is missing"))
                            };
                            ticket.clear();
                            if let Err(error) = result {
                                bevy::log::error!("p2p: {error:#}");
                                if error.downcast_ref::<CheckpointFailed>().is_some() {
                                    if let Ok(mut failure) = failure.lock() {
                                        *failure = Some(format!("{error:#}"));
                                    }
                                }
                                let _ = events.try_send(SessionEvent::Disconnected {
                                    reason: format!("P2P session ended: {error:#}"),
                                    end: SessionEnd::Lost,
                                });
                            }
                            if settings.0.encounter.is_some() {
                                stop.store(true, Ordering::SeqCst);
                            }
                            ready.store(true, Ordering::SeqCst);
                        }
                    })
                    .map_err(|error| {
                        self.stop.store(true, Ordering::SeqCst);
                        format!("Could not start P2P characters: {error}. Restart to retry.")
                    })?,
            );
        }
        self.requests
            .send(request)
            .map_err(|_| "P2P character service stopped. Restart to retry.".to_string())
    }
    pub(super) fn retirement_failure(&self) -> Option<String> {
        match self.failure.lock() {
            Ok(failure) => failure.clone(),
            Err(_) => Some(
                "Could not verify the saved profile; restart after checking local state.".into(),
            ),
        }
    }
    /// Stop without blocking the render thread. The worker checkpoints before it exits.
    pub(super) fn begin_retirement(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
    /// Joining is safe only after `is_finished`; an error must prevent replacement.
    pub(super) fn finish_retirement(&mut self) -> Option<std::result::Result<(), String>> {
        if self
            .worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
        {
            return None;
        }
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                let error = "Character worker stopped unexpectedly; verify the saved profile before restarting.".to_string();
                if let Ok(mut failure) = self.failure.lock() {
                    *failure = Some(error.clone());
                }
                return Some(Err(error));
            }
        }
        Some(match self.failure.lock() {
            Ok(failure) => match failure.as_ref() {
                Some(error) => Err(format!(
                    "Session could not finish safely: {error}. Network changes were not applied."
                )),
                None => Ok(()),
            },
            Err(_) => {
                Err("Could not verify the saved profile; network changes were not applied.".into())
            }
        })
    }
    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            // The backend checks this flag in its bounded receive loop and checkpoints before exit.
            if worker.join().is_err() {
                bevy::log::error!(
                    "p2p: character worker stopped unexpectedly; verify the last saved position"
                );
            }
        }
    }
}
impl Drop for P2pStart {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub(super) fn shutdown(mut start: Option<bevy::prelude::ResMut<P2pStart>>) {
    if let Some(start) = start.as_deref_mut() {
        start.shutdown();
    }
}
/// Zone metadata comes from the app's existing player-area authority, never a remote frame.
pub(super) fn refresh_local_zone(
    start: Option<bevy::prelude::Res<P2pStart>>,
    zone: Option<bevy::prelude::Res<crate::area::CurrentPlayerZone>>,
    player: bevy::prelude::Res<super::SelfGuid>,
) {
    if player.0.is_none() {
        return;
    }
    if let (Some(start), Some(zone)) = (start, zone.and_then(|zone| zone.0)) {
        start.local_zone.store(zone, Ordering::SeqCst);
    }
}

/// Channels only; profile IO is deliberate Start, and transport IO is deliberate Enter World.
pub(super) fn prepare(
    settings: P2pSettings,
    ticket: P2pEndpointTicket,
) -> (io::NetHandles, P2pStart) {
    let (events_tx, events) = crossbeam_channel::bounded(1024);
    let (commands, commands_rx) = crossbeam_channel::unbounded();
    let (park_tx, park_rx) = crossbeam_channel::unbounded::<ParkRequest>();
    let (login, login_rx) = crossbeam_channel::unbounded();
    let (requests, requests_rx) = crossbeam_channel::bounded(1);
    let generation = Arc::new(AtomicU64::new(0));
    let handles = io::NetHandles {
        events,
        commands,
        pick: ParkSender::park(park_tx.clone()),
        realm: ParkSender::park(park_tx),
        screen_asks_roster: Arc::new(AtomicBool::new(false)),
        login,
        login_abandon: generation.clone(),
        ping: Arc::new(Mutex::new(io::PingClock::default())),
    };
    let start = P2pStart {
        pending: Some(PendingStart {
            settings,
            ticket,
            events: events_tx,
            commands: commands_rx,
            park: park_rx,
            login: login_rx,
            requests: requests_rx,
        }),
        requests,
        ready: Arc::new(AtomicBool::new(true)),
        stop: Arc::new(AtomicBool::new(false)),
        generation,
        local_zone: Arc::new(AtomicU32::new(0)),
        worker: None,
        failure: Arc::new(Mutex::new(None)),
    };
    (handles, start)
}

fn character_name(character: &Character) -> SessionEvent {
    SessionEvent::PlayerName {
        guid: character_guid(character),
        name: character.name.clone(),
        race: u32::from(character.race),
        gender: u32::from(character.gender),
        class: u32::from(character.class),
    }
}
fn character_avatar(character: &Character, catalog: &AvatarCatalog) -> Result<SessionEvent> {
    let (display, faction) = catalog.body(character)?;
    let id = character_guid(character);
    let power: u32 = match character.class {
        1 => 1,
        4 => 3,
        _ => 0,
    };
    let appearance = character.appearance;
    // Exploration baselines only, not a claimed implementation of racial/class starting stats.
    let (current, maximum) = if power == 1 { (0, 1000) } else { (100, 100) };
    let mut field_pairs = vec![
        (0, id as u32),
        (1, (id >> 32) as u32),
        (2, 25),
        (4, 1.0f32.to_bits()),
        (22, 100),
        (28, 100),
        (23 + power as u16, current),
        (29 + power as u16, maximum),
        (34, u32::from(character.level)),
        (35, faction),
        (
            36,
            u32::from(character.race)
                | (u32::from(character.class) << 8)
                | (u32::from(character.gender) << 16)
                | (power << 24),
        ),
        (126, 2000),
        (131, display),
        (132, display),
        (
            193,
            u32::from_le_bytes([
                appearance.skin,
                appearance.face,
                appearance.hair_style,
                appearance.hair_color,
            ]),
        ),
        (194, u32::from(appearance.facial_hair) | (2 << 24)),
        (716, 0),
        (717, 400),
    ];
    speech::append_skills(character.race, &mut field_pairs);
    let fields = ObjectFields::from_pairs(&field_pairs)
        .into_created(benilla_protocol::messages::ObjectType::Player);
    Ok(SessionEvent::ObjectCreate {
        guid: id,
        kind: EntityKind::Player,
        display_id: Some(display),
        position: character.position.position,
        orientation: character.position.orientation,
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
        fields,
    })
}
fn validate_peer_character(
    peer: PeerId,
    character: &Character,
    own: &Character,
    peers: &BTreeMap<PeerId, Option<Character>>,
    catalog: &AvatarCatalog,
) -> Result<()> {
    character.validate()?;
    if character.owner_id != peer {
        bail!("peer character owner does not match authenticated identity");
    }
    validate_character_guid(
        character_guid(character),
        character_guid(own),
        peers.values().flatten().map(character_guid),
    )?;
    catalog.body(character)?;
    Ok(())
}

fn validate_character_guid(
    candidate: u64,
    own: u64,
    mut occupied: impl Iterator<Item = u64>,
) -> Result<()> {
    if candidate == own || occupied.any(|other| other == candidate) {
        bail!("duplicate peer character identity");
    }
    Ok(())
}

/// Test entry retains byte-level codec coverage; live dispatch decodes each packet once.
#[cfg(test)]
fn apply_peer_frame(
    events: &Sender<SessionEvent>,
    peer_id: PeerId,
    payload: &[u8],
    peers: &mut BTreeMap<PeerId, Option<Character>>,
    character: &Character,
    catalog: &AvatarCatalog,
) -> Result<()> {
    if !peers.contains_key(&peer_id) {
        return Ok(());
    }
    apply_decoded_peer_frame(
        events,
        peer_id,
        Frame::decode(payload)?,
        peers,
        character,
        catalog,
    )
}

fn apply_decoded_peer_frame(
    events: &Sender<SessionEvent>,
    peer_id: PeerId,
    frame: Frame,
    peers: &mut BTreeMap<PeerId, Option<Character>>,
    character: &Character,
    catalog: &AvatarCatalog,
) -> Result<()> {
    let Some(joined) = peers.get(&peer_id) else {
        return Ok(());
    };
    // A valid remote continent is a visibility boundary, not an authentication failure.
    // Keep its authenticated descriptor and transport; this protocol has no map-transfer frame.
    if joined
        .as_ref()
        .is_some_and(|other| other.position.map_id != character.position.map_id)
        && !matches!(&frame, Frame::Avatar { .. })
    {
        return Ok(());
    }
    match frame {
        Frame::Avatar {
            character: other, ..
        } if joined.is_none() => {
            validate_peer_character(peer_id, &other, character, peers, catalog)?;
            if other.position.map_id == character.position.map_id {
                emit(events, character_name(&other))?;
                emit(events, character_avatar(&other, catalog)?)?;
            }
            peers.insert(peer_id, Some(other));
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
        } if joined.is_some() => {
            emit(
                events,
                SessionEvent::UnitMove {
                    guid: character_guid(joined.as_ref().expect("admitted character")),
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
        Frame::Say { language, text } if joined.is_some() => {
            let sender = joined.as_ref().expect("admitted character");
            emit(
                events,
                speech::message(character_guid(sender), sender.race, language, text)?,
            )?;
        }
        Frame::TextEmote { text_id, target } if joined.is_some() => {
            emotes::emit_text_emote(
                events,
                character_guid(joined.as_ref().expect("admitted character")),
                text_id,
                target,
                character,
                peers,
                catalog,
            )?;
        }
        Frame::EmoteClock {
            text_id,
            generation,
            elapsed_ms,
        } if joined.is_some() => {
            emotes::emit_clock(
                events,
                character_guid(joined.as_ref().expect("admitted character")),
                text_id,
                generation,
                elapsed_ms,
                catalog,
            )?;
        }
        Frame::Presentation {
            stand_state,
            state_emote,
        } if joined.is_some() => {
            emotes::emit_presentation(
                events,
                joined.as_ref().expect("admitted character"),
                stand_state,
                state_emote,
                catalog,
            )?;
        }
        // A broadcast already queued at the connection edge may beat its avatar.
        Frame::Move { .. }
        | Frame::Say { .. }
        | Frame::TextEmote { .. }
        | Frame::EmoteClock { .. }
        | Frame::Presentation { .. }
            if joined.is_none() => {}
        _ => bail!("peer sent an invalid session transition"),
    }
    Ok(())
}
fn reject_peer(
    network: &Network,
    events: &Sender<SessionEvent>,
    peers: &mut BTreeMap<PeerId, Option<Character>>,
    peer: PeerId,
    own_map: u32,
) -> Result<()> {
    // Remove first: already queued frames cannot mutate any local/remaining entity.
    if let Some(Some(character)) = peers.remove(&peer).filter(|entry| {
        entry
            .as_ref()
            .is_some_and(|character| character.position.map_id == own_map)
    }) {
        emit(
            events,
            SessionEvent::ObjectDestroyed(character_guid(&character)),
        )?;
    }
    network
        .try_send(Command::Disconnect { peer_id: peer })
        .map_err(|_| anyhow::anyhow!("peer send queue unavailable"))?;
    Ok(())
}

/// Explicit gameplay entry is separate from exploration: a failure must return to its caller,
/// never start exploration, mutate the profile position, or silently create another world.
/// The native controller constructor is closed until compiled mechanics are supported.
fn run_gameplay_character(
    events: &Sender<SessionEvent>,
    commands: &Receiver<ClientCommand>,
    controls: &Receiver<world::Control>,
    store: &ProfileStore,
    character: &Character,
    catalog: &AvatarCatalog,
    controller: &mut dyn world::Driven,
    stop: &AtomicBool,
) -> Result<world::Exit> {
    let _selection = world::Selection::from_profile(store, character)?;
    if controller.selected_player()
        != (benilla_world_runtime::PlayerId {
            owner: store.owner_id(),
            character: character.id,
        })
    {
        bail!("selected world character does not match the open profile");
    }
    let mut display = character.clone();
    display.position.map_id = controller.map_id();
    display.position.position = controller.position_xyz();
    display.position.orientation = controller.facing();
    // Validate local appearance before the first Connected event; use the saved identity,
    // then let authoritative projection provide progression, inventory and quest fields.
    let mut avatar = character_avatar(&display, catalog)?;
    if let SessionEvent::ObjectCreate { fields, .. } = &mut avatar {
        fields.merge(controller.initial_player_fields()?);
    }
    emit(
        events,
        SessionEvent::Connected {
            self_guid: character_guid(character),
            name: character.name.clone(),
            billing_time_rested: 0,
            tutorial_flags: None,
            addon_info: Some(Vec::new()),
        },
    )?;
    emit(
        events,
        SessionEvent::Worldport {
            map_id: display.position.map_id,
            position: display.position.position,
            orientation: display.position.orientation,
            needs_ack: false,
        },
    )?;
    emit(events, character_name(character))?;
    emit(events, avatar)?;
    Ok(world::run(controller, events, commands, controls, stop)?)
}

fn run_character(
    settings: P2pSettings,
    requested_world: Option<world::Request>,
    events: &Sender<SessionEvent>,
    commands: &Receiver<ClientCommand>,
    ticket: &P2pEndpointTicket,
    store: &mut ProfileStore,
    character: &mut Character,
    catalog: &AvatarCatalog,
    stop: &AtomicBool,
    local_zone: &AtomicU32,
) -> Result<()> {
    world::dispatch(
        requested_world,
        (
            settings, events, commands, ticket, store, character, catalog, stop, local_zone,
        ),
        |(settings, events, commands, ticket, store, character, catalog, stop, _), request| {
            // Solo worlds do not join exploration transport or publish profile state to peers.
            ticket.clear();
            let root =
                crate::local_state::home().context("No local world directory is available")?;
            let (mut controller, controls) = request.enter(&root, store, character, settings.1)?;
            let exit = run_gameplay_character(
                events,
                commands,
                &controls,
                store,
                character,
                catalog,
                controller.as_mut(),
                stop,
            )?;
            bevy::log::info!("p2p: solo world saved at tick {}, generation {} ({:?}); {} pending intentions cancelled",
                exit.saved.tick, exit.saved.generation, exit.saved.durability, exit.cancelled_inputs);
            Ok(())
        },
        |(settings, events, commands, ticket, store, character, catalog, stop, zone)| {
            run_exploration_character(
                settings, events, commands, ticket, store, character, catalog, stop, zone,
            )
        },
    )
}

fn run_exploration_character(
    settings: P2pSettings,
    events: &Sender<SessionEvent>,
    commands: &Receiver<ClientCommand>,
    ticket: &P2pEndpointTicket,
    store: &mut ProfileStore,
    character: &mut Character,
    catalog: &AvatarCatalog,
    stop: &AtomicBool,
    local_zone: &AtomicU32,
) -> Result<()> {
    character.validate()?;
    let signer = store.identity();
    let identity = OwnerIdentity::new(signer.public_key(), move |challenge| {
        signer.sign_transport_challenge(challenge)
    })?;
    let id = identity.owner_id();
    if character.owner_id != id {
        bail!("selected character does not belong to the local profile");
    }
    local_zone.store(character.position.zone_id, Ordering::SeqCst);
    let hello = Hello {
        protocol_version: benilla_p2p::PROTOCOL_VERSION,
        network: settings.0.network.clone(),
        peer_id: id,
        ruleset_hash: settings.0.timezone.exploration_fingerprint(settings.1),
        content_version: CONTENT_VERSION.into(),
    };
    let transport = TransportConfig {
        listen: settings.0.listen,
        peers: settings.0.peers.clone(),
        handshake_timeout: Duration::from_millis(settings.0.timeout_ms),
        max_peers: usize::from(settings.0.max_peers),
    };
    let network = match settings.0.internet.clone() {
        Some(internet) => Network::start_internet_owned(
            transport,
            internet,
            settings.0.psk_bytes(),
            hello,
            identity,
        )?,
        None => Network::start_owned(transport, settings.0.psk_bytes(), hello, identity)?,
    };
    let mut setup = world_setup::Service::open(store, ticket.world_setup());
    ticket.set_listen(network.local_addr());
    bevy::log::info!(
        "p2p: exploration session listening on {} as {}",
        network.local_addr(),
        character.name
    );
    let mut peers = BTreeMap::<PeerId, Option<Character>>::new();
    let mut whispers = whispers::State::default();
    let mut denied = std::collections::BTreeSet::new();
    // Transient presentation is bounded to this avatar and never enters saved progression.
    let mut stand_state = 0;
    let mut state_emote = None;
    let mut emote_generation = 0u64;
    let mut emote_started = Instant::now();
    let mut last_emote_clock = Instant::now();
    let mut last_checkpoint = Instant::now();
    let mut last_rejection_warning: Option<Instant> = None;
    let started = Instant::now();
    emit(
        events,
        SessionEvent::Connected {
            self_guid: character_guid(character),
            name: character.name.clone(),
            billing_time_rested: 0,
            tutorial_flags: None,
            addon_info: Some(Vec::new()),
        },
    )?;
    emit(events, creation_start::worldport(character))?;
    emit(events, character_name(character))?;
    emit(events, character_avatar(character, catalog)?)?;
    emit(events, speech::spell_book(character.race))?;
    // Fork deviation: exploration follows its configured IANA zone (UTC by default),
    // rather than restarting at noon. Realm and synthetic encounter clocks are unchanged.
    let mut clock_sample = settings
        .0
        .timezone
        .now()
        .context("system date outside world-clock range")?;
    publish_exploration_clock(events, clock_sample)?;
    emit(events, chat(0, HELP.into()))?;
    ticket.set(network.endpoint_ticket());
    let mut ticket_updated = Instant::now();
    loop {
        character.position.zone_id = local_zone.load(Ordering::SeqCst);
        if stop.load(Ordering::SeqCst) {
            // The app has stopped producing input; consume already-queued local movement
            // before the shutdown checkpoint without waiting on network work.
            for command in commands.try_iter().take(256) {
                if let ClientCommand::Move {
                    pos,
                    orientation,
                    transport: None,
                    ..
                } = command
                {
                    validate_pose(&pos, orientation)?;
                    character.position.position = pos;
                    character.position.orientation = orientation;
                }
            }
            return Ok(());
        }
        // Resample wall time so minute boundaries, DST and OS clock corrections are reflected.
        // This changes only the session clock; manual lighting overrides retain precedence.
        let current = settings
            .0
            .timezone
            .now()
            .context("system date outside world-clock range")?;
        if current != clock_sample {
            publish_exploration_clock(events, current)?;
            clock_sample = current;
        }
        setup.poll(store);
        whispers.expire(events)?;
        if last_checkpoint.elapsed() >= CHECKPOINT_INTERVAL {
            store.save_position(character.id, character.position).context("could not save local position; session stopped, check local profile before rejoining")?;
            last_checkpoint = Instant::now();
        }
        if ticket_updated.elapsed() >= Duration::from_secs(1) {
            let current = network.endpoint_ticket();
            if ticket.get() != current {
                ticket.set(current);
            }
            ticket_updated = Instant::now();
        }
        for _ in 0..256 {
            let event = match network.try_recv() {
                Ok(event) => event,
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => bail!("peer transport stopped"),
            };
            match event {
                Event::PeerJoined(hello) => {
                    let peer = hello.peer_id;
                    if peer == id || peers.contains_key(&peer) || denied.contains(&peer) {
                        reject_peer(
                            &network,
                            events,
                            &mut peers,
                            peer,
                            character.position.map_id,
                        )?;
                        continue;
                    }
                    peers.insert(peer, None);
                    network
                        .try_send(Command::Send {
                            peer_id: peer,
                            payload: Frame::Avatar {
                                version: AVATAR_VERSION,
                                character: character.clone(),
                            }
                            .encode()?,
                        })
                        .map_err(|_| anyhow::anyhow!("peer send queue unavailable"))?;
                    network
                        .try_send(Command::Send {
                            peer_id: peer,
                            payload: Frame::Presentation {
                                stand_state,
                                state_emote,
                            }
                            .encode()?,
                        })
                        .map_err(|_| anyhow::anyhow!("peer send queue unavailable"))?;
                    if let Some(text_id) = state_emote {
                        network
                            .try_send(Command::Send {
                                peer_id: peer,
                                payload: Frame::EmoteClock {
                                    text_id,
                                    generation: emote_generation,
                                    elapsed_ms: emote_started
                                        .elapsed()
                                        .as_millis()
                                        .min(u128::from(emotes::MAX_CLOCK_MS))
                                        as u32,
                                }
                                .encode()?,
                            })
                            .map_err(|_| anyhow::anyhow!("peer send queue unavailable"))?;
                    }
                }
                Event::PeerLeft(peer) => {
                    whispers.disconnected(events, peer)?;
                    if let Some(Some(other)) = peers.remove(&peer).filter(|entry| {
                        entry
                            .as_ref()
                            .is_some_and(|other| other.position.map_id == character.position.map_id)
                    }) {
                        emit(
                            events,
                            SessionEvent::ObjectDestroyed(character_guid(&other)),
                        )?;
                    }
                }
                Event::Frame { peer_id, payload } => {
                    let applied = match Frame::decode(&payload) {
                        Ok(Frame::Whisper { message }) => whispers.receive(
                            events,
                            character,
                            &peers,
                            peer_id,
                            message,
                            |peer_id, message| send_whisper(&network, peer_id, message),
                        ),
                        Ok(frame) => apply_decoded_peer_frame(
                            events, peer_id, frame, &mut peers, character, catalog,
                        ),
                        Err(error) => Err(error),
                    };
                    if applied.is_err() {
                        whispers.disconnected(events, peer_id)?;
                        // Bound quarantine memory; unknown owners beyond this cap are still disconnected.
                        if denied.len() < 256 {
                            denied.insert(peer_id);
                        }
                        reject_peer(
                            &network,
                            events,
                            &mut peers,
                            peer_id,
                            character.position.map_id,
                        )?;
                    }
                }
                Event::Rejected { address, reason } => {
                    // Unauthenticated traffic must not turn a reachable listener into a log flood.
                    if last_rejection_warning
                        .is_none_or(|last| last.elapsed() >= Duration::from_secs(10))
                    {
                        bevy::log::warn!("p2p: peer at {address} was rejected: {reason}; further warnings are rate-limited");
                        last_rejection_warning = Some(Instant::now());
                    }
                }
            }
        }
        if let Some(text_id) =
            state_emote.filter(|_| last_emote_clock.elapsed() >= Duration::from_secs(1))
        {
            // Renew a very long-lived loop before its bounded wire clock saturates.
            if emote_started.elapsed() >= Duration::from_millis(u64::from(emotes::MAX_CLOCK_MS)) {
                emote_generation = emote_generation.saturating_add(1);
                emote_started = Instant::now();
            }
            let elapsed_ms = emote_started
                .elapsed()
                .as_millis()
                .min(u128::from(emotes::MAX_CLOCK_MS)) as u32;
            emotes::emit_clock(
                events,
                character_guid(character),
                text_id,
                emote_generation,
                elapsed_ms,
                catalog,
            )?;
            network
                .try_send(Command::Broadcast(
                    Frame::EmoteClock {
                        text_id,
                        generation: emote_generation,
                        elapsed_ms,
                    }
                    .encode()?,
                ))
                .map_err(|_| anyhow::anyhow!("peer send queue unavailable"))?;
            last_emote_clock = Instant::now();
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
                        validate_pose(&pos, facing)?;
                        if emote_motion::cancel_loop(&mut state_emote, flags) {
                            // Clear both the local descriptor and peers before forwarding motion;
                            // future admission snapshots read the same cleared state.
                            emotes::emit_presentation(
                                events,
                                character,
                                stand_state,
                                state_emote,
                                catalog,
                            )?;
                            network
                                .try_send(Command::Broadcast(
                                    Frame::Presentation {
                                        stand_state,
                                        state_emote,
                                    }
                                    .encode()?,
                                ))
                                .map_err(|_| anyhow::anyhow!("peer send queue unavailable"))?;
                        }
                        character.position.position = pos;
                        character.position.orientation = facing;
                        Some(Frame::Move {
                            position: pos,
                            orientation: facing,
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
                        language,
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
                        let Ok(language) = speech::language(character.race, language) else {
                            emit(
                                events,
                                chat(0, "Your character does not know that language.".into()),
                            )?;
                            continue;
                        };
                        emit(
                            events,
                            speech::message(
                                character_guid(character),
                                character.race,
                                language,
                                text.clone(),
                            )?,
                        )?;
                        Some(Frame::Say { language, text })
                    }
                    ClientCommand::TextEmote { text_id, target } => {
                        if !catalog.1.contains_text(text_id) {
                            emit(
                                events,
                                chat(0, "This emote is unavailable in your game data.".into()),
                            )?;
                            continue;
                        }
                        if emotes::state_emote_id(&catalog.1, text_id).is_some() {
                            state_emote = Some(text_id);
                            emote_generation = emote_generation.saturating_add(1);
                            emote_started = Instant::now();
                            // Queue the clock before the text/state edge on both paths. It has no voice.
                            emotes::emit_clock(
                                events,
                                character_guid(character),
                                text_id,
                                emote_generation,
                                0,
                                catalog,
                            )?;
                            network
                                .try_send(Command::Broadcast(
                                    Frame::EmoteClock {
                                        text_id,
                                        generation: emote_generation,
                                        elapsed_ms: 0,
                                    }
                                    .encode()?,
                                ))
                                .map_err(|_| anyhow::anyhow!("peer send queue unavailable"))?;
                            last_emote_clock = Instant::now();
                        }
                        emotes::emit_text_emote(
                            events,
                            character_guid(character),
                            text_id,
                            target,
                            character,
                            &peers,
                            catalog,
                        )?;
                        Some(Frame::TextEmote { text_id, target })
                    }
                    ClientCommand::StandStateChange { state } => {
                        if !emotes::valid_stand_state(state) {
                            continue;
                        }
                        stand_state = state as u8;
                        emotes::emit_presentation(
                            events,
                            character,
                            stand_state,
                            state_emote,
                            catalog,
                        )?;
                        Some(Frame::Presentation {
                            stand_state,
                            state_emote,
                        })
                    }
                    ClientCommand::Chat {
                        kind: ChatKind::Whisper,
                        target,
                        text,
                        ..
                    } => {
                        whispers.send(
                            events,
                            character,
                            &peers,
                            target.as_deref(),
                            text,
                            |peer_id, message| send_whisper(&network, peer_id, message),
                        )?;
                        None
                    }
                    ClientCommand::Chat { .. } => {
                        emit(
                            events,
                            chat(
                                0,
                                "P2P exploration supports /say and private /w, /whisper and /r. Party and raid chat are unavailable.".into(),
                            ),
                        )?;
                        None
                    }
                    ClientCommand::NameQuery { guid: query } => {
                        if query == character_guid(character) {
                            emit(events, character_name(character))?;
                        } else if let Some(other) = peers
                            .values()
                            .flatten()
                            .find(|other| character_guid(other) == query)
                        {
                            emit(events, character_name(other))?;
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
mod ecs_tests;
#[cfg(test)]
mod tests;

/// Keep the stock clock/lighting consumers on the same automatic world time.
fn publish_exploration_clock(
    events: &Sender<SessionEvent>,
    sample: benilla_config::world_clock::WorldClockSample,
) -> Result<()> {
    emit(
        events,
        SessionEvent::TimeSpeed {
            hours: sample.hours,
            minutes: sample.minutes,
            day_serial: sample.day_serial,
            timescale: 1.0 / 60.0,
        },
    )
}

fn send_whisper(network: &Network, peer_id: PeerId, message: whispers::Packet) -> Result<()> {
    network
        .try_send(Command::Send {
            peer_id,
            payload: Frame::Whisper { message }.encode()?,
        })
        .map_err(|_| anyhow::anyhow!("private chat send queue unavailable"))
}
