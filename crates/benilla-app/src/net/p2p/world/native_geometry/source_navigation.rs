// SPDX-License-Identifier: GPL-3.0-or-later
//! Conservative navigation veto, composed after current support/body/participants and VMap.
use super::{
    core::{Body, Corridor, Failure, Session},
    products::{
        navigation::{NavigationSnapshot, RouteRequest, SnapshotBinding},
        vmap::VmapSnapshot,
    },
    source_vmap::{SourceVmapCorridor, SourceVmapProvider},
};
use benilla_world_runtime::native::AuthorityRequest;
use sha2::{Digest, Sha256};
use std::time::Instant;
pub(super) struct SourceNavigationProvider {
    vmap: SourceVmapProvider,
    snapshot: NavigationSnapshot,
}
impl SourceNavigationProvider {
    pub(super) fn bind(
        session: &Session,
        vmap: VmapSnapshot,
        snapshot: NavigationSnapshot,
        raw: [u8; 32],
    ) -> Result<Self, Failure> {
        let a = vmap.binding();
        let b = snapshot.binding();
        if a.map != b.map
            || a.content != b.content
            || a.source_epoch != b.source_epoch
            || a.inventory != b.inventory
        {
            return Err(Failure::Binding);
        }
        let vmap = SourceVmapProvider::bind_with_navigation(session, vmap, raw, b)?;
        Ok(Self { vmap, snapshot })
    }
    pub(super) fn raw_geometry(&self) -> [u8; 32] {
        self.vmap.raw_geometry()
    }
    #[allow(
        dead_code,
        reason = "production app dispatch, covered through the same VMap method in headless tests"
    )]
    pub(super) fn line_of_sight(
        &self,
        session: &Session,
        request: &AuthorityRequest,
        deadline: Instant,
    ) -> Result<benilla_world_runtime::native::Facts, Failure> {
        self.vmap.line_of_sight(session, request, deadline)
    }
    pub(super) fn corridor<'a, C: Corridor>(
        &'a self,
        session: &Session,
        request: &'a AuthorityRequest,
        inner: C,
        deadline: Instant,
    ) -> Result<SourceNavigationCorridor<'a, C>, Failure> {
        let body = session.body(request)?;
        let agent = self.snapshot.agent_dimensions();
        if body.height.as_f32() > agent[0] || body.radius.as_f32() > agent[1] {
            return Err(Failure::UnresolvedBody);
        }
        let inner = self.vmap.corridor(session, request, inner, deadline)?;
        Ok(SourceNavigationCorridor {
            provider: self,
            inner,
            body,
            deadline,
            ticket: 0,
        })
    }
}
pub(super) struct SourceNavigationCorridor<'a, C> {
    provider: &'a SourceNavigationProvider,
    inner: SourceVmapCorridor<'a, C>,
    body: Body,
    deadline: Instant,
    ticket: u64,
}
impl<C: Corridor> Corridor for SourceNavigationCorridor<'_, C> {
    fn certify(&mut self, from: [f32; 3], to: [f32; 3], body: Body) -> Result<(), Failure> {
        if body != self.body {
            return Err(Failure::Binding);
        }
        // Raw support, body sweep, participant audit, and static VMap veto remain mandatory.
        self.inner.certify(from, to, body)?;
        self.ticket = self.ticket.checked_add(1).ok_or(Failure::Invalid)?;
        let request = RouteRequest {
            snapshot: self.provider.snapshot.binding(),
            ticket: self.ticket,
            from: from.map(f32::to_bits),
            to: to.map(f32::to_bits),
        };
        let reply = self
            .provider
            .snapshot
            .query(request, self.deadline)
            .map_err(Failure::navigation)?;
        if reply.request != request {
            return Err(Failure::Binding);
        }
        // The existing solver certifies a direct segment. Snapping endpoints or routing
        // around an excluded polygon cannot certify that segment. Reject conservatively.
        let r = reply.route;
        if !r.start.over
            || !r.end.over
            || r.start.point != request.from
            || r.end.point != request.to
        {
            return Err(Failure::UnsupportedNavigation);
        }
        if from != to
            && (r.corners.len() != 2
                || r.corners[0].point != request.from
                || r.corners[1].point != request.to)
        {
            return Err(Failure::UnsupportedNavigation);
        }
        if Instant::now() >= self.deadline {
            return Err(Failure::Deadline);
        }
        Ok(())
    }
}
pub(crate) fn geometry_revision(
    raw: [u8; 32],
    vmap: super::products::vmap::SnapshotBinding,
    nav: SnapshotBinding,
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"benilla-raw-static-vmap-dry-navigation-v1");
    h.update(super::source_vmap::geometry_revision(raw, vmap));
    h.update(nav.map.to_le_bytes());
    for value in [nav.content, nav.source_epoch, nav.inventory, nav.policy] {
        h.update(value)
    }
    h.finalize().into()
}
