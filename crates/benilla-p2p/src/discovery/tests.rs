use super::*;

fn id(byte: u8) -> EndpointId {
    iroh::SecretKey::from_bytes(&[byte; 32]).public()
}
fn ticket(byte: u8, port: u16) -> String {
    serde_json::to_string(&EndpointAddr::new(id(byte)).with_ip_addr(([127, 0, 0, 1], port).into()))
        .unwrap()
}
fn pool(limit: usize) -> CandidatePool {
    CandidatePool::new(id(1), limit, vec![]).unwrap()
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|v| format!("{v:02x}")).collect()
}

#[test]
fn hkdf_matches_independent_python_hmac_vector() {
    // Python stdlib hmac.digest: extract(salt, ikm), then expand(info || 0x01).
    let psk = std::array::from_fn(|i| i as u8);
    let keys = DiscoveryKeys::derive(&psk, "test-world").unwrap();
    assert_eq!(
        hex(keys.topic()),
        "808f2abc41b55b530390e9a68970b5920140d8e13d277fa571f5f245750f4556"
    );
    assert_eq!(
        hex(keys.announcement_key()),
        "bd5f5b091f497c4dd85f19a58e93a2b40ebdfadac0110b4ffe978a64a993cc52"
    );
    assert_ne!(keys.topic(), keys.announcement_key());
    assert_ne!(keys.topic(), &psk);
}

#[test]
fn derivation_is_exact_and_network_bound() {
    let a = DiscoveryKeys::derive(&[7; 32], "world").unwrap();
    let same = DiscoveryKeys::derive(&[7; 32], "world").unwrap();
    assert_eq!(a.topic(), same.topic());
    assert_eq!(a.announcement_key(), same.announcement_key());
    for network in ["World", "world ", "wörld"] {
        let different = DiscoveryKeys::derive(&[7; 32], network).unwrap();
        assert_ne!(a.topic(), different.topic());
        assert_ne!(a.announcement_key(), different.announcement_key());
    }
    assert_ne!(
        a.topic(),
        DiscoveryKeys::derive(&[8; 32], "world").unwrap().topic()
    );
    assert!(DiscoveryKeys::derive(&[0; 32], "").is_err());
    assert!(DiscoveryKeys::derive(&[0; 32], &"é".repeat(65)).is_err());
    assert!(DiscoveryKeys::derive(&[0; 32], &"é".repeat(64)).is_ok());
    // No strength policy here: even config-derived keys from empty passphrases remain valid.
    assert!(DiscoveryKeys::derive(&[0; 32], "world").is_ok());
}

#[test]
fn capacity_self_expiration_and_removal_are_bounded() {
    let now = Instant::now();
    let mut p = pool(2);
    assert!(!p.observe(&ticket(1, 1234), now).unwrap());
    assert!(p.is_empty());
    assert!(p.observe(&ticket(2, 1234), now).unwrap());
    assert!(p.observe(&ticket(3, 1234), now).unwrap());
    assert!(!p.observe(&ticket(4, 1234), now).unwrap());
    assert_eq!(p.len(), 2);
    p.remove(&id(2));
    assert!(p.observe(&ticket(4, 1234), now).unwrap());
    p.expire(now + CANDIDATE_TTL - Duration::from_nanos(1));
    assert_eq!(p.len(), 2);
    assert!(p.next_due(now + CANDIDATE_TTL).is_none());
    assert!(p.is_empty());
    assert!(p.observe(&ticket(2, 1234), now + CANDIDATE_TTL).unwrap());
}

#[test]
fn refresh_updates_address_and_ttl_without_resetting_backoff() {
    let now = Instant::now();
    let mut p = pool(1);
    p.observe(&ticket(2, 1234), now).unwrap();
    assert!(p.next_due(now).is_some());
    p.observe(&ticket(2, 4321), now + Duration::from_millis(500))
        .unwrap();
    assert!(p.next_due(now + Duration::from_millis(999)).is_none());
    let address = p.next_due(now + Duration::from_secs(1)).unwrap();
    assert_eq!(address.ip_addrs().next().unwrap().port(), 4321);
    assert_eq!(p.len(), 1);
    p.expire(now + CANDIDATE_TTL);
    assert_eq!(p.len(), 1);
    p.expire(now + CANDIDATE_TTL + Duration::from_millis(500));
    assert!(p.is_empty());
}

#[test]
fn retries_are_exponential_and_capped_even_under_repeated_advertising() {
    let mut now = Instant::now();
    let mut p = pool(1);
    p.observe(&ticket(2, 1234), now).unwrap();
    for seconds in [1, 2, 4, 8, 16, 32, 60, 60, 60] {
        assert!(p.next_due(now).is_some());
        assert!(p.next_due(now).is_none());
        p.observe(&ticket(2, 1234), now).unwrap();
        now += Duration::from_secs(seconds);
        assert!(p.next_due(now - Duration::from_nanos(1)).is_none());
    }
    assert!(p.next_due(now).is_some());
}

