// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
fn world() -> NativeWorld {
    NativeWorld::synthetic_component_test(
        synthetic_food_drink_fixture::profile(),
        [31; 16],
        synthetic_fixture::player(),
        42,
    )
    .unwrap()
}
fn step(
    w: &mut NativeWorld,
    a: &mut synthetic_food_drink_fixture::SpatialOnly,
    c: Option<Command>,
) {
    let input = c.map(|c| synthetic_distinct_quest_fixture::command(w, c));
    w.step(w.state.tick + 1, input, a).unwrap();
}
fn slot(w: &NativeWorld, id: u32) -> u8 {
    *w.state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .as_ref()
        .unwrap()
        .slots
        .iter()
        .find(|(_, item)| item.item == id)
        .unwrap()
        .0
}
#[test]
fn food_drink_item_cast_consumes_selected_member_sits_and_reopens_exactly() {
    let mut world = world();
    let mut authority = synthetic_food_drink_fixture::SpatialOnly::default();
    let food_slot = slot(&world, 2070);
    step(
        &mut world,
        &mut authority,
        Some(Command::UseItem {
            slot: food_slot,
            spell_index: 0,
        }),
    );
    assert!(!world.state.combat.as_ref().unwrap().standing);
    assert_eq!(
        world.state.food_drink().unwrap().holders[&433]
            .aura
            .remaining_ms(),
        10_000
    );
    assert!(world.last_tick().unwrap().events.iter().any(|e| matches!(
        e,
        Event::FoodDrink(FoodDrinkEvent::PlaySpellVisual { kit: 406 })
    )));
    let drink_slot = slot(&world, 159);
    step(
        &mut world,
        &mut authority,
        Some(Command::UseItem {
            slot: drink_slot,
            spell_index: 0,
        }),
    );
    assert_eq!(world.state.food_drink().unwrap().holders.len(), 2);
    let mut reopened = NativeWorld::restore_synthetic_component_test(
        world.profile.clone(),
        world.checkpoint().unwrap(),
    )
    .unwrap();
    let mut other = synthetic_food_drink_fixture::SpatialOnly::default();
    step(
        &mut world,
        &mut authority,
        Some(Command::SetStanding { standing: true }),
    );
    step(
        &mut reopened,
        &mut other,
        Some(Command::SetStanding { standing: true }),
    );
    assert_eq!(world.checkpoint().unwrap(), reopened.checkpoint().unwrap());
    assert!(world.state.food_drink().unwrap().holders.is_empty());
}
#[test]
fn food_drink_final_callbacks_precede_removal_and_reopen_matches_every_tick() {
    let mut world = world();
    let mut authority = synthetic_food_drink_fixture::SpatialOnly::default();
    let food = slot(&world, 2070);
    step(
        &mut world,
        &mut authority,
        Some(Command::UseItem {
            slot: food,
            spell_index: 0,
        }),
    );
    let drink = slot(&world, 159);
    step(
        &mut world,
        &mut authority,
        Some(Command::UseItem {
            slot: drink,
            spell_index: 0,
        }),
    );
    let mut reopened = NativeWorld::restore_synthetic_component_test(
        world.profile.clone(),
        world.checkpoint().unwrap(),
    )
    .unwrap();
    let mut other = synthetic_food_drink_fixture::SpatialOnly::default();
    let mut food_ticks = 0;
    let mut emotes = 0;
    while world.state.tick < 103 {
        step(&mut world, &mut authority, None);
        step(&mut reopened, &mut other, None);
        assert_eq!(world.checkpoint().unwrap(), reopened.checkpoint().unwrap());
        assert_eq!(world.last_tick(), reopened.last_tick());
        let events = &world.last_tick().unwrap().events;
        food_ticks += events
            .iter()
            .filter(|e| matches!(e, Event::FoodDrink(FoodDrinkEvent::Health { .. })))
            .count();
        emotes += events
            .iter()
            .filter(|e| matches!(e, Event::FoodDrink(FoodDrinkEvent::EatEmote)))
            .count();
        if world.state.tick == 101 {
            let health = events
                .iter()
                .position(|e| matches!(e, Event::FoodDrink(FoodDrinkEvent::Health { .. })))
                .unwrap();
            let removed = events
                .iter()
                .position(|e| matches!(e, Event::FoodDrink(FoodDrinkEvent::Removed { spell: 433 })))
                .unwrap();
            assert!(health < removed);
        }
    }
    assert_eq!(food_ticks, 2);
    assert_eq!(emotes, 3);
    assert!(world.state.food_drink().unwrap().holders.is_empty());
}
