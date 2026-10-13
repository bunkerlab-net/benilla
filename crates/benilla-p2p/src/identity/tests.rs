use super::*;
use ed25519_dalek::{Signer, SigningKey};
use std::time::Instant;

fn key() -> [u8; 32] {
    std::array::from_fn(|i| i as u8)
}
fn signing_key(seed: u8) -> SigningKey {
    // Fictional fixture keys, never persisted or suitable for a real player.
    SigningKey::from_bytes(&[seed; 32])
}
fn sign(key: &SigningKey, challenge: &[u8; 32]) -> [u8; 64] {
    let mut message = SIGNING_DOMAIN.to_vec();
    message.extend_from_slice(challenge);
    key.sign(&message).to_bytes()
}
fn identity(seed: u8) -> OwnerIdentity {
    let key = signing_key(seed);
    OwnerIdentity::new(key.verifying_key().to_bytes(), move |challenge| {
        sign(&key, challenge)
    })
    .unwrap()
}
fn hello(seed: u8) -> Hello {
    Hello {
        protocol_version: PROTOCOL_VERSION,
        network: "owner-test".into(),
        peer_id: identity(seed).owner_id(),
        ruleset_hash: [3; 32],
        content_version: "test-content-v1".into(),
    }
}
fn config(peers: Vec<SocketAddr>) -> TransportConfig {
    TransportConfig {
        listen: "127.0.0.1:0".parse().unwrap(),
        peers,
        handshake_timeout: Duration::from_secs(2),
        max_peers: 8,
    }
}
fn next(network: &Network) -> Event {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        match network.try_recv() {
            Ok(event) => return event,
            Err(mpsc::TryRecvError::Empty) if Instant::now() < until => {
                thread::sleep(Duration::from_millis(5));
            }
            other => panic!("expected owner-authentication event: {other:?}"),
        }
    }
}
fn joined(network: &Network, seed: u8) {
    assert!(matches!(next(network), Event::PeerJoined(h) if h == hello(seed)));
}
fn owned_hello(seed: u8) -> OwnedHello {
    OwnedHello {
        hello: hello(seed),
        public_key: identity(seed).public_key,
    }
}

#[test]
fn owner_id_derivation_has_a_stable_profile_format_vector() {
    assert_eq!(
        identity(1).owner_id(),
        [
            0xa2, 0x43, 0x02, 0xea, 0xb0, 0xac, 0x44, 0xa2, 0xb8, 0x8e, 0x29, 0xbb, 0xac, 0xa5,
            0x5e, 0x83
        ],
    );
}

#[test]
fn owned_peers_still_require_the_group_key_and_compatible_claims() {
    for incompatible in 0..4 {
        let a = Network::start_owned(config(vec![]), &key(), hello(1), identity(1)).unwrap();
        let mut peer_key = key();
        let mut other = hello(2);
        match incompatible {
            0 => peer_key[0] ^= 1,
            1 => other.network.push('x'),
            2 => other.ruleset_hash[0] ^= 1,
            _ => other.content_version.push('x'),
        }
        let b = Network::start_owned(config(vec![a.local_addr()]), &peer_key, other, identity(2))
            .unwrap();
        assert!(matches!(next(&a), Event::Rejected { .. }));
        assert!(matches!(next(&b), Event::Rejected { .. }));
    }
}

#[test]
fn three_owned_tcp_peers_exchange_and_survive_one_departure() {
    let a = Network::start_owned(config(vec![]), &key(), hello(1), identity(1)).unwrap();
    let b =
        Network::start_owned(config(vec![a.local_addr()]), &key(), hello(2), identity(2)).unwrap();
    joined(&a, 2);
    joined(&b, 1);
    let c = Network::start_owned(
        config(vec![a.local_addr(), b.local_addr()]),
        &key(),
        hello(3),
        identity(3),
    )
    .unwrap();
    joined(&a, 3);
    joined(&b, 3);
    let mut c_peers = Vec::new();
    for _ in 0..2 {
        match next(&c) {
            Event::PeerJoined(h) => c_peers.push(h.peer_id),
            event => panic!("expected authenticated owner: {event:?}"),
        }
    }
    c_peers.sort();
    let mut expected = vec![hello(1).peer_id, hello(2).peer_id];
    expected.sort();
    assert_eq!(c_peers, expected);
    a.try_send(Command::Broadcast(b"character-introduction".to_vec()))
        .unwrap();
    for peer in [&b, &c] {
        assert!(matches!(next(peer), Event::Frame { peer_id, payload }
            if peer_id == hello(1).peer_id && payload == b"character-introduction"));
    }
    drop(a);
    for peer in [&b, &c] {
        assert!(matches!(next(peer), Event::PeerLeft(id) if id == hello(1).peer_id));
    }
    b.try_send(Command::Send {
        peer_id: hello(3).peer_id,
        payload: vec![7; MAX_PAYLOAD_SIZE],
    })
    .unwrap();
    assert!(matches!(next(&c), Event::Frame { peer_id, payload }
        if peer_id == hello(2).peer_id && payload == vec![7; MAX_PAYLOAD_SIZE]));
}

