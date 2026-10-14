// SPDX-License-Identifier: GPL-3.0-or-later
//! Committed player-holder projection using authored source-shaped fixtures only.
use super::*;
use n::synthetic_slow_fixture::{self as fixture, SyntheticSlowEffect, SyntheticSlowPhase};

struct SlowCatalog;
impl ClientCatalog for SlowCatalog {
    fn build(&self) -> u32 {
        tests::Catalog.build()
    }
    fn manifest_digest(&self) -> Digest {
        tests::Catalog.manifest_digest()
    }
    fn contains_map(&self, id: u32) -> bool {
        tests::Catalog.contains_map(id)
    }
    fn contains_creature_display(&self, id: u32) -> bool {
        tests::Catalog.contains_creature_display(id)
    }
    fn contains_item_display(&self, id: u32) -> bool {
        tests::Catalog.contains_item_display(id)
    }
    fn contains_spell(&self, id: u32) -> bool {
        matches!(id, 61004 | 61005) || tests::Catalog.contains_spell(id)
    }
    fn player_power_type(&self, player: PlayerId) -> Option<u8> {
        tests::Catalog.player_power_type(player)
    }
    fn item_metadata(&self, id: u32) -> Option<p::ItemInfo> {
        tests::Catalog.item_metadata(id)
    }
    fn quest_metadata(&self, id: u32) -> Option<crate::QuestMetadata> {
        tests::Catalog.quest_metadata(id)
    }
    fn quest_greeting(&self, id: u32) -> Option<String> {
        tests::Catalog.quest_greeting(id)
    }
}
fn authority() -> n::synthetic_motion_fixture::SyntheticMotionAuthority {
    n::synthetic_motion_fixture::SyntheticMotionAuthority {
        visits: Some(false),
        ..Default::default()
    }
}
fn world(profile: n::Profile) -> n::NativeWorld {
    n::NativeWorld::synthetic_component_test(profile, [8; 16], n::synthetic_fixture::player(), 1234)
        .unwrap()
}
fn speeds(events: &[SessionEvent]) -> Vec<(p::SpeedKind, u32, u32)> {
    events
        .iter()
        .filter_map(|event| match event {
            SessionEvent::ForceSpeedChange {
                kind,
                counter,
                speed,
                ..
            } => Some((*kind, *counter, speed.to_bits())),
            _ => None,
        })
        .collect()
}
fn trace(events: &[SessionEvent], player: u64) -> Vec<&'static str> {
    events
        .iter()
        .filter_map(|event| match event {
            SessionEvent::ObjectValues { guid, fields } if *guid == player => {
                Some(if fields.unit_aura(32).is_some() {
                    "slot"
                } else {
                    "clear"
                })
            }
            SessionEvent::AuraDuration { .. } => Some("duration"),
            SessionEvent::ForceSpeedChange { kind, .. } => Some(match kind {
                p::SpeedKind::Run => "run",
                p::SpeedKind::RunBack => "back",
                p::SpeedKind::Swim => "swim",
                _ => panic!("unexpected speed kind"),
            }),
            _ => None,
        })
        .collect()
}

#[test]
fn aura_catalog_is_bound_independently_from_learned_spells_and_initial_orders_are_fresh() {
    let w = fixture::world();
    assert!(matches!(
        NativeProjection::new(&w, w.state().player, &tests::Catalog),
        Err(Error::UnboundSpell(61004))
    ));
    let projection = NativeProjection::new(&w, w.state().player, &SlowCatalog).unwrap();
    assert!(matches!(
        projection.initial_events(&w),
        Err(Error::MissingMovementOrderState)
    ));
    let mut orders = NativeMovementOrders::from_session_next_counter(37);
    let events = projection
        .initial_events_with_movement_orders(&w, &mut orders)
        .unwrap();
    assert_eq!(
        speeds(&events)
            .iter()
            .map(|s| (s.0, s.1))
            .collect::<Vec<_>>(),
        vec![
            (p::SpeedKind::Run, 37),
            (p::SpeedKind::RunBack, 38),
            (p::SpeedKind::Swim, 39)
        ]
    );
    assert_eq!(orders.next_counter(), 40);
    assert!(events.iter().any(|event| matches!(event, SessionEvent::SpellBook { spell_ids, .. } if !spell_ids.contains(&61004) && !spell_ids.contains(&61005))));
}

