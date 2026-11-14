use super::*;
use std::collections::BTreeSet;
#[derive(Default)]
struct Known {
    missing: BTreeSet<(DbScriptLookup, u32)>,
    quests: BTreeMap<u32, bool>,
    taxi: Option<u32>,
    calls: Vec<DbScriptLookup>,
}
impl DbScriptLoaderOwner for Known {
    fn contains(&mut self, kind: DbScriptLookup, id: u32) -> Result<bool> {
        self.calls.push(kind);
        Ok(id != 999
            && !(matches!(
                kind,
                DbScriptLookup::CreaturePool | DbScriptLookup::GameObjectPool
            ) && id == 9)
            && !self.missing.contains(&(kind, id)))
    }
    fn quest_exploration(&mut self, id: u32) -> Result<Option<bool>> {
        Ok((id != 999).then(|| *self.quests.get(&id).unwrap_or(&false)))
    }
    fn set_quest_exploration(&mut self, id: u32) -> Result<()> {
        self.quests.insert(id, true);
        Ok(())
    }
    fn gameobject_entry(&mut self, guid: u32) -> Result<Option<u32>> {
        Ok((guid != 999).then_some(if guid == 1000 { 999 } else { guid }))
    }
    fn gameobject_type(&mut self, entry: u32) -> Result<Option<u32>> {
        Ok(match entry {
            999 => None,
            1 => Some(0),
            2 => Some(1),
            3 => Some(17),
            4 => Some(25),
            _ => Some(3),
        })
    }
    fn max_pool_id(&mut self) -> Result<u32> {
        Ok(10)
    }
    fn taxi_spell(&mut self, _: u32) -> Result<Option<u32>> {
        Ok(self.taxi)
    }
}
struct Missing;
impl DbScriptLoaderOwner for Missing {}
fn row(command: u32, a: u32, b: u32) -> DbScriptRow {
    DbScriptRow {
        id: 7,
        command,
        data: [a, b, 0],
        text: [1, 0, 0, 0],
        ..Default::default()
    }
}
fn templates() -> SourceDbScriptTemplates {
    SourceDbScriptTemplates::load([
        DbScriptRandomRow {
            ordinal: 0,
            id: 8,
            kind: 1,
            target: 7,
            chance: 0,
        },
        DbScriptRandomRow {
            ordinal: 1,
            id: 8,
            kind: 1,
            target: 9,
            chance: 50,
        },
    ])
    .unwrap()
}
fn load(rows: Vec<DbScriptRow>, owner: &mut impl DbScriptLoaderOwner) -> Result<LoadedDbScriptMap> {
    LoadedDbScriptMap::load(DbScriptType::Relay, rows, &templates(), None, owner)
}
fn dropped(r: DbScriptRow, reason: &str) {
    let map = load(vec![r], &mut Known::default()).unwrap();
    assert_eq!(map.loaded_count(), 0);
    assert_eq!(map.rows()[0].disposition, DbScriptDisposition::Dropped);
    assert_eq!(map.rows()[0].logs.last().unwrap().reason, reason);
}
#[test]
fn source_map_orders_ids_then_delays_and_keeps_equal_delay_cursor_order() {
    // ScriptMgr.h:653-655; ScriptMgr.cpp:118,989. Ordinal is identity, not a sort key.
    let mut rows = vec![row(18, 1, 0), row(18, 2, 0), row(18, 3, 0), row(18, 4, 0)];
    for (r, (id, delay, ordinal)) in
        rows.iter_mut()
            .zip([(9, 2, 50), (7, 5, 8), (7, 1, 9), (7, 5, 3)])
    {
        r.id = id;
        r.delay = delay;
        r.ordinal = ordinal;
    }
    let map = load(rows, &mut Known::default()).unwrap();
    assert_eq!(map.scripts().keys().copied().collect::<Vec<_>>(), [7, 9]);
    assert_eq!(
        map.scripts()[&7]
            .iter()
            .map(|r| (r.delay, r.data[0], r.ordinal))
            .collect::<Vec<_>>(),
        [(1, 3, 9), (5, 2, 8), (5, 4, 3)]
    );
    assert_eq!(map.loaded_count(), 4);
}
#[test]
fn every_validated_command_reproduces_source_drop_reason() {
    // Source ScriptMgr.cpp:268-980, including the source's non-checks.
    let mut cases = vec![
        (row(0, 0, 0), "talk_text_zero"),
        (row(1, 999, 0), "emote_missing"),
        (row(6, 999, 0), "map_missing"),
        (row(7, 999, 0), "quest_missing"),
        (row(7, 1, 101), "quest_distance_too_large"),
        (row(7, 1, 4), "quest_distance_too_small"),
        (row(8, 999, 0), "kill_credit_creature_missing"),
        (row(9, 0, 0), "gameobject_and_buddy_missing"),
        (row(9, 999, 0), "gameobject_guid_missing"),
        (row(9, 1000, 0), "gameobject_template_missing"),
        (row(9, 1, 0), "gameobject_type_unsupported"),
        (row(10, 999, 0), "summon_creature_missing"),
        (row(11, 0, 0), "gameobject_and_buddy_missing"),
        (row(11, 999, 0), "gameobject_guid_missing"),
        (row(11, 1000, 0), "gameobject_template_missing"),
        (row(11, 5, 0), "gameobject_type_unsupported"),
        (row(12, 5, 0), "gameobject_type_unsupported"),
        (row(14, 999, 0), "remove_aura_spell_missing"),
        (row(14, 1, 3), "remove_aura_mode_invalid"),
        (row(15, 999, 0), "cast_spell_missing"),
        (row(16, 999, 0), "sound_missing"),
        (row(17, 999, 1), "item_missing"),
        (row(17, 1, 0), "item_amount_zero"),
        (row(19, 1, 0), "movie_unsupported"),
        (row(20, 5, 0), "movement_invalid"),
        (row(22, 999, 0), "faction_missing"),
        (row(23, 999, 0), "morph_or_mount_missing"),
        (row(24, 999, 0), "morph_or_mount_missing"),
        (row(27, 0, 0), "lock_state_invalid"),
        (row(28, 10, 0), "stand_state_invalid"),
        (row(30, 999, 0), "taxi_path_missing"),
        (row(31, 999, 0), "terminate_buddy_missing"),
        (row(34, 999, 0), "terminate_condition_missing"),
        (row(34, 1, 999), "terminate_quest_missing"),
        (row(37, 1, 2), "dynamic_distance_reversed"),
        (row(38, 999, 0), "mail_template_missing"),
        (row(38, 1, 999), "mail_sender_missing"),
        (row(44, 999, 0), "update_template_missing"),
        (row(45, 0, 999), "relay_template_missing"),
        (row(46, 999, 0), "custom_spell_missing"),
        (row(47, 4, 0), "current_spell_type_invalid"),
        (row(51, 100, 7), "formation_shape_invalid"),
        (row(51, 151, 999), "formation_group_missing"),
        (row(51, 99, 0), "formation_command_unknown"),
        (row(53, 0, 0), "worldstate_name_missing"),
        (row(54, 3, 0), "sheathe_invalid"),
        (row(55, 999, 0), "string_id_missing"),
        (row(55, 1, 2), "string_apply_invalid"),
        (row(58, 999, 0), "spawn_group_missing"),
        (row(200, 999, 0), "area_missing"),
        (row(56, 0, 0), "command_unknown"),
    ];
    cases[0].0.text[0] = 0;
    let worldstate = cases.iter_mut().find(|(r, _)| r.command == 53).unwrap();
    worldstate.0.text[0] = 999;
    let mut extra = row(15, 1, 0);
    extra.text = [999, -1, 0, 0];
    cases.push((extra, "cast_alternative_missing"));
    let mut extra = row(20, 1, 0);
    extra.text[0] = -1;
    cases.push((extra, "forced_movement_invalid"));
    let mut extra = row(31, 1, 0);
    extra.data[2] = 11;
    cases.push((extra, "terminate_pool_invalid"));
    let mut extra = row(42, 0, 0);
    extra.text[2] = -1;
    cases.push((extra, "equipment_negative"));
    let mut extra = row(46, 1, 0);
    extra.text = [0; 4];
    cases.push((extra, "custom_basepoints_zero"));
    let mut extra = row(51, 150, 0);
    extra.text[0] = -1;
    cases.push((extra, "formation_shape_invalid"));
    let mut extra = row(51, 101, 0);
    extra.floats[1] = 0.49f32.to_bits();
    cases.push((extra, "formation_spread_invalid"));
    for command in [6, 10] {
        let mut extra = row(command, 1, 0);
        extra.floats[1] = f32::INFINITY.to_bits();
        cases.push((
            extra,
            if command == 6 {
                "teleport_coordinates_invalid"
            } else {
                "summon_coordinates_invalid"
            },
        ));
    }
    for (r, reason) in cases {
        dropped(r, reason);
    }
}
#[test]
fn generic_buddy_checks_follow_source_precedence_and_flag_masks() {
    let mut r = row(18, 0, 0);
    r.condition = 999;
    dropped(r, "condition_missing");
    for command in [18, 9] {
        let mut r = row(command, 0, 0);
        r.buddy = 999;
        r.radius_or_guid = 1;
        dropped(r, "buddy_template_missing");
        let mut r = row(command, 0, 0);
        r.buddy = 1;
        dropped(r, "buddy_radius_zero");
        let mut r = row(command, 0, 0);
        r.flags = 0x10;
        r.radius_or_guid = 999;
        dropped(r, "buddy_guid_missing");
        let mut r = row(command, 0, 0);
        r.flags = 0x80;
        r.radius_or_guid = 9;
        dropped(r, "buddy_pool_empty");
    }
    let mut r = row(18, 0, 0);
    r.flags = 0x1000;
    dropped(r, "flags_invalid");
    let mut r = row(46, 1, 0);
    r.flags = 8;
    dropped(r, "additional_flag_invalid");
    let mut r = row(18, 0, 0);
    r.flags = 1;
    dropped(r, "buddy_required");
    let mut r = row(18, 0, 0);
    r.flags = 0x100;
    r.buddy = 999;
    dropped(r, "buddy_group_missing");
    let mut r = row(18, 0, 0);
    r.flags = 0x800;
    r.buddy = 999;
    dropped(r, "buddy_string_missing");
    // BY_POOL does not bypass the earlier template/radius checks, :168-184.
    let mut r = row(18, 0, 0);
    r.flags = 0x80;
    r.buddy = 1;
    dropped(r, "buddy_radius_zero");
    // BY_GUID wins over group/string/pool and ignores an unrelated buddy entry.
    let mut r = row(18, 0, 0);
    r.flags = 0x10 | 0x80 | 0x100 | 0x800;
    r.buddy = 999;
    r.radius_or_guid = 1;
    let mut owner = Known::default();
    assert_eq!(load(vec![r], &mut owner).unwrap().loaded_count(), 1);
    assert_eq!(owner.calls, [DbScriptLookup::CreatureGuid]);
    for command in [31, 36, 37] {
        let mut r = row(command, 0, 0);
        r.flags = 0x400;
        r.buddy = 1;
        r.radius_or_guid = 1;
        let mut owner = Known::default();
        load(vec![r], &mut owner).unwrap();
        assert_eq!(owner.calls[0], DbScriptLookup::GameObject);
    }
}
#[test]
fn warning_only_validation_does_not_drop_or_repair_operands() {
    let mut emote = row(1, 1, 0);
    emote.text = [999, -1, 0, 999];
    let rows = vec![emote, row(16, 1, 0x25), row(29, 1, 3), row(48, 1, 3)];
    let map = load(rows.clone(), &mut Known::default()).unwrap();
    assert_eq!(map.scripts()[&7], rows);
    assert_eq!(
        map.rows().iter().map(|r| r.logs.len()).collect::<Vec<_>>(),
        [2, 2, 1, 1]
    );
    assert_eq!(map.rows()[2].logs[0].source_line, 645); // :644-646 logs "skipping", but has no continue.
    assert_eq!(map.rows()[3].logs[0].source_line, 825);
}
#[test]
fn quest_flag_is_mutated_before_distance_drop_and_only_once() {
    let mut owner = Known::default();
    let map = load(
        vec![row(7, 1, 101), row(7, 1, 100), row(7, 2, 0), row(7, 3, 5)],
        &mut owner,
    )
    .unwrap();
    assert_eq!(map.loaded_count(), 3);
    assert_eq!(
        map.rows()[0]
            .logs
            .iter()
            .map(|l| l.reason)
            .collect::<Vec<_>>(),
        ["quest_exploration_flag_set", "quest_distance_too_large"]
    );
    assert!(map.rows()[1].logs.is_empty());
    assert_eq!(
        owner.quests,
        BTreeMap::from([(1, true), (2, true), (3, true)])
    );
}
#[test]
fn command_boundaries_and_source_nonchecks_remain_defined() {
    for state in [1, 2, 4, 5, 6, 8, 9, 10] {
        assert_eq!(
            load(vec![row(27, state, 0)], &mut Known::default())
                .unwrap()
                .loaded_count(),
            1
        );
    }
    for state in [3, 7, 11, 12, 15, 16] {
        dropped(row(27, state, 0), "lock_state_invalid");
    }
    for movement in [0, 1, 2, 3, 4, 15, 18] {
        assert_eq!(
            load(vec![row(20, movement, 0)], &mut Known::default())
                .unwrap()
                .loaded_count(),
            1
        );
    }
    for command in [9, 11, 12] {
        let valid = if command == 9 { 5 } else { 1 };
        assert_eq!(
            load(vec![row(command, valid, 0)], &mut Known::default())
                .unwrap()
                .loaded_count(),
            1
        );
    }
    for entry in [1, 3, 4] {
        dropped(row(9, entry, 0), "gameobject_type_unsupported");
    }
    for edge in [0.5f32, 15.0, f32::NAN] {
        let mut r = row(51, 101, 0);
        r.floats[1] = edge.to_bits();
        assert_eq!(
            load(vec![r], &mut Known::default()).unwrap().loaded_count(),
            1
        ); // :891 ordered comparisons accept NaN.
    }
    let mut r = row(51, 150, 0);
    r.text[0] = 6;
    assert_eq!(
        load(vec![r], &mut Known::default()).unwrap().loaded_count(),
        1
    );
    let mut r = row(51, 150, 0);
    r.text[0] = 7;
    dropped(r, "formation_shape_invalid");
    for command in [6, 10] {
        let mut r = row(command, 1, 0);
        r.floats[1] = (17066.0f32).to_bits();
        r.floats[3] = f32::MAX.to_bits();
        r.floats[4] = f32::MAX.to_bits();
        assert_eq!(
            load(vec![r.clone()], &mut Known::default())
                .unwrap()
                .loaded_count(),
            1
        );
        r.floats[2] = (17067.0f32).to_bits();
        dropped(
            r,
            if command == 6 {
                "teleport_coordinates_invalid"
            } else {
                "summon_coordinates_invalid"
            },
        );
    }
    for command in [
        2, 3, 4, 5, 13, 18, 21, 25, 26, 32, 33, 35, 36, 39, 40, 41, 43, 49, 50, 52,
    ] {
        assert_eq!(
            load(vec![row(command, u32::MAX, u32::MAX)], &mut Missing)
                .unwrap()
                .loaded_count(),
            1
        );
    }
    let mut owner = Known {
        taxi: Some(7),
        ..Default::default()
    };
    let map = load(vec![row(30, 1, 0)], &mut owner).unwrap();
    assert_eq!(map.loaded_count(), 0);
    assert_eq!(map.rows()[0].logs[0].reason, "taxi_spell_exists");
    assert_eq!(
        load(vec![row(30, 1, 0)], &mut Known::default())
            .unwrap()
            .loaded_count(),
        1
    );
}
#[test]
fn missing_owners_refuse_at_first_reached_query_without_partial_publication() {
    for (r, name) in [
        (row(1, 1, 0), "emote"),
        (row(6, 1, 0), "map"),
        (row(7, 1, 0), "effective_quest_flags"),
        (row(9, 1, 0), "gameobject_spawn"),
        (row(14, 1, 0), "spell_template"),
        (row(16, 1, 0), "sound"),
        (row(17, 1, 1), "item_prototype"),
        (row(22, 1, 0), "faction_template"),
        (row(30, 1, 0), "taxi_path"),
        (row(38, 1, 0), "mail_template"),
        (row(55, 1, 0), "string_id"),
        (row(58, 1, 0), "spawn_group"),
        (row(200, 1, 0), "area"),
    ] {
        assert_eq!(
            load(vec![r], &mut Missing).unwrap_err().id,
            format!("execution.db_scripts.{name}_owner")
        );
    }
    let report = LoadedDbScriptMap::inspect(
        DbScriptType::Relay,
        vec![row(18, 0, 0), row(1, 1, 0)],
        &templates(),
        None,
        &mut Missing,
    );
    assert_eq!(report.validated_count(), 1);
    assert!(report.into_loaded().is_err());
    struct FirstOnly;
    impl DbScriptLoaderOwner for FirstOnly {
        fn contains(&mut self, _: DbScriptLookup, _: u32) -> Result<bool> {
            Err(missing("first"))
        }
        fn quest_exploration(&mut self, _: u32) -> Result<Option<bool>> {
            panic!("loader continued after first refusal")
        }
    }
    assert_eq!(
        load(vec![row(1, 1, 0), row(7, 1, 0)], &mut FirstOnly)
            .unwrap_err()
            .id,
        "execution.db_scripts.first_owner"
    );
}
#[test]
fn relays_load_first_nested_missing_ids_log_only_and_movement_drops_missing_ids() {
    let maps = SourceDbScriptMaps::load(
        vec![
            (
                DbScriptType::CreatureMovement,
                vec![row(45, 7, 8), row(45, 999, 0)],
            ),
            (DbScriptType::Relay, vec![row(45, 999, 0)]),
        ],
        templates(),
        &mut Known::default(),
    )
    .unwrap();
    assert_eq!(maps.map(DbScriptType::Relay).unwrap().loaded_count(), 1);
    assert_eq!(
        maps.map(DbScriptType::CreatureMovement)
            .unwrap()
            .loaded_count(),
        1
    );
    assert_eq!(
        maps.relay_logs()
            .iter()
            .map(|r| r.reason)
            .collect::<Vec<_>>(),
        ["nested_relay_missing", "template_relay_missing"]
    );
    maps.validate().unwrap();
    assert_eq!(maps.inventory()["version"], 1);
    assert_eq!(
        maps.map(DbScriptType::QuestEnd).unwrap_err().id,
        "execution.db_scripts.dbscripts_on_quest_end_owner"
    );
    assert_eq!(
        LoadedDbScriptMap::load(
            DbScriptType::CreatureMovement,
            vec![row(45, 1, 0)],
            &templates(),
            None,
            &mut Known::default()
        )
        .unwrap_err()
        .id,
        "execution.db_scripts.loaded_relay_map_owner"
    );
}
#[test]
fn talk_loader_does_not_depend_on_later_broadcast_or_template_checks() {
    // :268-277 defers validation to LoadDbScriptStrings at :1214-1225.
    let templates = templates();
    let mut talk = row(0, 123, 0);
    talk.text = [999, -1, 0, 0];
    let map = LoadedDbScriptMap::load(
        DbScriptType::Relay,
        vec![talk.clone()],
        &templates,
        None,
        &mut Missing,
    )
    .unwrap();
    assert!(templates.string(123).is_none());
    assert_eq!(map.scripts()[&7], [talk]);
    assert!(map.rows()[0].logs.is_empty());
    assert_eq!(map.loaded_count(), 1);
    let mut zero = row(0, 123, 0);
    zero.text = [0; 4];
    assert_eq!(load(vec![zero], &mut Missing).unwrap().loaded_count(), 1);
}
#[test]
fn random_template_type_guard_covers_both_signed_index_boundaries() {
    // ScriptMgr.cpp:1239-1244: negative int32 passes <MAX_TYPE then indexes before the array.
    for kind in [0x8000_0000, u32::MAX] {
        assert_eq!(
            SourceDbScriptTemplates::load([DbScriptRandomRow {
                ordinal: 0,
                id: 1,
                kind,
                target: 0,
                chance: 0
            }])
            .unwrap_err()
            .id,
            "execution.db_scripts.random_template_index_undefined"
        );
    }
    let rows = [(0, 1), (1, 2), (1, 3), (2, 4), (0x7fff_ffff, 5)].map(|(kind, target)| {
        DbScriptRandomRow {
            ordinal: target as u64,
            id: 1,
            kind,
            target,
            chance: target as u32,
        }
    });
    let t = SourceDbScriptTemplates::load(rows).unwrap();
    assert_eq!(t.string(1), Some([(1, 1)].as_slice()));
    assert_eq!(t.relay(1), Some([(2, 2), (3, 3)].as_slice()));
    assert_eq!(
        t.dropped()
            .iter()
            .map(|r| r.row.ordinal)
            .collect::<Vec<_>>(),
        [4, 5]
    );
    assert!(t
        .dropped()
        .iter()
        .all(|r| r.source_line == 1251 && r.reason == "random_template_type_invalid"));
}

