// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored control transactions and source relation checks; no content admission.
use super::synthetic_reputation_fixture as fixture;
use super::*;
use benilla_sim::{classic_targets as relations, reputation as rep};

fn step(w: &mut NativeWorld, command: Command) {
    let input = synthetic_quest_fixture::command(w, command);
    w.step(
        w.state.tick + 1,
        Some(input),
        &mut synthetic_quest_fixture::QuestAuthority::default(),
    )
    .unwrap();
}
fn view(w: &NativeWorld) -> &rep::State {
    w.state
        .quest_progression
        .as_ref()
        .unwrap()
        .reputation
        .as_ref()
        .unwrap()
        .view()
}
fn reopen(w: &NativeWorld) {
    let restored = NativeWorld::restore_synthetic_component_test(
        w.profile.clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(restored.checkpoint().unwrap(), w.checkpoint().unwrap());
}
fn replay(w: &NativeWorld, initial: Checkpoint) {
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
}
fn control() -> InteractionControlSeed {
    InteractionControlSeed {
        unit_state: 0,
        owner_guid: 0,
        charmer_guid: 0,
        mount_display_id: 0,
        shapeshift_form: 0,
        aura_flags: 0,
    }
}
fn faction(
    template_id: u32,
    faction: u32,
    reputation_list_id: Option<i32>,
) -> InteractionFactionSource {
    InteractionFactionSource {
        template_id,
        faction,
        reputation_list_id,
        template_flags: 0,
        group_mask: 1,
        friend_group_mask: 1,
        enemy_group_mask: 0,
        enemies: [0; 4],
        friends: [0; 4],
    }
}
fn relation_world() -> NativeWorld {
    let (mut p, input, mut data) = fixture::setup();
    p.npcs[0].faction_template = 2;
    p.npcs[0].ai.spawn = synthetic_fixture::point(2.0);
    p.npcs[0].ai.reaction = ai::Reaction::Passive;
    data.npc_rows.push(QuestNpcRow {
        entry: p.npcs[0].entry,
        npc_flags: 2,
        unit_flags: 0,
    });
    p.initialize_quest_progression(&input, data).unwrap();
    let absent = InteractionEvidence::Absent;
    let rows = InteractionInput {
        player: PlayerInteractionSource {
            faction_template: 1,
            unit_flags: relations::unit_flags::PLAYER_CONTROLLED,
            player_flags: 0,
            control: control(),
        },
        npcs: p
            .npcs
            .iter()
            .map(|n| {
                let row = p.quest_progression.as_ref().unwrap().npc_rows.get(&n.entry);
                NpcInteractionSource {
                    entry: n.entry,
                    faction_template: n.faction_template,
                    npc_flags: row.map_or(0, |r| r.npc_flags),
                    unit_flags: row.map_or(0, |r| r.unit_flags),
                    creature_type_flags: 0,
                    control: control(),
                    spawn_flag_or_faction_overrides: absent,
                }
            })
            .collect(),
        factions: vec![faction(1, 0, None), faction(2, fixture::PRIMARY, Some(3))],
        audit: InteractionAudit {
            external_state_mutations: absent,
            controlled_or_summoned_units: absent,
            pvp_duel_or_sanctuary: absent,
            forced_reactions: absent,
            temporary_factions: absent,
            immunity_overrides: absent,
            npc_aura_or_script_hooks: absent,
        },
    };
    p.initialize_interactions(rows).unwrap();
    NativeWorld::synthetic_component_test(p, [1; 16], synthetic_fixture::player(), 42).unwrap()
}
#[derive(Default)]
struct Capture {
    base: synthetic_fixture::KernelAuthority,
    replies: Vec<AuthorityReply>,
}
impl Authority for Capture {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        assert!(
            !matches!(request.query, Query::QuestStatus { .. }),
            "source relation escaped to external authority"
        );
        self.base.resolve(request)
    }
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        self.replies.push(reply.clone());
        Ok(())
    }
}
fn reaction(w: &mut NativeWorld) -> u8 {
    let mut authority = Capture::default();
    let target = w.state.npcs[&2].id;
    w.preview_quest_status(target, &mut authority).unwrap();
    let Facts::QuestStatus {
        player_reaction,
        npc_reaction,
    } = authority.replies.last().unwrap().facts
    else {
        panic!("source relation reply")
    };
    assert_eq!(npc_reaction, rep::Rank::Unfriendly as u8);
    player_reaction
}
// This is the source relation subcheck using actual retained native reputation,
// NOT a claim that tracked-faction mainhand historical proofs are admitted.
fn attack_allowed(w: &NativeWorld) -> bool {
    let player = benilla_formats::FactionTemplate {
        faction: 0,
        group_mask: 1,
        friend_group_mask: 1,
        enemy_group_mask: 0,
        enemies: [0; 4],
        friends: [0; 4],
    };
    let npc = benilla_formats::FactionTemplate {
        faction: fixture::PRIMARY,
        ..player
    };
    let absent = relations::ContextState::Absent;
    let input = relations::SourcePlayerNpcInput {
        units: relations::PlayerNpcInput {
            player_faction: relations::FactionRef {
                template: Some(&player),
                template_flags: Some(0),
                reputation: relations::FactionReputation::NoFaction,
            },
            npc_faction: relations::FactionRef {
                template: Some(&npc),
                template_flags: Some(0),
                reputation: relations::FactionReputation::Tracked { index: 3 },
            },
            player: relations::UnitTargetState {
                unit_flags: relations::unit_flags::PLAYER_CONTROLLED,
                alive: w.state.player_alive(),
                mounted: false,
            },
            npc: relations::UnitTargetState {
                unit_flags: 0,
                alive: w.state.npcs[&2].health > 0,
                mounted: false,
            },
            context: relations::RelationContext {
                control: absent,
                pvp: absent,
                reputation_overrides: absent,
                immunity: absent,
                special_state: absent,
            },
        },
        reputation: Some(relations::ReputationRef {
            catalog: w
                .profile
                .quest_progression
                .as_ref()
                .unwrap()
                .reputation_catalog()
                .unwrap(),
            state: view(w),
        }),
    };
    matches!(
        relations::check_can_attack(&input, relations::Side::Player),
        relations::TargetDecision::Allowed
    )
}
#[test]
fn at_war_command_changes_current_source_relation_and_attack_subcheck() {
    let mut w = relation_world();
    let initial = w.checkpoint().unwrap();
    assert_eq!(reaction(&mut w), rep::Rank::Hostile as u8);
    assert!(attack_allowed(&w));
    step(
        &mut w,
        Command::SetFactionAtWar {
            list_id: 3,
            at_war: false,
        },
    );
    assert_eq!(reaction(&mut w), rep::Rank::Friendly as u8);
    assert!(!attack_allowed(&w));
    reopen(&w);
    step(
        &mut w,
        Command::SetFactionAtWar {
            list_id: 3,
            at_war: true,
        },
    );
    assert_eq!(reaction(&mut w), rep::Rank::Hostile as u8);
    assert!(attack_allowed(&w));
    reopen(&w);
    replay(&w, initial);
}

