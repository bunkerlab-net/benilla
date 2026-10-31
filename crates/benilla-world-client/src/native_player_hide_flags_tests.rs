// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded hide flags through actual stock object-field accessors; no rendered pixels.
use super::*;
use n::{
    synthetic_fixture as f, synthetic_player_appearance_fixture as appearance,
    synthetic_source_spline_fixture as source,
};
struct HideCatalog(super::quests::tests::Catalog);
impl HideCatalog {
    fn new(world: &n::NativeWorld) -> Self {
        Self(super::quests::tests::Catalog::new(world))
    }
}
impl ClientCatalog for HideCatalog {
    fn build(&self) -> u32 {
        self.0.build()
    }
    fn manifest_digest(&self) -> Digest {
        self.0.manifest_digest()
    }
    fn contains_map(&self, id: u32) -> bool {
        self.0.contains_map(id)
    }
    fn contains_creature_display(&self, id: u32) -> bool {
        matches!(id, 31 | 34 | 131 | 134) || self.0.contains_creature_display(id)
    }
    fn contains_item_display(&self, id: u32) -> bool {
        id == 900002
    }
    fn contains_spell(&self, id: u32) -> bool {
        matches!(id, 81 | 133 | 3050) || self.0.contains_spell(id)
    }
    fn player_power_type(&self, id: PlayerId) -> Option<u8> {
        self.0.player_power_type(id)
    }
    fn player_bytes_2(&self, id: PlayerId) -> Option<u32> {
        self.0.player_bytes_2(id)
    }
    fn item_metadata(&self, id: u32) -> Option<p::ItemInfo> {
        self.0.item_metadata(id)
    }
    fn quest_metadata(&self, id: u32) -> Option<crate::QuestMetadata> {
        self.0.quest_metadata(id)
    }
    fn quest_greeting(&self, id: u32) -> Option<String> {
        self.0.quest_greeting(id)
    }
    fn native_quest_greeting(&self, id: u32) -> Option<crate::NativeQuestGreeting> {
        self.0.native_quest_greeting(id)
    }
    fn native_quest_presentation(&self, id: u32) -> Option<crate::NativeQuestPresentation> {
        self.0.native_quest_presentation(id)
    }
}

