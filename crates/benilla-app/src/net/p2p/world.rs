// SPDX-License-Identifier: GPL-3.0-or-later
//! App-owned solo tick, intent and checkpoint boundary. Native entry remains fail-closed:
//! a validated import is not a supported mechanics profile, and fixtures are test-only.
//! This module never writes the selected profile, contains keys, or starts a peer quorum.
use std::{
    collections::VecDeque,
    fmt,
    path::{Path, PathBuf},
    time::Duration,
};

use benilla_profile::{Character, ProfileStore};
use benilla_protocol::SessionEvent;
use benilla_world_client::{ClientCatalog, NpcHandle, Projection, UiRequest};
use benilla_world_data::{Source, WorldData};
use benilla_world_runtime::{self as runtime, PlayerId, Rules, World, WorldId};
use benilla_world_store::{Durability, WorldIdentity, WorldStore};

#[path = "world/commands.rs"]
mod commands;
#[path = "world/driver.rs"]
mod driver;
#[path = "world/entry.rs"]
mod entry;
#[path = "world/native_controller.rs"]
mod native_controller;
pub(super) use driver::{run, Control, Driven, Exit};
pub(super) use entry::{dispatch, Request};
#[cfg(test)]
#[allow(unused_imports)]
// The actual Service fixture is intentionally absent from the headless harness.
pub(crate) use native_controller::tests::distinct_quest_loop_tests::loaded;
#[cfg(all(test, not(benilla_headless_geometry)))]
pub(crate) use native_controller::tests::resident_fixture;

const CHECKPOINT_PERIOD: Duration = Duration::from_secs(10);
const MAX_ACTIONS: usize = 64;
const MAX_TICKS_PER_PUMP: usize = 32;

#[derive(Debug)]
pub(crate) enum Error {
    WrongOwner,
    WrongCharacter,
    WrongRoster,
    FixtureNotPlayable,
    NativeMechanicsUnavailable,
    InvalidWorld,
    InvalidMovement,
    UnsupportedCommand,
    UnsupportedDropRate,
    Backpressure,
    Paused,
    Closed,
    ClockOverflow,
    Profile(benilla_profile::Error),
    Client(benilla_world_client::Error),
    Runtime(runtime::Error),
    NativeRuntime(runtime::native::NativeError),
    NativeStore(benilla_world_store::native::Error),
    InvalidMovementAck,
    Store(benilla_world_store::Error),
    Io(std::io::Error),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Profile(e) => e.fmt(f),
            Self::Client(e) => e.fmt(f),
            Self::Runtime(e) => e.fmt(f),
            Self::NativeRuntime(e) => e.fmt(f),
            Self::NativeStore(e) => e.fmt(f),
            Self::Store(e) => e.fmt(f),
            Self::Io(e) => e.fmt(f),
            Self::NativeMechanicsUnavailable => f.write_str(
                "native starter mechanics are not yet supported; no world was opened or created",
            ),
            Self::FixtureNotPlayable => {
                f.write_str("original test content cannot be opened as a playable Classic world")
            }
            Self::Backpressure => {
                f.write_str("world input queue is full; this action was not accepted")
            }
            Self::Paused => f.write_str(
                "world is paused; resolve its checkpoint, projection or commit acknowledgement error before continuing",
            ),
            _ => write!(f, "solo world: {self:?}"),
        }
    }
}
impl std::error::Error for Error {}
macro_rules! convert_error {
    ($source:ty, $variant:ident) => {
        impl From<$source> for Error {
            fn from(e: $source) -> Self {
                Self::$variant(e)
            }
        }
    };
}
convert_error!(benilla_profile::Error, Profile);
convert_error!(benilla_world_client::Error, Client);
convert_error!(runtime::Error, Runtime);
convert_error!(runtime::native::NativeError, NativeRuntime);
convert_error!(benilla_world_store::native::Error, NativeStore);
convert_error!(benilla_world_store::Error, Store);
convert_error!(std::io::Error, Io);

