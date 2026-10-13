//! Fork-only peer transport: direct TCP, or explicitly configured iroh QUIC connectivity.
//! Every peer can listen and dial; no central game authority is involved.
//!
//! A shared key grants group membership, not unique player identity. Members can impersonate
//! each other. Internet mode uses private endpoint tickets, optional configured relays and
//! opt-in router mapping. No secret-derived discovery identifier or public service is enabled.
//! TCP remains the default. See `InternetConfig` for the separate internet-mode contract.
//!
//! Framing: four-byte big-endian length followed by a Noise message. The two-message
//! handshake uses an application prologue. The first encrypted transport message is a
//! bounded JSON Hello; subsequent messages begin with 0 (heartbeat) or 1 (application).
//! Snow's ordered transport nonces reject replay/reordering; any framing, decryption,
//! timeout, or protocol error closes the connection. Never resume a failed cipher state.
//! Application-event overflow shuts down the network; callers must handle Disconnected.
//!
//! Cryptographic construction: <https://noiseprotocol.org/noise.html>.
//! Implementation API: <https://docs.rs/snow/0.10.0/snow/struct.Builder.html>.
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::{collections::HashMap, fmt, net::SocketAddr, sync::mpsc, thread, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{mpsc as async_mpsc, Mutex},
    time::{self, timeout},
};
use zeroize::Zeroizing;
mod internet;
pub use internet::InternetConfig;
trait PeerStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send> PeerStream for T {}
type Stream = Box<dyn PeerStream>;

// Version 2 is the passphrase-derived-key boundary; no raw-key compatibility fallback.
pub const PROTOCOL_VERSION: u16 = 2;
pub const MAX_PAYLOAD_SIZE: usize = 16 * 1024;
const MAX_WIRE: usize = MAX_PAYLOAD_SIZE + 64;
const QUEUE: usize = 128;
const NOISE: &str = "Noise_NNpsk0_25519_ChaChaPoly_SHA256";
const PROLOGUE: &[u8] = b"benilla-p2p-v2";
pub type PeerId = [u8; 16];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Hello {
    pub protocol_version: u16,
    pub network: String,
    pub peer_id: PeerId,
    pub ruleset_hash: [u8; 32],
    pub content_version: String,
}
impl Hello {
    fn validate(&self) -> Result<(), Error> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(Error::Protocol("unsupported protocol version"));
        }
        if self.network.is_empty()
            || self.network.len() > 128
            || self.content_version.is_empty()
            || self.content_version.len() > 128
            || self.peer_id == [0; 16]
        {
            return Err(Error::Protocol("invalid hello"));
        }
        Ok(())
    }
    fn compatible(&self, remote: &Self) -> Result<(), Error> {
        remote.validate()?;
        if self.network != remote.network
            || self.ruleset_hash != remote.ruleset_hash
            || self.content_version != remote.content_version
        {
            return Err(Error::Protocol("incompatible network, ruleset, or content"));
        }
        if self.peer_id == remote.peer_id {
            return Err(Error::Protocol("duplicate local peer identity"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct TransportConfig {
    pub listen: SocketAddr,
    pub peers: Vec<SocketAddr>,
    pub handshake_timeout: Duration,
    pub max_peers: usize,
}
impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            listen: "0.0.0.0:3725".parse().unwrap(),
            peers: vec![],
            handshake_timeout: Duration::from_secs(5),
            max_peers: 16,
        }
    }
}
#[derive(Clone, Debug)]
pub enum Event {
    PeerJoined(Hello),
    PeerLeft(PeerId),
    Frame { peer_id: PeerId, payload: Vec<u8> },
    Rejected { address: String, reason: String },
}
#[derive(Clone, Debug)]
pub enum Command {
    Broadcast(Vec<u8>),
    Send { peer_id: PeerId, payload: Vec<u8> },
    Shutdown,
}
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Crypto,
    Protocol(&'static str),
    Timeout,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "peer I/O: {e}"),
            Self::Crypto => f.write_str("peer authentication failed"),
            Self::Protocol(s) => f.write_str(s),
            Self::Timeout => f.write_str("peer operation timed out"),
        }
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<snow::Error> for Error {
    fn from(_: snow::Error) -> Self {
        Self::Crypto
    }
}
pub fn generate_peer_id() -> Result<PeerId, Error> {
    let mut id = [0; 16];
    getrandom::fill(&mut id).map_err(|_| Error::Protocol("secure random source unavailable"))?;
    Ok(id)
}

