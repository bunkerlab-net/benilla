//! Deliberately bounded probes. No automatic dialing, discovery, router mappings or relays.
use super::Settings;
use std::{
    net::{TcpListener, TcpStream},
    time::Duration,
};
pub(super) fn start(settings: &mut Settings, peers: bool) {
    if settings.recovery {
        settings.status = "Network tests are disabled until the repaired config is saved and Benilla is restarted.".into();
        settings.refresh = true;
        return;
    }
    if settings.diagnostic.is_some() {
        return;
    }
    let addresses = if peers {
        match settings
            .prepare_config()
            .and_then(|e| e.validate().map_err(|e| e.to_string()))
        {
            Ok(config) => match config.p2p {
                Some(config) => config.peers.into_iter().take(8).collect::<Vec<_>>(),
                None => {
                    settings.status = "Enable and configure P2P first; no traffic was sent.".into();
                    settings.refresh = true;
                    return;
                }
            },
            Err(e) => {
                settings.status = e;
                settings.refresh = true;
                return;
            }
        }
    } else {
        Vec::new()
    };
    if peers && addresses.is_empty() {
        settings.status = "No direct TCP peers configured; no traffic was sent. Relay/PSK tests are not available here.".into();
        settings.refresh = true;
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    settings.diagnostic = Some(std::sync::Mutex::new(rx));
    settings.status = "Testing… (maximum 2 seconds per peer, first 8 peers only)".into();
    settings.refresh = true;
    std::thread::spawn(move || {
        let result = if peers {
            let passed = addresses
                .iter()
                .filter(|&&address| {
                    TcpStream::connect_timeout(&address, Duration::from_secs(2)).is_ok()
                })
                .count();
            format!(
                "TCP: {passed}/{} configured peers accepted a connection. PSK handshake, relay and NAT reachability are NOT tested.",
                addresses.len()
            )
        } else {
            match TcpListener::bind("127.0.0.1:0").and_then(|listener| TcpStream::connect_timeout(&listener.local_addr()?,Duration::from_secs(2))) {
                Ok(_) => "Local TCP loopback works. Configured listen port, PSK, relay and Internet reachability are NOT tested.".into(),
                Err(_) => "Local TCP loopback failed. Check local socket permissions; no Internet connection was attempted.".into(),
            }
        };
        let _ = tx.send(result);
    });
}

pub(super) fn start_psk(settings: &mut Settings) {
    if settings.recovery {
        settings.status = "Network tests are disabled until restart.".into();
        settings.refresh = true;
        return;
    }
    if settings.diagnostic.is_some() {
        return;
    }
    let config = match settings
        .prepare_config()
        .and_then(|e| e.validate().map_err(|e| e.to_string()))
    {
        Ok(config) if config.p2p.is_some() => config,
        Ok(_) => {
            settings.status =
                "Enable P2P and enter a passphrase first; no traffic was sent.".into();
            settings.refresh = true;
            return;
        }
        Err(error) => {
            settings.status = error;
            settings.refresh = true;
            return;
        }
    };
    let (tx, rx) = std::sync::mpsc::channel();
    settings.diagnostic = Some(std::sync::Mutex::new(rx));
    settings.status = "Testing PSK authentication and encrypted payload on localhost only…".into();
    settings.refresh = true;
    std::thread::spawn(move || {
        let result = if local_psk(config).is_ok() {
            "Local PSK handshake and encrypted payload passed. Remote peer passphrases, relay and NAT were NOT tested."
        } else {
            "Local PSK self-test failed. No configured peer or relay was contacted."
        };
        let _ = tx.send(result.into());
    });
}
fn local_psk(config: benilla_config::AppConfig) -> Result<(), ()> {
    use benilla_p2p::{Command, Event, Hello, Network, TransportConfig, PROTOCOL_VERSION};
    let p2p = config.p2p.ok_or(())?;
    let hello = |id| Hello {
        protocol_version: PROTOCOL_VERSION,
        network: p2p.network.clone(),
        peer_id: [id; 16],
        ruleset_hash: config.ruleset.fingerprint(),
        content_version: "benilla-settings-local-test".into(),
    };
    let transport = |peers| TransportConfig {
        listen: "127.0.0.1:0".parse().unwrap(),
        peers,
        handshake_timeout: Duration::from_secs(2),
        max_peers: 2,
    };
    let a = Network::start(transport(vec![]), p2p.psk_bytes(), hello(1)).map_err(|_| ())?;
    let b = Network::start(transport(vec![a.local_addr()]), p2p.psk_bytes(), hello(2))
        .map_err(|_| ())?;
    let deadline = std::time::Instant::now() + Duration::from_secs(4);
    let mut sent = false;
    while std::time::Instant::now() < deadline {
        if !sent && matches!(b.try_recv(), Ok(Event::PeerJoined(_))) {
            b.try_send(Command::Broadcast(b"settings-self-test".to_vec()))
                .map_err(|_| ())?;
            sent = true;
        }
        if matches!(a.try_recv(),Ok(Event::Frame { payload,.. }) if payload == b"settings-self-test")
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Err(())
}
