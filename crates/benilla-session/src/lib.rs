// SPDX-License-Identifier: GPL-3.0-or-later
//! Trusted-group, full-mesh lockstep for the synthetic encounter, not WoW gameplay.
//! Every member proposes the identical frozen manifest and acknowledges it before
//! tick zero. Every tick requires every input and every matching input/state vote.
//! No leader, implicit roster shrink, reconnect resume, rollback, or catch-up.
//! PSK membership is not individual identity or Byzantine consensus/anti-cheat.
#![forbid(unsafe_code)]

use benilla_p2p::{Command as TransportCommand, Event as TransportEvent, Network, PeerId};
use benilla_sim::encounter::{self as sim, CommittedTick, InputFrame, World};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::mpsc::TryRecvError,
    time::{Duration, Instant},
};

pub const SESSION_PROTOCOL: u32 = 1;
pub const MAX_PEERS: usize = 16;
pub const MAX_TICKS: u64 = 100_000;

/// Single-dial mesh policy: higher IDs dial lower IDs; both still listen.
/// Apply when building TransportConfig. Symmetric dial replacement can emit a
/// PeerLeft event, which intentionally aborts even an unstarted frozen session.
pub fn should_dial_pair(local: PeerId, remote: PeerId) -> bool {
    local > remote
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Member {
    pub transport: PeerId,
    pub simulation: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub protocol: u32,
    pub simulation_version: u32,
    /// Fresh nonzero session nonce agreed out of band. Never reuse for reconnect.
    pub epoch: [u8; 32],
    /// Strictly increasing transport IDs, unique simulation IDs.
    pub roster: Vec<Member>,
    pub rules_hash: [u8; 32],
    pub content_hash: [u8; 32],
    pub initial_state_hash: [u8; 32],
    pub tick_limit: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Command {
    Attack(u64),
    StopAttack,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
enum Wire {
    Propose(Manifest),
    Ready {
        epoch: [u8; 32],
        manifest: [u8; 32],
    },
    Input {
        epoch: [u8; 32],
        tick: u64,
        commands: Vec<Command>,
    },
    Vote {
        epoch: [u8; 32],
        tick: u64,
        inputs: [u8; 32],
        state: [u8; 32],
    },
    Abort {
        epoch: [u8; 32],
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidManifest,
    InvalidTimeout,
    NotRunning,
    DuplicateInput,
    TooManyCommands,
    InvalidCommand,
    Aborted(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Started,
    Committed(CommittedTick),
    Finished,
    Aborted(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Handshaking,
    Running,
    Finished,
    Aborted(String),
}
#[derive(Default)]
struct Round {
    inputs: BTreeMap<PeerId, Vec<Command>>,
    votes: BTreeMap<PeerId, ([u8; 32], [u8; 32])>,
}

pub struct Session {
    network: Network,
    local: PeerId,
    manifest: Manifest,
    digest: [u8; 32],
    world: World,
    connected: BTreeSet<PeerId>,
    proposals: BTreeSet<PeerId>,
    ready: BTreeSet<PeerId>,
    sent_ready: bool,
    status: Status,
    rounds: BTreeMap<u64, Round>,
    staged: Option<(World, CommittedTick)>,
    timeout: Duration,
    deadline: Instant,
    events: Vec<Event>,
}
impl Session {
    /// Call before consuming any Network events. Transport Hello must use an
    /// encounter-specific network/content label, separate from exploration traffic.
    /// World must be a fresh tick-zero world with no pending inputs.
    pub fn new(
        network: Network,
        local: PeerId,
        manifest: Manifest,
        world: World,
        timeout: Duration,
    ) -> Result<Self, Error> {
        if timeout.is_zero() || timeout > Duration::from_secs(60) {
            return Err(Error::InvalidTimeout);
        }
        let roster = &manifest.roster;
        let ids: BTreeSet<_> = roster.iter().map(|m| m.simulation).collect();
        if manifest.protocol != SESSION_PROTOCOL
            || manifest.simulation_version != sim::SIMULATION_VERSION
            || manifest.epoch == [0; 32]
            || roster.len() < 2
            || roster.len() > MAX_PEERS
            || roster.iter().any(|m| m.transport == [0; 16])
            || roster.windows(2).any(|w| w[0].transport >= w[1].transport)
            || ids.len() != roster.len()
            || ids != world.players().keys().copied().collect()
            || !roster.iter().any(|m| m.transport == local)
            || world.tick() != 0
            || world.snapshot().is_err()
            || manifest.initial_state_hash != world.state_hash()
            || manifest.tick_limit == 0
            || manifest.tick_limit > MAX_TICKS
        {
            return Err(Error::InvalidManifest);
        }
        let digest = hash(&manifest);
        Ok(Self {
            network,
            local,
            manifest,
            digest,
            world,
            connected: BTreeSet::new(),
            proposals: BTreeSet::from([local]),
            ready: BTreeSet::new(),
            sent_ready: false,
            status: Status::Handshaking,
            rounds: BTreeMap::new(),
            staged: None,
            timeout,
            deadline: Instant::now() + timeout,
            events: vec![],
        })
    }
    pub fn status(&self) -> &Status {
        &self.status
    }
    /// Only all-peer-confirmed state is exposed; staged rewards remain private.
    pub fn world(&self) -> &World {
        &self.world
    }
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
    pub fn missing_inputs(&self) -> Vec<PeerId> {
        self.manifest
            .roster
            .iter()
            .filter(|m| {
                !self
                    .rounds
                    .get(&self.world.tick())
                    .is_some_and(|r| r.inputs.contains_key(&m.transport))
            })
            .map(|m| m.transport)
            .collect()
    }
    pub fn submit(&mut self, commands: Vec<Command>) -> Result<(), Error> {
        if self.status != Status::Running {
            return Err(Error::NotRunning);
        }
        if commands.len() > sim::MAX_COMMANDS_PER_FRAME {
            return Err(Error::TooManyCommands);
        }
        if commands
            .iter()
            .any(|c| matches!(c, Command::Attack(id) if !self.world.creatures().contains_key(id)))
        {
            return Err(Error::InvalidCommand);
        }
        let tick = self.world.tick();
        let round = self.rounds.entry(tick).or_default();
        if round.inputs.contains_key(&self.local) {
            return Err(Error::DuplicateInput);
        }
        round.inputs.insert(self.local, commands.clone());
        self.broadcast(Wire::Input {
            epoch: self.manifest.epoch,
            tick,
            commands,
        });
        self.progress();
        if let Status::Aborted(reason) = &self.status {
            Err(Error::Aborted(reason.clone()))
        } else {
            Ok(())
        }
    }
    /// Nonblocking bounded work. Call regularly; a missing member/input/vote stops
    /// progress immediately and aborts after the finite per-round timeout.
    pub fn poll(&mut self) -> Vec<Event> {
        if matches!(self.status, Status::Handshaking | Status::Running) {
            for _ in 0..256 {
                match self.network.try_recv() {
                    Ok(event) => self.receive(event),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.abort("transport closed");
                        break;
                    }
                }
                self.progress();
                if matches!(self.status, Status::Aborted(_) | Status::Finished) {
                    break;
                }
            }
            if Instant::now() >= self.deadline
                && matches!(self.status, Status::Handshaking | Status::Running)
            {
                self.abort("all-peer barrier timed out");
            }
        }
        std::mem::take(&mut self.events)
    }
    pub fn stop(&mut self) {
        self.abort("locally stopped");
    }
    fn member(&self, id: PeerId) -> bool {
        self.manifest.roster.iter().any(|m| m.transport == id)
    }
    fn broadcast(&mut self, wire: Wire) {
        let bytes = serde_json::to_vec(&wire).expect("bounded wire serializes");
        if self
            .network
            .try_send(TransportCommand::Broadcast(bytes))
            .is_err()
        {
            self.abort("transport backpressure or closure");
        }
    }
    fn abort(&mut self, reason: &str) {
        if matches!(self.status, Status::Aborted(_) | Status::Finished) {
            return;
        }
        self.status = Status::Aborted(reason.into());
        self.events.push(Event::Aborted(reason.into()));
        self.staged = None;
        self.rounds.clear();
        if let Ok(payload) = serde_json::to_vec(&Wire::Abort {
            epoch: self.manifest.epoch,
        }) {
            let _ = self.network.try_send(TransportCommand::Broadcast(payload));
        }
    }
    fn receive(&mut self, event: TransportEvent) {
        match event {
            TransportEvent::Rejected { .. } => (),
            TransportEvent::PeerLeft(id) if self.member(id) => {
                self.abort("frozen member disconnected; fresh epoch required")
            }
            TransportEvent::PeerJoined(hello) if self.member(hello.peer_id) => {
                if !self.connected.insert(hello.peer_id) {
                    self.abort("member reconnected; fresh epoch required");
                    return;
                }
                let payload = serde_json::to_vec(&Wire::Propose(self.manifest.clone())).unwrap();
                if self
                    .network
                    .try_send(TransportCommand::Send {
                        peer_id: hello.peer_id,
                        payload,
                    })
                    .is_err()
                {
                    self.abort("proposal send failed");
                }
            }
            TransportEvent::Frame { peer_id, payload } if self.member(peer_id) => {
                if !self.connected.contains(&peer_id)
                    || payload.len() > benilla_p2p::MAX_PAYLOAD_SIZE
                {
                    self.abort("invalid frame source or size");
                    return;
                }
                match serde_json::from_slice::<Wire>(&payload) {
                    Ok(wire) => self.wire(peer_id, wire),
                    Err(_) => self.abort("malformed session frame"),
                }
            }
            _ => (),
        }
    }
    fn wire(&mut self, peer: PeerId, wire: Wire) {
        match wire {
            Wire::Propose(manifest) => {
                if manifest != self.manifest || !self.proposals.insert(peer) {
                    self.abort("mismatched or duplicate start proposal");
                }
            }
            Wire::Ready { epoch, manifest } => {
                if epoch != self.manifest.epoch
                    || manifest != self.digest
                    || !self.proposals.contains(&peer)
                    || !self.ready.insert(peer)
                {
                    self.abort("invalid or duplicate ready");
                }
            }
            Wire::Input {
                epoch,
                tick,
                commands,
            } => {
                if !self.valid_round(peer, epoch, tick) {
                    return;
                }
                if commands.len()>sim::MAX_COMMANDS_PER_FRAME || commands.iter().any(|c|matches!(c,Command::Attack(id) if !self.world.creatures().contains_key(id))) {self.abort("invalid commands");return;}
                if self
                    .rounds
                    .entry(tick)
                    .or_default()
                    .inputs
                    .insert(peer, commands)
                    .is_some()
                {
                    self.abort("duplicate peer input");
                }
            }
            Wire::Vote {
                epoch,
                tick,
                inputs,
                state,
            } => {
                if !self.valid_round(peer, epoch, tick) {
                    return;
                }
                let round = self.rounds.entry(tick).or_default();
                if !round.inputs.contains_key(&peer)
                    || round.votes.insert(peer, (inputs, state)).is_some()
                {
                    self.abort("invalid or duplicate vote");
                }
            }
            Wire::Abort { epoch } => {
                if epoch == self.manifest.epoch {
                    self.abort("peer aborted");
                } else {
                    self.abort("wrong epoch");
                }
            }
        }
    }
    fn valid_round(&mut self, peer: PeerId, epoch: [u8; 32], tick: u64) -> bool {
        if epoch != self.manifest.epoch
            || !self.ready.contains(&peer)
            || tick < self.world.tick()
            || tick > self.world.tick() + 1
            || tick >= self.manifest.tick_limit
        {
            self.abort("wrong epoch, tick, or unready peer");
            false
        } else {
            true
        }
    }
    fn progress(&mut self) {
        if matches!(self.status, Status::Aborted(_) | Status::Finished) {
            return;
        }
        let n = self.manifest.roster.len();
        if self.proposals.len() == n && !self.sent_ready {
            self.sent_ready = true;
            self.ready.insert(self.local);
            self.broadcast(Wire::Ready {
                epoch: self.manifest.epoch,
                manifest: self.digest,
            });
        }
        if self.status == Status::Handshaking && self.ready.len() == n {
            self.status = Status::Running;
            self.deadline = Instant::now() + self.timeout;
            self.events.push(Event::Started);
        }
        if self.status != Status::Running {
            return;
        }
        let tick = self.world.tick();
        if self.staged.is_none() && self.rounds.get(&tick).is_some_and(|r| r.inputs.len() == n) {
            let round = &self.rounds[&tick];
            let inputs = hash(&round.inputs.iter().collect::<Vec<_>>());
            let mut next = self.world.clone();
            for member in &self.manifest.roster {
                let commands = round.inputs[&member.transport]
                    .iter()
                    .map(|c| match c {
                        Command::Attack(id) => sim::Command::Attack(*id),
                        Command::StopAttack => sim::Command::StopAttack,
                    })
                    .collect();
                if next
                    .submit(InputFrame {
                        tick,
                        peer: member.simulation,
                        commands,
                    })
                    .is_err()
                {
                    self.abort("simulation rejected input");
                    return;
                }
            }
            match next.step() {
                Ok(sim::StepOutcome::Advanced(committed)) => {
                    let state = committed.state_hash;
                    self.staged = Some((next, committed));
                    self.rounds
                        .get_mut(&tick)
                        .unwrap()
                        .votes
                        .insert(self.local, (inputs, state));
                    self.broadcast(Wire::Vote {
                        epoch: self.manifest.epoch,
                        tick,
                        inputs,
                        state,
                    });
                }
                _ => {
                    self.abort("simulation failed");
                    return;
                }
            }
        }
        if let Some(round) = self.rounds.get(&tick) {
            if let Some(expected) = round.votes.get(&self.local) {
                if round.votes.values().any(|v| v != expected) {
                    self.abort("input or state hash mismatch");
                    return;
                }
                if round.votes.len() == n {
                    let (world, committed) = self.staged.take().unwrap();
                    self.world = world;
                    self.rounds.remove(&tick);
                    self.deadline = Instant::now() + self.timeout;
                    self.events.push(Event::Committed(committed));
                    if self.world.tick() == self.manifest.tick_limit {
                        self.status = Status::Finished;
                        self.events.push(Event::Finished);
                    }
                }
            }
        }
    }
}
fn hash(value: &impl Serialize) -> [u8; 32] {
    Sha256::digest(serde_json::to_vec(value).expect("canonical ordered value")).into()
}
