// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual animation sender and controller descriptor/durable-state seam, not rendered poses.
use super::*;
fn sender(state: u8) -> ClientCommand {
    let (tx, rx) = crossbeam_channel::unbounded();
    crate::creature_anim::send_sheath_state(&tx, state);
    let command = rx.try_recv().unwrap();
    assert!(matches!(command,ClientCommand::SetSheathed {state:raw} if raw==u32::from(state)));
    assert!(rx.try_recv().is_err());
    command
}
fn assert_field(events: &[SessionEvent], player: u64, state: u8) {
    let fields = values(events, player);
    assert_eq!(fields.unit_sheath_state(), Some(state));
    assert_eq!(
        fields.raw_fields().find(|(index, _)| *index == 164),
        Some((164, 0x2800 | u32::from(state)))
    );
}
fn no_item_or_spell_callbacks(events: &[SessionEvent]) {
    assert!(!events.iter().any(|event| matches!(
        event,
        SessionEvent::ItemCreate { .. }
            | SessionEvent::ObjectDestroyed(_)
            | SessionEvent::ItemPushResult(_)
            | SessionEvent::SpellStart { .. }
            | SessionEvent::SpellGo { .. }
    )));
}
#[test]
fn actual_sheath_sender_waits_for_checkpoint_then_reopens_exact_packed_siblings() {
    let h = Harness::new();
    let mut c = enter_staff(&h, false);
    assert!(c.world.profile().supports_player_sheath());
    let initial = c.initial_events().unwrap();
    assert_field(&initial, c.player_guid(), 0);
    let equipment = c.world.state().combat.as_ref().unwrap().equipment.clone();
    let stats = c.world.state().combat.as_ref().unwrap().stats.clone();
    let items = inventory(&c).clone();
    c.receive(sender(1)).unwrap();
    inject(&h);
    let failed = c.pump(c.tick_duration());
    paused(&failed);
    assert!(failed.events.is_empty());
    assert_eq!(c.world.state().sheath_state(), 1);
    assert_eq!(
        c.world.state().combat.as_ref().unwrap().equipment,
        equipment
    );
    assert_eq!(c.world.state().combat.as_ref().unwrap().stats, stats);
    assert_eq!(inventory(&c), &items);
    assert!(c.world.state().casts.is_empty());
    let digest = c.world.state_digest();
    repair(&h);
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    assert_field(&out.events, c.player_guid(), 1);
    no_item_or_spell_callbacks(&out.events);
    assert!(c.pump(Duration::ZERO).events.is_empty());
    drop(c);
    let mut c = enter_staff(&h, true);
    assert_eq!(c.world.state_digest(), digest);
    let initial = c.initial_events().unwrap();
    assert_field(&initial, c.player_guid(), 1);
    for state in [2, 0, 0] {
        let previous = c.world.state().sheath_state();
        let ledger = serde_json::to_value(c.world.checkpoint().unwrap()).unwrap()["state"]
            ["player_sheath"]
            .clone();
        assert!(ledger.is_object());
        c.receive(sender(state)).unwrap();
        let out = step(&mut c);
        if state == previous {
            assert_eq!(
                serde_json::to_value(c.world.checkpoint().unwrap()).unwrap()["state"]
                    ["player_sheath"],
                ledger
            );
        }
        assert_field(&out.events, c.player_guid(), state);
        no_item_or_spell_callbacks(&out.events);
        assert_eq!(c.world.state().sheath_state(), state);
        assert_eq!(inventory(&c), &items);
        assert_eq!(
            c.world.state().combat.as_ref().unwrap().equipment,
            equipment
        );
        assert_eq!(c.world.state().combat.as_ref().unwrap().stats, stats);
        assert!(c.world.state().casts.is_empty());
    }
    for state in [3, 255, 256, u32::MAX] {
        let ledger = serde_json::to_value(c.world.checkpoint().unwrap()).unwrap()["state"]
            ["player_sheath"]
            .clone();
        c.receive(ClientCommand::SetSheathed { state }).unwrap();
        let out = step(&mut c);
        assert_eq!(
            serde_json::to_value(c.world.checkpoint().unwrap()).unwrap()["state"]["player_sheath"],
            ledger
        );
        assert_field(&out.events, c.player_guid(), 0);
        no_item_or_spell_callbacks(&out.events);
        assert_eq!(c.world.state().sheath_state(), 0);
        assert_eq!(inventory(&c), &items);
        assert_eq!(
            c.world.state().combat.as_ref().unwrap().equipment,
            equipment
        );
        assert_eq!(c.world.state().combat.as_ref().unwrap().stats, stats);
        assert!(c.world.state().casts.is_empty());
    }
    c.shutdown().unwrap();
    let checkpoint = c.world.checkpoint().unwrap();
    drop(c);
    assert_eq!(
        enter_staff(&h, true).world.checkpoint().unwrap(),
        checkpoint
    );
}
#[test]
fn sheath_commands_accept_detached_weapon_actual_combat_and_actual_death() {
    for lethal in [false, true] {
        let h = Harness::new();
        let mut c = NativeController::enter_component_test(
            &h.root,
            &h.profile,
            &h.character,
            Entry::Create {
                world: [8; 16],
                seed: 7,
            },
            if lethal {
                armor::lethal_weapon_profile()
            } else {
                armor::weapon_profile()
            },
            &StaffCatalog,
            armor::EquipmentAuthority::default(),
        )
        .unwrap();
        c.initial_events().unwrap();
        c.receive(ClientCommand::SwapInvItem {
            src_slot: 15,
            dst_slot: 23,
        })
        .unwrap();
        step(&mut c);
        c.receive(sender(2)).unwrap();
        let out = step(&mut c);
        assert_field(&out.events, c.player_guid(), 2);
        assert!(c
            .world
            .state()
            .combat
            .as_ref()
            .unwrap()
            .equipment
            .item(15)
            .is_none());
        let target = c.projection.npc_guid(c.world.state().npcs[&2].id).unwrap();
        c.receive(ClientCommand::CastSpell {
            spell_id: 133,
            target: Some(target),
        })
        .unwrap();
        for _ in 0..150 {
            step(&mut c);
            if if lethal {
                !c.world.state().player_alive()
            } else {
                c.world.state().combat.as_ref().unwrap().in_combat
            } {
                break;
            }
        }
        if lethal {
            assert!(!c.world.state().player_alive());
        } else {
            assert!(c.world.state().combat.as_ref().unwrap().in_combat);
        }
        let items = inventory(&c).clone();
        c.receive(sender(1)).unwrap();
        let out = step(&mut c);
        assert_field(&out.events, c.player_guid(), 1);
        assert_eq!(c.world.state().sheath_state(), 1);
        assert_eq!(inventory(&c), &items);
        assert!(c
            .world
            .state()
            .combat
            .as_ref()
            .unwrap()
            .equipment
            .item(15)
            .is_none());
        if lethal {
            assert_eq!(c.world.state().health, 0);
            assert!(!c.world.state().player_alive());
        }
        c.shutdown().unwrap();
    }
}
