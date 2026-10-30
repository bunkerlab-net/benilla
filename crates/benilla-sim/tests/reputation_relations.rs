// SPDX-License-Identifier: GPL-3.0-or-later
// Authored, asset-free differential inputs against exact pinned source methods.
use benilla_formats::FactionTemplate;
use benilla_sim::{classic_spell::SpellFloat, classic_targets::*, reputation as rep};
use sha2::{Digest, Sha256};

const BASES: [i32; 16] = [
    -42000, -6001, -6000, -3001, -3000, -1, 0, 2999, 3000, 8999, 9000, 20999, 21000, 41999, 42000,
    42999,
];

fn row(id: u32, list_id: i32) -> rep::Row {
    rep::Row {
        faction: rep::Faction {
            id,
            reputation_list_id: list_id,
            race_masks: [0; 4],
            class_masks: [0; 4],
            base_values: [0; 4],
            flags: [0; 4],
            team: 0,
        },
        reward_rates: rep::Resolved::Absent,
        spillover: rep::Resolved::Absent,
    }
}

fn catalog(rows: Vec<rep::Row>, absent: Vec<u32>) -> rep::Catalog {
    let one = SpellFloat::from_bits(1.0f32.to_bits()).unwrap();
    rep::Catalog::new(
        rows,
        absent,
        rep::Config {
            gain_rate: one,
            low_level_kill_rate: one,
            low_level_quest_rate: one,
        },
        rep::Audit {
            general_reputation_auras: rep::Evidence::Absent,
            faction_reputation_auras: rep::Evidence::Absent,
            forced_reactions: rep::Evidence::Absent,
            reputation_scripts: rep::Evidence::Absent,
        },
    )
    .unwrap()
}

fn seed(c: u32) -> rep::Catalog {
    let mut npc = row(2, if !c.is_multiple_of(2) { 0 } else { -1 });
    npc.faction.race_masks = [if c.is_multiple_of(3) { 0 } else { 2 }, 1, 0, 0];
    npc.faction.class_masks = [0, 128, 0, 0];
    npc.faction.base_values = [
        BASES[(c / 2) as usize % 16],
        BASES[(c / 2 + 5) as usize % 16],
        0,
        0,
    ];
    npc.faction.flags = [
        if !(c / 32).is_multiple_of(2) {
            rep::AT_WAR
        } else {
            0
        },
        if !(c / 64).is_multiple_of(2) {
            rep::AT_WAR
        } else {
            0
        },
        0,
        0,
    ];
    catalog(vec![row(1, -1), npc], vec![])
}

fn state(catalog: &rep::Catalog) -> rep::State {
    catalog
        .initialize(rep::Identity {
            race_mask: 1,
            class_mask: 128,
        })
        .unwrap()
}

fn template(faction: u32) -> FactionTemplate {
    FactionTemplate {
        faction,
        flags: 0,
        group_mask: 0,
        friend_group_mask: 0,
        enemy_group_mask: 0,
        enemies: [0; 4],
        friends: [0; 4],
    }
}

fn input<'a>(
    player: &'a FactionTemplate,
    npc: &'a FactionTemplate,
    catalog: &'a rep::Catalog,
    state: &'a rep::State,
) -> SourcePlayerNpcInput<'a> {
    let faction = |t: &'a FactionTemplate| FactionRef {
        template: Some(t),
        template_flags: Some(0),
        reputation: match catalog
            .row(t.faction)
            .unwrap()
            .unwrap()
            .faction
            .reputation_list_id
        {
            index if index >= 0 => FactionReputation::Tracked { index },
            _ => FactionReputation::Untracked,
        },
    };
    SourcePlayerNpcInput {
        units: PlayerNpcInput {
            player_faction: faction(player),
            npc_faction: faction(npc),
            player: UnitTargetState {
                unit_flags: unit_flags::PLAYER_CONTROLLED,
                alive: true,
                mounted: false,
            },
            npc: UnitTargetState {
                unit_flags: 0,
                alive: true,
                mounted: false,
            },
            context: RelationContext {
                control: ContextState::Absent,
                pvp: ContextState::Absent,
                reputation_overrides: ContextState::Absent,
                immunity: ContextState::Absent,
                special_state: ContextState::Absent,
            },
        },
        reputation: Some(ReputationRef { catalog, state }),
    }
}

