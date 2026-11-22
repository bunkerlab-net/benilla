//! Optional QUIC connectivity. Tickets contain public endpoint addresses, never the PSK.
//! No public discovery, relay, or router mapping is enabled implicitly.
use super::*;
use iroh::{
    endpoint::{Builder, NetReportConfig, PortmapperConfig},
    Endpoint, EndpointAddr, RelayMap, RelayMode, RelayUrl,
};
use std::collections::HashSet;

const ALPN: &[u8] = b"benilla/psk-stream/1";
const MAX_TICKET: usize = 8192;

/// Internet mode uses private tickets or opt-in runtime candidate submissions. A relay is infrastructure,
/// not game authority: QUIC and the existing Noise session encrypt all game traffic.
#[derive(Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct InternetConfig {
    /// Explicit relay allowlist. Empty means direct QUIC only, without hole-punch assistance.
    pub relay_urls: Vec<String>,
    /// JSON endpoint tickets obtained from `Network::endpoint_ticket`.
    pub peer_tickets: Vec<String>,
    /// Explicit permission to request router mappings via UPnP/PCP/NAT-PMP.
    pub port_mapping: bool,
    /// Accept bounded runtime candidate submissions. Disabled by default. This alone starts
    /// no discovery service: an explicitly configured producer must submit endpoint tickets.
    pub dynamic_candidates: bool,
}
impl fmt::Debug for InternetConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InternetConfig")
            .field("relay_count", &self.relay_urls.len())
            .field("peer_ticket_count", &self.peer_tickets.len())
            .field("port_mapping", &self.port_mapping)
            .field("dynamic_candidates", &self.dynamic_candidates)
            .finish()
    }
}

impl InternetConfig {
    fn relays(&self) -> Result<Vec<RelayUrl>, Error> {
        if self.relay_urls.len() > 8 || self.peer_tickets.len() > 64 {
            return Err(Error::Protocol("too many internet endpoints"));
        }
        let mut seen = HashSet::new();
        self.relay_urls.iter().map(|value| {
            if value.len() > 2048 { return Err(Error::Protocol("relay URL too long")); }
            let url: RelayUrl = value.parse().map_err(|_| Error::Protocol("invalid relay URL"))?;
            let raw = &url;
            if raw.scheme() != "https" || raw.host_str().is_none() || !raw.username().is_empty()
                || raw.password().is_some() || raw.query().is_some() || raw.fragment().is_some()
                || !seen.insert(url.clone()) {
                return Err(Error::Protocol("relay URLs must be unique HTTPS URLs without credentials, queries or fragments"));
            }
            Ok(url)
        }).collect()
    }
    fn tickets(&self, relays: &[RelayUrl]) -> Result<Vec<EndpointAddr>, Error> {
        let mut ids = HashSet::new();
        self.peer_tickets
            .iter()
            .map(|ticket| {
                let addr = parse_ticket(ticket)?;
                if !ids.insert(addr.id) || addr.relay_urls().any(|url| !relays.contains(url)) {
                    return Err(Error::Protocol(
                        "duplicate endpoint or ticket relay missing from explicit allowlist",
                    ));
                }
                Ok(addr)
            })
            .collect()
    }
    /// Validates all input before binding sockets or contacting infrastructure.
    pub fn validate(&self) -> Result<(), Error> {
        self.tickets(&self.relays()?).map(|_| ())
    }
}
pub(super) fn parse_ticket(ticket: &str) -> Result<EndpointAddr, Error> {
    if ticket.len() > MAX_TICKET {
        return Err(Error::Protocol("endpoint ticket too long"));
    }
    let addr: EndpointAddr =
        serde_json::from_str(ticket).map_err(|_| Error::Protocol("invalid endpoint ticket"))?;
    if addr.is_empty()
        || addr.addrs.len() > 16
        || addr
            .ip_addrs()
            .any(|a| a.port() == 0 || a.ip().is_multicast() || a.ip().is_unspecified())
        || addr.addrs.iter().any(|a| !a.is_ip() && !a.is_relay())
    {
        return Err(Error::Protocol(
            "endpoint ticket requires bounded unicast or relay addresses",
        ));
    }
    Ok(addr)
}

