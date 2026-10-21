// SPDX-License-Identifier: GPL-3.0-or-later
//! Refusal-only attempts on an actual warm callback, before its successful staging.
//! Controlled authored scene/source changes never substitute a NoPath or clearance.
use super::{
    core::Failure,
    products::{
        source_height::{
            inventory::InventoryCompatibility, CompositionError, PreparedSourceHeight,
        },
        vmap::installed_source_fixture as input,
    },
    real_corridor_fixture::installed_source::InstalledFixture,
    source_route_filter::resident::{ResidentFilterError, ResidentFilterProvider},
    source_route_resident_local::ledger::ResidentLedger,
};
use avian3d::prelude::{Collider, CollisionLayers, RigidBody};
use benilla_assets::{coords::wow_to_bevy, AdtTile};
use benilla_world::terrain_stream::NativeStaticMapIndex;
use benilla_world_runtime::native::{AuthorityRequest, Facts, MotionQuery, Query};
use bevy::prelude::{Assets, Transform, Vec3};
use std::{
    cell::Cell,
    collections::BTreeMap,
    time::{Duration, Instant},
};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}

#[derive(Default)]
struct Refusals {
    second: bool,
    attempts: usize,
    emitted: usize,
}
impl Refusals {
    #[allow(clippy::too_many_arguments)]
    fn attempt(
        &mut self,
        ledger: &mut ResidentLedger,
        geometry: &mut InstalledFixture,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        request: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        after: impl FnOnce(),
    ) -> Result<Facts, ResidentFilterError> {
        let before = ledger.diagnostic();
        let constructors = provider.retained_constructor_count();
        geometry.advance_physics();
        let result = if self.second {
            ledger.certify_second_retained_observed(
                geometry.app.world_mut(),
                compatibility,
                provider,
                request,
                401,
                deadline,
                cancelled,
                after,
            )
        } else {
            ledger.certify_retained_observed(
                geometry.app.world_mut(),
                compatibility,
                provider,
                request,
                201,
                deadline,
                cancelled,
                after,
            )
        };
        self.attempts += 1;
        self.emitted += usize::from(result.is_ok());
        assert_eq!(
            ledger.diagnostic(),
            before,
            "every failed concrete attempt leaves the full ledger unchanged"
        );
        assert_eq!(provider.retained_constructor_count(), constructors);
        result
    }
}

fn expect(
    result: Result<Facts, ResidentFilterError>,
    expected: impl FnOnce(&ResidentFilterError) -> bool,
) {
    match result {
        Err(error) => assert!(expected(&error), "wrong retained refusal stage: {error:?}"),
        Ok(_) => panic!("a retained refusal emitted successful Facts"),
    }
}

/// Only the authored temporary navigation archive is rewritten. Restore even if
/// the synchronous certifier or any assertion unwinds after the mutation hook.
struct RestoreSource<'a> {
    install: &'a input::Install,
    original: Vec<(String, Vec<u8>)>,
    before: BTreeMap<String, Vec<u8>>,
}
impl<'a> RestoreSource<'a> {
    fn new(install: &'a input::Install) -> Self {
        Self {
            install,
            original: input::navigation_entries(),
            before: install.bytes(),
        }
    }
    fn change_wdt(&self) {
        let mut changed = self.original.clone();
        changed
            .iter_mut()
            .find(|(name, _)| name == input::WDT)
            .unwrap()
            .1[20] ^= 1;
        self.install.write(&changed);
        assert_ne!(self.install.bytes(), self.before);
    }
}
impl Drop for RestoreSource<'_> {
    fn drop(&mut self) {
        self.install.write(&self.original);
        assert_eq!(self.install.bytes(), self.before);
    }
}

