// SPDX-License-Identifier: GPL-3.0-or-later
use super::super::super::production_compiler::{ObjectMgrSpawnRow, VerifiedCatalog};
use super::*;
fn row(map: u32) -> InstanceTemplateRow {
    InstanceTemplateRow {
        map,
        parent: 0,
        level_min: 1,
        level_max: 60,
        max_players: 5,
        reset_days: 2,
        ghost_map: -1,
        ghost_xy: [0.0; 2],
        script_id: 7,
        mount_allowed: false,
    }
}
fn reset() -> InstanceResetConfig {
    InstanceResetConfig::parse(b"Rate.InstanceResetTime = 0.5").unwrap()
}
fn maps() -> BTreeMap<u32, u32> {
    BTreeMap::from([
        (0, 0),
        (1, 0),
        (2, 1),
        (3, 2),
        (4, 3),
        (5, 0),
        (369, 0),
        (449, 0),
        (450, 0),
    ])
}
#[test]
fn instance_erase_rules_keep_battlegrounds_and_vanilla_exceptions() {
    // ObjectMgr.cpp:5382,5390,5420,5436, with :5428 unreachable after IsValidMAP.
    let mut invalid_xy = row(2);
    invalid_xy.ghost_map = 0;
    invalid_xy.ghost_xy[0] = f32::NAN;
    let mut noncontinent = row(3);
    noncontinent.ghost_map = 5;
    let loaded = LoadedInstanceTemplates::load(
        [
            row(99),
            row(5),
            invalid_xy,
            noncontinent,
            row(4),
            row(369),
            row(449),
            row(450),
        ],
        &maps(),
        &reset(),
    )
    .unwrap();
    assert_eq!(loaded.raw_rows(), 8);
    assert_eq!(
        loaded.rows().keys().copied().collect::<Vec<_>>(),
        [4, 369, 449, 450]
    );
    assert_eq!(
        loaded
            .dispositions()
            .iter()
            .map(|r| (r.map, r.line))
            .collect::<Vec<_>>(),
        [(2, 5420), (3, 5436), (5, 5390), (99, 5382)]
    );
    let mut missing_ghost = row(2);
    missing_ghost.ghost_map = 99;
    let loaded = LoadedInstanceTemplates::load([missing_ghost], &maps(), &reset()).unwrap();
    assert_eq!(loaded.dispositions()[0].line, 5420);
}
#[test]
fn parent_corrections_preserve_the_missing_parent_continue() {
    // :5402-5403 skips invalid ghosts and reset scaling; :5410 does not.
    let mut absent = row(2);
    absent.parent = 99;
    absent.ghost_map = 99;
    let mut continent = row(3);
    continent.parent = 1;
    continent.ghost_map = 99;
    let mut valid = row(4);
    valid.parent = 3;
    let loaded =
        LoadedInstanceTemplates::load([absent, continent, valid], &maps(), &reset()).unwrap();
    assert_eq!(
        (loaded.rows()[&2].parent, loaded.rows()[&2].reset_days),
        (0, 2)
    );
    assert!(!loaded.rows().contains_key(&3));
    assert_eq!(
        (loaded.rows()[&4].parent, loaded.rows()[&4].reset_days),
        (3, 1)
    );
    assert_eq!(
        loaded
            .dispositions()
            .iter()
            .map(|r| (r.map, r.line))
            .collect::<Vec<_>>(),
        [(2, 5402), (3, 5410), (3, 5420)]
    );
}
#[test]
fn indexed_order_uses_prior_erases_but_future_raw_templates() {
    // :5372 indexed ascending loop; MapManager.cpp:248 reads its current lookup.
    let mut early = row(2);
    early.ghost_map = 3;
    let mut late = row(3);
    late.ghost_map = 2;
    let loaded = LoadedInstanceTemplates::load([late, early], &maps(), &reset()).unwrap();
    assert_eq!(
        loaded
            .dispositions()
            .iter()
            .map(|r| (r.map, r.line))
            .collect::<Vec<_>>(),
        [(2, 5436), (3, 5420)]
    );
    assert!(loaded.rows().is_empty());
}
#[test]
fn reset_scaling_keeps_zero_minimum_and_refuses_undefined_conversion() {
    // :5441-5442 keeps zero, truncates float, and enforces at least one day.
    let mut zero = row(2);
    zero.reset_days = 0;
    let loaded = LoadedInstanceTemplates::load([zero, row(3)], &maps(), &reset()).unwrap();
    assert_eq!(loaded.rows()[&2].reset_days, 0);
    assert_eq!(loaded.rows()[&3].reset_days, 1);
    for rate in ["NaN", "inf", "4294967296"] {
        let config =
            InstanceResetConfig::parse(format!("Rate.InstanceResetTime={rate}").as_bytes())
                .unwrap();
        assert_eq!(
            LoadedInstanceTemplates::load([row(2)], &maps(), &config)
                .unwrap_err()
                .id,
            "execution.instance_templates.reset_delay_conversion_undefined"
        );
    }
    let mut high = row(2);
    high.reset_days = 4_294_967_040;
    let one = InstanceResetConfig::parse(b"Rate.InstanceResetTime=1").unwrap();
    assert_eq!(
        LoadedInstanceTemplates::load([high], &maps(), &one)
            .unwrap()
            .rows()[&2]
            .reset_days,
        4_294_967_040
    );
    let negative = InstanceResetConfig::parse(b"Rate.InstanceResetTime=-2").unwrap();
    assert_eq!(
        LoadedInstanceTemplates::load([row(2)], &maps(), &negative)
            .unwrap()
            .rows()[&2]
            .reset_days,
        2
    );
    assert!(InstanceResetConfig::parse(b"Rate.InstanceResetTime=invalid").is_err());
}
#[test]
fn recorded_autoload_has_no_absence_default_and_malformed_values_are_false() {
    for value in ["1", "true", "YES", "\"True\""] {
        let config =
            AutoloadActiveConfig::parse(format!("# comment\nautoload.ACTIVE = {value}").as_bytes())
                .unwrap();
        assert!(config.enabled());
        assert_eq!(config.line, 2);
    }
    for value in ["0", "false", "no", "invalid", "2", ""] {
        assert!(
            !AutoloadActiveConfig::parse(format!("Autoload.Active={value}").as_bytes())
                .unwrap()
                .enabled()
        );
    }
    assert_eq!(
        AutoloadActiveConfig::parse(b"# absent").unwrap_err().id,
        "execution.spawn_loader.config_missing.Autoload.Active"
    );
    assert_eq!(
        AutoloadActiveConfig::parse(b"Autoload.Active 1")
            .unwrap_err()
            .id,
        "execution.spawn_loader.config_syntax"
    );
    let last = AutoloadActiveConfig::parse(b"Autoload.Active=0\nAutoload.Active=1").unwrap();
    assert!(last.enabled());
    assert_eq!(last.line, 2);
}
struct Unbound;
impl EffectiveSpawnOwner for Unbound {}
fn spawn(map: u32) -> ObjectMgrSpawnRow {
    ObjectMgrSpawnRow {
        ordinal: 0,
        guid: 7,
        entry: 10,
        map,
        pose: [0.0; 4],
        respawn: [10, 20],
        radius: 0.0,
        movement: 0,
        mask: 1,
        event: 0,
        pools: [0; 2],
        spawn_template: 0,
        rotation: [0.0, 0.0, 0.0, 1.0],
    }
}
#[test]
fn effective_bindings_use_loaded_instances_and_recorded_autoload() {
    let mut erased = row(3);
    erased.ghost_map = 99;
    let loaded = LoadedInstanceTemplates::load([row(2), erased], &maps(), &reset()).unwrap();
    assert_eq!(loaded.dispositions()[0].line, 5420);
    let mut inputs = EffectiveSpawnInputs {
        maps: maps(),
        gameobjects: BTreeMap::from([(
            10,
            EffectiveSpawnTemplate {
                flags: 0x1000,
                ..Default::default()
            },
        )]),
        ..Default::default()
    };
    loaded.bind_effective_spawns(&mut inputs);
    for (text, active) in [
        (b"Autoload.Active=1".as_slice(), 1),
        (b"Autoload.Active=0".as_slice(), 0),
    ] {
        let config = AutoloadActiveConfig::parse(text).unwrap();
        let mut owner = ConfiguredEffectiveSpawnOwner {
            execution: &mut Unbound,
            autoload: &config,
        };
        let mut spawns = SourceEffectiveSpawns::default();
        spawns
            .inspect_row(GridSpawnKind::GameObject, spawn(2), &inputs, &mut owner)
            .unwrap();
        assert_eq!(spawns.complete_count(), 1);
        assert_eq!(spawns.active.values().map(Vec::len).sum::<usize>(), active);
        let mut skipped = SourceEffectiveSpawns::default();
        skipped
            .inspect_row(GridSpawnKind::GameObject, spawn(3), &inputs, &mut owner)
            .unwrap();
        assert_eq!(skipped.complete_count(), 0);
        assert!(skipped.pre_terrain_refusals().is_empty());
        assert!(skipped
            .records
            .iter()
            .any(|r| r.reason == "invalid_map_orientation" && r.line == 2221));
    }
}
#[test]
fn catalog04_startup_owners_census_or_explicit_absence() {
    let (Some(root), Some(occurrences), Some(config)) = (
        std::env::var_os("BENILLA_F2_CATALOG"),
        std::env::var_os("BENILLA_F2_SPAWN_OCCURRENCES"),
        std::env::var_os("BENILLA_F5_EVENT_AI_CONFIG"),
    ) else {
        println!("SKIP startup owners: bind catalog, spawn occurrences and recorded config");
        return;
    };
    let bytes = std::fs::read(&config).unwrap();
    let hash = Sha256Id::of(&bytes);
    let autoload = AutoloadActiveConfig::recorded(Path::new(&config), &hash).unwrap();
    let reset = InstanceResetConfig::recorded(Path::new(&config), &hash).unwrap();
    assert!(autoload.enabled());
    assert_eq!((autoload.line, reset.line), (258, 1528));
    assert_eq!(
        AutoloadActiveConfig::recorded(Path::new(&config), &Sha256Id::of(b"wrong"))
            .unwrap_err()
            .id,
        "execution.spawn_loader.config_identity"
    );
    let mut catalog = VerifiedCatalog::open(Path::new(&root), false).unwrap();
    let loaded = LoadedInstanceTemplates::catalog_diagnostic(&mut catalog, &reset).unwrap();
    assert_eq!(loaded.raw_rows(), 33);
    assert_eq!(
        loaded.rows().keys().copied().collect::<Vec<_>>(),
        [
            30, 33, 34, 36, 43, 47, 48, 70, 90, 109, 129, 169, 189, 209, 229, 230, 249, 289, 309,
            329, 349, 369, 389, 409, 429, 449, 450, 469, 489, 509, 529, 531, 533
        ]
    );
    let mut erased = BTreeMap::from([(5382, 0usize), (5390, 0), (5420, 0), (5428, 0), (5436, 0)]);
    for r in loaded.dispositions().iter().filter(|r| r.erased) {
        *erased.get_mut(&r.line).unwrap() += 1;
    }
    assert!(erased.values().all(|count| *count == 0));
    let before =
        SourceEffectiveSpawns::inspect_catalog_pre_terrain(&mut catalog, Path::new(&occurrences))
            .unwrap();
    let after = SourceEffectiveSpawns::inspect_catalog_startup_owners(
        &mut catalog,
        Path::new(&occurrences),
        Path::new(&config),
        &hash,
        &mut Unbound,
    )
    .unwrap();
    let mut changed = BTreeMap::<String, BTreeMap<String, usize>>::new();
    let mut instance_pass = 0;
    let mut instance_skip = 0;
    let mut instance_maps = BTreeSet::new();
    for (key, old) in before.pre_terrain_refusals() {
        if old.id.ends_with("loaded_instance_template_owner") {
            let map = before.pre_terrain_map()[key].row.map;
            instance_maps.insert(map);
            if loaded.rows().contains_key(&map) {
                instance_pass += 1;
            } else {
                instance_skip += 1;
                assert!(after
                    .records
                    .iter()
                    .any(|r| r.key == *key && r.line == 2221));
                assert!(!after.pre_terrain_refusals().contains_key(key));
            }
        }
        if old.id.ends_with("autoload_active_config_owner")
            || old.id.ends_with("loaded_instance_template_owner")
        {
            let new = match after.complete_data(*key) {
                Ok(Some(_)) => "complete".to_owned(),
                Ok(None) => "source_skip".to_owned(),
                Err(error) => error.id,
            };
            *changed
                .entry(old.id.clone())
                .or_default()
                .entry(new)
                .or_default() += 1;
        }
    }
    assert_eq!(
        (instance_pass, instance_skip, instance_maps.len()),
        (3579, 0, 25)
    );
    let mut left = BTreeMap::new();
    for error in after.pre_terrain_refusals().values() {
        *left.entry(&error.id).or_insert(0usize) += 1;
    }
    assert!(
        !left.keys().any(|id| id.ends_with("_undefined")),
        "stock undefined reach: {left:?}"
    );
    assert_eq!(
        changed["execution.effective_spawns.autoload_active_config_owner"],
        BTreeMap::from([("complete".to_owned(), 11650)])
    );
    assert_eq!(after.complete_count(), 15565);
    assert_eq!(
        left.iter()
            .map(|(id, count)| (id.as_str(), *count))
            .collect::<Vec<_>>(),
        [
            (
                "execution.effective_spawns.terrain_original_zone_owner",
                92977
            ),
            (
                "execution.effective_spawns.world_thread_startup_rng_position_owner",
                5324
            )
        ]
    );
    println!(
        "F4_STARTUP_OWNERS {}",
        serde_json::json!({"raw_instances":loaded.raw_rows(),
        "erased_by_source_line":erased,"loaded_maps":loaded.rows().keys().collect::<Vec<_>>(),
        "loaded_count":loaded.rows().len(),"instance_go_pass":instance_pass,"instance_go_skip":instance_skip,
        "autoload":autoload.enabled(),"autoload_line":autoload.line,"changed":changed,
        "active_registrations":after.active.values().map(Vec::len).sum::<usize>(),
        "complete_rows":after.complete_count(),"refusals_left":left,"undefined_reach":0})
    );
}
