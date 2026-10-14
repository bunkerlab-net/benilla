// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored source rows exercise the real private host, never authored interaction facts.
use super::synthetic_fixture::{player, point, KernelAuthority};
use super::*;
use benilla_sim::{classic_targets::unit_flags, reputation as rep};

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
fn audit() -> InteractionAudit {
    let absent = InteractionEvidence::Absent;
    InteractionAudit {
        external_state_mutations: absent,
        controlled_or_summoned_units: absent,
        pvp_duel_or_sanctuary: absent,
        forced_reactions: absent,
        temporary_factions: absent,
        immunity_overrides: absent,
        npc_aura_or_script_hooks: absent,
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
        template_flags: 0,
        group_mask: 1,
        friend_group_mask: 1,
        enemy_group_mask: 0,
        enemies: [0; 4],
        friends: [0; 4],
        reputation_list_id,
    }
}
fn source_input(profile: &Profile) -> InteractionInput {
    InteractionInput {
        player: PlayerInteractionSource {
            faction_template: 1,
            unit_flags: unit_flags::PLAYER_CONTROLLED,
            player_flags: 0,
            control: control(),
        },
        npcs: profile
            .npcs
            .iter()
            .map(|npc| {
                let flags = profile
                    .quest_progression
                    .as_ref()
                    .and_then(|g| g.npc_rows.get(&npc.entry));
                NpcInteractionSource {
                    entry: npc.entry,
                    faction_template: npc.faction_template,
                    npc_flags: flags.map_or(0, |r| r.npc_flags),
                    unit_flags: flags.map_or(0, |r| r.unit_flags),
                    creature_type_flags: 0,
                    control: control(),
                    spawn_flag_or_faction_overrides: InteractionEvidence::Absent,
                }
            })
            .collect(),
        factions: vec![faction(1, 0, None)],
        audit: audit(),
    }
}
fn source_world(mut profile: Profile) -> NativeWorld {
    let rows = source_input(&profile);
    profile.initialize_interactions(rows).unwrap();
    NativeWorld::synthetic_component_test(profile, [1; 16], player(), 42).unwrap()
}
#[derive(Default)]
struct ExternalOnly {
    base: KernelAuthority,
    derived: Vec<AuthorityReply>,
    order: Vec<bool>,
    fail: bool,
}
impl Authority for ExternalOnly {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        assert!(
            !matches!(
                request.query,
                Query::QuestStatus { .. }
                    | Query::QuestInteraction { .. }
                    | Query::LootInteraction { .. }
            ),
            "source interaction escaped to external authority"
        );
        self.order.push(false);
        if self.fail {
            return Err(NativeError::AuthorityUnavailable(
                "external-only test failure",
            ));
        }
        self.base.resolve(request)
    }
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        self.order.push(true);
        self.derived.push(reply.clone());
        Ok(())
    }
}
fn step(
    w: &mut NativeWorld,
    commands: Vec<Command>,
    authority: &mut impl Authority,
) -> Result<(), NativeError> {
    let inputs = commands
        .into_iter()
        .enumerate()
        .map(|(index, command)| OwnedInput {
            authenticated_owner: w.state.player.owner,
            input: Input {
                world: w.state.world,
                player: w.state.player,
                tick: w.state.tick + 1,
                sequence: w.state.next_sequence + index as u64,
                command,
            },
        })
        .collect();
    w.step_batch(w.state.tick + 1, inputs, authority)
        .map(|_| ())
}
fn replay(w: &NativeWorld, initial: Checkpoint) -> NativeWorld {
    let mut result =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    result.replay(w.records()).unwrap();
    assert_eq!(result.checkpoint().unwrap(), w.checkpoint().unwrap());
    result
}
fn quest_fact(reply: &AuthorityReply) -> &QuestInteractionFacts {
    let Facts::QuestInteraction(facts) = &reply.facts else {
        panic!("expected interaction facts")
    };
    facts
}
#[test]
fn status_at_tick_zero_is_read_only_and_has_no_distance_or_alive_gate() {
    let mut profile = synthetic_quest_fixture::profile();
    profile.npcs[1].ai.spawn = point(1000.0);
    let mut w = source_world(profile);
    let giver = w.state.npcs[&3].id;
    let before = w.checkpoint().unwrap();
    let mut a = ExternalOnly {
        fail: true,
        ..Default::default()
    };
    assert!(w.preview_quest_status(giver, &mut a).unwrap());
    assert_eq!(w.checkpoint().unwrap(), before);
    let Query::QuestStatus { witness, .. } = &a.derived[0].request.query else {
        unreachable!()
    };
    assert_eq!(witness.moment, InteractionMoment::ReadOnly);
    assert_eq!(a.derived[0].request.binding.tick, 0);
    assert_eq!(
        w.preview_quest_interaction(giver, 0, false, &mut a)
            .unwrap(),
        Some(Rejection::QuestInteraction)
    );
    // Status is generic CanInteract even for a dead player's unreleased body.
    w.state.health = 0;
    assert!(w.preview_quest_status(giver, &mut a).unwrap());
    assert_eq!(
        w.preview_quest_interaction(giver, 0, false, &mut a)
            .unwrap(),
        Some(Rejection::Dead)
    );
}
#[test]
fn spirit_service_is_status_visible_but_immediate_interaction_denied() {
    let (mut profile, input, mut rows) = synthetic_quest_fixture::setup();
    rows.npc_rows
        .iter_mut()
        .find(|r| r.entry == 60003)
        .unwrap()
        .npc_flags |= 0x20;
    profile.initialize_quest_progression(&input, rows).unwrap();
    let w = source_world(profile);
    let giver = w.state.npcs[&3].id;
    let mut a = ExternalOnly::default();
    assert!(w.preview_quest_status(giver, &mut a).unwrap());
    assert_eq!(
        w.preview_quest_interaction(giver, 0, false, &mut a)
            .unwrap(),
        Some(Rejection::QuestInteraction)
    );
    assert!(quest_fact(a.derived.last().unwrap()).npc_invisible_to_alive);
}
#[test]
fn hello_requires_any_npc_flag_but_accept_requires_questgiver() {
    let (mut profile, input, mut rows) = synthetic_quest_fixture::setup();
    rows.npc_rows
        .iter_mut()
        .find(|r| r.entry == 60003)
        .unwrap()
        .npc_flags = 1;
    profile.initialize_quest_progression(&input, rows).unwrap();
    let w = source_world(profile);
    let giver = w.state.npcs[&3].id;
    let mut a = ExternalOnly::default();
    assert!(w.preview_quest_status(giver, &mut a).unwrap());
    assert_eq!(
        w.preview_quest_interaction(giver, 0, false, &mut a)
            .unwrap(),
        None
    );
    assert_eq!(
        w.preview_quest_interaction(giver, 70001, false, &mut a)
            .unwrap(),
        Some(Rejection::QuestInteraction)
    );
}
fn reputation_world() -> NativeWorld {
    let (mut profile, input, mut rows) = synthetic_reputation_fixture::setup();
    profile.npcs[0].faction_template = 2;
    profile.npcs[0].ai.spawn = point(2.0);
    profile.npcs[0].ai.reaction = ai::Reaction::Passive;
    rows.npc_rows.push(QuestNpcRow {
        entry: profile.npcs[0].entry,
        npc_flags: 2,
        unit_flags: 0,
    });
    rows.relations
        .starters
        .push(benilla_sim::quest_lifecycle::CreatureQuestRelation {
            creature: profile.npcs[0].entry,
            quest: synthetic_reputation_fixture::OBJECTIVE,
        });
    profile.initialize_quest_progression(&input, rows).unwrap();
    let mut semantic = source_input(&profile);
    semantic
        .factions
        .push(faction(2, synthetic_reputation_fixture::PRIMARY, Some(3)));
    profile.initialize_interactions(semantic).unwrap();
    NativeWorld::synthetic_component_test(profile, [1; 16], player(), 42).unwrap()
}
#[test]
fn reward_then_interaction_uses_current_reputation_and_exact_reply_order() {
    let mut w = reputation_world();
    let initial = w.checkpoint().unwrap();
    let giver = w.state.npcs[&3].id;
    let tracked = w.state.npcs[&2].id;
    let mut a = ExternalOnly::default();
    assert!(!w.preview_quest_status(tracked, &mut a).unwrap());
    assert_eq!(
        a.derived[0].facts,
        Facts::QuestStatus {
            player_reaction: rep::Rank::Hostile as u8,
            npc_reaction: rep::Rank::Unfriendly as u8
        }
    );
    a.order.clear();
    a.derived.clear();
    step(
        &mut w,
        vec![
            Command::AcceptQuest {
                quest: synthetic_reputation_fixture::GRANT,
                giver,
            },
            Command::Move {
                destination: point(1.0),
            },
            Command::TurnInQuest {
                quest: synthetic_reputation_fixture::GRANT,
                giver,
            },
            Command::AcceptQuest {
                quest: synthetic_reputation_fixture::OBJECTIVE,
                giver: tracked,
            },
        ],
        &mut a,
    )
    .unwrap();
    assert_eq!(&a.order[..4], &[true, false, true, true]);
    assert_eq!(
        quest_fact(&a.derived[2]).player_reaction,
        rep::Rank::Friendly as u8
    );
    assert_eq!(
        quest_fact(&a.derived[2]).npc_reaction,
        rep::Rank::Neutral as u8
    );
    let moments: Vec<_> = a
        .derived
        .iter()
        .map(|r| match &r.request.query {
            Query::QuestInteraction {
                witness,
                player_position,
                ..
            } => {
                if r == &a.derived[2] {
                    assert_eq!(*player_position, point(1.0));
                }
                witness.clone()
            }
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(
        moments.iter().map(|w| w.moment).collect::<Vec<_>>(),
        [
            InteractionMoment::Frame { action_ordinal: 0 },
            InteractionMoment::Frame { action_ordinal: 2 },
            InteractionMoment::Frame { action_ordinal: 3 },
        ]
    );
    assert!(moments
        .windows(2)
        .all(|pair| pair[0].draft != pair[1].draft));
    assert!(w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .quests
        .progress(synthetic_reputation_fixture::OBJECTIVE)
        .is_some());
    replay(&w, initial.clone());
    let restored =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), w.checkpoint().unwrap())
            .unwrap();
    assert!(restored.preview_quest_status(tracked, &mut a).unwrap());
    for tamper in [
        "fact", "draft", "ordinal", "preview", "extra", "missing", "reorder",
    ] {
        let mut records = w.records().to_vec();
        let replies = &mut records[0].replies;
        match tamper {
            "fact" => {
                let Facts::QuestInteraction(f) = &mut replies[0].facts else {
                    unreachable!()
                };
                f.npc_reaction = 7;
            }
            "draft" | "ordinal" | "preview" => {
                let Query::QuestInteraction { witness, .. } = &mut replies[0].request.query else {
                    unreachable!()
                };
                match tamper {
                    "draft" => witness.draft[0] ^= 1,
                    "ordinal" => witness.moment = InteractionMoment::Frame { action_ordinal: 1 },
                    _ => witness.moment = InteractionMoment::ReadOnly,
                }
            }
            "extra" => replies.push(replies[0].clone()),
            "missing" => {
                replies.remove(0);
            }
            _ => replies.swap(0, 1),
        }
        let mut replay =
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial.clone())
                .unwrap();
        let before = replay.state_digest();
        assert!(replay.replay(&records).is_err(), "{tamper}");
        assert_eq!(replay.state_digest(), before);
    }
}
#[test]
fn source_catalog_never_falls_back_and_external_failure_is_atomic() {
    let mut w = reputation_world();
    let giver = w.state.npcs[&3].id;
    let before = w.checkpoint().unwrap();
    let mut a = ExternalOnly {
        fail: true,
        ..Default::default()
    };
    assert!(step(
        &mut w,
        vec![
            Command::AcceptQuest {
                quest: synthetic_reputation_fixture::GRANT,
                giver
            },
            Command::Move {
                destination: point(1.0)
            }
        ],
        &mut a
    )
    .is_err());
    assert_eq!(w.checkpoint().unwrap(), before);
    assert_eq!(a.derived.len(), 1);
    w.state.quest_progression.as_mut().unwrap().reputation = None;
    assert_eq!(
        w.preview_quest_status(giver, &mut a),
        Err(NativeError::InvalidState)
    );
}
#[test]
fn stale_generation_position_health_and_private_draft_are_not_reused() {
    let mut w = source_world(synthetic_quest_fixture::profile());
    let giver = w.state.npcs[&3].id;
    let mut a = ExternalOnly::default();
    assert_eq!(
        w.preview_quest_interaction(giver, 0, false, &mut a)
            .unwrap(),
        None
    );
    let original = a.derived[0].request.clone();
    for field in [
        "generation",
        "position",
        "health",
        "flags",
        "draft",
        "moment",
    ] {
        let mut request = original.clone();
        match field {
            "generation" => request.binding.npc.as_mut().unwrap().generation += 1,
            _ => {
                let Query::QuestInteraction {
                    witness,
                    player_position,
                    npc_health,
                    unit_flags,
                    ..
                } = &mut request.query
                else {
                    unreachable!()
                };
                match field {
                    "position" => *player_position = point(7.0),
                    "health" => *npc_health += 1,
                    "flags" => *unit_flags |= unit_flags::IN_COMBAT,
                    "draft" => witness.draft[0] ^= 1,
                    _ => witness.moment = InteractionMoment::Frame { action_ordinal: 0 },
                }
            }
        }
        assert_eq!(
            interactions::resolve(
                &w.profile,
                w.profile_digest,
                &w.state,
                &request,
                InteractionMoment::ReadOnly
            ),
            Err(NativeError::AuthorityBinding),
            "{field}"
        );
    }
    w.state.npcs.remove(&2); // A future AI callback must supply its working-NPC overlay.
    let mut request = original;
    let Query::QuestInteraction { witness, .. } = &mut request.query else {
        unreachable!()
    };
    *witness =
        interactions::witness(w.profile_digest, &w.state, InteractionMoment::ReadOnly).unwrap();
    assert!(matches!(
        interactions::resolve(
            &w.profile,
            w.profile_digest,
            &w.state,
            &request,
            InteractionMoment::ReadOnly
        ),
        Err(NativeError::UnsupportedMechanic(_))
    ));
}
#[test]
fn unresolved_catalog_inputs_are_unsupported_and_do_not_mutate_profile() {
    let mut p = synthetic_quest_fixture::profile();
    let original = p.digest();
    for field in [
        "unknown", "present", "template", "metadata", "control", "flags", "override", "tracked",
    ] {
        let mut rows = source_input(&p);
        match field {
            "unknown" => rows.audit.external_state_mutations = InteractionEvidence::Unknown,
            "present" => rows.audit.temporary_factions = InteractionEvidence::Present,
            "template" => rows.factions.clear(),
            "metadata" => rows.factions[0].reputation_list_id = Some(-1),
            "control" => rows.player.control.charmer_guid = 9,
            "flags" => rows.npcs[0].unit_flags = 0x8000_0000,
            "override" => {
                rows.npcs[0].spawn_flag_or_faction_overrides = InteractionEvidence::Unknown
            }
            _ => {
                rows.factions[0].faction = 99;
                rows.factions[0].reputation_list_id = Some(1);
            }
        }
        assert!(p.initialize_interactions(rows).is_err(), "{field}");
        assert_eq!(p.digest(), original);
    }
}
#[test]
fn loot_uses_runtime_standing_lifecycle_and_source_control_facts() {
    let mut w = source_world(synthetic_loot_fixture::profile());
    let initial = w.checkpoint().unwrap();
    let mut a = ExternalOnly::default();
    let target = w.state.npcs[&2].id;
    step(
        &mut w,
        vec![Command::Cast {
            ability: 60001,
            target,
        }],
        &mut a,
    )
    .unwrap();
    for _ in 0..100 {
        if w.state.npcs[&2].health == 0 {
            break;
        }
        step(&mut w, vec![], &mut a).unwrap();
    }
    assert_eq!(w.state.npcs[&2].health, 0);
    let corpse = w.state.npcs[&2].id;
    let destination = w.state.npcs[&2].ai.position();
    step(&mut w, vec![Command::Move { destination }], &mut a).unwrap();
    step(&mut w, vec![Command::OpenLoot { corpse }], &mut a).unwrap();
    assert_eq!(
        w.state.quest_progression.as_ref().unwrap().loot_target,
        Some(corpse)
    );
    assert!(a.derived.iter().any(|r| matches!(
        r.facts,
        Facts::LootInteraction {
            player_in_world: true,
            player_stunned: false
        }
    )));
    replay(&w, initial);
}