/// Owns one bounded background runtime. Dropping the handle closes all sockets.
pub struct Network {
    address: SocketAddr,
    endpoint: Option<iroh::Endpoint>,
    commands: async_mpsc::Sender<Command>,
    events: mpsc::Receiver<Event>,
    diagnostics: mpsc::Receiver<Event>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Network {
    pub fn start(config: TransportConfig, psk: &[u8; 32], hello: Hello) -> Result<Self, Error> {
        hello.validate()?;
        if config.max_peers == 0
            || config.max_peers > 64
            || config.peers.len() > 64
            || config.handshake_timeout.is_zero()
            || config.handshake_timeout > Duration::from_secs(60)
        {
            return Err(Error::Protocol("invalid transport limits"));
        }
        if psk.iter().all(|byte| *byte == psk[0]) {
            return Err(Error::Protocol("PSK must not be a constant-byte key"));
        }
        let listener = std::net::TcpListener::bind(config.listen)?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let key = Zeroizing::new(*psk);
        let (commands, rx) = async_mpsc::channel(QUEUE);
        let (tx, events) = mpsc::sync_channel(QUEUE);
        // Unauthenticated failures cannot consume authenticated lifecycle/frame capacity.
        let (diagnostic_tx, diagnostics) = mpsc::sync_channel(16);
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let worker = thread::Builder::new().name("benilla-p2p".into()).spawn(move || {
            runtime.block_on(async move {
                if let Ok(listener) = TcpListener::from_std(listener) {
                    tokio::select! { _ = run(internet::Backend::Tcp(listener),config,key,hello,rx,tx,diagnostic_tx) => {}, _ = stopped => {} }
                }
            });
        })?;
        Ok(Self {
            address,
            endpoint: None,
            commands,
            events,
            diagnostics,
            stop: Some(stop),
            worker: Some(worker),
        })
    }
    pub fn local_addr(&self) -> SocketAddr {
        self.address
    }
    pub fn try_recv(&self) -> Result<Event, mpsc::TryRecvError> {
        match self.events.try_recv() {
            Err(mpsc::TryRecvError::Empty) => self.diagnostics.try_recv(),
            result => result,
        }
    }
    pub fn try_send(&self, command: Command) -> Result<(), mpsc::TrySendError<Command>> {
        let size = match &command {
            Command::Broadcast(p) | Command::Send { payload: p, .. } => p.len(),
            _ => 0,
        };
        if size > MAX_PAYLOAD_SIZE {
            return Err(mpsc::TrySendError::Full(command));
        }
        self.commands.try_send(command).map_err(|e| match e {
            async_mpsc::error::TrySendError::Full(c) => mpsc::TrySendError::Full(c),
            async_mpsc::error::TrySendError::Closed(c) => mpsc::TrySendError::Disconnected(c),
        })
    }
}
impl Drop for Network {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

async fn write_wire<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut W,
    bytes: &[u8],
) -> Result<(), Error> {
    if bytes.is_empty() || bytes.len() > MAX_WIRE {
        return Err(Error::Protocol("invalid frame length"));
    }
    writer.write_u32(bytes.len() as u32).await?;
    writer.write_all(bytes).await?;
    Ok(())
}
async fn read_wire<R: tokio::io::AsyncRead + Unpin>(reader: &mut R) -> Result<Vec<u8>, Error> {
    let size = reader.read_u32().await? as usize;
    if size == 0 || size > MAX_WIRE {
        return Err(Error::Protocol("invalid frame length"));
    }
    let mut bytes = vec![0; size];
    reader.read_exact(&mut bytes).await?;
    Ok(bytes)
}
async fn handshake<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    stream: &mut S,
    key: &[u8; 32],
    local: &Hello,
    initiator: bool,
) -> Result<(snow::TransportState, Hello), Error> {
    let builder = snow::Builder::new(NOISE.parse().map_err(|_| Error::Crypto)?)
        .psk(0, key)?
        .prologue(PROLOGUE)?;
    let mut noise = if initiator {
        builder.build_initiator()?
    } else {
        builder.build_responder()?
    };
    let mut output = vec![0; MAX_WIRE];
    if initiator {
        let size = noise.write_message(&[], &mut output)?;
        write_wire(stream, &output[..size]).await?;
        noise.read_message(&read_wire(stream).await?, &mut output)?;
    } else {
        noise.read_message(&read_wire(stream).await?, &mut output)?;
        let size = noise.write_message(&[], &mut output)?;
        write_wire(stream, &output[..size]).await?;
    }
    let mut transport = noise.into_transport_mode()?;
    let hello = serde_json::to_vec(local).map_err(|_| Error::Protocol("invalid local hello"))?;
    let size = transport.write_message(&hello, &mut output)?;
    write_wire(stream, &output[..size]).await?;
    let size = transport.read_message(&read_wire(stream).await?, &mut output)?;
    let remote: Hello = serde_json::from_slice(&output[..size])
        .map_err(|_| Error::Protocol("invalid remote hello"))?;
    local.compatible(&remote)?;
    Ok((transport, remote))
}

