// SPDX-License-Identifier: GPL-3.0-or-later
//! App-native clock, durable boundary and local command driver. The production capability
//! is still uninhabited. Component-test fixtures cannot enter through a production request.
use super::*;
use crate::net::p2p::native_geometry::route_commit::{
    RouteCommitBoundary, RouteCommitNotice, RouteCommitSink,
};
use crate::{creature_anim::move_flags, net::ClientCommand};
use benilla_protocol::messages::{MoveMode, RelayVerb, SpeedKind};
use benilla_sim::creature_ai::Point;
use benilla_world_client::native::{NativeMovementOrders, NativeProjection};
use benilla_world_runtime::native as n;
#[cfg(test)]
use benilla_world_store::native::{Identity, NativeStore};
const _: () = assert!(MAX_ACTIONS <= n::MAX_INPUTS_PER_TICK);
// At most one projected tick is retained. The runtime bounds records and catalog text;
// this additional event-count cap prevents an expanding projection from growing the outbox.
#[path = "native_controller/admin.rs"]
mod admin;
#[path = "native_controller/store.rs"]
mod store;
const MAX_PENDING_EVENTS: usize = 65_536;
struct PendingEvents {
    admin_rejections: Vec<(n::AdminRequest, n::AdminRejection)>,
    tick: u64,
    events: Vec<SessionEvent>,
    saved: Option<SavedBoundary>,
}

struct RouteCommitState {
    sink: Box<dyn RouteCommitSink>,
    boundary: RouteCommitBoundary,
    initialized: bool,
    // A capture error is also post-commit: retain it instead of retrying the inputs or
    // attempting to reconstruct evidence after a checkpoint has cleared the journal.
    pending: Option<Result<RouteCommitNotice, n::NativeError>>,
}

