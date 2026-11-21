// SPDX-License-Identifier: GPL-3.0-or-later
//! Loaded authored source rows through the real field parser; no rendered-pixel
//! assertion, production entry, transform, ghost or invented display fallback.
use super::*;
use n::{synthetic_fixture as f, synthetic_player_model_fixture as models};

struct Catalog {
    player_models: bool,
}
impl ClientCatalog for Catalog {
    fn build(&self) -> u32 {
        5875
    }
    fn manifest_digest(&self) -> Digest {
        [9; 32]
    }
    fn contains_map(&self, id: u32) -> bool {
        id == 7
    }
    fn contains_creature_display(&self, id: u32) -> bool {
        id == 60001 || (self.player_models && matches!(id, 31 | 34))
    }
    fn contains_item_display(&self, _: u32) -> bool {
        false
    }
    fn contains_spell(&self, id: u32) -> bool {
        matches!(id, 60001 | 60002)
    }
    fn player_power_type(&self, _: PlayerId) -> Option<u8> {
        Some(0)
    }
    fn item_metadata(&self, _: u32) -> Option<p::ItemInfo> {
        None
    }
    fn quest_metadata(&self, _: u32) -> Option<crate::QuestMetadata> {
        None
    }
    fn quest_greeting(&self, _: u32) -> Option<String> {
        None
    }
}
fn world(profile: n::Profile) -> n::NativeWorld {
    n::NativeWorld::synthetic_component_test(profile, [8; 16], f::player(), 1234).unwrap()
}
fn fields(events: &[SessionEvent], guid: u64) -> &ObjectFields {
    let mut found = events.iter().filter_map(|event| match event {
        SessionEvent::ObjectValues { guid: id, fields } if *id == guid => Some(fields),
        _ => None,
    });
    let fields = found.next().expect("actual player descriptor update");
    assert!(found.next().is_none());
    fields
}
fn assert_model(fields: &ObjectFields, w: &n::NativeWorld, display: u32) {
    let witness = w.state().player_collision_height().unwrap().witness();
    let raw: BTreeMap<_, _> = fields.raw_fields().collect();
    assert_eq!(raw.get(&4), Some(&witness.stored_object_scale().bits()));
    assert_eq!(raw.get(&131), Some(&display));
    assert_eq!(raw.get(&132), Some(&display));
    assert_eq!(
        fields.object_scale_x().unwrap().to_bits(),
        witness.stored_object_scale().bits()
    );
    assert_eq!(fields.unit_displayid(), Some(display as i32));
    assert_eq!(fields.unit_native_displayid(), Some(display as i32));
    assert_eq!(witness.native_display_id(), display);
}
#[test]
fn loaded_player_model_descriptors_survive_actual_update_and_reopen() {
    for gender in [0, 1] {
        let mut profile = models::unowned_profile();
        let mut input = f::initialized_starter_input();
        input.gender = gender;
        profile
            .initialize_player(&input, f::float(100.0), 1)
            .unwrap();
        profile
            .initialize_player_collision_height(
                &models::catalog(),
                &n::synthetic_creature_model_fixture::catalog(),
            )
            .unwrap();
        let mut w = world(profile);
        let display = if gender == 0 { 31 } else { 34 };
        let catalog = Catalog {
            player_models: true,
        };
        let mut projection = NativeProjection::new(&w, w.state().player, &catalog).unwrap();
        let initial = projection.initial_events(&w).unwrap();
        assert_model(fields(&initial, projection.player_guid()), &w, display);
        assert_eq!(
            fields(&initial, projection.player_guid()).object_scale_x(),
            Some(4.375)
        );
        w.step(
            1,
            None,
            &mut n::synthetic_motion_fixture::SyntheticMotionAuthority::default(),
        )
        .unwrap();
        let update = projection.advance(&w).unwrap();
        assert_model(fields(&update, projection.player_guid()), &w, display);
        let checkpoint = w.checkpoint().unwrap();
        let restored = n::NativeWorld::restore_synthetic_component_test(
            w.profile().clone(),
            checkpoint.clone(),
        )
        .unwrap();
        let reopened = NativeProjection::new(&restored, restored.state().player, &catalog).unwrap();
        assert_model(
            fields(
                &reopened.initial_events(&restored).unwrap(),
                reopened.player_guid(),
            ),
            &restored,
            display,
        );
        assert_eq!(restored.checkpoint().unwrap(), checkpoint);
    }
}
#[test]
fn loaded_signed_zero_source_scales_publish_stored_zero_not_effective_one() {
    for zero in [0.0f32, -0.0f32] {
        for (display_scale, model_scale) in [(zero, 2.5), (1.75, zero)] {
            let w = world(models::profile_with_fields(
                display_scale,
                model_scale,
                2.875,
            ));
            let witness = w.state().player_collision_height().unwrap().witness();
            // InitDisplayIds' zero SetFloatValue keeps the initially stored +0;
            // Object::GetObjectScale separately repairs either zero to one.
            assert_eq!(witness.stored_object_scale().bits(), 0);
            assert_eq!(witness.effective_object_scale().bits(), 1.0f32.to_bits());
            let projection = NativeProjection::new(
                &w,
                w.state().player,
                &Catalog {
                    player_models: true,
                },
            )
            .unwrap();
            let events = projection.initial_events(&w).unwrap();
            assert_model(fields(&events, projection.player_guid()), &w, 31);
            assert_eq!(
                fields(&events, projection.player_guid())
                    .object_scale_x()
                    .unwrap()
                    .to_bits(),
                0
            );
        }
    }
}
#[test]
fn absent_player_model_owner_leaves_descriptor_words_unknown_not_defaulted() {
    let w = world(models::unowned_profile());
    assert!(w.state().player_collision_height().is_none());
    let projection = NativeProjection::new(
        &w,
        w.state().player,
        &Catalog {
            player_models: false,
        },
    )
    .unwrap();
    let events = projection.initial_events(&w).unwrap();
    let fields = fields(&events, projection.player_guid());
    let raw: BTreeMap<_, _> = fields.raw_fields().collect();
    for offset in [4, 131, 132] {
        assert!(!raw.contains_key(&offset));
    }
    assert_eq!(fields.object_scale_x(), None);
    assert_eq!(fields.unit_displayid(), None);
    assert_eq!(fields.unit_native_displayid(), None);
}
#[test]
fn retained_player_model_requires_exact_client_display_metadata() {
    let w = world(models::profile());
    assert!(matches!(
        NativeProjection::new(
            &w,
            w.state().player,
            &Catalog {
                player_models: false
            }
        ),
        Err(Error::UnboundPlayerDisplay(31))
    ));
    assert!(NativeProjection::new(
        &w,
        w.state().player,
        &Catalog {
            player_models: true
        }
    )
    .is_ok());
}
