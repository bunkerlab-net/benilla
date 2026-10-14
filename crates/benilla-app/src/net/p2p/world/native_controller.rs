// SPDX-License-Identifier: GPL-3.0-or-later
//! App-native clock, durable boundary and local command driver. The production capability
//! is still uninhabited. Component-test fixtures cannot enter through a production request.
use super::*;
use crate::{creature_anim::move_flags, net::ClientCommand};
use benilla_protocol::messages::{MoveMode, RelayVerb};
use benilla_sim::creature_ai::Point;
use benilla_world_client::native::{NativeMovementOrders, NativeProjection};
use benilla_world_runtime::native as n;
#[cfg(test)]
use benilla_world_store::native::Identity;
use benilla_world_store::native::NativeStore;
const _: () = assert!(MAX_ACTIONS <= n::MAX_INPUTS_PER_TICK);
// At most one projected tick is retained. The runtime bounds records and catalog text;
// this additional event-count cap prevents an expanding projection from growing the outbox.
const MAX_PENDING_EVENTS: usize = 65_536;
struct PendingEvents {
    tick: u64,
    events: Vec<SessionEvent>,
    saved: Option<SavedBoundary>,
}

struct LocalAuthority(Box<dyn n::Authority + Send>);
impl n::Authority for LocalAuthority {
    fn resolve(
        &mut self,
        request: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        self.0.resolve(request)
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct RootOrder {
    guid: u64,
    counter: u32,
}
enum CancelTarget {
    Instance(u64),
    Queued(u64),
}
enum Intent {
    Command { ticket: u64, command: n::Command },
    CancelCast { spell_id: u32, target: CancelTarget },
}
struct PendingTick {
    inputs: Vec<n::OwnedInput>,
    moving: bool,
}
/// One forced-order namespace per actual session, independent of saved simulation time.
/// Root is currently its only producer: unsupported speed/mode commands cannot allocate a
/// second namespace. A restored corpse receives a fresh root order and must acknowledge it.
struct MovementOrders {
    counter: NativeMovementOrders,
    pending: Option<RootOrder>,
    acknowledged: Option<RootOrder>,
}
impl MovementOrders {
    #[cfg(test)]
    fn new_session() -> Self {
        Self {
            counter: NativeMovementOrders::from_session_next_counter(0),
            pending: None,
            acknowledged: None,
        }
    }
    fn emitted(&mut self, events: &[SessionEvent]) {
        for event in events {
            if let SessionEvent::MoveMode {
                guid,
                counter,
                mode: MoveMode::Root,
                apply: true,
            } = event
            {
                self.pending = Some(RootOrder {
                    guid: *guid,
                    counter: *counter,
                });
            }
        }
    }
    fn acknowledge(&mut self, order: RootOrder, flags: u32) -> Result<(), Error> {
        // The real mover applies Root, parks its stream and ends the arc before sending ACK
        // (player/wire_in.rs). Turning and other echoed stationary flags are legitimate.
        if flags & move_flags::ROOT == 0
            || flags & (move_flags::ANY_MOVE | move_flags::FALLING | move_flags::FALLING_FAR) != 0
        {
            return Err(Error::InvalidMovementAck);
        }
        if self.pending == Some(order) {
            self.pending = None;
            self.acknowledged = Some(order);
            Ok(())
        } else if self.acknowledged == Some(order) {
            // A duplicate echo is idempotent; it never consumes another order or changes pose.
            Ok(())
        } else {
            Err(Error::InvalidMovementAck)
        }
    }
}

pub(crate) struct NativeController {
    world: n::NativeWorld,
    projection: NativeProjection,
    store: NativeStore,
    selected: Selection,
    authority: LocalAuthority,
    orders: MovementOrders,
    initial: Option<Vec<SessionEvent>>,
    initial_fields: benilla_protocol::ObjectFields,
    actions: VecDeque<Intent>,
    next_ticket: u64,
    pending: Option<PendingTick>,
    outbox: Option<PendingEvents>,
    debt: Duration,
    saved_tick: u64,
    pause: Option<Pause>,
    closing: bool,
    closed: bool,
}
impl NativeController {
    /// No checked report, JSON profile, raw NativeWorld or synthetic feature flag grants
    /// this entry. The compiler must eventually provide an actual admitted constructor.
    pub(super) fn enter(
        _root: &Path,
        profile: &ProfileStore,
        character: &Character,
        _entry: Entry,
        capability: n::NativeCapability,
    ) -> Result<Self, Error> {
        let _selected = Selection::from_profile(profile, character)?;
        match capability {}
    }

    // Kept private and entirely absent from production, including when dependency test
    // features are unified. Tests use only original placeholder catalogs/component fixtures.
    #[cfg(test)]
    fn enter_component_test(
        root: &Path,
        profile: &ProfileStore,
        character: &Character,
        entry: Entry,
        data: n::Profile,
        catalog: &impl ClientCatalog,
        authority: impl n::Authority + Send + 'static,
    ) -> Result<Self, Error> {
        let selected = Selection::from_profile(profile, character)?;
        let path = world_path(root, entry.id())?;
        let (store, world, projection) = match entry {
            Entry::Create { world, seed } => {
                let mut world =
                    n::NativeWorld::synthetic_component_test(data, world, selected.player, seed)?;
                let projection = NativeProjection::new(&world, selected.player, catalog)?;
                // Validate all initial fields before creating even the world container.
                projection.initial_events(&world)?;
                match std::fs::create_dir(path.parent().expect("fixed world container")) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error.into()),
                }
                let (store, receipt) = NativeStore::create(path, &world)?;
                receipt.acknowledge(&mut world)?;
                (store, world, projection)
            }
            Entry::Open { world } => {
                let identity = Identity {
                    world,
                    player: selected.player,
                    profile: data.digest(),
                    domain: n::Domain::SyntheticComponentTest,
                };
                let (store, world) = NativeStore::open(path, identity, |checkpoint| {
                    n::NativeWorld::restore_synthetic_component_test(data.clone(), checkpoint)
                })?;
                let projection = NativeProjection::new(&world, selected.player, catalog)?;
                (store, world, projection)
            }
        };
        let mut orders = MovementOrders::new_session();
        let initial =
            projection.initial_events_with_movement_orders(&world, &mut orders.counter)?;
        orders.emitted(&initial);
        let initial_fields = initial
            .iter()
            .find_map(|event| match event {
                SessionEvent::ObjectValues { guid, fields }
                    if *guid == projection.player_guid() =>
                {
                    Some(fields.clone())
                }
                _ => None,
            })
            .ok_or(Error::WrongCharacter)?;
        let saved_tick = world.state().tick;
        Ok(Self {
            world,
            projection,
            store,
            selected,
            authority: LocalAuthority(Box::new(authority)),
            orders,
            initial: Some(initial),
            initial_fields,
            actions: VecDeque::new(),
            next_ticket: 0,
            pending: None,
            outbox: None,
            debt: Duration::ZERO,
            saved_tick,
            pause: None,
            closing: false,
            closed: false,
        })
    }
    fn ensure_input(&self) -> Result<(), Error> {
        if self.closed || self.closing {
            return Err(Error::Closed);
        }
        if self.pause.is_some() || self.outbox.is_some() {
            return Err(Error::Paused);
        }
        Ok(())
    }
    fn preview_quest(&mut self, giver: NpcHandle, quest: u32, reward: bool) -> Result<(), Error> {
        if self
            .world
            .preview_quest_interaction(giver.id, quest, reward, &mut self.authority)?
            .is_some()
        {
            return Err(benilla_world_client::Error::UnavailableQuest.into());
        }
        Ok(())
    }
    fn tick_duration(&self) -> Duration {
        Duration::from_millis(u64::from(self.world.profile().tick_ms))
    }
    fn queue(&mut self, command: n::Command) -> Result<(), Error> {
        if self.actions.len() == MAX_ACTIONS {
            return Err(Error::Backpressure);
        }
        let ticket = self.next_ticket;
        let next = ticket.checked_add(1).ok_or(n::NativeError::Limit)?;
        self.actions.push_back(Intent::Command { ticket, command });
        self.next_ticket = next;
        Ok(())
    }
    fn queue_cancel(&mut self, spell_id: u32, target: CancelTarget) -> Result<(), Error> {
        if self.actions.len() == MAX_ACTIONS {
            return Err(Error::Backpressure);
        }
        self.actions
            .push_back(Intent::CancelCast { spell_id, target });
        Ok(())
    }
    fn pose(position: [f32; 3], facing: f32) -> Result<Point, Error> {
        if !facing.is_finite() || facing.abs() > 100.0 {
            return Err(Error::InvalidMovement);
        }
        Point::new(position[0], position[1], position[2]).map_err(|_| Error::InvalidMovement)
    }
    fn correction(&self) -> SessionEvent {
        SessionEvent::UnitMove {
            guid: self.player_guid(),
            position: self.position_xyz(),
            orientation: self.selected.facing,
            flags: if self.world.state().health == 0 {
                move_flags::ROOT
            } else {
                0
            },
            pitch: 0.0,
            time: self.world.state().now_ms as u32,
            verb: RelayVerb::Pose,
            fall_time: 0,
            jump: None,
            transport: None,
        }
    }
    fn next_tick(&mut self) -> Result<PendingTick, Error> {
        self.world
            .state()
            .next_sequence
            .checked_add(self.actions.len() as u64)
            .ok_or(n::NativeError::Limit)?;
        // All accepted local actions run in source order before one fixed simulation update.
        // Session-only tickets bind a pre-SpellStart cancel to its queued request; the durable
        // batch records its exact preceding input sequence instead of a predicted cast ID.
        let mut inputs = Vec::with_capacity(self.actions.len());
        let mut cast_sequences = std::collections::BTreeMap::new();
        let mut moving = false;
        for intent in self.actions.drain(..) {
            let sequence = self.world.state().next_sequence + inputs.len() as u64;
            let command = match intent {
                Intent::Command { ticket, command } => {
                    if matches!(command, n::Command::Cast { .. }) {
                        cast_sequences.insert(ticket, sequence);
                    }
                    command
                }
                Intent::CancelCast { spell_id, target } => n::Command::CancelCast {
                    spell_id,
                    target: match target {
                        CancelTarget::Instance(id) => n::CancelCastTarget::Instance(id),
                        CancelTarget::Queued(ticket) => {
                            n::CancelCastTarget::PriorInput(cast_sequences[&ticket])
                        }
                    },
                },
            };
            moving |= matches!(command, n::Command::Move { .. });
            inputs.push(n::OwnedInput {
                authenticated_owner: self.selected.player.owner,
                input: n::Input {
                    world: self.world.state().world,
                    tick: self.world.state().tick + 1,
                    player: self.selected.player,
                    sequence,
                    command,
                },
            });
        }
        Ok(PendingTick { inputs, moving })
    }
    fn advance_one(&mut self, out: &mut Pump) -> Result<(), Error> {
        // A committed but unpublished tick cannot be overtaken by another simulation step.
        if self.outbox.is_some() {
            return Err(Error::Paused);
        }
        if self.pending.is_none() {
            self.pending = Some(self.next_tick()?);
        }
        let inputs = self.pending.as_ref().expect("staged tick").inputs.clone();
        self.world
            .step_batch(self.world.state().tick + 1, inputs, &mut self.authority)?;
        // Immediately retire the committed input. A projection/checkpoint failure must never
        // rerun a committed cast, loot/reward, RNG draw or sequence on explicit retry.
        let pending = self.pending.take().expect("committed staged tick");
        let projected = self
            .projection
            .advance_with_movement_orders(&self.world, &mut self.orders.counter)
            .and_then(|mut projected| {
                if pending.moving {
                    projected.push(self.correction());
                }
                if projected.len() > MAX_PENDING_EVENTS {
                    return Err(benilla_world_client::Error::Unrepresentable);
                }
                Ok(projected)
            });
        let projected = match projected {
            Ok(projected) => projected,
            Err(error) => {
                // No partial projection is published. Shutdown can save this committed world;
                // recovery needs a fresh projection on reopen, never a replay of its inputs.
                self.pause = Some(Pause {
                    kind: PauseKind::Projection,
                    message: error.to_string(),
                });
                return Err(error.into());
            }
        };
        if self.world.profile().quest_progression.is_some() {
            // Progression also mutates without an event (played time, resource/skill sync).
            // Persist every enabled tick, accepting one synchronous checkpoint per tick.
            // Project first: the store receipt clears last_tick and its event journal.
            self.outbox = Some(PendingEvents {
                tick: self.world.state().tick,
                events: projected,
                saved: None,
            });
            out.saved = Some(self.checkpoint()?);
            self.publish_pending(&mut out.events);
        } else {
            self.orders.emitted(&projected);
            out.events.extend(projected);
        }
        Ok(())
    }
    fn publish_pending(&mut self, events: &mut Vec<SessionEvent>) {
        // This is one delivery to the local caller, not an acknowledged network outbox.
        // A crash after persistence is recovered through initial state, not notification replay.
        if self
            .outbox
            .as_ref()
            .is_some_and(|batch| batch.saved.is_some())
        {
            let batch = self.outbox.take().expect("durable pending events");
            self.orders.emitted(&batch.events);
            events.extend(batch.events);
        }
    }
    fn save_due(&self) -> bool {
        u128::from(self.world.state().tick - self.saved_tick)
            * u128::from(self.world.profile().tick_ms)
            >= CHECKPOINT_PERIOD.as_millis()
            || self.world.checkpoint_due()
    }
    fn checkpoint(&mut self) -> Result<SavedBoundary, Error> {
        match self.store.checkpoint(&mut self.world) {
            Ok(receipt) => {
                self.saved_tick = self.world.state().tick;
                let saved = SavedBoundary {
                    tick: self.saved_tick,
                    generation: receipt.generation(),
                    durability: receipt.durability(),
                };
                if let Some(batch) = &mut self.outbox {
                    assert_eq!(
                        batch.tick, saved.tick,
                        "unpublished tick cannot be overtaken"
                    );
                    batch.saved = Some(saved.clone());
                }
                Ok(saved)
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
}
impl Driven for NativeController {
    fn initial_events(&mut self) -> Result<Vec<SessionEvent>, Error> {
        self.initial.take().ok_or(Error::Closed)
    }
    fn initial_player_fields(&self) -> Result<benilla_protocol::ObjectFields, Error> {
        Ok(self.initial_fields.clone())
    }
    fn selected_player(&self) -> PlayerId {
        self.selected.player
    }
    fn map_id(&self) -> u32 {
        self.world.profile().map_id
    }
    fn position_xyz(&self) -> [f32; 3] {
        let p = self.world.state().position;
        [p.x.as_f32(), p.y.as_f32(), p.z.as_f32()]
    }
    fn facing(&self) -> f32 {
        self.selected.facing
    }
    fn player_guid(&self) -> u64 {
        self.projection.player_guid()
    }
    fn tick(&self) -> u64 {
        self.world.state().tick
    }
    fn pause(&self) -> Option<&Pause> {
        self.pause.as_ref()
    }
    fn closing(&self) -> bool {
        self.closing
    }
    fn closed(&self) -> bool {
        self.closed
    }
    fn receive(&mut self, command: ClientCommand) -> Result<commands::Received, Error> {
        if matches!(command, ClientCommand::Logout | ClientCommand::ForceLogout) {
            return Ok(commands::Received {
                events: vec![],
                logout: true,
            });
        }
        // Root acknowledgment is session bookkeeping, also accepted while checkpoint-paused.
        // It never supplies authoritative position, elapsed time, owner identity or a tick.
        if let ClientCommand::MoveModeAck {
            guid,
            counter,
            mode,
            apply,
            flags,
            pos,
            orientation,
        } = command
        {
            if self.closed || guid != self.player_guid() || mode != MoveMode::Root || !apply {
                return Err(Error::InvalidMovementAck);
            }
            Self::pose(pos, orientation)?;
            self.orders
                .acknowledge(RootOrder { guid, counter }, flags)?;
            return Ok(commands::Received {
                events: vec![],
                logout: false,
            });
        }
        self.ensure_input()?;
        // Callback-time quest presentation cannot overtake queued owner input or
        // a retained failed tick. Re-run the request after committing that boundary.
        if matches!(
            &command,
            ClientCommand::QuestQuery { .. }
                | ClientCommand::QuestgiverStatusQuery { .. }
                | ClientCommand::QuestgiverHello { .. }
                | ClientCommand::QuestgiverQuery { .. }
                | ClientCommand::QuestgiverComplete { .. }
                | ClientCommand::QuestgiverRequestReward { .. }
        ) && (!self.actions.is_empty() || self.pending.is_some())
        {
            return Err(Error::Backpressure);
        }
        let mut events = Vec::new();
        match command {
            ClientCommand::Move {
                pos,
                orientation,
                flags,
                pitch,
                jump,
                transport: None,
                ..
            } => {
                let destination = Self::pose(pos, orientation)?;
                let supported_flags = move_flags::ANY_MOVE
                    | move_flags::TURN_LEFT
                    | move_flags::TURN_RIGHT
                    | move_flags::WALK_MODE
                    | move_flags::ROOT;
                if flags & !supported_flags != 0
                    || !pitch.is_finite()
                    || pitch != 0.0
                    || jump.is_some()
                {
                    return Err(Error::InvalidMovement);
                }
                if self.world.state().health == 0
                    || self.orders.pending.is_some()
                    || self.orders.acknowledged.is_some()
                {
                    self.selected.facing = orientation;
                    events.push(self.correction());
                } else {
                    self.queue(n::Command::Move { destination })?;
                    self.selected.facing = orientation;
                }
            }
            ClientCommand::CastSpell {
                spell_id,
                target: Some(guid),
            } => {
                let target = self.projection.capture_npc(guid)?;
                self.queue(n::Command::Cast {
                    ability: spell_id,
                    target: target.id,
                })?;
            }
            ClientCommand::CancelCast { spell_id } => {
                // PendingCast is armed at send, so Esc may arrive before SpellStart. Preserve
                // that queued request's identity even when its eventual prepare is rejected.
                let queued = self.actions.iter().rev().find_map(|intent| match intent {
                    Intent::Command {
                        ticket,
                        command: n::Command::Cast { ability, .. },
                    } if spell_id == 0 || *ability == spell_id => Some(*ticket),
                    _ => None,
                });
                let target = queued.map(CancelTarget::Queued).or_else(|| {
                    self.world
                        .state()
                        .casts
                        .values()
                        .find(|cast| {
                            (spell_id == 0 || cast.ability == spell_id)
                                && matches!(
                                    cast.execution.state(),
                                    benilla_sim::classic_spell::CastState::Casting { .. }
                                )
                        })
                        .map(|cast| CancelTarget::Instance(cast.id))
                });
                if let Some(target) = target {
                    self.queue_cancel(spell_id, target)?;
                }
            }
            ClientCommand::QuestgiverAccept { npc, quest } => {
                let giver = self.projection.capture_npc(npc)?;
                let input = self
                    .projection
                    .local(&self.world, UiRequest::AcceptQuest { quest, giver })?;
                self.queue(input.input.command)?;
            }
            ClientCommand::QuestgiverChooseReward {
                npc,
                quest,
                choice: 0,
            } => {
                let giver = self.projection.capture_npc(npc)?;
                let input = self
                    .projection
                    .local(&self.world, UiRequest::TurnInQuest { quest, giver })?;
                self.queue(input.input.command)?;
            }
            ClientCommand::QuestQuery { quest } => {
                events.push(self.projection.quest_template(&self.world, quest)?);
            }
            ClientCommand::QuestgiverStatusQuery { npc } => {
                let giver = self.projection.capture_npc(npc)?;
                if self
                    .world
                    .preview_quest_status(giver.id, &mut self.authority)?
                {
                    events.push(self.projection.quest_status(&self.world, giver)?);
                } else {
                    events.push(SessionEvent::QuestGiverStatus {
                        npc,
                        status: benilla_protocol::messages::dialog_status::NONE,
                    });
                }
            }
            ClientCommand::QuestgiverHello { npc } => {
                let giver = self.projection.capture_npc(npc)?;
                self.preview_quest(giver, 0, false)?;
                events.extend(self.projection.quest_greeting_events(&self.world, giver)?);
            }
            ClientCommand::QuestgiverQuery { npc, quest } => {
                let giver = self.projection.capture_npc(npc)?;
                self.preview_quest(giver, quest, false)?;
                events.push(self.projection.quest_detail(&self.world, quest, giver)?);
            }
            ClientCommand::QuestgiverComplete { npc, quest } => {
                let giver = self.projection.capture_npc(npc)?;
                self.preview_quest(giver, quest, true)?;
                events.push(self.projection.quest_progress(&self.world, quest, giver)?);
            }
            ClientCommand::QuestgiverRequestReward { npc, quest } => {
                let giver = self.projection.capture_npc(npc)?;
                self.preview_quest(giver, quest, true)?;
                events.push(self.projection.quest_offer(&self.world, quest, giver)?);
            }
            ClientCommand::RepopRequest => self.queue(n::Command::ReleaseSpirit)?,
            ClientCommand::SetActiveMover { guid } if guid == self.player_guid() => {}
            ClientCommand::WorldportAck | ClientCommand::SetSelection { .. } => {}
            _ => return Err(Error::UnsupportedCommand),
        }
        Ok(commands::Received {
            events,
            logout: false,
        })
    }
    fn pump(&mut self, elapsed: Duration) -> Pump {
        let mut out = Pump::default();
        if !self.closed && !self.closing && self.pause.is_none() {
            match self.debt.checked_add(elapsed) {
                Some(debt) => self.debt = debt,
                None => {
                    self.pause = Some(Pause {
                        kind: PauseKind::Clock,
                        message: Error::ClockOverflow.to_string(),
                    })
                }
            }
            // A successful explicit retry returns only its receipt. Flush the retained batch
            // on this pump, even with zero elapsed time, before admitting another tick/input.
            if self.pause.is_none() && self.outbox.is_some() {
                self.publish_pending(&mut out.events);
                out.pending_ticks = self.debt.as_nanos() / self.tick_duration().as_nanos();
                out.pause = self.pause.clone();
                return out;
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
                let mut result = self.advance_one(&mut out);
                // Only the runtime's dedicated, atomic journal-capacity refusal permits this
                // save/retry. A single-frame size or mechanics Limit never takes this path.
                if matches!(
                    result,
                    Err(Error::NativeRuntime(n::NativeError::JournalFull))
                ) {
                    match self.checkpoint() {
                        Ok(saved) => {
                            out.saved = Some(saved);
                            result = self.advance_one(&mut out);
                        }
                        Err(_) => break,
                    }
                }
                if self.world.state().tick != before {
                    self.debt -= self.tick_duration();
                    out.ticks += 1;
                }
                if let Err(error) = result {
                    if self.pause.is_none() {
                        let kind = if matches!(
                            error,
                            Error::NativeRuntime(n::NativeError::AuthorityUnavailable(_))
                        ) {
                            PauseKind::Authority
                        } else {
                            PauseKind::Simulation
                        };
                        self.pause = Some(Pause {
                            kind,
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
    fn retry_checkpoint(&mut self) -> Result<SavedBoundary, Error> {
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
    fn retry_tick(&mut self) -> Result<(), Error> {
        if self.closing
            || self.closed
            || !matches!(
                self.pause.as_ref().map(|p| &p.kind),
                Some(PauseKind::Authority)
            )
            || self.pending.is_none()
        {
            return Err(Error::Paused);
        }
        self.pause = None;
        Ok(())
    }
    fn shutdown(&mut self) -> Result<(SavedBoundary, usize, Vec<SessionEvent>), Error> {
        if self.closed {
            return Err(Error::Closed);
        }
        self.closing = true;
        let cancelled = self.actions.len() + self.pending.as_ref().map_or(0, |p| p.inputs.len());
        let saved = match self.outbox.as_ref().and_then(|batch| batch.saved.clone()) {
            Some(saved) => saved,
            None => self.checkpoint()?,
        };
        let mut events = vec![];
        self.publish_pending(&mut events);
        self.actions.clear();
        self.pending = None;
        self.closed = true;
        Ok((saved, cancelled, events))
    }
}
#[cfg(test)]
#[path = "native_controller/tests.rs"]
mod tests;
