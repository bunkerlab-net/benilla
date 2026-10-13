// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic, asset-free differential tests against exact pinned C++ method bodies.
use benilla_formats::{FactionCatalog, FactionTemplate, Reaction};
use benilla_sim::classic_targets::*;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

fn template(faction: u32) -> FactionTemplate {
    FactionTemplate {
        faction,
        group_mask: 0,
        friend_group_mask: 0,
        enemy_group_mask: 0,
        enemies: [0; 4],
        friends: [0; 4],
    }
}

fn list(shape: u32, matched: u32) -> [u32; 4] {
    match shape {
        0 => [0, 0, 0, 0],
        1 => [matched, 0, 0, 0],
        2 => [7, 8, 9, matched],
        3 => [7, 0, matched, 0],
        4 => [0, matched, 0, 0],
        _ => [7, 8, 9, 10],
    }
}

fn context() -> RelationContext {
    RelationContext {
        control: ContextState::Absent,
        pvp: ContextState::Absent,
        reputation_overrides: ContextState::Absent,
        immunity: ContextState::Absent,
        special_state: ContextState::Absent,
    }
}

fn faction_ref(row: &FactionTemplate) -> FactionRef<'_> {
    FactionRef {
        template: Some(row),
        template_flags: Some(0),
        reputation: if row.faction == 0 {
            FactionReputation::NoFaction
        } else {
            FactionReputation::Untracked
        },
    }
}

fn input<'a>(player: &'a FactionTemplate, npc: &'a FactionTemplate) -> PlayerNpcInput<'a> {
    PlayerNpcInput {
        player_faction: faction_ref(player),
        npc_faction: faction_ref(npc),
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
        context: context(),
    }
}

fn spell() -> HarmfulSpellTargetInput {
    HarmfulSpellTargetInput {
        attributes_ex2: 0,
        attributes_ex3: 0,
        is_aoe: false,
    }
}

fn check_digest(count: usize, hash: Sha256, fixture: &str) {
    let mut parts = fixture.split_whitespace();
    assert_eq!(parts.next().unwrap().parse::<usize>().unwrap(), count);
    assert_eq!(parts.next().unwrap(), format!("{:x}", hash.finalize()));
    assert_eq!(parts.next(), None);
}

#[test]
fn pinned_cpp_faction_comparator_6912_branch_vectors() {
    let mut hash = Sha256::new();
    let mut count = 0;
    for identities in 0..4 {
        for masks in 0..8 {
            for enemy in 0..6 {
                for friend_a in 0..6 {
                    for friend_b in 0..6 {
                        let mut a = template(if identities & 1 != 0 { 1 } else { 0 });
                        let mut b = template(if identities & 2 != 0 { 2 } else { 0 });
                        a.group_mask = 1;
                        b.group_mask = 2;
                        a.enemy_group_mask = if masks & 1 != 0 { 2 } else { 0 };
                        a.friend_group_mask = if masks & 2 != 0 { 2 } else { 0 };
                        b.friend_group_mask = if masks & 4 != 0 { 1 } else { 0 };
                        a.enemies = list(enemy, b.faction);
                        a.friends = list(friend_a, b.faction);
                        b.friends = list(friend_b, a.faction);
                        let reaction = faction_reaction(&a, &b) as u8;
                        hash.update(format!(
                            "{identities} {masks} {enemy} {friend_a} {friend_b} {reaction}\n"
                        ));
                        count += 1;
                    }
                }
            }
        }
    }
    check_digest(
        count,
        hash,
        include_str!("fixtures/factions_cmangos.sha256"),
    );
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
    .fold(0, |flags, (_, flag)| flags | flag)
}

fn allowed(decision: TargetDecision) -> u8 {
    match decision {
        TargetDecision::Allowed => 1,
        TargetDecision::Denied(_) => 0,
        TargetDecision::Unsupported(reason) => {
            panic!("oracle-domain input was rejected: {reason:?}")
        }
    }
}

