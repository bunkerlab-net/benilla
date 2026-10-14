// SPDX-License-Identifier: GPL-3.0-or-later
//! Additional static-product veto in the current corridor. A VMap miss is never
//! positive authority: the existing support/body/participant gates still run first.
use super::{
    core::{Body, Corridor, Failure, Session},
    products::vmap::{RayRequest, StaticRay, VmapSnapshot},
};
use benilla_world_runtime::native::AuthorityRequest;
use sha2::{Digest, Sha256};
use std::time::Instant;

pub(super) struct SourceVmapProvider {
    session: Session,
    raw_geometry: [u8; 32],
    snapshot: VmapSnapshot,
}
impl SourceVmapProvider {
    pub(super) fn bind(
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
    pub(super) fn corridor<'a, C: Corridor>(
        &'a self,
        session: &Session,
        request: &'a AuthorityRequest,
        inner: C,
        deadline: Instant,
    ) -> Result<SourceVmapCorridor<'a, C>, Failure> {
        if session.world != self.session.world
            || session.profile != self.session.profile
            || session.policy != self.session.policy
            || session.content != self.session.content
            || session.geometry != self.session.geometry
            || session.map != self.session.map
            || session.player != self.session.player
            || session.bodies != self.session.bodies
        {
            return Err(Failure::Binding);
        }
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
    h.update(b"benilla-raw-plus-static-vmap-corridor-v1");
    h.update(raw);
    h.update(b.map.to_le_bytes());
    h.update(b.content);
    h.update(b.source_epoch);
    h.update(b.inventory);
    h.update(b.policy);
    h.finalize().into()
}
