// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded local geometry transport, not peer authority or a navigation mesh. The only
//! movement solver is an unobstructed, continuously certified dry horizontal segment.
//! Separately selected static LOS does not imply walking support or accessibility.
use benilla_sim::creature_ai::{self as ai, movement::*};
use benilla_world_runtime::{
    native::{
        Authority, AuthorityReply, AuthorityRequest, CombatFacts, CombatPhase, CombatQuery, Facts,
        MotionFacts, MotionQuery, NativeError, Query, SourceSpellFacts, SourceSpellPhase,
        SourceSpellQuery,
    },
    Digest, WorldId,
};
use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub(super) const QUEUE_CAPACITY: usize = 16;
pub(super) const PER_FRAME: usize = 8;
const WAIT: Duration = Duration::from_millis(500);
const MAX_SEGMENT: f32 = 8.0;

/// Resolved by a future source-audited compiler, never supplied by a network packet.
/// Collision height is distinct from combat reach; it cannot be inferred from it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Body {
    pub radius: ai::Real,
    pub height: ai::Real,
}
impl Body {
    fn validate(self) -> bool {
        (0.0..=4.0).contains(&self.radius.as_f32())
            && self.radius.as_f32() > 0.0
            && (0.0..=16.0).contains(&self.height.as_f32())
            && self.height.as_f32() > 0.0
    }
}

/// Trusted source-resolved player body and maximum speed for this bounded movement state.
/// Enabled native slow profiles supply their current source-derived ground speed in the
/// authority request. Sprint/mount/swim/flight, transports and jump/fall remain excluded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PlayerMotion {
    pub body: Body,
    pub speed: ai::Real,
}
impl PlayerMotion {
    fn validate(self) -> bool {
        self.body.validate()
            && (0.0..=MAX_SEGMENT).contains(&self.speed.as_f32())
            && self.speed.as_f32() > 0.0
    }
}

/// Closed compiler/controller proof that this content/map has no non-map collision
/// participants: game-object/door/transport spawns, runtime creation and scripted colliders
/// are excluded, and the controller cannot create them while this token is installed.
/// There is deliberately no production constructor until that complete input audit exists.
/// Empty ADT references, empty entity queries and a digest alone cannot construct this token.
#[derive(Clone)]
pub(crate) struct NonRenderParticipants {
    content: Digest,
    map: u32,
}
impl NonRenderParticipants {
    #[cfg(test)]
    pub(crate) fn authored_fixture(content: Digest, map: u32) -> Self {
        Self { content, map }
    }
    pub(super) fn matches(&self, content: Digest, map: u32) -> bool {
        self.content == content && self.map == map
    }
}

/// Explicit LOS-enabled policy with frozen heights from the runtime's source-motion
/// owner. This is not native admission: world construction remains capability gated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StaticLosPolicy {
    profile: Digest,
    map: u32,
    content: Digest,
    geometry: Digest,
    authority: Digest,
    player_height: ai::Real,
    npc: u64,
    npc_height: ai::Real,
}
impl StaticLosPolicy {
    #[allow(
        dead_code,
        reason = "used by app/controller integration, not the standalone geometry harness"
    )]
    pub(crate) fn from_profile(
        profile: &benilla_world_runtime::native::Profile,
        line_of_sight_enabled: bool,
    ) -> Result<Self, Failure> {
        if !line_of_sight_enabled || profile.npcs.len() != 1 {
            return Err(Failure::UnsupportedQuery);
        }
        let motion = profile
            .source_motion
            .as_ref()
            .ok_or(Failure::UnresolvedBody)?;
        Ok(Self {
            profile: profile.digest(),
            map: profile.map_id,
            content: profile.content_digest,
            geometry: profile.geometry_revision,
            authority: profile.authority_policy,
            player_height: motion.player_collision_height,
            npc: profile.npcs[0].ai.creature_id,
            npc_height: motion.npc_policy.settings.collision_height,
        })
    }
    fn check(self, session: &Session) -> Result<(), Failure> {
        if self.profile != session.profile
            || self.map != session.map
            || self.content != session.content
            || self.geometry != session.geometry
            || self.authority != session.policy
        {
            return Err(Failure::Binding);
        }
        if session
            .player
            .is_none_or(|p| p.body.height != self.player_height)
            || session.bodies.len() != 1
            || session
                .bodies
                .get(&self.npc)
                .is_none_or(|b| b.height != self.npc_height)
        {
            return Err(Failure::UnresolvedBody);
        }
        Ok(())
    }
}