fn blocking(selection: u32) -> u32 {
    [
        unit_flags::SPAWNING,
        unit_flags::NOT_ATTACKABLE_1,
        unit_flags::UNTARGETABLE,
        unit_flags::TAXI_FLIGHT,
        unit_flags::UNINTERACTIBLE,
    ]
    .into_iter()
    .enumerate()
    .filter(|(i, _)| selection & (1 << i) != 0)
    .fold(0, |flags, (_, bit)| flags | bit)
}

fn number(decision: TargetDecision) -> u8 {
    match decision {
        TargetDecision::Allowed => 1,
        TargetDecision::Denied(_) => 0,
        TargetDecision::Unsupported(reason) => panic!("unexpected unsupported domain: {reason:?}"),
    }
}

fn digest(count: usize, hash: Sha256, fixture: &str) {
    assert_eq!(fixture.trim(), format!("{count} {:x}", hash.finalize()));
}

fn ordinary_sight() -> OnSightState {
    OnSightState {
        target_evading_home: false,
        source_ignores_feign_death: false,
        target_feigning_death_successfully: false,
    }
}

#[test]
fn pinned_cpp_reputation_relations_32768_vectors() {
    relation_vectors(0);
}

#[test]
fn pinned_cpp_static_npc_disarm_preserves_all_32768_relation_vectors() {
    relation_vectors(unit_flags::DISARMED);
}

fn relation_vectors(npc_extra_flags: u32) {
    let mut hash = Sha256::new();
    for c in 0..32768u32 {
        let catalog = seed(c);
        let state = state(&catalog);
        let mut p = template(1);
        let mut n = template(2);
        if c.is_multiple_of(3) {
            p.enemies[0] = 2;
        }
        if c % 3 == 2 {
            p.friends[0] = 2;
        }
        if (c / 3).is_multiple_of(3) {
            n.enemies[0] = 1;
        }
        if c / 3 % 3 == 2 {
            n.friends[0] = 1;
        }
        let mut input = input(&p, &n, &catalog, &state);
        input.units.player.unit_flags |= blocking(c / 64 % 32);
        input.units.npc.unit_flags |= blocking(c / 1024 % 32) | npc_extra_flags;
        input.units.player.alive = !(c / 7).is_multiple_of(2);
        input.units.npc.alive = !(c / 11).is_multiple_of(2);
        input.units.player.mounted = !(c / 13).is_multiple_of(2);
        input.units.npc.mounted = !(c / 17).is_multiple_of(2);
        let ranks = source_player_npc_reactions(&input).unwrap();
        let player_sight = OnSightState {
            target_evading_home: !(c / 23).is_multiple_of(2),
            source_ignores_feign_death: !(c / 37).is_multiple_of(2),
            target_feigning_death_successfully: !(c / 31).is_multiple_of(2),
        };
        let npc_sight = OnSightState {
            target_evading_home: !(c / 19).is_multiple_of(2),
            source_ignores_feign_death: !(c / 41).is_multiple_of(2),
            target_feigning_death_successfully: !(c / 29).is_multiple_of(2),
        };
        hash.update(format!(
            "{c} {} {} {} {} {} {} {} {} {} {}\n",
            ranks.player_to_npc as u8,
            ranks.npc_to_player as u8,
            number(check_can_attack(&input, Side::Player)),
            number(check_can_attack(&input, Side::Npc)),
            number(check_can_attack_now(&input, Side::Player)),
            number(check_can_attack_now(&input, Side::Npc)),
            number(check_can_attack_on_sight(
                &input,
                Side::Player,
                player_sight
            )),
            number(check_can_attack_on_sight(&input, Side::Npc, npc_sight)),
            number(check_can_attack_in_combat(&input, Side::Player)),
            number(check_can_attack_in_combat(&input, Side::Npc))
        ));
    }
    digest(
        32768,
        hash,
        include_str!("fixtures/relations_reputation_cmangos.sha256"),
    );
}

fn bit_pattern(i: u32) -> u32 {
    match i {
        0 => 0,
        33 => u32::MAX,
        _ => 1 << (i - 1),
    }
}

