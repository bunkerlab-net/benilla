// SPDX-License-Identifier: GPL-3.0-or-later
//! App-only resident controller fixture. Geometry and commit acknowledgements come
//! exclusively from the supplied channels, never from this presentation catalog.
use super::*;
use benilla_sim::creature_init as init;
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
            _harness: self.harness,
        })
    }
}

pub(crate) struct Driver {
    // Drop the exclusive world lock before the harness removes its test directory.
    controller: NativeController,
    _harness: Harness,
}
impl Driver {
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