/// Local session identity. Digests bind inputs; they do not attest that client collision
/// equals CMaNGOS VMap data. Production admission must separately verify asset identity.
#[derive(Clone)]
pub(crate) struct Session {
    pub world: WorldId,
    pub profile: Digest,
    pub policy: Digest,
    pub content: Digest,
    pub geometry: Digest,
    pub map: u32,
    pub bodies: BTreeMap<u64, Body>,
    pub player: Option<PlayerMotion>,
    pub participants: Option<NonRenderParticipants>,
    pub static_los: Option<StaticLosPolicy>,
}
impl Session {
    pub(super) fn body(&self, request: &AuthorityRequest) -> Result<Body, Failure> {
        self.check_participants()?;
        let b = &request.binding;
        if b.world != self.world
            || b.profile != self.profile
            || b.authority_policy != self.policy
            || b.geometry != self.geometry
            || b.tick == 0
        {
            return Err(Failure::Binding);
        }
        let m = match &request.query {
            Query::Movement(m) => m,
            Query::PlayerMovement {
                accepted_path,
                elapsed_ms,
                ..
            } => {
                if accepted_path.len() >= benilla_world_runtime::native::MAX_INPUTS_PER_TICK
                    || *elapsed_ms == 0
                    || *elapsed_ms > 1000
                {
                    return Err(Failure::Invalid);
                }
                if b.npc.is_some() {
                    return Err(Failure::Binding);
                }
                return self.player.map(|p| p.body).ok_or(Failure::UnresolvedBody);
            }
            Query::SourceSpell(SourceSpellQuery::LineOfSight(_))
            | Query::SourceMotion(MotionQuery::Combat(CombatQuery::NpcLineOfSight(_))) => {
                let supported = match &request.query {
                    Query::SourceSpell(SourceSpellQuery::LineOfSight(q)) => matches!(
                        q.phase,
                        SourceSpellPhase::Prepare | SourceSpellPhase::ReleaseCheck
                    ),
                    Query::SourceMotion(MotionQuery::Combat(CombatQuery::NpcLineOfSight(q))) => {
                        q.phase == CombatPhase::Sight
                    }
                    _ => false,
                };
                if !supported {
                    return Err(Failure::UnsupportedQuery);
                }
                self.static_los
                    .ok_or(Failure::UnsupportedQuery)?
                    .check(self)?;
                self.player.ok_or(Failure::UnresolvedBody)?;
                let npc = b.npc.ok_or(Failure::Binding)?;
                return self
                    .bodies
                    .get(&npc.spawn)
                    .copied()
                    .ok_or(Failure::UnresolvedBody);
            }
            _ => return Err(Failure::UnsupportedQuery),
        };
        if m.map_id != self.map
            || m.content_revision.0 != self.content
            || m.geometry_revision.0 != self.geometry
            || b.npc.is_none_or(|id| id.spawn != m.creature_id)
        {
            return Err(Failure::Binding);
        }
        self.bodies
            .get(&m.creature_id)
            .copied()
            .ok_or(Failure::UnresolvedBody)
    }
    fn check_participants(&self) -> Result<(), Failure> {
        if self
            .participants
            .as_ref()
            .is_some_and(|proof| proof.matches(self.content, self.map))
        {
            Ok(())
        } else {
            Err(Failure::UnprovenParticipants)
        }
    }
    fn validate(&self) -> bool {
        [self.profile, self.policy, self.content, self.geometry]
            .iter()
            .all(|d| *d != [0; 32])
            && self.world != [0; 16]
            && (!self.bodies.is_empty() || self.player.is_some())
            && self.player.is_none_or(PlayerMotion::validate)
            && self.bodies.len() <= 64
            && self.bodies.iter().all(|(id, b)| *id != 0 && b.validate())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Failure {
    Binding,
    Invalid,
    UnresolvedBody,
    UnprovenParticipants,
    UnsupportedQuery,
    UnsupportedNavigation,
    MissingGeometry,
    Obstacle,
    Water,
    UnsupportedGround,
}
impl Failure {
    fn native(self) -> NativeError {
        match self {
            Self::UnprovenParticipants => NativeError::AuthorityUnavailable(
                "non-render collision participants have not been excluded",
            ),
            Self::Binding => NativeError::AuthorityBinding,
            Self::Invalid => NativeError::InvalidInput,
            Self::UnsupportedQuery => {
                NativeError::AuthorityUnavailable("local geometry query or policy unsupported")
            }
            Self::UnresolvedBody => {
                NativeError::AuthorityUnavailable("native collision dimensions unresolved")
            }
            Self::MissingGeometry => {
                NativeError::AuthorityUnavailable("local geometry coverage incomplete")
            }
            Self::UnsupportedNavigation => {
                NativeError::AuthorityUnavailable("outside local planar navigation domain")
            }
            Self::UnsupportedGround => {
                NativeError::AuthorityUnavailable("continuous dry planar support unproven")
            }
            Self::Water => NativeError::AuthorityUnavailable("liquid navigation unsupported"),
            Self::Obstacle => NativeError::Navigation(ai::AiError::Movement(BlockReason::Obstacle)),
        }
    }
}

struct Work {
    ticket: u64,
    request: AuthorityRequest,
    deadline: Instant,
    reply: Sender<Reply>,
}
struct Reply {
    ticket: u64,
    request: AuthorityRequest,
    result: Result<Facts, Failure>,
}

/// Owned exclusively by the off-thread native driver. The main-thread service never waits.
/// A timeout leaves NativeWorld::step uncommitted; retry the same input/tick or pause explicitly.
/// Do not call resolve from a Bevy system: it deliberately waits for the main-thread service.
pub(crate) struct LocalAuthority {
    session: Session,
    tx: Sender<Work>,
    ticket: u64,
    active: Arc<AtomicBool>,
    cancelled: Receiver<()>,
}
pub(super) struct Mailbox {
    session: Session,
    rx: Receiver<Work>,
    active: Arc<AtomicBool>,
    _cancellation: Sender<()>,
}
impl Drop for Mailbox {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
        // Crossbeam keeps queued values alive while a sender exists. Release old
        // reply endpoints now; the separate cancellation channel also covers a
        // producer racing this bounded drain before receiver destruction.
        for _ in 0..QUEUE_CAPACITY {
            if self.rx.try_recv().is_err() {
                break;
            }
        }
    }
}
pub(super) fn mailbox(session: Session) -> Result<(LocalAuthority, Mailbox), Failure> {
    session.check_participants()?;
    if !session.validate() {
        return Err(Failure::Invalid);
    }
    let (tx, rx) = bounded(QUEUE_CAPACITY);
    let active = Arc::new(AtomicBool::new(true));
    let (cancellation, cancelled) = bounded(0);
    Ok((
        LocalAuthority {
            session: session.clone(),
            tx,
            ticket: 0,
            active: active.clone(),
            cancelled,
        },
        Mailbox {
            session,
            rx,
            active,
            _cancellation: cancellation,
        },
    ))
}
impl Authority for LocalAuthority {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        self.check_active()?;
        self.session.body(request).map_err(Failure::native)?;
        self.ticket = self.ticket.checked_add(1).ok_or(NativeError::Limit)?;
        let (tx, rx) = bounded(1);
        let deadline = Instant::now() + WAIT;
        self.tx
            .try_send(Work {
                ticket: self.ticket,
                request: request.clone(),
                deadline,
                reply: tx,
            })
            .map_err(|e| match e {
                TrySendError::Full(_) => {
                    NativeError::AuthorityUnavailable("local geometry queue full")
                }
                TrySendError::Disconnected(_) => {
                    NativeError::AuthorityUnavailable("local geometry service closed")
                }
            })?;
        let reply = receive_reply_until(&rx, &self.cancelled, deadline)?;
        self.finish_reply(request, reply)
    }
}
impl LocalAuthority {
    fn check_active(&self) -> Result<(), NativeError> {
        if self.active.load(Ordering::Acquire) {
            Ok(())
        } else {
            Err(NativeError::AuthorityUnavailable(
                "local geometry snapshot replaced",
            ))
        }
    }
    fn finish_reply(
        &self,
        request: &AuthorityRequest,
        reply: Reply,
    ) -> Result<AuthorityReply, NativeError> {
        self.check_active()?;
        if reply.ticket != self.ticket || reply.request != *request {
            return Err(NativeError::AuthorityBinding);
        }
        let facts = reply.result.map_err(Failure::native)?;
        validate_facts(&self.session, request, &facts).map_err(Failure::native)?;
        self.check_active()?;
        Ok(AuthorityReply {
            request: request.clone(),
            facts,
        })
    }
}

