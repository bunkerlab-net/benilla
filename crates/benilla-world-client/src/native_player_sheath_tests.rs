// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual source-world projection and stock field parser, not rendered poses.
use super::*;
fn assert_sheath(events: &[SessionEvent], guid: u64, expected: u8) {
    let fields = player_fields(events, guid);
    assert_eq!(fields.unit_sheath_state(), Some(expected));
    let raw: Vec<_> = fields.raw_fields().collect();
    assert_eq!(
        raw.iter()
            .filter(|(i, _)| *i == 164)
            .copied()
            .collect::<Vec<_>>(),
        vec![(164, 0x2800 | u32::from(expected))]
    );
    assert!(
        !raw.iter().any(|(i, _)| (37..46).contains(i)),
        "no unowned virtual-item fields"
    );
    for slot in 0..3 {
        assert_eq!(fields.unit_virtual_item_display(slot), None);
        assert_eq!(fields.unit_virtual_item_info(slot), None);
        assert_eq!(fields.unit_virtual_item_sheath(slot), None);
    }
    // Count before field merging as well: no duplicate whole-word publication.
    let occurrences = events
        .iter()
        .filter_map(|event| match event {
            SessionEvent::ObjectValues {
                guid: actual,
                fields,
            } if *actual == guid => Some(fields.raw_fields().filter(|(i, _)| *i == 164).count()),
            _ => None,
        })
        .sum::<usize>();
    assert_eq!(occurrences, 1);
}
#[test]
fn actual_source_sheath_commands_project_one_complete_word_without_equipment_mutation() {
    let mut world = n::synthetic_equipment_fixture::world();
    let inventory = world
        .state()
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .clone();
    let stats = world.state().combat.as_ref().unwrap().stats.clone();
    let equipment = world.state().combat.as_ref().unwrap().equipment.snapshot();
    let mut projection =
        NativeProjection::new(&world, world.state().player, &Catalog::new(&world)).unwrap();
    assert_sheath(
        &projection.initial_events(&world).unwrap(),
        projection.player_guid(),
        0,
    );
    for (raw, expected) in [
        (0, 0),
        (1, 1),
        (1, 1),
        (2, 2),
        (3, 2),
        (256, 2),
        (u32::MAX, 2),
        (0, 0),
    ] {
        let input =
            n::synthetic_quest_fixture::command(&world, n::Command::SetSheathed { state: raw });
        world
            .step(
                world.state().tick + 1,
                Some(input),
                &mut n::synthetic_equipment_fixture::EquipmentAuthority::default(),
            )
            .unwrap();
        let events = projection.advance(&world).unwrap();
        assert_sheath(&events, projection.player_guid(), expected);
        assert!(!events.iter().any(|e| matches!(
            e,
            SessionEvent::ItemCreate { .. }
                | SessionEvent::ObjectDestroyed(_)
                | SessionEvent::ItemPushResult(_)
        )));
        assert_eq!(
            world.state().quest_progression.as_ref().unwrap().inventory,
            inventory
        );
        assert_eq!(world.state().combat.as_ref().unwrap().stats, stats);
        assert_eq!(
            world.state().combat.as_ref().unwrap().equipment.snapshot(),
            equipment
        );
        let cp = world.checkpoint().unwrap();
        let restored =
            n::NativeWorld::restore_synthetic_component_test(world.profile().clone(), cp.clone())
                .unwrap();
        assert_eq!(restored.checkpoint().unwrap(), cp);
        let fresh =
            NativeProjection::new(&restored, restored.state().player, &Catalog::new(&restored))
                .unwrap();
        assert_sheath(
            &fresh.initial_events(&restored).unwrap(),
            fresh.player_guid(),
            expected,
        );
    }
}
#[test]
fn missing_source_qualification_keeps_sheath_word_unknown_not_zero() {
    // This authored loot world has metadata but lacks the raw spell/interaction
    // closure required to own the complete descriptor word.
    let world = fixture::world();
    assert!(!world.profile().supports_player_sheath());
    let projection =
        NativeProjection::new(&world, world.state().player, &Catalog::new(&world)).unwrap();
    let events = projection.initial_events(&world).unwrap();
    let fields = player_fields(&events, projection.player_guid());
    assert_eq!(fields.unit_sheath_state(), None);
    assert!(!fields.raw_fields().any(|(i, _)| i == 164));
    let restored = n::NativeWorld::restore_synthetic_component_test(
        world.profile().clone(),
        world.checkpoint().unwrap(),
    )
    .unwrap();
    let fresh = NativeProjection::new(&restored, restored.state().player, &Catalog::new(&restored))
        .unwrap();
    assert_eq!(
        player_fields(
            &fresh.initial_events(&restored).unwrap(),
            fresh.player_guid()
        )
        .unit_sheath_state(),
        None
    );
}