/// Constructed only from the already-open profile. Neither a GUI nor a remote envelope can
/// assert the owner, select an absent character, or pass a signing key into the simulation.
#[derive(Clone)]
pub(crate) struct Selection {
    player: PlayerId,
    facing: f32,
}
impl Selection {
    pub(crate) fn from_profile(store: &ProfileStore, character: &Character) -> Result<Self, Error> {
        if store.owner_id() != character.owner_id {
            return Err(Error::WrongOwner);
        }
        character.validate()?;
        if store.selected_character() != Some(character)
            || store.character(character.id) != Some(character)
        {
            return Err(Error::WrongCharacter);
        }
        Ok(Self {
            player: PlayerId {
                owner: store.owner_id(),
                character: character.id,
            },
            facing: character.position.orientation,
        })
    }
}

/// Create and Open are distinct user decisions. Open never falls back to Create.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "No supported native compiler entry producer exists yet"
    )
)]
pub(crate) enum Entry {
    Create { world: WorldId, seed: u64 },
    Open { world: WorldId },
}
impl Entry {
    fn id(self) -> WorldId {
        match self {
            Self::Create { world, .. } | Self::Open { world } => world,
        }
    }
}

/// Integer configuration mapping. The current guaranteed-loot model supports only rate 1;
/// multiplying quantity is not a faithful implementation of a drop-chance multiplier.
pub(crate) fn apply_rates(
    mut rules: Rules,
    rates: benilla_config::Ruleset,
) -> Result<Rules, Error> {
    if rates.drop_milli() != 1000 {
        return Err(Error::UnsupportedDropRate);
    }
    rules.xp_rate_numerator = rates.xp_milli();
    rules.xp_rate_denominator = 1000;
    Ok(rules)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SavedBoundary {
    pub tick: u64,
    pub generation: u64,
    pub durability: Durability,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PauseKind {
    Checkpoint { requires_reopen: bool },
    Simulation,
    Authority,
    CommittedAwaitingRouteAck,
    Projection,
    Clock,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Pause {
    pub kind: PauseKind,
    pub message: String,
}
/// Events from already committed ticks are retained even when a later checkpoint fails.
#[derive(Default)]
pub(crate) struct Pump {
    pub events: Vec<SessionEvent>,
    pub ticks: usize,
    pub pending_ticks: u128,
    pub saved: Option<SavedBoundary>,
    pub pause: Option<Pause>,
}

pub(crate) struct Controller {
    world: World,
    projection: Projection,
    store: WorldStore,
    selected: Selection,
    actions: VecDeque<UiRequest>,
    desired_pose: Option<[f32; 3]>,
    autoattack: Option<NpcHandle>,
    announced_attack: Option<(NpcHandle, u64)>,
    next_lane: usize,
    debt: Duration,
    saved_tick: u64,
    pause: Option<Pause>,
    closing: bool,
    closed: bool,
}
impl Controller {
    /// The sole production constructor. The exact profile owner is checked before any world
    /// filesystem operation. Native preparation reports cannot be coerced into this model.
    pub(crate) fn enter(
        local_state: &Path,
        profile: &ProfileStore,
        character: &Character,
        entry: Entry,
        data: WorldData,
        rules: Rules,
        catalog: &impl ClientCatalog,
    ) -> Result<Self, Error> {
        let selected = Selection::from_profile(profile, character)?;
        if entry.id() == [0; 16] {
            return Err(Error::InvalidWorld);
        }
        if matches!(data.export().source, Source::OriginalFixture { .. }) {
            return Err(Error::FixtureNotPlayable);
        }
        require_native_profile(&data)?;
        Self::enter_validated(local_state, selected, entry, data, rules, catalog)
    }

    // Private implementation seam for a future opaque, supported native profile. Tests alone
    // supply authored fixture bindings; there is no public fixture or trust-Classic switch.
    fn enter_validated(
        local_state: &Path,
        selected: Selection,
        entry: Entry,
        data: WorldData,
        rules: Rules,
        catalog: &impl ClientCatalog,
    ) -> Result<Self, Error> {
        let path = world_path(local_state, entry.id())?;
        match entry {
            Entry::Create { world: id, seed } => {
                let mut world = World::new(data, rules, id, seed, vec![selected.player])?;
                let projection = validated_projection(&world, &selected, catalog)?;
                // Catalog/roster/runtime checks all precede even creating the world container.
                match std::fs::create_dir(path.parent().expect("fixed world container")) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(e) => return Err(e.into()),
                }
                let (store, receipt) = WorldStore::create(path, &world)?;
                receipt.acknowledge(&mut world)?;
                Ok(Self::from_parts(world, projection, store, selected))
            }
            Entry::Open { world: id } => {
                let identity = WorldIdentity {
                    world: id,
                    content_digest: data.content_digest(),
                    rules_digest: rules.digest(),
                };
                let (store, world) = WorldStore::open(path, identity, data, rules)?;
                // This is a solo controller. Every restored roster entry must be this selection;
                // an otherwise-valid foreign/extra owner cannot ride an opened snapshot.
                let projection = validated_projection(&world, &selected, catalog)?;
                Ok(Self::from_parts(world, projection, store, selected))
            }
        }
    }
    fn from_parts(
        world: World,
        projection: Projection,
        store: WorldStore,
        selected: Selection,
    ) -> Self {
        let saved_tick = world.state().tick;
        Self {
            world,
            projection,
            store,
            selected,
            actions: VecDeque::new(),
            desired_pose: None,
            autoattack: None,
            announced_attack: None,
            next_lane: 0,
            debt: Duration::ZERO,
            saved_tick,
            pause: None,
            closing: false,
            closed: false,
        }
    }
    pub(crate) fn initial_events(&self) -> Result<Vec<SessionEvent>, Error> {
        Ok(self.projection.initial_events(&self.world)?)
    }
    /// Overlay the selected avatar before its ObjectCreate is emitted, so first-frame health,
    /// level, XP, money, inventory and quests come from this world rather than profile baselines.
    pub(crate) fn initial_player_fields(&self) -> Result<benilla_protocol::ObjectFields, Error> {
        self.initial_events()?
            .into_iter()
            .find_map(|event| match event {
                SessionEvent::ObjectValues { guid, fields } if guid == self.player_guid() => {
                    Some(fields)
                }
                _ => None,
            })
            .ok_or(Error::WrongCharacter)
    }
    pub(crate) fn selected_player(&self) -> PlayerId {
        self.selected.player
    }
    pub(crate) fn position(&self) -> runtime::Position {
        self.world
            .player(self.selected.player)
            .expect("validated selected player")
            .position
    }
    pub(crate) fn facing(&self) -> f32 {
        self.selected.facing
    }
    pub(crate) fn pause(&self) -> Option<&Pause> {
        self.pause.as_ref()
    }
    #[cfg(test)]
    pub(crate) fn is_closed(&self) -> bool {
        self.closed
    }
    pub(crate) fn tick_duration(&self) -> Duration {
        Duration::from_millis(u64::from(self.world.rules().tick_ms))
    }

    fn ensure_input(&self) -> Result<(), Error> {
        if self.closed || self.closing {
            return Err(Error::Closed);
        }
        if self.pause.is_some() {
            return Err(Error::Paused);
        }
        Ok(())
    }
    fn queue(&mut self, request: UiRequest) -> Result<(), Error> {
        if self.actions.len() == MAX_ACTIONS {
            return Err(Error::Backpressure);
        }
        self.actions.push_back(request);
        Ok(())
    }
    fn movement(&mut self, position: [f32; 3], facing: f32) -> Result<(), Error> {
        if !facing.is_finite()
            || facing.abs() > 100.0
            || position
                .iter()
                .any(|v| !v.is_finite() || v.abs() > runtime::MAX_COORD as f32 / 1000.0)
        {
            return Err(Error::InvalidMovement);
        }
        // Presentation stays floating until a simulation tick admits this latest target.
        self.desired_pose = Some(position);
        self.selected.facing = facing;
        Ok(())
    }
    fn correct_blocked_movement(&mut self, events: &mut Vec<SessionEvent>) {
        let player = self
            .world
            .player(self.selected.player)
            .expect("validated selected player");
        if self.desired_pose.is_some() && (player.health == 0 || player.casting.is_some()) {
            // The app predicts movement even during casting. The original runtime refuses it;
            // discard the prediction rather than replaying it after the blocking state ends.
            self.desired_pose = None;
            events.push(self.correction());
            events.push(SessionEvent::Notification {
                text: "Movement rejected while casting or dead; position corrected.".into(),
            });
        }
    }
    fn move_request(&mut self) -> Option<UiRequest> {
        let desired = self.desired_pose?;
        let player = self
            .world
            .player(self.selected.player)
            .expect("validated selected player");
        if player.health == 0 || player.casting.is_some() {
            return None;
        }
        let target = desired.map(|v| (f64::from(v) * 1000.0).round() as i32);
        let delta = std::array::from_fn::<_, 3, _>(|i| {
            i64::from(target[i]) - i64::from(player.position.xyz[i])
        });
        let delta = bounded_delta(delta, self.world.rules().movement_per_tick);
        if delta == [0; 3] {
            self.desired_pose = None;
            return None;
        }
        Some(UiRequest::MoveDelta(delta))
    }
    fn melee_request(&self) -> Option<UiRequest> {
        let target = self.autoattack?;
        let player = self
            .world
            .player(self.selected.player)
            .expect("validated selected player");
        (player.health > 0
            && player.casting.is_none()
            && player.melee_ready_at <= self.world.state().tick + 1)
            .then_some(UiRequest::MeleeOnce(target))
    }
    fn next_request(&mut self) -> Option<UiRequest> {
        // Fair deterministic lanes: an action flood cannot permanently starve movement/melee.
        for offset in 0..3 {
            let lane = (self.next_lane + offset) % 3;
            let request = match lane {
                0 => self.actions.pop_front(),
                1 => self.move_request(),
                _ => self.melee_request(),
            };
            if request.is_some() {
                self.next_lane = (lane + 1) % 3;
                return request;
            }
        }
        None
    }
    fn stop_attack(&mut self, events: &mut Vec<SessionEvent>) {
        self.autoattack = None;
        if let Some((_, victim)) = self.announced_attack.take() {
            events.push(SessionEvent::AttackStop {
                attacker: self.player_guid(),
                victim,
            });
        }
    }
    fn player_guid(&self) -> u64 {
        self.projection
            .player_guid(self.selected.player)
            .expect("validated selected player")
    }
    fn correction(&self) -> SessionEvent {
        SessionEvent::UnitMove {
            guid: self.player_guid(),
            position: self.position().xyz.map(|v| v as f32 / 1000.0),
            orientation: self.selected.facing,
            flags: 0,
            pitch: 0.0,
            time: (self.world.state().tick * u64::from(self.world.rules().tick_ms)) as u32,
            verb: benilla_protocol::messages::RelayVerb::Pose,
            fall_time: 0,
            jump: None,
            transport: None,
        }
    }
    fn advance_one(&mut self, events: &mut Vec<SessionEvent>) -> Result<(), Error> {
        if self.autoattack.is_some_and(|target| {
            self.world
                .player(self.selected.player)
                .is_none_or(|p| p.health == 0)
                || self
                    .world
                    .npc(target.id.spawn)
                    .is_none_or(|n| n.id != target.id || n.health == 0)
        }) {
            self.stop_attack(events);
        }
        self.correct_blocked_movement(events);
        let request = self.next_request();
        let moving = matches!(request, Some(UiRequest::MoveDelta(_)));
        let melee = match request {
            Some(UiRequest::MeleeOnce(target)) => Some(target),
            _ => None,
        };
        let input = request.and_then(|request| {
            match self.projection.commands().local(&self.world, request) {
                Ok(input) => Some(input),
                // A stale captured window/target is an input rejection, never a reason to freeze
                // unrelated empty ticks, casts, respawns or subsequent valid actions.
                Err(error) => {
                    events.push(SessionEvent::Notification {
                        text: error.to_string(),
                    });
                    None
                }
            }
        });
        let result = self
            .world
            .step(self.world.state().tick + 1, input.into_iter().collect())?;
        // Project before checkpoint acknowledgement, which clears last_tick().
        let projected = self.projection.tick(&self.world);
        if let Err(error) = projected {
            self.pause = Some(Pause {
                kind: PauseKind::Projection,
                message: error.to_string(),
            });
            return Err(error.into());
        }
        events.extend(projected.expect("checked projection result"));
        if let Some(target) = melee {
            if result
                .outcomes
                .first()
                .is_some_and(|o| o.rejection.is_none())
            {
                if self
                    .announced_attack
                    .is_none_or(|(announced, _)| announced != target)
                {
                    if let Some(victim) = self.projection.npc_guid(target.id) {
                        events.push(SessionEvent::AttackStart {
                            attacker: self.player_guid(),
                            victim,
                        });
                        self.announced_attack = Some((target, victim));
                    }
                }
            } else {
                self.stop_attack(events);
            }
        }
        if moving {
            events.push(self.correction());
        }
        self.correct_blocked_movement(events);
        if self.autoattack.is_some_and(|target| {
            self.world
                .player(self.selected.player)
                .is_none_or(|p| p.health == 0)
                || self
                    .world
                    .npc(target.id.spawn)
                    .is_none_or(|n| n.id != target.id || n.health == 0)
        }) {
            self.stop_attack(events);
        }
        Ok(())
    }
    fn save_due(&self) -> bool {
        let ticks = self.world.state().tick - self.saved_tick;
        u128::from(ticks) * u128::from(self.world.rules().tick_ms) >= CHECKPOINT_PERIOD.as_millis()
            || self.world.checkpoint_due()
    }
    fn checkpoint(&mut self) -> Result<SavedBoundary, Error> {
        match self.store.checkpoint(&mut self.world) {
            Ok(receipt) => {
                self.saved_tick = self.world.state().tick;
                Ok(SavedBoundary {
                    tick: self.saved_tick,
                    generation: receipt.generation(),
                    durability: receipt.durability(),
                })
            }
            Err(error) => {
                self.pause = Some(Pause {
                    kind: PauseKind::Checkpoint {
                        requires_reopen: self.store.requires_reopen(),
                    },
                    message: error.to_string(),
                });
                Err(error.into())
            }
        }
    }
    /// Wall time determines only how many fixed ticks become due. Catch-up yields after a
    /// bounded batch and retains all debt; command arrival never creates extra game ticks.
    /// Paused intervals add no debt, and no mutation proceeds past a failed durable boundary.
    pub(crate) fn pump(&mut self, elapsed: Duration) -> Pump {
        let mut out = Pump::default();
        if !self.closed && !self.closing && self.pause.is_none() {
            if let Some(debt) = self.debt.checked_add(elapsed) {
                self.debt = debt;
            } else {
                self.pause = Some(Pause {
                    kind: PauseKind::Clock,
                    message: Error::ClockOverflow.to_string(),
                });
            }
            while self.pause.is_none()
                && out.ticks < MAX_TICKS_PER_PUMP
                && self.debt >= self.tick_duration()
            {
                if self.save_due() {
                    match self.checkpoint() {
                        Ok(saved) => out.saved = Some(saved),
                        Err(_) => break,
                    }
                }
                let before = self.world.state().tick;
                let result = self.advance_one(&mut out.events);
                if self.world.state().tick != before {
                    self.debt -= self.tick_duration();
                    out.ticks += 1;
                }
                if let Err(error) = result {
                    if self.pause.is_none() {
                        self.pause = Some(Pause {
                            kind: PauseKind::Simulation,
                            message: error.to_string(),
                        });
                    }
                    break;
                }
                if self.save_due() {
                    match self.checkpoint() {
                        Ok(saved) => out.saved = Some(saved),
                        Err(_) => break,
                    }
                }
            }
        }
        out.pending_ticks = self.debt.as_nanos() / self.tick_duration().as_nanos();
        out.pause = self.pause.clone();
        out
    }
    /// Explicit retry preserves world, command queue and journal. Ambiguous publication stays
    /// blocked until a separate explicit recovery/reopen; never replace the live world here.
    pub(crate) fn retry_checkpoint(&mut self) -> Result<SavedBoundary, Error> {
        if !matches!(
            self.pause.as_ref().map(|p| &p.kind),
            Some(PauseKind::Checkpoint {
                requires_reopen: false
            })
        ) {
            return Err(Error::Paused);
        }
        let saved = self.checkpoint()?;
        self.pause = None;
        Ok(saved)
    }
    /// Logout cancels unadmitted intentions, never advances ticks just to consume an input
    /// burst. Every committed tick is saved synchronously; return success only after receipt.
    /// On failure this controller remains alive and paused for inspection/retry.
    pub(crate) fn shutdown(&mut self) -> Result<(SavedBoundary, usize, Vec<SessionEvent>), Error> {
        if self.closed {
            return Err(Error::Closed);
        }
        self.closing = true;
        let cancelled = self.actions.len() + usize::from(self.desired_pose.is_some());
        let saved = self.checkpoint()?;
        let mut events = Vec::new();
        self.stop_attack(&mut events);
        self.actions.clear();
        self.desired_pose = None;
        self.closed = true;
        Ok((saved, cancelled, events))
    }
}
fn require_native_profile(_data: &WorldData) -> Result<(), Error> {
    // No named native mechanics profile exists yet. Do not remove this independently of
    // the compiler's field-disposition proof, runtime gate and exact client bindings.
    Err(Error::NativeMechanicsUnavailable)
}
fn validated_projection(
    world: &World,
    selected: &Selection,
    catalog: &impl ClientCatalog,
) -> Result<Projection, Error> {
    if world.state().players.len() != 1 || world.state().players[0].id != selected.player {
        return Err(Error::WrongRoster);
    }
    let projection = Projection::new(world, selected.player, catalog)?;
    projection.initial_events(world)?;
    Ok(projection)
}
fn world_path(root: &Path, world: WorldId) -> Result<PathBuf, Error> {
    if world == [0; 16] {
        return Err(Error::InvalidWorld);
    }
    let name: String = world.iter().map(|byte| format!("{byte:02x}")).collect();
    Ok(root.join("p2p-worlds").join(name))
}
/// Integer ceil-sqrt keeps a diagonal within the runtime's Euclidean speed bound. Floating
/// presentation is quantized before this function; no host floating point enters a command.
fn bounded_delta(delta: [i64; 3], limit: u32) -> [i32; 3] {
    let square: u64 = delta.iter().map(|v| v.unsigned_abs().pow(2)).sum();
    let norm = square.isqrt();
    let norm = norm + u64::from(norm * norm < square);
    if norm <= u64::from(limit) {
        return delta.map(|v| v as i32);
    }
    let mut bounded = delta.map(|v| (v * i64::from(limit) / norm as i64) as i32);
    if bounded == [0; 3] {
        let axis = (0..3).max_by_key(|&i| delta[i].unsigned_abs()).unwrap();
        bounded[axis] = delta[axis].signum() as i32;
    }
    bounded
}

#[cfg(test)]
#[path = "world/tests.rs"]
mod tests;