struct LocalAuthority(Box<dyn n::Authority + Send>);
impl n::Authority for LocalAuthority {
    fn observe_derived(&mut self, reply: &n::AuthorityReply) -> Result<(), n::NativeError> {
        self.0.observe_derived(reply)
    }
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SpeedOrder {
    guid: u64,
    counter: u32,
    kind: SpeedKind,
    speed_bits: u32,
}
fn speed_index(kind: SpeedKind) -> usize {
    match kind {
        SpeedKind::Walk => 0,
        SpeedKind::Run => 1,
        SpeedKind::RunBack => 2,
        SpeedKind::Swim => 3,
        SpeedKind::SwimBack => 4,
        SpeedKind::TurnRate => 5,
    }
}
enum CancelTarget {
    Instance(u64),
    Queued(u64),
}
enum Intent {
    Command { ticket: u64, command: n::Command },
    CancelCast { spell_id: u32, target: CancelTarget },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PublishedLoot {
    guid: u64,
    corpse: NpcHandle,
}
/// Last delivered projection, never an alternate simulation authority.
#[derive(Clone, Copy)]
struct PublishedPose {
    position: Point,
    orientation: n::Orientation,
}
impl PublishedPose {
    fn of(world: &n::NativeWorld) -> Self {
        Self {
            position: world.state().position,
            orientation: world.state().orientation,
        }
    }
}
struct PendingTick {
    inputs: Vec<n::OwnedInput>,
    moving: bool,
}
/// One forced-order namespace per actual session, independent of saved simulation time.
/// Root and forced speeds share the counter. Restored state receives fresh session orders.
/// Exact outstanding speed echoes are a local typed contract, not raw anti-cheat parity.
struct MovementOrders {
    counter: NativeMovementOrders,
    pending: Option<RootOrder>,
    acknowledged: Option<RootOrder>,
    pending_speeds: std::collections::BTreeMap<u32, SpeedOrder>,
    acknowledged_speeds: [Option<SpeedOrder>; 6],
}
impl MovementOrders {
    #[cfg(test)]
    fn new_session() -> Self {
        Self {
            counter: NativeMovementOrders::from_session_next_counter(0),
            pending: None,
            acknowledged: None,
            pending_speeds: std::collections::BTreeMap::new(),
            acknowledged_speeds: [None; 6],
        }
    }
    fn can_emit(&self, events: &[SessionEvent]) -> bool {
        self.pending_speeds.len()
            + events
                .iter()
                .filter(|e| matches!(e, SessionEvent::ForceSpeedChange { .. }))
                .count()
            <= MAX_PENDING_EVENTS
    }
    fn emitted(&mut self, events: &[SessionEvent]) {
        debug_assert!(self.can_emit(events));
        for event in events {
            if let SessionEvent::ForceSpeedChange {
                guid,
                kind,
                counter,
                speed,
            } = event
            {
                self.pending_speeds.insert(
                    *counter,
                    SpeedOrder {
                        guid: *guid,
                        counter: *counter,
                        kind: *kind,
                        speed_bits: speed.to_bits(),
                    },
                );
            }
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
    fn acknowledge_speed(&mut self, order: SpeedOrder) -> Result<(), Error> {
        let acknowledged = &mut self.acknowledged_speeds[speed_index(order.kind)];
        if self.pending_speeds.get(&order.counter) == Some(&order) {
            self.pending_speeds.remove(&order.counter);
            // Out-of-order exact ACKs retire only their own outstanding order.
            if acknowledged.is_none_or(|old| old.counter < order.counter) {
                *acknowledged = Some(order);
            }
            Ok(())
        } else if *acknowledged == Some(order) {
            Ok(())
        } else {
            Err(Error::InvalidMovementAck)
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
    admin_bridge: Option<admin::ControllerBridge>,
    world: n::NativeWorld,
    projection: NativeProjection,
    store: store::Store,
    selected: Selection,
    published_pose: PublishedPose,
    authority: LocalAuthority,
    route_commit: Option<RouteCommitState>,
    orders: MovementOrders,
    loot_window: Option<PublishedLoot>,
    initial: Option<Vec<SessionEvent>>,
    initial_reputation_ack: bool,
    initial_fields: benilla_protocol::ObjectFields,
    actions: VecDeque<Intent>,
    next_ticket: u64,
    pending: Option<PendingTick>,
    outbox: Option<PendingEvents>,
    resync_slow_durations: bool,
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
        if let Some(source) = &data.player_appearance {
            source
                .validate_request(n::PlayerAppearanceRequest {
                    player: selected.player,
                    race: character.race,
                    class: character.class,
                    gender: character.gender,
                    choices: n::NativePlayerAppearanceChoices {
                        skin: character.appearance.skin,
                        face: character.appearance.face,
                        hair_style: character.appearance.hair_style,
                        hair_color: character.appearance.hair_color,
                        facial_hair: character.appearance.facial_hair,
                    },
                })
                .map_err(Error::NativeRuntime)?;
        }
        let path = world_path(root, entry.id())?;
        let (store, world, projection) = match entry {
            Entry::Create { world, seed } => {
                let mut world =
                    n::NativeWorld::synthetic_component_test(data, world, selected.player, seed)?;
                let projection = NativeProjection::new(&world, selected.player, catalog)?;
                // Validate all initial fields before creating even the world container.
                projection.initial_events_with_movement_orders(
                    &world,
                    &mut NativeMovementOrders::from_session_next_counter(0),
                )?;
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
        Self::from_component_parts(world, projection, store.into(), selected, authority)
    }

    /// Assemble only an already-validated test world; production entry remains uninhabited.
    #[cfg(test)]
    fn from_component_parts(
        world: n::NativeWorld,
        projection: NativeProjection,
        store: store::Store,
        selected: Selection,
        authority: impl n::Authority + Send + 'static,
    ) -> Result<Self, Error> {
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
        let initial_reputation_ack = world.reputation_initial_ack_needed();
        let saved_tick = world.state().tick;
        let published_pose = PublishedPose::of(&world);
        Ok(Self {
            admin_bridge: None,
            world,
            projection,
            store,
            selected,
            published_pose,
            authority: LocalAuthority(Box::new(authority)),
            route_commit: None,
            orders,
            loot_window: None,
            initial: Some(initial),
            initial_reputation_ack,
            initial_fields,
            actions: VecDeque::new(),
            next_ticket: 0,
            pending: None,
            outbox: None,
            resync_slow_durations: false,
            debt: Duration::ZERO,
            saved_tick,
            pause: None,
            closing: false,
            closed: false,
        })
    }
    /// The paired selected-route sink must attach before resolving any owned Route.
    /// Failed initialization retains this exact attachment for explicit control retry.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "Native entry remains capability gated")
    )]
    pub(crate) fn attach_route_commit_sink(
        &mut self,
        sink: impl RouteCommitSink + 'static,
    ) -> Result<(), Error> {
        if self.closed || self.closing {
            return Err(Error::Closed);
        }
        if self.route_commit.is_some()
            || self.pending.is_some()
            || self.outbox.is_some()
            || self.pause.is_some()
            || !self.actions.is_empty()
        {
            return Err(Error::Paused);
        }
        let boundary = RouteCommitBoundary::capture_initial(&self.world)?;
        self.route_commit = Some(RouteCommitState {
            sink: Box::new(sink),
            boundary,
            initialized: false,
            pending: None,
        });
        self.drain_route_commit()
    }
    fn drain_route_commit(&mut self) -> Result<(), Error> {
        let result = (|| {
            let Some(commit) = self.route_commit.as_mut() else {
                return Ok(());
            };
            if !commit.initialized {
                commit.sink.initialize(&commit.boundary)?;
                commit.initialized = true;
            }
            if let Some(pending) = &commit.pending {
                let notice = pending.as_ref().map_err(Clone::clone)?;
                commit.sink.deliver(notice)?;
                commit.boundary = notice.boundary().clone();
                commit.pending = None;
            }
            Ok::<(), n::NativeError>(())
        })();
        result.map_err(|error| {
            self.pause = Some(Pause {
                kind: PauseKind::CommittedAwaitingRouteAck,
                message: error.to_string(),
            });
            error.into()
        })
    }
    fn ensure_input(&self) -> Result<(), Error> {
        if self.closed || self.closing {
            return Err(Error::Closed);
        }
        if self.pause.is_some()
            || self.outbox.is_some()
            || self
                .route_commit
                .as_ref()
                .is_some_and(|commit| !commit.initialized || commit.pending.is_some())
        {
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
    fn published(&mut self, events: &[SessionEvent]) {
        self.orders.emitted(events);
        for event in events {
            match event {
                SessionEvent::ObjectMove {
                    guid,
                    position,
                    orientation,
                } if *guid == self.player_guid() => {
                    // Consume the actual delivered pose, including delayed initial events.
                    // These values came only from validated native state/event projection.
                    self.published_pose = PublishedPose {
                        position: Point::new(position[0], position[1], position[2])
                            .expect("validated native projected position"),
                        orientation: n::Orientation::from_bits(orientation.to_bits())
                            .expect("validated native projected orientation"),
                    };
                }
                SessionEvent::LootResponse { guid, .. } => {
                    // Stock item/money packets have no GUID. Only the actual published panel
                    // binds their corpse; a queued open or restored target grants no panel.
                    self.loot_window =
                        self.projection
                            .capture_npc(*guid)
                            .ok()
                            .map(|corpse| PublishedLoot {
                                guid: *guid,
                                corpse,
                            });
                }
                SessionEvent::LootReleaseResponse { guid, result } if *result != 0 => {
                    if self.loot_window.is_some_and(|window| window.guid == *guid) {
                        self.loot_window = None;
                    }
                }
                SessionEvent::ObjectDestroyed(guid)
                    if self.loot_window.is_some_and(|window| window.guid == *guid) =>
                {
                    self.loot_window = None;
                }
                _ => {}
            }
        }
        // Explicit/missing-slot Release removes the looter before SendReleaseForAll,
        // so no response packet exists. Retire these rights only at publication.
        if self.loot_window.is_some_and(|window| {
            self.world
                .state()
                .quest_progression
                .as_ref()
                .and_then(|q| q.loot_target)
                != Some(window.corpse.id)
        }) {
            self.loot_window = None;
        }
    }
    fn ensure_no_pending_loot_open(&self) -> Result<(), Error> {
        // An open already in flight may replace the published old panel before a
        // GUID-less click executes. Reject that ambiguity instead of guessing its source.
        if self.actions.iter().any(|intent| {
            matches!(
                intent,
                Intent::Command {
                    command: n::Command::OpenLoot { .. },
                    ..
                }
            )
        }) || self.pending.as_ref().is_some_and(|pending| {
            pending
                .inputs
                .iter()
                .any(|input| matches!(input.input.command, n::Command::OpenLoot { .. }))
        }) {
            return Err(Error::Backpressure);
        }
        Ok(())
    }
    fn capture_loot_window(&self) -> Result<PublishedLoot, Error> {
        self.ensure_no_pending_loot_open()?;
        let window = self
            .loot_window
            .ok_or(benilla_world_client::Error::LootNotOpen)?;
        if self
            .world
            .state()
            .quest_progression
            .as_ref()
            .and_then(|p| p.loot_target)
            != Some(window.corpse.id)
            || self.projection.capture_npc(window.guid).ok() != Some(window.corpse)
        {
            return Err(benilla_world_client::Error::LootNotOpen.into());
        }
        Ok(window)
    }
    fn tick_duration(&self) -> Duration {
        Duration::from_millis(u64::from(self.world.profile().tick_ms))
    }
    fn queue(&mut self, command: n::Command) -> Result<(), Error> {
        if self.actions.len() + usize::from(self.initial_reputation_ack) == MAX_ACTIONS {
            return Err(Error::Backpressure);
        }
        let ticket = self.next_ticket;
        let next = ticket.checked_add(1).ok_or(n::NativeError::Limit)?;
        self.actions.push_back(Intent::Command { ticket, command });
        self.next_ticket = next;
        Ok(())
    }
    fn queue_cancel(&mut self, spell_id: u32, target: CancelTarget) -> Result<(), Error> {
        if self.actions.len() + usize::from(self.initial_reputation_ack) == MAX_ACTIONS {
            return Err(Error::Backpressure);
        }
        self.actions
            .push_back(Intent::CancelCast { spell_id, target });
        Ok(())
    }
    fn pose(position: [f32; 3], facing: f32) -> Result<(Point, n::Orientation), Error> {
        let orientation = n::Orientation::new(facing).map_err(|_| Error::InvalidMovement)?;
        let position = Point::new(position[0], position[1], position[2])
            .map_err(|_| Error::InvalidMovement)?;
        Ok((position, orientation))
    }
    fn correction(&self) -> SessionEvent {
        self.correction_at(self.published_pose)
    }
    fn correction_at(&self, pose: PublishedPose) -> SessionEvent {
        SessionEvent::UnitMove {
            guid: self.player_guid(),
            position: [
                pose.position.x.as_f32(),
                pose.position.y.as_f32(),
                pose.position.z.as_f32(),
            ],
            orientation: pose.orientation.as_f32(),
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
            .checked_add((self.actions.len() + usize::from(self.initial_reputation_ack)) as u64)
            .ok_or(n::NativeError::Limit)?;
        // All accepted local actions run in source order before one fixed simulation update.
        // Session-only tickets bind a pre-SpellStart cancel to its queued request; the durable
        // batch records its exact preceding input sequence instead of a predicted cast ID.
        let mut inputs =
            Vec::with_capacity(self.actions.len() + usize::from(self.initial_reputation_ack));
        if self.initial_reputation_ack {
            // Delivery bookkeeping never advances the world by itself. The initial
            // packet was sent from committed state; clear source needSend before
            // every user input and simulation callback of this natural update.
            inputs.push(n::OwnedInput {
                authenticated_owner: self.selected.player.owner,
                input: n::Input {
                    world: self.world.state().world,
                    tick: self.world.state().tick + 1,
                    player: self.selected.player,
                    sequence: self.world.state().next_sequence,
                    command: n::Command::AcknowledgeInitialReputations,
                },
            });
            self.initial_reputation_ack = false;
        }
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
            moving |= matches!(
                command,
                n::Command::Move { .. } | n::Command::Pose { .. } | n::Command::MovementPose { .. }
            );
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
        // An acknowledgement barrier precedes staging and calculation of the next tick.
        self.drain_route_commit()?;
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
        if let Some(commit) = self.route_commit.as_mut() {
            debug_assert!(commit.pending.is_none());
            commit.pending = Some(RouteCommitNotice::capture(&self.world, &commit.boundary));
        }
        let admin_rejections = self
            .world
            .last_tick()
            .into_iter()
            .flat_map(|r| &r.events)
            .filter_map(|event| match event {
                n::Event::AdminRejected { request, reason } => {
                    Some((request.clone(), reason.clone()))
                }
                _ => None,
            })
            .collect();
        let mut staged_orders =
            NativeMovementOrders::from_session_next_counter(self.orders.counter.next_counter());
        let projected = self
            .projection
            .advance_with_movement_orders(&self.world, &mut staged_orders)
            .and_then(|mut projected| {
                if pending.moving {
                    projected.push(self.correction_at(PublishedPose::of(&self.world)));
                }
                // A successful explicit pause retry may need one fresh UI timer per holder.
                let timers = self
                    .world
                    .state()
                    .player_slow
                    .as_ref()
                    .map_or(0, |state| state.holders().count());
                if projected.len() + timers > MAX_PENDING_EVENTS
                    || !self.orders.can_emit(&projected)
                {
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
        self.orders.counter = staged_orders;
        if self.world.profile().quest_progression.is_some()
            || self.world.profile().player_slow.is_some()
            || self.world.profile().player_environment.is_some()
            || self.world.profile().event_ai().is_some()
            || pending.moving
        {
            // Pose-bearing ticks also save before publishing authoritative position/facing.
            // Progression, slow durations and environment countdowns also mutate
            // without a projected event.
            // Persist every enabled tick, accepting one synchronous checkpoint per tick.
            // Project first: the store receipt clears last_tick and its event journal.
            self.outbox = Some(PendingEvents {
                admin_rejections,
                tick: self.world.state().tick,
                events: projected,
                saved: None,
            });
            out.saved = Some(self.checkpoint()?);
            self.publish_pending(&mut out.events);
        } else {
            self.published(&projected);
            out.events.extend(projected);
        }
        // Delivery follows required projection/outbox work. A dropped acknowledgement
        // cannot suppress already committed events or turn this into an input retry.
        self.drain_route_commit()
    }
    fn resync_slow_timers(&mut self, events: &mut Vec<SessionEvent>) {
        if !self.resync_slow_durations {
            return;
        }
        self.resync_slow_durations = false;
        // UI aura countdowns use wall-clock time; the authoritative simulation pauses.
        // Refresh only durations on retry, without replaying applications or speed orders.
        if let Some(state) = &self.world.state().player_slow {
            for holder in state.holders() {
                if !events.iter().any(|event| {
                    matches!(event,
                    SessionEvent::AuraDuration { slot, remaining_ms }
                    if *slot == holder.slot && *remaining_ms == holder.remaining_ms)
                }) {
                    events.push(SessionEvent::AuraDuration {
                        slot: holder.slot,
                        remaining_ms: holder.remaining_ms,
                    });
                }
            }
        }
    }
    fn publish_pending(&mut self, events: &mut Vec<SessionEvent>) {
        // This is one delivery to the local caller, not an acknowledged network outbox.
        // A crash after persistence is recovered through initial state, not notification replay.
        if self
            .outbox
            .as_ref()
            .is_some_and(|batch| batch.saved.is_some())
        {
            let mut batch = self.outbox.take().expect("durable pending events");
            self.publish_admin_rejections(&batch.admin_rejections);
            self.resync_slow_timers(&mut batch.events);
            self.published(&batch.events);
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
        let events = self.initial.take().ok_or(Error::Closed)?;
        self.published(&events);
        Ok(events)
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
        let p = self.published_pose.position;
        [p.x.as_f32(), p.y.as_f32(), p.z.as_f32()]
    }
    fn facing(&self) -> f32 {
        self.published_pose.orientation.as_f32()
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
        // ACK poses are validated but never admitted as movement or authority facts.
        if let ClientCommand::ForceSpeedAck {
            kind,
            guid,
            counter,
            speed,
            flags,
            pos,
            orientation,
            pitch,
            fall_time: _,
            jump,
            transport,
        } = command
        {
            let supported_flags = move_flags::ANY_MOVE
                | move_flags::TURN_LEFT
                | move_flags::TURN_RIGHT
                | move_flags::WALK_MODE
                | move_flags::ROOT;
            if self.closed
                || guid != self.player_guid()
                || !speed.is_finite()
                || speed < 0.0
                || flags & !supported_flags != 0
                || !pitch.is_finite()
                || pitch != 0.0
                || jump.is_some()
                || transport.is_some()
            {
                return Err(Error::InvalidMovementAck);
            }
            Self::pose(pos, orientation).map_err(|_| Error::InvalidMovementAck)?;
            self.orders.acknowledge_speed(SpeedOrder {
                guid,
                counter,
                kind,
                speed_bits: speed.to_bits(),
            })?;
            return Ok(commands::Received {
                events: vec![],
                logout: false,
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
                let (destination, orientation) = Self::pose(pos, orientation)?;
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
                let movement_flags = self
                    .world
                    .profile()
                    .source_motion
                    .as_ref()
                    .map(|_| benilla_sim::creature_motion::GroundMovementFlags::from_bits(flags))
                    .transpose()
                    .map_err(|_| Error::InvalidMovement)?;
                if self.world.state().health == 0
                    || self.orders.pending.is_some()
                    || self.orders.acknowledged.is_some()
                {
                    events.push(self.correction());
                } else {
                    self.queue(match movement_flags {
                        Some(movement_flags) => n::Command::MovementPose {
                            destination,
                            orientation,
                            movement_flags,
                        },
                        None => n::Command::Pose {
                            destination,
                            orientation,
                        },
                    })?;
                }
            }
            ClientCommand::AttackSwing { guid } => {
                if self.world.profile().player_melee.is_none() {
                    return Err(Error::UnsupportedCommand);
                }
                let target = self.projection.capture_npc(guid)?;
                let input = self.projection.prepare_attack_start(&self.world, target)?;
                self.queue(input.input.command)?;
            }
            ClientCommand::AttackStop => {
                if self.world.profile().player_melee.is_none() {
                    return Err(Error::UnsupportedCommand);
                }
                let input = self.projection.prepare_attack_stop(&self.world)?;
                self.queue(input.input.command)?;
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
            ClientCommand::ToggleHelm => {
                self.queue(n::Command::ToggleHelm)?;
            }
            ClientCommand::ToggleCloak => {
                self.queue(n::Command::ToggleCloak)?;
            }
            ClientCommand::SetActionBarToggles { toggles } => {
                self.queue(n::Command::SetActionBarToggles { toggles })?;
            }
            ClientCommand::SetWatchedFaction { rep_list_id } => {
                self.queue(n::Command::SetWatchedFaction {
                    list_id: rep_list_id,
                })?;
            }
            ClientCommand::SetFactionAtWar {
                rep_list_id,
                at_war,
            } => {
                self.queue(n::Command::SetFactionAtWar {
                    list_id: rep_list_id,
                    at_war,
                })?;
            }
            ClientCommand::SetFactionInactive {
                rep_list_id,
                inactive,
            } => {
                self.queue(n::Command::SetFactionInactive {
                    list_id: rep_list_id,
                    inactive,
                })?;
            }
            ClientCommand::QuestlogRemove { slot } => {
                // Source silently ignores out-of-range slots before touching quest state.
                if usize::from(slot) >= benilla_sim::quest_lifecycle::LOG_SIZE {
                    return Ok(commands::Received {
                        events: vec![],
                        logout: false,
                    });
                }
                self.queue(n::Command::AbandonQuest { slot })?;
            }
            ClientCommand::QuestgiverAccept { npc, quest } => {
                let giver = self.projection.capture_npc(npc)?;
                let input = self
                    .projection
                    .local(&self.world, UiRequest::AcceptQuest { quest, giver })?;
                self.queue(input.input.command)?;
            }
            ClientCommand::QuestgiverChooseReward { npc, quest, choice } => {
                // HandleQuestgiverChooseRewardOpcode checks bounds before giver lookup.
                if choice >= 6 {
                    return Err(Error::UnsupportedCommand);
                }
                let giver = self.projection.capture_npc(npc)?;
                let input = self.projection.local(
                    &self.world,
                    UiRequest::ChooseQuestReward {
                        quest,
                        giver,
                        choice,
                    },
                )?;
                self.queue(input.input.command)?;
            }
            ClientCommand::Loot { guid } => {
                self.ensure_no_pending_loot_open()?;
                let corpse = self.projection.capture_npc(guid)?;
                let input = self
                    .projection
                    .local(&self.world, UiRequest::OpenLoot(corpse))?;
                self.queue(input.input.command)?;
            }
            ClientCommand::DestroyItem {
                bag_index,
                slot,
                count,
            } => {
                use benilla_sim::inventory as inv;
                if bag_index != 255
                    || !(inv::FIRST_SLOT..inv::END_SLOT).contains(&slot)
                    || self
                        .world
                        .state()
                        .quest_progression
                        .as_ref()
                        .and_then(|s| s.inventory.as_ref())
                        .is_none()
                {
                    return Err(Error::UnsupportedCommand);
                }
                self.queue(n::Command::InventoryDestroy(n::InventoryDestroyRequest {
                    slot,
                    count,
                }))?;
            }
            ClientCommand::SplitItem {
                src_bag,
                src_slot,
                dst_bag,
                dst_slot,
                count,
            } => {
                // Source packet guards precede position lookup, including unsupported positions.
                if (src_bag, src_slot) == (dst_bag, dst_slot) || count == 0 {
                    return Ok(commands::Received {
                        events: vec![],
                        logout: false,
                    });
                }
                use benilla_sim::inventory as inv;
                if src_bag != 255
                    || dst_bag != 255
                    || !(inv::FIRST_SLOT..inv::END_SLOT).contains(&src_slot)
                    || !(inv::FIRST_SLOT..inv::END_SLOT).contains(&dst_slot)
                    || self
                        .world
                        .state()
                        .quest_progression
                        .as_ref()
                        .and_then(|state| state.inventory.as_ref())
                        .is_none()
                {
                    return Err(Error::UnsupportedCommand);
                }
                self.queue(n::Command::InventorySplit(n::InventorySplitRequest {
                    source: src_slot,
                    destination: dst_slot,
                    count,
                }))?;
            }
            ClientCommand::AutoStoreBagItem {
                src_bag,
                src_slot,
                dst_bag,
            } => {
                use benilla_sim::inventory as inv;
                if src_bag != 255
                    || dst_bag != 255
                    || !(inv::FIRST_SLOT..inv::END_SLOT).contains(&src_slot)
                    || self
                        .world
                        .state()
                        .quest_progression
                        .as_ref()
                        .and_then(|s| s.inventory.as_ref())
                        .is_none()
                {
                    return Err(Error::UnsupportedCommand);
                }
                self.queue(n::Command::InventoryAutoStore(
                    n::InventoryAutoStoreRequest { source: src_slot },
                ))?;
            }
            ClientCommand::AutoEquipItem { bag_index, slot } => {
                use benilla_sim::inventory as inv;
                if bag_index != 255
                    || !(inv::FIRST_SLOT..inv::END_SLOT).contains(&slot)
                    || self
                        .world
                        .state()
                        .quest_progression
                        .as_ref()
                        .and_then(|q| q.inventory.as_ref())
                        .is_none()
                {
                    return Err(Error::UnsupportedCommand);
                }
                self.queue(n::Command::EquipmentAutoEquip { source: slot })?;
            }
            ClientCommand::SwapInvItem { src_slot, dst_slot } => {
                use benilla_sim::inventory as inv;
                let backpack = |slot| (inv::FIRST_SLOT..inv::END_SLOT).contains(&slot);
                if self
                    .world
                    .state()
                    .quest_progression
                    .as_ref()
                    .and_then(|s| s.inventory.as_ref())
                    .is_none()
                {
                    return Err(Error::UnsupportedCommand);
                }
                // Resolve source custody at command execution, after preceding inputs.
                if backpack(src_slot) && backpack(dst_slot) {
                    self.queue(n::Command::InventoryMove(n::InventoryMoveRequest {
                        source: src_slot,
                        destination: dst_slot,
                    }))?;
                } else if (src_slot <= 15 && backpack(dst_slot))
                    || (backpack(src_slot) && dst_slot <= 15)
                {
                    self.queue(n::Command::EquipmentMove(n::EquipmentMoveRequest {
                        source: src_slot,
                        destination: dst_slot,
                    }))?;
                } else {
                    return Err(Error::UnsupportedCommand);
                }
            }
            ClientCommand::AutostoreLootItem { slot } => {
                let target = self.capture_loot_window()?.corpse;
                let input = self
                    .projection
                    .local(&self.world, UiRequest::TakeLootItem { target, slot })?;
                self.queue(input.input.command)?;
            }
            ClientCommand::LootMoney => {
                let target = self.capture_loot_window()?.corpse;
                let input = self
                    .projection
                    .local(&self.world, UiRequest::TakeLootMoney(target))?;
                self.queue(input.input.command)?;
            }
            ClientCommand::LootRelease { guid } => {
                let window = self.capture_loot_window()?;
                if window.guid != guid {
                    return Err(benilla_world_client::Error::LootNotOpen.into());
                }
                let input = self
                    .projection
                    .local(&self.world, UiRequest::ReleaseLoot(window.corpse))?;
                self.queue(input.input.command)?;
                // The client closes immediately when it sends release. Later GUID-less
                // clicks cannot attach to this panel or to a pending replacement panel.
                self.loot_window = None;
            }
            ClientCommand::ItemQuery { entry, .. } => {
                events.push(self.projection.item_query(&self.world, entry)?);
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
            // Pinned TaxiHandler parses this acknowledgement but performs no state update.
            ClientCommand::MoveSplineDone { .. } if self.world.state().source_splines.is_some() => {
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
        self.admit_admin_mailbox();
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
                let _ = self.drain_route_commit();
                out.pending_ticks = self.debt.as_nanos() / self.tick_duration().as_nanos();
                out.pause = self.pause.clone();
                self.refresh_admin_mailbox();
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
        self.refresh_admin_mailbox();
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
        self.resync_slow_durations = true;
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
        self.resync_slow_durations = true;
        self.pause = None;
        Ok(())
    }
    fn retry_route_commit(&mut self) -> Result<(), Error> {
        if self.closing
            || self.closed
            || !matches!(
                self.pause.as_ref().map(|pause| &pause.kind),
                Some(PauseKind::CommittedAwaitingRouteAck)
            )
        {
            return Err(Error::Paused);
        }
        self.drain_route_commit()?;
        self.resync_slow_durations = true;
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
        // Closing a loaded owner does not rehydrate it on the next session. Saving
        // committed state is still permitted when its acknowledgement channel was lost.
        self.route_commit = None;
        self.closed = true;
        self.refresh_admin_mailbox();
        Ok((saved, cancelled, events))
    }
}
#[cfg(test)]
#[path = "native_controller/tests.rs"]
pub(crate) mod tests;
