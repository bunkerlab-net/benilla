// SPDX-License-Identifier: GPL-3.0-or-later
//! Inert quest-boundary captures and sealed source-reconstruction controller admission.
use super::*;

struct Archive {
    path: PathBuf,
    recipe: Recipe,
    generation: u64,
}

/// The baseline controller and query owner are gone; only selected identity and saved data remain.
pub(crate) struct Capture {
    archive: Archive,
    profile: n::Profile,
    final_recipe: Recipe,
    captured_progress: n::QuestProgressionState,
    post_reward: bool,
    loot_prefix_tick: u64,
    final_progress: n::QuestProgressionState,
    harness: Harness,
}
impl Fixture {
    /// Both runs use the real controller save barrier, not a second shadow store.
    pub(crate) fn capture_recipe(
        self,
        authority: impl n::Authority + Send + 'static,
        sink: impl RouteCommitSink + 'static,
        observed: Observed,
        inspect: impl Fn(&RouteCommitBoundary, &[n::AuthorityReply], bool) -> Heads,
    ) -> Capture {
        self.capture_at(authority, sink, observed, inspect, false)
    }
    /// Capture the successful reward itself, not the later duplicate-reward tick.
    pub(crate) fn capture_rewarded_recipe(
        self,
        authority: impl n::Authority + Send + 'static,
        sink: impl RouteCommitSink + 'static,
        observed: Observed,
        inspect: impl Fn(&RouteCommitBoundary, &[n::AuthorityReply], bool) -> Heads,
    ) -> Capture {
        self.capture_at(authority, sink, observed, inspect, true)
    }
    fn capture_at(
        self,
        authority: impl n::Authority + Send + 'static,
        sink: impl RouteCommitSink + 'static,
        observed: Observed,
        inspect: impl Fn(&RouteCommitBoundary, &[n::AuthorityReply], bool) -> Heads,
        post_reward: bool,
    ) -> Capture {
        let mut driver =
            self.open_accepted(authority, sink, observed.clone(), &inspect, false, true);
        let accepted_archive = driver.checked_archive();
        let c = &driver.controller;
        let profile = c.world.profile().clone();
        let (hostile, giver) = role_ids(&profile);
        assert!(c.world.state().npcs[&hostile].health > 0);
        assert!(c.world.state().npcs[&giver].health > 0);
        assert_eq!(
            c.world.validate_saved_session_reopen(),
            Err(n::NativeError::UnsupportedMechanic(
                "saved native PathFinder cache rehydration"
            ))
        );
        assert!(
            n::NativeWorld::restore_synthetic_component_test(
                profile.clone(),
                accepted_archive.recipe.expected_checkpoint().clone()
            )
            .is_err(),
            "ordinary runtime restore cannot admit the accepted cached checkpoint"
        );
        let heads = c.route_commit.as_ref().unwrap().boundary.heads();
        assert_eq!(
            heads.iter().filter(|head| head.cache().is_some()).count(),
            1
        );
        assert!(heads.iter().any(|head| {
            head.npc() == c.world.state().npcs[&hostile].id && head.cache().is_some()
        }));
        assert!(heads.iter().any(|head| {
            head.npc() == c.world.state().npcs[&giver].id && head.cache().is_none()
        }));
        // Historical receipt replay is only a negative ordinary-store probe, never
        // the positive source reconstruction or a controller admission path.
        let before = n::NativeWorld::restore_synthetic_component_test(
            profile.clone(),
            accepted_archive.recipe.anchor().clone(),
        )
        .unwrap();
        let mut after = before.clone();
        after.replay(accepted_archive.recipe.records()).unwrap();
        assert_eq!(
            &after.checkpoint().unwrap(),
            accepted_archive.recipe.expected_checkpoint()
        );
        let negative_path = driver.harness.root.join("accepted-recipe-native-refusal");
        let (mut negative, _) = NativeStore::create(&negative_path, &before).unwrap();
        negative.commit(negative.prepare(&after).unwrap()).unwrap();
        drop(negative);
        reject_open(
            &negative_path,
            Identity::of(&before.checkpoint().unwrap()),
            &profile,
            true,
        );
        drop((before, after));
        let loot_prefix_tick =
            post_reward.then(|| driver.reward_success(&observed, FinishMode::Uninterrupted));
        let archive = driver.snapshot_recipe();
        let captured_progress = progress(&driver.controller).clone();
        let loot_prefix_tick = if let Some(tick) = loot_prefix_tick {
            assert_eq!(
                archive.recipe.expected_checkpoint().identity().tick,
                tick + 1
            );
            assert_rewarded(&driver.controller);
            let boundary = &driver.controller.route_commit.as_ref().unwrap().boundary;
            assert!(inspect(boundary, &[], true).committed.is_empty());
            driver.finish_rewarded(&observed, FinishMode::Uninterrupted, true);
            tick
        } else {
            driver.finish(&observed, FinishMode::Uninterrupted)
        };
        let final_archive = driver.checked_archive();
        let final_progress = progress(&driver.controller).clone();
        let final_recipe = final_archive.recipe.clone();
        let original_bytes = fs::read(final_archive.path.join("world.checkpoint")).unwrap();
        assert!(
            open_archive(&final_archive, &profile).is_err(),
            "the baseline controller must retain the original exclusive recipe lease"
        );
        let Driver {
            controller,
            harness,
            ..
        } = driver;
        drop(controller);
        // Reopen the original, not merely the snapshot, to prove its lease was released.
        let (store, actual) = open_archive(&final_archive, &profile).unwrap();
        assert_eq!(actual, final_recipe);
        assert_eq!(store.generation(), final_archive.generation);
        assert_eq!(
            fs::read(final_archive.path.join("world.checkpoint")).unwrap(),
            original_bytes
        );
        drop(store);
        let (store, actual) = open_archive(&archive, &profile).unwrap();
        assert_eq!(actual, archive.recipe);
        assert_eq!(store.generation(), archive.generation);
        drop(store);
        Capture {
            archive,
            profile,
            final_recipe,
            captured_progress,
            post_reward,
            loot_prefix_tick,
            final_progress,
            harness,
        }
    }
}
impl Driver {
    fn checked_archive(&self) -> Archive {
        let c = &self.controller;
        assert!(c.pause.is_none());
        assert!(c.pending.is_none());
        assert!(c.outbox.is_none());
        assert!(c.actions.is_empty());
        assert!(!c.closed && !c.closing);
        assert!(c.world.records().is_empty());
        assert_eq!(c.saved_tick, c.tick());
        assert!(c
            .route_commit
            .as_ref()
            .is_some_and(|commit| { commit.initialized && commit.pending.is_none() }));
        let store::Store::Recipe(recipe) = &c.store else {
            panic!("the quest restart must capture the actual recipe save barrier")
        };
        assert!(!recipe.requires_reopen());
        assert_eq!(
            recipe.identity(),
            &Identity::of(&c.world.checkpoint().unwrap())
        );
        assert_eq!(
            recipe.recipe().expected_checkpoint(),
            &c.world.checkpoint().unwrap()
        );
        assert_eq!(recipe.recipe().records().len() as u64, c.tick());
        Archive {
            path: self.path.clone(),
            recipe: recipe.recipe().clone(),
            generation: recipe.generation(),
        }
    }
    fn snapshot_recipe(&self) -> Archive {
        let mut archive = self.checked_archive();
        assert!(!archive.path.join(".world-checkpoint.pending").exists());
        let path = archive.path.with_extension(format!(
            "quest-captured-{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        #[allow(unused_mut)] // Unix requests private creation; never repair an existing path.
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
        for file in ["world.checkpoint", "world.previous"] {
            fs::copy(archive.path.join(file), path.join(file)).unwrap();
            assert_eq!(
                fs::read(archive.path.join(file)).unwrap(),
                fs::read(path.join(file)).unwrap(),
                "the captured archive must preserve exact {file} bytes"
            );
        }
        assert!(!path.join("world.lock").exists());
        assert!(!path.join(".world-checkpoint.pending").exists());
        archive.path = path;
        archive
    }
}
fn open_archive(
    archive: &Archive,
    profile: &n::Profile,
) -> Result<(RecipeStore, Recipe), benilla_world_store::native::Error> {
    RecipeStore::open(
        &archive.path,
        Identity::of(archive.recipe.expected_checkpoint()),
        |anchor| n::NativeWorld::restore_synthetic_component_test(profile.clone(), anchor),
    )
}
impl Capture {
    pub(crate) fn is_post_reward(&self) -> bool {
        self.post_reward
    }
    pub(crate) fn loot_prefix_tick(&self) -> u64 {
        self.loot_prefix_tick
    }
    pub(crate) fn path(&self) -> &Path {
        &self.archive.path
    }
    pub(crate) fn profile(&self) -> &n::Profile {
        &self.profile
    }
    pub(crate) fn player(&self) -> PlayerId {
        self.checkpoint().identity().player
    }
    pub(crate) fn world_id(&self) -> WorldId {
        self.checkpoint().identity().world
    }
    pub(crate) fn recipe(&self) -> &Recipe {
        &self.archive.recipe
    }
    pub(crate) fn checkpoint(&self) -> &n::Checkpoint {
        self.archive.recipe.expected_checkpoint()
    }
    /// A caller cannot admit a checkpoint, copied replies, or an unsealed runtime here.
    #[cfg(not(benilla_headless_geometry))]
    pub(crate) fn resume(
        self,
        completed: crate::net::p2p::native_geometry::CompletedReconstruction<()>,
        authority: impl n::Authority + Send + 'static,
        sink: impl RouteCommitSink + 'static,
        observed: Observed,
        retry_faults: bool,
    ) {
        let (recipe, world, (), boundary) = completed.into_parts();
        let selected =
            Selection::from_profile(&self.harness.profile, &self.harness.character).unwrap();
        assert_eq!(selected.player, self.player());
        assert_eq!(world.state().player, self.player());
        assert_eq!(world.state().world, self.world_id());
        assert_eq!(world.profile(), self.profile());
        assert_eq!(&world.checkpoint().unwrap(), self.checkpoint());
        assert!(world.records().is_empty());
        assert!(world.last_tick().is_none());
        assert_eq!(recipe.identity(), &Identity::of(self.checkpoint()));
        assert_eq!(recipe.recipe(), self.recipe());
        assert_eq!(recipe.generation(), self.archive.generation);
        assert!(!recipe.requires_reopen());
        assert_eq!(boundary.world(), world.state().world);
        assert_eq!(boundary.player(), world.state().player);
        assert_eq!(boundary.profile(), world.profile().digest());
        assert_eq!(boundary.tick(), world.state().tick);
        assert_eq!(boundary.state(), world.state_digest());
        assert_eq!(boundary.record(), self.checkpoint().identity().history_tip);
        let catalog = LoopCatalog::new(&self.profile);
        let projection = NativeProjection::new(&world, selected.player, &catalog).unwrap();
        let mut c = NativeController::from_component_parts(
            world,
            projection,
            store::Store::Recipe(Box::new(recipe)),
            selected,
            authority,
        )
        .unwrap();
        // Install the fresh paired session atomically; do not reinitialize its acknowledged head.
        c.route_commit = Some(RouteCommitState {
            sink: Box::new(sink),
            boundary,
            initialized: true,
            pending: None,
        });
        assert_eq!(progress(&c), &self.captured_progress);
        if self.post_reward {
            assert_rewarded(&c);
        }
        assert_initial_once(&mut c, &observed, self.post_reward);
        let mut driver = Driver {
            controller: c,
            path: self.archive.path,
            harness: self.harness,
        };
        if self.post_reward {
            assert!(
                !retry_faults,
                "post-reward tests do not inject new continuation faults"
            );
            driver.finish_rewarded(&observed, FinishMode::Reconstructed { retry_faults }, true);
        } else {
            assert_eq!(
                driver.finish(&observed, FinishMode::Reconstructed { retry_faults }),
                self.loot_prefix_tick
            );
        }
        let actual = driver.checked_archive();
        assert_eq!(actual.recipe, self.final_recipe);
        assert_eq!(
            driver.controller.world.checkpoint().unwrap(),
            *self.final_recipe.expected_checkpoint()
        );
        let progress = progress(&driver.controller);
        assert_eq!(progress.quests, self.final_progress.quests);
        assert_eq!(progress.inventory, self.final_progress.inventory);
        assert_eq!(
            progress.player.view().xp,
            self.final_progress.player.view().xp
        );
        assert_eq!(
            progress.player.view().level,
            self.final_progress.player.view().level
        );
        assert_eq!(progress.money, self.final_progress.money);
        assert_eq!(progress, &self.final_progress);
        let Driver {
            controller,
            harness,
            ..
        } = driver;
        drop(controller);
        let (store, durable) = open_archive(&actual, &self.profile).unwrap();
        assert_eq!(durable, self.final_recipe);
        assert_eq!(store.generation(), actual.generation);
        drop(store);
        drop(harness);
    }
}
#[cfg(not(benilla_headless_geometry))]
fn assert_initial_once(c: &mut NativeController, observed: &Observed, post_reward: bool) {
    let checkpoint = c.world.checkpoint().unwrap();
    let generation = c.store.generation();
    let calls = observed.controls.calls.load(Ordering::SeqCst);
    let committed = observed.committed.lock().unwrap().len();
    let initial = c.initial_events().unwrap();
    no_transient_progress(&initial);
    if post_reward {
        assert!(!initial.iter().any(|event| matches!(event,
            SessionEvent::ItemCreate { fields, .. }
                if fields.object_entry() == Some(loop_fixture::ITEM)
        )));
        assert!(c.loot_window.is_none());
    }
    assert!(!initial.iter().any(|event| matches!(
        event,
        SessionEvent::SpellStart { .. }
            | SessionEvent::SpellGo { .. }
            | SessionEvent::SpellDamageLog(_)
            | SessionEvent::LootResponse { .. }
    )));
    for npc in c.world.state().npcs.values() {
        let guid = c.projection.npc_guid(npc.id).unwrap();
        assert_eq!(
            initial
                .iter()
                .filter(|event| matches!(event,
                    SessionEvent::ObjectCreate { guid: created, .. } if *created == guid
                ))
                .count(),
            1
        );
    }
    assert_eq!(
        initial
            .iter()
            .filter(|event| matches!(event,
                SessionEvent::ObjectValues { guid, .. } if *guid == c.projection.player_guid()
            ))
            .count(),
        1
    );
    assert!(matches!(c.initial_events(), Err(Error::Closed)));
    let idle = c.pump(Duration::ZERO);
    assert_eq!(idle.ticks, 0);
    assert!(idle.events.is_empty());
    assert!(idle.pause.is_none());
    assert_eq!(c.world.checkpoint().unwrap(), checkpoint);
    assert_eq!(c.store.generation(), generation);
    assert_eq!(observed.controls.calls.load(Ordering::SeqCst), calls);
    assert_eq!(observed.committed.lock().unwrap().len(), committed);
}

pub(super) fn assert_ack_retry(
    path: &Path,
    c: &mut NativeController,
    observed: &Observed,
    notice: &RouteCommitNotice,
    attempts: usize,
) {
    let checkpoint = c.world.checkpoint().unwrap();
    let generation = c.store.generation();
    let saved = fs::read(path.join("world.checkpoint")).unwrap();
    let committed = observed.committed.lock().unwrap().len();
    let calls = observed.controls.calls.load(Ordering::SeqCst);
    assert!(!observed.lose_ack.load(Ordering::SeqCst));
    assert!(c.outbox.is_none());
    assert!(c.pending.is_none());
    assert!(c.world.records().is_empty());
    assert_eq!(
        &observed.attempted.lock().unwrap()[attempts..],
        std::slice::from_ref(notice)
    );
    assert!(c.retry_tick().is_err());
    assert!(c.retry_checkpoint().is_err());
    let blocked = c.pump(Duration::from_secs(600));
    assert_eq!(blocked.ticks, 0);
    assert!(blocked.events.is_empty());
    assert_eq!(observed.attempted.lock().unwrap().len(), attempts + 1);
    c.retry_route_commit().unwrap();
    assert!(c.pause.is_none());
    assert!(c.route_commit.as_ref().unwrap().pending.is_none());
    assert_eq!(
        &observed.attempted.lock().unwrap()[attempts..],
        &[notice.clone(), notice.clone()]
    );
    assert_eq!(observed.committed.lock().unwrap().len(), committed);
    assert_eq!(observed.committed.lock().unwrap().last(), Some(notice));
    assert_eq!(c.world.checkpoint().unwrap(), checkpoint);
    assert_eq!(c.store.generation(), generation);
    assert_eq!(fs::read(path.join("world.checkpoint")).unwrap(), saved);
    assert_eq!(observed.controls.calls.load(Ordering::SeqCst), calls);
    assert!(c.pump(Duration::ZERO).events.is_empty());
}
