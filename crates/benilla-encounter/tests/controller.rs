// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_encounter::Controller;
use benilla_session::Status;
use std::{
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};
fn pair() -> [Controller; 2] {
    configured_pair(100, false)
}
fn configured_pair(tick_limit: u64, mismatch: bool) -> [Controller; 2] {
    let a = TcpListener::bind("127.0.0.1:0").unwrap();
    let b = TcpListener::bind("127.0.0.1:0").unwrap();
    let addresses = [a.local_addr().unwrap(), b.local_addr().unwrap()];
    let make = |i: usize| {
        let id = format!("{:02x}", i + 1).repeat(16);
        let text = format!(
            r#"
[p2p]
psk = "test-only synthetic encounter phrase"
listen = "{}"
[p2p.encounter]
local_id = "{id}"
epoch = "0101010101010101010101010101010101010101010101010101010101010101"
tick_limit = {tick_limit}
roster = [{{id="01010101010101010101010101010101", address="{}"}}, {{id="02020202020202020202020202020202", address="{}"}}]
"#,
            addresses[i], addresses[0], addresses[1]
        );
        let mut cfg = benilla_config::AppConfig::parse(&text, &std::env::temp_dir()).unwrap();
        if mismatch && i == 1 {
            cfg.p2p.as_mut().unwrap().encounter.as_mut().unwrap().epoch = [9; 32];
        }
        Controller::new(cfg.p2p.unwrap(), cfg.ruleset).unwrap()
    };
    [make(0), make(1)]
}
fn wait(cs: &mut [Controller], predicate: impl Fn(&[Controller]) -> bool) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut output = Vec::new();
    loop {
        for c in cs.iter_mut() {
            output.extend(c.poll());
        }
        if predicate(cs) {
            return output;
        }
        assert!(Instant::now() < deadline, "controller deadline: {output:?}");
        thread::sleep(Duration::from_millis(3));
    }
}
#[test]
fn chat_controls_agree_attack_pause_resume_commit_rewards_and_abort() {
    let mut cs = pair();
    assert!(cs[0].command("!attack")[0].contains("Wait"));
    assert!(cs.iter().all(|c| c.world().is_none()));
    for c in &mut cs {
        assert!(c.command("!ready")[0].contains("Ready submitted"));
    }
    assert!(cs[0].command("!ready")[0].contains("Already"));
    wait(&mut cs, |cs| {
        cs.iter().all(|c| c.status() == Some(&Status::Running))
    });
    for c in &mut cs {
        c.command("!attack");
    }
    wait(&mut cs, |cs| {
        cs.iter().all(|c| c.world().unwrap().tick() >= 2)
    });
    cs[0].command("!pause");
    // Let any already submitted round settle, then ensure no subsequent progress/rewards.
    let deadline = Instant::now() + Duration::from_millis(600);
    while Instant::now() < deadline {
        for c in &mut cs {
            c.poll();
        }
        thread::sleep(Duration::from_millis(3));
    }
    let snapshot = cs[0].world().unwrap().state_hash();
    let deadline = Instant::now() + Duration::from_millis(600);
    while Instant::now() < deadline {
        for c in &mut cs {
            c.poll();
        }
        thread::sleep(Duration::from_millis(3));
    }
    assert_eq!(snapshot, cs[0].world().unwrap().state_hash());
    assert_eq!(snapshot, cs[1].world().unwrap().state_hash());
    assert!(cs[0].report()[0].contains("paused=true"));
    cs[0].command("!resume");
    wait(&mut cs, |cs| {
        cs.iter()
            .all(|c| c.world().unwrap().players().values().any(|p| p.xp > 0))
    });
    cs[0].command("!pause");
    cs[1].command("!pause");
    wait(&mut cs, |cs| {
        cs[0].world().unwrap().state_hash() == cs[1].world().unwrap().state_hash()
    });
    let w = cs[0].world().unwrap();
    assert!(w
        .players()
        .values()
        .any(|p| p.inventory.get(&1) == Some(&1)));
    assert!(cs[0].report().iter().any(|s| s.contains("not WoW items")));
    cs[0].command("!leave");
    wait(&mut cs, |cs| {
        matches!(cs[1].status(), Some(Status::Aborted(_)))
    });
    assert!(cs[0].closed());
    assert!(cs[0].command("!ready")[0].contains("Restart"));
}
#[test]
fn missing_member_never_starts_or_commits() {
    let mut cs = pair();
    // Ready without the other frozen member never shrinks the roster or elects a host.
    cs[0].command("!ready");
    let until = Instant::now() + Duration::from_millis(350);
    while Instant::now() < until {
        cs[0].poll();
        thread::sleep(Duration::from_millis(3));
    }
    assert_eq!(cs[0].status(), Some(&Status::Handshaking));
    assert_eq!(cs[0].world().unwrap().tick(), 0);
    cs[0].command("!leave");
    assert!(matches!(cs[0].status(), Some(Status::Aborted(_))));
}

#[test]
fn differing_epoch_aborts_before_tick_zero() {
    let mut cs = configured_pair(2, true);
    for c in &mut cs {
        c.command("!ready");
    }
    wait(&mut cs, |cs| {
        cs.iter()
            .all(|c| matches!(c.status(), Some(Status::Aborted(_))))
    });
    assert!(cs.iter().all(|c| c.world().unwrap().tick() == 0));
}
#[test]
fn terminal_tick_limit_preserves_only_confirmed_results() {
    let mut cs = configured_pair(2, false);
    for c in &mut cs {
        c.command("!ready");
    }
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut turn = 0;
    while !cs.iter().all(|c| c.status() == Some(&Status::Finished)) {
        cs[0].poll();
        // The second UI drains much less frequently than the first.
        if turn % 10 == 0 {
            cs[1].poll();
        }
        assert!(Instant::now() < deadline);
        turn += 1;
        thread::sleep(Duration::from_millis(3));
    }
    assert!(cs
        .iter()
        .all(|c| c.closed() && c.world().unwrap().tick() == 2));
    assert_eq!(
        cs[0].world().unwrap().state_hash(),
        cs[1].world().unwrap().state_hash()
    );
    assert!(cs[0].command("!attack")[0].contains("closed"));
}