#[test]
fn inactive_requires_visibility_and_control_noops_preserve_source_state() {
    let mut w = fixture::world();
    let initial = w.checkpoint().unwrap();
    let before = view(&w).clone();
    for command in [
        Command::SetFactionAtWar {
            list_id: u32::MAX,
            at_war: true,
        },
        Command::SetFactionInactive {
            list_id: u32::MAX,
            inactive: true,
        },
        Command::SetFactionInactive {
            list_id: 7,
            inactive: true,
        },
    ] {
        step(&mut w, command);
        assert_eq!(view(&w), &before);
        reopen(&w);
    }
    let giver = w.state.npcs[&3].id;
    step(
        &mut w,
        Command::AcceptQuest {
            quest: fixture::OBJECTIVE,
            giver,
        },
    );
    assert_ne!(view(&w).factions()[&7].flags & rep::VISIBLE, 0);
    step(
        &mut w,
        Command::SetFactionInactive {
            list_id: 7,
            inactive: true,
        },
    );
    assert_ne!(view(&w).factions()[&7].flags & rep::INACTIVE, 0);
    reopen(&w);
    step(&mut w, Command::AcknowledgeInitialReputations);
    let before = view(&w).clone();
    step(
        &mut w,
        Command::SetFactionInactive {
            list_id: 7,
            inactive: true,
        },
    );
    assert_eq!(view(&w), &before);
    step(
        &mut w,
        Command::SetFactionInactive {
            list_id: 7,
            inactive: false,
        },
    );
    assert_eq!(view(&w).factions()[&7].flags & rep::INACTIVE, 0);
    assert!(view(&w).factions()[&7].need_send);
    assert!(view(&w).factions()[&7].need_save);
    reopen(&w);
    replay(&w, initial);
}