fn world() -> n::NativeWorld {
    let mut profile = source::surviving_profile();
    profile
        .initialize_player_appearance(appearance::request(f::player()), &appearance::catalog())
        .unwrap();
    n::NativeWorld::synthetic_component_test(profile, [1; 16], f::player(), 42).unwrap()
}
fn fields(events: &[SessionEvent], player: u64) -> &ObjectFields {
    events
        .iter()
        .filter_map(|event| match event {
            SessionEvent::ObjectValues { guid, fields } if *guid == player => Some(fields),
            _ => None,
        })
        .next_back()
        .expect("actual player descriptor")
}
fn assert_flags(fields: &ObjectFields, expected: u32) {
    let raw: BTreeMap<_, _> = fields.raw_fields().collect();
    assert_eq!(raw.get(&190), Some(&expected));
    assert_eq!(fields.player_flags(), expected);
    assert_eq!(fields.player_hides_helm(), expected & 0x400 != 0);
    assert_eq!(fields.player_hides_cloak(), expected & 0x800 != 0);
    assert!(!fields.player_is_ghost());
    assert!(!fields.player_is_resting());
    assert!(!fields.player_is_group_leader());
    assert_eq!(expected & !0xc00, 0);
}
#[test]
fn source_hide_toggles_project_all_combinations_and_reopen_without_changing_appearance_or_equipment(
) {
    let mut world = world();
    assert!(world.profile().supports_player_hide_flags());
    let appearance = world.state().player_appearance().unwrap().choices();
    let equipment = world.state().combat.as_ref().unwrap().equipment.snapshot();
    let mut projection =
        NativeProjection::new(&world, world.state().player, &HideCatalog::new(&world)).unwrap();
    let initial = projection.initial_events(&world).unwrap();
    let initial_fields = fields(&initial, projection.player_guid());
    assert_flags(initial_fields, 0);
    let appearance_word = initial_fields.player_bytes();
    let sibling_word = initial_fields.player_bytes_2();
    assert!(appearance_word.is_some());
    let mut authority = source::CompleteAuthority::default();
    for (command, expected) in [
        (n::Command::ToggleHelm, 0x400),
        (n::Command::ToggleCloak, 0xc00),
        (n::Command::ToggleHelm, 0x800),
        (n::Command::ToggleCloak, 0),
        (n::Command::ToggleHelm, 0x400),
        (n::Command::ToggleHelm, 0),
        (n::Command::ToggleCloak, 0x800),
        (n::Command::ToggleCloak, 0),
    ] {
        let input = n::synthetic_quest_fixture::command(&world, command);
        world
            .step(world.state().tick + 1, Some(input), &mut authority)
            .unwrap();
        assert_eq!(world.state().player_hide_flags(), expected);
        assert_eq!(
            world.state().player_appearance().unwrap().choices(),
            appearance
        );
        assert_eq!(
            world.state().combat.as_ref().unwrap().equipment.snapshot(),
            equipment
        );
        let update = projection.advance(&world).unwrap();
        let updated_fields = fields(&update, projection.player_guid());
        assert_flags(updated_fields, expected);
        assert_eq!(updated_fields.player_bytes(), appearance_word);
        assert_eq!(updated_fields.player_bytes_2(), sibling_word);
        let checkpoint = world.checkpoint().unwrap();
        let restored = n::NativeWorld::restore_synthetic_component_test(
            world.profile().clone(),
            checkpoint.clone(),
        )
        .unwrap();
        assert_eq!(restored.checkpoint().unwrap(), checkpoint);
        let fresh = NativeProjection::new(
            &restored,
            restored.state().player,
            &HideCatalog::new(&restored),
        )
        .unwrap();
        assert_flags(
            fields(
                &fresh.initial_events(&restored).unwrap(),
                fresh.player_guid(),
            ),
            expected,
        );
        assert_eq!(
            restored.state().player_appearance().unwrap().choices(),
            appearance
        );
        assert_eq!(
            restored
                .state()
                .combat
                .as_ref()
                .unwrap()
                .equipment
                .snapshot(),
            equipment
        );
    }
}
#[test]
fn absent_appearance_or_raw_source_owner_keeps_player_flags_unknown() {
    let no_appearance = source::surviving_world();
    let mut no_raw_profile = n::synthetic_quest_fixture::profile();
    no_raw_profile
        .initialize_player_appearance(appearance::request(f::player()), &appearance::catalog())
        .unwrap();
    let no_raw =
        n::NativeWorld::synthetic_component_test(no_raw_profile, [1; 16], f::player(), 42).unwrap();
    for world in [no_appearance, no_raw] {
        assert!(!world.profile().supports_player_hide_flags());
        let projection =
            NativeProjection::new(&world, world.state().player, &HideCatalog::new(&world)).unwrap();
        let events = projection.initial_events(&world).unwrap();
        assert!(!fields(&events, projection.player_guid())
            .raw_fields()
            .any(|(offset, _)| offset == 190));
    }
}
#[test]
fn appearance_without_quest_source_owner_does_not_invent_player_flags() {
    let world = n::NativeWorld::synthetic_component_test(
        appearance::profile(appearance::request(f::player())),
        [8; 16],
        f::player(),
        1234,
    )
    .unwrap();
    assert!(!world.profile().supports_player_hide_flags());
    let projection =
        NativeProjection::new(&world, world.state().player, &super::tests::Catalog).unwrap();
    let events = projection.initial_events(&world).unwrap();
    assert!(!fields(&events, projection.player_guid())
        .raw_fields()
        .any(|(offset, _)| offset == 190));
}
