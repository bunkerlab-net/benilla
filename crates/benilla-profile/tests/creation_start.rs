//! Compile the actual app creation selector and first-login worldport without Bevy.
#[path = "../../benilla-app/src/net/p2p/creation_start.rs"]
mod creation_start;
use benilla_profile::{
    valid_classes, Appearance, NewCharacter, Position, ProfileStore, SpawnPolicy,
};
use benilla_protocol::SessionEvent;
use creation_start::CreationStart;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "benilla-source-start-app-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn capture(&self, race: u8, class: u8) -> PathBuf {
        self.0.join(format!("{race}-{class}.json"))
    }
    fn write(&self, race: u8, class: u8) {
        // Original fake geometry. No actual world rows or native gameplay fixtures.
        let row = json!({"race": race, "class": class, "map": race % 2, "zone": 900 + u32::from(race),
            "position_x":{"decimal":"101.5"}, "position_y":{"decimal":"-22.25"},
            "position_z":{"decimal":"31.125"}, "orientation":{"decimal":"5.75"}});
        let mut rows = json!([row]);
        rows.sort_all_objects();
        let digest = format!("{:x}", Sha256::digest(serde_json::to_vec(&rows).unwrap()));
        let capture = json!({"format":"benilla-classic-source-slice-v2",
            "core_revision":"8ec338a1704e7dcb1c0213eb7ed58f9231ade40f",
            "database_revision":"ec4f596146be6467ea93c57397858e329e2db852", "client_build":5875,
            "revision_claims_verified_against_installation":false,
            "selection":{"format_version":2,"starter":{"race":race,"class":class,"level_cap":1}},
            "consistency":"transactional_snapshot", "tables":{"playercreateinfo":rows},
            "table_sha256":{"playercreateinfo":digest},"report":{"runnable":false,"capture_complete":false}});
        fs::write(
            self.capture(race, class),
            serde_json::to_vec(&capture).unwrap(),
        )
        .unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn input(race: u8, class: u8) -> NewCharacter {
    NewCharacter {
        name: "Fixture".into(),
        race,
        class,
        gender: 0,
        appearance: Appearance::default(),
    }
}
fn assert_worldport(character: &benilla_profile::Character, expected: Position) {
    let SessionEvent::Worldport {
        map_id,
        position,
        orientation,
        needs_ack,
    } = creation_start::worldport(character)
    else {
        panic!("worldport required")
    };
    assert_eq!(
        (map_id, position, orientation, needs_ack),
        (
            expected.map_id,
            expected.position,
            expected.orientation,
            false
        )
    );
    assert_eq!(character.position.zone_id, expected.zone_id);
}
#[test]
fn configured_app_creation_selects_all_forty_starts_and_first_login_pose() {
    let mut count = 0;
    for race in 1..=8 {
        for &class in valid_classes(race) {
            count += 1;
            let f = Fixture::new();
            f.write(race, class);
            let mut profile = ProfileStore::create(f.0.join("profile")).unwrap();
            let policy = CreationStart::load(Some(&f.0), race, class).unwrap();
            let id = policy.create(&mut profile, input(race, class)).unwrap();
            let expected = Position {
                map_id: u32::from(race % 2),
                zone_id: 900 + u32::from(race),
                position: [101.5, -22.25, 31.125],
                orientation: 5.75,
            };
            assert_worldport(profile.character(id).unwrap(), expected);
            assert!(matches!(
                profile.character(id).unwrap().spawn_policy,
                SpawnPolicy::ClassicSourceV1 { .. }
            ));
            let moved = Position {
                map_id: 1 - expected.map_id,
                zone_id: 950,
                position: [77.0, 88.0, 99.0],
                orientation: 2.25,
            };
            profile.save_position(id, moved).unwrap();
            profile.archive_character(profile.owner_id(), id).unwrap();
            drop(profile);
            fs::remove_file(f.capture(race, class)).unwrap();
            assert!(CreationStart::load(Some(&f.0), race, class).is_err());
            let mut profile = ProfileStore::open(f.0.join("profile")).unwrap();
            profile.restore_character(profile.owner_id(), id).unwrap();
            profile.select_character(id).unwrap();
            assert_worldport(profile.selected_character().unwrap(), moved);
        }
    }
    assert_eq!(count, 40);
}
#[test]
fn default_is_racial_but_configured_errors_do_not_fall_back_or_write() {
    let f = Fixture::new();
    let mut profile = ProfileStore::create(f.0.join("profile")).unwrap();
    let id = CreationStart::load(None, 1, 1)
        .unwrap()
        .create(&mut profile, input(1, 1))
        .unwrap();
    assert_eq!(
        profile.character(id).unwrap().spawn_policy,
        SpawnPolicy::ClassicRacialV1
    );
    let initial = profile.character(id).unwrap().position;
    let before = fs::read(f.0.join("profile/profile.json")).unwrap();
    assert!(CreationStart::load(Some(&f.0), 6, 11).is_err());
    fs::write(f.capture(6, 11), b"{broken").unwrap();
    assert!(CreationStart::load(Some(&f.0), 6, 11).is_err());
    f.write(1, 1);
    fs::copy(f.capture(1, 1), f.capture(6, 11)).unwrap();
    assert!(CreationStart::load(Some(&f.0), 6, 11).is_err());
    assert!(CreationStart::load(None, 6, 4).is_err());
    assert!(CreationStart::load(Some(std::path::Path::new("relative")), 1, 1).is_err());
    assert_eq!(fs::read(f.0.join("profile/profile.json")).unwrap(), before);
    profile.select_character(id).unwrap();
    assert_worldport(profile.character(id).unwrap(), initial);
    // A repaired selected file is reread on the next request, without reopening the lobby.
    f.write(6, 11);
    let mut next = input(6, 11);
    next.name = "Repaired".into();
    let restored = CreationStart::load(Some(&f.0), 6, 11)
        .unwrap()
        .create(&mut profile, next)
        .unwrap();
    assert_eq!(profile.character(restored).unwrap().position.map_id, 0);
    assert_eq!(
        profile.character(id).unwrap().spawn_policy,
        SpawnPolicy::ClassicRacialV1
    );
}
#[cfg(unix)]
#[test]
fn capture_symlinks_and_special_files_are_rejected_without_blocking() {
    let f = Fixture::new();
    f.write(1, 1);
    std::os::unix::fs::symlink(f.capture(1, 1), f.capture(6, 11)).unwrap();
    assert!(CreationStart::load(Some(&f.0), 6, 11).is_err());
    fs::create_dir(f.capture(2, 1)).unwrap();
    assert!(CreationStart::load(Some(&f.0), 2, 1).is_err());
}
#[test]
fn production_dispatch_keeps_capture_lookup_only_in_create_and_uses_saved_entry_pose() {
    let worker = include_str!("../../benilla-app/src/net/p2p/profile.rs");
    let create = worker
        .split_once("super::super::CharRequest::Create(req) => {")
        .unwrap()
        .1;
    let (create, enter) = create
        .split_once("super::super::CharRequest::Enter(wanted) => {")
        .unwrap();
    assert!(create.contains("super::creation_start::CreationStart::load("));
    assert!(create.contains("settings.0.classic_start_captures.as_deref()"));
    assert!(create.contains(".create(&mut store, input)"));
    assert!(!enter.contains("CreationStart::load"));
    assert!(worker.contains("zone: c.position.zone_id,"));
    assert!(worker.contains("map: c.position.map_id,"));
    assert!(worker.contains("x: c.position.position[0],"));
    assert!(include_str!("../../benilla-app/src/net/p2p.rs")
        .contains("creation_start::worldport(character)"));
}

#[test]
fn default_app_creation_covers_all_forty_pairs_and_never_resets_saved_positions() {
    // Independent expected map/zone/pose facts checked against the pinned source.
    let expected = [
        (0, 12, [-8949.95, -132.493, 83.5312], 0.0),
        (1, 14, [-618.518, -4251.67, 38.718], 0.0),
        (0, 1, [-6240.32, 331.033, 382.758], 6.17716),
        (1, 141, [10311.3, 831.463, 1326.41], 5.48033),
        (0, 85, [1676.35, 1677.45, 121.67], 2.70526),
        (1, 215, [-2917.58, -257.98, 52.9968], 0.0),
        (0, 1, [-6240.32, 331.033, 382.758], 0.0),
        (1, 14, [-618.518, -4251.67, 38.718], 0.0),
    ];
    let mut count = 0;
    for race in 1..=8 {
        for &class in valid_classes(race) {
            count += 1;
            let f = Fixture::new();
            let path = f.0.join("profile");
            let mut profile = ProfileStore::create(&path).unwrap();
            let id = CreationStart::load(None, race, class)
                .unwrap()
                .create(&mut profile, input(race, class))
                .unwrap();
            let (map_id, zone_id, position, orientation) = expected[usize::from(race - 1)];
            assert_worldport(
                profile.character(id).unwrap(),
                Position {
                    map_id,
                    zone_id,
                    position,
                    orientation,
                },
            );
            assert_eq!(
                profile.character(id).unwrap().spawn_policy,
                SpawnPolicy::ClassicRacialV1
            );
            drop(profile);
            let mut profile = ProfileStore::open(&path).unwrap();
            assert_worldport(
                profile.character(id).unwrap(),
                Position {
                    map_id,
                    zone_id,
                    position,
                    orientation,
                },
            );
            let moved = Position {
                map_id: 1 - map_id,
                zone_id: 950,
                position: [77.0, 88.0, 99.0],
                orientation: 2.25,
            };
            profile.save_position(id, moved).unwrap();
            profile.archive_character(profile.owner_id(), id).unwrap();
            drop(profile);
            let mut profile = ProfileStore::open(&path).unwrap();
            profile.restore_character(profile.owner_id(), id).unwrap();
            profile.select_character(id).unwrap();
            assert_worldport(profile.selected_character().unwrap(), moved);
        }
    }
    assert_eq!(count, 40);
    for (race, class) in [(0, 1), (9, 1), (6, 4), (1, 0)] {
        assert!(CreationStart::load(None, race, class).is_err());
    }
}

#[test]
fn legacy_characters_keep_their_pose_when_new_characters_use_racial_defaults() {
    let f = Fixture::new();
    let path = f.0.join("profile");
    let mut profile = ProfileStore::create(&path).unwrap();
    let old = profile.create_character(input(6, 11)).unwrap();
    let old_pose = profile.character(old).unwrap().position;
    drop(profile);
    let mut profile = ProfileStore::open(&path).unwrap();
    let mut new = input(6, 11);
    new.name = "Newtauren".into();
    let new = CreationStart::load(None, 6, 11)
        .unwrap()
        .create(&mut profile, new)
        .unwrap();
    assert_eq!(profile.character(new).unwrap().position.map_id, 1);
    profile.select_character(old).unwrap();
    assert_worldport(profile.selected_character().unwrap(), old_pose);
    assert_eq!(
        profile.character(old).unwrap().spawn_policy,
        SpawnPolicy::NorthshireMeetingV1
    );
}

#[test]
fn selected_default_cannot_be_reused_for_a_different_race_or_class() {
    let f = Fixture::new();
    let mut profile = ProfileStore::create(f.0.join("profile")).unwrap();
    for (race, class) in [(6, 11), (1, 8)] {
        assert!(CreationStart::load(None, 1, 1)
            .unwrap()
            .create(&mut profile, input(race, class))
            .is_err());
    }
    assert!(profile.characters().is_empty());
}
