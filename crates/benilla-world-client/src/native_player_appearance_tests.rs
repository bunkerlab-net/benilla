// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact loaded-source cosmetic ownership through stock descriptor accessors.
//! No rendered pixels, guild/bank/rest state, ghost or native entry is claimed.
use super::*;
use n::{synthetic_fixture as f, synthetic_player_appearance_fixture as appearance};
fn fields(events: &[SessionEvent], guid: u64) -> &ObjectFields {
    events
        .iter()
        .find_map(|event| match event {
            SessionEvent::ObjectValues { guid: id, fields } if *id == guid => Some(fields),
            _ => None,
        })
        .expect("actual player values")
}
fn assert_appearance(fields: &ObjectFields, choices: n::NativePlayerAppearanceChoices) {
    let expected = u32::from_le_bytes([
        choices.skin,
        choices.face,
        choices.hair_style,
        choices.hair_color,
    ]);
    let raw: BTreeMap<_, _> = fields.raw_fields().collect();
    assert_eq!(raw.get(&193), Some(&expected));
    assert_eq!(fields.player_bytes(), Some(expected));
    assert_eq!(fields.player_skin(), Some(choices.skin));
    assert_eq!(fields.player_face(), Some(choices.face));
    assert_eq!(fields.player_hair_style(), Some(choices.hair_style));
    assert_eq!(fields.player_hair_color(), Some(choices.hair_color));
    assert!(
        !raw.contains_key(&194),
        "facial hair alone cannot supply unknown bank/rest bytes"
    );
    assert_eq!(fields.player_bytes_2(), None);
    assert_eq!(fields.player_facial_hair(), None);
}
#[test]
fn exact_loaded_player_appearance_packs_all_byte_positions_without_inventing_bytes_two() {
    let catalog = appearance::catalog();
    for bits in 0..16u8 {
        let mut request = appearance::request(f::player());
        request.choices = n::NativePlayerAppearanceChoices {
            skin: bits & 1,
            face: (bits >> 1) & 1,
            hair_style: (bits >> 2) & 1,
            hair_color: (bits >> 3) & 1,
            facial_hair: 1,
        };
        let mut profile = f::initialized_profile();
        profile
            .initialize_player_appearance(request, &catalog)
            .unwrap();
        let world =
            n::NativeWorld::synthetic_component_test(profile, [8; 16], f::player(), 1234).unwrap();
        assert_eq!(
            world.state().player_appearance().unwrap().choices(),
            request.choices
        );
        let projection =
            NativeProjection::new(&world, world.state().player, &super::tests::Catalog).unwrap();
        assert_appearance(
            fields(
                &projection.initial_events(&world).unwrap(),
                projection.player_guid(),
            ),
            request.choices,
        );
    }
}
#[test]
fn selected_appearance_and_unprojected_facial_hair_survive_update_and_reopen() {
    let request = appearance::request(f::player());
    let mut world = n::NativeWorld::synthetic_component_test(
        appearance::profile(request),
        [8; 16],
        f::player(),
        1234,
    )
    .unwrap();
    let mut projection =
        NativeProjection::new(&world, world.state().player, &super::tests::Catalog).unwrap();
    assert_appearance(
        fields(
            &projection.initial_events(&world).unwrap(),
            projection.player_guid(),
        ),
        request.choices,
    );
    world
        .step(1, None, &mut f::SyntheticAuthority::default())
        .unwrap();
    let update = projection.advance(&world).unwrap();
    assert_appearance(fields(&update, projection.player_guid()), request.choices);
    let checkpoint = world.checkpoint().unwrap();
    let restored = n::NativeWorld::restore_synthetic_component_test(
        world.profile().clone(),
        checkpoint.clone(),
    )
    .unwrap();
    assert_eq!(
        restored
            .state()
            .player_appearance()
            .unwrap()
            .choices()
            .facial_hair,
        1
    );
    let fresh =
        NativeProjection::new(&restored, restored.state().player, &super::tests::Catalog).unwrap();
    assert_appearance(
        fields(
            &fresh.initial_events(&restored).unwrap(),
            fresh.player_guid(),
        ),
        request.choices,
    );
    assert_eq!(restored.checkpoint().unwrap(), checkpoint);
}
#[test]
fn absent_appearance_owner_does_not_fabricate_default_cosmetic_bytes() {
    let mut world = n::NativeWorld::synthetic_component_test(
        f::initialized_profile(),
        [8; 16],
        f::player(),
        1234,
    )
    .unwrap();
    assert!(world.state().player_appearance().is_none());
    let mut projection =
        NativeProjection::new(&world, world.state().player, &super::tests::Catalog).unwrap();
    for update in [false, true] {
        let events = if update {
            world
                .step(1, None, &mut f::SyntheticAuthority::default())
                .unwrap();
            projection.advance(&world).unwrap()
        } else {
            projection.initial_events(&world).unwrap()
        };
        let fields = fields(&events, projection.player_guid());
        let raw: BTreeMap<_, _> = fields.raw_fields().collect();
        assert!(!raw.contains_key(&193));
        assert!(!raw.contains_key(&194));
        assert_eq!(fields.player_skin(), None);
        assert_eq!(fields.player_face(), None);
        assert_eq!(fields.player_hair_style(), None);
        assert_eq!(fields.player_hair_color(), None);
        assert_eq!(fields.player_facial_hair(), None);
    }
}
#[test]
fn appearance_word_preserves_existing_quest_owned_bytes_two_exactly() {
    let baseline = n::synthetic_quest_fixture::world();
    let baseline_projection = NativeProjection::new(
        &baseline,
        baseline.state().player,
        &super::quests::tests::Catalog::new(&baseline),
    )
    .unwrap();
    let baseline_events = baseline_projection.initial_events(&baseline).unwrap();
    let baseline_fields = fields(&baseline_events, baseline_projection.player_guid());
    assert_eq!(baseline_fields.player_bytes(), None);
    let prior_bytes_two = baseline_fields.player_bytes_2().unwrap();
    assert_eq!(prior_bytes_two & 0x00ff_ffff, 0x00ab_cdef);

    let request = appearance::request(baseline.state().player);
    assert_eq!(request.choices.facial_hair, 1);
    let mut profile = baseline.profile().clone();
    profile
        .initialize_player_appearance(request, &appearance::catalog())
        .unwrap();
    let world = n::NativeWorld::synthetic_component_test(
        profile,
        baseline.state().world,
        baseline.state().player,
        42,
    )
    .unwrap();
    let projection = NativeProjection::new(
        &world,
        world.state().player,
        &super::quests::tests::Catalog::new(&world),
    )
    .unwrap();
    let events = projection.initial_events(&world).unwrap();
    let actual = fields(&events, projection.player_guid());
    let raw: BTreeMap<_, _> = actual.raw_fields().collect();
    assert_eq!(raw.get(&193), Some(&u32::from_le_bytes([1, 0, 1, 0])));
    assert_eq!(raw.get(&194), Some(&prior_bytes_two));
    assert_eq!(actual.player_bytes_2(), Some(prior_bytes_two));
    assert_eq!(
        actual.player_facial_hair(),
        baseline_fields.player_facial_hair()
    );
    assert_ne!(
        actual.player_facial_hair(),
        Some(request.choices.facial_hair)
    );
    assert_eq!(
        world.state().player_appearance().unwrap().choices(),
        request.choices
    );
}