#[test]
fn hidden_invisible_and_peace_forced_controls_follow_source_guards() {
    for flags in [rep::HIDDEN, rep::INVISIBLE_FORCED, rep::PEACE_FORCED] {
        let (mut p, input, mut data) = fixture::setup();
        let mut catalog = serde_json::to_value(data.reputation.take().unwrap()).unwrap();
        catalog["rows"][fixture::PRIMARY.to_string()]["Present"]["faction"]["flags"] =
            serde_json::json!([flags, 0, 0, 0]);
        data.reputation = Some(serde_json::from_value(catalog).unwrap());
        p.initialize_quest_progression(&input, data).unwrap();
        let mut w =
            NativeWorld::synthetic_component_test(p, [1; 16], synthetic_fixture::player(), 42)
                .unwrap();
        let initial = w.checkpoint().unwrap();
        let before = view(&w).clone();
        step(
            &mut w,
            Command::SetFactionAtWar {
                list_id: 3,
                at_war: true,
            },
        );
        assert_eq!(view(&w), &before, "flags {flags}");
        step(
            &mut w,
            Command::SetFactionInactive {
                list_id: 3,
                inactive: true,
            },
        );
        assert_eq!(view(&w), &before, "flags {flags}");
        reopen(&w);
        replay(&w, initial);
    }
}

#[test]
fn controls_checkpoint_flags_and_call_history_cannot_be_reblessed() {
    let mut w = fixture::world();
    step(
        &mut w,
        Command::SetFactionAtWar {
            list_id: 3,
            at_war: false,
        },
    );
    step(
        &mut w,
        Command::SetFactionAtWar {
            list_id: 3,
            at_war: true,
        },
    );
    for field in ["flags", "need_send", "history"] {
        let mut forged = w.checkpoint().unwrap();
        let mut encoded = serde_json::to_value(&forged.state).unwrap();
        let reputation = &mut encoded["quest_progression"]["reputation"];
        match field {
            "flags" => reputation["state"]["factions"]["3"]["flags"] = serde_json::json!(0),
            "need_send" => {
                reputation["state"]["factions"]["3"]["need_send"] = serde_json::json!(false)
            }
            "history" => {
                reputation["history"].as_array_mut().unwrap().pop();
            }
            _ => unreachable!(),
        }
        forged.state = serde_json::from_value(encoded).unwrap();
        forged.digest = crate::digest(b"benilla-native-state-v66\0", &forged.state);
        forged.checksum = crate::digest(
            b"benilla-native-checkpoint-v66\0",
            &(&forged.state, forged.digest, forged.history_tip),
        );
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), forged).is_err(),
            "{field}"
        );
    }
}

