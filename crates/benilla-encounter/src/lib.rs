// SPDX-License-Identifier: GPL-3.0-or-later
//! Chat-driven synthetic encounter, isolated from exploration and real WoW state.
//! All displayed simulation state comes from Session::world's committed snapshot.
use anyhow::{anyhow, bail, Result};
use benilla_config::{P2pConfig, Ruleset};
use benilla_p2p::{Hello, Network, TransportConfig};
use benilla_session::{Command, Event, Manifest, Member, Session, Status, SESSION_PROTOCOL};
use benilla_sim::{encounter as sim, xp::Level};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

pub const HELP: &str = "SYNTHETIC TEST ENCOUNTER: /say !ready, !attack, !stop, !pause, !resume, !status, !leave. No WoW combat, range, shared movement or saves. HP/XP/test tokens exist only in these reports, never your character inventory.";
const CONTENT: &str = "benilla-synthetic-chat-encounter-v1";
const TICK: Duration = Duration::from_millis(250);
const TIMEOUT: Duration = Duration::from_secs(60);
const TARGET: u64 = 1;
const TOKEN: u32 = 1;

pub struct Controller {
    config: P2pConfig,
    rules: Ruleset,
    session: Option<Session>,
    terminal: Option<(sim::World, Status)>,
    release_at: Option<Instant>,
    paused: bool,
    attack: bool,
    submitted: Option<u64>,
    last_submit: Instant,
    closed: bool,
}
impl Controller {
    pub fn new(config: P2pConfig, rules: Ruleset) -> Result<Self> {
        if config.encounter.is_none() {
            bail!("encounter is not configured");
        }
        // Test tokens are guaranteed drops. A chance multiplier has no meaningful effect here.
        if rules.drop_milli() != 1000 {
            bail!("synthetic encounter requires ruleset.drop_rate = 1; loot chances are not implemented");
        }
        Ok(Self {
            config,
            rules,
            session: None,
            terminal: None,
            release_at: None,
            paused: false,
            attack: false,
            submitted: None,
            last_submit: Instant::now(),
            closed: false,
        })
    }
    pub fn closed(&self) -> bool {
        self.closed
    }
    pub fn world(&self) -> Option<&sim::World> {
        self.session
            .as_ref()
            .map(Session::world)
            .or_else(|| self.terminal.as_ref().map(|(world, _)| world))
    }
    pub fn status(&self) -> Option<&Status> {
        self.session
            .as_ref()
            .map(Session::status)
            .or_else(|| self.terminal.as_ref().map(|(_, status)| status))
    }
    pub fn lobby(&self) -> Vec<String> {
        let cfg = self.config.encounter.as_ref().unwrap();
        let mut lines = vec![HELP.into(), format!("Frozen roster: {} members; 250ms logical ticks; {} ticks maximum. Every member must /say !ready within 60s of the first. Confirm the same fresh epoch and roster out of band.", cfg.roster.len(), cfg.tick_limit)];
        for (i, m) in cfg.roster.iter().enumerate() {
            let id: String = m.id.iter().map(|b| format!("{b:02x}")).collect();
            lines.push(format!(
                "Test player {}: {}{}",
                i + 1,
                id,
                if m.id == cfg.local_id { " (you)" } else { "" }
            ));
        }
        lines
    }
    pub fn command(&mut self, text: &str) -> Vec<String> {
        if text.trim() == "!status" {
            return self.report();
        }
        if self.closed {
            return vec!["Encounter closed. Restart with a fresh epoch.".into()];
        }
        match text.trim() {
            "!ready" if self.session.is_none() => match self.start() {
                Ok(()) => vec!["Ready submitted. Waiting for every frozen member to agree; no simulation has committed yet.".into()],
                Err(_) => { self.closed = true; vec!["Cannot start encounter transport/session. Check the configured local endpoint and roster; restart with a fresh epoch.".into()] }
            },
            "!ready" => vec!["Already submitted ready; no second session was started.".into()],
            "!leave" => {
                if let Some(s) = &mut self.session { s.stop(); }
                self.closed = true;
                self.close_transport();
                vec!["Left encounter. Remaining peers abort; everyone needs a fresh epoch to restart.".into()]
            },
            "!attack" | "!stop" | "!pause" | "!resume" => {
                if self.status() != Some(&Status::Running) { return vec!["Wait for every member to start before issuing encounter controls.".into()]; }
                match text.trim() {
                    "!attack" => { self.attack = true; vec!["Attack queued for your next unsent tick (synthetic target #1); rewards wait for all-peer confirmation.".into()] },
                    "!stop" => { self.attack = false; vec!["Stop attack queued for your next unsent tick.".into()] },
                    "!pause" => { self.paused = true; vec!["Paused before your next unsent tick. An already submitted tick may still commit. The whole group then waits; the existing 60s barrier deadline still aborts everyone. !resume continues before timeout.".into()] },
                    _ => { self.paused = false; vec!["Resumed local tick submissions; every member is still required.".into()] }
                }
            },
            _ => vec![HELP.into()],
        }
    }
    fn start(&mut self) -> Result<()> {
        let cfg = self.config.encounter.as_ref().unwrap();
        let roster: Vec<_> = cfg
            .roster
            .iter()
            .enumerate()
            .map(|(i, m)| Member {
                transport: m.id,
                simulation: i as u64 + 1,
            })
            .collect();
        let world = sim::World::new(
            sim::Rules {
                tick_ms: TICK.as_millis() as u32,
                xp_milli: self.rules.xp_milli(),
            },
            sim::Content {
                creatures: vec![sim::CreatureSpec {
                    entity: TARGET,
                    level: Level::new(1).unwrap(),
                    max_health: 100,
                    damage: 5,
                    swing_ticks: 4,
                    respawn_ticks: 40,
                    loot: Some(sim::LootSpec {
                        item: TOKEN,
                        quantity: 1,
                    }),
                }],
            },
            roster
                .iter()
                .map(|m| sim::PlayerSpec {
                    peer: m.simulation,
                    level: Level::new(1).unwrap(),
                    max_health: 100,
                    damage: 10,
                    swing_ticks: 4,
                    respawn_ticks: 20,
                })
                .collect(),
        )
        .map_err(|e| anyhow!("invalid synthetic world: {e:?}"))?;
        let manifest = Manifest {
            protocol: SESSION_PROTOCOL,
            simulation_version: sim::SIMULATION_VERSION,
            epoch: cfg.epoch,
            roster,
            rules_hash: self.rules.fingerprint(),
            content_hash: Sha256::digest(CONTENT.as_bytes()).into(),
            initial_state_hash: world.state_hash(),
            tick_limit: cfg.tick_limit,
        };
        let network = Network::start(
            TransportConfig {
                listen: self.config.listen,
                peers: cfg
                    .roster
                    .iter()
                    .filter(|m| benilla_session::should_dial_pair(cfg.local_id, m.id))
                    .map(|m| m.address)
                    .collect(),
                handshake_timeout: Duration::from_millis(self.config.timeout_ms),
                max_peers: cfg.roster.len() - 1,
            },
            self.config.psk_bytes(),
            Hello {
                protocol_version: benilla_p2p::PROTOCOL_VERSION,
                network: self.config.network.clone(),
                peer_id: cfg.local_id,
                ruleset_hash: self.rules.fingerprint(),
                content_version: CONTENT.into(),
            },
        )?;
        // Ownership transfers without consuming even one PeerJoined event.
        self.session = Some(
            Session::new(network, cfg.local_id, manifest, world, TIMEOUT)
                .map_err(|e| anyhow!("session: {e:?}"))?,
        );
        Ok(())
    }
    /// Bounded nonblocking work; the UI calls regularly, including while paused.
    pub fn poll(&mut self) -> Vec<String> {
        if self
            .release_at
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.close_transport();
        }
        let Some(session) = &mut self.session else {
            return vec![];
        };
        let mut lines = Vec::new();
        let mut changed = false;
        for event in session.poll() {
            match event {
                Event::Started => lines.push("All members agreed. Synthetic encounter running; /say !attack to attack target #1.".into()),
                Event::Committed(tick) => { changed |= !tick.events.is_empty(); },
                // Retain transport for one barrier window: dropping now could cancel the
                // final vote still queued to another peer. This is not atomic persistence.
                Event::Finished => { lines.push("Encounter tick limit reached. Final confirmed result follows; restart with a fresh epoch.".into()); changed = true; self.closed = true; self.release_at = Some(Instant::now() + TIMEOUT); },
                Event::Aborted(reason) => { lines.push(format!("Encounter aborted: {reason}. Unconfirmed results discarded; restart with a fresh epoch.")); changed = true; self.closed = true; }
            }
        }
        if !self.closed
            && !self.paused
            && session.status() == &Status::Running
            && self.submitted != Some(session.world().tick())
            && self.last_submit.elapsed() >= TICK
        {
            let tick = session.world().tick();
            let command = if self.attack {
                Command::Attack(TARGET)
            } else {
                Command::StopAttack
            };
            match session.submit(vec![command]) {
                Ok(()) => {
                    self.submitted = Some(tick);
                    self.last_submit = Instant::now();
                }
                Err(error) => {
                    session.stop();
                    self.closed = true;
                    lines.push(format!(
                        "Encounter submission failed: {error:?}. Restart with a fresh epoch."
                    ));
                }
            }
        }
        if changed {
            lines.extend(self.report());
        }
        if self.closed && self.release_at.is_none() {
            self.close_transport();
        }
        lines
    }
    fn close_transport(&mut self) {
        self.release_at = None;
        if let Some(session) = self.session.take() {
            self.terminal = Some((session.world().clone(), session.status().clone()));
            // Drop shuts down the transport task; retain only confirmed display data.
        }
    }
    pub fn report(&self) -> Vec<String> {
        let Some(world) = self.world() else {
            return vec!["Lobby: not ready. Review roster, then /say !ready.".into()];
        };
        let mut lines = vec![format!("TEST confirmed tick {} | {:?} | local paused={} | last submitted={:?} | missing inputs={}", world.tick(), self.status().unwrap(), self.paused, self.submitted, self.session.as_ref().map_or(0, |s| s.missing_inputs().len()))];
        for (id, p) in world.players() {
            lines.push(format!(
                "TEST player {id}: HP {}/{}; XP {}; test tokens {} (not WoW items)",
                p.health,
                p.spec.max_health,
                p.xp,
                p.inventory.get(&TOKEN).copied().unwrap_or(0)
            ));
        }
        let target = &world.creatures()[&TARGET];
        lines.push(format!(
            "TEST target #1: HP {}/{}; generation {}",
            target.health, target.spec.max_health, target.generation
        ));
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expired_drain_deadline_releases_listener_and_retains_confirmed_snapshot() {
        let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = reservation.local_addr().unwrap();
        let text = format!(
            r#"
[p2p]
psk = "public drain window regression phrase"
listen = "{addr}"
[p2p.encounter]
local_id = "01010101010101010101010101010101"
epoch = "0101010101010101010101010101010101010101010101010101010101010101"
roster = [{{id="01010101010101010101010101010101", address="{addr}"}}, {{id="02020202020202020202020202020202", address="127.0.0.1:65534"}}]
"#
        );
        let cfg = benilla_config::AppConfig::parse(&text, &std::env::temp_dir()).unwrap();
        let mut c = Controller::new(cfg.p2p.unwrap(), cfg.ruleset).unwrap();
        drop(reservation);
        c.start().unwrap();
        let confirmed = c.world().unwrap().state_hash();
        c.closed = true;
        // Inject expiry instead of sleeping for the production 60-second window.
        c.release_at = Some(Instant::now());
        c.poll();
        assert!(c.session.is_none());
        assert_eq!(c.world().unwrap().state_hash(), confirmed);
        let _released = std::net::TcpListener::bind(addr).unwrap();
    }
}