#[test]
fn pinned_cpp_target_methods_24576_state_vectors() {
    let mut hash = Sha256::new();
    let mut count = 0;
    for reaction in 0..3 {
        for source in 0..32 {
            for target in 0..32 {
                for states in 0..8 {
                    let mut player = template(1);
                    let npc = template(2);
                    if reaction == 0 {
                        player.enemies[0] = 2;
                    }
                    if reaction == 2 {
                        player.friends[0] = 2;
                    }
                    let mut pair = input(&player, &npc);
                    pair.player.unit_flags |= blocking(source);
                    pair.npc.unit_flags = blocking(target);
                    pair.player.alive = states & 1 != 0;
                    pair.npc.alive = states & 2 != 0;
                    pair.player.mounted = states & 4 != 0;
                    let melee = allowed(check_player_melee_target(&pair));
                    let harmful = allowed(check_player_harmful_spell_relationship(&pair, &spell()));
                    let assist = allowed(check_player_assist_relationship(&pair));
                    hash.update(format!(
                        "{reaction} {source} {target} {states} {melee} {harmful} {assist}\n"
                    ));
                    count += 1;
                }
            }
        }
    }
    check_digest(count, hash, include_str!("fixtures/targets_cmangos.sha256"));
}

#[test]
fn server_zero_gap_semantics_do_not_modify_client_comparator() {
    let mut a = template(1);
    let b = template(2);
    a.enemies = [7, 0, 2, 0];
    assert_eq!(faction_reaction(&a, &b), Reaction::Hostile);
    assert_eq!(a.reaction_toward(&b), Reaction::Neutral);
    a.enemies = [0, 2, 0, 0];
    assert_eq!(faction_reaction(&a, &b), Reaction::Neutral);
    a.friends = [7, 0, 2, 0];
    assert_eq!(faction_reaction(&a, &b), Reaction::Friendly);
    assert_eq!(a.reaction_toward(&b), Reaction::Neutral);
    // A group hostility check wins even if an explicit friendship says otherwise.
    a.enemy_group_mask = 0x8000_0000;
    let b = FactionTemplate {
        group_mask: 0x8000_0000,
        ..b
    };
    assert_eq!(faction_reaction(&a, &b), Reaction::Hostile);
}

#[test]
fn directionality_neutral_attack_and_friendly_assistance_are_distinct() {
    let mut player = template(1);
    let npc = template(2);
    player.enemies = [2, 0, 0, 0];
    let pair = input(&player, &npc);
    assert_eq!(
        player_npc_reactions(&pair),
        Ok(PlayerNpcReactions {
            player_to_npc: Reaction::Hostile,
            npc_to_player: Reaction::Neutral,
        })
    );
    assert_eq!(check_player_melee_target(&pair), TargetDecision::Allowed);
    player.enemies = [0; 4];
    assert_eq!(
        check_player_melee_target(&input(&player, &npc)),
        TargetDecision::Allowed
    );
    player.friends = [2, 0, 0, 0];
    let pair = input(&player, &npc);
    assert_eq!(
        check_player_melee_target(&pair),
        TargetDecision::Denied(TargetDenial::Friendly)
    );
    assert_eq!(
        check_player_assist_relationship(&pair),
        TargetDecision::Denied(TargetDenial::NpcNotPvp)
    );
}

fn assert_unsupported(pair: &PlayerNpcInput<'_>, reason: TargetUnsupported) {
    assert_eq!(player_npc_reactions(pair), Err(reason));
    assert_eq!(
        check_player_melee_target(pair),
        TargetDecision::Unsupported(reason)
    );
    assert_eq!(
        check_player_harmful_spell_relationship(pair, &spell()),
        TargetDecision::Unsupported(reason)
    );
    assert_eq!(
        check_player_assist_relationship(pair),
        TargetDecision::Unsupported(reason)
    );
}