#[test]
fn retained_catalog_db_scripts_counts_or_explicit_absence() {
    use super::super::super::production_compiler::VerifiedCatalog;
    let Some(root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        println!("SKIP DB-script catalog-04 census: set BENILLA_F5_AI_CATALOG and BENILLA_F5_DB_SCRIPT_OCCURRENCES");
        return;
    };
    let occurrences = std::env::var_os("BENILLA_F5_DB_SCRIPT_OCCURRENCES")
        .expect("explicit catalog requires retained script occurrences");
    let mut catalog = VerifiedCatalog::open(Path::new(&root), false).unwrap();
    let templates = catalog_db_script_templates(&mut catalog, Path::new(&occurrences)).unwrap();
    let mut inputs = BTreeMap::new();
    for kind in DbScriptType::ALL {
        inputs.insert(
            kind,
            catalog_db_script_rows(&mut catalog, Path::new(&occurrences), kind).unwrap(),
        );
    }
    let pool_index_sites = inputs
        .values()
        .flatten()
        .filter(|r| r.flags & 0x90 == 0x80)
        .count();
    assert_eq!(
        pool_index_sites, 0,
        "stock buddy-pool lookups need an actual loaded pool bound"
    );
    struct SpellMissing;
    impl SpellLoaderOwner for SpellMissing {}
    let spells = SourceSpellStores::load(&mut catalog, &mut SpellMissing).unwrap();
    let mut external = catalog_group_census_owner(&mut catalog);
    let pool_occurrences = std::env::var_os("BENILLA_F2_POOL_OCCURRENCES")
        .expect("pool-max component census requires pool occurrences");
    let maximum =
        SourcePoolMaximum::load_catalog(&mut catalog, Path::new(&pool_occurrences)).unwrap();
    assert_eq!(maximum.maximum().unwrap(), 50003);
    let mut owner = DbScriptCatalog::new(&mut catalog, Some(&spells), &mut external, None)
        .with_pool_maximum(&maximum);
    let mut relay = None;
    let mut census = BTreeMap::new();
    for kind in [
        DbScriptType::Relay,
        DbScriptType::CreatureMovement,
        DbScriptType::QuestEnd,
        DbScriptType::QuestStart,
        DbScriptType::Spell,
        DbScriptType::GameObject,
        DbScriptType::GameObjectTemplate,
        DbScriptType::Event,
        DbScriptType::Gossip,
        DbScriptType::CreatureDeath,
    ] {
        let raw = inputs.remove(&kind).unwrap();
        let count = raw.len();
        let report = LoadedDbScriptMap::inspect(kind, raw, &templates, relay.as_ref(), &mut owner);
        let mut drops = BTreeMap::<&str, usize>::new();
        let mut refused = BTreeMap::<&str, usize>::new();
        let mut logs = BTreeMap::<&str, usize>::new();
        let mut refused_rows = Vec::new();
        let mut unmeasured_rows = Vec::new();
        let mut loaded = 0;
        let mut measured_groups = Vec::new();
        for r in report.rows() {
            match &r.disposition {
                DbScriptDisposition::Loaded => loaded += 1,
                DbScriptDisposition::Dropped => {
                    *drops.entry(r.logs.last().unwrap().reason).or_default() += 1;
                }
                DbScriptDisposition::Refused(e)
                    if e.id == "execution.db_scripts.spawn_group_owner" =>
                {
                    unmeasured_rows.push(serde_json::json!({"id":r.id,"ordinal":r.ordinal,
                        "reason":"spawn-group container not bound in census","status":"unmeasured",
                        "missing_owner":"catalog-bound GroupLoadOwner, spawn_groups.rs:171-177"}));
                }
                DbScriptDisposition::Refused(e) => {
                    if kind == DbScriptType::CreatureMovement && (470001..=470004).contains(&r.id) {
                        measured_groups.push((r.id, e.id.clone()));
                    }
                    *refused.entry(e.id.as_str()).or_default() += 1;
                    refused_rows
                        .push(serde_json::json!({"id":r.id,"ordinal":r.ordinal,"owner":e.id}));
                }
            }
            for l in &r.logs {
                *logs.entry(l.reason).or_default() += 1;
            }
        }
        assert_eq!(
            loaded
                + drops.values().sum::<usize>()
                + refused.values().sum::<usize>()
                + unmeasured_rows.len(),
            count
        );
        if kind == DbScriptType::CreatureMovement {
            let group_owner = if std::env::var_os("BENILLA_F2_SPAWN_OCCURRENCES").is_some() {
                "execution.effective_spawns.world_thread_startup_rng_position_owner"
            } else {
                "execution.spawn_groups.objectmgr_load_gameobjects_effective_spawn_owner"
            };
            assert_eq!(
                measured_groups,
                (470001..=470004)
                    .map(|id| (id, group_owner.into()))
                    .collect::<Vec<_>>()
            );
            println!("DB-script measured spawn-group rows {measured_groups:?}");
        }
        census.insert(
            kind.table(),
            serde_json::json!({"raw":count,"validated":loaded,
            "complete_loaded_map":refused.is_empty() && unmeasured_rows.is_empty(),
            "drops":drops,"refused":refused,"refused_rows":refused_rows,
            "unmeasured_rows":unmeasured_rows,"logs":logs}),
        );
        if kind == DbScriptType::Relay {
            relay = report.into_loaded().ok();
        }
    }
    let expected = [
        ("dbscripts_on_creature_death", 54),
        ("dbscripts_on_creature_movement", 2411),
        ("dbscripts_on_event", 445),
        ("dbscripts_on_go_template_use", 180),
        ("dbscripts_on_go_use", 37),
        ("dbscripts_on_gossip", 424),
        ("dbscripts_on_quest_end", 1943),
        ("dbscripts_on_quest_start", 734),
        ("dbscripts_on_relay", 907),
        ("dbscripts_on_spell", 31),
    ];
    for (kind, count) in expected {
        assert_eq!(census[kind]["raw"], count);
    }
    assert_eq!(census["dbscripts_on_spell"]["validated"], 31);
    assert_eq!(census["dbscripts_on_spell"]["complete_loaded_map"], true);
    println!("Spell map publishes given the pool-max owner; this component census does not load pool templates/memberships or pass earlier integrated startup spawn owners.");
    println!(
        "DB-script catalog-04 census {}",
        serde_json::to_string(&census).unwrap()
    );
    println!("DB-script random template index UB stock reach: 0; any negative type makes the checked loader refuse before this census");
    println!(
        "DB-script buddy pool index UB stock reach: 0; stock lookup-site rows: {pool_index_sites}"
    );
    if let Some(path) = std::env::var_os("BENILLA_F5_DB_SCRIPT_CENSUS_OUTPUT") {
        let path = Path::new(&path);
        let private_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
            .join("benilla-config");
        assert!(path.starts_with(private_root));
        std::fs::write(path, serde_json::to_vec_pretty(&census).unwrap()).unwrap();
    }
}