#[test]
fn returning_home_without_victim_allows_interaction_but_fighting_does_not() {
    let mut profile = synthetic_quest_fixture::profile();
    profile.npcs[1].ai.reaction = ai::Reaction::Aggressive;
    profile.npcs[1].ai.hard_leash_radius = Some(synthetic_fixture::real(0.1));
    let mut w = source_world(profile);
    let mut a = ExternalOnly::default();
    let giver = w.state.npcs[&3].id;
    step(&mut w, vec![], &mut a).unwrap();
    assert_eq!(w.state.npcs[&3].ai.phase(), ai::Phase::Combat);
    assert!(w.preview_quest_status(giver, &mut a).unwrap());
    assert_eq!(
        w.preview_quest_interaction(giver, 0, false, &mut a)
            .unwrap(),
        Some(Rejection::QuestInteraction)
    );
    for _ in 0..30 {
        if w.state.npcs[&3].ai.phase() == ai::Phase::ReturningHome {
            break;
        }
        step(&mut w, vec![], &mut a).unwrap();
    }
    assert_eq!(w.state.npcs[&3].ai.phase(), ai::Phase::ReturningHome);
    assert!(w.state.npcs[&3].ai.target().is_none());
    assert!(w.preview_quest_status(giver, &mut a).unwrap());
    assert_eq!(
        w.preview_quest_interaction(giver, 0, false, &mut a)
            .unwrap(),
        None
    );
}