#[test]
fn active_cast_at_war_control_reopens_without_reinterpreting_cast_origin() {
    let (mut p, input, mut data) = fixture::setup();
    p.npcs.retain(|n| n.entry == 60003);
    data.kill.creatures.retain(|n| n.entry == 60003);
    p.npcs[0].faction_template = 2;
    p.npcs[0].ai.reaction = ai::Reaction::Aggressive;
    let original = fixture::catalog();
    let mut rows = [fixture::PRIMARY, fixture::SPILLOVER, fixture::OTHER]
        .map(|id| original.row(id).unwrap().unwrap().clone());
    rows[0].faction.base_values[0] = 3000;
    data.reputation = Some(
        rep::Catalog::new(
            rows.into(),
            vec![],
            original.config(),
            rep::Audit {
                general_reputation_auras: rep::Evidence::Absent,
                faction_reputation_auras: rep::Evidence::Absent,
                forced_reactions: rep::Evidence::Absent,
                reputation_scripts: rep::Evidence::Absent,
            },
        )
        .unwrap(),
    );
    p.initialize_quest_progression(&input, data).unwrap();
    p.source_motion = Some(synthetic_motion_fixture::genesis(&p, 3));
    let mut rows = synthetic_combat_fixture::source_rows(&p);
    rows.npcs[0].npc_flags = 2;
    rows.factions[1].faction = fixture::PRIMARY;
    rows.factions[1].reputation_list_id = Some(3);
    // Tracked reputation makes Unit::IsNeutralToAll false even when the raw
    // template alone has no friendly/enemy masks or explicit enemy rows.
    rows.factions[1].enemy_group_mask = 0;
    rows.factions[1].friend_group_mask = 0;
    p.initialize_interactions(rows).unwrap();
    p.initialize_combat_authority(vec![NpcCombatAuthoritySource {
        entry: 60003,
        extra_flags: 0,
        static_flags1: 0,
        static_flags3: 0,
        static_flags4: 0,
        static_flags2: 0,
    }])
    .unwrap();
    let mut w =
        NativeWorld::synthetic_component_test(p, [8; 16], synthetic_fixture::player(), 7).unwrap();
    let initial = w.checkpoint().unwrap();
    let mut authority = synthetic_combat_fixture::SpatialOnly::default();
    let target = w.state.npcs[&3].id;
    let cast = synthetic_quest_fixture::command(
        &w,
        Command::Cast {
            ability: 60001,
            target,
        },
    );
    w.step(w.state.tick + 1, Some(cast), &mut authority)
        .unwrap();
    assert_eq!(
        w.state.casts.len(),
        1,
        "{:?}",
        w.last_tick().unwrap().events
    );
    assert!(matches!(
        w.state.casts[&1].execution.state(),
        spell::CastState::Casting { .. } | spell::CastState::Traveling { .. }
    ));
    reopen(&w);
    let off = synthetic_quest_fixture::command(
        &w,
        Command::SetFactionAtWar {
            list_id: 3,
            at_war: false,
        },
    );
    w.step(w.state.tick + 1, Some(off), &mut authority).unwrap();
    assert_eq!(view(&w).factions()[&3].flags & rep::AT_WAR, 0);
    assert_eq!(
        w.state.casts.len(),
        1,
        "control must run with retained active cast"
    );
    reopen(&w);
    replay(&w, initial);
    let checkpoint = w.checkpoint().unwrap();
    let mut reopened =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), checkpoint).unwrap();
    let on = synthetic_quest_fixture::command(
        &w,
        Command::SetFactionAtWar {
            list_id: 3,
            at_war: true,
        },
    );
    w.step(w.state.tick + 1, Some(on.clone()), &mut authority)
        .unwrap();
    reopened
        .step(
            reopened.state.tick + 1,
            Some(on),
            &mut synthetic_combat_fixture::SpatialOnly::default(),
        )
        .unwrap();
    assert_eq!(w.state_digest(), reopened.state_digest());
    reopen(&w);
    for _ in 0..60 {
        if w.state.combat.as_ref().unwrap().in_combat {
            break;
        }
        w.step(w.state.tick + 1, None, &mut authority).unwrap();
    }
    assert!(
        w.state.combat.as_ref().unwrap().in_combat,
        "real projectile impact must enter combat"
    );
    let toggle =
        synthetic_quest_fixture::command(&w, Command::SetActionBarToggles { toggles: 0xf5 });
    let alive_before = w.state.player_alive();
    w.step(w.state.tick + 1, Some(toggle), &mut authority)
        .unwrap();
    assert_eq!(w.state.action_bar_toggles(), 0xf5);
    assert_eq!(w.state.player_alive(), alive_before);
    reopen(&w);
    let watch_start = w.checkpoint().unwrap();
    let record_offset = w.records().len();
    let watch =
        synthetic_quest_fixture::command(&w, Command::SetWatchedFaction { list_id: i32::MIN });
    w.step(w.state.tick + 1, Some(watch), &mut authority)
        .unwrap();
    assert_eq!(w.state.watched_faction(), i32::MIN);
    reopen(&w);
    let mut watch_replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), watch_start).unwrap();
    watch_replay.replay(&w.records()[record_offset..]).unwrap();
    assert_eq!(watch_replay.state_digest(), w.state_digest());
    let off = synthetic_quest_fixture::command(
        &w,
        Command::SetFactionAtWar {
            list_id: 3,
            at_war: false,
        },
    );
    w.step(w.state.tick + 1, Some(off), &mut authority).unwrap();
    assert_eq!(view(&w).factions()[&3].flags & rep::AT_WAR, 0);
    reopen(&w);
}

