// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual loaded environment facts cross the mailbox, controller and durable store.
//! Authored bytes only; movement/combat authorities remain explicitly synthetic.
use super::*;
use crate::net::p2p::native_geometry::{
    authored_player_environment,
    core::{self, Body, Failure, NonRenderParticipants, PlayerMotion, Session},
    SourcePlayerEnvironmentProvider,
};
use benilla_sim::creature_ai as ai;
use n::{synthetic_motion_fixture as source, synthetic_player_model_fixture as player};
use std::collections::BTreeMap;

struct EnvironmentAuthority {
    local: core::LocalAuthority,
    other: source::SyntheticMotionAuthority,
    observed: Arc<Mutex<Vec<n::AuthorityRequest>>>,
    stop: Arc<AtomicBool>,
    service: Option<std::thread::JoinHandle<()>>,
}
impl n::Authority for EnvironmentAuthority {
    fn resolve(
        &mut self,
        request: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        self.observed.lock().unwrap().push(request.clone());
        if matches!(request.query, n::Query::PlayerEnvironment(_)) {
            self.local.resolve(request)
        } else {
            self.other.resolve(request)
        }
    }
    fn observe_derived(&mut self, reply: &n::AuthorityReply) -> Result<(), n::NativeError> {
        self.other.observe_derived(reply)
    }
}
impl Drop for EnvironmentAuthority {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let result = self.service.take().unwrap().join();
        if !std::thread::panicking() {
            result.unwrap();
        }
    }
}
struct Controls {
    fault: Arc<AtomicU64>,
    observed: Arc<Mutex<Vec<n::AuthorityRequest>>>,
}
fn setup(h: &Harness, tick_ms: u32) -> (n::Profile, EnvironmentAuthority, Controls) {
    let mut profile = player::profile();
    let snapshot = authored_player_environment(profile.map_id, profile.content_digest);
    profile.tick_ms = tick_ms;
    profile.player.start = ai::Point::new(0.0, 0.0, 31.0).unwrap();
    profile.npcs[0].ai.spawn = ai::Point::new(8.0, 0.0, 31.0).unwrap();
    profile.npcs[0].ai.reaction = ai::Reaction::Passive;
    profile
        .initialize_player_environment(
            snapshot.environment_binding(),
            snapshot.environment_catalog().clone(),
        )
        .unwrap();
    let player_id = Selection::from_profile(&h.profile, &h.character)
        .unwrap()
        .player;
    let motion = profile.source_motion.as_ref().unwrap();
    let session = Session {
        world: [8; 16],
        profile: profile.digest(),
        policy: profile.authority_policy,
        content: profile.content_digest,
        geometry: profile.geometry_revision,
        map: profile.map_id,
        bodies: BTreeMap::from([(
            2,
            Body {
                radius: profile.npcs[0].ai.bounding_radius,
                height: motion.npc_policy.settings.collision_height,
            },
        )]),
        player: Some(PlayerMotion {
            body: Body {
                radius: profile.player.bounding_radius,
                height: motion.player_collision_height,
            },
            speed: fixture::real(7.0),
        }),
        participants: Some(NonRenderParticipants::authored_fixture(
            profile.content_digest,
            profile.map_id,
        )),
        static_los: None,
        primary_near_point: None,
        empty_selector_near_point: None,
        same_polygon_route: None,
        player_environment: Some(
            core::PlayerEnvironmentPolicy::from_profile(&profile, player_id).unwrap(),
        ),
    };
    let provider =
        SourcePlayerEnvironmentProvider::bind(&session, &profile, player_id, snapshot).unwrap();
    let (local, mailbox) = core::mailbox(session).unwrap();
    let fault = Arc::new(AtomicU64::new(0));
    let observed = Arc::new(Mutex::new(Vec::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    let thread_fault = fault.clone();
    let service = std::thread::spawn(move || {
        while !thread_stop.load(Ordering::Acquire) {
            let count = mailbox.serve_queries_until(1, |session, request, _, deadline| {
                if thread_fault.load(Ordering::SeqCst) == 1 {
                    return Err(Failure::MissingGeometry);
                }
                provider.resolve(session, request, deadline)
            });
            if count == 0 {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    });
    (
        profile,
        EnvironmentAuthority {
            local,
            other: source::SyntheticMotionAuthority::default(),
            observed: observed.clone(),
            stop,
            service: Some(service),
        },
        Controls { fault, observed },
    )
}
fn enter_environment(
    h: &Harness,
    entry: Entry,
    profile: n::Profile,
    authority: EnvironmentAuthority,
) -> NativeController {
    NativeController::enter_component_test(
        &h.root,
        &h.profile,
        &h.character,
        entry,
        profile,
        &Catalog,
        authority,
    )
    .unwrap()
}
fn create_environment(h: &Harness, tick_ms: u32) -> (NativeController, n::Profile, Controls) {
    let (profile, authority, controls) = setup(h, tick_ms);
    let controller = enter_environment(
        h,
        Entry::Create {
            world: [8; 16],
            seed: 7,
        },
        profile.clone(),
        authority,
    );
    (controller, profile, controls)
}
fn query_count(controls: &Controls) -> usize {
    controls
        .observed
        .lock()
        .unwrap()
        .iter()
        .filter(|r| matches!(r.query, n::Query::PlayerEnvironment(_)))
        .count()
}
fn pose(x: f32) -> ClientCommand {
    move_to([x, 0.0, 31.0])
}
fn cached(c: &NativeController) -> &n::environment::EnvironmentState {
    c.world.state().player_environment().unwrap().environment()
}
#[test]
fn loaded_environment_countdown_is_durable_even_without_input_or_projection() {
    let h = Harness::new();
    let (mut c, profile, controls) = create_environment(&h, 25);
    c.initial_events().unwrap();
    c.receive(pose(-0.125)).unwrap();
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none(), "{:?}", out.pause);
    assert_eq!(out.ticks, 1);
    assert!(out.saved.is_some());
    assert_eq!(cached(&c).timer_ms(), 75);
    assert_eq!(cached(&c).flags(), 0x40);
    assert_eq!(query_count(&controls), 1);
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none(), "{:?}", out.pause);
    assert!(out.saved.is_some(), "stationary countdown must be saved");
    assert_eq!(cached(&c).timer_ms(), 50);
    assert_eq!(query_count(&controls), 1);
    assert!(c.world.records().is_empty());
    let digest = c.world.state_digest();
    drop(c);
    let (_, authority, _) = setup(&h, 25);
    let mut c = enter_environment(&h, Entry::Open { world: [8; 16] }, profile, authority);
    assert_eq!(c.world.state_digest(), digest);
    assert_eq!(cached(&c).timer_ms(), 50);
    for _ in 0..3 {
        let out = c.pump(c.tick_duration());
        assert!(out.pause.is_none());
        assert!(out.saved.is_some());
    }
    assert_eq!(cached(&c).timer_ms(), 0);
}
#[test]
fn accepted_position_calls_precede_the_timer_countdown_and_keep_historical_cache_pose() {
    let h = Harness::new();
    let (mut c, _, controls) = create_environment(&h, 99);
    c.initial_events().unwrap();
    c.receive(pose(-0.125)).unwrap();
    c.receive(pose(-0.25)).unwrap();
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none(), "{:?}", out.pause);
    assert_eq!(query_count(&controls), 1);
    assert_eq!(cached(&c).timer_ms(), 1);
    assert_eq!(
        cached(&c).cached_query().unwrap().position[0],
        (-0.125f32).to_bits()
    );
    assert_eq!(
        cached(&c).last_position().unwrap().position[0],
        (-0.25f32).to_bits()
    );
    c.receive(pose(-0.25)).unwrap();
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none(), "{:?}", out.pause);
    assert_eq!(
        query_count(&controls),
        1,
        "timer expiry occurs after unchanged accepted pose"
    );
    assert_eq!(cached(&c).timer_ms(), 0);
    c.receive(pose(-0.25)).unwrap();
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none(), "{:?}", out.pause);
    assert_eq!(query_count(&controls), 2);
    assert_eq!(
        cached(&c).cached_query().unwrap().position[0],
        (-0.25f32).to_bits()
    );
}
#[test]
fn missing_loaded_environment_pauses_and_retries_identical_inputs_without_publishing() {
    let h = Harness::new();
    let (mut c, _, controls) = create_environment(&h, 25);
    c.initial_events().unwrap();
    let before = c.world.checkpoint().unwrap();
    controls.fault.store(1, Ordering::SeqCst);
    c.receive(pose(-0.125)).unwrap();
    let out = c.pump(c.tick_duration());
    assert_eq!(out.ticks, 0);
    assert!(out.events.is_empty());
    assert!(out.saved.is_none());
    assert!(matches!(out.pause.unwrap().kind, PauseKind::Authority));
    assert_eq!(c.world.checkpoint().unwrap(), before);
    assert_eq!(c.position_xyz(), [0.0, 0.0, 31.0]);
    controls.fault.store(0, Ordering::SeqCst);
    c.retry_tick().unwrap();
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 1);
    assert!(out.pause.is_none(), "{:?}", out.pause);
    assert!(out.saved.is_some());
    assert_eq!(c.world.state().next_sequence, 1);
    assert_eq!(c.position_xyz(), [-0.125, 0.0, 31.0]);
    let requests = controls.observed.lock().unwrap();
    let queries: Vec<_> = requests
        .iter()
        .filter(|r| matches!(r.query, n::Query::PlayerEnvironment(_)))
        .collect();
    assert_eq!(queries.len(), 2);
    assert_eq!(queries[0], queries[1]);
}
#[test]
fn environment_checkpoint_failure_retains_commit_but_withholds_projection_until_retry() {
    let h = Harness::new();
    let (mut c, _, controls) = create_environment(&h, 25);
    c.initial_events().unwrap();
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored checkpoint fault",
    )
    .unwrap();
    c.receive(pose(-0.125)).unwrap();
    let out = c.pump(c.tick_duration());
    assert_eq!(out.ticks, 1);
    assert!(out.events.is_empty());
    assert!(out.saved.is_none());
    assert!(matches!(
        out.pause.unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: false
        }
    ));
    assert_eq!(cached(&c).timer_ms(), 75);
    assert_eq!(query_count(&controls), 1);
    assert_eq!(c.position_xyz(), [0.0, 0.0, 31.0]);
    let digest = c.world.state_digest();
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 0);
    assert_eq!(c.world.state_digest(), digest);
    assert_eq!(query_count(&controls), 1);
    assert_eq!(c.position_xyz(), [-0.125, 0.0, 31.0]);
    assert!(!out.events.is_empty());
    assert!(c.pump(Duration::ZERO).events.is_empty());
}

#[test]
fn rotation_only_position_call_refreshes_expired_environment_without_displacement() {
    let h = Harness::new();
    let (mut c, _, controls) = create_environment(&h, 25);
    c.initial_events().unwrap();
    c.receive(pose(-0.125)).unwrap();
    pump_to(&mut c, 5);
    assert_eq!(query_count(&controls), 1);
    assert_eq!(cached(&c).timer_ms(), 0);
    let mut rotate = pose(-0.125);
    let ClientCommand::Move { orientation, .. } = &mut rotate else {
        unreachable!()
    };
    *orientation = 1.25;
    c.receive(rotate).unwrap();
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none(), "{:?}", out.pause);
    assert_eq!(query_count(&controls), 2);
    assert_eq!(cached(&c).timer_ms(), 75);
    assert_eq!(c.position_xyz(), [-0.125, 0.0, 31.0]);
    assert_eq!(c.facing(), 1.25);
}
