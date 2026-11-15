// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
struct Drain {
    owner: MessageOwner,
    graveyards: SourceGraveyardManager,
    begun: Vec<u64>,
}
impl MessageWork for Drain {
    fn begin_message(&mut self, m: &MessageEnvelope) -> Result<()> {
        self.begun.push(m.id);
        Ok(())
    }
    fn forward_game_event(&mut self, _: u64, _: &[EventMapGuids], _: bool) -> Result<()> {
        panic!("unexpected event")
    }
    fn map_game_event(
        &mut self,
        _: u64,
        _: MapIdentity,
        _: &[u32],
        _: &[u32],
        _: bool,
    ) -> Result<()> {
        panic!("unexpected event")
    }
    fn creature_present(&self, _: MapIdentity, _: u64) -> bool {
        panic!("unexpected creature")
    }
    fn update_creature_event(
        &mut self,
        _: u64,
        _: MapIdentity,
        _: EventCreatureUpdate,
    ) -> Result<()> {
        panic!("unexpected event")
    }
    fn graveyard_manager(&mut self, owner: MessageOwner) -> Result<&mut SourceGraveyardManager> {
        assert_eq!(owner, self.owner);
        Ok(&mut self.graveyards)
    }
}
fn graveyards() -> SourceGraveyardManager {
    let mut graves = SourceGraveyardManager::default();
    graves.load_zones(
        Some(&[[927, 139, 0, 0]]),
        &GraveyardReferences {
            safe_locations: [927].into(),
            areas: [139].into(),
            maps: [0, 1].into(),
        },
    );
    graves
}
fn map(id: u32) -> MapIdentity {
    MapIdentity {
        map: id,
        instance: 0,
        creation_generation: u64::from(id) + 1,
    }
}
#[test]
fn classic_registers_only_two_main_zones_and_queues_before_any_mutation() {
    let mut manager = SourceOutdoorPvp::default();
    assert_eq!(
        manager.get_script(33).unwrap_err().id,
        "execution.gameobject.loaded_outdoor_pvp_script_owner"
    );
    let mut world = SourceMessager::new(MessageOwner::World);
    let mut work = Drain {
        owner: MessageOwner::World,
        graveyards: graveyards(),
        begun: vec![],
    };
    let census = manager
        .initialize([true, true], 1, &mut world, &mut BTreeMap::new())
        .unwrap();
    assert_eq!(
        census,
        OutdoorPvpInitCensus {
            scripts: 2,
            maps_visited: 0,
            map_messages: 0
        }
    );
    assert_eq!(manager.update_interval, 60_000);
    assert_eq!(
        manager.silithus,
        Some(SilithusPvp {
            resources: [0; 2],
            owner: 0
        })
    );
    assert_eq!(
        manager.plaguelands,
        Some(PlaguelandsPvp {
            towers: [0; 2],
            tower_world_states: [2352, 2355, 2361, 2353],
            tower_owners: [0; 4]
        })
    );
    assert_eq!(
        manager.get_script(1377).unwrap(),
        Some(OutdoorPvpScript::Silithus)
    );
    assert_eq!(
        manager.get_script(139).unwrap(),
        Some(OutdoorPvpScript::EasternPlaguelands)
    );
    for zone in [0, 33, 3428, 3429, 3478, 2017, 2057] {
        assert_eq!(manager.get_script(zone).unwrap(), None);
    }
    assert_eq!(world.pending(), 1);
    assert_eq!(work.graveyards.entries()[&139][0].team, 0);
    world.inventory().validate().unwrap();
    let result = world.execute(MessageOwner::World, 1, &mut work).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(work.begun, [1]);
    assert_eq!(world.pending(), 0);
    assert_eq!(work.graveyards.entries()[&139][0].team, 1);
}
#[test]
fn current_map_traversal_queues_only_map_zero_and_disabled_config_has_null_scripts() {
    let mut manager = SourceOutdoorPvp::default();
    let mut world = SourceMessager::new(MessageOwner::World);
    let mut maps = [0, 1]
        .into_iter()
        .map(|id| (map(id), SourceMessager::new(MessageOwner::Map(map(id)))))
        .collect();
    let census = manager
        .initialize([false, true], 1, &mut world, &mut maps)
        .unwrap();
    assert_eq!(
        census,
        OutdoorPvpInitCensus {
            scripts: 1,
            maps_visited: 1,
            map_messages: 1
        }
    );
    assert_eq!(maps[&map(1)].pending(), 0);
    let owner = MessageOwner::Map(map(0));
    let mut work = Drain {
        owner,
        graveyards: graveyards(),
        begun: vec![],
    };
    assert_eq!(work.graveyards.entries()[&139][0].team, 0);
    maps.get_mut(&map(0))
        .unwrap()
        .execute(owner, 6, &mut work)
        .unwrap();
    assert_eq!(work.graveyards.entries()[&139][0].team, 1);
    assert_eq!(manager.get_script(1377).unwrap(), None);
    let mut disabled = SourceOutdoorPvp::default();
    disabled
        .initialize([false, false], 2, &mut world, &mut BTreeMap::new())
        .unwrap();
    assert_eq!(disabled.get_script(139).unwrap(), None);
    assert_eq!(
        SourceOutdoorPvp::config(b"OutdoorPvp.SIEnabled = yes\nOutdoorPvp.EPEnabled = false\n")
            .unwrap(),
        [true, false]
    );
}
#[test]
fn graveyard_set_team_preserves_first_match_and_missing_link_quirk() {
    let mut graves = graveyards();
    assert_eq!(
        graves.set_link_team(927, 139, 469).unwrap(),
        GraveyardTeamChange::default()
    );
    assert_eq!(graves.entries()[&139][0].team, 469);
    assert_eq!(
        graves.set_link_team(999, 139, 1).unwrap(),
        GraveyardTeamChange::default()
    );
    assert_eq!(graves.entries()[&139].len(), 1);
    let change = graves.set_link_team(999, 0x8000_008b, 67).unwrap();
    assert!(change.missing_link_error);
    assert_eq!(change.insert_sql.as_deref(), Some("INSERT INTO game_graveyard_zone (id, ghost_loc, link_kind, faction) VALUES ('999', '139','2147483648', '67')"));
    // :276 passes 0x80000000 as linkKind; AddGraveYardLink's <<31 drops it.
    assert_eq!(
        graves.entries()[&139][1],
        GraveyardData {
            safe_location: 999,
            team: 67
        }
    );
    let change = graves.set_link_team(999, 0x8000_008b, 469).unwrap();
    assert!(change.missing_link_error && change.insert_sql.is_none());
    assert_eq!(graves.entries()[&139][1].team, 67);
    assert_eq!(graves.entries()[&139].len(), 2);
}