#[test]
fn reputation_controls_remain_available_after_real_lethal_damage() {
    use benilla_sim::player_death as d;
    let mut p = fixture::profile();
    p.npcs[0].max_health = 3000;
    p.npcs[0].combat.as_mut().unwrap().minimum_damage = synthetic_fixture::float(10000.0);
    p.npcs[0].combat.as_mut().unwrap().maximum_damage = synthetic_fixture::float(10000.0);
    p.configure_player_death(
        d::DeathConfig {
            map_instanceable: false,
            pve_reclaim_delay_enabled: true,
        },
        d::DeathSupport::checked(&d::REQUIRED_ABSENCES.map(|dependency| (dependency, Some(false))))
            .unwrap(),
    )
    .unwrap();
    let mut w =
        NativeWorld::synthetic_component_test(p, [1; 16], synthetic_fixture::player(), 7).unwrap();
    let initial = w.checkpoint().unwrap();
    let giver = w.state.npcs[&3].id;
    step(
        &mut w,
        Command::AcceptQuest {
            quest: fixture::OBJECTIVE,
            giver,
        },
    );
    let target = w.state.npcs[&2].id;
    step(
        &mut w,
        Command::Cast {
            ability: 60001,
            target,
        },
    );
    for _ in 0..100 {
        if !w.state.player_alive() {
            break;
        }
        w.step(
            w.state.tick + 1,
            None,
            &mut synthetic_quest_fixture::QuestAuthority::default(),
        )
        .unwrap();
    }
    assert_eq!(w.state.health, 0, "actual hostile melee must kill player");
    assert!(!w
        .state
        .combat
        .as_ref()
        .unwrap()
        .death
        .as_ref()
        .unwrap()
        .is_alive());
    step(
        &mut w,
        Command::SetFactionAtWar {
            list_id: 3,
            at_war: false,
        },
    );
    assert_eq!(view(&w).factions()[&3].flags & rep::AT_WAR, 0);
    step(
        &mut w,
        Command::SetFactionInactive {
            list_id: 7,
            inactive: true,
        },
    );
    assert_ne!(view(&w).factions()[&7].flags & rep::INACTIVE, 0);
    assert_eq!(w.state.health, 0);
    assert!(!w.state.player_alive());
    step(&mut w, Command::SetWatchedFaction { list_id: i32::MAX });
    assert_eq!(w.state.watched_faction(), i32::MAX);
    assert!(!w.state.player_alive());
    step(&mut w, Command::SetActionBarToggles { toggles: 0xff });
    assert_eq!(w.state.action_bar_toggles(), 0xff);
    assert_eq!(w.state.health, 0);
    assert!(!w.state.player_alive());
    reopen(&w);
    replay(&w, initial);
}
