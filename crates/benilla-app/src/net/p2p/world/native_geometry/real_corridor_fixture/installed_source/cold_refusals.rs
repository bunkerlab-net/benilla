// SPDX-License-Identifier: GPL-3.0-or-later
//! Refusal-only concrete tests. No successful Route facts are returned to runtime.
use super::super::super::{
    source_pathfinder_query::resident_joined_tests as runtime,
    source_route_filter::resident::{
        chase_los::water::ResidentChaseWaterDependency, ResidentFilterDependency,
        ResidentFilterError, ResidentFilterProvider,
    },
    source_route_resident_local::{self as cold, ResidentColdCandidate},
};
use super::*;
use benilla_world_runtime::native::{AuthorityRequest, Facts, NativeError, NativeWorld};

type Prerequisites = (ResidentFilterDependency, ResidentChaseWaterDependency);
type Certification = Result<(ResidentColdCandidate, Facts), ResidentFilterError>;
const REFUSED: &str = "resident concrete refusal checks completed without Facts";

#[derive(Default)]
struct Refusals {
    attempts: usize,
    emitted: usize,
}
impl Refusals {
    fn expect(
        &mut self,
        result: Certification,
        expected: impl FnOnce(&ResidentFilterError) -> bool,
    ) {
        self.attempts += 1;
        match result {
            Err(error) => assert!(expected(&error), "wrong refusal stage: {error:?}"),
            Ok(_) => {
                self.emitted += 1;
                panic!("a refusal case emitted a candidate and Facts");
            }
        }
    }
    fn finish(&self, result: Result<NativeWorld, NativeError>, attempts: usize) {
        assert!(matches!(
            result,
            Err(NativeError::AuthorityUnavailable(REFUSED))
        ));
        assert_eq!(self.attempts, attempts, "the live Route callback ran");
        assert_eq!(self.emitted, 0);
    }
}
fn binding(error: &ResidentFilterError) -> bool {
    matches!(error, ResidentFilterError::Context(Failure::Binding))
}
fn missing(error: &ResidentFilterError) -> bool {
    matches!(
        error,
        ResidentFilterError::Context(Failure::MissingGeometry)
    )
}
#[allow(clippy::too_many_arguments)]
fn attempt(
    fixture: &mut InstalledFixture,
    compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
    provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
    context: &AuthorityRequest,
    (constructor, water): Prerequisites,
    deadline: Instant,
    cancelled: impl Fn() -> bool,
    after: impl FnOnce(),
) -> Certification {
    fixture.advance_physics();
    cold::certify(
        fixture.app.world_mut(),
        compatibility,
        provider,
        context,
        constructor,
        water,
        105,
        deadline,
        cancelled,
        after,
    )
}
fn prerequisites(
    provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
    context: &AuthorityRequest,
) -> Prerequisites {
    runtime::prerequisites(provider, context, 101, runtime::until())
}

