// SPDX-License-Identifier: GPL-3.0-or-later
//! App-only resident controller fixture. Geometry and commit acknowledgements come
//! exclusively from the supplied channels, never from this presentation catalog.
use super::*;
use benilla_sim::creature_init as init;
use benilla_world_store::native::reconstruction::{Recipe, RecipeStore};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct Fixture {
    harness: Harness,
    profile: n::Profile,
    world: WorldId,
    selected: Selection,
}
impl Fixture {
    pub(crate) fn new(profile: n::Profile, world: WorldId) -> Self {
        let harness = Harness::new();
        let selected = Selection::from_profile(&harness.profile, &harness.character).unwrap();
        world_path(&harness.root, world).unwrap();
        Self {
            harness,
            profile,
            world,
            selected,
        }
    }
    pub(crate) fn player(&self) -> PlayerId {
        self.selected.player
    }
    pub(crate) fn world_id(&self) -> WorldId {
        self.world
    }
    /// Capture through the actual controller save barrier, never a second shadow save.
    /// Only a just-opened, cache-free genesis can enter this component-only path.
    pub(crate) fn open_recipe_initialized(
        self,
        world: n::NativeWorld,
        authority: impl n::Authority + Send + 'static,
    ) -> Result<Driver, Error> {
        let mut driver = self.open_initialized(world, authority)?;
        let controller = &mut driver.controller;
        assert_eq!(controller.world.state().tick, 0);
        assert!(controller.world.records().is_empty());
        assert!(controller.route_commit.is_none());
        controller.world.validate_saved_session_reopen()?;
        let checkpoint = controller.world.checkpoint()?;
        let path = world_path(&driver._harness.root, controller.world.state().world)?
            .with_extension("recipe");
        let (recipe, receipt) = RecipeStore::create(&path, &controller.world)?;
        assert_eq!(recipe.identity(), &Identity::of(&checkpoint));
        assert_eq!(recipe.recipe().expected_checkpoint(), &checkpoint);
        receipt.acknowledge(&mut controller.world)?;
        controller.store = store::Store::Recipe(Box::new(recipe));
        driver.recipe_path = Some(path);
        Ok(driver)
    }
    /// The source owner initializes this world using the already-selected player.
    /// Persisting that genesis allows Open to avoid a synthetic initialization path.
    pub(crate) fn open_initialized(
        self,
        mut world: n::NativeWorld,
        authority: impl n::Authority + Send + 'static,
    ) -> Result<Driver, Error> {
        assert_eq!(world.state().world, self.world);
        assert_eq!(world.state().player, self.player());
        assert_eq!(world.profile(), &self.profile);
        assert_eq!(world.state().tick, 0);
        assert!(world.records().is_empty());
        world.validate_saved_session_reopen()?;
        let catalog = ResidentCatalog::new(&world);
        let path = world_path(&self.harness.root, self.world)?;
        fs::create_dir(path.parent().expect("fixed world container"))?;
        let (store, receipt) = NativeStore::create(path, &world)?;
        receipt.acknowledge(&mut world)?;
        let checkpoint = world.checkpoint()?;
        drop(store);
        let controller = NativeController::enter_component_test(
            &self.harness.root,
            &self.harness.profile,
            &self.harness.character,
            Entry::Open { world: self.world },
            self.profile,
            &catalog,
            authority,
        )?;
        assert_eq!(controller.world.checkpoint()?, checkpoint);
        Ok(Driver {
            controller,
            recipe_path: None,
            _harness: self.harness,
        })
    }
}

/// Inert persisted recipe bytes, never a retained runtime or loaded route owner.
pub(crate) struct RecipeArchive {
    root: PathBuf,
    path: PathBuf,
    profile: n::Profile,
    player: PlayerId,
    world: WorldId,
    recipe: Recipe,
    generation: u64,
}

impl RecipeArchive {
    pub(crate) fn checkpoint(&self) -> &n::Checkpoint {
        self.recipe.expected_checkpoint()
    }
    pub(crate) fn recipe(&self) -> &Recipe {
        &self.recipe
    }
}