#[test]
fn missing_witness_and_unknown_versus_present_evidence_are_distinct() {
    let w = source_world(synthetic_quest_fixture::profile());
    let mut a = ExternalOnly::default();
    assert!(w.preview_quest_status(w.state.npcs[&3].id, &mut a).unwrap());
    let mut request = serde_json::to_value(&a.derived[0].request).unwrap();
    request["query"]["QuestStatus"]
        .as_object_mut()
        .unwrap()
        .remove("witness");
    assert!(serde_json::from_value::<AuthorityRequest>(request).is_err());
    for observed in [InteractionEvidence::Unknown, InteractionEvidence::Present] {
        let mut p = synthetic_quest_fixture::profile();
        let mut rows = source_input(&p);
        rows.audit.forced_reactions = observed;
        assert_eq!(
            p.initialize_interactions(rows),
            Err(NativeError::InteractionEvidence {
                dependency: "forced interaction reactions",
                observed,
            })
        );
    }
}

#[test]
fn source_death_clears_npc_flags_and_respawn_restores_them_for_new_generation() {
    let (mut profile, input, mut rows) = synthetic_quest_fixture::setup();
    profile.npcs[0].ai.spawn = point(1.0);
    rows.npc_rows.push(QuestNpcRow {
        entry: profile.npcs[0].entry,
        npc_flags: 2,
        unit_flags: 0,
    });
    profile.initialize_quest_progression(&input, rows).unwrap();
    let mut w = source_world(profile);
    let mut a = ExternalOnly::default();
    let target = w.state.npcs[&2].id;
    assert!(w.preview_quest_status(target, &mut a).unwrap());
    step(
        &mut w,
        vec![Command::Cast {
            ability: 60001,
            target,
        }],
        &mut a,
    )
    .unwrap();
    for _ in 0..30 {
        if w.state.npcs[&2].health == 0 {
            break;
        }
        step(&mut w, vec![], &mut a).unwrap();
    }
    assert_eq!(w.state.npcs[&2].ai.phase(), ai::Phase::Corpse);
    assert!(!w.preview_quest_status(target, &mut a).unwrap());
    for _ in 0..30 {
        if w.state.npcs[&2].ai.phase() == ai::Phase::Despawned {
            break;
        }
        step(&mut w, vec![], &mut a).unwrap();
    }
    assert_eq!(
        w.preview_quest_status(target, &mut a),
        Err(NativeError::UnknownTarget)
    );
    for _ in 0..30 {
        if w.state.npcs[&2].id != target {
            break;
        }
        step(&mut w, vec![], &mut a).unwrap();
    }
    let revived = w.state.npcs[&2].id;
    assert_ne!(target, revived);
    assert!(w.preview_quest_status(revived, &mut a).unwrap());
    assert_eq!(
        w.preview_quest_status(target, &mut a),
        Err(NativeError::UnknownTarget)
    );
}