#[test]
fn resident_cold_certification_refuses_scene_and_owner_changes() {
    runtime::with_route_fixture(
        |install, height, compatibility, inventory, policy, p, query, _| {
            let s = runtime::session(&p);
            let mut provider =
                ResidentFilterProvider::bind_with_policy(&s, &p, height, query, policy).unwrap();
            let mut fixture = InstalledFixture::new(install.path(), inventory);
            let mut refused = Refusals::default();
            let result = runtime::complete_cold_probe(
                &p,
                &s,
                height,
                compatibility,
                &mut provider,
                |provider, context| {
                    let reached = Cell::new(0);
                    // Obtain genuine terminal endpoints without emitting a Route or Facts.
                    let (constructor, water) = prerequisites(provider, context);
                    let source = provider
                        .observe_joined_cold(
                            constructor,
                            water,
                            context,
                            105,
                            Default::default(),
                            runtime::until(),
                            || false,
                        )
                        .unwrap();
                    let observed = provider
                        .joined_for(&source, context, runtime::until(), || false)
                        .unwrap();
                    let neighbor = (input::CENTER.0 as i32 - 1, input::CENTER.1 as i32 - 1);
                    let handle = fixture.tiles[&neighbor].id();
                    let removed = fixture
                        .app
                        .world_mut()
                        .resource_mut::<Assets<AdtTile>>()
                        .remove(handle)
                        .unwrap();
                    let ready = prerequisites(provider, context);
                    refused.expect(
                        attempt(
                            &mut fixture,
                            compatibility,
                            provider,
                            context,
                            ready,
                            runtime::until(),
                            || false,
                            || reached.set(reached.get() + 1),
                        ),
                        missing,
                    );
                    fixture
                        .app
                        .world_mut()
                        .resource_mut::<Assets<AdtTile>>()
                        .insert(handle, removed)
                        .unwrap();

                    let root = fixture.roots[&(input::CENTER.0 as i32, input::CENTER.1 as i32)];
                    let collider = fixture.app.world().get::<Collider>(root).unwrap().clone();
                    let mesh = collider.shape_scaled().as_trimesh().unwrap();
                    let mut indices = mesh.indices().to_vec();
                    indices.pop().unwrap();
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(root)
                        .insert(Collider::trimesh(mesh.vertices().to_vec(), indices));
                    let ready = prerequisites(provider, context);
                    refused.expect(
                        attempt(
                            &mut fixture,
                            compatibility,
                            provider,
                            context,
                            ready,
                            runtime::until(),
                            || false,
                            || reached.set(reached.get() + 1),
                        ),
                        missing,
                    );
                    fixture.app.world_mut().entity_mut(root).insert(collider);

                    let [a, b] = observed
                        .terminal
                        .points
                        .map(|point| point.map(f32::from_bits));
                    assert_ne!(a, b, "the obstacle spans a genuine nonzero swept corridor");
                    let middle = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, a[2] + 1.];
                    let obstacle = fixture
                        .app
                        .world_mut()
                        .spawn((
                            Collider::cuboid(2., 3., 0.25),
                            RigidBody::Static,
                            CollisionLayers::DEFAULT,
                            Transform::from_translation(benilla_assets::coords::wow_to_bevy(
                                middle,
                            )),
                        ))
                        .id();
                    let ready = prerequisites(provider, context);
                    refused.expect(
                        attempt(
                            &mut fixture,
                            compatibility,
                            provider,
                            context,
                            ready,
                            runtime::until(),
                            || false,
                            || reached.set(reached.get() + 1),
                        ),
                        |error| {
                            matches!(
                                error,
                                ResidentFilterError::Context(Failure::UnsupportedNavigation)
                            )
                        },
                    );
                    fixture.app.world_mut().despawn(obstacle);

                    let index = fixture
                        .app
                        .world_mut()
                        .remove_resource::<NativeStaticMapIndex>()
                        .unwrap();
                    let ready = prerequisites(provider, context);
                    refused.expect(
                        attempt(
                            &mut fixture,
                            compatibility,
                            provider,
                            context,
                            ready,
                            runtime::until(),
                            || false,
                            || reached.set(reached.get() + 1),
                        ),
                        binding,
                    );
                    fixture.app.insert_resource(index);

                    let replacement = input::raw_inventory(install);
                    let replacement_compatibility = height
                        .match_raw_inventory(&replacement, runtime::until(), || false)
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
                    fixture.app.insert_resource(
                        NativeStaticMapIndex::from_inventory(replacement).unwrap(),
                    );
                    for supplied in [compatibility, &replacement_compatibility] {
                        let ready = prerequisites(provider, context);
                        refused.expect(
                            attempt(
                                &mut fixture,
                                supplied,
                                provider,
                                context,
                                ready,
                                runtime::until(),
                                || false,
                                || reached.set(reached.get() + 1),
                            ),
                            binding,
                        );
                    }
                    assert_eq!(
                        reached.get(),
                        0,
                        "scene/owner refusals precede final-source hook"
                    );
                    Err(NativeError::AuthorityUnavailable(REFUSED))
                },
            );
            refused.finish(result, 6);
        },
    );
}

