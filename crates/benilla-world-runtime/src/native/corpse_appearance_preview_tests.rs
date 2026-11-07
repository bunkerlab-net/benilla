// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only preview over authored loaded owners, not CreateCorpse or admission.
use super::*;
use synthetic_equipment_fixture::EquipmentAuthority;

fn profile(lethal: bool, appearance: bool, map: bool, model: bool) -> Profile {
    with_owners(
        synthetic_equipment_fixture::profile_with_lethal(lethal),
        appearance,
        map,
        model,
    )
}
fn with_owners(mut p: Profile, appearance: bool, map: bool, model: bool) -> Profile {
    if appearance {
        p.initialize_player_appearance(
            PlayerAppearanceRequest {
                player: synthetic_fixture::player(),
                race: 1,
                class: 8,
                gender: 0,
                choices: NativePlayerAppearanceChoices {
                    skin: 1,
                    face: 0,
                    hair_style: 1,
                    hair_color: 1,
                    facial_hair: 1,
                },
            },
            &synthetic_player_appearance_fixture::catalog(),
        )
        .unwrap();
    }
    if map {
        p.initialize_map_classification(&synthetic_map_classification_fixture::catalog(
            p.map_id, 0,
        ))
        .unwrap();
    }
    if model {
        // Existing equipment fixture already owns source motion; no synthetic
        // scalar or source-motion replacement is introduced by this test.
        assert!(p.source_motion.is_some());
        p.initialize_player_collision_height(
            &synthetic_player_model_fixture::catalog(),
            &synthetic_creature_model_fixture::catalog(),
        )
        .unwrap();
    }
    p
}
fn world(p: Profile) -> NativeWorld {
    NativeWorld::synthetic_component_test(p, [1; 16], synthetic_fixture::player(), 42).unwrap()
}
fn step(w: &mut NativeWorld, command: Command) {
    let input = synthetic_quest_fixture::command(w, command);
    w.step(
        w.state.tick + 1,
        Some(input),
        &mut EquipmentAuthority::default(),
    )
    .unwrap();
}
fn preview_unchanged(w: &NativeWorld) -> CorpseAppearancePreview {
    let checkpoint = w.checkpoint().unwrap();
    let records = w.records().to_vec();
    let preview = w.corpse_appearance_preview().unwrap();
    assert_eq!(preview.identity(), &checkpoint.identity());
    preview.validate_for(w).unwrap();
    assert_eq!(w.checkpoint().unwrap(), checkpoint);
    assert_eq!(w.records(), records);
    preview
}
#[test]
fn preview_uses_current_worn_custody_and_becomes_stale_after_real_tick() {
    let mut w = world(profile(false, true, true, true));
    let original = preview_unchanged(&w);
    let words = original.words();
    assert_eq!(words.native_display_id, 31);
    assert_eq!(words.bytes_1, (1 << 8) | (1 << 24));
    assert_eq!(words.bytes_2, (1 << 8) | (1 << 16) | (1 << 24));
    assert_eq!(words.guild_id, 0);
    assert_eq!(words.flags, 4);
    assert_eq!(words.equipment[4], 900002 | (5 << 24));
    assert!(words
        .equipment
        .iter()
        .enumerate()
        .all(|(slot, word)| slot == 4 || *word == 0));
    w.step(w.state.tick + 1, None, &mut EquipmentAuthority::default())
        .unwrap();
    assert!(matches!(
        original.validate_for(&w),
        Err(NativeError::AuthorityBinding)
    ));
    assert_eq!(preview_unchanged(&w).words(), original.words());
    step(
        &mut w,
        Command::EquipmentMove(EquipmentMoveRequest {
            source: 4,
            destination: 23,
        }),
    );
    assert!(w.state.combat.as_ref().unwrap().equipment.item(4).is_none());
    let detached = preview_unchanged(&w);
    assert_eq!(detached.words().equipment[4], 0);
    assert!(w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .as_ref()
        .unwrap()
        .slots
        .contains_key(&23));
    step(
        &mut w,
        Command::EquipmentMove(EquipmentMoveRequest {
            source: 23,
            destination: 4,
        }),
    );
    assert_eq!(preview_unchanged(&w).words(), original.words());
    let restored =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), w.checkpoint().unwrap())
            .unwrap();
    assert_eq!(preview_unchanged(&restored), preview_unchanged(&w));
}
#[test]
fn hidden_broken_worn_item_remains_in_preview_after_actual_lethal_durability_loss() {
    let mut w = world(profile(true, true, true, true));
    step(&mut w, Command::ToggleHelm);
    step(&mut w, Command::ToggleCloak);
    let before = preview_unchanged(&w);
    assert_eq!(before.words().flags, 4 | 8 | 16);
    let durability = w
        .state
        .combat
        .as_ref()
        .unwrap()
        .equipment
        .item(4)
        .unwrap()
        .current_durability;
    // Existing authored initializer has max durability 1; the source death
    // loss minimum-one branch genuinely breaks it. No live durability injection.
    assert_eq!(durability, 1);
    assert_eq!(
        w.state
            .combat
            .as_ref()
            .unwrap()
            .equipment
            .item(4)
            .unwrap()
            .max_durability,
        1
    );
    let target = w.state.npcs[&2].id;
    step(
        &mut w,
        Command::Cast {
            ability: 133,
            target,
        },
    );
    for _ in 0..100 {
        if !w.state.player_alive() {
            break;
        }
        w.step(w.state.tick + 1, None, &mut EquipmentAuthority::default())
            .unwrap();
    }
    assert!(!w.state.player_alive());
    assert!(
        w.state
            .combat
            .as_ref()
            .unwrap()
            .equipment
            .item(4)
            .unwrap()
            .current_durability
            < durability
    );
    assert!(w
        .state
        .combat
        .as_ref()
        .unwrap()
        .equipment
        .item(4)
        .unwrap()
        .is_broken());
    let after = preview_unchanged(&w);
    assert_eq!(after.words(), before.words());
    assert!(matches!(
        before.validate_for(&w),
        Err(NativeError::AuthorityBinding)
    ));
    let restored =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), w.checkpoint().unwrap())
            .unwrap();
    assert_eq!(preview_unchanged(&restored), after);
}
#[test]
fn missing_appearance_map_or_model_owner_is_not_defaulted_and_changes_nothing() {
    for (appearance, map, model) in [
        (false, true, true),
        (true, false, true),
        (true, true, false),
    ] {
        let w = world(profile(false, appearance, map, model));
        let checkpoint = w.checkpoint().unwrap();
        let records = w.records().to_vec();
        let result = w.corpse_appearance_preview();
        if !appearance {
            assert!(matches!(result, Err(NativeError::UnsupportedMechanic(_))));
        } else if !map {
            assert!(matches!(
                result,
                Err(NativeError::AuthorityUnavailable(
                    "corpse preview exact map classification"
                ))
            ));
        } else {
            assert!(matches!(
                result,
                Err(NativeError::AuthorityUnavailable(
                    "corpse preview native display owner"
                ))
            ));
        }
        assert_eq!(w.checkpoint().unwrap(), checkpoint);
        assert_eq!(w.records(), records);
    }
}

#[test]
fn missing_equipment_prototype_owner_is_not_known_empty_equipment() {
    let p = synthetic_source_spline_fixture::surviving_profile();
    assert!(p.quest_progression.as_ref().unwrap().loot.is_none());
    let w = world(with_owners(p, true, true, true));
    let checkpoint = w.checkpoint().unwrap();
    let records = w.records().to_vec();
    assert!(matches!(
        w.corpse_appearance_preview(),
        Err(NativeError::AuthorityUnavailable(
            "corpse preview equipment prototype owner"
        ))
    ));
    assert_eq!(w.checkpoint().unwrap(), checkpoint);
    assert_eq!(w.records(), records);
}
