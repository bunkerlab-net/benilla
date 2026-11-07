// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn profile(kill_rate_bits: u32, aggro_rate_bits: u32) -> n::Profile {
    let (mut profile, input, mut rows) = loop_fixture::setup();
    rows.world_rules.start_player_money = 123;
    rows.world_rules.quest_xp_rate_bits = 2f32.to_bits();
    rows.world_rules.kill.kill_rate_bits = kill_rate_bits;
    rows.kill.rules = rows.world_rules.kill;
    rows.world_rules.loot.money_rate_bits = 2f32.to_bits();
    rows.loot.as_mut().unwrap().rules = rows.world_rules.loot.clone();
    rows.world_rules.combat.aggro_rate_bits = aggro_rate_bits;
    for npc in &mut profile.npcs {
        npc.ai.aggro_rate = benilla_sim::creature_ai::Real::from_bits(aggro_rate_bits).unwrap();
    }
    profile.source_motion.as_mut().unwrap().aggro_rate =
        benilla_sim::creature_ai::Real::from_bits(aggro_rate_bits).unwrap();
    profile.initialize_quest_progression(&input, rows).unwrap();
    profile
}

#[test]
fn explicit_progression_policy_survives_controller_rewards_and_reopen() {
    let h = Harness::new();
    let data = profile(0.5f32.to_bits(), 0.5f32.to_bits());
    let controls = Controls::default();
    let mut c = enter(&h, data.clone(), false, controls.clone());
    c.initial_events().unwrap();
    assert_eq!(progress(&c).money, 123);
    c = reopen(&h, c, &data, &controls);
    accept(&mut c, loop_fixture::GIVER);
    let events = kill(&mut c, loop_fixture::HOSTILE);
    assert!(events
        .iter()
        .any(|event| matches!(event, SessionEvent::XpGain(xp) if xp.kill && xp.total == 25)));
    c = reopen(&h, c, &data, &controls);
    let opened = open_loot(&mut c, loop_fixture::HOSTILE);
    assert!(opened
        .events
        .iter()
        .any(|event| matches!(event, SessionEvent::LootResponse { gold: 6, .. })));
    collect(&mut c);
    choose_reward(&mut c, loop_fixture::GIVER);
    let rewarded = step(&mut c);
    assert!(rewarded
        .events
        .iter()
        .any(|event| matches!(event, SessionEvent::XpGain(xp) if !xp.kill && xp.total == 200)));
    assert_eq!(progress(&c).money, 143);
    c = reopen(&h, c, &data, &controls);
    no_transient_progress(&step(&mut c).events);
    let digest = c.world.profile_digest();
    drop(c);
    let changed = profile(1f32.to_bits(), 0.5f32.to_bits());
    assert_ne!(changed.digest(), digest);
    let catalog = LoopCatalog::new(&changed);
    assert!(NativeController::enter_component_test(
        &h.root,
        &h.profile,
        &h.character,
        Entry::Open { world: [8; 16] },
        changed,
        &catalog,
        ObservedAuthority {
            controls,
            inner: Default::default()
        },
    )
    .is_err());
}

#[test]
fn combat_world_policy_survives_controller_reopen_and_refuses_changed_aggro() {
    let h = Harness::new();
    let data = profile(1f32.to_bits(), 0.5f32.to_bits());
    let controls = Controls::default();
    let mut c = enter(&h, data.clone(), false, controls.clone());
    c.initial_events().unwrap();
    let c = reopen(&h, c, &data, &controls);
    assert_eq!(c.world.profile_digest(), data.digest());
    assert_eq!(
        data.quest_progression
            .as_ref()
            .unwrap()
            .combat_rules()
            .aggro_rate_bits,
        0.5f32.to_bits()
    );
    drop(c);
    let changed = profile(1f32.to_bits(), 1f32.to_bits());
    assert_ne!(data.digest(), changed.digest());
    let catalog = LoopCatalog::new(&changed);
    assert!(NativeController::enter_component_test(
        &h.root,
        &h.profile,
        &h.character,
        Entry::Open { world: [8; 16] },
        changed,
        &catalog,
        ObservedAuthority {
            controls,
            inner: Default::default()
        },
    )
    .is_err());
}
