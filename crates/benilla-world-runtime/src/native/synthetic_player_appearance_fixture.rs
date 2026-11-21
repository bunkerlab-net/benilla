// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored complete cosmetic rows, not an install or native admission token.
use super::*;
use benilla_formats::{Chain, CharCreateCatalog};
use std::sync::atomic::{AtomicU64, Ordering};
fn dbc<const N: usize>(rows: &[[u32; N]]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for word in [rows.len() as u32, N as u32, (4 * N) as u32, 1] {
        bytes.extend(word.to_le_bytes());
    }
    for row in rows {
        for word in row {
            bytes.extend(word.to_le_bytes());
        }
    }
    bytes.push(0);
    bytes
}
pub fn catalog() -> CharCreateCatalog {
    let mut entries = synthetic_player_model_fixture::synthetic_char_catalog_entries();
    let mut sections = Vec::new();
    let mut facial = Vec::new();
    for race in 1..=8 {
        for gender in 0..=1 {
            for color in 0..=1 {
                let id = sections.len() as u32 + 1;
                sections.push([id, race, gender, 0, 0, color, 0, 0, 0, 0]);
                for variation in 0..=1 {
                    for section in [1, 2, 3] {
                        let id = sections.len() as u32 + 1;
                        sections.push([id, race, gender, section, variation, color, 0, 0, 0, 0]);
                    }
                }
            }
            for variation in 0..=1 {
                facial.push([race, gender, variation, 0, 0, 0, 0, 0, 0]);
            }
        }
    }
    for (name, bytes) in &mut entries {
        if *name == "DBFilesClient\\CharSections.dbc" {
            *bytes = dbc(&sections);
        }
        if *name == "DBFilesClient\\CharacterFacialHairStyles.dbc" {
            *bytes = dbc(&facial);
        }
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "benilla-authored-appearance-{}-{}.MPQ",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, synthetic_creature_model_fixture::archive(&entries)).unwrap();
    let mut chain = Chain::open(&path).unwrap();
    let catalog = CharCreateCatalog::load(&mut chain).unwrap();
    std::fs::remove_file(path).unwrap();
    catalog
}
pub fn request(player: crate::PlayerId) -> PlayerAppearanceRequest {
    PlayerAppearanceRequest {
        player,
        race: 1,
        class: 8,
        gender: 0,
        choices: NativePlayerAppearanceChoices {
            skin: 1,
            face: 0,
            hair_style: 1,
            hair_color: 0,
            facial_hair: 1,
        },
    }
}
pub fn profile(request: PlayerAppearanceRequest) -> Profile {
    let mut p = synthetic_fixture::initialized_profile();
    p.initialize_player_appearance(request, &catalog()).unwrap();
    p
}
