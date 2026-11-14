// SPDX-License-Identifier: GPL-3.0-or-later
//! Selected source facial hair composes with the real quest/rest owner, not catalog guesses.
use super::*;
use n::{
    synthetic_player_appearance_fixture as appearance, synthetic_source_spline_fixture as splines,
};

fn selected() -> Harness {
    let mut h = Harness::new();
    let id = h
        .profile
        .create_character(benilla_profile::NewCharacter {
            name: "Brenda".into(),
            race: 1,
            class: 8,
            gender: 0,
            appearance: benilla_profile::Appearance {
                skin: 1,
                face: 0,
                hair_style: 1,
                hair_color: 1,
                facial_hair: 1,
            },
        })
        .unwrap();
    h.character = h.profile.character(id).unwrap().clone();
    h
}
fn profile(h: &Harness) -> n::Profile {
    let mut data = splines::surviving_profile();
    let a = h.character.appearance;
    data.initialize_player_appearance(
        n::PlayerAppearanceRequest {
            player: Selection::from_profile(&h.profile, &h.character)
                .unwrap()
                .player,
            race: h.character.race,
            class: h.character.class,
            gender: h.character.gender,
            choices: n::NativePlayerAppearanceChoices {
                skin: a.skin,
                face: a.face,
                hair_style: a.hair_style,
                hair_color: a.hair_color,
                facial_hair: a.facial_hair,
            },
        },
        &appearance::catalog(),
    )
    .unwrap();
    data
}
fn enter(h: &Harness, data: n::Profile, open: bool) -> NativeController {
    NativeController::enter_component_test(
        &h.root,
        &h.profile,
        &h.character,
        if open {
            Entry::Open { world: [8; 16] }
        } else {
            Entry::Create {
                world: [8; 16],
                seed: 42,
            }
        },
        data,
        &MeleeCatalog,
        MeleeAuthority::default(),
    )
    .unwrap()
}
fn assert_descriptor(c: &NativeController, events: &[SessionEvent]) {
    let mut fields = benilla_protocol::ObjectFields::default();
    for event in events {
        if let SessionEvent::ObjectValues {
            guid,
            fields: delta,
        } = event
        {
            if *guid == c.player_guid() {
                fields.merge(delta.clone());
            }
        }
    }
    let rest = c
        .world
        .state()
        .quest_progression
        .as_ref()
        .unwrap()
        .rest
        .client_state;
    let expected = (u32::from(rest) << 24) | 1;
    assert_eq!(
        fields
            .raw_fields()
            .filter(|(field, _)| *field == 194)
            .collect::<Vec<_>>(),
        vec![(194, expected)]
    );
    assert_eq!(fields.player_bytes_2(), Some(expected));
    assert_eq!(fields.player_facial_hair(), Some(1));
    assert_eq!(
        expected & 0x00ffff00,
        0,
        "fresh reserved and bank bytes stay zero"
    );
    assert_eq!(fields.player_skin(), Some(1));
    assert_eq!(
        c.world
            .state()
            .player_appearance()
            .unwrap()
            .choices()
            .facial_hair,
        1
    );
}
#[test]
fn selected_facial_hair_overrides_catalog_lower_bytes_and_survives_save_retry_reopen() {
    let h = selected();
    let data = profile(&h);
    let mut c = enter(&h, data.clone(), false);
    assert_eq!(
        MeleeCatalog.player_bytes_2(c.world.state().player),
        Some(0x00abcdef)
    );
    assert_ne!(
        MeleeCatalog.player_bytes_2(c.world.state().player).unwrap() & 0x00ffffff,
        1
    );
    let initial = c.initial_events().unwrap();
    assert_descriptor(&c, &initial);
    c.receive(move_to([0.25, 0.0, 0.0])).unwrap();
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored recoverable fault",
    )
    .unwrap();
    let withheld = c.pump(c.tick_duration());
    assert_eq!(withheld.ticks, 1);
    assert!(withheld.events.is_empty());
    assert!(matches!(
        withheld.pause.unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: false
        }
    ));
    let checkpoint = c.world.checkpoint().unwrap();
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    assert!(out.pause.is_none());
    assert_descriptor(&c, &out.events);
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(c.world.checkpoint().unwrap(), checkpoint);
    drop(c);
    let mut reopened = enter(&h, data, true);
    assert_eq!(reopened.world.checkpoint().unwrap(), checkpoint);
    let initial = reopened.initial_events().unwrap();
    assert_descriptor(&reopened, &initial);
}