/// Keeps the selected identity and directory alive after the actual controller is dropped.
pub(crate) struct Capture {
    archive: RecipeArchive,
    harness: Harness,
}
impl Capture {
    pub(crate) fn path(&self) -> &Path {
        &self.archive.path
    }
    pub(crate) fn profile(&self) -> &n::Profile {
        &self.archive.profile
    }
    pub(crate) fn player(&self) -> PlayerId {
        self.archive.player
    }
    pub(crate) fn world_id(&self) -> WorldId {
        self.archive.world
    }
    pub(crate) fn checkpoint(&self) -> &n::Checkpoint {
        self.archive.recipe.expected_checkpoint()
    }
    pub(crate) fn expected_checkpoint(&self) -> &n::Checkpoint {
        self.checkpoint()
    }
    pub(crate) fn recipe(&self) -> &Recipe {
        &self.archive.recipe
    }
    pub(crate) fn select_archive(mut self, archive: RecipeArchive) -> Self {
        assert_eq!(archive.root, self.harness.root);
        assert_eq!(archive.profile, self.archive.profile);
        assert_eq!(archive.player, self.archive.player);
        assert_eq!(archive.world, self.archive.world);
        assert!(
            archive.recipe.expected_checkpoint().identity().tick
                <= self.checkpoint().identity().tick
        );
        self.archive = archive;
        self
    }
    /// Only successful, sealed source reconstruction can provide these live parts.
    /// The paired channel sink continues its already-acknowledged fresh session.
    pub(crate) fn resume<A>(
        self,
        completed: crate::net::p2p::native_geometry::CompletedReconstruction<A>,
        authority: impl n::Authority + Send + 'static,
        sink: impl RouteCommitSink + 'static,
    ) -> Result<(Driver, A), Error> {
        let (recipe, world, owner, boundary) = completed.into_parts();
        let selected = Selection::from_profile(&self.harness.profile, &self.harness.character)?;
        assert_eq!(selected.player, self.player());
        assert_eq!(world.state().player, self.player());
        assert_eq!(world.state().world, self.world_id());
        assert_eq!(world.profile(), self.profile());
        assert!(world.records().is_empty());
        assert_eq!(&world.checkpoint()?, self.checkpoint());
        assert_eq!(recipe.identity(), &Identity::of(self.checkpoint()));
        assert_eq!(recipe.recipe(), &self.archive.recipe);
        assert_eq!(recipe.generation(), self.archive.generation);
        assert!(!recipe.requires_reopen());
        let catalog = ResidentCatalog::new(&world);
        let projection = NativeProjection::new(&world, selected.player, &catalog)?;
        let mut controller = NativeController::from_component_parts(
            world,
            projection,
            store::Store::Recipe(Box::new(recipe)),
            selected,
            authority,
        )?;
        controller.route_commit = Some(RouteCommitState {
            sink: Box::new(sink),
            boundary,
            initialized: true,
            pending: None,
        });
        Ok((
            Driver {
                controller,
                recipe_path: Some(self.archive.path),
                _harness: self.harness,
            },
            owner,
        ))
    }
}

