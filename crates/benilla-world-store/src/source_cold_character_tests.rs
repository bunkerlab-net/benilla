// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_sim::source_rng::{DrawCursor, Mt19937, SourceRandom};
use benilla_world_runtime::native::source_execution::{
    self as source,
    character_login::{
        create_character_after_preflight, SourceCharacterCreateCast, SourceCharacterCreateError,
        SourceCharacterCreateInputs, SourceCharacterCreateObservation,
    },
    player::{
        ItemLifetimeObservation, PlayerCreateRows, SourceItemState, SourceItems, SourcePlayer,
        SourcePlayerCreateConfig, SourcePlayerCreation, SourcePlayerItemInputs,
        SourcePlayerSaveError, SourcePlayerSaveInputs, SourcePlayerSaveObservation,
    },
    *,
};

const MAP: MapIdentity = MapIdentity {
    map: 0,
    instance: 0,
    creation_generation: 1,
};
const THREAD: u64 = 9;
const CONFIG: SourcePlayerCreateConfig = SourcePlayerCreateConfig {
    save_interval: 900_000,
    start_level: 1,
    start_money: 0,
    max_primary_professions: 2,
};

struct Work;
impl WorldStartupWork for Work {
    fn clock(&mut self, site: WorldStartupClockSite) -> source::Result<ClockSample> {
        panic!("unexpected wall call {site:?}");
    }

    fn observe(&mut self, _: WorldStartupObservation) -> source::Result<()> {
        Ok(())
    }
}

fn guid_allocators() -> GlobalGuidAllocators {
    GlobalGuidAllocators::from_seed_rows(GuidSeedRows {
        player: GuidMaximum::NoResult,
        item: GuidMaximum::Value(1),
        corpse: GuidMaximum::NoResult,
        pet_number: GuidMaximum::NoResult,
        creature: GuidMaximum::NoResult,
        gameobject: GuidMaximum::NoResult,
        creature_reserve: 0,
        gameobject_reserve: 0,
    })
}

fn request() -> SourcePlayerCreation {
    SourcePlayerCreation {
        guid_low: 99, // CharacterHandler.cpp:495 supplies its own GUID.
        name: "Authoredmage".into(),
        race: 1,
        class: 8,
        gender: 0,
        appearance: [0; 5],
    }
}

struct Fixture {
    objects: SourceObjects,
    units: SourceUnits,
    items: SourceItems,
    session: SourceWorldSession,
    spells: SourceSpellStores,
    quests: SourceQuestMap,
    clock: SourceWorldClock,
    world: SourceWorld,
    player_rows: PlayerCreateRows,
    templates: SourceStartupItems,
    models: LoadedCreatureModels,
    cells: SourceCells,
    guids: GlobalGuidAllocators,
    item_events: Vec<ItemLifetimeObservation>,
}

impl Fixture {
    fn new() -> Self {
        let mut session = SourceWorldSession::construct(identity().account, true);
        session.set_in_char_selection();
        session.bind_tutorials(None);
        session.set_tutorial_word(4, 23).unwrap();
        let mut templates = SourceStartupItems::default();
        templates
            .validate(
                [(
                    19,
                    StartupItemPrototype {
                        class: 15,
                        allowable_class: 1 << 7,
                        allowable_race: 1,
                        buy_count: 3,
                        stackable: 5,
                        spell_charges: [-1, 0, 0, 0, 0],
                        ..Default::default()
                    },
                )]
                .into(),
                &Default::default(),
                &mut SourceRandom::new(
                    THREAD,
                    &mut Mt19937::from_seed(1),
                    &mut DrawCursor::default(),
                    &mut |_| {},
                )
                .unwrap(),
            )
            .unwrap();
        Self {
            objects: SourceObjects::default(),
            units: SourceUnits::default(),
            items: SourceItems::default(),
            session,
            spells: SourceSpellStores::default(),
            quests: SourceQuestMap::default(),
            clock: SourceWorldClock::default(),
            world: SourceWorld::new(12, 200).unwrap(),
            player_rows: PlayerCreateRows::authored_save_fixture(),
            templates,
            models: LoadedCreatureModels::load(
                Vec::new(),
                &Default::default(),
                &Default::default(),
            )
            .unwrap(),
            cells: SourceCells::new(MAP).unwrap(),
            guids: guid_allocators(),
            item_events: Vec::new(),
        }
    }

