// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded local geometry transport, not peer authority or a navigation mesh. The only
//! positive solver is an unobstructed, continuously certified dry horizontal segment.
use benilla_sim::creature_ai::{self as ai, movement::*};
use benilla_world_runtime::{
    native::{Authority, AuthorityReply, AuthorityRequest, Facts, NativeError, Query},
    Digest, WorldId,
};
use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};
use std::{
    collections::BTreeMap,
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

/// Closed compiler/controller proof that this content/map has no other collision
/// participants: game-object/door/transport spawns, runtime creation and scripted colliders
/// are excluded, and the controller cannot create them while this token is installed.
/// There is deliberately no production constructor until that complete input audit exists.
/// Empty ADT references, empty entity queries and a digest alone cannot construct this token.
#[derive(Clone)]
pub(crate) struct TerrainOnlyParticipants {
    content: Digest,
    map: u32,
}
impl TerrainOnlyParticipants {
    #[cfg(test)]
    fn authored_fixture(content: Digest, map: u32) -> Self {
        Self { content, map }
    }
    fn matches(&self, content: Digest, map: u32) -> bool {
        self.content == content && self.map == map
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
    pub participants: Option<TerrainOnlyParticipants>,
}
impl Session {
    fn body(&self, request: &AuthorityRequest) -> Result<Body, Failure> {
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
        let Query::Movement(m) = &request.query else {
            return Err(Failure::UnsupportedQuery);
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
            && !self.bodies.is_empty()
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
                "non-terrain collision participants have not been excluded",
            ),
            Self::Binding => NativeError::AuthorityBinding,
            Self::Invalid => NativeError::InvalidInput,
            Self::UnsupportedQuery => {
                NativeError::AuthorityUnavailable("local geometry handles movement only")
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
    result: Result<MovementOutcome, Failure>,
}

/// Owned exclusively by the off-thread native driver. The main-thread service never waits.
/// A timeout leaves NativeWorld::step uncommitted; retry the same input/tick or pause explicitly.
/// Do not call resolve from a Bevy system: it deliberately waits for the main-thread service.
pub(crate) struct LocalAuthority {
    session: Session,
    tx: Sender<Work>,
    ticket: u64,
}
pub(super) struct Mailbox {
    session: Session,
    rx: Receiver<Work>,
}
pub(super) fn mailbox(session: Session) -> Result<(LocalAuthority, Mailbox), Failure> {
    session.check_participants()?;
    if !session.validate() {
        return Err(Failure::Invalid);
    }
    let (tx, rx) = bounded(QUEUE_CAPACITY);
    Ok((
        LocalAuthority {
            session: session.clone(),
            tx,
            ticket: 0,
        },
        Mailbox { session, rx },
    ))
}
impl Authority for LocalAuthority {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
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
        let reply = receive_reply_until(&rx, deadline)?;
        if reply.ticket != self.ticket || reply.request != *request {
            return Err(NativeError::AuthorityBinding);
        }
        let outcome = reply.result.map_err(Failure::native)?;
        let Query::Movement(m) = &request.query else {
            return Err(NativeError::AuthorityBinding);
        };
        validate_outcome(m, &outcome).map_err(Failure::native)?;
        Ok(AuthorityReply {
            request: request.clone(),
            facts: Facts::Movement(outcome),
        })
    }
}

fn receive_reply_until(rx: &Receiver<Reply>, deadline: Instant) -> Result<Reply, NativeError> {
    let unavailable = || NativeError::AuthorityUnavailable("local geometry reply unavailable");
    let reply = rx.recv_deadline(deadline).map_err(|_| unavailable())?;
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
    pub(super) fn serve(
        &self,
        limit: usize,
        mut geometry: impl FnMut(&MovementRequest, Body) -> Result<MovementOutcome, Failure>,
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
                let Query::Movement(m) = &work.request.query else {
                    return Err(Failure::UnsupportedQuery);
                };
                let outcome = geometry(m, body)?;
                validate_outcome(m, &outcome)?;
                Ok(outcome)
            });
            if Instant::now() >= work.deadline {
                continue;
            }
            // Receiver cancellation is ordinary; never block the renderer on a stalled driver.
            let _ = work.reply.try_send(Reply {
                ticket: work.ticket,
                request: work.request,
                result,
            });
        }
        count
    }
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
pub(super) trait Corridor {
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
) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(b"benilla-local-dry-planar-adt-corridor-v2\0");
    hash.update(map.to_le_bytes());
    hash.update(wdt);
    for ((x, y), digest) in tiles {
        hash.update(x.to_le_bytes());
        hash.update(y.to_le_bytes());
        hash.update(digest);
    }
    hash.finalize().into()
}