#[test]
fn pinned_cpp_initiative_combat_entry_18496_vectors() {
    let mut hash = Sha256::new();
    for s in 0..34 {
        for f in 0..34 {
            for c in 0..16 {
                let initiate = can_initiate_attack(CreatureInitiateAttackInput {
                    unit_state: bit_pattern(s),
                    unit_flags: bit_pattern(f),
                    force_attacking_capability: c & 1 != 0,
                    can_aggro: c & 2 != 0,
                });
                let enter = can_enter_combat(CombatEntryInput {
                    can_enter_combat: c & 4 != 0,
                    evading_home: c & 8 != 0,
                });
                hash.update(format!(
                    "{s} {f} {c} {} {}\n",
                    u8::from(initiate),
                    u8::from(enter)
                ));
            }
        }
    }
    digest(
        18496,
        hash,
        include_str!("fixtures/lifecycle_reputation_cmangos.sha256"),
    );
}

#[test]
fn friendly_at_war_is_asymmetric_and_in_combat_overrides_target_flags() {
    let mut npc = row(2, 0);
    npc.faction.base_values[0] = 9000;
    npc.faction.flags[0] = rep::AT_WAR;
    let catalog = catalog(vec![row(1, -1), npc], vec![]);
    let state = state(&catalog);
    let (p, n) = (template(1), template(2));
    let mut input = input(&p, &n, &catalog, &state);
    assert_eq!(
        source_player_npc_reactions(&input).unwrap(),
        SourcePlayerNpcReactions {
            player_to_npc: rep::Rank::Hostile,
            npc_to_player: rep::Rank::Honored
        }
    );
    assert_eq!(
        check_can_attack(&input, Side::Player),
        TargetDecision::Allowed
    );
    assert_eq!(
        check_can_attack(&input, Side::Npc),
        TargetDecision::Denied(TargetDenial::Friendly)
    );
    assert_eq!(
        check_can_attack_in_combat(&input, Side::Npc),
        TargetDecision::Allowed
    );
    input.units.player.unit_flags |= unit_flags::UNTARGETABLE;
    assert_eq!(
        check_can_attack(&input, Side::Npc),
        TargetDecision::Denied(TargetDenial::TargetFlags {
            flags: unit_flags::UNTARGETABLE
        })
    );
    assert_eq!(
        check_can_attack_in_combat(&input, Side::Npc),
        TargetDecision::Allowed
    );
    assert_eq!(
        check_can_attack_on_sight(&input, Side::Npc, ordinary_sight()),
        TargetDecision::Denied(TargetDenial::TargetFlags {
            flags: unit_flags::UNTARGETABLE
        })
    );
}

#[test]
fn generic_attack_does_not_inherit_now_or_on_sight_requirements() {
    let catalog = catalog(vec![row(1, -1), row(2, -1)], vec![]);
    let state = state(&catalog);
    let (p, n) = (template(1), template(2));
    let mut input = input(&p, &n, &catalog, &state);
    for source in [Side::Player, Side::Npc] {
        assert_eq!(check_can_attack(&input, source), TargetDecision::Allowed);
        assert_eq!(
            check_can_attack_on_sight(&input, source, ordinary_sight()),
            TargetDecision::Denied(TargetDenial::NotEnemy)
        );
    }
    input.units.player.alive = false;
    input.units.npc.alive = false;
    input.units.player.mounted = true;
    input.units.npc.mounted = true;
    for source in [Side::Player, Side::Npc] {
        assert_eq!(check_can_attack(&input, source), TargetDecision::Allowed);
        assert_eq!(
            check_can_attack_in_combat(&input, source),
            TargetDecision::Allowed
        );
    }
    assert_eq!(
        check_can_attack_now(&input, Side::Player),
        TargetDecision::Denied(TargetDenial::PlayerDead)
    );
    assert_eq!(
        check_can_attack_now(&input, Side::Npc),
        TargetDecision::Denied(TargetDenial::NpcDead)
    );
    input.units.player.alive = true;
    input.units.npc.alive = true;
    assert_eq!(
        check_can_attack_now(&input, Side::Player),
        TargetDecision::Denied(TargetDenial::PlayerMounted)
    );
    assert_eq!(
        check_can_attack_now(&input, Side::Npc),
        TargetDecision::Denied(TargetDenial::NpcMounted)
    );
}