    fn create(
        &mut self,
        store: &mut SourceColdStore,
        observe: impl FnMut(SourceCharacterCreateObservation),
    ) -> source::Result<Receipt, SourceCharacterCreateError<Error>> {
        let mut mt = Mt19937::from_seed(1);
        let mut draws = DrawCursor::default();
        create_character_after_preflight(
            request(),
            &mut SourceCharacterCreateInputs {
                save: SourcePlayerSaveInputs {
                    objects: &mut self.objects,
                    units: &mut self.units,
                    items: &mut self.items,
                    session: &mut self.session,
                    quests: &self.quests,
                    spells: &self.spells,
                    clock: &self.clock,
                    world: &self.world,
                    calendar: Calendar::new(CalendarOwner::World, 10),
                    maintenance_weekday: 3,
                    save_interval: CONFIG.save_interval,
                    world_work: &mut Work,
                    wall_seconds: &mut || Ok(101),
                },
                cells: &mut self.cells,
                item_events: &mut |event| self.item_events.push(event),
                guids: &mut self.guids,
                player_rows: &self.player_rows,
                item_templates: &self.templates,
                models: &self.models,
                config: CONFIG,
                cast: SourceCharacterCreateCast {
                    targets: &SourceSpellTargetMgr::initialize(&self.spells),
                    terrain: &mut SourceTerrainManager::default(),
                    map_time: &|_| panic!("no create cast"),
                    aura_seconds: &mut |_| panic!("no create aura"),
                    max_casts: 20,
                    indoor_policy: true,
                },
                thread: THREAD,
                have_same_race: false,
                skip_cinematics: 2,
            },
            &mut SourceRandom::new(THREAD, &mut mt, &mut draws, &mut |_| {}).unwrap(),
            store,
            observe,
        )
    }

    fn pending_tutorial(&self) -> Vec<SourcePlayerSaveObservation> {
        let mut pending = Vec::new();
        self.session
            .save_tutorials_data(&mut |row| {
                pending.push(row);
                Ok(())
            })
            .unwrap();
        pending
    }

    fn assert_detached_cleanup(&self) {
        // Player.cpp:9418-9429 destroys the equip probe before storing the stack.
        let probe = SourceGuid::without_entry(HighGuid::Item, 2);
        let guid = SourceGuid::without_entry(HighGuid::Item, 3);
        assert_eq!(
            self.item_events,
            [
                ItemLifetimeObservation::Assigned {
                    guid: probe,
                    entry: 19,
                },
                ItemLifetimeObservation::Destroyed { guid: probe },
                ItemLifetimeObservation::Assigned { guid, entry: 19 },
                ItemLifetimeObservation::Destroyed { guid }
            ]
        );
        let player = ObjectHandle {
            thread: THREAD,
            construction: 1,
        };
        let item = ObjectHandle {
            thread: THREAD,
            construction: 3,
        };
        assert!(self.objects.get(player).is_err());
        assert!(self.objects.get(item).is_err());
        assert!(self.items.get(item).is_err());
    }
}