fn receive_reply_until(
    rx: &Receiver<Reply>,
    cancelled: &Receiver<()>,
    deadline: Instant,
) -> Result<Reply, NativeError> {
    let unavailable = || NativeError::AuthorityUnavailable("local geometry reply unavailable");
    let timeout = crossbeam_channel::at(deadline);
    let reply = crossbeam_channel::select_biased! {
        recv(cancelled) -> _ => return Err(unavailable()),
        recv(rx) -> result => result.map_err(|_| unavailable())?,
        recv(timeout) -> _ => return Err(unavailable()),
    };
    // Channel receives may return an already-buffered reply after their deadline. A driver
    // descheduled after enqueue must not renew the request's authority window when it resumes.
    if Instant::now() >= deadline {
        return Err(unavailable());
    }
    Ok(reply)
}

impl Mailbox {
    /// The count includes expired/rejected jobs, so cancellation cannot bypass the frame bound.
    /// A caller provides a fresh read-only geometry view for each service invocation.
    #[cfg(test)]
    pub(super) fn serve(
        &self,
        limit: usize,
        mut geometry: impl FnMut(&MovementRequest, Body) -> Result<MovementOutcome, Failure>,
    ) -> usize {
        self.serve_queries(limit, |_, request, body| {
            let Query::Movement(m) = &request.query else {
                return Err(Failure::UnsupportedQuery);
            };
            geometry(m, body).map(Facts::Movement)
        })
    }
    pub(super) fn serve_queries(
        &self,
        limit: usize,
        mut geometry: impl FnMut(&Session, &AuthorityRequest, Body) -> Result<Facts, Failure>,
    ) -> usize {
        self.serve_queries_until(limit, |session, request, body, _| {
            geometry(session, request, body)
        })
    }
    /// Product-backed queries inherit the original job and frame deadlines, never renew them.
    pub(super) fn serve_queries_until(
        &self,
        limit: usize,
        mut geometry: impl FnMut(&Session, &AuthorityRequest, Body, Instant) -> Result<Facts, Failure>,
    ) -> usize {
        let deadline = Instant::now() + Duration::from_millis(2);
        let mut count = 0;
        while count < limit.min(PER_FRAME) {
            if count > 0 && Instant::now() >= deadline {
                break;
            }
            let Ok(work) = self.rx.try_recv() else { break };
            count += 1;
            if Instant::now() >= work.deadline {
                continue;
            }
            let result = self.session.body(&work.request).and_then(|body| {
                let facts = geometry(
                    &self.session,
                    &work.request,
                    body,
                    work.deadline.min(deadline),
                )?;
                validate_facts(&self.session, &work.request, &facts)?;
                Ok(facts)
            });
            if Instant::now() >= work.deadline {
                continue;
            }
            let _ = work.reply.try_send(Reply {
                ticket: work.ticket,
                request: work.request,
                result,
            });
        }
        count
    }
}