#[test]
fn current_source_state_rebuild_changes_both_directions_without_client_base_fallback() {
    let mut npc = row(2, 0);
    npc.faction.base_values = [-1, 42000, 0, 0];
    npc.faction.race_masks = [0, 1, 0, 0];
    npc.faction.class_masks = [0, 128, 0, 0];
    let catalog = catalog(vec![row(1, -1), npc], vec![]);
    let state = state(&catalog);
    let (p, n) = (template(1), template(2));
    let before = input(&p, &n, &catalog, &state);
    assert_eq!(
        source_player_npc_reactions(&before).unwrap(),
        SourcePlayerNpcReactions {
            player_to_npc: rep::Rank::Friendly,
            npc_to_player: rep::Rank::Unfriendly
        }
    );
    // Pinned first-match wildcard slot zero wins over a race/class-specific slot.
    assert_eq!(
        check_can_attack(&before, Side::Npc),
        TargetDecision::Allowed
    );
    assert_eq!(
        check_can_attack_on_sight(&before, Side::Npc, ordinary_sight()),
        TargetDecision::Denied(TargetDenial::NotEnemy)
    );
    let changed = state
        .reduce(
            &catalog,
            rep::Command::Set {
                faction: 2,
                standing: -6000,
                incremental: false,
                no_spillover: true,
            },
        )
        .unwrap()
        .state;
    let after = input(&p, &n, &catalog, &changed);
    assert_eq!(
        source_player_npc_reactions(&after).unwrap(),
        SourcePlayerNpcReactions {
            player_to_npc: rep::Rank::Hostile,
            npc_to_player: rep::Rank::Hostile
        }
    );
    assert_eq!(
        check_can_attack_on_sight(&after, Side::Npc, ordinary_sight()),
        TargetDecision::Allowed
    );
    assert_eq!(
        source_player_npc_reactions(&before).unwrap().npc_to_player,
        rep::Rank::Unfriendly
    );
}

fn assert_unsupported(input: &SourcePlayerNpcInput<'_>, expected: TargetUnsupported) {
    assert_eq!(source_player_npc_reactions(input), Err(expected));
    for side in [Side::Player, Side::Npc] {
        assert_eq!(
            check_can_attack(input, side),
            TargetDecision::Unsupported(expected)
        );
        assert_eq!(
            check_can_attack_now(input, side),
            TargetDecision::Unsupported(expected)
        );
        assert_eq!(
            check_can_attack_on_sight(input, side, ordinary_sight()),
            TargetDecision::Unsupported(expected)
        );
        assert_eq!(
            check_can_attack_in_combat(input, side),
            TargetDecision::Unsupported(expected)
        );
    }
}

#[test]
fn tracked_metadata_and_catalog_bound_state_cannot_be_invented() {
    let catalog = catalog(vec![row(1, -1), row(2, 0)], vec![]);
    let state = state(&catalog);
    let (p, n) = (template(1), template(2));
    let original = input(&p, &n, &catalog, &state);
    let mut changed = original;
    changed.reputation = None;
    assert_unsupported(&changed, TargetUnsupported::Reputation(Side::Npc));
    changed = original;
    changed.units.npc_faction.reputation = FactionReputation::Untracked;
    assert_unsupported(
        &changed,
        TargetUnsupported::InconsistentFactionMetadata(Side::Npc),
    );
    changed.units.npc_faction.reputation = FactionReputation::Tracked { index: 1 };
    assert_unsupported(
        &changed,
        TargetUnsupported::InconsistentFactionMetadata(Side::Npc),
    );
    let wrong_catalog = seed(1);
    changed = original;
    changed.reputation = Some(ReputationRef {
        catalog: &wrong_catalog,
        state: &state,
    });
    assert_unsupported(&changed, TargetUnsupported::InvalidReputationState);
    let mut json = serde_json::to_value(&state).unwrap();
    json["factions"] = serde_json::json!({});
    let missing: rep::State = serde_json::from_value(json).unwrap();
    changed = original;
    changed.reputation = Some(ReputationRef {
        catalog: &catalog,
        state: &missing,
    });
    assert_unsupported(&changed, TargetUnsupported::InvalidReputationState);
}

