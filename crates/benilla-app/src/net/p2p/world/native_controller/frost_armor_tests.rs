// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored self cast, melee proc, projection, expiration and durable reopen.
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
        n::synthetic_frost_armor_fixture::SpatialOnly::default(),
    )
    .unwrap()
}
#[test]
fn self_cast_proc_and_mid_aura_reopen_keep_source_modifiers_and_expire() {
    let h = Harness::new();
    let profile = n::synthetic_frost_armor_fixture::profile();
    let mut c = enter(&h, profile.clone(), false);
    c.initial_events().unwrap();
    c.receive(ClientCommand::CastSpell {
        spell_id: 168,
        target: Some(c.projection.player_guid()),
    })
    .unwrap();
    let events = pump_to(&mut c, 1);
    assert!(events.iter().any(|e| matches!(
        e,
        SessionEvent::CastResult {
            spell_id: 168,
            success: true,
            ..
        }
    )));
    assert!(events.iter().any(|e|matches!(e,SessionEvent::SpellGo{spell_id:168,hits,..} if hits==&vec![c.projection.player_guid()])));
    assert!(events.iter().any(|e| matches!(
        e,
        SessionEvent::AuraDuration {
            slot: 0,
            remaining_ms: 3900
        }
    )));
    let mut saw_proc = false;
    for tick in 2..40 {
        let events = pump_to(&mut c, tick);
        saw_proc |= events
            .iter()
            .any(|e| matches!(e, SessionEvent::SpellGo { spell_id: 6136, .. }));
        if c.world
            .state()
            .npcs
            .values()
            .any(|npc| npc.chilled.is_some())
        {
            break;
        }
    }
    assert!(saw_proc);
    assert!(c
        .world
        .state()
        .npcs
        .values()
        .any(|npc| npc.chilled.is_some()));
    let checkpoint = c.world.checkpoint().unwrap();
    let mut uninterrupted = c.world.clone();
    drop(c);
    let mut c = enter(&h, profile, true);
    assert_eq!(c.world.checkpoint().unwrap(), checkpoint);
    let initial = c.initial_events().unwrap();
    assert!(initial
        .iter()
        .any(|e| matches!(e,SessionEvent::AuraDuration{slot:0,remaining_ms} if *remaining_ms>0)));
    assert!(!initial.iter().any(|e| matches!(
        e,
        SessionEvent::SpellGo {
            spell_id: 168 | 6136,
            ..
        }
    )));
    let catalog = LoopCatalog::new(uninterrupted.profile());
    let mut projection =
        NativeProjection::new(&uninterrupted, uninterrupted.state().player, &catalog).unwrap();
    let mut orders =
        NativeMovementOrders::from_session_next_counter(c.orders.counter.next_counter());
    let mut authority = n::synthetic_frost_armor_fixture::SpatialOnly::default();
    while c.tick() < 55 {
        let tick = c.tick() + 1;
        let published = pump_to(&mut c, tick);
        uninterrupted.step(tick, None, &mut authority).unwrap();
        assert_eq!(
            c.world.checkpoint().unwrap(),
            uninterrupted.checkpoint().unwrap()
        );
        // A durable receipt clears the controller's journal, not its output.
        // Compare the complete publication against uninterrupted projection.
        let expected = projection
            .advance_with_movement_orders(&uninterrupted, &mut orders)
            .unwrap();
        assert_eq!(format!("{published:?}"), format!("{expected:?}"));
    }
    assert!(c.world.state().frost_armor().is_none());
    assert!(c
        .world
        .state()
        .npcs
        .values()
        .all(|npc| npc.chilled.is_none()));
}
#[test]
fn self_cast_cooldown_refusal_is_one_acknowledgement_without_another_go() {
    let h = Harness::new();
    let mut c = enter(&h, n::synthetic_frost_armor_fixture::profile(), false);
    c.initial_events().unwrap();
    c.receive(ClientCommand::CastSpell {
        spell_id: 168,
        target: None,
    })
    .unwrap();
    pump_to(&mut c, 1);
    let mana = c.world.state().resources.caster.mana;
    c.receive(ClientCommand::CastSpell {
        spell_id: 168,
        target: None,
    })
    .unwrap();
    let events = pump_to(&mut c, 2);
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(
                e,
                SessionEvent::CastResult {
                    spell_id: 168,
                    success: false,
                    reason: Some(0x3c),
                    ..
                }
            ))
            .count(),
        1
    );
    assert!(!events
        .iter()
        .any(|e| matches!(e, SessionEvent::SpellGo { spell_id: 168, .. })));
    assert_eq!(c.world.state().resources.caster.mana, mana);
}