struct LivePeer {
    token: u64,
    preferred: bool,
    send: async_mpsc::Sender<Vec<u8>>,
    task: tokio::task::JoinHandle<()>,
}
enum Internal {
    Ready {
        token: u64,
        address: String,
        outbound: bool,
        stream: Stream,
        noise: Box<snow::TransportState>,
        hello: Hello,
        permit: tokio::sync::OwnedSemaphorePermit,
    },
    Failed {
        address: String,
        outbound: bool,
        reason: String,
    },
    Closed {
        token: u64,
        id: PeerId,
        address: String,
        outbound: bool,
    },
    Frame {
        token: u64,
        id: PeerId,
        payload: Vec<u8>,
    },
}
async fn run(
    backend: internet::Backend,
    config: TransportConfig,
    key: Zeroizing<[u8; 32]>,
    hello: Hello,
    mut commands: async_mpsc::Receiver<Command>,
    events: mpsc::SyncSender<Event>,
    diagnostics: mpsc::SyncSender<Event>,
) {
    let (updates, mut incoming) = async_mpsc::channel(QUEUE);
    let permits = Arc::new(tokio::sync::Semaphore::new(config.max_peers * 2));
    let mut peers: HashMap<PeerId, LivePeer> = HashMap::new();
    let mut outbound_addresses = std::collections::HashSet::new();
    let mut known_outbound_ids: HashMap<String, PeerId> = HashMap::new();
    let mut token = 0u64;
    let mut retry = time::interval(Duration::from_secs(2));
    retry.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
    let targets = backend.targets(&config);
    loop {
        tokio::select! {
            accepted = backend.accept() => {
                if let Some((candidate,address)) = accepted {
                    if let Ok(permit) = permits.clone().try_acquire_owned() {
                        token += 1;
                        launch_candidate(token,address,Some(candidate),&backend,key.clone(),hello.clone(),config.handshake_timeout,permit,updates.clone());
                    }
                }
            }
            _ = retry.tick() => {
                for address in &targets {
                    if outbound_addresses.contains(address)
                        || known_outbound_ids.get(address).is_some_and(|id| peers.contains_key(id))
                        || backend.is_self(address) { continue; }
                    if let Ok(permit) = permits.clone().try_acquire_owned() {
                        outbound_addresses.insert(address.clone());
                        token += 1;
                        launch_candidate(token,address.clone(),None,&backend,key.clone(),hello.clone(),config.handshake_timeout,permit,updates.clone());
                    }
                }
            }
            command = commands.recv() => match command {
                Some(Command::Shutdown) | None => break,
                Some(Command::Broadcast(payload)) => {
                    for peer in peers.values() { if peer.send.try_send(payload.clone()).is_err() { peer.task.abort(); } }
                }
                Some(Command::Send { peer_id,payload }) => {
                    if let Some(peer) = peers.get(&peer_id) { if peer.send.try_send(payload).is_err() { peer.task.abort(); } }
                }
            },
            update = incoming.recv() => match update {
                Some(Internal::Failed { address,outbound,reason }) => {
                    if outbound { outbound_addresses.remove(&address); }
                    // These unauthenticated diagnostics are deliberately lossy and isolated.
                    let _ = diagnostics.try_send(Event::Rejected { address: internet::diagnostic_address(&address),reason });
                }
                Some(Internal::Ready { token,address,outbound,stream,noise,hello: remote,permit }) => {
                    let id = remote.peer_id;
                    if outbound { known_outbound_ids.insert(address.clone(),id); }
                    // Prefer the connection dialed by the lexicographically smaller ephemeral ID.
                    let preferred = outbound == (hello.peer_id < id);
                    if peers.get(&id).is_some_and(|old| old.preferred || !preferred) || (!peers.contains_key(&id) && peers.len() >= config.max_peers) {
                        if outbound { outbound_addresses.remove(&address); }
                        continue;
                    }
                    let replacing = peers.remove(&id);
                    if let Some(old) = &replacing { old.task.abort(); }
                    let (send,receive) = async_mpsc::channel(QUEUE);
                    let updates = updates.clone();
                    let guard = CloseGuard { updates: updates.clone(), token,id,address,outbound };
                    let task = tokio::spawn(async move {
                        let _permit = permit;
                        let _guard = guard;
                        let _ = session(stream,*noise,id,token,receive,updates).await;
                    });
                    peers.insert(id,LivePeer { token,preferred,send,task });
                    // A replacement is a fresh stream: force the application to resend its
                    // initial state rather than silently losing a queued introduction.
                    if replacing.is_some() && events.try_send(Event::PeerLeft(id)).is_err() { break; }
                    if events.try_send(Event::PeerJoined(remote)).is_err() { break; }
                }
                Some(Internal::Closed { token,id,address,outbound }) => {
                    if outbound { outbound_addresses.remove(&address); }
                    if peers.get(&id).is_some_and(|peer| peer.token == token) {
                        peers.remove(&id);
                        if events.try_send(Event::PeerLeft(id)).is_err() { break; }
                    }
                }
                Some(Internal::Frame { token,id,payload }) => {
                    if peers.get(&id).is_some_and(|peer| peer.token == token) && events.try_send(Event::Frame { peer_id:id,payload }).is_err() { break; }
                }
                None => break,
            }
        }
    }
    for peer in peers.values() {
        peer.task.abort();
    }
}