#[test]
fn character_create_reopens_every_written_family() {
    use SourceCharacterCreateObservation::*;
    let temp = Temp::new();
    let (mut store, initial) = temp.create();
    let old = fs::read(temp.file(CURRENT)).unwrap();
    let mut game = Fixture::new();
    let mut observations = Vec::new();
    let receipt = game
        .create(&mut store, |event| {
            assert_ne!(fs::read(temp.file(CURRENT)).unwrap(), old);
            observations.push(event);
        })
        .unwrap();
    game.assert_detached_cleanup();
    assert!(game.pending_tutorial().is_empty());
    assert_eq!(receipt.generation(), initial.generation() + 1);
    assert_eq!(
        receipt.digest(),
        hash(&fs::read(temp.file(CURRENT)).unwrap())
    );
    assert_eq!(
        receipt
            .operation_counts()
            .iter()
            .map(|row| row.affected)
            .collect::<Vec<_>>(),
        [0, 1, 1, 1, 0, 1, 0, 1, 0, 1]
    );
    assert_eq!(
        observations,
        [
            ResetRosterCount { account: 1 },
            RosterEntry {
                account: 1,
                guid: 1
            },
            Packet {
                opcode: 0x03a,
                result: 0x2e
            }
        ]
    ); // C/client-packets.jsonl:8.

    assert!(matches!(
        game.create(&mut store, |event| panic!(
            "duplicate create emitted {event:?}"
        )),
        Err(SourceCharacterCreateError::Save(
            SourcePlayerSaveError::Storage(Error::AlreadyExists)
        ))
    ));
    assert_eq!(store.generation(), receipt.generation());
    assert!(!store.requires_reopen());
    drop(store);

    let (reopened, rows) = temp.open().unwrap();
    assert_eq!(reopened.generation(), receipt.generation());
    let character = rows.character().unwrap();
    assert_eq!(
        (character.account, character.guid, character.name.as_str()),
        (1, 1, "Authoredmage")
    );
    assert_eq!(
        (character.cinematic, character.at_login, character.online),
        (1, 0x20, false)
    );
    assert_eq!(character.xyz.map(FloatWord::as_float), [1.0, 2.0, 3.0]);
    assert_eq!(
        rows.inventory(),
        [InventoryRow {
            guid: 1,
            bag_guid: 0,
            slot: 23,
            item_guid: 3,
            item_template: 19
        }]
    );
    assert_eq!(
        rows.items(),
        [ItemInstanceRow {
            guid: 3,
            owner_guid: 1,
            entry: 19,
            creator: 0,
            gift_creator: 0,
            count: 3,
            duration: 0,
            charges: [-1, 0, 0, 0, 0],
            flags: 0,
            enchantments: [[0; 3]; 7],
            random_property: 0,
            durability: 0,
            text_id: 0
        }]
    );
    assert_eq!(
        rows.skills(),
        [SkillRow {
            guid: 1,
            skill: 98,
            value: 300,
            max: 300
        }]
    );
    assert_eq!(
        rows.actions(),
        [ActionRow {
            guid: 1,
            button: 11,
            action: 19,
            kind: 128
        }]
    );
    assert_eq!(
        rows.tutorial(),
        Some(&TutorialRow {
            account: 1,
            words: [0, 0, 0, 0, 23, 0, 0, 0]
        })
    );
    assert!(rows.quests().is_empty());
    assert!(rows.reputation().is_empty());
}

