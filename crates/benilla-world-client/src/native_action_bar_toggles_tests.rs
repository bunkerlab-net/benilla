// SPDX-License-Identifier: GPL-3.0-or-later
//! PLAYER_FIELD_BYTES byte2 projection, including real death's low-byte release flag.
use super::*;
fn fields(events: &[SessionEvent], player: u64) -> &ObjectFields {
    events
        .iter()
        .filter_map(|event| match event {
            SessionEvent::ObjectValues { guid, fields } if *guid == player => Some(fields),
            _ => None,
        })
        .next_back()
        .expect("player descriptor")
}
fn command(world: &n::NativeWorld, toggles: u8) -> n::OwnedInput {
    n::OwnedInput {
        authenticated_owner: world.state().player.owner,
        input: n::Input {
            world: world.state().world,
            player: world.state().player,
            tick: world.state().tick + 1,
            sequence: world.state().next_sequence,
            command: n::Command::SetActionBarToggles { toggles },
        },
    }
}
#[test]
fn all_action_bar_bytes_reach_protocol_parser_initial_update_and_reopen() {
    let mut world = n::synthetic_quest_fixture::world();
    let mut projection = NativeProjection::new(
        &world,
        world.state().player,
        &super::quests::tests::Catalog::new(&world),
    )
    .unwrap();
    assert_eq!(
        fields(
            &projection.initial_events(&world).unwrap(),
            projection.player_guid()
        )
        .player_action_bar_toggles(),
        Some(0)
    );
    for value in 0..=255u8 {
        let input = command(&world, value);
        world
            .step(
                world.state().tick + 1,
                Some(input),
                &mut n::synthetic_quest_fixture::QuestAuthority::default(),
            )
            .unwrap();
        let events = projection.advance(&world).unwrap();
        let current = fields(&events, projection.player_guid());
        assert_eq!(current.player_action_bar_toggles(), Some(value));
        assert!(!current.player_release_timer_running());
        assert_eq!(current.player_combo_points(), Some(0));
        let restored = n::NativeWorld::restore_synthetic_component_test(
            world.profile().clone(),
            n::Checkpoint::decode(&world.checkpoint().unwrap().encode().unwrap()).unwrap(),
        )
        .unwrap();
        let fresh = NativeProjection::new(
            &restored,
            restored.state().player,
            &super::quests::tests::Catalog::new(&world),
        )
        .unwrap();
        assert_eq!(
            fields(
                &fresh.initial_events(&restored).unwrap(),
                fresh.player_guid()
            )
            .player_action_bar_toggles(),
            Some(value)
        );
    }
}
#[test]
fn real_death_and_dead_toggle_preserve_both_bytes_of_single_player_field() {
    use benilla_sim::player_death as d;
    use n::synthetic_fixture::*;
    for instanceable in [false, true] {
        let mut profile = n::synthetic_quest_fixture::profile();
        profile.npcs[0].max_health = 3000;
        let combat = profile.npcs[0].combat.as_mut().unwrap();
        combat.minimum_damage = float(10000.0);
        combat.maximum_damage = float(10000.0);
        profile
            .configure_player_death(
                d::DeathConfig {
                    map_instanceable: instanceable,
                    pve_reclaim_delay_enabled: true,
                },
                d::DeathSupport::checked(&d::REQUIRED_ABSENCES.map(|x| (x, Some(false)))).unwrap(),
            )
            .unwrap();
        let mut world =
            n::NativeWorld::synthetic_component_test(profile, [1; 16], player(), 7).unwrap();
        let mut projection = NativeProjection::new(
            &world,
            world.state().player,
            &super::quests::tests::Catalog::new(&world),
        )
        .unwrap();
        let mut orders = NativeMovementOrders::from_session_next_counter(37);
        let mut authority = n::synthetic_quest_fixture::QuestAuthority::default();
        let input = command(&world, 0xf5);
        world
            .step(world.state().tick + 1, Some(input), &mut authority)
            .unwrap();
        projection
            .advance_with_movement_orders(&world, &mut orders)
            .unwrap();
        let input = cast(&world, 60001);
        world
            .step(world.state().tick + 1, Some(input), &mut authority)
            .unwrap();
        projection
            .advance_with_movement_orders(&world, &mut orders)
            .unwrap();
        for _ in 0..100 {
            if world.state().health == 0 {
                break;
            }
            world
                .step(world.state().tick + 1, None, &mut authority)
                .unwrap();
            projection
                .advance_with_movement_orders(&world, &mut orders)
                .unwrap();
        }
        assert_eq!(world.state().health, 0);
        // Next Player update executes KillPlayer and owns the release flag/root.
        world
            .step(world.state().tick + 1, None, &mut authority)
            .unwrap();
        let events = projection
            .advance_with_movement_orders(&world, &mut orders)
            .unwrap();
        let dead = fields(&events, projection.player_guid());
        assert_eq!(dead.player_action_bar_toggles(), Some(0xf5));
        assert_eq!(dead.player_release_timer_running(), !instanceable);
        let input = command(&world, 0xff);
        world
            .step(world.state().tick + 1, Some(input), &mut authority)
            .unwrap();
        let events = projection
            .advance_with_movement_orders(&world, &mut orders)
            .unwrap();
        let dead = fields(&events, projection.player_guid());
        assert_eq!(dead.player_action_bar_toggles(), Some(0xff));
        assert_eq!(dead.player_release_timer_running(), !instanceable);
        assert_eq!(world.state().health, 0);
        let restored = n::NativeWorld::restore_synthetic_component_test(
            world.profile().clone(),
            world.checkpoint().unwrap(),
        )
        .unwrap();
        let fresh = NativeProjection::new(
            &restored,
            restored.state().player,
            &super::quests::tests::Catalog::new(&world),
        )
        .unwrap();
        let events = fresh
            .initial_events_with_movement_orders(&restored, &mut orders)
            .unwrap();
        let dead = fields(&events, fresh.player_guid());
        assert_eq!(dead.player_action_bar_toggles(), Some(0xff));
        assert_eq!(dead.player_release_timer_running(), !instanceable);
    }
}

#[test]
fn bare_alive_projection_does_not_publish_unproved_packed_player_field() {
    let world = n::synthetic_fixture::world();
    let projection =
        NativeProjection::new(&world, world.state().player, &super::tests::Catalog).unwrap();
    let events = projection.initial_events(&world).unwrap();
    let player = fields(&events, projection.player_guid());
    assert_eq!(player.player_action_bar_toggles(), None);
    assert_eq!(player.player_combo_points(), None);
}