#[test]
fn source_interaction_and_loot_interruptible_passives_are_explicitly_unsupported() {
    use benilla_sim::starter_init as init;
    let mut identities = vec![];
    for interrupt_flags in [0, 0x400, 0x800, 0xc00] {
        let mut input = synthetic_fixture::initialized_starter_input();
        let mut words = [0; 173];
        words[0] = 60003;
        words[6] = 0x40; // Passive numeric stat aura still has source interrupt semantics.
        words[22] = interrupt_flags;
        words[58] = u32::MAX;
        words[61] = 6;
        words[64] = 1;
        words[67] = 1;
        words[76] = 4;
        words[82] = 1;
        words[91] = 29;
        words[106] = 3;
        input.spells.push(init::SpellRow {
            id: 60003,
            words,
            duration_ms: Some(-1),
            previous_rank: None,
            talent: false,
            learned_spells: init::Rows::Complete(vec![]),
            external_initial_hooks: false,
        });
        let init::Rows::Complete(defaults) = &mut input.default_spells else {
            unreachable!()
        };
        defaults.push(60003);
        let mut profile = synthetic_fixture::initialized_profile();
        profile
            .initialize_player(&input, synthetic_fixture::float(100.0), 1)
            .unwrap();
        let metadata = &profile.combat.as_ref().unwrap().initial_auras;
        assert_eq!(metadata.len(), 1);
        assert_eq!(metadata[0].aura_interrupt_flags, interrupt_flags);
        assert_eq!(metadata[0].aura_type, 29);
        // Legacy component input remains constructible; this is a missing source-host path.
        NativeWorld::synthetic_component_test(profile.clone(), [1; 16], player(), 42).unwrap();
        let before = profile.digest();
        identities.push(before);
        let rows = source_input(&profile);
        if interrupt_flags == 0 {
            profile.initialize_interactions(rows).unwrap();
        } else {
            assert_eq!(
                profile.initialize_interactions(rows),
                Err(NativeError::UnsupportedMechanic(
                    "interaction or loot interrupted player aura removal"
                ))
            );
            assert_eq!(profile.digest(), before);
        }
    }
    assert!(identities.windows(2).all(|pair| pair[0] != pair[1]));
}
#[test]
fn same_entry_spawns_cannot_disagree_with_source_template_faction() {
    let mut profile = synthetic_quest_fixture::profile();
    let mut second = profile.npcs[1].clone();
    second.ai.creature_id = 4;
    second.faction_template = 2;
    profile.npcs.push(second);
    profile.validate().unwrap();
    let mut rows = source_input(&profile);
    rows.npcs.pop(); // One template source row serves both occurrences of entry 60003.
    rows.factions.push(faction(2, 0, None));
    let before = profile.digest();
    assert_eq!(
        profile.initialize_interactions(rows.clone()),
        Err(NativeError::InvalidProfile)
    );
    assert_eq!(profile.digest(), before);
    profile.npcs[2].faction_template = 1;
    profile.initialize_interactions(rows).unwrap();
}