impl Network {
    /// Legacy group authentication only. TCP peers must be empty, so mode errors fail closed.
    pub fn start_internet(
        config: TransportConfig,
        internet: InternetConfig,
        psk: &[u8; 32],
        hello: Hello,
    ) -> Result<Self, Error> {
        Self::start_internet_mode(config, internet, psk, hello, None)
    }
    /// Internet connectivity with persistent owner proofs; never accepts legacy peers.
    pub fn start_internet_owned(
        config: TransportConfig,
        internet: InternetConfig,
        psk: &[u8; 32],
        hello: Hello,
        identity: OwnerIdentity,
    ) -> Result<Self, Error> {
        identity.validate_hello(&hello)?;
        Self::start_internet_mode(config, internet, psk, hello, Some(identity))
    }
    fn start_internet_mode(
        config: TransportConfig,
        internet: InternetConfig,
        psk: &[u8; 32],
        hello: Hello,
        identity: Option<OwnerIdentity>,
    ) -> Result<Self, Error> {
        hello.validate()?;
        if !config.peers.is_empty()
            || config.max_peers == 0
            || config.max_peers > 64
            || internet.peer_tickets.len() > config.max_peers
            || config.handshake_timeout.is_zero()
            || config.handshake_timeout > Duration::from_secs(60)
        {
            return Err(Error::Protocol(
                "invalid internet transport limits or TCP peers in internet mode",
            ));
        }
        if psk.iter().all(|byte| *byte == psk[0]) {
            return Err(Error::Protocol("PSK must not be a constant-byte key"));
        }
        let relays = internet.relays()?;
        let peers = internet.tickets(&relays)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let relay_mode = if relays.is_empty() {
            RelayMode::Disabled
        } else {
            RelayMode::Custom(relays.clone().into_iter().collect::<RelayMap>())
        };
        let mapping = if internet.port_mapping {
            PortmapperConfig::default()
        } else {
            PortmapperConfig::Disabled
        };
        let mut report = NetReportConfig::default();
        report.captive_portal_check = false;
        let endpoint = runtime.block_on(async {
            Builder::new(iroh::endpoint::presets::Minimal)
                .clear_ip_transports()
                .bind_addr(config.listen)
                .map_err(|_| Error::Protocol("invalid QUIC bind address"))?
                .alpns(vec![ALPN.to_vec()])
                .relay_mode(relay_mode)
                .portmapper_config(mapping)
                .net_report_config(report)
                .bind()
                .await
                .map_err(|_| Error::Protocol("cannot bind internet endpoint"))
        })?;
        let address = endpoint
            .bound_sockets()
            .into_iter()
            .next()
            .ok_or(Error::Protocol("internet endpoint has no socket"))?;
        let public_endpoint = endpoint.clone();
        let (candidate_input, discovery) = if internet.dynamic_candidates {
            let (input, adapter) =
                discovery::Adapter::new(endpoint.id(), config.max_peers, relays)?;
            (Some(input), Some(adapter))
        } else {
            (None, None)
        };
        let key = Zeroizing::new(*psk);
        let (commands, rx) = async_mpsc::channel(QUEUE);
        let (tx, events) = mpsc::sync_channel(QUEUE);
        let (diagnostic_tx, diagnostics) = mpsc::sync_channel(16);
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let worker = thread::Builder::new().name("benilla-internet".into()).spawn(move || {
            runtime.block_on(async move {
                let backend = Backend::Internet { endpoint: endpoint.clone(), peers };
                tokio::select! { _ = run(backend,config,key,hello,identity,rx,tx,diagnostic_tx,discovery) => {}, _ = stopped => {} }
                // Best-effort bounded graceful close, including router mapping cleanup.
                let _ = timeout(Duration::from_secs(2), endpoint.close()).await;
            });
        })?;
        Ok(Self {
            address,
            endpoint: Some(public_endpoint),
            candidate_input,
            commands,
            events,
            diagnostics,
            stop: Some(stop),
            worker: Some(worker),
        })
    }
    /// Current private-share bootstrap ticket. It contains public identity and addresses, no
    /// authentication secret. Wait for a configured relay to appear before sharing across NAT.
    /// Endpoint identity is ephemeral: restarting requires sharing a fresh ticket.
    pub fn endpoint_ticket(&self) -> Option<String> {
        self.endpoint
            .as_ref()
            .and_then(|endpoint| serde_json::to_string(&endpoint.addr()).ok())
    }
}

