// SPDX-License-Identifier: GPL-3.0-or-later
//! Invented loaded player display rows only; no native producer/admission claim.
use super::{
    synthetic_creature_model_fixture as models, synthetic_fixture as f,
    synthetic_motion_fixture as motion, *,
};
use benilla_formats::{Chain, CharCreateCatalog};
use std::sync::atomic::{AtomicU64, Ordering};

pub fn catalog() -> CharCreateCatalog {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "benilla-authored-player-model-{}-{}.MPQ",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, models::archive(&synthetic_char_catalog_entries())).unwrap();
    let mut chain = Chain::open(&path).unwrap();
    let catalog = CharCreateCatalog::load(&mut chain).unwrap();
    std::fs::remove_file(&path).unwrap();
    catalog
}
pub fn unowned_profile() -> Profile {
    let mut p = f::initialized_profile();
    p.npcs[0].ai.spawn = f::point(8.0);
    p.npcs[0].ai.reaction = ai::Reaction::Aggressive;
    p.source_motion = Some(motion::genesis(&p, 2));
    p.initialize_creature_capabilities(motion::creature_capability_sources(&p))
        .unwrap();
    p
}
pub fn profile() -> Profile {
    profile_with_fields(1.75, 2.5, 2.875)
}
pub fn profile_with_fields(display_scale: f32, model_scale: f32, height: f32) -> Profile {
    let mut p = unowned_profile();
    p.initialize_player_collision_height(
        &catalog(),
        &models::catalog_with_height_fields(1.375, height, model_scale, display_scale, false),
    )
    .unwrap();
    p
}

// Authored fixture helper for runtime tests; all five returned members can share an MPQ
// with CreatureDisplayInfo / CreatureModelData. Increase archive hash slots to >=16.
// Authored player displays 31/34 share the separately loaded creature fixture catalog.
const CHR_RACES: &str = "DBFilesClient\\ChrRaces.dbc";
const CHAR_BASE_INFO: &str = "DBFilesClient\\CharBaseInfo.dbc";
const CHAR_START_OUTFIT: &str = "DBFilesClient\\CharStartOutfit.dbc";
const CHAR_SECTIONS: &str = "DBFilesClient\\CharSections.dbc";
const CHAR_FACIAL_HAIR_STYLES: &str = "DBFilesClient\\CharacterFacialHairStyles.dbc";
const KNOWN_FILES: [(u8, &str); 8] = [
    (1, "Human"),
    (2, "Orc"),
    (3, "Dwarf"),
    (4, "NightElf"),
    (5, "Scourge"),
    (6, "Tauren"),
    (7, "Gnome"),
    (8, "Troll"),
];
const KNOWN_COMBOS: [(u8, &[u8]); 8] = [
    (1, &[1, 2, 4, 5, 8, 9]),
    (2, &[1, 3, 4, 7, 9]),
    (3, &[1, 2, 3, 4, 5, 8]),
    (4, &[1, 3, 4, 5, 11]),
    (5, &[1, 4, 5, 8, 9]),
    (6, &[1, 3, 7, 11]),
    (7, &[1, 4, 8, 9]),
    (8, &[1, 3, 4, 5, 7, 8]),
];
fn player_dbc<const N: usize>(rows: &[[u32; N]], strings: &[u8], stride: usize) -> Vec<u8> {
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
                row[4] = 31;
                row[5] = 34;
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
            (CHAR_SECTIONS, player_dbc(&sections, b"\0", 40)),
            (CHAR_FACIAL_HAIR_STYLES, player_dbc(&facial, b"\0", 36)),
        ]
    }
}
pub(super) fn synthetic_char_catalog_entries() -> Vec<(&'static str, Vec<u8>)> {
    let fixture = Fixture::new();
    let races = player_dbc(&fixture.races, &fixture.race_strings, 116);
    fixture.members(races)
}