#[test]
fn owned_starts_reject_false_local_owner_before_binding() {
    let config = config(vec![]);
    assert!(Network::start_owned(config.clone(), &key(), hello(2), identity(1)).is_err());
    assert!(Network::start_internet_owned(
        config,
        InternetConfig::default(),
        &key(),
        hello(2),
        identity(1),
    )
    .is_err());
    assert!(OwnerIdentity::new([0; 32], |_| [0; 64]).is_err());
}

#[test]
fn legacy_downgrade_is_rejected_in_both_dial_directions() {
    for owned_listener in [true, false] {
        let a = if owned_listener {
            Network::start_owned(config(vec![]), &key(), hello(1), identity(1))
        } else {
            Network::start(config(vec![]), &key(), hello(1))
        }
        .unwrap();
        let b = if owned_listener {
            Network::start(config(vec![a.local_addr()]), &key(), hello(2))
        } else {
            Network::start_owned(config(vec![a.local_addr()]), &key(), hello(2), identity(2))
        }
        .unwrap();
        assert!(matches!(next(&a), Event::Rejected { .. }));
        assert!(matches!(next(&b), Event::Rejected { .. }));
    }
}

#[test]
fn owner_challenge_binds_both_claim_sets_key_role_and_noise_session() {
    let first = owned_hello(1);
    let second = owned_hello(2);
    let binding = [8; 32];
    let digest = challenge(&binding, &first, &second, true);
    let signature = sign(&signing_key(1), &digest);
    verify(&first.public_key, &digest, &signature).unwrap();
    assert!(verify(&first.public_key, &digest, &signature[..63]).is_err());
    assert!(verify(
        &first.public_key,
        &challenge(&[9; 32], &first, &second, true),
        &signature,
    )
    .is_err());
    assert!(verify(
        &first.public_key,
        &challenge(&binding, &first, &second, false),
        &signature,
    )
    .is_err());
    for change_initiator in [true, false] {
        for field in 0..6 {
            let mut a = first.clone();
            let mut b = second.clone();
            let peer = if change_initiator { &mut a } else { &mut b };
            match field {
                0 => peer.hello.protocol_version += 1,
                1 => peer.hello.network.push('x'),
                2 => peer.hello.peer_id[0] ^= 1,
                3 => peer.hello.ruleset_hash[0] ^= 1,
                4 => peer.hello.content_version.push('x'),
                _ => peer.public_key[0] ^= 1,
            }
            assert!(verify(
                &first.public_key,
                &challenge(&binding, &a, &b, true),
                &signature,
            )
            .is_err());
        }
    }
    let raw_digest_signature = signing_key(1).sign(&digest).to_bytes();
    assert!(verify(&first.public_key, &digest, &raw_digest_signature).is_err());
}

async fn introduce(
    address: SocketAddr,
    local: &OwnedHello,
) -> (TcpStream, snow::TransportState, [u8; 32], OwnedHello) {
    let mut stream = TcpStream::connect(address).await.unwrap();
    let (mut noise, binding) = noise_handshake(&mut stream, &key(), true, true)
        .await
        .unwrap();
    send(&mut stream, &mut noise, &serde_json::to_vec(local).unwrap())
        .await
        .unwrap();
    let remote = receive(&mut stream, &mut noise, MAX_HELLO).await.unwrap();
    let remote = serde_json::from_slice(&remote).unwrap();
    (stream, noise, binding, remote)
}

#[tokio::test]
async fn same_psk_cannot_impersonate_another_owner_or_replace_their_connection() {
    let a = Network::start_owned(config(vec![]), &key(), hello(1), identity(1)).unwrap();
    let victim =
        Network::start_owned(config(vec![a.local_addr()]), &key(), hello(2), identity(2)).unwrap();
    joined(&a, 2);
    joined(&victim, 1);
    // Attacker knows the shared PSK and victim public identity, but only owns key 3.
    let claim = owned_hello(2);
    let (mut stream, mut noise, binding, remote) = introduce(a.local_addr(), &claim).await;
    let forged = sign(&signing_key(3), &challenge(&binding, &claim, &remote, true));
    send(&mut stream, &mut noise, &forged).await.unwrap();
    assert!(matches!(next(&a), Event::Rejected { .. }));
    victim
        .try_send(Command::Broadcast(b"original owner survives".to_vec()))
        .unwrap();
    assert!(matches!(next(&a), Event::Frame { peer_id, payload }
        if peer_id == hello(2).peer_id && payload == b"original owner survives"));
}