#[test]
fn unknown_or_present_excluded_context_stays_explicit_before_denials() {
    let catalog = seed(1);
    let state = state(&catalog);
    let (p, n) = (template(1), template(2));
    for evidence in [ContextState::Unknown, ContextState::Present] {
        for i in 0..5 {
            let mut input = input(&p, &n, &catalog, &state);
            input.units.player.alive = false;
            input.units.npc.alive = false;
            let reason = match i {
                0 => {
                    input.units.context.control = evidence;
                    TargetUnsupported::ControlContext
                }
                1 => {
                    input.units.context.pvp = evidence;
                    TargetUnsupported::PvpContext
                }
                2 => {
                    input.units.context.reputation_overrides = evidence;
                    TargetUnsupported::ReputationOverrides
                }
                3 => {
                    input.units.context.immunity = evidence;
                    TargetUnsupported::Immunity
                }
                _ => {
                    input.units.context.special_state = evidence;
                    TargetUnsupported::SpecialState
                }
            };
            assert_unsupported(&input, reason);
        }
    }
}

#[test]
fn source_catalog_absence_and_unresolved_lookups_do_not_become_neutral() {
    let full = catalog(vec![row(1, -1), row(2, 0)], vec![]);
    let full_state = state(&full);
    let (p, n) = (template(1), template(2));
    let original = input(&p, &n, &full, &full_state);
    for (absent, expected) in [
        (vec![2], TargetUnsupported::MissingFaction(Side::Npc)),
        (
            vec![],
            TargetUnsupported::UnresolvedReputationFaction(Side::Npc),
        ),
    ] {
        let missing = catalog(vec![row(1, -1)], absent);
        let state = state(&missing);
        let input = SourcePlayerNpcInput {
            units: original.units,
            reputation: Some(ReputationRef {
                catalog: &missing,
                state: &state,
            }),
        };
        assert_unsupported(&input, expected);
    }
    // The old capability remains deliberately untracked-only.
    assert_eq!(
        player_npc_reactions(&original.units),
        Err(TargetUnsupported::Reputation(Side::Npc))
    );
}

#[test]
fn tracked_player_faction_does_not_replace_the_npcs_directional_rank() {
    let mut player_row = row(1, 1);
    player_row.faction.base_values[0] = -42000;
    player_row.faction.flags[0] = rep::AT_WAR;
    let mut npc_row = row(2, 0);
    npc_row.faction.base_values[0] = 42000;
    let catalog = catalog(vec![player_row, npc_row], vec![]);
    let state = state(&catalog);
    let (p, n) = (template(1), template(2));
    let input = input(&p, &n, &catalog, &state);
    assert_eq!(
        source_player_npc_reactions(&input).unwrap(),
        SourcePlayerNpcReactions {
            player_to_npc: rep::Rank::Friendly,
            npc_to_player: rep::Rank::Exalted
        }
    );
}

#[test]
fn sight_feign_and_evade_checks_are_independent_of_liveness_and_mount() {
    let mut npc = row(2, 0);
    npc.faction.base_values[0] = -6000;
    npc.faction.flags[0] = rep::AT_WAR;
    let catalog = catalog(vec![row(1, -1), npc], vec![]);
    let state = state(&catalog);
    let (p, n) = (template(1), template(2));
    let mut input = input(&p, &n, &catalog, &state);
    input.units.player.alive = false;
    input.units.player.mounted = true;
    input.units.npc.alive = false;
    for source in [Side::Player, Side::Npc] {
        assert_eq!(
            check_can_attack_on_sight(&input, source, ordinary_sight()),
            TargetDecision::Allowed
        );
        let feign = OnSightState {
            target_feigning_death_successfully: true,
            ..ordinary_sight()
        };
        assert_eq!(
            check_can_attack_on_sight(&input, source, feign),
            TargetDecision::Denied(TargetDenial::TargetFeigningDeath)
        );
        assert_eq!(
            check_can_attack_on_sight(
                &input,
                source,
                OnSightState {
                    source_ignores_feign_death: true,
                    ..feign
                }
            ),
            TargetDecision::Allowed
        );
        assert_eq!(
            check_can_attack_on_sight(
                &input,
                source,
                OnSightState {
                    target_evading_home: true,
                    ..feign
                }
            ),
            TargetDecision::Denied(TargetDenial::TargetEvadingHome)
        );
    }
}