#[test]
fn applied_fields_duration_and_replacement_restore_apply_speeds_keep_source_order() {
    let effect = SyntheticSlowEffect {
        tick: 1,
        spell_id: 61004,
        caster: NpcId {
            spawn: 2,
            generation: 0,
        },
        phase: SyntheticSlowPhase::BeforePlayerUpdate,
    };
    let mut w = world(fixture::with_effects(vec![effect, effect]));
    let mut projection = NativeProjection::new(&w, w.state().player, &SlowCatalog).unwrap();
    let mut orders = NativeMovementOrders::from_session_next_counter(10);
    w.step(1, None, &mut authority()).unwrap();
    let events = projection
        .advance_with_movement_orders(&w, &mut orders)
        .unwrap();
    assert_eq!(
        trace(&events, projection.player_guid()),
        vec![
            "slot", "duration", "run", "back", "swim", "run", "back", "swim", "clear", "slot",
            "duration", "run", "back", "swim", "slot"
        ]
    );
    let fields = events
        .iter()
        .find_map(|event| match event {
            SessionEvent::ObjectValues { guid, fields } if *guid == projection.player_guid() => {
                Some(fields)
            }
            _ => None,
        })
        .unwrap();
    let aura = fields.unit_aura(32).unwrap();
    assert_eq!(
        (aura.spell_id, aura.flags, aura.level, aura.stacks),
        (61004, 8, w.profile().npcs[0].ai.level, 1)
    );
    assert!(fields.unit_aura(33).is_none());
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(
                e,
                SessionEvent::AuraDuration {
                    slot: 32,
                    remaining_ms: 400
                }
            ))
            .count(),
        2
    );
    let orders = speeds(&events);
    assert_eq!(orders.len(), 9);
    assert_eq!(
        orders.iter().map(|s| s.1).collect::<Vec<_>>(),
        (10..19).collect::<Vec<_>>()
    );
    assert_eq!(orders[0].2, orders[6].2);
    assert_eq!(orders[3].2, 7f32.to_bits());
}

#[test]
fn expiry_restores_speed_before_slot_clear_and_reopen_restores_remaining_duration() {
    let mut w = fixture::world();
    let mut projection = NativeProjection::new(&w, w.state().player, &SlowCatalog).unwrap();
    let mut orders = NativeMovementOrders::from_session_next_counter(0);
    let mut authority = authority();
    for tick in 1..=2 {
        w.step(tick, None, &mut authority).unwrap();
        projection
            .advance_with_movement_orders(&w, &mut orders)
            .unwrap();
    }
    let restored = n::NativeWorld::restore_synthetic_component_test(
        w.profile().clone(),
        w.checkpoint().unwrap(),
    )
    .unwrap();
    let fresh = NativeProjection::new(&restored, restored.state().player, &SlowCatalog).unwrap();
    let mut fresh_orders = NativeMovementOrders::from_session_next_counter(73);
    let initial = fresh
        .initial_events_with_movement_orders(&restored, &mut fresh_orders)
        .unwrap();
    assert!(initial.iter().any(|e| matches!(
        e,
        SessionEvent::AuraDuration {
            slot: 32,
            remaining_ms: 300
        }
    )));
    assert_eq!(
        speeds(&initial).iter().map(|s| s.1).collect::<Vec<_>>(),
        vec![73, 74, 75]
    );
    for tick in 3..5 {
        w.step(tick, None, &mut authority).unwrap();
        assert!(speeds(
            &projection
                .advance_with_movement_orders(&w, &mut orders)
                .unwrap()
        )
        .is_empty());
    }
    w.step(5, None, &mut authority).unwrap();
    let expired = projection
        .advance_with_movement_orders(&w, &mut orders)
        .unwrap();
    assert_eq!(
        trace(&expired, projection.player_guid()),
        vec!["run", "back", "swim", "clear", "clear"]
    );
    assert!(w
        .state()
        .player_slow
        .as_ref()
        .unwrap()
        .holders()
        .next()
        .is_none());
    let fields = expired
        .iter()
        .find_map(|event| match event {
            SessionEvent::ObjectValues { guid, fields } if *guid == projection.player_guid() => {
                Some(fields)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(
        fields
            .raw_fields()
            .find(|(index, _)| *index == 109)
            .unwrap()
            .1
            & 0xff,
        u32::from(w.profile().npcs[0].ai.level)
    );
    let restored = n::NativeWorld::restore_synthetic_component_test(
        w.profile().clone(),
        w.checkpoint().unwrap(),
    )
    .unwrap();
    let fresh = NativeProjection::new(&restored, restored.state().player, &SlowCatalog).unwrap();
    let initial = fresh
        .initial_events_with_movement_orders(
            &restored,
            &mut NativeMovementOrders::from_session_next_counter(0),
        )
        .unwrap();
    let initial_fields = initial
        .iter()
        .find_map(|event| match event {
            SessionEvent::ObjectValues { guid, fields } if *guid == fresh.player_guid() => {
                Some(fields)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(
        fields.raw_fields().find(|(index, _)| *index == 109),
        initial_fields.raw_fields().find(|(index, _)| *index == 109)
    );
}

#[test]
fn failed_counter_allocation_does_not_advance_projection_or_session_namespace() {
    let mut w = fixture::world();
    let mut projection = NativeProjection::new(&w, w.state().player, &SlowCatalog).unwrap();
    let mut overflow = NativeMovementOrders::from_session_next_counter(u32::MAX - 1);
    assert!(matches!(
        projection.initial_events_with_movement_orders(&w, &mut overflow),
        Err(Error::Unrepresentable)
    ));
    assert_eq!(overflow.next_counter(), u32::MAX - 1);
    w.step(1, None, &mut authority()).unwrap();
    assert!(matches!(
        projection.advance_with_movement_orders(&w, &mut overflow),
        Err(Error::Unrepresentable)
    ));
    assert_eq!(overflow.next_counter(), u32::MAX - 1);
    let mut retry = NativeMovementOrders::from_session_next_counter(100);
    assert_eq!(
        speeds(
            &projection
                .advance_with_movement_orders(&w, &mut retry)
                .unwrap()
        )
        .len(),
        3
    );
    assert_eq!(retry.next_counter(), 103);
}
