//! Authored DBC rows loaded through the production MPQ chain and character catalog.
//! No installed game data, captured records, models or textures are used.

use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::authored_mpq_crypto as crypto;

struct Temp(PathBuf);

impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "benilla-authored-player-display-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, members: &[(&str, Vec<u8>)]) {
        std::fs::write(self.0.join(name), archive(members)).unwrap();
    }

    fn load(&self) -> Result<CharCreateCatalog> {
        CharCreateCatalog::load(&mut Chain::open(&self.0)?)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn archive(members: &[(&str, Vec<u8>)]) -> Vec<u8> {
    use crypto::{encrypt_block, hash_string, hash_type};
    const SLOTS: u32 = 8;
    assert!(members.len() < SLOTS as usize);
    let hash_pos = 32;
    let block_pos = hash_pos + SLOTS * 16;
    let mut data_pos = block_pos + members.len() as u32 * 16;
    let mut hashes = vec![u32::MAX; SLOTS as usize * 4];
    let mut blocks = Vec::new();
    for (index, (name, data)) in members.iter().enumerate() {
        let mut slot = (hash_string(name, hash_type::TABLE_OFFSET) & (SLOTS - 1)) as usize;
        while hashes[slot * 4 + 3] != u32::MAX {
            slot = (slot + 1) & (SLOTS as usize - 1);
        }
        hashes[slot * 4..slot * 4 + 4].copy_from_slice(&[
            hash_string(name, hash_type::NAME_A),
            hash_string(name, hash_type::NAME_B),
            0,
            index as u32,
        ]);
        blocks.extend([data_pos, data.len() as u32, data.len() as u32, 0x8000_0000]);
        data_pos += data.len() as u32;
    }
    encrypt_block(
        &mut hashes,
        hash_string("(hash table)", hash_type::FILE_KEY),
    );
    encrypt_block(
        &mut blocks,
        hash_string("(block table)", hash_type::FILE_KEY),
    );
    let mut bytes = vec![0; 32];
    for (offset, value) in [
        (0, 0x1a51_504d_u32),
        (4, 32),
        (8, data_pos),
        (16, hash_pos),
        (20, block_pos),
        (24, SLOTS),
        (28, members.len() as u32),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    for word in hashes.into_iter().chain(blocks) {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    for (_, data) in members {
        bytes.extend_from_slice(data);
    }
    bytes
}

fn dbc<const N: usize>(rows: &[[u32; N]], strings: &[u8], stride: usize) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for value in [
        rows.len() as u32,
        N as u32,
        stride as u32,
        strings.len() as u32,
    ] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    for row in rows {
        let start = bytes.len();
        for value in row {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        // Short rows retain partial words; long rows contain authored padding bytes.
        bytes.resize(start + stride, 0x5a);
    }
    bytes.extend_from_slice(strings);
    bytes
}

/// Minimal authored tables sufficient for the existing client catalog's independent guards.
struct Fixture {
    races: Vec<[u32; 29]>,
    race_strings: Vec<u8>,
    combos: Vec<(u8, u8)>,
}

impl Fixture {
    fn new() -> Self {
        // Even offset zero is a nonempty authored token. Short-stride tests therefore expose
        // the existing decoder's permissive cross-row/zero-fill behavior past required IDs.
        let mut race_strings = b"AuthoredDefault\0".to_vec();
        let races = KNOWN_FILES
            .iter()
            .map(|&(race, file)| {
                let mut row = [0; 29];
                row[0] = u32::from(race);
                row[4] = 1000 + u32::from(race) * 2;
                row[5] = row[4] + 1;
                row[15] = race_strings.len() as u32;
                race_strings.extend_from_slice(file.as_bytes());
                race_strings.push(0);
                row
            })
            .collect();
        let combos = KNOWN_COMBOS
            .iter()
            .flat_map(|&(race, classes)| classes.iter().map(move |&class| (race, class)))
            .collect();
        Self {
            races,
            race_strings,
            combos,
        }
    }

    fn race_bytes(&self, stride: usize) -> Vec<u8> {
        dbc(&self.races, &self.race_strings, stride)
    }

    fn base_bytes(&self) -> Vec<u8> {
        let mut bytes = b"WDBC".to_vec();
        for word in [self.combos.len() as u32, 2, 2, 0] {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        for &(race, class) in &self.combos {
            bytes.extend([race, class]);
        }
        bytes
    }

    fn members(&self, races: Vec<u8>) -> Vec<(&'static str, Vec<u8>)> {
        let mut outfit = vec![0; 152];
        outfit[4] = 1;
        outfit[5] = 1;
        for (slot, (display, inv)) in [
            (9891_u32, 4_u32),
            (9892, 7),
            (10141, 8),
            (1542, 21),
            (18730, 14),
        ]
        .into_iter()
        .enumerate()
        {
            outfit[56 + slot * 4..60 + slot * 4].copy_from_slice(&display.to_le_bytes());
            outfit[104 + slot * 4..108 + slot * 4].copy_from_slice(&inv.to_le_bytes());
        }
        let mut outfits = b"WDBC".to_vec();
        for word in [1_u32, 41, 152, 0] {
            outfits.extend_from_slice(&word.to_le_bytes());
        }
        outfits.extend(outfit);
        let mut sections = Vec::new();
        let mut facial = Vec::new();
        for race in 1..=8 {
            for gender in 0..=1 {
                for section in [0, 1, 3] {
                    sections.push([0, race, gender, section, 0, 0, 0, 0, 0, 0]);
                }
                facial.push([race, gender, 0, 0, 0, 0, 0, 0, 0]);
            }
        }
        vec![
            (CHR_RACES, races),
            (CHAR_BASE_INFO, self.base_bytes()),
            (CHAR_START_OUTFIT, outfits),
            (CHAR_SECTIONS, dbc(&sections, b"\0", 40)),
            (CHAR_FACIAL_HAIR_STYLES, dbc(&facial, b"\0", 36)),
        ]
    }

    fn load_bytes(&self, races: Vec<u8>) -> Result<CharCreateCatalog> {
        let temp = Temp::new();
        temp.write("dbc.MPQ", &self.members(races));
        temp.load()
    }

    fn load(&self) -> CharCreateCatalog {
        self.load_bytes(self.race_bytes(116)).unwrap()
    }
}

#[test]
fn native_player_display_preserves_loaded_identity_and_gender_selected_ids() {
    let mut fixture = Fixture::new();
    fixture.races[0][4] = u32::MAX;
    let catalog = fixture.load();
    let male: crate::NativePlayerDisplayRow = catalog.native_player_display(1, 8, 0).unwrap();
    assert_eq!(male.raw_race_id(), 1);
    assert_eq!(male.class(), 8);
    assert_eq!(male.gender(), 0);
    assert_eq!(male.display_id(), u32::MAX);
    let female = catalog.native_player_display(1, 8, 1).unwrap();
    assert_eq!(female.raw_race_id(), 1);
    assert_eq!(female.class(), 8);
    assert_eq!(female.gender(), 1);
    assert_eq!(female.display_id(), 1003);
    assert_eq!(
        catalog.native_player_display(2, 7, 0).unwrap().display_id(),
        1004
    );
    // The independent, optional name table does not own native model selection.
    assert!(catalog.name_gen_error().is_some());
}

#[test]
fn native_player_display_rejects_unknown_gender_and_unavailable_client_pairs() {
    let catalog = Fixture::new().load();
    for gender in [2, 3, 127, 255] {
        assert_eq!(catalog.native_player_display(1, 8, gender), None);
    }
    for (race, class) in [(0, 8), (1, 0), (1, 3), (1, 255), (3, 8), (9, 8), (255, 8)] {
        assert_eq!(catalog.native_player_display(race, class, 0), None);
    }
    // The dead Dwarf-Mage CharBaseInfo row exists but remains excluded by client eligibility.
    assert!(catalog.base_info.contains(&(3, 8)));
    assert!(!catalog.allows(3, 8));
}

#[test]
fn native_player_display_does_not_claim_server_pair_completeness() {
    let mut fixture = Fixture::new();
    fixture.combos.push((1, 3));
    let catalog = fixture.load();
    assert!(catalog.allows(1, 3));
    assert_eq!(
        catalog.native_player_display(1, 3, 0).unwrap().display_id(),
        1002
    );
}

#[test]
fn native_player_display_rejects_selected_duplicates_without_changing_client_overwrite() {
    for same_display in [true, false] {
        let mut fixture = Fixture::new();
        let mut duplicate = fixture.races[0];
        if !same_display {
            duplicate[4] = 9001;
            duplicate[5] = 9002;
        }
        fixture.races.push(duplicate);
        let catalog = fixture.load();
        assert_eq!(catalog.body_display(1, 0), Some(duplicate[4]));
        assert_eq!(catalog.native_player_display(1, 8, 0), None);
        assert_eq!(catalog.native_player_display(1, 8, 1), None);
        assert!(catalog.native_player_display(2, 7, 0).is_some());
    }
}

#[test]
fn native_player_display_rejects_full_width_aliases_in_both_orders() {
    for raw_id in [257, 65537, 0x8000_0001, (-255_i32) as u32] {
        for first in [true, false] {
            let mut fixture = Fixture::new();
            let mut alias = fixture.races[0];
            alias[0] = raw_id;
            alias[4] = 7001;
            if first {
                fixture.races.insert(0, alias);
            } else {
                fixture.races.push(alias);
            }
            let catalog = fixture.load();
            assert_eq!(
                catalog.body_display(1, 0),
                Some(if first { 1002 } else { 7001 })
            );
            assert_eq!(
                catalog.native_player_display(1, 8, 0),
                None,
                "raw {raw_id} first {first}"
            );
            assert!(catalog.native_player_display(2, 7, 0).is_some());
        }
    }
}

#[test]
fn native_player_display_never_accepts_an_alias_as_the_only_race_identity() {
    for raw_id in [257, 0xffff_ff01] {
        let mut fixture = Fixture::new();
        fixture.races[0][0] = raw_id;
        let catalog = fixture.load();
        assert_eq!(catalog.body_display(1, 0), Some(1002));
        assert_eq!(catalog.race_rows()[0].id, 1);
        assert_eq!(catalog.native_player_display(1, 8, 0), None);
    }
}

#[test]
fn native_player_display_requires_exact_stride_even_when_client_loads() {
    let fixture = Fixture::new();
    for stride in [108, 112, 117, 120] {
        let catalog = fixture.load_bytes(fixture.race_bytes(stride)).unwrap();
        assert_eq!(catalog.body_display(1, 0), Some(1002), "stride {stride}");
        assert_eq!(
            catalog.native_player_display(1, 8, 0),
            None,
            "stride {stride}"
        );
    }
    assert!(fixture.load().native_player_display(1, 8, 0).is_some());
}

#[test]
fn native_player_display_requires_present_race_and_both_nonzero_displays() {
    let mut fixture = Fixture::new();
    fixture.combos.push((9, 8));
    let catalog = fixture.load();
    assert!(catalog.allows(9, 8));
    assert_eq!(catalog.native_player_display(9, 8, 0), None);
    let mut extra = [0; 29];
    extra[0] = 9;
    extra[5] = 1009;
    fixture.races.push(extra);
    let catalog = fixture.load();
    assert_eq!(catalog.native_player_display(9, 8, 0), None);
    assert_eq!(catalog.body_display(9, 1), Some(1009));
    assert_eq!(catalog.native_player_display(9, 8, 1), None);
    let last = fixture.races.last_mut().unwrap();
    last[4] = 1008;
    last[5] = 0;
    let catalog = fixture.load();
    assert_eq!(catalog.body_display(9, 0), Some(1008));
    assert_eq!(catalog.native_player_display(9, 8, 0), None);
    fixture.races.last_mut().unwrap()[5] = 1009;
    let catalog = fixture.load();
    assert_eq!(
        catalog.native_player_display(9, 8, 0).unwrap().display_id(),
        1008
    );
    assert_eq!(
        catalog.native_player_display(9, 8, 1).unwrap().display_id(),
        1009
    );
}

#[test]
fn malformed_or_missing_race_table_cannot_yield_loaded_native_evidence() {
    let fixture = Fixture::new();
    let mut bad_magic = fixture.race_bytes(116);
    bad_magic[0] = b'X';
    let mut wrong_field_count = fixture.race_bytes(116);
    wrong_field_count[8..12].copy_from_slice(&28_u32.to_le_bytes());
    let mut truncated = fixture.race_bytes(116);
    truncated.pop();
    let mut missing_playable_race = Fixture::new();
    missing_playable_race.races.remove(0);
    for bytes in [
        vec![],
        bad_magic,
        wrong_field_count,
        truncated,
        fixture.race_bytes(0),
        missing_playable_race.race_bytes(116),
    ] {
        assert!(fixture.load_bytes(bytes).is_err());
    }
    let temp = Temp::new();
    let members: Vec<_> = fixture
        .members(fixture.race_bytes(116))
        .into_iter()
        .filter(|(name, _)| *name != CHR_RACES)
        .collect();
    temp.write("dbc.MPQ", &members);
    assert!(temp.load().is_err());
}

#[test]
fn missing_or_malformed_base_info_cannot_yield_loaded_native_evidence() {
    let fixture = Fixture::new();
    for replacement in [
        None,
        Some(vec![]),
        Some({
            let mut bytes = fixture.base_bytes();
            bytes[12..16].copy_from_slice(&4_u32.to_le_bytes());
            bytes
        }),
        Some({
            let mut bytes = fixture.base_bytes();
            bytes.pop();
            bytes
        }),
    ] {
        let temp = Temp::new();
        let mut members: Vec<_> = fixture
            .members(fixture.race_bytes(116))
            .into_iter()
            .filter(|(name, _)| *name != CHAR_BASE_INFO)
            .collect();
        if let Some(bytes) = replacement {
            members.push((CHAR_BASE_INFO, bytes));
        }
        temp.write("dbc.MPQ", &members);
        assert!(temp.load().is_err());
    }
}

#[test]
fn native_player_display_uses_winning_patch_race_table() {
    let fixture = Fixture::new();
    let temp = Temp::new();
    temp.write("dbc.MPQ", &fixture.members(fixture.race_bytes(116)));
    let mut patched = Fixture::new();
    patched.races[0][4] = 6001;
    temp.write("patch.MPQ", &[(CHR_RACES, patched.race_bytes(116))]);
    let catalog = temp.load().unwrap();
    assert_eq!(
        catalog.native_player_display(1, 8, 0).unwrap().display_id(),
        6001
    );
    patched.races[0][0] = 257;
    temp.write("patch.MPQ", &[(CHR_RACES, patched.race_bytes(116))]);
    let catalog = temp.load().unwrap();
    assert_eq!(catalog.body_display(1, 0), Some(6001));
    assert_eq!(catalog.native_player_display(1, 8, 0), None);
}

#[path = "native_player_appearance_tests.rs"]
mod appearance;