pub(super) enum Backend {
    Tcp(TcpListener),
    Internet {
        endpoint: Endpoint,
        peers: Vec<EndpointAddr>,
    },
}
pub(super) enum Candidate {
    Tcp(TcpStream),
    Internet(Box<iroh::endpoint::Incoming>),
}
impl Backend {
    pub fn endpoint(&self) -> Option<Endpoint> {
        match self {
            Self::Tcp(_) => None,
            Self::Internet { endpoint, .. } => Some(endpoint.clone()),
        }
    }
    pub fn targets(&self, config: &TransportConfig) -> Vec<String> {
        match self {
            Self::Tcp(_) => config.peers.iter().map(ToString::to_string).collect(),
            Self::Internet { peers, .. } => peers
                .iter()
                .filter_map(|p| serde_json::to_string(p).ok())
                .collect(),
        }
    }
    pub fn is_self(&self, address: &str) -> bool {
        match self {
            Self::Tcp(listener) => listener
                .local_addr()
                .is_ok_and(|local| local.to_string() == address),
            Self::Internet { endpoint, .. } => {
                parse_ticket(address).is_ok_and(|addr| addr.id == endpoint.id())
            }
        }
    }
    pub async fn accept(&self) -> Option<(Candidate, String)> {
        match self {
            Self::Tcp(listener) => listener
                .accept()
                .await
                .ok()
                .map(|(stream, addr)| (Candidate::Tcp(stream), addr.to_string())),
            Self::Internet { endpoint, .. } => endpoint.accept().await.map(|incoming| {
                let address = format!("{:?}", incoming.remote_addr());
                (Candidate::Internet(Box::new(incoming)), address)
            }),
        }
    }
}
pub(super) fn diagnostic_address(address: &str) -> String {
    // Never emit a full private-share ticket through application logging.
    parse_ticket(address)
        .map(|addr| format!("endpoint {}", addr.id))
        .unwrap_or_else(|_| address.to_owned())
}

