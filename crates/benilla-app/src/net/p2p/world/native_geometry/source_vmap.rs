// SPDX-License-Identifier: GPL-3.0-or-later
//! Static VMap movement veto and separately selected source LOS. Movement retains
//! positive support/body proofs; source LOS requires its explicit frozen policy and
//! independent participant exclusion, never a walking-corridor approximation.
use super::{
    core::{Body, Corridor, Failure, Session},
    products::vmap::{RayRequest, StaticRay, VmapSnapshot},
};
use benilla_sim::creature_ai::movement::Check;
use benilla_world_runtime::native::{
    AuthorityRequest, CombatFacts, CombatPhase, CombatQuery, Facts, MotionFacts, MotionQuery,
    Query, SourceSpellFacts, SourceSpellPhase, SourceSpellQuery,
};
use sha2::{Digest, Sha256};
use std::time::Instant;

pub(crate) struct SourceVmapProvider {
    session: Session,
    raw_geometry: [u8; 32],
    snapshot: VmapSnapshot,
}
impl SourceVmapProvider {
    pub(crate) fn bind(
        session: &Session,
        snapshot: VmapSnapshot,
        raw_geometry: [u8; 32],
    ) -> Result<Self, Failure> {
        let expected = geometry_revision(raw_geometry, snapshot.binding());
        Self::bind_revision(session, snapshot, raw_geometry, expected)
    }
    pub(super) fn bind_with_navigation(
        session: &Session,
        snapshot: VmapSnapshot,
        raw_geometry: [u8; 32],
        navigation: super::products::navigation::SnapshotBinding,
    ) -> Result<Self, Failure> {
        let b = snapshot.binding();
        if b.map != navigation.map
            || b.content != navigation.content
            || b.source_epoch != navigation.source_epoch
            || b.inventory != navigation.inventory
        {
            return Err(Failure::Binding);
        }
        let expected = super::source_navigation::geometry_revision(raw_geometry, b, navigation);
        Self::bind_revision(session, snapshot, raw_geometry, expected)
    }
    fn bind_revision(
        session: &Session,
        snapshot: VmapSnapshot,
        raw_geometry: [u8; 32],
        expected: [u8; 32],
    ) -> Result<Self, Failure> {
        let b = snapshot.binding();
        if b.map != session.map
            || b.content != session.content
            || b.source_epoch == [0; 32]
            || raw_geometry == [0; 32]
            || session.geometry != expected
        {
            return Err(Failure::Binding);
        }
        Ok(Self {
            session: session.clone(),
            raw_geometry,
            snapshot,
        })
    }
    pub(super) fn raw_geometry(&self) -> [u8; 32] {
        self.raw_geometry
    }
    /// WorldObject::IsWithinLOSInMap -> Map::IsInLineOfSight for the explicitly
    /// installed dry static domain. The independent participant proof excludes the
    /// dynamic tree; no movement corridor, navigation result or renderer ray substitutes.
    pub(crate) fn line_of_sight(
        &self,
        session: &Session,
        request: &AuthorityRequest,
        deadline: Instant,
    ) -> Result<Facts, Failure> {
        self.check_session(session)?;
        let npc_body = session.body(request)?;
        let player_body = session.player.ok_or(Failure::UnresolvedBody)?.body;
        let (player, npc, creature_first) = match &request.query {
            Query::SourceSpell(SourceSpellQuery::LineOfSight(q))
                if matches!(
                    q.phase,
                    SourceSpellPhase::Prepare | SourceSpellPhase::ReleaseCheck
                ) =>
            {
                (q.player_position, q.target_position, false)
            }
            Query::SourceMotion(MotionQuery::Combat(CombatQuery::NpcLineOfSight(q)))
                if q.phase == CombatPhase::Sight =>
            {
                (q.player_pose.position, q.creature_pose.position, true)
            }
            _ => return Err(Failure::UnsupportedQuery),
        };
        // Source binary32 addition, separately using each unit's collision height.
        // Combat reach, render height and the other endpoint's body are not substitutes.
        let eye = |p: benilla_sim::creature_ai::Point, body: Body| {
            [
                p.x.as_f32(),
                p.y.as_f32(),
                p.z.as_f32() + body.height.as_f32(),
            ]
            .map(f32::to_bits)
        };
        let (p, n) = (eye(player, player_body), eye(npc, npc_body));
        let query = RayRequest {
            snapshot: self.snapshot.binding(),
            ticket: 1,
            from: if creature_first { n } else { p },
            to: if creature_first { p } else { n },
            // Both source CheckCast and CreatureAI sight callers ignore MOD_M2.
            ignore_m2: true,
        };
        let reply = self
            .snapshot
            .query(query, deadline)
            .map_err(|_| Failure::MissingGeometry)?;
        if reply.request != query {
            return Err(Failure::Binding);
        }
        let value = match reply.result {
            StaticRay::Miss => Check::Passed,
            StaticRay::Hit { .. } => Check::Failed,
        };
        Ok(if creature_first {
            Facts::SourceMotion(MotionFacts::Combat(CombatFacts::NpcLineOfSight(value)))
        } else {
            Facts::SourceSpell(SourceSpellFacts::LineOfSight(value))
        })
    }
    fn check_session(&self, session: &Session) -> Result<(), Failure> {
        if session.world != self.session.world
            || session.profile != self.session.profile
            || session.policy != self.session.policy
            || session.content != self.session.content
            || session.geometry != self.session.geometry
            || session.map != self.session.map
            || session.player != self.session.player
            || session.bodies != self.session.bodies
            || session.static_los != self.session.static_los
        {
            return Err(Failure::Binding);
        }
        Ok(())
    }
    pub(super) fn corridor<'a, C: Corridor>(
        &'a self,
        session: &Session,
        request: &'a AuthorityRequest,
        inner: C,
        deadline: Instant,
    ) -> Result<SourceVmapCorridor<'a, C>, Failure> {
        self.check_session(session)?;
        let body = self.session.body(request)?;
        if session.body(request)? != body {
            return Err(Failure::Binding);
        }
        Ok(SourceVmapCorridor {
            provider: self,
            request,
            body,
            inner,
            deadline,
            ticket: 0,
        })
    }
}
pub(super) struct SourceVmapCorridor<'a, C> {
    provider: &'a SourceVmapProvider,
    request: &'a AuthorityRequest,
    body: Body,
    inner: C,
    deadline: Instant,
    ticket: u64,
}
impl<C: Corridor> Corridor for SourceVmapCorridor<'_, C> {
    fn certify(&mut self, from: [f32; 3], to: [f32; 3], body: Body) -> Result<(), Failure> {
        // Revalidate the original request/body, without allowing a product snapshot to
        // synthesize non-render participants or a source-epoch admission proof.
        if body != self.body || self.provider.session.body(self.request)? != body {
            return Err(Failure::Binding);
        }
        self.inner.certify(from, to, body)?;
        let eye = |p: [f32; 3]| [p[0], p[1], p[2] + body.height.as_f32()].map(f32::to_bits);
        self.ticket = self.ticket.checked_add(1).ok_or(Failure::Invalid)?;
        let query = RayRequest {
            snapshot: self.provider.snapshot.binding(),
            ticket: self.ticket,
            from: eye(from),
            to: eye(to),
            ignore_m2: false,
        };
        let reply = self
            .provider
            .snapshot
            .query(query, self.deadline)
            .map_err(|_| Failure::MissingGeometry)?;
        if reply.request != query {
            return Err(Failure::Binding);
        }
        match reply.result {
            StaticRay::Miss => Ok(()),
            StaticRay::Hit { .. } => Err(Failure::Obstacle),
        }
    }
}

/// Native request geometry binds both the raw continuous-support proof and loaded VMap policy.
pub(crate) fn geometry_revision(
    raw: [u8; 32],
    b: super::products::vmap::SnapshotBinding,
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"benilla-raw-plus-static-vmap-corridor-los-v2");
    h.update(raw);
    h.update(b.map.to_le_bytes());
    h.update(b.content);
    h.update(b.source_epoch);
    h.update(b.inventory);
    h.update(b.policy);
    h.finalize().into()
}