#[test]
fn due_selection_does_not_starve_older_candidates() {
    let now = Instant::now();
    let mut p = pool(2);
    p.observe(&ticket(2, 1234), now).unwrap();
    p.observe(&ticket(3, 1234), now).unwrap();
    let first = p.next_due(now).unwrap();
    let second = p.next_due(now + Duration::from_secs(10)).unwrap();
    assert_ne!(first.id, second.id);
}

#[test]
fn malformed_or_unapproved_candidates_never_enter_pool() {
    let now = Instant::now();
    let mut p = pool(1);
    for invalid in [
        "not json".into(),
        "x".repeat(8193),
        ticket(2, 0),
        serde_json::to_string(&EndpointAddr::new(id(2))).unwrap(),
    ] {
        assert!(p.observe(&invalid, now).is_err());
        assert!(p.is_empty());
    }
    let relay: RelayUrl = "https://relay.example.org".parse().unwrap();
    let address = EndpointAddr::new(id(2)).with_relay_url(relay.clone());
    let serialized = serde_json::to_string(&address).unwrap();
    assert!(p.observe(&serialized, now).is_err());
    let mut allowed = CandidatePool::new(id(1), 1, vec![relay.clone()]).unwrap();
    assert!(allowed.observe(&serialized, now).unwrap());
    assert!(CandidatePool::new(id(1), 1, vec![relay.clone(), relay]).is_err());
    assert!(
        CandidatePool::new(id(1), 1, vec!["http://relay.example.org".parse().unwrap()]).is_err()
    );
    assert!(CandidatePool::new(id(1), 0, vec![]).is_err());
    assert!(CandidatePool::new(id(1), 65, vec![]).is_err());
}

#[test]
fn refreshed_idle_incumbents_can_yield_to_newcomers_but_churn_is_paced() {
    let now = Instant::now();
    let mut p = pool(2);
    let empty = HashSet::new();
    p.observe(&ticket(2, 1234), now).unwrap();
    p.observe(&ticket(3, 1234), now).unwrap();
    p.next_due(now).unwrap();
    p.next_due(now).unwrap();
    assert!(!p
        .observe_protected(
            &ticket(4, 1234),
            now + MIN_RESIDENCE - Duration::from_nanos(1),
            &empty
        )
        .unwrap());
    // Advertisements do not reset original residence, so refreshing cannot pin a slot.
    p.observe(&ticket(2, 5678), now + MIN_RESIDENCE).unwrap();
    assert!(p
        .observe_protected(&ticket(4, 1234), now + MIN_RESIDENCE, &empty)
        .unwrap());
    assert_eq!(p.len(), 2);
    assert!(!p
        .observe_protected(&ticket(5, 1234), now + MIN_RESIDENCE, &empty)
        .unwrap());
    assert!(p
        .observe_protected(
            &ticket(5, 1234),
            now + MIN_RESIDENCE + EVICTION_INTERVAL,
            &empty
        )
        .unwrap());
    assert!(p.contains(&id(4)) && p.contains(&id(5)));
}

#[test]
fn active_candidates_are_neither_evicted_nor_scheduled_again() {
    let now = Instant::now();
    let mut p = pool(1);
    p.observe(&ticket(2, 1234), now).unwrap();
    p.next_due(now).unwrap();
    let protected = HashSet::from([id(2)]);
    p.observe(&ticket(2, 5678), now + MIN_RESIDENCE).unwrap();
    assert!(!p
        .observe_protected(&ticket(3, 1234), now + MIN_RESIDENCE, &protected)
        .unwrap());
    assert!(p
        .next_due_excluding(now + MIN_RESIDENCE, &protected)
        .is_none());
    // Removing protection resumes the same bounded retry state.
    assert_eq!(p.next_due(now + MIN_RESIDENCE).unwrap().id, id(2));
}

#[test]
fn candidates_get_an_initial_attempt_before_eviction() {
    let now = Instant::now();
    let mut p = pool(1);
    p.observe(&ticket(2, 1234), now).unwrap();
    assert!(!p
        .observe_protected(&ticket(3, 1234), now + MIN_RESIDENCE, &HashSet::new())
        .unwrap());
    assert_eq!(p.next_due(now + MIN_RESIDENCE).unwrap().id, id(2));
}

#[test]
fn candidate_input_validates_before_queueing_and_is_bounded() {
    let (input, mut adapter) = Adapter::new(id(1), 1, vec![]).unwrap();
    assert!(input.submit(&"x".repeat(8193)).is_err());
    let relay = serde_json::to_string(
        &EndpointAddr::new(id(2)).with_relay_url("https://unapproved.example".parse().unwrap()),
    )
    .unwrap();
    assert!(input.submit(&relay).is_err());
    for _ in 0..INPUT_QUEUE {
        input.submit(&ticket(2, 1234)).unwrap();
    }
    assert!(input.submit(&ticket(2, 1234)).is_err());
    assert_eq!(adapter.receiver.try_recv().unwrap(), ticket(2, 1234));
    input.submit(&ticket(2, 1234)).unwrap();
    drop(adapter);
    assert!(input.submit(&ticket(2, 1234)).is_err());
}