#[test]
fn empty_query_retains_unavailable_previous_map_but_all_dropped_publishes_empty() {
    // ScriptMgr.cpp:122-129 versus :984-995.
    assert_eq!(
        load(vec![], &mut Known::default()).unwrap_err().id,
        "execution.db_scripts.empty_query_previous_map_owner"
    );
    let map = load(vec![row(19, 0, 0)], &mut Known::default()).unwrap();
    assert_eq!(map.loaded_count(), 0);
    assert_eq!(map.rows().len(), 1);
}

#[test]
fn validated_commands_accept_source_boundary_operands() {
    let accepted = [
        (0, 0, 0),
        (1, 1, 0),
        (6, 1, 0),
        (7, 1, 5),
        (8, 0, 0),
        (9, 5, 0),
        (10, 1, 0),
        (11, 1, 0),
        (12, 2, 0),
        (14, 1, 2),
        (15, 1, 0),
        (16, 1, 31),
        (17, 1, 1),
        (20, 18, 0),
        (22, 0, 0),
        (23, 0, 0),
        (24, 0, 0),
        (27, 10, 0),
        (28, 9, 0),
        (29, 1, 2),
        (30, 1, 0),
        (31, 0, 0),
        (34, 1, 1),
        (37, 2, 2),
        (38, 1, 0),
        (42, 0, 0),
        (44, 1, 0),
        (45, 0, 8),
        (46, 1, 0),
        (47, 3, 0),
        (48, 1, 2),
        (51, 102, 0),
        (53, 0, 0),
        (54, 2, 0),
        (55, 1, 1),
        (58, 1, 0),
        (200, 1, 0),
    ];
    for (command, a, b) in accepted {
        assert_eq!(
            load(vec![row(command, a, b)], &mut Known::default())
                .unwrap()
                .loaded_count(),
            1,
            "command {command}"
        );
    }
    for command in [23, 24] {
        let mut r = row(command, 1, 0);
        r.flags = 8;
        let mut owner = Known::default();
        load(vec![r], &mut owner).unwrap();
        assert_eq!(owner.calls, [DbScriptLookup::Display]);
    }
    let mut r = row(31, 0, 0);
    r.data[2] = 10;
    assert_eq!(
        load(vec![r], &mut Known::default()).unwrap().loaded_count(),
        1
    );
}

