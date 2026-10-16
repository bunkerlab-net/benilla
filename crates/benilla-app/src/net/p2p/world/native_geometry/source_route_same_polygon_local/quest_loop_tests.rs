// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual installed Service/LocalCorridor plus the stock quest controller. Only
//! Environment, NearPoint and same-polygon Route are loaded geometry here. Cell
//! membership, directional visibility, accessibility, spell LOS/impact visibility
//! and player pose acceptance remain explicitly authored spatial observations.
use super::super::{
    core,
    real_corridor_fixture::Fixture as Geometry,
    route_commit::{RouteCommitBoundary, RouteCommitNotice, RouteCommitSink},
    source_route_same_polygon::two_owner_tests::quest_inputs,
    Service,
};
use crate::net::p2p::world::loaded;
use benilla_world_runtime::native::{AuthorityReply, NativeError};
use std::{
    collections::BTreeMap,
    sync::mpsc::{self, Receiver, SyncSender},
    time::{Duration, Instant},
};

struct Inspection {
    boundary: RouteCommitBoundary,
    routes: Vec<AuthorityReply>,
    acknowledged: bool,
    reply: SyncSender<loaded::Heads>,
}
#[derive(Clone)]
struct Inspect(SyncSender<Inspection>);
impl Inspect {
    fn get(
        &self,
        boundary: &RouteCommitBoundary,
        routes: &[AuthorityReply],
        acknowledged: bool,
    ) -> loaded::Heads {
        let (reply, receive) = mpsc::sync_channel(0);
        self.0
            .send(Inspection {
                boundary: boundary.clone(),
                routes: routes.to_vec(),
                acknowledged,
                reply,
            })
            .unwrap();
        receive.recv_timeout(Duration::from_secs(20)).unwrap()
    }
}
fn inspect_native(geometry: &Geometry, receive: &Receiver<Inspection>) {
    while let Ok(request) = receive.try_recv() {
        let service = geometry.app.world().non_send_resource::<Service>();
        let provider = service.same_polygon_route.as_ref().unwrap();
        let mut committed = BTreeMap::new();
        for head in request.boundary.heads() {
            let actual = provider.committed_candidate_for(head.npc());
            assert_eq!(actual.map(|(_, cache)| cache), head.cache());
            if let Some((pointer, cache)) = actual {
                committed.insert(head.npc(), (pointer, cache.clone()));
            }
        }
        let candidates = request
            .routes
            .iter()
            .map(|r| {
                provider
                    .candidate_identity(r)
                    .expect("actual native candidate")
            })
            .collect();
        if request.acknowledged {
            assert_eq!(provider.retained_candidate_count(), committed.len());
            assert_eq!(provider.certified_candidate_count(), committed.len());
        } else {
            assert!(provider.retained_candidate_count() <= 4);
        }
        request
            .reply
            .send(loaded::Heads {
                committed,
                candidates,
            })
            .unwrap();
    }
}

