// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored starting consumables, source callbacks and durable reopen.
use super::*;
use distinct_quest_loop_tests::LoopCatalog;
fn enter(h: &Harness, profile: n::Profile, open: bool) -> NativeController {
    let catalog = LoopCatalog::new(&profile);
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
        profile,
        &catalog,
        n::synthetic_food_drink_fixture::SpatialOnly::default(),
    )
    .unwrap()
}
fn consume(c: &mut NativeController, item_id: u32) {
    let slot = *c
        .world
        .state()
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .as_ref()
        .unwrap()
        .slots
        .iter()
        .find(|(_, item)| item.item == item_id)
        .unwrap()
        .0;
    c.receive(ClientCommand::UseItem {
        bag_index: 255,
        slot,
        spell_index: 0,
        target: benilla_protocol::messages::UseItemTarget::SelfImplicit,
    })
    .unwrap();
}
#[test]
fn food_and_drink_publish_casts_heartbeat_final_tick_and_expiry_across_reopen() {
    let h = Harness::new();
    let profile = n::synthetic_food_drink_fixture::profile();
    let mut c = enter(&h, profile.clone(), false);
    c.initial_events().unwrap();
    consume(&mut c, 2070);
    let events = pump_to(&mut c, 1);
    assert!(events.iter().any(|e| matches!(
        e,
        SessionEvent::CastResult {
            spell_id: 433,
            success: true,
            ..
        }
    )));
    assert!(events
        .iter()
        .any(|e| matches!(e, SessionEvent::SpellGo { spell_id: 433, .. })));
    assert!(events
        .iter()
        .any(|e| matches!(e, SessionEvent::PlaySpellVisual { kit_id: 406, .. })));
    assert!(!c.world.state().combat.as_ref().unwrap().standing);
    consume(&mut c, 159);
    let events = pump_to(&mut c, 2);
    assert!(events.iter().any(|e| matches!(
        e,
        SessionEvent::CastResult {
            spell_id: 430,
            success: true,
            ..
        }
    )));
    assert_eq!(c.world.state().food_drink().unwrap().holders.len(), 2);
    pump_to(&mut c, 37);
    let checkpoint = c.world.checkpoint().unwrap();
    let mut uninterrupted = c.world.clone();
    drop(c);
    let mut c = enter(&h, profile, true);
    assert_eq!(c.world.checkpoint().unwrap(), checkpoint);
    c.initial_events().unwrap();
    let catalog = LoopCatalog::new(uninterrupted.profile());
    let mut projection =
        NativeProjection::new(&uninterrupted, uninterrupted.state().player, &catalog).unwrap();
    let mut orders =
        NativeMovementOrders::from_session_next_counter(c.orders.counter.next_counter());
    let mut authority = n::synthetic_food_drink_fixture::SpatialOnly::default();
    let mut food_ticks = 0;
    let mut eat_emotes = 0;
    let mut heartbeat = false;
    while c.tick() < 103 {
        let tick = c.tick() + 1;
        let published = pump_to(&mut c, tick);
        uninterrupted.step(tick, None, &mut authority).unwrap();
        assert_eq!(
            c.world.checkpoint().unwrap(),
            uninterrupted.checkpoint().unwrap()
        );
        let expected = projection
            .advance_with_movement_orders(&uninterrupted, &mut orders)
            .unwrap();
        assert_eq!(format!("{published:?}"), format!("{expected:?}"));
        let events = &uninterrupted.last_tick().unwrap().events;
        food_ticks += events
            .iter()
            .filter(|e| matches!(e, n::Event::FoodDrink(n::FoodDrinkEvent::Health { .. })))
            .count();
        eat_emotes += events
            .iter()
            .filter(|e| matches!(e, n::Event::FoodDrink(n::FoodDrinkEvent::EatEmote)))
            .count();
        heartbeat |= published
            .iter()
            .any(|e| matches!(e, SessionEvent::PlaySpellVisual { kit_id: 406, .. }));
        if tick == 101 {
            let health = events
                .iter()
                .position(|e| matches!(e, n::Event::FoodDrink(n::FoodDrinkEvent::Health { .. })))
                .unwrap();
            let removed = events
                .iter()
                .position(|e| {
                    matches!(
                        e,
                        n::Event::FoodDrink(n::FoodDrinkEvent::Removed { spell: 433 })
                    )
                })
                .unwrap();
            assert!(health < removed);
        }
    }
    assert_eq!(food_ticks, 2);
    assert_eq!(eat_emotes, 3);
    assert!(heartbeat);
    assert!(c.world.state().food_drink().unwrap().holders.is_empty());
}
#[test]
fn standing_interrupts_both_consumables_and_reopen_keeps_them_removed() {
    let h = Harness::new();
    let profile = n::synthetic_food_drink_fixture::profile();
    let mut c = enter(&h, profile.clone(), false);
    c.initial_events().unwrap();
    consume(&mut c, 2070);
    pump_to(&mut c, 1);
    consume(&mut c, 159);
    pump_to(&mut c, 2);
    c.receive(ClientCommand::StandStateChange { state: 0 })
        .unwrap();
    pump_to(&mut c, 3);
    assert!(c.world.state().food_drink().unwrap().holders.is_empty());
    assert!(c.world.state().combat.as_ref().unwrap().standing);
    let checkpoint = c.world.checkpoint().unwrap();
    drop(c);
    let c = enter(&h, profile, true);
    assert_eq!(c.world.checkpoint().unwrap(), checkpoint);
    assert!(c.world.state().food_drink().unwrap().holders.is_empty());
}