#[test]
fn character_create_failed_publish_blocks_until_strict_reopen() {
    for fault in [SaveFault::BeforeRename, SaveFault::BeforeVerification] {
        let temp = Temp::new();
        let (mut store, initial) = temp.create();
        let old = fs::read(temp.file(CURRENT)).unwrap();
        let mut game = Fixture::new();
        let dirty = game.pending_tutorial();
        store.storage.inject_fault(fault);
        let outcome = game.create(&mut store, |event| {
            panic!("failed publish emitted {event:?}")
        });
        let Err(SourceCharacterCreateError::Save(SourcePlayerSaveError::Storage(error))) = outcome
        else {
            panic!("expected checked publication error, got {outcome:?}");
        };
        let published = fault == SaveFault::BeforeVerification;
        if published {
            assert!(matches!(error, Error::CommitUncertain));
            assert_ne!(fs::read(temp.file(CURRENT)).unwrap(), old);
        } else {
            assert!(matches!(error, Error::Io(_)));
            assert_eq!(fs::read(temp.file(CURRENT)).unwrap(), old);
        }
        game.assert_detached_cleanup();
        assert_eq!(dirty.len(), 1);
        assert_eq!(game.pending_tutorial(), dirty); // WorldSession.cpp:1183 did not run.
        assert_eq!(store.generation(), initial.generation());
        assert!(store.requires_reopen());
        assert!(matches!(store.read_rows(), Err(Error::CommitUncertain)));
        let constructions = game.objects.construction_count(THREAD);
        let blocked = game.create(&mut store, |event| {
            panic!("blocked retry emitted {event:?}")
        });
        assert!(matches!(
            blocked,
            Err(SourceCharacterCreateError::Save(
                SourcePlayerSaveError::Storage(Error::CommitUncertain)
            ))
        ));
        assert_eq!(game.objects.construction_count(THREAD), constructions);
        assert_eq!(game.pending_tutorial(), dirty);
        drop(store);

        let (reopened, rows) = temp.open().unwrap();
        assert!(!reopened.requires_reopen());
        assert_eq!(
            reopened.generation(),
            initial.generation() + u64::from(published)
        );
        assert_eq!(rows.character().is_some(), published);
        assert_eq!(rows.inventory().len(), usize::from(published));
        assert_eq!(rows.tutorial().is_some(), published);
    }
}

#[test]
fn character_create_removed_item_refuses_before_publication() {
    let temp = Temp::new();
    let (mut store, initial) = temp.create();
    let old = fs::read(temp.file(CURRENT)).unwrap();
    let mut game = Fixture::new();
    let mut player = SourcePlayer::construct(
        &mut game.objects,
        &mut game.units,
        THREAD,
        CONFIG.save_interval,
        |min, _| Ok(min),
        || Ok(101),
    )
    .unwrap();
    player
        .create(
            &mut game.objects,
            &mut game.units,
            SourcePlayerCreation {
                guid_low: 1,
                ..request()
            },
            &game.player_rows,
            &game.spells,
            &game.templates,
            &game.models,
            &game.cells,
            CONFIG,
            None,
            Some(SourcePlayerItemInputs {
                items: &mut game.items,
                guids: &mut game.guids,
                observe: &mut |event| game.item_events.push(event),
                logging_out: false,
            }),
            || Ok(101),
        )
        .unwrap();
    let removed = game
        .items
        .authored_removed_save_item(&mut player, &mut game.objects, 4)
        .unwrap();
    let outcome = player.save_created_to_db(
        &mut SourcePlayerSaveInputs {
            objects: &mut game.objects,
            units: &mut game.units,
            items: &mut game.items,
            session: &mut game.session,
            quests: &game.quests,
            spells: &game.spells,
            clock: &game.clock,
            world: &game.world,
            calendar: Calendar::new(CalendarOwner::World, 10),
            maintenance_weekday: 3,
            save_interval: CONFIG.save_interval,
            world_work: &mut Work,
            wall_seconds: &mut || Ok(101),
        },
        &mut store,
    );
    let Err(SourcePlayerSaveError::Source(error)) = outcome else {
        panic!("removed Item was not refused, got {outcome:?}");
    };
    assert_eq!(
        error.id,
        "execution.player_save.Item_351.destroy_removed_owner"
    );
    let placed = ObjectHandle {
        thread: THREAD,
        construction: 3,
    };
    assert_eq!(
        game.items.get(placed).unwrap().authored_save_state(),
        (SourceItemState::New, Some(0))
    );
    assert_eq!(
        game.items.get(removed).unwrap().authored_save_state(),
        (SourceItemState::Removed, Some(1))
    );
    assert_eq!(game.pending_tutorial().len(), 1);
    assert_eq!(fs::read(temp.file(CURRENT)).unwrap(), old);
    assert_eq!(store.generation(), initial.generation());
    assert!(!store.requires_reopen());
    assert!(store.read_rows().unwrap().character().is_none());
}
