// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored contract claims only: no native query, geometry or physics executes.
use super::*;
use benilla_sim::{creature_ai as ai, creature_motion as m};
use benilla_world_runtime::native::{
    synthetic_combat_fixture as combat, synthetic_creature_model_fixture,
    synthetic_motion_fixture as fixture, *,
};

fn cache_world() -> NativeWorld {
    let mut p = combat::profile();
    p.npcs[0].ai.spawn = f::point(8.);
    p.npcs[0].max_health = 10;
    p.initialize_creature_capabilities(fixture::creature_capability_sources(&p))
        .unwrap();
    let t = Temp::new();
    let path = t.0.join("map.MPQ");
    let mut dbc = b"WDBC".to_vec();
    for value in [1_u32, 42, 168, 9] {
        dbc.extend(value.to_le_bytes());
    }
    let mut row = [0_u32; 42];
    row[0] = p.map_id;
    row[1] = 1;
    for value in row {
        dbc.extend(value.to_le_bytes());
    }
    dbc.extend(b"\0Fixture\0");
    fs::write(
        &path,
        synthetic_creature_model_fixture::archive(&[("DBFilesClient\\Map.dbc", dbc)]),
    )
    .unwrap();
    let mut chain = benilla_formats::Chain::open(&path).unwrap();
    let maps = benilla_formats::load_map_catalog(&mut chain).unwrap();
    p.initialize_pathfinder_configuration(
        SelectedMMapConfiguration {
            enabled: true,
            effective_disabled_maps: EffectiveDisabledMMapSet::Complete(Default::default()),
            source_instance_id: 0,
        },
        &maps,
    )
    .unwrap();
    NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 1234).unwrap()
}

struct AuthoredCacheClaims {
    inner: fixture::SyntheticMotionAuthority,
    source: m::RouteSource,
    calculations: usize,
}
impl AuthoredCacheClaims {
    fn new(source: m::RouteSource) -> Self {
        Self {
            inner: fixture::SyntheticMotionAuthority {
                critical_chance: Some(0),
                resist_chance: Some(0),
                ..Default::default()
            },
            source,
            calculations: 0,
        }
    }
    fn seed(&mut self, world: &mut NativeWorld) {
        while self.calculations == 0 {
            assert!(world.state().tick < 10);
            world.step(world.state().tick + 1, None, self).unwrap();
        }
    }
}
impl Authority for AuthoredCacheClaims {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        let mut reply = self.inner.resolve(request)?;
        let Query::SourceMotion(MotionQuery::Route(q)) = &request.query else {
            return Ok(reply);
        };
        let Facts::SourceMotion(MotionFacts::Spatial(spatial)) = &mut reply.facts else {
            panic!("motion reply");
        };
        let m::Fact::Route(outcome) = &spatial.fact else {
            panic!("fixture route");
        };
        let mut after = q
            .request
            .prior_cache
            .as_deref()
            .cloned()
            .unwrap_or_else(|| m::PathFinderCache {
                binding: m::PathFinderCacheBinding {
                    identity: q.request.binding.identity,
                    owner_generation: q.owner.npc().generation,
                    inventory: [3; 32],
                    source_epoch: [4; 32],
                    navigation_policy: [5; 32],
                },
                revision: 0,
                source: self.source,
                poly_refs: (0..77)
                    .map(|i| if !(2..74).contains(&i) { i + 1 } else { 0 })
                    .collect(),
                active_len: 2,
                point_limit: 74,
                path_type: 1,
                requested_end: q.request.goal,
                include_flags: 1,
                exclude_flags: 0,
            });
        after.revision += 1;
        after.requested_end = q.request.goal;
        let mut outcome = outcome.clone();
        let m::RouteOutcome::Complete(path) = &mut outcome else {
            panic!("complete authored route");
        };
        path.source = self.source;
        spatial.fact = m::Fact::RouteCalculation(Box::new(m::RouteCalculation {
            source: self.source,
            outcome,
            prior_cache: q.request.prior_cache.clone(),
            after_cache: after,
        }));
        self.calculations += 1;
        Ok(reply)
    }
}

fn open_cache(path: &Path, world: &NativeWorld) -> Result<(NativeStore, NativeWorld), Error> {
    NativeStore::open(path, Identity::of(&world.checkpoint().unwrap()), |c| {
        NativeWorld::restore_synthetic_component_test(world.profile().clone(), c)
    })
}