#[tokio::test]
async fn claimed_owner_id_must_match_public_key_even_with_the_shared_psk() {
    let a = Network::start_owned(config(vec![]), &key(), hello(1), identity(1)).unwrap();
    let mut claim = owned_hello(2);
    claim.hello.peer_id = hello(3).peer_id;
    let (_stream, _noise, _, _) = introduce(a.local_addr(), &claim).await;
    assert!(matches!(next(&a), Event::Rejected { reason, .. }
        if reason.contains("does not match owner public key")));
}

#[tokio::test]
async fn owner_signature_replay_over_a_fresh_real_tcp_noise_session_is_rejected() {
    let a = Network::start_owned(config(vec![]), &key(), hello(1), identity(1)).unwrap();
    let claim = owned_hello(2);
    let (mut stream, mut noise, binding, remote) = introduce(a.local_addr(), &claim).await;
    let signature = sign(&signing_key(2), &challenge(&binding, &claim, &remote, true));
    send(&mut stream, &mut noise, &signature).await.unwrap();
    let response = receive(&mut stream, &mut noise, 64).await.unwrap();
    verify(
        &remote.public_key,
        &challenge(&binding, &claim, &remote, false),
        &response,
    )
    .unwrap();
    joined(&a, 2);
    drop(stream);
    assert!(matches!(next(&a), Event::PeerLeft(_)));
    let (mut stream, mut noise, fresh_binding, _) = introduce(a.local_addr(), &claim).await;
    assert_ne!(binding, fresh_binding);
    send(&mut stream, &mut noise, &signature).await.unwrap();
    assert!(matches!(next(&a), Event::Rejected { .. }));
}

#[tokio::test]
async fn owner_authentication_frames_are_bounded_and_proof_is_mandatory() {
    let mut cfg = config(vec![]);
    cfg.handshake_timeout = Duration::from_millis(300);
    let a = Network::start_owned(cfg, &key(), hello(1), identity(1)).unwrap();
    let mut stream = TcpStream::connect(a.local_addr()).await.unwrap();
    let _ = noise_handshake(&mut stream, &key(), true, true)
        .await
        .unwrap();
    stream.write_u32((MAX_HELLO + 17) as u32).await.unwrap();
    assert!(matches!(next(&a), Event::Rejected { .. }));
    let (_stream, _noise, _, _) = introduce(a.local_addr(), &owned_hello(2)).await;
    assert!(matches!(next(&a), Event::Rejected { reason, .. } if reason.contains("timed out")));
}

#[test]
fn disconnect_removes_only_the_targeted_tcp_owner_and_reports_it_once() {
    let a = Network::start_owned(config(vec![]), &key(), hello(1), identity(1)).unwrap();
    let b =
        Network::start_owned(config(vec![a.local_addr()]), &key(), hello(2), identity(2)).unwrap();
    joined(&a, 2);
    joined(&b, 1);
    let c =
        Network::start_owned(config(vec![a.local_addr()]), &key(), hello(3), identity(3)).unwrap();
    joined(&a, 3);
    joined(&c, 1);
    a.try_send(Command::Disconnect {
        peer_id: hello(2).peer_id,
    })
    .unwrap();
    assert!(matches!(next(&a), Event::PeerLeft(id) if id == hello(2).peer_id));
    assert!(matches!(next(&b), Event::PeerLeft(id) if id == hello(1).peer_id));
    // Duplicate and unknown disconnect commands are harmless.
    a.try_send(Command::Disconnect {
        peer_id: hello(2).peer_id,
    })
    .unwrap();
    a.try_send(Command::Disconnect {
        peer_id: hello(4).peer_id,
    })
    .unwrap();
    c.try_send(Command::Broadcast(b"honest owner remains".to_vec()))
        .unwrap();
    assert!(matches!(next(&a), Event::Frame { peer_id, payload }
        if peer_id == hello(3).peer_id && payload == b"honest owner remains"));
    a.try_send(Command::Broadcast(b"solo state still running".to_vec()))
        .unwrap();
    assert!(matches!(next(&c), Event::Frame { peer_id, payload }
        if peer_id == hello(1).peer_id && payload == b"solo state still running"));
    assert!(matches!(a.try_recv(), Err(mpsc::TryRecvError::Empty)));
}
