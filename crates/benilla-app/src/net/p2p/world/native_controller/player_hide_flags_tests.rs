// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual Lua queue -> app sender -> controller, not a full Bevy/render execution.
use super::*;
use benilla_ui::script::UiScript;
use n::{
    synthetic_player_appearance_fixture as appearance, synthetic_source_spline_fixture as splines,
};
#[cfg(benilla_headless_geometry)]
mod sender {
    include!(concat!(env!("OUT_DIR"), "/worn_display_sender.rs"));
}
#[cfg(not(benilla_headless_geometry))]
use crate::ui_unit as sender;

fn profile(h: &Harness, fragile: bool) -> n::Profile {
    let mut data = if fragile {
        splines::profile()
    } else {
        splines::surviving_profile()
    };
    let a = h.character.appearance;
    data.initialize_player_appearance(
        n::PlayerAppearanceRequest {
            player: Selection::from_profile(&h.profile, &h.character)
                .unwrap()
                .player,
            race: h.character.race,
            class: h.character.class,
            gender: h.character.gender,
            choices: n::NativePlayerAppearanceChoices {
                skin: a.skin,
                face: a.face,
                hair_style: a.hair_style,
                hair_color: a.hair_color,
                facial_hair: a.facial_hair,
            },
        },
        &appearance::catalog(),
    )
    .unwrap();
    assert!(data.supports_player_hide_flags());
    data
}
fn enter(h: &Harness, data: n::Profile, open: bool) -> NativeController {
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
        data,
        &MeleeCatalog,
        MeleeAuthority::default(),
    )
    .unwrap()
}
fn lua_flips(c: &mut NativeController, ui: &mut UiScript, source: &str) -> usize {
    ui.run(source).unwrap();
    let queued = ui.take_worn_display_toggles();
    let count = queued.len();
    let (tx, rx) = crossbeam_channel::unbounded();
    for which in queued {
        sender::send_worn_display_toggle(which, &tx);
    }
    drop(tx);
    for command in rx {
        assert!(c.receive(command).unwrap().events.is_empty());
    }
    count
}
fn descriptor(events: &[SessionEvent], guid: u64, hide: u32) -> benilla_protocol::ObjectFields {
    let mut fields = benilla_protocol::ObjectFields::default();
    for event in events {
        if let SessionEvent::ObjectValues {
            guid: target,
            fields: delta,
        } = event
        {
            if *target == guid {
                fields.merge(delta.clone());
            }
        }
    }
    assert!(fields.raw_fields().any(|(field, _)| field == 190));
    assert_eq!(fields.player_flags() & 0xc00, hide);
    assert_eq!(fields.player_hides_helm(), hide & 0x400 != 0);
    assert_eq!(fields.player_hides_cloak(), hide & 0x800 != 0);
    assert!(!fields.player_is_ghost());
    fields
}
#[test]
fn lua_hide_flags_withhold_then_retry_reopen_and_preserve_two_flips() {
    let h = Harness::new();
    let data = profile(&h, false);
    let mut c = enter(&h, data.clone(), false);
    let initial = c.initial_events().unwrap();
    let initial_flags = descriptor(&initial, c.player_guid(), 0).player_flags();
    let mut ui = UiScript::new().unwrap();
    assert_eq!(lua_flips(&mut c, &mut ui, "ShowHelm(0); ShowCloak(0)"), 2);
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored recoverable fault",
    )
    .unwrap();
    let withheld = c.pump(c.tick_duration());
    assert!(withheld.events.is_empty());
    assert!(matches!(
        withheld.pause.unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: false
        }
    ));
    assert_eq!(c.world.state().player_hide_flags(), 0xc00);
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    assert!(out.pause.is_none());
    let fields = descriptor(&out.events, c.player_guid(), 0xc00);
    assert_eq!(fields.player_flags() & !0xc00, initial_flags & !0xc00);
    ui.set_worn_display(!fields.player_hides_helm(), !fields.player_hides_cloak());
    assert!(ui
        .eval::<bool>("return ShowingHelm() == nil and ShowingCloak() == nil")
        .unwrap());
    assert!(c.pump(Duration::ZERO).events.is_empty());
    let before = c.world.state().next_sequence;
    assert_eq!(lua_flips(&mut c, &mut ui, "ShowHelm(1); ShowHelm(0)"), 2);
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none(), "{:?}", out.pause);
    assert_eq!(c.world.state().next_sequence, before + 2);
    assert_eq!(c.world.state().player_hide_flags(), 0xc00);
    descriptor(&out.events, c.player_guid(), 0xc00);
    let checkpoint = c.world.checkpoint().unwrap();
    drop(c);
    let mut reopened = enter(&h, data, true);
    assert_eq!(reopened.world.checkpoint().unwrap(), checkpoint);
    let initial = reopened.initial_events().unwrap();
    descriptor(&initial, reopened.player_guid(), 0xc00);
}
#[test]
fn actual_dead_player_can_toggle_both_hide_flags_without_resurrection() {
    let h = Harness::new();
    let data = profile(&h, true);
    let mut c = enter(&h, data.clone(), false);
    c.receive(ClientCommand::AttackSwing { guid: guid(&c, 2) })
        .unwrap();
    for _ in 0..300 {
        if !c.world.state().player_alive() {
            break;
        }
        let out = c.pump(c.tick_duration());
        assert!(out.pause.is_none(), "{:?}", out.pause);
    }
    assert!(
        !c.world.state().player_alive(),
        "actual NPC must kill source player"
    );
    let mut ui = UiScript::new().unwrap();
    assert_eq!(lua_flips(&mut c, &mut ui, "ShowHelm(0); ShowCloak(0)"), 2);
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none(), "{:?}", out.pause);
    assert_eq!(c.world.state().health, 0);
    assert!(!c.world.state().player_alive());
    assert_eq!(c.world.state().player_hide_flags(), 0xc00);
    descriptor(&out.events, c.player_guid(), 0xc00);
    let checkpoint = c.world.checkpoint().unwrap();
    drop(c);
    let mut reopened = enter(&h, data, true);
    assert_eq!(reopened.world.checkpoint().unwrap(), checkpoint);
    assert!(!reopened.world.state().player_alive());
    let initial = reopened.initial_events().unwrap();
    descriptor(&initial, reopened.player_guid(), 0xc00);
}