#[test]
fn missing_metadata_never_grants_a_relationship() {
    let player = template(1);
    let npc = template(2);
    for side in [Side::Player, Side::Npc] {
        for variant in 0..6 {
            let mut pair = input(&player, &npc);
            let row = if side == Side::Player {
                &mut pair.player_faction
            } else {
                &mut pair.npc_faction
            };
            let reason = match variant {
                0 => {
                    row.template = None;
                    TargetUnsupported::MissingTemplate(side)
                }
                1 => {
                    row.template_flags = None;
                    TargetUnsupported::MissingTemplateFlags(side)
                }
                2 => {
                    row.reputation = FactionReputation::Missing;
                    TargetUnsupported::MissingFaction(side)
                }
                3 => {
                    row.reputation = FactionReputation::Tracked { index: 0 };
                    TargetUnsupported::Reputation(side)
                }
                4 => {
                    row.reputation = FactionReputation::NoFaction;
                    TargetUnsupported::InconsistentFactionMetadata(side)
                }
                _ => {
                    row.reputation = FactionReputation::Tracked { index: -1 };
                    TargetUnsupported::InconsistentFactionMetadata(side)
                }
            };
            assert_unsupported(&pair, reason);
        }
    }
    let catalog = FactionCatalog::from_rows(HashMap::from([(1, player), (2, npc)]), HashMap::new());
    let from_fixture = FactionRef::from_catalog(&catalog, 1);
    assert!(from_fixture.template.is_some());
    assert_eq!(from_fixture.template_flags, None);
    assert_eq!(from_fixture.reputation, FactionReputation::Missing);
    let absent = FactionRef::from_catalog(&catalog, 3);
    assert!(absent.template.is_none());
    assert_eq!(absent.reputation, FactionReputation::Missing);
    let no_faction = template(0);
    let mut pair = input(&player, &no_faction);
    assert!(player_npc_reactions(&pair).is_ok());
    pair.npc_faction.reputation = FactionReputation::Untracked;
    assert_unsupported(
        &pair,
        TargetUnsupported::InconsistentFactionMetadata(Side::Npc),
    );
}

#[test]
fn every_template_flag_is_explicitly_unsupported() {
    let player = template(1);
    let npc = template(2);
    for side in [Side::Player, Side::Npc] {
        for shift in 0..32 {
            let flags = 1 << shift;
            let mut pair = input(&player, &npc);
            let row = if side == Side::Player {
                &mut pair.player_faction
            } else {
                &mut pair.npc_faction
            };
            row.template_flags = Some(flags);
            assert_unsupported(&pair, TargetUnsupported::TemplateFlags { side, flags });
        }
    }
}

#[test]
fn unknown_or_present_override_context_never_falls_back_to_flags() {
    let player = template(1);
    let npc = template(2);
    for state in [ContextState::Present, ContextState::Unknown] {
        for field in 0..5 {
            let mut pair = input(&player, &npc);
            let reason = match field {
                0 => {
                    pair.context.control = state;
                    TargetUnsupported::ControlContext
                }
                1 => {
                    pair.context.pvp = state;
                    TargetUnsupported::PvpContext
                }
                2 => {
                    pair.context.reputation_overrides = state;
                    TargetUnsupported::ReputationOverrides
                }
                3 => {
                    pair.context.immunity = state;
                    TargetUnsupported::Immunity
                }
                _ => {
                    pair.context.special_state = state;
                    TargetUnsupported::SpecialState
                }
            };
            assert_unsupported(&pair, reason);
        }
    }
}

