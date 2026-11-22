// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_p2p::{Hello, Network, TransportConfig};
use benilla_session::{Command, Event, Manifest, Member, Session, Status, SESSION_PROTOCOL};
use benilla_sim::{
    encounter::{self as sim, Content, CreatureSpec, LootSpec, PlayerSpec, Rules, World},
    xp::Level,
};
use std::{
    thread,
    time::{Duration, Instant},
};
fn world(n: u8) -> World {
    World::new(
        Rules {
            tick_ms: 50,
            xp_milli: 1000,
        },
        Content {
            creatures: vec![CreatureSpec {
                entity: 99,
                level: Level::new(10).unwrap(),
                max_health: 20,
                damage: 1,
                swing_ticks: 1,
                respawn_ticks: 3,
                loot: Some(LootSpec {
                    item: 123,
                    quantity: 2,
                }),
            }],
        },
        (1..=n)
            .map(|i| PlayerSpec {
                peer: i as u64,
                level: Level::new(10).unwrap(),
                max_health: 100,
                damage: 10,
                swing_ticks: 1,
                respawn_ticks: 2,
            })
            .collect(),
    )
    .unwrap()
}
fn network(i: u8, peers: Vec<std::net::SocketAddr>) -> Network {
    Network::start(
        TransportConfig {
            listen: "127.0.0.1:0".parse().unwrap(),
            peers,
            handshake_timeout: Duration::from_secs(1),
            max_peers: 16,
        },
        &std::array::from_fn(|i| i as u8),
        Hello {
            protocol_version: benilla_p2p::PROTOCOL_VERSION,
            network: "synthetic-encounter-test".into(),
            peer_id: [i; 16],
            ruleset_hash: [3; 32],
            content_version: "synthetic-v1-classic-1.12.1".into(),
        },
    )
    .unwrap()
}
fn manifest(n: u8) -> Manifest {
    Manifest {
        protocol: SESSION_PROTOCOL,
        simulation_version: sim::SIMULATION_VERSION,
        epoch: [1; 32],
        roster: (1..=n)
            .map(|i| Member {
                transport: [i; 16],
                simulation: i as u64,
            })
            .collect(),
        rules_hash: [3; 32],
        content_hash: [4; 32],
        initial_state_hash: world(n).state_hash(),
        tick_limit: 8,
    }
}
fn setup(n: u8) -> Vec<Session> {
    let mut addresses = vec![];
    let mut sessions = vec![];
    for i in 1..=n {
        let net = network(i, addresses.clone());
        addresses.push(net.local_addr());
        sessions.push(
            Session::new(net, [i; 16], manifest(n), world(n), Duration::from_secs(2)).unwrap(),
        );
    }
    sessions
}
fn wait(s: &mut [Session], mut done: impl FnMut(&[Session]) -> bool) -> Vec<Vec<Event>> {
    let until = Instant::now() + Duration::from_secs(5);
    let mut events = vec![vec![]; s.len()];
    loop {
        for (i, session) in s.iter_mut().enumerate() {
            events[i].extend(session.poll());
        }
        if done(s) {
            return events;
        }
        assert!(
            Instant::now() < until,
            "timed out: {:?}",
            s.iter().map(Session::status).collect::<Vec<_>>()
        );
        thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn two_and_three_real_transports_commit_identical_combat_rewards_and_replay() {
    for n in [2, 3] {
        let mut s = setup(n);
        wait(&mut s, |s| s.iter().all(|s| s.status() == &Status::Running));
        let initial = world(n).snapshot().unwrap();
        let mut logs = vec![vec![]; n as usize];
        for tick in 0..8 {
            for session in s.iter_mut().rev() {
                session.submit(vec![Command::Attack(99)]).unwrap();
            }
            let events = wait(&mut s, |s| s.iter().all(|s| s.world().tick() == tick + 1));
            for (i, events) in events.into_iter().enumerate() {
                for event in events {
                    if let Event::Committed(c) = event {
                        logs[i].push(c);
                    }
                }
            }
        }
        assert!(s.iter().all(|s| s.status() == &Status::Finished));
        assert!(logs.windows(2).all(|pair| pair[0] == pair[1]));
        assert_eq!(logs[0].len(), 8);
        assert!(logs[0]
            .iter()
            .flat_map(|c| &c.events)
            .any(|e| matches!(e,sim::Event::Reward{xp,..} if *xp>0)));
        assert!(s[0]
            .world()
            .players()
            .values()
            .any(|p| p.inventory.get(&123).is_some_and(|n| *n > 0)));
        for session in &s {
            assert_eq!(
                session.world().state_hash(),
                World::replay(initial.clone(), &logs[0])
                    .unwrap()
                    .state_hash()
            );
        }
        println!("{n} real TCP/Noise peers: 8 identical ticks, synthetic combat/XP/loot and replay verified");
    }
}
#[test]
fn mismatched_start_aborts_before_tick_zero() {
    let a = network(1, vec![]);
    let b = network(2, vec![a.local_addr()]);
    let mut bad = manifest(2);
    bad.content_hash[0] ^= 1;
    let mut s = vec![
        Session::new(a, [1; 16], manifest(2), world(2), Duration::from_secs(2)).unwrap(),
        Session::new(b, [2; 16], bad, world(2), Duration::from_secs(2)).unwrap(),
    ];
    wait(&mut s, |s| {
        s.iter().all(|s| matches!(s.status(), Status::Aborted(_)))
    });
    assert!(s.iter().all(|s| s.world().tick() == 0));
}
#[test]
fn missing_input_times_out_without_advancing_and_duplicate_local_rejected() {
    let mut s = setup(2);
    wait(&mut s, |s| s.iter().all(|s| s.status() == &Status::Running));
    s[0].submit(vec![]).unwrap();
    assert_eq!(
        s[0].submit(vec![]),
        Err(benilla_session::Error::DuplicateInput)
    );
    wait(&mut s, |s| {
        s.iter().all(|s| matches!(s.status(), Status::Aborted(_)))
    });
    assert!(s.iter().all(|s| s.world().tick() == 0));
}
#[test]
fn disconnect_and_same_identity_reconnect_cannot_resume_epoch() {
    let a = network(1, vec![]);
    let addr = a.local_addr();
    let b = network(2, vec![addr]);
    let mut s = vec![
        Session::new(a, [1; 16], manifest(2), world(2), Duration::from_secs(2)).unwrap(),
        Session::new(b, [2; 16], manifest(2), world(2), Duration::from_secs(2)).unwrap(),
    ];
    wait(&mut s, |s| s.iter().all(|s| s.status() == &Status::Running));
    drop(s.pop());
    wait(&mut s, |s| matches!(s[0].status(), Status::Aborted(_)));
    let _reconnected = network(2, vec![addr]);
    thread::sleep(Duration::from_millis(100));
    s[0].poll();
    assert!(matches!(s[0].status(), Status::Aborted(_)));
    assert_eq!(s[0].world().tick(), 0);
    assert!(s[0].submit(vec![]).is_err());
}

fn raw_send(net: &Network, value: serde_json::Value) {
    net.try_send(benilla_p2p::Command::Broadcast(
        serde_json::to_vec(&value).unwrap(),
    ))
    .unwrap();
}
fn raw_pair() -> (Session, Network) {
    use sha2::{Digest, Sha256};
    let a = network(1, vec![]);
    let b = network(2, vec![a.local_addr()]);
    let mut session =
        Session::new(a, [1; 16], manifest(2), world(2), Duration::from_secs(2)).unwrap();
    let until = Instant::now() + Duration::from_secs(3);
    let mut proposed = false;
    let mut ready = false;
    while session.status() != &Status::Running {
        session.poll();
        while let Ok(event) = b.try_recv() {
            if let benilla_p2p::Event::Frame { payload, .. } = event {
                let msg: serde_json::Value = serde_json::from_slice(&payload).unwrap();
                if msg.get("Propose").is_some() {
                    assert!(!proposed);
                    proposed = true;
                    raw_send(&b, serde_json::json!({"Propose":manifest(2)}));
                }
                if msg.get("Ready").is_some() {
                    assert!(!ready);
                    ready = true;
                    let digest: [u8; 32] =
                        Sha256::digest(serde_json::to_vec(&manifest(2)).unwrap()).into();
                    raw_send(
                        &b,
                        serde_json::json!({"Ready":{"epoch":vec![1;32],"manifest":digest.to_vec()}}),
                    );
                }
            }
        }
        assert!(
            Instant::now() < until,
            "raw handshake timed out {:?}",
            session.status()
        );
        thread::sleep(Duration::from_millis(2));
    }
    (session, b)
}
#[test]
fn duplicate_remote_input_aborts_over_actual_transport() {
    let (session, b) = raw_pair();
    let mut s = vec![session];
    let msg = serde_json::json!({"Input":{"epoch":vec![1;32],"tick":0,"commands":[]}});
    raw_send(&b, msg.clone());
    raw_send(&b, msg);
    wait(&mut s, |s| matches!(s[0].status(), Status::Aborted(_)));
    assert_eq!(s[0].world().tick(), 0);
    assert!(matches!(s[0].status(),Status::Aborted(reason) if reason.contains("duplicate")));
}
#[test]
fn mismatched_vote_aborts_without_committing_rewards() {
    let (mut session, b) = raw_pair();
    session.submit(vec![Command::Attack(99)]).unwrap();
    let mut s = vec![session];
    raw_send(
        &b,
        serde_json::json!({"Input":{"epoch":vec![1;32],"tick":0,"commands":[{"Attack":99}]}}),
    );
    raw_send(
        &b,
        serde_json::json!({"Vote":{"epoch":vec![1;32],"tick":0,"inputs":vec![9;32],"state":vec![9;32]}}),
    );
    wait(&mut s, |s| matches!(s[0].status(), Status::Aborted(_)));
    assert_eq!(s[0].world().state_hash(), world(2).state_hash());
    assert!(matches!(s[0].status(),Status::Aborted(reason) if reason.contains("hash mismatch")));
}
#[test]
fn stale_epoch_future_tick_and_oversized_commands_abort() {
    for msg in [
        serde_json::json!({"Input":{"epoch":vec![2;32],"tick":0,"commands":[]}}),
        serde_json::json!({"Input":{"epoch":vec![1;32],"tick":2,"commands":[]}}),
        serde_json::json!({"Input":{"epoch":vec![1;32],"tick":0,"commands":vec!["StopAttack";17]}}),
    ] {
        let (session, b) = raw_pair();
        let mut s = vec![session];
        raw_send(&b, msg);
        wait(&mut s, |s| matches!(s[0].status(), Status::Aborted(_)));
        assert_eq!(s[0].world().tick(), 0);
    }
}

#[test]
fn independently_paced_peers_tolerate_one_tick_of_arrival_skew() {
    let mut sessions = setup(3);
    let mut sent = [None; 3];
    let mut committed = [0; 3];
    let until = Instant::now() + Duration::from_secs(5);
    let mut turn = 0;
    while !sessions.iter().all(|s| s.status() == &Status::Finished) {
        for (i, session) in sessions.iter_mut().enumerate() {
            if turn % (i + 1) != 0 {
                continue;
            }
            for event in session.poll() {
                if matches!(event, Event::Committed(_)) {
                    committed[i] += 1;
                }
            }
            assert!(
                !matches!(session.status(), Status::Aborted(_)),
                "{:?}",
                session.status()
            );
            if session.status() == &Status::Running && sent[i] != Some(session.world().tick()) {
                sent[i] = Some(session.world().tick());
                session.submit(vec![Command::Attack(99)]).unwrap();
            }
        }
        assert!(Instant::now() < until);
        turn += 1;
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(committed, [8; 3]);
    assert!(sessions
        .windows(2)
        .all(|p| p[0].world().state_hash() == p[1].world().state_hash()));
}

#[test]
fn manifest_validation_rejects_ambiguous_or_inconsistent_rosters() {
    for case in 0..7 {
        let mut m = manifest(2);
        match case {
            0 => m.roster.reverse(),
            1 => m.roster[1].simulation = 1,
            2 => m.initial_state_hash[0] ^= 1,
            3 => m.epoch = [0; 32],
            4 => m.tick_limit = 0,
            5 => m.protocol += 1,
            _ => m.simulation_version += 1,
        }
        assert!(matches!(
            Session::new(
                network(1, vec![]),
                [1; 16],
                m,
                world(2),
                Duration::from_secs(1)
            ),
            Err(benilla_session::Error::InvalidManifest)
        ));
    }
    assert!(benilla_session::should_dial_pair([2; 16], [1; 16]));
    assert!(!benilla_session::should_dial_pair([1; 16], [2; 16]));
    assert!(!benilla_session::should_dial_pair([1; 16], [1; 16]));
}
#[test]
fn missing_hash_vote_keeps_kill_xp_and_loot_uncommitted() {
    let (mut session, b) = raw_pair();
    session.submit(vec![Command::Attack(99)]).unwrap();
    let mut s = vec![session];
    raw_send(
        &b,
        serde_json::json!({"Input":{"epoch":vec![1;32],"tick":0,"commands":[{"Attack":99}]}}),
    );
    wait(&mut s, |s| matches!(s[0].status(), Status::Aborted(_)));
    assert_eq!(s[0].world().state_hash(), world(2).state_hash());
    assert!(s[0]
        .world()
        .players()
        .values()
        .all(|p| p.xp == 0 && p.inventory.is_empty()));
}
