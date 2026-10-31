// SPDX-License-Identifier: GPL-3.0-or-later
//! Real saved-character selection and semantic descriptor delivery; no native admission.
use super::*;
use n::synthetic_player_appearance_fixture as appearance;

fn selected_character() -> Harness {
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
fn choices(h: &Harness) -> n::NativePlayerAppearanceChoices {
    let a = h.character.appearance;
    n::NativePlayerAppearanceChoices {
        skin: a.skin,
        face: a.face,
        hair_style: a.hair_style,
        hair_color: a.hair_color,
        facial_hair: a.facial_hair,
    }
}
fn data(h: &Harness, choices: n::NativePlayerAppearanceChoices) -> n::Profile {
    let mut data = fixture::initialized_profile();
    data.initialize_player_appearance(
        n::PlayerAppearanceRequest {
            player: Selection::from_profile(&h.profile, &h.character)
                .unwrap()
                .player,
            race: h.character.race,
            class: h.character.class,
            gender: h.character.gender,
            choices,
        },
        &appearance::catalog(),
    )
    .unwrap();
    data
}
fn assert_descriptor(events: &[SessionEvent], player: u64) {
    let mut fields = benilla_protocol::ObjectFields::default();
    for event in events {
        if let SessionEvent::ObjectValues {
            guid,
            fields: delta,
        } = event
        {
            if *guid == player {
                fields.merge(delta.clone());
            }
        }
    }
    assert_eq!(
        fields.player_bytes(),
        Some(u32::from_le_bytes([1, 0, 1, 1]))
    );
    assert_eq!(
        fields
            .raw_fields()
            .filter(|(field, _)| *field == 193)
            .collect::<Vec<_>>(),
        vec![(193, u32::from_le_bytes([1, 0, 1, 1]))]
    );
    assert_eq!(fields.player_skin(), Some(1));
    assert_eq!(fields.player_face(), Some(0));
    assert_eq!(fields.player_hair_style(), Some(1));
    assert_eq!(fields.player_hair_color(), Some(1));
    // Facial hair is retained by its source owner, not permission to fabricate
    // the bank/rest bytes sharing PLAYER_BYTES_2 without that separate owner.
    assert_eq!(fields.player_bytes_2(), None);
    assert!(!fields.raw_fields().any(|(field, _)| field == 194));
}
#[test]
fn selected_saved_appearance_survives_checkpoint_withholding_retry_and_reopen() {
    let h = selected_character();
    let selected = choices(&h);
    let data = data(&h, selected);
    let mut c = h.start(data.clone());
    assert_eq!(
        c.world.state().player_appearance().unwrap().choices(),
        selected
    );
    let initial = c.initial_events().unwrap();
    assert_descriptor(&initial, c.player_guid());
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
    let published = c.pump(Duration::ZERO);
    assert!(published.pause.is_none());
    assert_descriptor(&published.events, c.player_guid());
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(c.world.checkpoint().unwrap(), checkpoint);
    drop(c);
    let mut reopened = h.reopen(data);
    assert_eq!(reopened.world.checkpoint().unwrap(), checkpoint);
    assert_eq!(
        reopened
            .world
            .state()
            .player_appearance()
            .unwrap()
            .choices(),
        selected
    );
    let initial = reopened.initial_events().unwrap();
    assert_descriptor(&initial, reopened.player_guid());
}
#[test]
fn source_or_selected_snapshot_cosmetic_mismatch_refuses_before_world_container() {
    let h = selected_character();
    let mut wrong_choices = choices(&h);
    wrong_choices.skin = 0;
    let wrong_source = data(&h, wrong_choices);
    assert!(matches!(
        h.enter(
            Entry::Create {
                world: [8; 16],
                seed: 7
            },
            wrong_source,
            fixture::KernelAuthority::default()
        ),
        Err(Error::NativeRuntime(n::NativeError::AuthorityBinding))
    ));
    assert!(!h.path().parent().unwrap().exists());
    let correct = data(&h, choices(&h));
    let mut wrong_snapshot = h.character.clone();
    wrong_snapshot.appearance.skin = 0;
    assert!(matches!(
        NativeController::enter_component_test(
            &h.root,
            &h.profile,
            &wrong_snapshot,
            Entry::Create {
                world: [8; 16],
                seed: 7
            },
            correct,
            &Catalog,
            fixture::KernelAuthority::default()
        ),
        Err(Error::WrongCharacter)
    ));
    assert!(!h.path().parent().unwrap().exists());
    assert_eq!(h.profile.character(h.character.id).unwrap(), &h.character);
}