#[test]
fn source_npc_disarm_preserves_both_attack_directions_and_real_blockers() {
    let catalog = catalog(vec![row(1, -1), row(2, -1)], vec![]);
    let state = state(&catalog);
    let (mut player, mut npc) = (template(1), template(2));
    player.enemies[0] = 2;
    npc.enemies[0] = 1;
    let mut input = input(&player, &npc, &catalog, &state);
    let expected = source_player_npc_reactions(&input).unwrap();
    input.units.npc.unit_flags = unit_flags::DISARMED;
    assert_eq!(source_player_npc_reactions(&input), Ok(expected));
    for source in [Side::Player, Side::Npc] {
        assert_eq!(check_can_attack(&input, source), TargetDecision::Allowed);
        assert_eq!(
            check_can_attack_now(&input, source),
            TargetDecision::Allowed
        );
        assert_eq!(
            check_can_attack_on_sight(&input, source, ordinary_sight()),
            TargetDecision::Allowed
        );
        assert_eq!(
            check_can_attack_in_combat(&input, source),
            TargetDecision::Allowed
        );
    }
    assert!(can_initiate_attack(CreatureInitiateAttackInput {
        unit_state: 0,
        unit_flags: unit_flags::DISARMED,
        force_attacking_capability: false,
        can_aggro: true,
    }));
    for blocker in [
        unit_flags::SPAWNING,
        unit_flags::NOT_ATTACKABLE_1,
        unit_flags::UNTARGETABLE,
        unit_flags::TAXI_FLIGHT,
        unit_flags::UNINTERACTIBLE,
    ] {
        let mut blocked = input;
        blocked.units.npc.unit_flags |= blocker;
        assert_eq!(
            check_can_attack(&blocked, Side::Player),
            TargetDecision::Denied(TargetDenial::TargetFlags { flags: blocker })
        );
        blocked.units.npc.unit_flags = unit_flags::DISARMED;
        blocked.units.player.unit_flags |= blocker;
        assert_eq!(
            check_can_attack(&blocked, Side::Npc),
            TargetDecision::Denied(TargetDenial::TargetFlags { flags: blocker })
        );
    }
}

#[test]
fn source_npc_disarm_does_not_admit_player_legacy_or_other_unknown_flags() {
    let catalog = catalog(vec![row(1, -1), row(2, -1)], vec![]);
    let state = state(&catalog);
    let (player, npc) = (template(1), template(2));
    let mut input = input(&player, &npc, &catalog, &state);
    input.units.npc.unit_flags = unit_flags::DISARMED;
    assert_eq!(
        player_npc_reactions(&input.units),
        Err(TargetUnsupported::UnitFlags {
            side: Side::Npc,
            flags: unit_flags::DISARMED,
        })
    );
    input.units.player.unit_flags |= unit_flags::DISARMED;
    assert_eq!(
        source_player_npc_reactions(&input),
        Err(TargetUnsupported::UnitFlags {
            side: Side::Player,
            flags: unit_flags::DISARMED,
        })
    );
    input.units.player.unit_flags = unit_flags::PLAYER_CONTROLLED;
    for unsupported in [0x0004_0000, 0x0040_0000, 0x0080_0000, 0x1000_0000] {
        input.units.npc.unit_flags = unit_flags::DISARMED | unit_flags::UNTARGETABLE | unsupported;
        assert_eq!(
            check_can_attack(&input, Side::Player),
            TargetDecision::Unsupported(TargetUnsupported::UnitFlags {
                side: Side::Npc,
                flags: unsupported
            })
        );
    }
    input.units.npc.unit_flags = unit_flags::DISARMED;
    input.units.context.special_state = ContextState::Unknown;
    assert_eq!(
        source_player_npc_reactions(&input),
        Err(TargetUnsupported::SpecialState)
    );
}
