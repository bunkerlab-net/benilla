// SPDX-License-Identifier: GPL-3.0-or-later
//! Raw signed watched-field projection through the existing protocol parser.
use super::*;
fn watched(events: &[SessionEvent], player: u64) -> i32 {
    events
        .iter()
        .filter_map(|event| match event {
            SessionEvent::ObjectValues { guid, fields } if *guid == player => {
                fields.player_watched_faction()
            }
            _ => None,
        })
        .next_back()
        .expect("player descriptor field1261 must be published")
}
#[test]
fn watched_raw_signed_field_initial_update_and_reopen_reach_protocol_parser() {
    let mut world = n::synthetic_fixture::world();
    let mut projection =
        NativeProjection::new(&world, world.state().player, &super::tests::Catalog).unwrap();
    assert_eq!(
        watched(
            &projection.initial_events(&world).unwrap(),
            projection.player_guid()
        ),
        -1
    );
    for value in [0, -2, 64, i32::MIN, i32::MAX, -1] {
        let command = n::OwnedInput {
            authenticated_owner: world.state().player.owner,
            input: n::Input {
                world: world.state().world,
                player: world.state().player,
                tick: world.state().tick + 1,
                sequence: world.state().next_sequence,
                command: n::Command::SetWatchedFaction { list_id: value },
            },
        };
        world
            .step(
                world.state().tick + 1,
                Some(command),
                &mut n::synthetic_fixture::SyntheticAuthority::default(),
            )
            .unwrap();
        assert_eq!(
            watched(
                &projection.advance(&world).unwrap(),
                projection.player_guid()
            ),
            value
        );
        let restored = n::NativeWorld::restore_synthetic_component_test(
            world.profile().clone(),
            n::Checkpoint::decode(&world.checkpoint().unwrap().encode().unwrap()).unwrap(),
        )
        .unwrap();
        let fresh =
            NativeProjection::new(&restored, restored.state().player, &super::tests::Catalog)
                .unwrap();
        assert_eq!(
            watched(
                &fresh.initial_events(&restored).unwrap(),
                fresh.player_guid()
            ),
            value
        );
    }
}