#[allow(clippy::too_many_arguments)]
fn probe_inner(
    second: bool,
    ledger: &mut ResidentLedger,
    geometry: &mut InstalledFixture,
    install: &input::Install,
    height: &PreparedSourceHeight<'_, '_, '_>,
    compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
    provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
    request: &AuthorityRequest,
) {
    let Query::SourceMotion(MotionQuery::Route(route)) = &request.query else {
        panic!("refusal probe requires the actual retained Route callback")
    };
    assert_eq!(
        route.request.prior_cache.as_ref().unwrap().revision,
        if second { 2 } else { 1 }
    );
    assert_eq!(ledger.counts(), (1, 1));
    let original_ledger = ledger.diagnostic();
    let original_source = install.bytes();
    let reached = Cell::new(0);
    let mut refused = Refusals {
        second,
        ..Refusals::default()
    };

    // Public asset IDs suffice; no fixture-private tile handles or visibility change.
    let removed: Vec<_> = {
        let mut assets = geometry.app.world_mut().resource_mut::<Assets<AdtTile>>();
        let ids: Vec<_> = assets.iter().map(|(id, _)| id).collect();
        assert!(
            !ids.is_empty(),
            "loaded authored terrain must exist before removal"
        );
        ids.into_iter()
            .map(|id| (id, assets.remove(id).unwrap()))
            .collect()
    };
    let result = refused.attempt(
        ledger,
        geometry,
        compatibility,
        provider,
        request,
        deadline(),
        || false,
        || reached.set(reached.get() + 1),
    );
    {
        let mut assets = geometry.app.world_mut().resource_mut::<Assets<AdtTile>>();
        for (id, tile) in removed {
            assets.insert(id, tile).unwrap();
        }
    }
    geometry.advance_physics();
    expect(result, |error| {
        matches!(
            error,
            ResidentFilterError::Context(Failure::MissingGeometry)
        )
    });
    assert_eq!(reached.get(), 0);

    // Enclose the actual origin/goal corridor in an uncatalogued static obstacle.
    // The exact refusal is unsupported collider membership, not a synthetic NoPath.
    let to_array = |point: benilla_sim::creature_ai::Point| {
        [point.x.as_f32(), point.y.as_f32(), point.z.as_f32()]
    };
    let from = wow_to_bevy(to_array(route.request.origin));
    let to = wow_to_bevy(to_array(route.request.goal));
    assert_ne!(from, to, "actual retained corridor must be nonzero");
    let dimensions = (to - from).abs() + Vec3::new(2., 3., 2.);
    let obstacle = geometry
        .app
        .world_mut()
        .spawn((
            Collider::cuboid(dimensions.x, dimensions.y, dimensions.z),
            RigidBody::Static,
            CollisionLayers::DEFAULT,
            Transform::from_translation((from + to) * 0.5 + Vec3::Y * 1.5),
        ))
        .id();
    let result = refused.attempt(
        ledger,
        geometry,
        compatibility,
        provider,
        request,
        deadline(),
        || false,
        || reached.set(reached.get() + 1),
    );
    assert!(geometry.app.world_mut().despawn(obstacle));
    geometry.advance_physics();
    expect(result, |error| {
        matches!(
            error,
            ResidentFilterError::Context(Failure::UnsupportedNavigation)
        )
    });
    assert_eq!(reached.get(), 0);

    let replacement = input::raw_inventory(install);
    let replacement_compatibility = height
        .match_raw_inventory(&replacement, deadline(), || false)
        .unwrap();
    assert_eq!(
        replacement.source_digest(),
        compatibility.owner().inventory_digest()
    );
    assert_eq!(
        replacement_compatibility.revision(),
        compatibility.revision()
    );
    assert_eq!(
        replacement_compatibility.owner().receipt_digest(),
        compatibility.owner().receipt_digest()
    );
    assert!(!replacement_compatibility
        .owner()
        .same_owner(compatibility.owner()));
    let original_index = geometry
        .app
        .world_mut()
        .remove_resource::<NativeStaticMapIndex>()
        .unwrap();
    geometry
        .app
        .insert_resource(NativeStaticMapIndex::from_inventory(replacement).unwrap());
    let result = refused.attempt(
        ledger,
        geometry,
        compatibility,
        provider,
        request,
        deadline(),
        || false,
        || reached.set(reached.get() + 1),
    );
    geometry.app.insert_resource(original_index);
    geometry.advance_physics();
    expect(result, |error| {
        matches!(error, ResidentFilterError::Context(Failure::Binding))
    });
    assert_eq!(
        reached.get(),
        0,
        "scene and owner failures precede both-checks hook"
    );

    let cancelled = Cell::new(false);
    expect(
        refused.attempt(
            ledger,
            geometry,
            compatibility,
            provider,
            request,
            deadline(),
            || cancelled.get(),
            || {
                reached.set(reached.get() + 1);
                cancelled.set(true);
            },
        ),
        |error| {
            matches!(
                error,
                ResidentFilterError::Source(CompositionError::Cancelled)
            )
        },
    );
    assert_eq!(
        reached.get(),
        1,
        "cancellation follows corridor and source segment LOS"
    );

    let source_restore = RestoreSource::new(install);
    let current_deadline = deadline();
    let result = refused.attempt(
        ledger,
        geometry,
        compatibility,
        provider,
        request,
        current_deadline,
        || false,
        || {
            reached.set(reached.get() + 1);
            source_restore.change_wdt();
        },
    );
    drop(source_restore);
    expect(result, |error| {
        matches!(
            error,
            ResidentFilterError::Source(CompositionError::TerrainSource)
        )
    });
    assert!(
        Instant::now() < current_deadline,
        "expiry cannot mask source mutation"
    );
    assert_eq!(
        reached.get(),
        2,
        "mutation follows corridor and source segment LOS"
    );
    assert_eq!(install.bytes(), original_source);

    // One fixed original 30-second deadline, never renewed after either check.
    let original_deadline = deadline();
    expect(
        refused.attempt(
            ledger,
            geometry,
            compatibility,
            provider,
            request,
            original_deadline,
            || false,
            || {
                assert!(
                    Instant::now() < original_deadline,
                    "both checks precede original expiry"
                );
                reached.set(reached.get() + 1);
                while Instant::now() < original_deadline {
                    std::thread::sleep(original_deadline.saturating_duration_since(Instant::now()));
                }
            },
        ),
        |error| matches!(error, ResidentFilterError::Context(Failure::Deadline)),
    );
    assert_eq!(
        reached.get(),
        3,
        "expiry follows corridor and source segment LOS"
    );
    assert_eq!(refused.attempts, 6);
    assert_eq!(refused.emitted, 0);
    assert_eq!(ledger.diagnostic(), original_ledger);
    assert_eq!(ledger.counts(), (1, 1));
    assert_eq!(install.bytes(), original_source);
    if second {
        eprintln!("resident second retained concrete refusals: attempts=6 emitted=0 post-segment-hooks=3 committed-cache2=1 staged-cache3=0");
    } else {
        eprintln!("resident retained concrete refusals: attempts=6 emitted=0 post-segment-hooks=3 committed-cold=1 staged-warm=0");
    }
}

#[test]
fn resident_controller_one_retained_refuses_concrete_source_changes_before_fresh_success() {
    super::source_route_resident_controller::one_retained_refusals_probe();
}

#[allow(clippy::too_many_arguments)]
pub(super) fn probe(
    ledger: &mut ResidentLedger,
    geometry: &mut InstalledFixture,
    install: &input::Install,
    height: &PreparedSourceHeight<'_, '_, '_>,
    compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
    provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
    request: &AuthorityRequest,
) {
    probe_inner(
        false,
        ledger,
        geometry,
        install,
        height,
        compatibility,
        provider,
        request,
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn probe_second(
    ledger: &mut ResidentLedger,
    geometry: &mut InstalledFixture,
    install: &input::Install,
    height: &PreparedSourceHeight<'_, '_, '_>,
    compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
    provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
    request: &AuthorityRequest,
) {
    probe_inner(
        true,
        ledger,
        geometry,
        install,
        height,
        compatibility,
        provider,
        request,
    );
}