fn validate_facts(
    session: &Session,
    request: &AuthorityRequest,
    facts: &Facts,
) -> Result<(), Failure> {
    match (&request.query, facts) {
        (Query::Movement(m), Facts::Movement(outcome)) => validate_outcome(m, outcome),
        (Query::PlayerMovement { .. }, Facts::PlayerMovement { accepted }) => {
            let player = session.player.ok_or(Failure::UnresolvedBody)?;
            let within_budget = player_path(&request.query, player)?.is_some();
            if *accepted && !within_budget {
                Err(Failure::Binding)
            } else {
                Ok(())
            }
        }
        (
            Query::SourceSpell(SourceSpellQuery::LineOfSight(_)),
            Facts::SourceSpell(SourceSpellFacts::LineOfSight(Check::Passed | Check::Failed)),
        )
        | (
            Query::SourceMotion(MotionQuery::Combat(CombatQuery::NpcLineOfSight(_))),
            Facts::SourceMotion(MotionFacts::Combat(CombatFacts::NpcLineOfSight(
                Check::Passed | Check::Failed,
            ))),
        ) => session.body(request).map(|_| ()),
        _ => Err(Failure::Binding),
    }
}

/// Each accepted packet extends the same immutable path; even returning to the origin spends
/// distance. This is a local conservative policy, not CMaNGOS client anti-cheat parity.
/// Every segment length is rounded upward to binary32, then added in binary64 with an upward
/// guard when addition is inexact. Budget arithmetic starts from the source-resolved f32 speed.
fn player_path(query: &Query, player: PlayerMotion) -> Result<Option<Vec<[f32; 3]>>, Failure> {
    let Query::PlayerMovement {
        frame_origin,
        accepted_path,
        destination,
        elapsed_ms,
        speed,
    } = query
    else {
        return Err(Failure::UnsupportedQuery);
    };
    if !player.validate()
        || *elapsed_ms == 0
        || *elapsed_ms > 1000
        || accepted_path.len() >= benilla_world_runtime::native::MAX_INPUTS_PER_TICK
    {
        return Err(Failure::Invalid);
    }
    let points: Vec<_> = std::iter::once(*frame_origin)
        .chain(accepted_path.iter().copied())
        .chain(std::iter::once(*destination))
        .map(point)
        .collect();
    if points
        .iter()
        .flatten()
        .any(|v| !v.is_finite() || v.abs() > 32760.0)
    {
        return Err(Failure::Invalid);
    }
    // Only the trusted runtime constructs this request; the speed is echoed and replay-bound.
    // Legacy profiles retain the session speed. Enabled profiles carry the live movement mode.
    let speed = speed.unwrap_or(player.speed).as_f32();
    if !(0.0..=MAX_SEGMENT).contains(&speed) {
        return Err(Failure::Invalid);
    }
    let numerator = f64::from(speed) * f64::from(*elapsed_ms);
    let nearest_budget = numerator / 1000.0;
    // Fused residual retains the sign of division error; multiplying the quotient back
    // normally can round away an upward error (e.g. .001 * 1000 == 1).
    let budget = if (-nearest_budget).mul_add(1000.0, numerator) < 0.0 {
        f64::from_bits(nearest_budget.to_bits() - 1)
    } else {
        nearest_budget
    };
    if budget > f64::from(MAX_SEGMENT) {
        return Err(Failure::UnsupportedNavigation);
    }
    let mut distance = 0.0_f64;
    for pair in points.windows(2) {
        if pair[0][2] != pair[1][2] {
            return Err(Failure::UnsupportedGround);
        }
        let dx = f64::from(pair[1][0]) - f64::from(pair[0][0]);
        let dy = f64::from(pair[1][1]) - f64::from(pair[0][1]);
        // Non-axis-aligned segments use outward-rounded binary64 operations before
        // conversion; this can reject a borderline diagonal, never grant extra distance.
        let up = |v: f64| {
            if v == 0.0 {
                v
            } else {
                f64::from_bits(v.to_bits() + 1)
            }
        };
        let abs_difference = |a: f32, b: f32, difference: f64| {
            let a = f64::from(a);
            let b = -f64::from(b);
            let v = difference - a;
            let error = (a - (difference - v)) + (b - v);
            if error.signum() == difference.signum() && error != 0.0 {
                up(difference.abs())
            } else {
                difference.abs()
            }
        };
        let dx = abs_difference(pair[1][0], pair[0][0], dx);
        let dy = abs_difference(pair[1][1], pair[0][1], dy);
        let upper64 = if dx == 0.0 {
            dy
        } else if dy == 0.0 {
            dx
        } else {
            up(up(up(dx * dx) + up(dy * dy)).sqrt())
        };
        let mut upper = upper64 as f32;
        if f64::from(upper) < upper64 {
            upper = f32::from_bits(upper.to_bits() + 1);
        }
        let next = distance + f64::from(upper);
        // Error-free TwoSum detects a rounded-down accumulation; never let a very small
        // movement disappear beside a large one. Zero movement does not spend an ULP.
        let v = next - distance;
        let error = (distance - (next - v)) + (f64::from(upper) - v);
        distance = if error > 0.0 {
            f64::from_bits(next.to_bits() + 1)
        } else {
            next
        };
        if distance > budget {
            return Ok(None);
        }
    }
    Ok(Some(points))
}