struct CheckedSink {
    inner: core::MailboxRouteCommitSink,
    session: core::Session,
    inspect: Inspect,
    observed: loaded::Observed,
    boundary: Option<RouteCommitBoundary>,
}
impl RouteCommitSink for CheckedSink {
    fn initialize(&mut self, boundary: &RouteCommitBoundary) -> Result<(), NativeError> {
        self.inner.initialize(boundary)?;
        assert!(self.inspect.get(boundary, &[], true).committed.is_empty());
        self.boundary = Some(boundary.clone());
        Ok(())
    }
    fn deliver(&mut self, notice: &RouteCommitNotice) -> Result<(), NativeError> {
        let previous = self.boundary.as_ref().unwrap();
        assert_eq!(notice.boundary().tick(), previous.tick() + 1);
        assert_eq!(notice.before(), previous.state());
        assert_eq!(notice.previous_record(), previous.record());
        let before = self.inspect.get(previous, notice.routes(), false);
        for route in notice.routes() {
            core::validate_facts(&self.session, &route.request, &route.facts).unwrap();
            let npc = route.request.binding.npc.unwrap();
            let calculation = loaded::calculation(route);
            assert_eq!(calculation.after_cache.binding.identity.creature, npc.spawn);
            assert_eq!(
                calculation.after_cache.binding.owner_generation,
                npc.generation
            );
            if let Some((_, cache)) = before.committed.get(&npc) {
                assert_eq!(calculation.prior_cache.as_deref(), Some(cache));
                assert_eq!(calculation.after_cache.revision, cache.revision + 1);
            } else {
                assert!(calculation.prior_cache.is_none());
                assert_eq!(calculation.after_cache.revision, 1);
            }
        }
        self.inner.deliver(notice)?;
        let after = self.inspect.get(notice.boundary(), notice.routes(), true);
        for (route, pointer) in notice.routes().iter().zip(before.candidates) {
            if let Some((committed, cache)) =
                after.committed.get(&route.request.binding.npc.unwrap())
            {
                assert_eq!(
                    *committed, pointer,
                    "only the exact native proposal may be acknowledged"
                );
                assert_eq!(*cache, loaded::calculation(route).after_cache);
            }
        }
        self.inner.deliver(notice)?;
        assert_eq!(
            self.inspect.get(notice.boundary(), &[], true),
            loaded::Heads {
                committed: after.committed,
                candidates: vec![]
            }
        );
        self.boundary = Some(notice.boundary().clone());
        self.observed.committed.lock().unwrap().push(notice.clone());
        Ok(())
    }
}

fn installed(hostile: u64, giver: u64, order: [u64; 2], faults: bool) {
    let mut geometry = Geometry::new();
    let (profile, mut session, products) = quest_inputs(
        geometry.products(),
        geometry.raw_geometry_revision(),
        &geometry
            .app
            .world()
            .resource::<benilla_assets::MapCatalogRes>()
            .0,
        hostile,
        giver,
        order,
    );
    profile.validate().unwrap();
    let fixture = loaded::Fixture::new(profile.clone());
    let player = fixture.player();
    // The controller derives its actual selected identity from ProfileStore.
    // Loaded owner policies must bind that identity, never the standalone fixture ID.
    session.player_environment =
        Some(core::PlayerEnvironmentPolicy::from_profile(&profile, player).unwrap());
    session.same_polygon_route = Some(
        core::SamePolygonRoutePolicy::from_profile(&profile, player, products.binding().unwrap())
            .unwrap(),
    );
    super::super::plugin(&mut geometry.app);
    let (local, sink) = super::super::install_with_same_polygon(
        geometry.app.world_mut(),
        session.clone(),
        &profile,
        player,
        products,
    )
    .unwrap();
    let (send, receive) = mpsc::sync_channel(0);
    let inspect = Inspect(send);
    let observed = loaded::Observed::default();
    let worker = std::thread::spawn(move || {
        let authority = loaded::Authority::new(local, observed.clone());
        let sink = CheckedSink {
            inner: sink,
            session,
            inspect: inspect.clone(),
            observed: observed.clone(),
            boundary: None,
        };
        fixture.run(
            authority,
            sink,
            observed,
            |boundary, routes, acknowledged| inspect.get(boundary, routes, acknowledged),
            faults,
        );
    });
    let deadline = Instant::now() + Duration::from_secs(90);
    while !worker.is_finished() && Instant::now() < deadline {
        geometry.advance_physics();
        inspect_native(&geometry, &receive);
        std::thread::sleep(Duration::from_millis(1));
    }
    if !worker.is_finished() {
        super::super::uninstall(geometry.app.world_mut());
    }
    drop(receive);
    worker.join().unwrap();
    super::super::uninstall(geometry.app.world_mut());
}

#[test]
fn installed_loaded_quest_loop_completes_stock_commands_in_both_orders_and_role_ids() {
    for (hostile, giver) in [(2, 3), (3, 2)] {
        for order in [[hostile, giver], [giver, hostile]] {
            installed(hostile, giver, order, false);
        }
    }
}
#[test]
fn installed_loaded_quest_loop_rolls_back_late_giver_and_retries_durable_publication() {
    installed(2, 3, [2, 3], true);
}