#[test]
fn continent_object_create_uses_live_entry_and_preserves_source_store_choices() {
    let mut script = SourceWorldScript {
        map: map(0),
        stores: WorldScriptStores::default(),
        state: WorldScriptState::EasternKingdoms,
    };
    assert!(!script.gameobject_create(3211, 18));
    assert_eq!(script.stores, WorldScriptStores::default());
    assert!(!script.gameobject_create(181136, 99));
    assert_eq!(script.stores.gameobject_collections[&181136], [99]);
    script.state = WorldScriptState::Kalimdor(KalimdorWorldState::default());
    for (entry, guid) in [
        (180875, 1),
        (179664, 2),
        (179665, 3),
        (179666, 4),
        (179667, 5),
    ] {
        assert!(!script.gameobject_create(entry, guid));
    }
    assert!(script.gameobject_create(177746, 6));
    assert_eq!(script.stores.gameobjects[&180875], 1);
    let WorldScriptState::Kalimdor(state) = &script.state else {
        panic!("wrong continent")
    };
    assert_eq!(state.elemental_rifts, [vec![2], vec![5], vec![4], vec![3]]);
    assert_eq!(
        state.ghost_events[0],
        GhostOPlasmEvent {
            guid: 6,
            despawn_timer: 0,
            phase_counter: 0,
            summoned_magrami: vec![]
        }
    );
}
#[test]
fn large_gameobject_predicate_reads_only_the_selected_union_offset() {
    let mut template = GameObjectTemplate {
        entry: 1,
        kind: 3,
        display: 0,
        faction: 0,
        flags: 0,
        extra_flags: 0,
        scale_bits: 1f32.to_bits(),
        data: [1; 24],
        string_id: 0,
    };
    assert!(!template.large());
    for (kind, index) in [
        (1, 5),
        (2, 9),
        (5, 3),
        (6, 10),
        (8, 5),
        (10, 13),
        (22, 4),
        (29, 18),
    ] {
        template.kind = kind;
        template.data = [0; 24];
        assert!(!template.large());
        template.data[index] = 1;
        assert!(template.large());
    }
    template.kind = 11;
    template.data = [0; 24];
    assert!(template.large());
}

#[test]
fn missing_graveyard_inserts_memory_before_unbound_world_sql_refusal() {
    for (key, expected_kind) in [(140, 0), (0x8000_008c, 2147483648u32)] {
        let mut messages = SourceMessager::new(MessageOwner::World);
        let mut work = Drain {
            owner: MessageOwner::World,
            graveyards: graveyards(),
            begun: vec![],
        };
        let change = work.graveyards.set_link_team(999, key, 67).unwrap();
        assert_eq!(change.insert_sql,Some(format!("INSERT INTO game_graveyard_zone (id, ghost_loc, link_kind, faction) VALUES ('999', '140','{expected_kind}', '67')")));
        work.graveyards = graveyards();
        messages.enqueue_graveyard_team(1, 999, key, 67).unwrap();
        assert_eq!(
            messages
                .execute(MessageOwner::World, 1, &mut work)
                .unwrap_err()
                .id,
            "execution.graveyard.GraveyardManager_247.insert_sql_owner"
        );
        assert_eq!(
            work.graveyards.entries()[&140],
            [GraveyardData {
                safe_location: 999,
                team: 67
            }]
        );
    }
}
#[test]
fn stock_outdoor_constructor_world_drain_never_inserts_a_graveyard() {
    let Some(root) = std::env::var_os("BENILLA_F2_CATALOG") else {
        eprintln!("SKIP stock_outdoor_constructor_world_drain_never_inserts_a_graveyard: BENILLA_F2_CATALOG absent");
        return;
    };
    let mut catalog =
        super::super::super::production_compiler::VerifiedCatalog::open(Path::new(&root), false)
            .unwrap();
    let mut graves = SourceGraveyardManager::default();
    graves.load_catalog(&mut catalog).unwrap();
    let mut work = Drain {
        owner: MessageOwner::World,
        graveyards: graves,
        begun: vec![],
    };
    let mut messages = SourceMessager::new(MessageOwner::World);
    SourceOutdoorPvp::default()
        .initialize([true, true], 1, &mut messages, &mut BTreeMap::new())
        .unwrap();
    assert_eq!(messages.pending(), 1);
    messages.execute(MessageOwner::World, 1, &mut work).unwrap();
    assert_eq!(
        work.graveyards
            .entries()
            .values()
            .map(Vec::len)
            .sum::<usize>(),
        190
    );
    println!(
        "F6_OUTDOOR_PVP_WORLD_DRAIN queued=1 completed=1 stock_GraveyardManager_247_insert_reach=0"
    );
}