pub(crate) fn solve_player(
    query: &Query,
    player: PlayerMotion,
    geometry: &mut impl Corridor,
) -> Result<Facts, Failure> {
    let Some(points) = player_path(query, player)? else {
        return Ok(Facts::PlayerMovement { accepted: false });
    };
    for pair in points.windows(2) {
        match geometry.certify(pair[0], pair[1], player.body) {
            Ok(()) => {}
            Err(Failure::Obstacle) => return Ok(Facts::PlayerMovement { accepted: false }),
            Err(failure) => return Err(failure),
        }
    }
    Ok(Facts::PlayerMovement { accepted: true })
}

fn validate_outcome(request: &MovementRequest, outcome: &MovementOutcome) -> Result<(), Failure> {
    struct Answer(MovementOutcome);
    impl GeometryAuthority for Answer {
        fn resolve(&mut self, _: &MovementRequest) -> Result<MovementOutcome, ai::AiError> {
            Ok(self.0.clone())
        }
    }
    resolve_movement(&mut Answer(outcome.clone()), request.clone())
        .map(|_| ())
        .map_err(|_| Failure::Binding)
}

/// Geometry providers must prove coverage first, then the entire swept body, elevated LOS,
/// continuous support and absence of liquid. No-hit alone is never this certificate.
pub(crate) trait Corridor {
    fn certify(&mut self, from: [f32; 3], to: [f32; 3], body: Body) -> Result<(), Failure>;
}