#[test]
fn authored_owner_journal_replays_exactly_but_saved_reopen_rejects_current_cache() {
    let t = Temp::new();
    let mut world = cache_world();
    let genesis = world.checkpoint().unwrap();
    let (mut store, _) = NativeStore::create(t.path(), &world).unwrap();
    AuthoredCacheClaims::new(m::RouteSource::OwnerPathFinder).seed(&mut world);
    let source = world.state().npcs[&2].source_motion.as_ref().unwrap();
    let m::Generator::Chase(chase) = source.motion.generator() else {
        panic!("retained Chase owner");
    };
    let cache = chase.pathfinder_cache.as_ref().unwrap();
    assert_eq!(cache.source, m::RouteSource::OwnerPathFinder);
    assert_eq!(cache.revision, 1);
    assert!(source
        .motion
        .snapshot()
        .spline
        .unwrap()
        .route_request
        .prior_cache
        .is_none());
    let expected = world.checkpoint().unwrap();
    assert!(NativeWorld::restore_synthetic_component_test(
        world.profile().clone(),
        expected.clone()
    )
    .is_err());
    let records = world.records().to_vec();
    let receipt = store.commit(store.prepare(&world).unwrap()).unwrap();
    assert_eq!(receipt.generation(), 2);
    assert_eq!(world.records(), records);
    drop(store);
    let current = fs::read(t.path().join("world.checkpoint")).unwrap();
    let previous = fs::read(t.path().join("world.previous")).unwrap();
    let envelope = Envelope::decode(&current).unwrap();
    assert_eq!(envelope.base, genesis);
    assert_eq!(envelope.records, records);
    // This is the former open path: base restore plus exact recorded-fact replay.
    // Its success is not a native loaded-query rehydration capability.
    let replay = envelope
        .restore(&Identity::of(&genesis), &mut |c| {
            NativeWorld::restore_synthetic_component_test(world.profile().clone(), c)
        })
        .unwrap();
    assert_eq!(replay.checkpoint().unwrap(), expected);
    assert_eq!(replay.records(), records);
    assert!(matches!(
        open_cache(&t.path(), &world),
        Err(Error::Runtime(NativeError::UnsupportedMechanic(
            "saved native PathFinder cache rehydration"
        )))
    ));
    assert_eq!(
        fs::read(t.path().join("world.checkpoint")).unwrap(),
        current
    );
    assert_eq!(fs::read(t.path().join("world.previous")).unwrap(), previous);
    assert_eq!(world.checkpoint().unwrap(), expected);
    assert_eq!(world.records(), records);
    // Rejection released the writer lock and did not repair or replace the files.
    assert!(matches!(
        open_cache(&t.path(), &world),
        Err(Error::Runtime(NativeError::UnsupportedMechanic(
            "saved native PathFinder cache rehydration"
        )))
    ));
}

#[test]
fn authored_owner_base_remains_rejected_after_second_envelope() {
    let t = Temp::new();
    let mut world = cache_world();
    let (mut store, _) = NativeStore::create(t.path(), &world).unwrap();
    AuthoredCacheClaims::new(m::RouteSource::OwnerPathFinder).seed(&mut world);
    store.checkpoint(&mut world).unwrap();
    store.checkpoint(&mut world).unwrap();
    drop(store);
    assert!(matches!(
        open_cache(&t.path(), &world),
        Err(Error::Runtime(NativeError::InvalidCheckpoint))
    ));
}

#[test]
fn authored_synthetic_cache_reopens_from_journal_and_then_cached_base() {
    let t = Temp::new();
    let mut world = cache_world();
    let (mut store, _) = NativeStore::create(t.path(), &world).unwrap();
    AuthoredCacheClaims::new(m::RouteSource::SyntheticFixture).seed(&mut world);
    let expected = world.checkpoint().unwrap();
    for generation in 2..=3 {
        assert_eq!(
            store.checkpoint(&mut world).unwrap().generation(),
            generation
        );
        drop(store);
        let (reopened, restored) = open_cache(&t.path(), &world).unwrap();
        assert_eq!(restored.checkpoint().unwrap(), expected);
        assert!(restored.records().is_empty());
        store = reopened;
        world = restored;
    }
}

#[test]
fn historical_authored_owner_route_reopens_after_modeled_death_drops_cache() {
    let t = Temp::new();
    let mut world = cache_world();
    let (mut store, _) = NativeStore::create(t.path(), &world).unwrap();
    let mut authority = AuthoredCacheClaims::new(m::RouteSource::OwnerPathFinder);
    authority.seed(&mut world);
    assert!(world.validate_saved_session_reopen().is_err());
    let cast = f::cast(&world, 60001);
    world
        .step(world.state().tick + 1, Some(cast), &mut authority)
        .unwrap();
    while world.state().npcs[&2].health != 0 {
        assert!(world.state().tick < 100);
        world
            .step(world.state().tick + 1, None, &mut authority)
            .unwrap();
    }
    assert!(matches!(
        world.state().npcs[&2].ai.phase(),
        ai::Phase::Corpse | ai::Phase::Despawned
    ));
    let source = world.state().npcs[&2].source_motion.as_ref().unwrap();
    assert!(matches!(source.motion.generator(), m::Generator::Idle));
    assert!(source.motion.snapshot().spline.is_none());
    assert!(world
        .records()
        .iter()
        .flat_map(|r| &r.replies)
        .any(|r| matches!(
            &r.facts,
            Facts::SourceMotion(MotionFacts::Spatial(reply))
                if matches!(&reply.fact, m::Fact::RouteCalculation(c)
                    if c.source == m::RouteSource::OwnerPathFinder)
        )));
    world.validate_saved_session_reopen().unwrap();
    let expected = world.checkpoint().unwrap();
    store.checkpoint(&mut world).unwrap();
    drop(store);
    let (_, restored) = open_cache(&t.path(), &world).unwrap();
    assert_eq!(restored.checkpoint().unwrap(), expected);
    assert!(restored.records().is_empty());
}