pub(super) async fn connect(
    candidate: Option<Candidate>,
    endpoint: Option<Endpoint>,
    address: &str,
) -> Result<(Stream, Option<iroh::EndpointId>), Error> {
    let connection = match candidate {
        Some(Candidate::Tcp(stream)) => {
            stream.set_nodelay(true)?;
            return Ok((Box::new(stream), None));
        }
        Some(Candidate::Internet(incoming)) => {
            let connection = (*incoming)
                .await
                .map_err(|_| Error::Protocol("internet handshake failed"))?;
            let (send, recv) = connection
                .accept_bi()
                .await
                .map_err(|_| Error::Protocol("internet stream failed"))?;
            return Ok((
                Box::new(tokio::io::join(recv, send)),
                Some(connection.remote_id()),
            ));
        }
        None => match endpoint {
            None => {
                let stream = TcpStream::connect(address).await?;
                stream.set_nodelay(true)?;
                return Ok((Box::new(stream), None));
            }
            Some(endpoint) => endpoint
                .connect(parse_ticket(address)?, ALPN)
                .await
                .map_err(|_| Error::Protocol("internet connection failed"))?,
        },
    };
    let (send, recv) = connection
        .open_bi()
        .await
        .map_err(|_| Error::Protocol("internet stream failed"))?;
    Ok((
        Box::new(tokio::io::join(recv, send)),
        Some(connection.remote_id()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    fn key() -> [u8; 32] {
        std::array::from_fn(|i| i as u8)
    }
    fn hello(id: u8) -> Hello {
        Hello {
            protocol_version: PROTOCOL_VERSION,
            network: "internet-test".into(),
            peer_id: [id; 16],
            ruleset_hash: [7; 32],
            content_version: "test".into(),
        }
    }
    fn config() -> TransportConfig {
        TransportConfig {
            listen: "127.0.0.1:0".parse().unwrap(),
            handshake_timeout: Duration::from_secs(3),
            ..Default::default()
        }
    }
    fn next(n: &Network) -> Event {
        let start = Instant::now();
        loop {
            match n.try_recv() {
                Ok(event) => return event,
                Err(mpsc::TryRecvError::Empty) if start.elapsed() < Duration::from_secs(8) => {
                    thread::sleep(Duration::from_millis(5))
                }
                result => panic!("expected internet event: {result:?}"),
            }
        }
    }
    fn ticket(n: &Network) -> String {
        let start = Instant::now();
        loop {
            let ticket = n.endpoint_ticket().unwrap();
            if parse_ticket(&ticket).is_ok() {
                return ticket;
            }
            assert!(
                start.elapsed() < Duration::from_secs(3),
                "endpoint did not publish local address"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn no_implicit_infrastructure_or_router_mapping() {
        let config = InternetConfig::default();
        assert!(!config.port_mapping);
        assert!(!config.dynamic_candidates);
        assert!(config.relay_urls.is_empty());
        assert!(config.peer_tickets.is_empty());
        config.validate().unwrap();
    }
    #[test]
    fn tickets_and_relays_are_bounded_and_allowlisted() {
        for value in [
            "http://example.com",
            "https://user:password@example.com",
            "https://example.com/?token=secret",
            "https://example.com/#fragment",
            "not a url",
        ] {
            let config = InternetConfig {
                relay_urls: vec![value.into()],
                ..Default::default()
            };
            assert!(config.validate().is_err());
        }
        let addr = EndpointAddr::new(iroh::SecretKey::from_bytes(&[1; 32]).public())
            .with_relay_url("https://relay.example".parse().unwrap());
        let mut config = InternetConfig {
            peer_tickets: vec![serde_json::to_string(&addr).unwrap()],
            ..Default::default()
        };
        assert!(config.validate().is_err());
        let diagnostic = diagnostic_address(&config.peer_tickets[0]);
        assert!(!diagnostic.contains("relay.example"));
        assert!(diagnostic.starts_with("endpoint "));
        config.relay_urls.push("https://relay.example".into());
        config.validate().unwrap();
        config.peer_tickets.push(config.peer_tickets[0].clone());
        assert!(config.validate().is_err());
        config.peer_tickets = vec!["x".repeat(MAX_TICKET + 1)];
        assert!(config.validate().is_err());
        assert!(parse_ticket("{}").is_err());
    }
    #[test]
    fn quic_loopback_authenticates_and_exchanges_bounded_frames() {
        let a =
            Network::start_internet(config(), InternetConfig::default(), &key(), hello(1)).unwrap();
        let b = Network::start_internet(
            config(),
            InternetConfig {
                peer_tickets: vec![ticket(&a)],
                ..Default::default()
            },
            &key(),
            hello(2),
        )
        .unwrap();
        assert!(matches!(next(&a), Event::PeerJoined(h) if h.peer_id == [2;16]));
        assert!(matches!(next(&b), Event::PeerJoined(h) if h.peer_id == [1;16]));
        a.try_send(Command::Broadcast(vec![5; MAX_PAYLOAD_SIZE]))
            .unwrap();
        assert!(
            matches!(next(&b), Event::Frame { payload, .. } if payload == vec![5;MAX_PAYLOAD_SIZE])
        );
        assert!(a
            .try_send(Command::Broadcast(vec![5; MAX_PAYLOAD_SIZE + 1]))
            .is_err());
        drop(a);
        assert!(matches!(next(&b), Event::PeerLeft(_)));
    }
    fn owner(seed: u8) -> OwnerIdentity {
        use ed25519_dalek::{Signer, SigningKey};
        let key = SigningKey::from_bytes(&[seed; 32]);
        OwnerIdentity::new(key.verifying_key().to_bytes(), move |challenge| {
            let mut message = b"benilla-transport-identity-v1\0".to_vec();
            message.extend_from_slice(challenge);
            key.sign(&message).to_bytes()
        })
        .unwrap()
    }
    fn owned_hello(seed: u8) -> Hello {
        let mut hello = hello(seed);
        hello.peer_id = owner(seed).owner_id();
        hello
    }
    #[test]
    fn owned_quic_loopback_authenticates_persistent_owners_and_exchanges_frames() {
        let a = Network::start_internet_owned(
            config(),
            InternetConfig::default(),
            &key(),
            owned_hello(1),
            owner(1),
        )
        .unwrap();
        let b = Network::start_internet_owned(
            config(),
            InternetConfig {
                peer_tickets: vec![ticket(&a)],
                ..Default::default()
            },
            &key(),
            owned_hello(2),
            owner(2),
        )
        .unwrap();
        assert!(matches!(next(&a), Event::PeerJoined(h) if h == owned_hello(2)));
        assert!(matches!(next(&b), Event::PeerJoined(h) if h == owned_hello(1)));
        b.try_send(Command::Broadcast(vec![9; MAX_PAYLOAD_SIZE]))
            .unwrap();
        assert!(matches!(next(&a), Event::Frame { peer_id, payload }
            if peer_id == owner(2).owner_id() && payload == vec![9; MAX_PAYLOAD_SIZE]));
        drop(b);
        assert!(matches!(next(&a), Event::PeerLeft(id) if id == owner(2).owner_id()));
    }
    #[test]
    fn disconnect_removes_only_the_targeted_quic_owner() {
        let a = Network::start_internet_owned(
            config(),
            InternetConfig::default(),
            &key(),
            owned_hello(1),
            owner(1),
        )
        .unwrap();
        let peer_config = InternetConfig {
            peer_tickets: vec![ticket(&a)],
            ..Default::default()
        };
        let b = Network::start_internet_owned(
            config(),
            peer_config.clone(),
            &key(),
            owned_hello(2),
            owner(2),
        )
        .unwrap();
        assert!(matches!(next(&a), Event::PeerJoined(h) if h == owned_hello(2)));
        assert!(matches!(next(&b), Event::PeerJoined(h) if h == owned_hello(1)));
        let c =
            Network::start_internet_owned(config(), peer_config, &key(), owned_hello(3), owner(3))
                .unwrap();
        assert!(matches!(next(&a), Event::PeerJoined(h) if h == owned_hello(3)));
        assert!(matches!(next(&c), Event::PeerJoined(h) if h == owned_hello(1)));
        a.try_send(Command::Disconnect {
            peer_id: owner(2).owner_id(),
        })
        .unwrap();
        assert!(matches!(next(&a), Event::PeerLeft(id) if id == owner(2).owner_id()));
        assert!(matches!(next(&b), Event::PeerLeft(id) if id == owner(1).owner_id()));
        c.try_send(Command::Broadcast(b"unaffected QUIC owner".to_vec()))
            .unwrap();
        assert!(matches!(next(&a), Event::Frame { peer_id, payload }
            if peer_id == owner(3).owner_id() && payload == b"unaffected QUIC owner"));
        a.try_send(Command::Broadcast(b"continues running".to_vec()))
            .unwrap();
        assert!(matches!(next(&c), Event::Frame { peer_id, payload }
            if peer_id == owner(1).owner_id() && payload == b"continues running"));
        assert!(matches!(a.try_recv(), Err(mpsc::TryRecvError::Empty)));
    }
    #[test]
    fn owned_quic_rejects_legacy_group_only_peers() {
        let a = Network::start_internet_owned(
            config(),
            InternetConfig::default(),
            &key(),
            owned_hello(1),
            owner(1),
        )
        .unwrap();
        let b = Network::start_internet(
            config(),
            InternetConfig {
                peer_tickets: vec![ticket(&a)],
                ..Default::default()
            },
            &key(),
            hello(2),
        )
        .unwrap();
        assert!(matches!(next(&a), Event::Rejected { .. }));
        assert!(matches!(next(&b), Event::Rejected { .. }));
    }
    #[test]
    fn quic_wrong_psk_never_joins() {
        let a =
            Network::start_internet(config(), InternetConfig::default(), &key(), hello(1)).unwrap();
        let mut wrong = key();
        wrong[0] ^= 5;
        let b = Network::start_internet(
            config(),
            InternetConfig {
                peer_tickets: vec![ticket(&a)],
                ..Default::default()
            },
            &wrong,
            hello(2),
        )
        .unwrap();
        assert!(matches!(next(&a), Event::Rejected { .. }));
        assert!(matches!(next(&b), Event::Rejected { .. }));
    }
    // Real loopback relay, with all direct IP transports removed. This is a relay-path
    // regression test, not proof that hole punching works across household NATs or CGNAT.
    #[tokio::test]
    async fn relay_only_stream_retains_end_to_end_psk_authentication() {
        relay_only(false).await;
    }
    #[tokio::test]
    async fn relay_only_stream_retains_end_to_end_owner_authentication() {
        relay_only(true).await;
    }
    async fn relay_only(owned: bool) {
        timeout(Duration::from_secs(20), async {
            let (relays, _, _server) = iroh::test_utils::run_relay_server_with(false)
                .await
                .unwrap();
            let make_endpoint = || {
                Builder::new(iroh::endpoint::presets::Minimal)
                    .clear_ip_transports()
                    .relay_mode(RelayMode::Custom(relays.clone()))
                    .alpns(vec![ALPN.to_vec()])
                    .portmapper_config(PortmapperConfig::Disabled)
                    .net_report_config({
                        let mut config = NetReportConfig::default();
                        config.captive_portal_check = false;
                        config
                    })
                    // Only this loopback fixture uses its generated self-signed certificate.
                    .ca_tls_config(iroh::tls::CaTlsConfig::insecure_skip_verify())
            };
            let a = make_endpoint().bind().await.unwrap();
            let b = make_endpoint().bind().await.unwrap();
            timeout(Duration::from_secs(5), a.online())
                .await
                .expect("first relay endpoint online");
            timeout(Duration::from_secs(5), b.online())
                .await
                .expect("second relay endpoint online");
            assert!(a.bound_sockets().is_empty());
            assert!(b.bound_sockets().is_empty());
            let address = serde_json::to_string(&a.addr()).unwrap();
            let receive = async {
                let incoming = a.accept().await.unwrap();
                let (mut stream, _) = connect(
                    Some(Candidate::Internet(Box::new(incoming))),
                    None,
                    "loopback relay",
                )
                .await
                .unwrap();
                let identity = owned.then(|| owner(1));
                let local = if owned { owned_hello(1) } else { hello(1) };
                let (mut noise, remote) =
                    handshake_mode(&mut stream, &key(), &local, false, identity.as_ref())
                        .await
                        .unwrap();
                assert_eq!(
                    remote.peer_id,
                    if owned { owner(2).owner_id() } else { [2; 16] }
                );
                let ciphertext = read_wire(&mut stream).await.unwrap();
                let mut clear = [0; 64];
                let len = noise.read_message(&ciphertext, &mut clear).unwrap();
                assert_eq!(&clear[..len], b"private relayed frame");
            };
            let send = async {
                let (mut stream, _) = connect(None, Some(b.clone()), &address).await.unwrap();
                let identity = owned.then(|| owner(2));
                let local = if owned { owned_hello(2) } else { hello(2) };
                let (mut noise, _) =
                    handshake_mode(&mut stream, &key(), &local, true, identity.as_ref())
                        .await
                        .unwrap();
                let mut ciphertext = [0; 64];
                let len = noise
                    .write_message(b"private relayed frame", &mut ciphertext)
                    .unwrap();
                write_wire(&mut stream, &ciphertext[..len]).await.unwrap();
                // Keep the stream alive until the receiver has consumed the queued bytes.
                time::sleep(Duration::from_millis(100)).await;
            };
            tokio::join!(receive, send);
            a.close().await;
            b.close().await;
        })
        .await
        .expect("loopback relay test timed out");
    }

    #[test]
    fn mixed_tcp_and_internet_configuration_fails_before_binding() {
        let mut config = config();
        config.peers.push("127.0.0.1:1234".parse().unwrap());
        assert!(
            Network::start_internet(config, InternetConfig::default(), &key(), hello(1)).is_err()
        );
    }

    #[test]
    fn dynamic_adapter_is_disabled_by_default_and_solo_startup_is_nonblocking() {
        let started = Instant::now();
        let network = Network::start_internet_owned(
            config(),
            InternetConfig::default(),
            &key(),
            owned_hello(1),
            owner(1),
        )
        .unwrap();
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(network
            .submit_discovery_candidate(&ticket(&network))
            .is_err());
        assert!(network.try_recv().is_err());
    }

    #[test]
    fn dynamic_owned_candidates_connect_without_manual_bootstrap_tickets() {
        let a = Network::start_internet_owned(
            config(),
            InternetConfig::default(),
            &key(),
            owned_hello(1),
            owner(1),
        )
        .unwrap();
        let b = Network::start_internet_owned(
            config(),
            InternetConfig {
                dynamic_candidates: true,
                ..Default::default()
            },
            &key(),
            owned_hello(2),
            owner(2),
        )
        .unwrap();
        assert!(b.try_recv().is_err());
        let initial = ticket(&a);
        b.submit_discovery_candidate(&initial).unwrap();
        assert!(matches!(next(&a), Event::PeerJoined(h) if h.peer_id == owner(2).owner_id()));
        assert!(matches!(next(&b), Event::PeerJoined(h) if h.peer_id == owner(1).owner_id()));
        // A changed address set for an already active EndpointId must not initiate another
        // handshake, reconnect, or reset its owner session.
        let blackhole = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let changed = parse_ticket(&initial)
            .unwrap()
            .with_ip_addr(blackhole.local_addr().unwrap());
        b.submit_discovery_candidate(&serde_json::to_string(&changed).unwrap())
            .unwrap();
        thread::sleep(Duration::from_millis(2200));
        assert!(b.try_recv().is_err());
        assert!(a.try_recv().is_err());
        b.try_send(Command::Broadcast(b"dynamic".to_vec())).unwrap();
        assert!(matches!(next(&a), Event::Frame { payload, .. } if payload == b"dynamic"));
    }

    #[test]
    fn dynamic_candidates_cannot_bypass_group_or_owner_authentication() {
        let a = Network::start_internet_owned(
            config(),
            InternetConfig::default(),
            &key(),
            owned_hello(1),
            owner(1),
        )
        .unwrap();
        let wrong_key = std::array::from_fn(|i| (i as u8).wrapping_add(1));
        let b = Network::start_internet_owned(
            config(),
            InternetConfig {
                dynamic_candidates: true,
                ..Default::default()
            },
            &wrong_key,
            owned_hello(2),
            owner(2),
        )
        .unwrap();
        b.submit_discovery_candidate(&ticket(&a)).unwrap();
        assert!(matches!(next(&b), Event::Rejected { .. }));
        assert!(matches!(next(&a), Event::Rejected { .. }));
        drop(b);
        // A legacy peer with the correct shared key still cannot join an owned network.
        let legacy = Network::start_internet(
            config(),
            InternetConfig {
                dynamic_candidates: true,
                ..Default::default()
            },
            &key(),
            hello(3),
        )
        .unwrap();
        legacy.submit_discovery_candidate(&ticket(&a)).unwrap();
        assert!(matches!(next(&legacy), Event::Rejected { .. }));
        assert!(matches!(next(&a), Event::Rejected { .. }));
    }

    #[test]
    fn stopping_cancels_pending_dynamic_dials() {
        let mut settings = config();
        settings.handshake_timeout = Duration::from_secs(30);
        let network = Network::start_internet_owned(
            settings,
            InternetConfig {
                dynamic_candidates: true,
                ..Default::default()
            },
            &key(),
            owned_hello(1),
            owner(1),
        )
        .unwrap();
        let blackhole = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let unreachable = EndpointAddr::new(iroh::SecretKey::from_bytes(&[99; 32]).public())
            .with_ip_addr(blackhole.local_addr().unwrap());
        network
            .submit_discovery_candidate(&serde_json::to_string(&unreachable).unwrap())
            .unwrap();
        // The retry timer has ticked while the QUIC handshake is still pending.
        thread::sleep(Duration::from_millis(2200));
        let started = Instant::now();
        drop(network);
        assert!(started.elapsed() < Duration::from_secs(4));
    }

    #[test]
    fn fresh_dynamic_ticket_is_not_starved_by_stale_manual_address() {
        let a = Network::start_internet_owned(
            config(),
            InternetConfig::default(),
            &key(),
            owned_hello(1),
            owner(1),
        )
        .unwrap();
        let fresh = ticket(&a);
        let blackhole = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let stale = EndpointAddr::new(parse_ticket(&fresh).unwrap().id)
            .with_ip_addr(blackhole.local_addr().unwrap());
        let mut settings = config();
        settings.handshake_timeout = Duration::from_secs(1);
        let b = Network::start_internet_owned(
            settings,
            InternetConfig {
                peer_tickets: vec![serde_json::to_string(&stale).unwrap()],
                dynamic_candidates: true,
                ..Default::default()
            },
            &key(),
            owned_hello(2),
            owner(2),
        )
        .unwrap();
        b.submit_discovery_candidate(&fresh).unwrap();
        let started = Instant::now();
        loop {
            match next(&b) {
                Event::PeerJoined(h) => {
                    assert_eq!(h.peer_id, owner(1).owner_id());
                    break;
                }
                Event::Rejected { .. } if started.elapsed() < Duration::from_secs(6) => {}
                event => panic!("unexpected dynamic event: {event:?}"),
            }
        }
        assert!(matches!(next(&a), Event::PeerJoined(_)));
    }

    #[test]
    fn late_advertisement_does_not_replace_an_inbound_owned_session() {
        let (local, remote) = if owner(1).owner_id() < owner(2).owner_id() {
            (1, 2)
        } else {
            (2, 1)
        };
        let a = Network::start_internet_owned(
            config(),
            InternetConfig {
                dynamic_candidates: true,
                ..Default::default()
            },
            &key(),
            owned_hello(local),
            owner(local),
        )
        .unwrap();
        let b = Network::start_internet_owned(
            config(),
            InternetConfig {
                peer_tickets: vec![ticket(&a)],
                ..Default::default()
            },
            &key(),
            owned_hello(remote),
            owner(remote),
        )
        .unwrap();
        assert!(matches!(next(&a), Event::PeerJoined(_)));
        assert!(matches!(next(&b), Event::PeerJoined(_)));
        // Local owner sorts smaller: an unnecessary outgoing handshake would replace the
        // healthy inbound session under the normal symmetric-dial preference rule.
        a.submit_discovery_candidate(&ticket(&b)).unwrap();
        thread::sleep(Duration::from_millis(2200));
        assert!(a.try_recv().is_err());
        assert!(b.try_recv().is_err());
        b.try_send(Command::Broadcast(b"still inbound".to_vec()))
            .unwrap();
        assert!(matches!(next(&a), Event::Frame { payload, .. } if payload == b"still inbound"));
    }
}
