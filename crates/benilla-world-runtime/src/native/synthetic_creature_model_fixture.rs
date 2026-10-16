// SPDX-License-Identifier: GPL-3.0-or-later
//! Invented initialization rows and WDBC/MPQ bytes only. This helper exercises the
//! actual catalog loader and opaque receipt constructor, never native admission.
use super::*;
use benilla_formats::{load_creature_catalog, Chain, CreatureCatalog};
use benilla_sim::classic_spell::SpellFloat;
use benilla_sim::creature_init::*;
use std::sync::atomic::{AtomicU64, Ordering};
fn f(value: f32) -> SpellFloat {
    SpellFloat::from_bits(value.to_bits()).unwrap()
}
pub fn input() -> benilla_sim::creature_init::Input {
    benilla_sim::creature_init::Input {
        template: Template {
            entry: 60001,
            minimum_level: 1,
            maximum_level: 1,
            unit_class: 1,
            rank: 0,
            displays: [
                DisplayChoice {
                    id: 31,
                    probability: 2,
                },
                DisplayChoice {
                    id: 0,
                    probability: 0,
                },
                DisplayChoice {
                    id: 0,
                    probability: 0,
                },
                DisplayChoice {
                    id: 0,
                    probability: 0,
                },
            ],
            scale: f(1.25),
            faction: 17,
            creature_type: 1,
            type_flags: 0,
            static_flags: [0; 4],
            regenerate_stats: 10,
            health_multiplier: f(1.),
            power_multiplier: f(1.),
            armor_multiplier: f(1.),
            damage_multiplier: f(1.),
            damage_variance: f(0.35),
            attribute_multipliers: [f(1.); 5],
            legacy: LegacyStats {
                minimum_health: 61,
                maximum_health: 603,
                minimum_mana: 97,
                maximum_mana: 331,
                armor: 17,
                minimum_melee_damage: f(1.75),
                maximum_melee_damage: f(5.5),
                melee_attack_power: 60,
                ranged_attack_power: 70,
            },
            mainhand_interval_ms: 2300,
            damage_school: 0,
            resistances: [0; 6],
            speed_walk: f(0.),
            speed_run: f(0.),
            corpse_delay_seconds: 0,
            equipment_template_id: 0,
            unit_flags: 0,
            npc_flags: 0,
        },
        class_levels: Rows::Complete(
            (1..=1)
                .map(|level| ClassLevelRow {
                    class: 1,
                    level,
                    base_health: 73,
                    base_mana: 137,
                    base_armor: 59,
                    base_melee_attack_power: f(100.),
                    base_ranged_attack_power: f(70.25),
                    base_damage: f(7.125),
                    attributes: [19, 23, 21, 22, 17],
                })
                .collect(),
        ),
        models: Rows::Complete(
            [31, 34]
                .into_iter()
                .map(|id| ModelRow {
                    id,
                    bounding_radius: f(0.5),
                    combat_reach: f(1.5),
                    speed_walk: f(1.),
                    speed_run: f(1.),
                    gender: u8::from(id == 34),
                    other_gender_id: match id {
                        31 => 34,
                        34 => 31,
                        _ => 0,
                    },
                })
                .collect(),
        ),
        displays: Rows::Complete(
            [31, 34]
                .into_iter()
                .map(|id| DisplayRow { id, scale: f(1.75) })
                .collect(),
        ),
        spawn_template: Rows::Complete(vec![SpawnTemplate {
            entry: 0,
            model_id: 0,
            equipment_id: -1,
            current_health: 0,
            current_mana: 0,
            faction: 0,
            npc_flags: -1,
            unit_flags: -1,
            spawn_flags: 0,
            relay_id: 0,
            string_id: 0,
        }]),
        external_features: Rows::Complete(vec![]),
        rates: Rates {
            normal_health: f(1.),
            normal_damage: f(1.),
            normal_corpse_decay_seconds: 83,
        },
    }
}
/// Default selection is display 31's other-gender display 34, object scale 1.25.
pub fn selection() -> ModelSelectionReceipt {
    prepare(&input())
        .unwrap()
        .resolve_with_model_receipt(SelectionRolls {
            model: Some(0),
            gender: Some(0),
            level: None,
        })
        .unwrap()
        .1
}
/// Authored selected raw width 1.375, display scale 1.75, model/render scale 2.5.
pub fn catalog() -> CreatureCatalog {
    catalog_with_width(1.375)
}
pub fn catalog_with_width(width: f32) -> CreatureCatalog {
    catalog_with_fields(width, 2.5, 1.75, false)
}
/// Test-only field mutations still pass through Chain and the production DBC reader.
pub fn catalog_with_fields(
    width: f32,
    model_scale: f32,
    display_scale: f32,
    short_model: bool,
) -> CreatureCatalog {
    catalog_with_height_fields(width, 2.875, model_scale, display_scale, short_model)
}
/// Raw field 15 remains independent of width, display scale and model/render scale.
pub fn catalog_with_height_fields(
    width: f32,
    height: f32,
    model_scale: f32,
    display_scale: f32,
    short_model: bool,
) -> CreatureCatalog {
    let mut model = [0_u32; 16];
    model[0] = 7;
    model[2] = 1;
    model[4] = model_scale.to_bits();
    model[14] = width.to_bits();
    model[15] = height.to_bits();
    let mut other = model;
    other[0] = 9;
    let mut display = [0_u32; 12];
    display[0] = 31;
    display[1] = 7;
    display[4] = display_scale.to_bits();
    let mut gender = display;
    gender[0] = 34;
    gender[1] = 9;
    let models = dbc(
        &[model, other],
        b"\0Authored\\WidthFixture.mdx\0",
        if short_model { 56 } else { 64 },
    );
    let displays = dbc(&[display, gender], b"\0", 48);
    let bytes = archive(&[
        ("DBFilesClient\\CreatureModelData.dbc", models),
        ("DBFilesClient\\CreatureDisplayInfo.dbc", displays),
    ]);
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "benilla-authored-model-width-{}-{}.MPQ",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, bytes).unwrap();
    let mut chain = Chain::open(&path).unwrap();
    let result = load_creature_catalog(&mut chain).unwrap();
    drop(chain);
    std::fs::remove_file(path).unwrap();
    result
}
/// The full fixture has genuine source-owned generation-zero width plus the
/// existing synthetic motion, combat and spatial authority assumptions.
pub fn profile() -> Profile {
    let mut p = synthetic_motion_fixture::profile();
    p.npcs[0].display_id = 34;
    p.initialize_creature_capabilities(synthetic_motion_fixture::creature_capability_sources(&p))
        .unwrap();
    p.initialize_creature_collision_widths(CreatureCollisionWidthRows::Complete(vec![
        CreatureCollisionWidthSource::from_initial_model(2, selection(), &catalog()).unwrap(),
    ]))
    .unwrap();
    p
}
fn dbc<const N: usize>(rows: &[[u32; N]], strings: &[u8], stride: u32) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for n in [rows.len() as u32, N as u32, stride, strings.len() as u32] {
        bytes.extend(n.to_le_bytes());
    }
    for row in rows {
        for n in &row[..stride as usize / 4] {
            bytes.extend(n.to_le_bytes());
        }
    }
    bytes.extend(strings);
    bytes
}