#[test]
fn waypoint_projection_uses_complete_maps_and_executes_check_dbscript() {
    let maps = SourceDbScriptMaps::load(
        vec![
            (DbScriptType::Relay, vec![row(18, 0, 0)]),
            (DbScriptType::CreatureMovement, vec![row(18, 0, 0)]),
        ],
        templates(),
        &mut Known::default(),
    )
    .unwrap();
    let projection = maps.waypoint_maps().unwrap();
    assert_eq!(projection.movement[&7][0].command, 18);
    assert_eq!(projection.movement[&7][0].despawn_delay, 0);
    assert_eq!(projection.relay, BTreeSet::from([7]));
    assert_eq!(projection.relay_templates.borrow()[&8], [7, 9]);
    let paths = SourceWaypointPaths::load(
        [
            vec![SourceWaypointRow {
                ordinal: 0,
                owner: 11,
                path: 0,
                point: 1,
                node: SourceWaypointNode {
                    position: [0.; 3],
                    orientation: None,
                    delay: 0,
                    script: 7,
                },
            }],
            vec![],
            vec![],
        ],
        &BTreeSet::from([11]),
        &BTreeSet::new(),
        Some(&projection),
    )
    .unwrap();
    let (_, path) = paths.select(1, 11, 0, WaypointOrigin::None).unwrap();
    assert_eq!(path.unwrap()[&1].delay, 1000);
}