pub(super) fn solve(
    request: &MovementRequest,
    body: Body,
    geometry: &mut impl Corridor,
) -> Result<MovementOutcome, Failure> {
    if !body.validate()
        || request.elapsed_ms == 0
        || request.elapsed_ms > 1000
        || request.request_id == 0
        || request.command_id == 0
        || request.creature_id == 0
        || request.speed.as_f32() <= 0.0
    {
        return Err(Failure::Invalid);
    }
    let from = point(request.origin);
    let goal = point(request.goal);
    if from
        .iter()
        .chain(&goal)
        .any(|x| !x.is_finite() || x.abs() > 32760.0)
    {
        return Err(Failure::Invalid);
    }
    if from[2] != goal[2] {
        return Err(Failure::UnsupportedGround);
    }
    // Client-local FPU geometry, explicitly outside the deterministic simulation kernel.
    // The independent software-binary32 movement validator remains the final budget gate.
    let budget = request.speed.as_f32() * request.elapsed_ms as f32 / 1000.0;
    if !budget.is_finite() || budget <= 0.0 || budget > MAX_SEGMENT {
        return Err(Failure::UnsupportedNavigation);
    }
    let dx = f64::from(goal[0]) - f64::from(from[0]);
    let dy = f64::from(goal[1]) - f64::from(from[1]);
    let distance = dx.hypot(dy);
    let mut to = goal;
    if distance > f64::from(budget) {
        // Leave several coordinate ULPs of headroom; rounding the position must not create
        // more movement than the software arithmetic allows. Extremely tiny progress refuses.
        let coordinate_ulp = from[..2]
            .iter()
            .map(|v| {
                let v = v.abs();
                f64::from(f32::from_bits(v.to_bits() + 1) - v)
            })
            .fold(0.0_f64, f64::max);
        let permitted = f64::from(budget) - coordinate_ulp * 4.0 - f64::from(budget) * 1e-6;
        if permitted <= 0.0 {
            return Err(Failure::UnsupportedNavigation);
        }
        let fraction = permitted / distance;
        to[0] = (f64::from(from[0]) + dx * fraction) as f32;
        to[1] = (f64::from(from[1]) + dy * fraction) as f32;
    }
    let actual =
        (f64::from(to[0]) - f64::from(from[0])).hypot(f64::from(to[1]) - f64::from(from[1]));
    if actual == 0.0 && from != goal {
        return Err(Failure::UnsupportedNavigation);
    }
    let mut travelled = actual as f32;
    // Round total distance upward, including diagonal displacement; never report straight-line
    // progress with a travelled value smaller than its actual displacement.
    if f64::from(travelled) < actual {
        travelled = f32::from_bits(travelled.to_bits() + 1);
    }
    match geometry.certify(from, to, body) {
        Ok(()) => {}
        Err(Failure::Obstacle) => {
            return Ok(MovementOutcome {
                request_id: request.request_id,
                map_id: request.map_id,
                content_revision: request.content_revision,
                geometry_revision: request.geometry_revision,
                result: MovementResult::Blocked(BlockReason::Obstacle),
            });
        }
        Err(error) => return Err(error),
    }
    let outcome = MovementOutcome {
        request_id: request.request_id,
        map_id: request.map_id,
        content_revision: request.content_revision,
        geometry_revision: request.geometry_revision,
        result: MovementResult::Progress {
            position: ai::Point::new(to[0], to[1], to[2]).map_err(|_| Failure::Invalid)?,
            travelled: ai::Real::new(travelled).map_err(|_| Failure::Invalid)?,
            arrived: to == goal,
            checks: SegmentChecks {
                loaded_coverage: Check::Passed,
                line_of_sight: Check::Passed,
                body_sweep: Check::Passed,
                continuous_ground_support: Check::Passed,
                dry_ground: Check::Passed,
            },
        },
    };
    validate_outcome(request, &outcome)?;
    Ok(outcome)
}
fn point(p: ai::Point) -> [f32; 3] {
    [p.x.as_f32(), p.y.as_f32(), p.z.as_f32()]
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

/// Render-frame accounting, separate from fixed ticks (which may run several times per frame).
#[derive(Default)]
pub(super) struct FrameBudget {
    frame: Option<u32>,
    used: usize,
}
impl FrameBudget {
    pub(super) fn remaining(&mut self, frame: u32) -> usize {
        if self.frame != Some(frame) {
            self.frame = Some(frame);
            self.used = 0;
        }
        PER_FRAME.saturating_sub(self.used)
    }
    pub(super) fn spend(&mut self, used: usize) {
        self.used = self.used.saturating_add(used);
    }
}

/// Exact current raw-source identity. Tile order is canonical (row-major) at the provider.
/// The policy names client-local geometry, never deterministic VMap/MMap parity.
pub(super) fn geometry_revision(
    map: u32,
    wdt: [u8; 32],
    tiles: &[((i32, i32), [u8; 32]); 9],
    models: &[(u32, [u8; 32])],
    inventory: [u8; 32],
) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(b"benilla-local-dry-flat-cell-static-los-corridor-v6\0");
    hash.update(map.to_le_bytes());
    hash.update(wdt);
    hash.update(inventory);
    for ((x, y), digest) in tiles {
        hash.update(x.to_le_bytes());
        hash.update(y.to_le_bytes());
        hash.update(digest);
    }
    hash.update((models.len() as u64).to_le_bytes());
    for (uid, source) in models {
        hash.update(uid.to_le_bytes());
        hash.update(source);
    }
    hash.finalize().into()
}