#[test]
fn repeated_status_queries_have_bounded_receipts_at_maximum_npc_count() {
    for npc_count in [2, 4096] {
        let mut profile = synthetic_quest_fixture::profile();
        let rows = source_input(&profile);
        while profile.npcs.len() < npc_count {
            let mut npc = profile.npcs[1].clone();
            npc.ai.creature_id = profile.npcs.len() as u64 + 2;
            profile.npcs.push(npc);
        }
        profile.initialize_interactions(rows).unwrap();
        let w = NativeWorld::synthetic_component_test(profile, [1; 16], player(), 42).unwrap();
        let initial = w.checkpoint().unwrap();
        let checkpoint_bytes = initial.encode().unwrap().len();
        let mut a = ExternalOnly::default();
        let started = std::time::Instant::now();
        for _ in 0..MAX_INPUTS_PER_TICK {
            assert!(w.preview_quest_status(w.state.npcs[&3].id, &mut a).unwrap());
        }
        let elapsed = started.elapsed();
        eprintln!("source_interaction_workload npcs={npc_count} checkpoint_bytes={checkpoint_bytes} queries={} full_hashes={} elapsed_ms={}", MAX_INPUTS_PER_TICK, MAX_INPUTS_PER_TICK * 2, elapsed.as_millis());
        assert_eq!(a.derived.len(), MAX_INPUTS_PER_TICK);
        assert!(a.derived.windows(2).all(|pair| pair[0] == pair[1]));
        assert!(a
            .derived
            .iter()
            .all(|reply| serde_json::to_vec(reply).unwrap().len() < 2048));
        assert_eq!(w.checkpoint().unwrap(), initial);
    }
}