// Authored archive encoder follows benilla-native-dbc/tests/extract_chain.rs and
// benilla-mpq/src/crypto.rs (MIT OR Apache-2.0). It exists only in the synthetic feature.
fn crypto_table() -> [u32; 0x500] {
    let mut table = [0; 0x500];
    let mut seed: u32 = 0x0010_0001;
    for i in 0..0x100 {
        for j in 0..5 {
            seed = (seed.wrapping_mul(125).wrapping_add(3)) % 0x2a_aaab;
            let high = (seed & 0xffff) << 16;
            seed = (seed.wrapping_mul(125).wrapping_add(3)) % 0x2a_aaab;
            table[i + j * 0x100] = high | (seed & 0xffff);
        }
    }
    table
}
fn hash(name: &str, kind: usize) -> u32 {
    let table = crypto_table();
    let (mut a, mut b) = (0x7fed_7fed_u32, 0xeeee_eeee_u32);
    for c in name.bytes().map(|c| {
        if c == b'/' {
            b'\\'
        } else {
            c.to_ascii_uppercase()
        }
    }) {
        a = table[kind + c as usize] ^ a.wrapping_add(b);
        b = u32::from(c)
            .wrapping_add(a)
            .wrapping_add(b)
            .wrapping_add(b << 5)
            .wrapping_add(3);
    }
    a
}
fn encrypt(words: &mut [u32], mut key: u32) {
    let table = crypto_table();
    let mut seed = 0xeeee_eeee_u32;
    for word in words {
        seed = seed.wrapping_add(table[0x400 + (key & 0xff) as usize]);
        let plain = *word;
        *word = plain ^ key.wrapping_add(seed);
        key = (!key << 21).wrapping_add(0x1111_1111) | (key >> 11);
        seed = plain
            .wrapping_add(seed)
            .wrapping_add(seed << 5)
            .wrapping_add(3);
    }
}
pub(super) fn archive(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let slots = 16_u32;
    let hash_pos = 32_u32;
    let block_pos = hash_pos + slots * 16;
    let mut data_pos = block_pos + entries.len() as u32 * 16;
    let mut hashes = vec![u32::MAX; slots as usize * 4];
    let mut blocks = Vec::new();
    for (index, (name, bytes)) in entries.iter().enumerate() {
        let mut slot = (hash(name, 0) & (slots - 1)) as usize;
        while hashes[slot * 4 + 3] != u32::MAX {
            slot = (slot + 1) & (slots as usize - 1);
        }
        hashes[slot * 4..slot * 4 + 4].copy_from_slice(&[
            hash(name, 0x100),
            hash(name, 0x200),
            0,
            index as u32,
        ]);
        blocks.extend([
            data_pos,
            bytes.len() as u32,
            bytes.len() as u32,
            0x8000_0000,
        ]);
        data_pos += bytes.len() as u32;
    }
    encrypt(&mut hashes, hash("(hash table)", 0x300));
    encrypt(&mut blocks, hash("(block table)", 0x300));
    let mut bytes = vec![0_u8; 32];
    for (offset, value) in [
        (0, 0x1a51_504d_u32),
        (16, hash_pos),
        (20, block_pos),
        (24, slots),
        (28, entries.len() as u32),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    for word in hashes.into_iter().chain(blocks) {
        bytes.extend(word.to_le_bytes());
    }
    for (_, data) in entries {
        bytes.extend(data);
    }
    bytes
}