// Cancellation reports closure with backpressure without blocking the runtime's thread.
struct CloseGuard {
    updates: async_mpsc::Sender<Internal>,
    token: u64,
    id: PeerId,
    address: String,
    outbound: bool,
}
impl Drop for CloseGuard {
    fn drop(&mut self) {
        let message = Internal::Closed {
            token: self.token,
            id: self.id,
            address: self.address.clone(),
            outbound: self.outbound,
        };
        let updates = self.updates.clone();
        tokio::spawn(async move {
            let _ = updates.send(message).await;
        });
    }
}
#[allow(clippy::too_many_arguments)]
fn launch_candidate(
    token: u64,
    address: String,
    candidate: Option<internet::Candidate>,
    backend: &internet::Backend,
    key: Zeroizing<[u8; 32]>,
    hello: Hello,
    limit: Duration,
    permit: tokio::sync::OwnedSemaphorePermit,
    updates: async_mpsc::Sender<Internal>,
) {
    let endpoint = backend.endpoint();
    tokio::spawn(async move {
        let outbound = candidate.is_none();
        let result = timeout(limit, async {
            let mut stream = internet::connect(candidate, endpoint, &address).await?;
            let (noise, remote) = handshake(&mut stream, &key, &hello, outbound).await?;
            Ok::<_, Error>((stream, noise, remote))
        })
        .await
        .unwrap_or(Err(Error::Timeout));
        let message = match result {
            Ok((stream, noise, hello)) => Internal::Ready {
                token,
                address,
                outbound,
                stream,
                noise: Box::new(noise),
                hello,
                permit,
            },
            Err(error) => Internal::Failed {
                address,
                outbound,
                reason: error.to_string(),
            },
        };
        let _ = updates.send(message).await;
    });
}
async fn session(
    stream: Stream,
    noise: snow::TransportState,
    id: PeerId,
    token: u64,
    mut outgoing: async_mpsc::Receiver<Vec<u8>>,
    updates: async_mpsc::Sender<Internal>,
) -> Result<(), Error> {
    let (mut reader, mut writer) = tokio::io::split(stream);
    let noise = Mutex::new(noise);
    let read = async {
        loop {
            // The timeout covers the whole frame, including its prefix, to prevent slowloris.
            let wire = timeout(Duration::from_secs(30), read_wire(&mut reader))
                .await
                .map_err(|_| Error::Timeout)??;
            let mut clear = vec![0; MAX_WIRE];
            let size = noise.lock().await.read_message(&wire, &mut clear)?;
            match clear.get(..size) {
                Some([0]) => {}
                Some([1, payload @ ..]) if payload.len() <= MAX_PAYLOAD_SIZE => {
                    updates
                        .send(Internal::Frame {
                            token,
                            id,
                            payload: payload.to_vec(),
                        })
                        .await
                        .map_err(|_| Error::Protocol("event channel closed"))?;
                }
                _ => return Err(Error::Protocol("invalid encrypted frame")),
            }
        }
    };
    let write = async {
        let mut ping = time::interval(Duration::from_secs(5));
        ping.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
        loop {
            let clear = tokio::select! {
                payload = outgoing.recv() => match payload {
                    Some(payload) if payload.len() <= MAX_PAYLOAD_SIZE => { let mut clear = vec![1]; clear.extend(payload); clear },
                    _ => return Err(Error::Protocol("send channel closed or oversized payload")),
                },
                _ = ping.tick() => vec![0],
            };
            let mut wire = vec![0; clear.len() + 16];
            let size = noise.lock().await.write_message(&clear, &mut wire)?;
            timeout(
                Duration::from_secs(5),
                write_wire(&mut writer, &wire[..size]),
            )
            .await
            .map_err(|_| Error::Timeout)??;
        }
    };
    tokio::select! { result = read => result, result = write => result }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, time::Instant};
    fn key() -> [u8; 32] {
        std::array::from_fn(|i| i as u8)
    }
    fn hello(id: u8) -> Hello {
        Hello {
            protocol_version: PROTOCOL_VERSION,
            network: "test-world".into(),
            peer_id: [id; 16],
            ruleset_hash: [3; 32],
            content_version: "1.12.1-5875".into(),
        }
    }
    fn config(peers: Vec<SocketAddr>) -> TransportConfig {
        TransportConfig {
            listen: "127.0.0.1:0".parse().unwrap(),
            peers,
            handshake_timeout: Duration::from_millis(300),
            max_peers: 8,
        }
    }
    fn next(network: &Network) -> Event {
        let start = Instant::now();
        loop {
            match network.try_recv() {
                Ok(event) => return event,
                Err(mpsc::TryRecvError::Empty) if start.elapsed() < Duration::from_secs(4) => {
                    thread::sleep(Duration::from_millis(5))
                }
                other => panic!("expected event, received {other:?}"),
            }
        }
    }
    fn joined(network: &Network) -> PeerId {
        match next(network) {
            Event::PeerJoined(hello) => hello.peer_id,
            other => panic!("expected join, got {other:?}"),
        }
    }
    #[test]
    fn two_peers_authenticate_exchange_and_disconnect() {
        let a = Network::start(config(vec![]), &key(), hello(1)).unwrap();
        let b = Network::start(config(vec![a.local_addr()]), &key(), hello(2)).unwrap();
        assert_eq!(joined(&a), [2; 16]);
        assert_eq!(joined(&b), [1; 16]);
        b.try_send(Command::Broadcast(b"movement".to_vec()))
            .unwrap();
        assert!(
            matches!(next(&a),Event::Frame { peer_id, payload } if peer_id == [2;16] && payload == b"movement")
        );
        a.try_send(Command::Send {
            peer_id: [2; 16],
            payload: vec![7; MAX_PAYLOAD_SIZE],
        })
        .unwrap();
        assert!(
            matches!(next(&b),Event::Frame { payload,.. } if payload.len() == MAX_PAYLOAD_SIZE)
        );
        assert!(a
            .try_send(Command::Broadcast(vec![0; MAX_PAYLOAD_SIZE + 1]))
            .is_err());
        drop(b);
        assert!(matches!(next(&a),Event::PeerLeft(id) if id == [2;16]));
    }
    #[test]
    fn wrong_psk_never_joins() {
        let a = Network::start(config(vec![]), &key(), hello(1)).unwrap();
        let mut wrong = key();
        wrong[0] ^= 1;
        let b = Network::start(config(vec![a.local_addr()]), &wrong, hello(2)).unwrap();
        assert!(matches!(next(&a), Event::Rejected { .. }));
        assert!(matches!(next(&b), Event::Rejected { .. }));
    }
    #[test]
    fn incompatible_rules_network_and_content_rejected() {
        for field in 0..3 {
            let a = Network::start(config(vec![]), &key(), hello(1)).unwrap();
            let mut other = hello(2);
            match field {
                0 => other.ruleset_hash[0] ^= 1,
                1 => other.network.push('x'),
                _ => other.content_version.push('x'),
            }
            let b = Network::start(config(vec![a.local_addr()]), &key(), other).unwrap();
            assert!(matches!(next(&a), Event::Rejected { .. }));
            assert!(matches!(next(&b), Event::Rejected { .. }));
        }
    }
    #[test]
    fn truncated_oversized_and_silent_handshakes_rejected() {
        let a = Network::start(config(vec![]), &key(), hello(1)).unwrap();
        let mut raw = std::net::TcpStream::connect(a.local_addr()).unwrap();
        raw.write_all(&((MAX_WIRE + 1) as u32).to_be_bytes())
            .unwrap();
        assert!(matches!(next(&a), Event::Rejected { .. }));
        let mut raw = std::net::TcpStream::connect(a.local_addr()).unwrap();
        raw.write_all(&[0, 0, 0, 32, 1, 2]).unwrap();
        drop(raw);
        assert!(matches!(next(&a), Event::Rejected { .. }));
        let _silent = std::net::TcpStream::connect(a.local_addr()).unwrap();
        assert!(matches!(next(&a),Event::Rejected { reason,.. } if reason.contains("timed out")));
    }
    fn noise_pair() -> (snow::TransportState, snow::TransportState) {
        let key = key();
        let build = || {
            snow::Builder::new(NOISE.parse().unwrap())
                .psk(0, &key)
                .unwrap()
                .prologue(PROLOGUE)
                .unwrap()
        };
        let mut a = build().build_initiator().unwrap();
        let mut b = build().build_responder().unwrap();
        let mut wire = [0; 128];
        let mut clear = [0; 128];
        let n = a.write_message(&[], &mut wire).unwrap();
        b.read_message(&wire[..n], &mut clear).unwrap();
        let n = b.write_message(&[], &mut wire).unwrap();
        a.read_message(&wire[..n], &mut clear).unwrap();
        (
            a.into_transport_mode().unwrap(),
            b.into_transport_mode().unwrap(),
        )
    }
    #[test]
    fn legacy_protocol_and_noise_prologue_are_rejected() {
        let mut legacy = hello(2);
        legacy.protocol_version = 1;
        assert!(legacy.validate().is_err());
        let key = key();
        let build = |prologue| {
            snow::Builder::new(NOISE.parse().unwrap())
                .psk(0, &key)
                .unwrap()
                .prologue(prologue)
                .unwrap()
        };
        let mut old = build(b"benilla-p2p-v1").build_initiator().unwrap();
        let mut current = build(PROLOGUE).build_responder().unwrap();
        let mut wire = [0; 128];
        let mut clear = [0; 128];
        let n = old.write_message(&[], &mut wire).unwrap();
        assert!(current.read_message(&wire[..n], &mut clear).is_err());
    }
    #[test]
    fn replay_tampering_and_previous_session_ciphertext_rejected() {
        let (mut a, mut b) = noise_pair();
        let mut wire = [0; 128];
        let mut clear = [0; 128];
        let n = a.write_message(b"one", &mut wire).unwrap();
        assert_eq!(b.read_message(&wire[..n], &mut clear).unwrap(), 3);
        assert!(b.read_message(&wire[..n], &mut clear).is_err());
        let (_, mut fresh) = noise_pair();
        assert!(fresh.read_message(&wire[..n], &mut clear).is_err());
        let n = a.write_message(b"two", &mut wire).unwrap();
        wire[0] ^= 1;
        assert!(b.read_message(&wire[..n], &mut clear).is_err());
    }
    #[test]
    fn hello_rejects_versions_identity_and_limits() {
        let local = hello(1);
        let mut remote = hello(2);
        remote.protocol_version += 1;
        assert!(local.compatible(&remote).is_err());
        assert!(local.compatible(&local).is_err());
        remote = hello(2);
        remote.network = "a".repeat(129);
        assert!(remote.validate().is_err());
        assert!(Network::start(config(vec![]), &[0; 32], local).is_err());
    }
    #[test]
    fn three_direct_peers_exchange_without_coordinator() {
        let a = Network::start(config(vec![]), &key(), hello(1)).unwrap();
        let b = Network::start(config(vec![a.local_addr()]), &key(), hello(2)).unwrap();
        joined(&a);
        joined(&b);
        let c = Network::start(
            config(vec![a.local_addr(), b.local_addr()]),
            &key(),
            hello(3),
        )
        .unwrap();
        joined(&a);
        joined(&b);
        joined(&c);
        joined(&c);
        drop(a);
        assert!(matches!(next(&b), Event::PeerLeft(_)));
        assert!(matches!(next(&c), Event::PeerLeft(_)));
        b.try_send(Command::Broadcast(b"still connected".to_vec()))
            .unwrap();
        assert!(matches!(next(&c),Event::Frame { payload,.. } if payload == b"still connected"));
    }
    #[test]
    fn shutdown_and_drop_release_listener() {
        let a = Network::start(config(vec![]), &key(), hello(1)).unwrap();
        let address = a.local_addr();
        drop(a);
        assert!(std::net::TcpListener::bind(address).is_ok());
    }
    #[tokio::test]
    async fn replay_on_live_connection_disconnects_after_first_frame() {
        let a = Network::start(config(vec![]), &key(), hello(1)).unwrap();
        let mut raw = TcpStream::connect(a.local_addr()).await.unwrap();
        let (mut noise, _) = handshake(&mut raw, &key(), &hello(2), true).await.unwrap();
        joined(&a);
        let mut wire = [0; 64];
        let n = noise.write_message(b"\x01movement", &mut wire).unwrap();
        write_wire(&mut raw, &wire[..n]).await.unwrap();
        write_wire(&mut raw, &wire[..n]).await.unwrap();
        assert!(matches!(next(&a),Event::Frame { payload,.. } if payload == b"movement"));
        assert!(matches!(next(&a), Event::PeerLeft(_)));
    }
    #[tokio::test]
    async fn encrypted_unsupported_version_never_joins() {
        let a = Network::start(config(vec![]), &key(), hello(1)).unwrap();
        let mut raw = TcpStream::connect(a.local_addr()).await.unwrap();
        let mut wrong = hello(2);
        wrong.protocol_version += 1;
        let _ = handshake(&mut raw, &key(), &wrong, true).await;
        assert!(matches!(next(&a), Event::Rejected { .. }));
    }
    #[test]
    fn symmetric_dial_converges_to_one_usable_connection() {
        let reserve = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let b_address = reserve.local_addr().unwrap();
        drop(reserve);
        let a = Network::start(config(vec![b_address]), &key(), hello(1)).unwrap();
        let mut b_config = config(vec![a.local_addr()]);
        b_config.listen = b_address;
        let b = Network::start(b_config, &key(), hello(2)).unwrap();
        let until = Instant::now() + Duration::from_millis(2300);
        let mut active_a = false;
        let mut active_b = false;
        while Instant::now() < until {
            for (network, active) in [(&a, &mut active_a), (&b, &mut active_b)] {
                while let Ok(event) = network.try_recv() {
                    match event {
                        Event::PeerJoined(_) => *active = true,
                        Event::PeerLeft(_) => *active = false,
                        _ => {}
                    }
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(active_a && active_b);
        a.try_send(Command::Broadcast(b"one copy".to_vec()))
            .unwrap();
        assert!(matches!(next(&b),Event::Frame { payload,.. } if payload == b"one copy"));
        thread::sleep(Duration::from_millis(50));
        assert!(matches!(b.try_recv(), Err(mpsc::TryRecvError::Empty)));
    }
    #[test]
    fn peer_limit_and_admission_limits_are_enforced() {
        let mut limits = config(vec![]);
        limits.max_peers = 1;
        let a = Network::start(limits, &key(), hello(1)).unwrap();
        let b = Network::start(config(vec![a.local_addr()]), &key(), hello(2)).unwrap();
        joined(&a);
        joined(&b);
        let c = Network::start(config(vec![a.local_addr()]), &key(), hello(3)).unwrap();
        thread::sleep(Duration::from_millis(100));
        assert!(matches!(a.try_recv(), Err(mpsc::TryRecvError::Empty)));
        drop(c);
        let mut limits = config(vec![]);
        limits.max_peers = 0;
        assert!(Network::start(limits, &key(), hello(4)).is_err());
    }
    #[test]
    fn rejection_flood_cannot_consume_authenticated_event_capacity() {
        use std::io::Read;
        let a = Network::start(config(vec![]), &key(), hello(1)).unwrap();
        let b = Network::start(config(vec![a.local_addr()]), &key(), hello(2)).unwrap();
        joined(&a);
        joined(&b);
        // Leave all diagnostics unread while exceeding the main event queue's capacity.
        for _ in 0..QUEUE + 32 {
            let mut raw = std::net::TcpStream::connect(a.local_addr()).unwrap();
            raw.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            raw.write_all(&((MAX_WIRE + 1) as u32).to_be_bytes())
                .unwrap();
            assert_eq!(raw.read(&mut [0; 1]).unwrap(), 0);
        }
        b.try_send(Command::Broadcast(b"survives rejection flood".to_vec()))
            .unwrap();
        loop {
            match next(&a) {
                Event::Rejected { .. } => {}
                Event::Frame { peer_id, payload } => {
                    assert_eq!(peer_id, [2; 16]);
                    assert_eq!(payload, b"survives rejection flood");
                    break;
                }
                event => panic!("legitimate session lost: {event:?}"),
            }
        }
        let c = Network::start(config(vec![a.local_addr()]), &key(), hello(3)).unwrap();
        loop {
            match next(&a) {
                Event::Rejected { .. } => {}
                Event::PeerJoined(hello) => {
                    assert_eq!(hello.peer_id, [3; 16]);
                    break;
                }
                event => panic!("listener stopped accepting: {event:?}"),
            }
        }
        assert_eq!(joined(&c), [1; 16]);
    }
    #[test]
    fn known_peer_reconnects_after_restart() {
        let b = Network::start(config(vec![]), &key(), hello(2)).unwrap();
        let address = b.local_addr();
        let a = Network::start(config(vec![address]), &key(), hello(1)).unwrap();
        joined(&a);
        joined(&b);
        drop(b);
        assert!(matches!(next(&a),Event::PeerLeft(id) if id == [2;16]));
        let mut restarted = config(vec![]);
        restarted.listen = address;
        let b = Network::start(restarted, &key(), hello(3)).unwrap();
        loop {
            match next(&a) {
                Event::Rejected { .. } => {}
                Event::PeerJoined(hello) => {
                    assert_eq!(hello.peer_id, [3; 16]);
                    break;
                }
                event => panic!("unexpected reconnect event: {event:?}"),
            }
        }
        assert_eq!(joined(&b), [1; 16]);
        b.try_send(Command::Broadcast(b"reconnected".to_vec()))
            .unwrap();
        assert!(matches!(next(&a),Event::Frame { payload,.. } if payload == b"reconnected"));
    }
}