#[test]
fn every_unit_flag_bit_is_classified_for_both_sides() {
    let player = template(1);
    let npc = template(2);
    let ignored =
        unit_flags::SWIMMING | unit_flags::IN_COMBAT | unit_flags::LOOTING | unit_flags::SHEATHE;
    for side in [Side::Player, Side::Npc] {
        for shift in 0..32 {
            let flags = 1 << shift;
            let mut pair = input(&player, &npc);
            let state = if side == Side::Player {
                &mut pair.player
            } else {
                &mut pair.npc
            };
            state.unit_flags |= flags;
            if flags == unit_flags::PLAYER_CONTROLLED {
                if side == Side::Npc {
                    assert_unsupported(&pair, TargetUnsupported::ControlContext);
                } else {
                    assert_eq!(check_player_melee_target(&pair), TargetDecision::Allowed);
                }
            } else if flags & (ATTACK_BLOCKING_FLAGS | ignored) != 0 {
                assert!(player_npc_reactions(&pair).is_ok());
                let expected = if side == Side::Npc && flags & ATTACK_BLOCKING_FLAGS != 0 {
                    TargetDecision::Denied(TargetDenial::TargetFlags { flags })
                } else {
                    TargetDecision::Allowed
                };
                assert_eq!(check_player_melee_target(&pair), expected);
            } else {
                let reason = match flags {
                    unit_flags::POSSESSED => TargetUnsupported::ControlContext,
                    unit_flags::PVP => TargetUnsupported::PvpContext,
                    unit_flags::PERSUADED => TargetUnsupported::ReputationOverrides,
                    unit_flags::IMMUNE_TO_PLAYER
                    | unit_flags::IMMUNE_TO_NPC
                    | unit_flags::IMMUNE => TargetUnsupported::Immunity,
                    _ => TargetUnsupported::UnitFlags { side, flags },
                };
                assert_unsupported(&pair, reason);
            }
        }
    }
    let mut pair = input(&player, &npc);
    pair.player.unit_flags = 0;
    assert_unsupported(&pair, TargetUnsupported::ControlContext);
}

#[test]
fn spell_attribute_bits_are_exhaustively_rejected_outside_zero_domain() {
    let player = template(1);
    let npc = template(2);
    let pair = input(&player, &npc);
    for extended_three in [false, true] {
        for shift in 0..32 {
            let bit = 1 << shift;
            let mut attributes = spell();
            if extended_three {
                attributes.attributes_ex3 = bit;
            } else {
                attributes.attributes_ex2 = bit;
            }
            let expected = if !extended_three && shift == 0 {
                TargetUnsupported::DeadTargetSpell
            } else if extended_three && shift == 28 {
                TargetUnsupported::IgnoreRestrictionsSpell
            } else {
                TargetUnsupported::SpellAttributes {
                    attributes_ex2: attributes.attributes_ex2,
                    attributes_ex3: attributes.attributes_ex3,
                }
            };
            assert_eq!(
                check_player_harmful_spell_relationship(&pair, &attributes),
                TargetDecision::Unsupported(expected)
            );
        }
    }
    assert_eq!(
        check_player_harmful_spell_relationship(
            &pair,
            &HarmfulSpellTargetInput {
                is_aoe: true,
                ..spell()
            }
        ),
        TargetDecision::Unsupported(TargetUnsupported::AreaSpell)
    );
}

#[test]
fn death_mount_and_target_flags_follow_upstream_melee_precedence() {
    let player = template(1);
    let npc = template(2);
    let mut pair = input(&player, &npc);
    pair.player.alive = false;
    pair.player.mounted = true;
    pair.npc.alive = false;
    pair.npc.unit_flags = ATTACK_BLOCKING_FLAGS;
    assert_eq!(
        check_player_melee_target(&pair),
        TargetDecision::Denied(TargetDenial::PlayerDead)
    );
    pair.player.alive = true;
    assert_eq!(
        check_player_melee_target(&pair),
        TargetDecision::Denied(TargetDenial::PlayerMounted)
    );
    pair.player.mounted = false;
    assert_eq!(
        check_player_melee_target(&pair),
        TargetDecision::Denied(TargetDenial::NpcDead)
    );
    pair.npc.alive = true;
    assert_eq!(
        check_player_melee_target(&pair),
        TargetDecision::Denied(TargetDenial::TargetFlags {
            flags: ATTACK_BLOCKING_FLAGS
        })
    );
    // CanAttackSpell is only a relationship subcheck: CheckCast owns caster state.
    pair.npc.unit_flags = 0;
    pair.player.alive = false;
    pair.player.mounted = true;
    assert_eq!(
        check_player_harmful_spell_relationship(&pair, &spell()),
        TargetDecision::Allowed
    );
    pair.npc.alive = false;
    assert_eq!(
        check_player_harmful_spell_relationship(&pair, &spell()),
        TargetDecision::Denied(TargetDenial::NpcDead)
    );
}