#[test]
fn resident_cold_certification_rechecks_final_source_cancel_and_expiry() {
    runtime::with_route_fixture(
        |install, height, compatibility, inventory, policy, p, query, _| {
            let s = runtime::session(&p);
            let mut provider =
                ResidentFilterProvider::bind_with_policy(&s, &p, height, query, policy).unwrap();
            let mut fixture = InstalledFixture::new(install.path(), inventory);
            let mut refused = Refusals::default();
            let result = runtime::complete_cold_probe(
                &p,
                &s,
                height,
                compatibility,
                &mut provider,
                |provider, context| {
                    let reached = Cell::new(0);
                    let ready = prerequisites(provider, context);
                    refused.expect(
                        attempt(
                            &mut fixture,
                            compatibility,
                            provider,
                            context,
                            ready,
                            Instant::now(),
                            || false,
                            || reached.set(reached.get() + 1),
                        ),
                        |error| {
                            matches!(
                                error,
                                ResidentFilterError::Source(CompositionError::Validation(
                                    Error::Deadline
                                ))
                            )
                        },
                    );
                    let ready = prerequisites(provider, context);
                    refused.expect(
                        attempt(
                            &mut fixture,
                            compatibility,
                            provider,
                            context,
                            ready,
                            runtime::until(),
                            || true,
                            || reached.set(reached.get() + 1),
                        ),
                        |error| {
                            matches!(
                                error,
                                ResidentFilterError::Source(CompositionError::Cancelled)
                            )
                        },
                    );
                    assert_eq!(reached.get(), 0);

                    let cancelled = Cell::new(false);
                    let ready = prerequisites(provider, context);
                    refused.expect(
                        attempt(
                            &mut fixture,
                            compatibility,
                            provider,
                            context,
                            ready,
                            runtime::until(),
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
                        "cancellation follows corridor AND source segment LOS"
                    );

                    let entries = input::navigation_entries();
                    let mut changed = entries.clone();
                    changed
                        .iter_mut()
                        .find(|(name, _)| name == input::WDT)
                        .unwrap()
                        .1[20] ^= 1;
                    let ready = prerequisites(provider, context);
                    let result = attempt(
                        &mut fixture,
                        compatibility,
                        provider,
                        context,
                        ready,
                        runtime::until(),
                        || false,
                        || {
                            reached.set(reached.get() + 1);
                            install.write(&changed);
                        },
                    );
                    install.write(&entries);
                    refused.expect(result, |error| {
                        matches!(
                            error,
                            ResidentFilterError::Source(CompositionError::TerrainSource)
                        )
                    });
                    assert_eq!(
                        reached.get(),
                        2,
                        "source mutation follows corridor AND source segment LOS"
                    );

                    // Keep one fixed original deadline through all source work; never extend it.
                    let deadline = Instant::now() + Duration::from_secs(30);
                    let ready = runtime::prerequisites(provider, context, 101, deadline);
                    refused.expect(
                        attempt(
                            &mut fixture,
                            compatibility,
                            provider,
                            context,
                            ready,
                            deadline,
                            || false,
                            || {
                                assert!(
                                    Instant::now() < deadline,
                                    "reach final hook before original deadline"
                                );
                                reached.set(reached.get() + 1);
                                while Instant::now() < deadline {
                                    std::thread::sleep(
                                        deadline.saturating_duration_since(Instant::now()),
                                    );
                                }
                            },
                        ),
                        |error| matches!(error, ResidentFilterError::Context(Failure::Deadline)),
                    );
                    assert_eq!(
                        reached.get(),
                        3,
                        "deadline expires only after both concrete/source checks"
                    );
                    Err(NativeError::AuthorityUnavailable(REFUSED))
                },
            );
            refused.finish(result, 5);
        },
    );
}

#[test]
fn resident_cold_certification_refuses_decoded_and_foreign_prerequisites() {
    runtime::with_route_fixture(
        |install, height, compatibility, inventory, policy, p, query, other| {
            let s = runtime::session(&p);
            let mut provider =
                ResidentFilterProvider::bind_with_policy(&s, &p, height, query, policy.clone())
                    .unwrap();
            let mut other =
                ResidentFilterProvider::bind_with_policy(&s, &p, height, other, policy).unwrap();
            let mut fixture = InstalledFixture::new(install.path(), inventory);
            let mut refused = Refusals::default();
            let result = runtime::complete_cold_probe(
                &p,
                &s,
                height,
                compatibility,
                &mut provider,
                |provider, context| {
                    let reached = Cell::new(0);
                    let decoded: AuthorityRequest =
                        serde_json::from_value(serde_json::to_value(context).unwrap()).unwrap();
                    assert_eq!(
                        decoded, *context,
                        "equal metadata does not restore a live seal"
                    );
                    let ready = prerequisites(provider, context);
                    refused.expect(
                        attempt(
                            &mut fixture,
                            compatibility,
                            provider,
                            &decoded,
                            ready,
                            runtime::until(),
                            || false,
                            || reached.set(reached.get() + 1),
                        ),
                        binding,
                    );
                    let ready = prerequisites(provider, context);
                    refused.expect(
                        attempt(
                            &mut fixture,
                            compatibility,
                            &mut other,
                            context,
                            ready,
                            runtime::until(),
                            || false,
                            || reached.set(reached.get() + 1),
                        ),
                        binding,
                    );
                    assert_eq!(reached.get(), 0);
                    Err(NativeError::AuthorityUnavailable(REFUSED))
                },
            );
            refused.finish(result, 2);
        },
    );
}

#[test]
fn resident_cold_certification_refuses_equal_bytes_new_live_seal() {
    runtime::with_route_fixture(
        |install, height, compatibility, inventory, policy, p, query, _| {
            let s = runtime::session(&p);
            let mut provider =
                ResidentFilterProvider::bind_with_policy(&s, &p, height, query, policy).unwrap();
            let mut fixture = InstalledFixture::new(install.path(), inventory);
            let mut saved = None;
            let first = runtime::complete_cold_probe(
                &p,
                &s,
                height,
                compatibility,
                &mut provider,
                |provider, context| {
                    assert!(saved.is_none());
                    saved = Some((context.clone(), prerequisites(provider, context)));
                    Err(NativeError::AuthorityUnavailable(REFUSED))
                },
            );
            assert!(matches!(
                first,
                Err(NativeError::AuthorityUnavailable(REFUSED))
            ));
            let (old, ready) = saved
                .take()
                .expect("first live callback captured genuine prerequisites");
            let mut ready = Some(ready);
            let mut refused = Refusals::default();
            // Separate equal-byte initialized worlds prove only seal refusal, NOT same-world retry.
            let second = runtime::complete_cold_probe(
                &p,
                &s,
                height,
                compatibility,
                &mut provider,
                |provider, context| {
                    assert_eq!(old, *context);
                    assert!(!old
                        .require_pathfinder_dispatch(&p)
                        .unwrap()
                        .same_live_dispatch(context.require_pathfinder_dispatch(&p).unwrap()));
                    let reached = Cell::new(false);
                    refused.expect(
                        attempt(
                            &mut fixture,
                            compatibility,
                            provider,
                            context,
                            ready.take().unwrap(),
                            runtime::until(),
                            || false,
                            || reached.set(true),
                        ),
                        binding,
                    );
                    assert!(!reached.get());
                    Err(NativeError::AuthorityUnavailable(REFUSED))
                },
            );
            refused.finish(second, 1);
        },
    );
}

#[test]
fn resident_cold_certification_refuses_observation_only_policy() {
    runtime::with_fixture(
        |install, height, compatibility, inventory, policy, p, query, _| {
            let s = runtime::session(&p);
            let mut provider =
                ResidentFilterProvider::bind_with_policy(&s, &p, height, query, policy).unwrap();
            let mut fixture = InstalledFixture::new(install.path(), inventory);
            let mut refused = Refusals::default();
            let result = runtime::complete_cold_probe(
                &p,
                &s,
                height,
                compatibility,
                &mut provider,
                |provider, context| {
                    assert!(context
                        .require_pathfinder_dispatch(&p)
                        .unwrap()
                        .first_construction());
                    let ready = prerequisites(provider, context);
                    let reached = Cell::new(false);
                    refused.expect(
                        attempt(
                            &mut fixture,
                            compatibility,
                            provider,
                            context,
                            ready,
                            runtime::until(),
                            || false,
                            || reached.set(true),
                        ),
                        |error| {
                            matches!(
                                error,
                                ResidentFilterError::Context(Failure::UnsupportedQuery)
                            )
                        },
                    );
                    assert!(!reached.get());
                    Err(NativeError::AuthorityUnavailable(REFUSED))
                },
            );
            refused.finish(result, 1);
        },
    );
}
