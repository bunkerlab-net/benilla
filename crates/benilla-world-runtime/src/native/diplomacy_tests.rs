// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored source-shaped fixed-passive cases, never a real DBC/profile admission.
use super::{synthetic_reputation_fixture as fixture, *};
use benilla_sim::{player_death as death, reputation as rep, starter_init as init};

fn world(profile: Profile) -> NativeWorld {
    NativeWorld::synthetic_component_test(profile, [1; 16], synthetic_fixture::player(), 42)
        .unwrap()
}
fn step(w: &mut NativeWorld, a: &mut impl Authority, command: Command) {
    let input = synthetic_quest_fixture::command(w, command);
    w.step(w.state().tick + 1, Some(input), a).unwrap();
}
fn quest(w: &mut NativeWorld, a: &mut impl Authority, id: u32, reward: bool) {
    let giver = w.state().npcs[&3].id;
    step(
        w,
        a,
        if reward {
            Command::TurnInQuest { quest: id, giver }
        } else {
            Command::AcceptQuest { quest: id, giver }
        },
    );
}
fn reopen(w: &NativeWorld) -> NativeWorld {
    let restored = NativeWorld::restore_synthetic_component_test(
        w.profile().clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(w.checkpoint().unwrap(), restored.checkpoint().unwrap());
    restored
}
fn current(w: &NativeWorld, faction: u32) -> i32 {
    w.state()
        .quest_progression
        .as_ref()
        .unwrap()
        .reputation(w.profile().quest_progression.as_ref().unwrap(), faction)
        .unwrap()
}
fn resign(mut checkpoint: Checkpoint, state: serde_json::Value) -> Checkpoint {
    checkpoint.state = serde_json::from_value(state).unwrap();
    checkpoint.digest = crate::digest(b"benilla-native-state-v52\0", &checkpoint.state);
    checkpoint.checksum = crate::digest(
        b"benilla-native-checkpoint-v52\0",
        &(&checkpoint.state, checkpoint.digest, checkpoint.history_tip),
    );
    checkpoint
}

#[test]
fn retained_modifier_is_exactly_bound_to_initializer_combat_and_raw_source() {
    for field in [
        "missing_catalog",
        "dropped_list",
        "amount",
        "effect",
        "spell",
        "misc",
        "fingerprint",
        "attribute",
        "raw_amount",
        "raw_provenance",
        "combat_amount",
    ] {
        let (mut p, mut input, mut data) = fixture::setup_with_modifier(10);
        let before = p.clone();
        if field == "missing_catalog" {
            data.reputation = None;
        } else if field.starts_with("raw_") {
            let raw = input
                .spells
                .iter_mut()
                .find(|s| s.id == fixture::MODIFIER_SPELL)
                .unwrap();
            if field == "raw_amount" {
                raw.words[76] += 1;
            } else {
                raw.words[150] ^= 1;
            }
            p.initialize_player(&input, synthetic_fixture::float(100.0), 1)
                .unwrap();
        } else if field == "combat_amount" {
            p.combat
                .as_mut()
                .unwrap()
                .initial_auras
                .iter_mut()
                .find(|a| a.aura_type == 156)
                .unwrap()
                .amount += 1;
        } else {
            let mut value = serde_json::to_value(data.reputation.take().unwrap()).unwrap();
            let aura = &mut value["general_reputation_auras"][0];
            match field {
                "dropped_list" => {
                    value["general_reputation_auras"] = serde_json::json!([]);
                    value["audit"]["general_reputation_auras"] = serde_json::json!("Absent");
                }
                "amount" => aura["amount"] = serde_json::json!(11),
                "effect" => aura["effect_index"] = serde_json::json!(1),
                "spell" => aura["spell_id"] = serde_json::json!(900_000_222),
                "misc" => aura["misc_value"] = serde_json::json!(1),
                "fingerprint" => {
                    aura["source_spell_hash"][0] =
                        serde_json::json!(aura["source_spell_hash"][0].as_u64().unwrap() ^ 1)
                }
                "attribute" => aura["attributes"] = serde_json::json!(0x40),
                _ => unreachable!(),
            }
            data.reputation = Some(serde_json::from_value(value).unwrap());
        }
        let before_attempt = p.clone();
        assert!(
            p.initialize_quest_progression(&input, data).is_err(),
            "{field}"
        );
        assert_eq!(p, before_attempt, "transactional refusal: {field}");
        assert!(before.quest_progression.is_none());
    }
}

#[test]
fn retained_passive_does_not_authorize_an_unbound_later_reputation_consumer() {
    let (mut profile, input, data) = fixture::setup_with_modifier(10);
    let (_, _, mut no_reputation) = synthetic_quest_fixture::setup();
    no_reputation.reputation = None;
    profile
        .initialize_quest_progression(&input, no_reputation)
        .unwrap();
    let mut absent = data.clone();
    absent.reputation = Some(fixture::catalog());
    assert!(profile
        .initialize_quest_progression(&input, absent)
        .is_err());
    profile.initialize_quest_progression(&input, data).unwrap();
}

#[test]
fn authored_seventy_five_and_two_fifty_apply_bonus_once_before_quarter_spillover() {
    let (mut profile, input, mut data) = fixture::setup_with_modifier(10);
    data.quests[0].excluded.reward_rep_faction = [fixture::PRIMARY, 0, 0, 0, 0];
    data.quests[0].excluded.reward_rep_value = [75, 0, 0, 0, 0];
    data.quests
        .iter_mut()
        .find(|q| q.id == fixture::LOSS)
        .unwrap()
        .excluded
        .reward_rep_value = [250, 0, 0, 0, 0];
    let mut catalog = serde_json::to_value(data.reputation.take().unwrap()).unwrap();
    catalog["rows"][fixture::PRIMARY.to_string()]["Present"]["spillover"]["Present"][0]["rate"] =
        serde_json::json!(0.25f32.to_bits());
    data.reputation = Some(serde_json::from_value(catalog).unwrap());
    profile.initialize_quest_progression(&input, data).unwrap();
    let mut w = world(profile);
    let mut a = synthetic_quest_fixture::QuestAuthority::default();
    for _ in 0..3 {
        w = reopen(&w);
    }
    quest(&mut w, &mut a, fixture::GRANT, false);
    quest(&mut w, &mut a, fixture::GRANT, true);
    assert_eq!(current(&w, fixture::PRIMARY), -100 + 82);
    assert_eq!(current(&w, fixture::SPILLOVER), 20);
    assert_eq!(w.player_level(), 2);
    for _ in 0..3 {
        w = reopen(&w);
    }
    quest(&mut w, &mut a, fixture::LOSS, false);
    quest(&mut w, &mut a, fixture::LOSS, true);
    assert_eq!(current(&w, fixture::PRIMARY), -100 + 82 + 275);
    assert_eq!(current(&w, fixture::SPILLOVER), 20 + 68);
    let bound = w
        .profile()
        .quest_progression
        .as_ref()
        .unwrap()
        .reputation_catalog()
        .unwrap();
    assert_eq!(bound.general_reputation_auras().len(), 1);
    assert_eq!(bound.general_reputation_auras()[0].amount, 10);
    reopen(&w);
}

#[test]
fn zero_gain_still_calls_reputation_callbacks_and_slot_flags_still_gate_spillover() {
    for modifier in [-100, 10] {
        let (mut profile, input, mut data) = fixture::setup_with_modifier(modifier);
        data.quests[0].excluded.reward_rep_faction = [fixture::PRIMARY, 0, 0, 0, 0];
        data.quests[0].excluded.reward_rep_value = [75, 0, 0, 0, 0];
        data.quests[0].excluded.reward_faction_flags = 1;
        profile.initialize_quest_progression(&input, data).unwrap();
        let mut w = world(profile);
        let mut a = synthetic_quest_fixture::QuestAuthority::default();
        quest(&mut w, &mut a, fixture::GRANT, false);
        quest(&mut w, &mut a, fixture::GRANT, true);
        let gain = if modifier == 10 { 82 } else { 0 };
        assert_eq!(current(&w, fixture::PRIMARY), -100 + gain);
        assert_eq!(current(&w, fixture::SPILLOVER), 0);
        let changes: Vec<_> = w
            .last_tick()
            .unwrap()
            .events
            .iter()
            .filter_map(|e| match e {
                Event::QuestProgression(QuestProgressionEvent::Reputation(e)) => match &**e {
                    rep::Event::ReputationChanged { faction, .. } => Some(*faction),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        assert_eq!(changes, vec![fixture::PRIMARY]);
        reopen(&w);
    }
}

#[test]
fn old_quest_policy_and_nested_identity_tampering_cannot_be_reblessed() {
    assert_eq!(NATIVE_FORMAT_VERSION, 52);
    assert_eq!(
        init::STARTER_INIT_POLICY_ID,
        "cmangos-8ec338a1-level1-mage-init-softfloat-v4"
    );
    for modifier in [false, true] {
        let w = if modifier {
            fixture::world_with_modifier(10)
        } else {
            fixture::world()
        };
        for field in ["quest_policy", "genesis", "catalog", "training"] {
            let checkpoint = w.checkpoint().unwrap();
            let mut state = serde_json::to_value(&checkpoint.state).unwrap();
            match field {
                "quest_policy" => {
                    state["quest_progression"]["policy"] = serde_json::json!(
                        "benilla-native-ordinary-quest-reputation-reconstruction-v4"
                    )
                }
                "genesis" => {
                    state["quest_progression"]["genesis"][0] = serde_json::json!(
                        state["quest_progression"]["genesis"][0].as_u64().unwrap() ^ 1
                    )
                }
                "catalog" => {
                    state["quest_progression"]["reputation"]["state"]["catalog_hash"][0] = serde_json::json!(
                        state["quest_progression"]["reputation"]["state"]["catalog_hash"][0]
                            .as_u64()
                            .unwrap()
                            ^ 1
                    )
                }
                "training" => {
                    state["quest_progression"]["training"]["identity"][0] = serde_json::json!(
                        state["quest_progression"]["training"]["identity"][0]
                            .as_u64()
                            .unwrap()
                            ^ 1
                    )
                }
                _ => unreachable!(),
            }
            assert!(
                NativeWorld::restore_synthetic_component_test(
                    w.profile().clone(),
                    resign(checkpoint, state)
                )
                .is_err(),
                "{modifier}:{field}"
            );
        }
    }
}

#[test]
fn passive_survives_death_and_corpse_reopen_once_without_opening_spirit_release() {
    let (mut profile, input, data) = fixture::setup_with_modifier(10);
    // Give this authored combat target enough health to survive the selected spell.
    profile.npcs[0].max_health = 3000;
    profile.npcs[0].combat.as_mut().unwrap().minimum_damage = synthetic_fixture::float(10000.0);
    profile.npcs[0].combat.as_mut().unwrap().maximum_damage = synthetic_fixture::float(10000.0);
    profile
        .configure_player_death(
            death::DeathConfig {
                map_instanceable: false,
                pve_reclaim_delay_enabled: true,
            },
            death::DeathSupport::checked(&death::REQUIRED_ABSENCES.map(|d| (d, Some(false))))
                .unwrap(),
        )
        .unwrap();
    profile.initialize_quest_progression(&input, data).unwrap();
    let mut w = world(profile);
    let mut a = synthetic_quest_fixture::QuestAuthority::default();
    quest(&mut w, &mut a, fixture::GRANT, false);
    quest(&mut w, &mut a, fixture::GRANT, true);
    let catalog = w
        .profile()
        .quest_progression
        .as_ref()
        .unwrap()
        .reputation_catalog()
        .unwrap()
        .clone();
    let target = w.state().npcs[&2].id;
    step(
        &mut w,
        &mut a,
        Command::Cast {
            ability: 60001,
            target,
        },
    );
    for _ in 0..100 {
        if w.state().health == 0 {
            break;
        }
        w.step(w.state().tick + 1, None, &mut a).unwrap();
    }
    assert_eq!(w.state().health, 0);
    for _ in 0..3 {
        w = reopen(&w);
        assert_eq!(current(&w, fixture::SPILLOVER), 240);
        assert_eq!(
            w.profile()
                .quest_progression
                .as_ref()
                .unwrap()
                .reputation_catalog(),
            Some(&catalog)
        );
        assert_eq!(
            w.profile()
                .combat
                .as_ref()
                .unwrap()
                .initial_auras
                .iter()
                .filter(|a| a.aura_type == 156)
                .count(),
            1
        );
    }
    step(&mut w, &mut a, Command::ReleaseSpirit);
    assert!(w.last_tick().unwrap().events.iter().any(|e| matches!(
        e,
        Event::Rejected(Rejection::SpiritReleaseUnavailable { .. })
    )));
    reopen(&w);
}

#[test]
fn prior_initializer_profile_identity_rejects_without_changing_native33_shape() {
    let profile = synthetic_fixture::initialized_profile();
    assert!(profile.quest_progression.is_none());
    assert!(profile
        .combat
        .as_ref()
        .unwrap()
        .initial_auras
        .iter()
        .all(|a| a.aura_type != 156));
    // Exactly the prior profile digest contract for this unchanged no-aura input.
    let legacy = crate::digest(
        b"benilla-native-profile-v52\0",
        &(
            NATIVE_POLICY,
            NPC_SPELL_POLICY,
            ADMIN_POLICY,
            benilla_sim::player_slow::PLAYER_SLOW_POLICY,
            RNG_POLICY,
            spell::SPELL_EXECUTION_POLICY_ID,
            ai::CREATURE_AI_POLICY_ID,
            benilla_sim::creature_motion::CREATURE_MOTION_POLICY_ID,
            benilla_sim::creature_regeneration::CREATURE_REGENERATION_POLICY_ID,
            mana::MAGE_RESOURCE_POLICY_ID,
            "cmangos-8ec338a1-level1-mage-init-softfloat-v2",
            benilla_sim::npc_melee::NPC_MELEE_POLICY_ID,
            equipment::EQUIPMENT_POLICY,
            death::PLAYER_DEATH_POLICY,
            &profile,
        ),
    );
    assert_ne!(legacy, profile.digest());
    let w = world(profile);
    let checkpoint = w.checkpoint().unwrap();
    let mut state = serde_json::to_value(&checkpoint.state).unwrap();
    state["profile"] = serde_json::json!(legacy);
    assert_eq!(state["version"], 52);
    assert!(NativeWorld::restore_synthetic_component_test(
        w.profile().clone(),
        resign(checkpoint, state)
    )
    .is_err());
}

#[test]
fn retained_general_modifier_does_not_open_native_on_kill_or_spell_reputation() {
    use benilla_sim::player_progression::kill_xp::ReputationAdmission;
    for admission in [
        ReputationAdmission::Unknown,
        ReputationAdmission::PresentOnKillRow,
    ] {
        let (mut profile, input, mut data) = fixture::setup_with_modifier(10);
        data.kill.creatures[0].reputation = admission;
        assert!(profile.initialize_quest_progression(&input, data).is_err());
    }
    let (mut profile, mut initial, mut source) = synthetic_spell_fixture::inputs();
    source.spells[0].words[61] = 103;
    // Source mismatch is rejected before morphology. Align both raw closures to
    // independently reach the unchanged executor's effect103 boundary.
    assert!(matches!(
        profile.initialize_spell_authority(&initial, source.clone()),
        Err(NativeError::InvalidProfile)
    ));
    initial.spells[0].words[61] = 103;
    profile
        .initialize_player(&initial, synthetic_fixture::float(100.0), 1)
        .unwrap();
    assert!(matches!(
        profile.initialize_spell_authority(&initial, source),
        Err(NativeError::UnsupportedMechanic(
            "raw spell effect morphology"
        ))
    ));
    assert_eq!(std::mem::size_of::<NativeCapability>(), 0);
}

#[test]
fn exact_legacy_no_aura_catalog_hash_fails_inside_current_profile_and_genesis() {
    // Base4a8's Catalog::hash v1 over the unchanged authored fixture rows/rates.
    // Independently reproduced from the fixed-width prior hash stream.
    let legacy = [
        91, 108, 36, 221, 98, 145, 62, 96, 69, 187, 162, 83, 69, 133, 229, 167, 56, 151, 38, 190,
        58, 130, 182, 236, 137, 254, 82, 34, 103, 240, 207, 104,
    ];
    let w = fixture::world();
    let catalog = w
        .profile()
        .quest_progression
        .as_ref()
        .unwrap()
        .reputation_catalog()
        .unwrap();
    assert!(catalog.general_reputation_auras().is_empty());
    assert_ne!(catalog.hash(), legacy);
    let checkpoint = w.checkpoint().unwrap();
    let mut state = serde_json::to_value(&checkpoint.state).unwrap();
    state["quest_progression"]["reputation"]["state"]["catalog_hash"] = serde_json::json!(legacy);
    assert!(NativeWorld::restore_synthetic_component_test(
        w.profile().clone(),
        resign(checkpoint, state)
    )
    .is_err());
}