#[test]
fn witness_guard_preserves_full_digest_and_rejects_oversize_before_authority() {
    let mut w = source_world(synthetic_quest_fixture::profile());
    let expected = crate::digest(
        b"benilla-native-interaction-draft-v1\0",
        &persistence::snapshot_state(w.profile_digest, &w.state),
    );
    let witness =
        interactions::witness(w.profile_digest, &w.state, InteractionMoment::ReadOnly).unwrap();
    assert_eq!(witness.draft, expected);
    // Deliberately malformed oversized internal draft isolates the serialization
    // guard. It is not a claim that this NPC count passes profile admission.
    let npc = w.state.npcs[&2].clone();
    for spawn in 4..16_004 {
        let mut extra = npc.clone();
        extra.id.spawn = spawn;
        w.state.npcs.insert(spawn, extra);
    }
    let before = w.state_digest();
    let mut a = ExternalOnly::default();
    assert_eq!(
        w.preview_quest_status(w.state.npcs[&3].id, &mut a),
        Err(NativeError::TooLarge)
    );
    assert!(a.order.is_empty());
    let giver = w.state.npcs[&3].id;
    assert_eq!(
        step(
            &mut w,
            vec![Command::AcceptQuest {
                quest: 70001,
                giver
            }],
            &mut a
        ),
        Err(NativeError::TooLarge)
    );
    assert!(a.order.is_empty());
    assert_eq!(w.state_digest(), before);
    assert!(w.records().is_empty());
}