#[test]
fn spawn_group_checks_use_f4_loaded_store_not_raw_membership() {
    use super::super::spawn_group_tests::{template, Owner, Rows};
    use benilla_sim::source_spawn_groups::{GroupInputData, GroupPhase};
    let mut groups = SourceSpawnGroups::default();
    assert_eq!(
        DbScriptLoaderOwner::contains(&mut groups, DbScriptLookup::SpawnGroup, 5)
            .unwrap_err()
            .id,
        "execution.group.loader_owner"
    );
    let mut rows = Rows::default();
    rows.push(
        GroupPhase::Templates,
        GroupInputData::Template(template(5, 0, 0)),
    );
    groups.load(&mut rows, &mut Owner::default()).unwrap();
    assert!(DbScriptLoaderOwner::contains(&mut groups, DbScriptLookup::SpawnGroup, 5).unwrap());
    assert!(!DbScriptLoaderOwner::contains(&mut groups, DbScriptLookup::SpawnGroup, 6).unwrap());
    let mut r = row(18, 0, 0);
    r.flags = 0x100;
    r.buddy = 5;
    assert_eq!(
        load(vec![r.clone()], &mut groups).unwrap().loaded_count(),
        1
    );
    r.buddy = 6;
    let map = load(vec![r], &mut groups).unwrap();
    assert_eq!(map.loaded_count(), 0);
    assert_eq!(map.rows()[0].logs[0].reason, "buddy_group_missing");
    assert_eq!(
        load(vec![row(7, 1, 0)], &mut groups).unwrap_err().id,
        "execution.db_scripts.effective_quest_flags_owner"
    );
}