pub(crate) struct Driver {
    // Drop the exclusive world lock before the harness removes its test directory.
    controller: NativeController,
    recipe_path: Option<PathBuf>,
    _harness: Harness,
}
impl Driver {
    fn checked_recipe_archive(&self) -> RecipeArchive {
        let controller = &self.controller;
        assert!(controller.pause.is_none());
        assert!(controller.pending.is_none());
        assert!(controller.outbox.is_none());
        assert!(controller.actions.is_empty());
        assert!(!controller.closed && !controller.closing);
        assert!(controller
            .route_commit
            .as_ref()
            .is_some_and(|commit| commit.initialized && commit.pending.is_none()));
        assert!(controller.world.records().is_empty());
        assert_eq!(controller.saved_tick, controller.world.state().tick);
        let checkpoint = controller.world.checkpoint().unwrap();
        let store::Store::Recipe(recipe) = &controller.store else {
            panic!("recipe capture required")
        };
        assert!(!recipe.requires_reopen());
        assert_eq!(recipe.identity(), &Identity::of(&checkpoint));
        assert_eq!(recipe.recipe().expected_checkpoint(), &checkpoint);
        RecipeArchive {
            root: self._harness.root.clone(),
            path: self.recipe_path.clone().expect("recipe directory"),
            profile: controller.world.profile().clone(),
            player: controller.selected.player,
            world: controller.world.state().world,
            recipe: recipe.recipe().clone(),
            generation: recipe.generation(),
        }
    }
    /// Snapshot only committed disk bytes while the baseline controller continues.
    pub(crate) fn snapshot_recipe_for_restart(&self) -> RecipeArchive {
        let mut archive = self.checked_recipe_archive();
        assert!(!archive.path.join(".world-checkpoint.pending").exists());
        let path = archive.path.with_extension(format!(
            "recipe-archive-{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::copy(
            archive.path.join("world.checkpoint"),
            path.join("world.checkpoint"),
        )
        .unwrap();
        assert_eq!(
            fs::read(archive.path.join("world.checkpoint")).unwrap(),
            fs::read(path.join("world.checkpoint")).unwrap(),
        );
        let previous = archive.path.join("world.previous");
        if previous.exists() {
            fs::copy(&previous, path.join("world.previous")).unwrap();
            assert_eq!(
                fs::read(previous).unwrap(),
                fs::read(path.join("world.previous")).unwrap()
            );
        }
        assert!(!path.join("world.lock").exists());
        assert!(!path.join(".world-checkpoint.pending").exists());
        archive.path = path;
        archive
    }
    /// Retire the actual controller and exclusive store lease, retaining only saved inputs.
    pub(crate) fn into_recipe_capture(self) -> Capture {
        let archive = self.checked_recipe_archive();
        let Self {
            controller,
            _harness: harness,
            ..
        } = self;
        drop(controller);
        // Prove release of the original controller's lease, not only access to a copy.
        let bytes = fs::read(archive.path.join("world.checkpoint")).unwrap();
        let (reopened, actual) = RecipeStore::open(
            &archive.path,
            Identity::of(archive.checkpoint()),
            |anchor| {
                n::NativeWorld::restore_synthetic_component_test(archive.profile.clone(), anchor)
            },
        )
        .unwrap();
        assert_eq!(actual, archive.recipe);
        assert_eq!(reopened.generation(), archive.generation);
        assert_eq!(
            fs::read(archive.path.join("world.checkpoint")).unwrap(),
            bytes
        );
        drop(reopened);
        Capture { archive, harness }
    }
    /// Receipt-only readback proves durable capture, not a reconstructed live owner.
    pub(crate) fn assert_recipe_readback(self) {
        let Self {
            controller,
            _harness: harness,
            recipe_path,
        } = self;
        assert!(controller.pause.is_none());
        assert!(controller.pending.is_none());
        assert!(controller.outbox.is_none());
        assert!(controller.actions.is_empty());
        assert!(controller
            .route_commit
            .as_ref()
            .is_some_and(|commit| commit.initialized && commit.pending.is_none()));
        assert!(controller.world.records().is_empty());
        let checkpoint = controller.world.checkpoint().unwrap();
        let profile = controller.world.profile().clone();
        let identity = Identity::of(&checkpoint);
        let store::Store::Recipe(recipe) = &controller.store else {
            panic!("recipe capture required")
        };
        assert_eq!(recipe.recipe().expected_checkpoint(), &checkpoint);
        assert_eq!(
            recipe.recipe().records().len() as u64,
            controller.world.state().tick
        );
        assert_eq!(
            recipe
                .recipe()
                .records()
                .iter()
                .flat_map(|record| &record.replies)
                .filter(|reply| matches!(
                    &reply.request.query,
                    n::Query::SourceMotion(n::MotionQuery::Route(_))
                ))
                .count(),
            2
        );
        let expected = recipe.recipe().clone();
        let generation = recipe.generation();
        let path = recipe_path.expect("recipe directory");
        let bytes = fs::read(path.join("world.checkpoint")).unwrap();
        assert!(
            RecipeStore::open(&path, identity.clone(), |anchor| {
                n::NativeWorld::restore_synthetic_component_test(profile.clone(), anchor)
            })
            .is_err(),
            "controller retains exclusive recipe lease"
        );
        assert_eq!(fs::read(path.join("world.checkpoint")).unwrap(), bytes);
        drop(controller);
        let (reopened, actual) = RecipeStore::open(&path, identity, |anchor| {
            n::NativeWorld::restore_synthetic_component_test(profile.clone(), anchor)
        })
        .unwrap();
        assert_eq!(actual, expected);
        assert_eq!(actual.expected_checkpoint(), &checkpoint);
        assert_eq!(reopened.generation(), generation);
        assert_eq!(fs::read(path.join("world.checkpoint")).unwrap(), bytes);
        drop(reopened);
        drop(harness);
    }
    pub(crate) fn initial_events(&mut self) -> Result<Vec<SessionEvent>, Error> {
        self.controller.initial_events()
    }
    pub(crate) fn attach_route_commit_sink(
        &mut self,
        sink: impl RouteCommitSink + 'static,
    ) -> Result<(), Error> {
        self.controller.attach_route_commit_sink(sink)
    }
    /// Match the resident runtime probe's two ordered target poses on tick one.
    /// The normal receive/FIFO path supplies the authenticated owner and sequences.
    pub(crate) fn queue_profile_target_movement(&mut self, tick: u64) -> Result<(), Error> {
        assert_eq!(tick, self.tick() + 1);
        if tick != 1 {
            return Ok(());
        }
        assert!(self.controller.actions.is_empty());
        let start = self.controller.world.profile().player.start;
        for offset in 1..=2 {
            let received = self.controller.receive(ClientCommand::Move {
                kind: MoveKind::Heartbeat,
                flags: 0,
                pos: [
                    start.x.as_f32(),
                    start.y.as_f32() - offset as f32,
                    start.z.as_f32(),
                ],
                orientation: 0.5,
                pitch: 0.0,
                fall_time: 0,
                jump: None,
                transport: None,
            })?;
            assert!(received.events.is_empty());
            assert!(!received.logout);
        }
        Ok(())
    }
    pub(crate) fn queue_movement(
        &mut self,
        destination: benilla_sim::creature_ai::Point,
        orientation: f32,
    ) -> Result<(), Error> {
        let received = self.controller.receive(ClientCommand::Move {
            kind: MoveKind::Heartbeat,
            flags: 0,
            pos: [
                destination.x.as_f32(),
                destination.y.as_f32(),
                destination.z.as_f32(),
            ],
            orientation,
            pitch: 0.0,
            fall_time: 0,
            jump: None,
            transport: None,
        })?;
        assert!(received.events.is_empty());
        assert!(!received.logout);
        Ok(())
    }
    pub(crate) fn pump_one(&mut self) -> Pump {
        self.controller.pump(self.controller.tick_duration())
    }
    pub(crate) fn pump_zero(&mut self) -> Pump {
        self.controller.pump(Duration::ZERO)
    }
    pub(crate) fn state(&self) -> &n::State {
        self.controller.world.state()
    }
    pub(crate) fn checkpoint(&self) -> n::Checkpoint {
        self.controller.world.checkpoint().unwrap()
    }
    pub(crate) fn records(&self) -> &[n::TickRecord] {
        self.controller.world.records()
    }
    pub(crate) fn pending_inputs(&self) -> Option<&[n::OwnedInput]> {
        self.controller
            .pending
            .as_ref()
            .map(|pending| pending.inputs.as_slice())
    }
    pub(crate) fn queued_actions(&self) -> usize {
        self.controller.actions.len()
    }
    pub(crate) fn route_pending(&self) -> bool {
        self.controller
            .route_commit
            .as_ref()
            .is_some_and(|commit| !commit.initialized || commit.pending.is_some())
    }
    pub(crate) fn pause(&self) -> Option<&Pause> {
        self.controller.pause.as_ref()
    }
    pub(crate) fn tick(&self) -> u64 {
        self.controller.tick()
    }
    pub(crate) fn store_generation(&self) -> u64 {
        self.controller.store.generation()
    }
    pub(crate) fn retry_route_commit(&mut self) -> Result<(), Error> {
        self.controller.retry_route_commit()
    }
    pub(crate) fn retry_tick(&mut self) -> Result<(), Error> {
        self.controller.retry_tick()
    }
}

/// Authored presentation membership mirrors exactly the supplied source profile.
/// These renderability checks supply no gameplay or spatial authority.
struct ResidentCatalog {
    manifest: runtime::Digest,
    map: u32,
    player: PlayerId,
    displays: BTreeSet<u32>,
    items: BTreeMap<u32, benilla_protocol::messages::ItemInfo>,
    spells: BTreeSet<u32>,
}
impl ResidentCatalog {
    fn new(world: &n::NativeWorld) -> Self {
        let profile = world.profile();
        let mut displays: BTreeSet<_> = profile.npcs.iter().map(|npc| npc.display_id).collect();
        let mut items = BTreeMap::new();
        for npc in world.state().npcs.values() {
            displays.insert(npc.effective_profile(profile).unwrap().display_id);
            if let Some(source) = profile
                .creature_generations
                .as_ref()
                .and_then(|catalog| catalog.source(npc.id.spawn))
            {
                let init::Rows::Complete(rows) = &source.input.displays else {
                    panic!("resident fixture needs complete creature displays")
                };
                displays.extend(rows.iter().map(|row| row.id));
                let init::Rows::Complete(rows) = &source.input.item_templates else {
                    panic!("resident fixture needs complete creature equipment rows")
                };
                for row in rows {
                    items.insert(row.entry, equipment_metadata(row));
                }
            }
        }
        let mut spells: BTreeSet<_> = world.abilities().iter().map(|ability| ability.id).collect();
        if let Some(slow) = &profile.player_slow {
            spells.extend(slow.spell_ids());
        }
        Self {
            manifest: profile.digest(),
            map: profile.map_id,
            player: world.state().player,
            displays,
            items,
            spells,
        }
    }
}
impl ClientCatalog for ResidentCatalog {
    fn build(&self) -> u32 {
        5875
    }
    fn manifest_digest(&self) -> runtime::Digest {
        self.manifest
    }
    fn contains_map(&self, id: u32) -> bool {
        id == self.map
    }
    fn contains_creature_display(&self, id: u32) -> bool {
        self.displays.contains(&id)
    }
    fn contains_item_display(&self, id: u32) -> bool {
        self.items.values().any(|item| item.display_info_id == id)
    }
    fn contains_spell(&self, id: u32) -> bool {
        self.spells.contains(&id)
    }
    fn player_power_type(&self, player: PlayerId) -> Option<u8> {
        (player == self.player).then_some(0)
    }
    fn item_metadata(&self, entry: u32) -> Option<benilla_protocol::messages::ItemInfo> {
        self.items.get(&entry).cloned()
    }
    fn quest_metadata(&self, _: u32) -> Option<benilla_world_client::QuestMetadata> {
        None
    }
    fn quest_greeting(&self, _: u32) -> Option<String> {
        None
    }
}

fn equipment_metadata(item: &init::ItemTemplateRow) -> benilla_protocol::messages::ItemInfo {
    use benilla_protocol::messages::{ItemDamage, ItemInfo};
    ItemInfo {
        class: item.class,
        subclass: item.subclass,
        name: format!("Authored resident equipment {}", item.entry),
        display_info_id: item.display_id,
        quality: 1,
        flags: 0,
        buy_price: 0,
        sell_price: 0,
        inventory_type: item.inventory_type,
        allowable_class: -1,
        allowable_race: -1,
        item_level: 1,
        required_level: 0,
        required_skill: 0,
        required_skill_rank: 0,
        required_spell: 0,
        required_honor_rank: 0,
        required_city_rank: 0,
        required_rep_faction: 0,
        required_rep_rank: 0,
        max_count: 0,
        stackable: 1,
        container_slots: 0,
        stats: vec![],
        damages: item
            .damage
            .iter()
            .filter(|damage| f32::from_bits(damage.maximum.bits()) > 0.0)
            .map(|damage| ItemDamage {
                min: f32::from_bits(damage.minimum.bits()),
                max: f32::from_bits(damage.maximum.bits()),
                school: damage.school,
            })
            .collect(),
        dmg_min: f32::from_bits(item.damage[0].minimum.bits()),
        dmg_max: f32::from_bits(item.damage[0].maximum.bits()),
        dmg_type: item.damage[0].school,
        armor: 0,
        resistances: [0; 6],
        delay_ms: item.delay_ms,
        ammo_type: 0,
        ranged_mod_range: 0.0,
        spells: vec![],
        spell_charges_0: 0,
        use_spell: None,
        bonding: 0,
        description: String::new(),
        page_text: 0,
        language_id: 0,
        page_material: 0,
        start_quest: 0,
        lock_id: 0,
        material: item.material as u32,
        sheath: item.sheath,
        random_property: 0,
        block: 0,
        item_set: 0,
        max_durability: 0,
        area: 0,
        map: 0,
        bag_family: 0,
    }
}