#[test]
fn buddy_pool_lookup_wraps_uint16_before_guarding_the_actual_vector_bound() {
    #[derive(Default)]
    struct Pools {
        seen: Vec<(DbScriptLookup, u32)>,
    }
    impl DbScriptLoaderOwner for Pools {
        fn max_pool_id(&mut self) -> Result<u32> {
            Ok(10)
        }
        fn contains(&mut self, kind: DbScriptLookup, id: u32) -> Result<bool> {
            self.seen.push((kind, id));
            Ok(id == 10)
        }
    }
    for (command, kind) in [
        (18, DbScriptLookup::CreaturePool),
        (40, DbScriptLookup::GameObjectPool),
    ] {
        let mut owner = Pools::default();
        let mut r = row(command, 0, 0);
        r.flags = 0x80;
        r.radius_or_guid = 0x1_000a;
        assert_eq!(load(vec![r.clone()], &mut owner).unwrap().loaded_count(), 1);
        assert_eq!(owner.seen, [(kind, 10)]);
        r.radius_or_guid = 0x1_000b;
        assert_eq!(
            load(vec![r], &mut owner).unwrap_err().id,
            "execution.db_scripts.buddy_pool_index_undefined"
        );
        assert_eq!(
            owner.seen,
            [(kind, 10)],
            "undefined index must refuse before lookup"
        );
    }
}

#[test]
fn terminate_script_uses_loaded_uint16_pool_maximum_without_narrowing_operand() {
    struct MaximumRows(Option<u16>);
    impl PoolRows for MaximumRows {
        fn maximum(&mut self) -> Result<Option<u16>> {
            Ok(self.0)
        }
        fn template(&mut self) -> Result<Option<PoolTemplateRow>> {
            panic!("MAX phase must not load templates")
        }
        fn member(&mut self, _: usize) -> Result<Option<PoolMembershipRow>> {
            panic!("MAX phase must not load memberships")
        }
    }
    let mut r = row(31, 0, 0);
    r.data[2] = 10;
    assert_eq!(
        load(vec![r.clone()], &mut SourcePoolMaximum::default())
            .unwrap_err()
            .id,
        "execution.db_scripts.pool_max_id_owner"
    );
    for (maximum, pool, accepted) in [
        (Some(10), 0, true),
        (Some(10), 9, true),
        (Some(10), 10, true),
        (Some(10), 11, false),
        (Some(10), 65546, false),
        (Some(u16::MAX), 65535, true),
        (Some(u16::MAX), 65536, false),
        (None, 0, true),
        (None, 1, false),
        (Some(0), 0, true),
        (Some(0), 1, false),
    ] {
        // ScriptMgr.cpp:698 compares uint32 poolId to promoted uint16 MAX.
        let mut owner = SourcePoolMaximum::load(&mut MaximumRows(maximum)).unwrap();
        assert_eq!(owner.maximum().unwrap(), maximum.unwrap_or(0));
        r.data[2] = pool;
        let map = load(vec![r.clone()], &mut owner).unwrap();
        assert_eq!(map.loaded_count(), usize::from(accepted));
        if !accepted {
            assert_eq!(map.rows()[0].logs[0].reason, "terminate_pool_invalid");
            assert_eq!(map.rows()[0].logs[0].source_line, 700);
        }
    }
    // Zero short-circuits before consulting an unloaded owner.
    r.data[2] = 0;
    assert_eq!(
        load(vec![r], &mut SourcePoolMaximum::default())
            .unwrap()
            .loaded_count(),
        1
    );
}
