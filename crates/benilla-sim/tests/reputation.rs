// SPDX-License-Identifier: GPL-3.0-or-later
// Authored data; differential comparison with extracted, pinned source methods.
use benilla_sim::{classic_spell::SpellFloat, reputation::*};
use std::fmt::Write;
const RATES: [u32; 8] = [
    0, 0x3e4ccccd, 0x3f000000, 0x3f800000, 0x3f8ccccd, 0x3fc00000, 0x40551eb8, 0x3f7fffff,
];
const BASES: [i32; 12] = [
    -42000, -6001, -6000, -3001, -3000, -1, 0, 2999, 3000, 8999, 21000, 41999,
];
const VALUES: [i32; 9] = [-16777217, -1001, -101, -1, 0, 1, 101, 1001, 16777217];
const RANKS: [Rank; 8] = [
    Rank::Hated,
    Rank::Hostile,
    Rank::Unfriendly,
    Rank::Neutral,
    Rank::Friendly,
    Rank::Honored,
    Rank::Revered,
    Rank::Exalted,
];
fn float(bits: u32) -> SpellFloat {
    SpellFloat::from_bits(bits).unwrap()
}
fn audit() -> Audit {
    Audit {
        general_reputation_auras: Evidence::Absent,
        faction_reputation_auras: Evidence::Absent,
        forced_reactions: Evidence::Absent,
        reputation_scripts: Evidence::Absent,
    }
}
fn config(c: usize) -> Config {
    Config {
        gain_rate: float(RATES[(c / 11) % 8]),
        low_level_kill_rate: float(RATES[(c / 13) % 4]),
        low_level_quest_rate: float(RATES[(c / 17) % 4]),
    }
}
fn rows(c: usize) -> Vec<Row> {
    (0..4)
        .map(|i| Row {
            faction: Faction {
                id: 100 + i as u32,
                reputation_list_id: i as i32,
                race_masks: [if c.is_multiple_of(3) { 0 } else { 2 }, 1, 0, 0],
                class_masks: [0, 128, 0, 0],
                base_values: [BASES[(c + i) % 12], BASES[(c + i + 3) % 12], 0, 0],
                flags: [((c / 3 + i) % 64) as u32, ((c / 5 + i) % 64) as u32, 0, 0],
                team: 100,
            },
            reward_rates: if i == 0 && !c.is_multiple_of(3) {
                Resolved::Present(RewardRates {
                    quest: float(RATES[(c / 19) % 8]),
                    creature: float(RATES[(c / 23) % 8]),
                    spell: float(RATES[(c / 29) % 8]),
                })
            } else {
                Resolved::Absent
            },
            spillover: if i == 0 {
                Resolved::Present([
                    Spillover {
                        faction: 101,
                        rate: float(0x3f000000),
                        maximum_rank: RANKS[(c / 7) % 8],
                    },
                    Spillover {
                        faction: 101,
                        rate: float(0x3f000000),
                        maximum_rank: RANKS[(c / 7) % 8],
                    },
                    Spillover {
                        faction: 100,
                        rate: float(0x3e4ccccd),
                        maximum_rank: Rank::Exalted,
                    },
                    Spillover {
                        faction: 102,
                        rate: float(0xbe800000),
                        maximum_rank: Rank::Exalted,
                    },
                ])
            } else {
                Resolved::Absent
            },
        })
        .collect()
}
fn catalog(c: usize) -> Catalog {
    Catalog::new(rows(c), vec![999], config(c), audit()).unwrap()
}
fn identity() -> Identity {
    Identity {
        race_mask: 1,
        class_mask: 128,
    }
}
fn snapshot(state: &State) -> String {
    let mut out = String::new();
    for s in state.factions().values() {
        write!(
            out,
            "{}:{}:{}:{}:{},",
            s.reputation_list_id,
            s.standing,
            s.flags,
            u8::from(s.need_send),
            u8::from(s.need_save)
        )
        .unwrap();
    }
    out
}
fn events(out: &mut String, events: Vec<Event>) {
    for e in events {
        match e {
            Event::Visible { list_id } => write!(out, "V:{list_id}").unwrap(),
            Event::ReputationChanged {
                faction,
                snapshot: state,
            } => write!(out, "C:{faction}@{}", snapshot(&state)).unwrap(),
            Event::StateSent { standings } => {
                out.push_str("S:");
                for (id, standing) in standings {
                    write!(out, "{id}:{standing},").unwrap();
                }
            }
            Event::InitialSent { .. } => panic!("unexpected initial event"),
        };
        out.push('|');
    }
}
#[test]
fn source_extracted_gain_sequences() {
    let mut count = 0;
    for line in include_str!("fixtures/reputation_gain.tsv").lines() {
        let mut cells = line.split('\t');
        let c: usize = cells.next().unwrap().parse().unwrap();
        let expected: i32 = cells.next().unwrap().parse().unwrap();
        let input = GainInput {
            source: [Source::Kill, Source::Quest, Source::Spell][c % 3],
            reputation: VALUES[(c / 9) % 9],
            faction: 100,
            player_level: (1 + c % 63) as u8,
            target_level: ((c * 7) % 66) as u32,
            no_aura_bonus: !c.is_multiple_of(2),
        };
        assert_eq!(
            catalog(c).calculate_gain(input).unwrap(),
            expected,
            "case {c}"
        );
        count += 1;
    }
    assert_eq!(count, 8192);
}
#[test]
fn source_extracted_state_and_callback_sequences() {
    check_source_state_and_callback_sequences(include_str!("fixtures/reputation.tsv"), false);
}
#[test]
fn source_extracted_aura_state_and_callback_sequences() {
    check_source_state_and_callback_sequences(include_str!("fixtures/reputation_aura.tsv"), true);
}
fn check_source_state_and_callback_sequences(fixture: &str, with_aura: bool) {
    let mut count = 0;
    for line in fixture.lines() {
        let cells: Vec<_> = line.split('\t').collect();
        let c: usize = cells[0].parse().unwrap();
        let cat = if with_aura {
            aura_catalog(c, &[10])
        } else {
            catalog(c)
        };
        let mut state = cat.initialize(identity()).unwrap();
        if !c.is_multiple_of(2) {
            state = state.reduce(&cat, Command::SendInitial).unwrap().state;
        }
        let value = VALUES[(c / 9) % 9];
        let result = state
            .reduce(
                &cat,
                Command::Set {
                    faction: 100,
                    standing: value,
                    incremental: !(c / 2).is_multiple_of(2),
                    no_spillover: !(c / 4).is_multiple_of(2),
                },
            )
            .unwrap();
        assert_eq!(
            u8::from(result.rank_increased).to_string(),
            cells[1],
            "case {c}"
        );
        let mut trace = String::new();
        events(&mut trace, result.events);
        state = result.state;
        for command in [
            Command::AtWar {
                list_id: 2,
                on: !(c / 8).is_multiple_of(2),
            },
            Command::Inactive {
                list_id: 1,
                on: !(c / 16).is_multiple_of(2),
            },
            Command::Visible { faction: 103 },
        ] {
            let t = state.reduce(&cat, command).unwrap();
            events(&mut trace, t.events);
            state = t.state;
        }
        let reward = QuestReward {
            factions: [100, 101, 100, 103, 0],
            values: [value, -value, value / 3, 0, 0],
            faction_flags: (c % 32) as u32,
            quest_level: if c.is_multiple_of(7) {
                -1
            } else {
                (1 + (c * 7) % 63) as i32
            },
        };
        let t = state
            .reward_quest(&cat, reward, (1 + c % 63) as u8)
            .unwrap();
        events(&mut trace, t.events);
        state = t.state;
        assert_eq!(snapshot(&state), cells[2], "state case {c}");
        assert_eq!(trace, cells[3], "events case {c}");
        let mut queries = String::new();
        for id in 100..104 {
            write!(
                queries,
                "{}:{}:{}:{},",
                state.base(&cat, id).unwrap(),
                state.current(&cat, id).unwrap(),
                state.rank(&cat, id).unwrap() as u8,
                u8::from(state.at_war(&cat, id).unwrap())
            )
            .unwrap();
        }
        assert_eq!(queries, cells[4], "queries case {c}");
        count += 1;
    }
    assert_eq!(count, 2048);
}
#[test]
fn first_matching_zero_masks_and_unmatched_slots() {
    let mut f = rows(1).remove(0).faction;
    f.race_masks = [0, 1, 0, 0];
    f.class_masks = [0, 128, 0, 0];
    assert_eq!(f.matching_slot(identity()), Some(0));
    assert_eq!(f.base(identity()), BASES[1]);
    f.class_masks[0] = 1;
    assert_eq!(f.matching_slot(identity()), Some(1));
    f.race_masks = [2; 4];
    assert_eq!(f.matching_slot(identity()), None);
    assert_eq!(f.base(identity()), 0);
    assert_eq!(f.default_flags(identity()), 0);
}
#[test]
fn rank_boundaries_and_clamping() {
    for (n, r) in [
        (-42001, Rank::Hated),
        (-6001, Rank::Hated),
        (-6000, Rank::Hostile),
        (-3001, Rank::Hostile),
        (-3000, Rank::Unfriendly),
        (-1, Rank::Unfriendly),
        (0, Rank::Neutral),
        (2999, Rank::Neutral),
        (3000, Rank::Friendly),
        (8999, Rank::Friendly),
        (9000, Rank::Honored),
        (20999, Rank::Honored),
        (21000, Rank::Revered),
        (41999, Rank::Revered),
        (42000, Rank::Exalted),
        (43000, Rank::Exalted),
    ] {
        assert_eq!(rank(n), r);
    }
    let cat = catalog(0);
    let state = cat.initialize(identity()).unwrap();
    for (n, expected) in [(i32::MIN, BOTTOM), (i32::MAX, CAP)] {
        let t = state
            .reduce(
                &cat,
                Command::Set {
                    faction: 100,
                    standing: n,
                    incremental: false,
                    no_spillover: true,
                },
            )
            .unwrap();
        assert_eq!(t.state.current(&cat, 100).unwrap(), expected);
    }
}
#[test]
fn explicit_absence_does_not_make_unresolved_inputs_absent() {
    let cat = catalog(1);
    let state = cat.initialize(identity()).unwrap();
    assert_eq!(state.current(&cat, 999).unwrap(), 0);
    assert_eq!(state.rank(&cat, 999).unwrap(), Rank::Neutral);
    assert_eq!(state.current(&cat, 998), Err(Error::Unresolved(998)));
    let mut rs = rows(1);
    if let Resolved::Present(ref mut spills) = rs[0].spillover {
        spills[0].faction = 998;
    }
    assert_eq!(
        Catalog::new(rs, vec![], config(1), audit()).unwrap_err(),
        Error::Unresolved(998)
    );
}
#[test]
fn immutable_hash_and_state_round_trip() {
    let cat = catalog(77);
    let state = cat.initialize(identity()).unwrap();
    let restored: Catalog = serde_json::from_str(&serde_json::to_string(&cat).unwrap()).unwrap();
    assert_eq!(cat, restored);
    assert_eq!(cat.hash(), restored.hash());
    let restored: State = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    restored.validate(&cat).unwrap();
    assert_eq!(state, restored);
    let changed = catalog(78);
    assert!(state.reduce(&changed, Command::SendInitial).is_err());
    let mut rows = rows(77);
    rows[0].faction.team += 1;
    assert_ne!(
        cat.hash(),
        Catalog::new(rows, vec![999], config(77), audit())
            .unwrap()
            .hash()
    );
}
#[test]
fn unsupported_dependencies_and_numeric_overflow_are_rejected_transactionally() {
    for evidence in [Evidence::Unknown, Evidence::Present] {
        let mut a = audit();
        a.forced_reactions = evidence;
        assert!(Catalog::new(rows(1), vec![999], config(1), a).is_err());
    }
    let cat = catalog(0);
    let state = cat.initialize(identity()).unwrap();
    let before = state.clone();
    assert!(state
        .reduce(
            &cat,
            Command::Set {
                faction: 100,
                standing: i32::MIN,
                incremental: true,
                no_spillover: true
            }
        )
        .is_err());
    assert_eq!(state, before);
    let mut config = config(1);
    config.gain_rate = float(0x49742400);
    let cat = Catalog::new(rows(0), vec![], config, audit()).unwrap();
    assert!(cat
        .calculate_gain(GainInput {
            source: Source::Spell,
            reputation: i32::MAX,
            faction: 100,
            player_level: 60,
            target_level: 60,
            no_aura_bonus: false
        })
        .is_err());
}
#[test]
fn duplicate_list_ids_keep_source_later_row_initialization() {
    let mut rs = rows(1);
    rs[1].faction.reputation_list_id = 0;
    let cat = Catalog::new(rs, vec![], config(1), audit()).unwrap();
    let s = cat.initialize(identity()).unwrap();
    assert_eq!(s.factions().len(), 3);
    assert_eq!(s.factions()[&0].id, 101);
    assert_eq!(s.factions()[&0].flags, 1);
    assert_ne!(s.base(&cat, 100), s.base(&cat, 101));
}
#[test]
fn quest_zero_fields_and_pre_xp_level() {
    let mut cfg = config(0);
    cfg.gain_rate = SpellFloat::ONE;
    cfg.low_level_quest_rate = float(0x3e4ccccd);
    let cat = Catalog::new(rows(0), vec![], cfg, audit()).unwrap();
    let reward = QuestReward {
        factions: [100, 999, 0, 0, 0],
        values: [100, 0, 100, 0, 0],
        faction_flags: 1,
        quest_level: 5,
    };
    assert!(reward.command(&cat, 10, 1).unwrap().is_none());
    assert!(reward.command(&cat, 10, 2).unwrap().is_none());
    let amount = |level| match reward.command(&cat, level, 0).unwrap().unwrap() {
        Command::Set { standing, .. } => standing,
        _ => panic!(),
    };
    assert_eq!(amount(10), 100);
    assert_eq!(amount(11), 80);
    let mut scaling = reward;
    scaling.quest_level = -1;
    match scaling.command(&cat, 60, 0).unwrap().unwrap() {
        Command::Set { standing, .. } => assert_eq!(standing, 100),
        _ => panic!(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReentrantHost {
    triggered: bool,
    level: u8,
    reject: bool,
}
impl Host for ReentrantHost {
    fn current_level(&self) -> u8 {
        self.level
    }
    fn reputation_changed(
        &mut self,
        catalog: &Catalog,
        state: &mut State,
        faction: u32,
    ) -> Result<Vec<Event>> {
        if !self.triggered && faction == 101 {
            self.triggered = true;
            self.level = 60;
            let next = state.reduce_with_host(
                catalog,
                Command::Set {
                    faction: 101,
                    standing: 42000,
                    incremental: false,
                    no_spillover: true,
                },
                self,
            )?;
            *self = next.host;
            *state = next.transition.state;
            if self.reject {
                return Err(Error::Callback("authored_failure"));
            }
            return Ok(next.transition.events);
        }
        Ok(vec![])
    }
}
#[test]
fn source_extracted_reentrant_callback_order_and_host_level_changes() {
    check_source_reentrant_callbacks(include_str!("fixtures/reputation_host.tsv"), false);
}
#[test]
fn source_extracted_aura_reentrant_callback_order_and_host_level_changes() {
    check_source_reentrant_callbacks(include_str!("fixtures/reputation_aura_host.tsv"), true);
}
fn check_source_reentrant_callbacks(fixture: &str, with_aura: bool) {
    let mut count = 0;
    for line in fixture.lines() {
        let cells: Vec<_> = line.split('\t').collect();
        let c: usize = cells[0].parse().unwrap();
        let cat = if with_aura {
            aura_catalog(c, &[10])
        } else {
            catalog(c)
        };
        let mut state = cat.initialize(identity()).unwrap();
        let mut host = ReentrantHost {
            triggered: false,
            level: (1 + c % 63) as u8,
            reject: false,
        };
        if !c.is_multiple_of(2) {
            state = state.reduce(&cat, Command::SendInitial).unwrap().state;
        }
        let value = VALUES[(c / 9) % 9];
        let t = state
            .reduce_with_host(
                &cat,
                Command::Set {
                    faction: 100,
                    standing: value,
                    incremental: !(c / 2).is_multiple_of(2),
                    no_spillover: !(c / 4).is_multiple_of(2),
                },
                &host,
            )
            .unwrap();
        assert_eq!(
            u8::from(t.transition.rank_increased).to_string(),
            cells[1],
            "case {c}"
        );
        let mut trace = String::new();
        events(&mut trace, t.transition.events);
        state = t.transition.state;
        host = t.host;
        for command in [
            Command::AtWar {
                list_id: 2,
                on: !(c / 8).is_multiple_of(2),
            },
            Command::Inactive {
                list_id: 1,
                on: !(c / 16).is_multiple_of(2),
            },
            Command::Visible { faction: 103 },
        ] {
            let t = state.reduce_with_host(&cat, command, &host).unwrap();
            events(&mut trace, t.transition.events);
            state = t.transition.state;
            host = t.host;
        }
        let reward = QuestReward {
            factions: [100, 101, 100, 103, 0],
            values: [value, -value, value / 3, 0, 0],
            faction_flags: (c % 32) as u32,
            quest_level: if c.is_multiple_of(7) {
                -1
            } else {
                (1 + (c * 7) % 63) as i32
            },
        };
        let t = state.reward_quest_with_host(&cat, reward, &host).unwrap();
        events(&mut trace, t.transition.events);
        state = t.transition.state;
        assert_eq!(snapshot(&state), cells[2], "host state case {c}");
        assert_eq!(trace, cells[3], "host trace case {c}");
        count += 1;
    }
    assert_eq!(count, 256);
}
#[test]
fn callback_failure_rolls_back_reputation_and_host() {
    let cat = catalog(0);
    let state = cat.initialize(identity()).unwrap();
    let before = state.clone();
    let host = ReentrantHost {
        triggered: false,
        level: 10,
        reject: true,
    };
    let original = host.clone();
    let result = state.reduce_with_host(
        &cat,
        Command::Set {
            faction: 101,
            standing: 10,
            incremental: true,
            no_spillover: true,
        },
        &host,
    );
    assert_eq!(result.unwrap_err(), Error::Callback("authored_failure"));
    assert_eq!(state, before);
    assert_eq!(host, original);
}

// Original authored aura records. These identifiers/fingerprints are not game rows.
fn aura(index: usize, amount: i32) -> GeneralReputationAura {
    GeneralReputationAura {
        spell_id: 900000 + index as u32,
        effect_index: 0,
        aura_type: 156,
        misc_value: index as i32 - 1,
        amount,
        attributes: 0x40,
        attributes_ex3: 0,
        caster_aura_state: 0,
        aura_interrupt_flags: 0,
        duration_ms: -1,
        source_spell_hash: [1 + (index % 255) as u8; 32],
    }
}
fn bound_catalog(rows: Vec<Row>, config: Config, amounts: &[i32]) -> Catalog {
    let mut evidence = audit();
    evidence.general_reputation_auras = Evidence::Present;
    Catalog::new_with_general_reputation_auras(
        rows,
        vec![999],
        config,
        evidence,
        amounts
            .iter()
            .enumerate()
            .map(|(i, &n)| aura(i, n))
            .collect(),
    )
    .unwrap()
}
fn aura_catalog(c: usize, amounts: &[i32]) -> Catalog {
    bound_catalog(rows(c), config(c), amounts)
}
fn unit_config() -> Config {
    Config {
        gain_rate: SpellFloat::ONE,
        low_level_kill_rate: float(0x3e4ccccd),
        low_level_quest_rate: float(0x3e4ccccd),
    }
}
const AURA_AMOUNTS: [&[i32]; 15] = [
    &[10],
    &[-10],
    &[0],
    &[7, 3],
    &[99],
    &[100],
    &[101],
    &[-99],
    &[-100],
    &[-101],
    &[1],
    &[-1],
    &[16777217],
    &[-16777217],
    &[16777217, 1, -16777217],
];
const AURA_VALUES: [i32; 7] = [-1001, -250, -75, 0, 75, 250, 16777217];
const AURA_LEVELS: [(u8, u32); 10] = [
    (60, 0),
    (60, 60),
    (60, 55),
    (60, 54),
    (60, 50),
    (60, 49),
    (60, 45),
    (10, 4),
    (1, 1),
    (63, 1),
];
#[test]
fn source_extracted_retained_aura_gain_matrix() {
    let mut count = 0;
    for line in include_str!("fixtures/reputation_aura_gain.tsv").lines() {
        let (case, expected) = line.split_once('\t').unwrap();
        let c: usize = case.parse().unwrap();
        assert_eq!(c, count);
        let mut n = c;
        let source = [Source::Kill, Source::Quest, Source::Spell][n % 3];
        n /= 3;
        let v = n % 7;
        n /= 7;
        let a = n % 15;
        n /= 15;
        let no_aura_bonus = !n.is_multiple_of(2);
        n /= 2;
        let (player_level, target_level) = AURA_LEVELS[n % 10];
        n /= 10;
        let mut cfg = unit_config();
        let mut rs = rows(0);
        rs[0].reward_rates = match n % 4 {
            0 => Resolved::Absent,
            1 => Resolved::Present(RewardRates {
                quest: SpellFloat::ZERO,
                creature: SpellFloat::ZERO,
                spell: SpellFloat::ZERO,
            }),
            2 => {
                cfg.gain_rate = float(0x3fc00000);
                cfg.low_level_kill_rate = SpellFloat::ZERO;
                cfg.low_level_quest_rate = float(0x3f000000);
                Resolved::Present(RewardRates {
                    quest: float(0x3f8ccccd),
                    creature: float(0x3f000000),
                    spell: float(0x3f7fffff),
                })
            }
            _ => {
                cfg.gain_rate = float(0x40551eb8);
                cfg.low_level_kill_rate = SpellFloat::ONE;
                cfg.low_level_quest_rate = SpellFloat::ZERO;
                Resolved::Present(RewardRates {
                    quest: SpellFloat::ONE,
                    creature: SpellFloat::ONE,
                    spell: SpellFloat::ONE,
                })
            }
        };
        let input = GainInput {
            source,
            reputation: if a >= 12 && v == 6 {
                101
            } else {
                AURA_VALUES[v]
            },
            faction: 100,
            player_level,
            target_level,
            no_aura_bonus,
        };
        assert_eq!(
            bound_catalog(rs, cfg, AURA_AMOUNTS[a])
                .calculate_gain(input)
                .unwrap(),
            expected.parse::<i32>().unwrap(),
            "aura case {c}"
        );
        count += 1;
    }
    assert_eq!(count, 25200);
}
#[test]
fn retained_modifier_sign_no_aura_and_source_rules() {
    for (amount, positive, negative) in [
        (10, 82, -67),
        (-10, 67, -82),
        (0, 75, -75),
        (100, 150, 0),
        (-100, 0, -150),
        (101, 150, 0),
        (-101, 0, -150),
    ] {
        let cat = bound_catalog(rows(0), unit_config(), &[amount]);
        for source in [Source::Kill, Source::Quest, Source::Spell] {
            for (reputation, expected) in [(75, positive), (-75, negative), (0, 0)] {
                let input = GainInput {
                    source,
                    reputation,
                    faction: 100,
                    player_level: 60,
                    target_level: 0,
                    no_aura_bonus: false,
                };
                assert_eq!(cat.calculate_gain(input).unwrap(), expected);
                assert_eq!(
                    cat.calculate_gain(GainInput {
                        no_aura_bonus: true,
                        ..input
                    })
                    .unwrap(),
                    reputation
                );
            }
        }
    }
}
#[test]
fn amplified_quest_gain_is_applied_once_before_spillover_and_never_to_set() {
    for (reward, expected, spill) in [(75, 82, 20), (250, 275, 68), (-75, -67, -16)] {
        let mut rs = rows(0);
        for row in &mut rs {
            row.faction.base_values = [0; 4];
            row.faction.flags = [0; 4];
        }
        rs[0].spillover = Resolved::Present([
            Spillover {
                faction: 101,
                rate: float(0x3e800000),
                maximum_rank: Rank::Exalted,
            },
            Spillover {
                faction: 102,
                rate: float(0x3e800000),
                maximum_rank: Rank::Exalted,
            },
            Spillover {
                faction: 103,
                rate: float(0x3e800000),
                maximum_rank: Rank::Exalted,
            },
            Spillover {
                faction: 0,
                rate: SpellFloat::ZERO,
                maximum_rank: Rank::Hated,
            },
        ]);
        let cat = bound_catalog(rs, unit_config(), &[10]);
        let initial = cat.initialize(identity()).unwrap();
        let t = initial
            .reward_quest(
                &cat,
                QuestReward {
                    factions: [100, 0, 0, 0, 0],
                    values: [reward, 0, 0, 0, 0],
                    faction_flags: 0,
                    quest_level: 60,
                },
                60,
            )
            .unwrap();
        assert_eq!(t.state.current(&cat, 100).unwrap(), expected);
        for faction in 101..104 {
            assert_eq!(t.state.current(&cat, faction).unwrap(), spill);
        }
        let set = initial
            .reduce(
                &cat,
                Command::Set {
                    faction: 100,
                    standing: reward,
                    incremental: true,
                    no_spillover: true,
                },
            )
            .unwrap();
        assert_eq!(set.state.current(&cat, 100).unwrap(), reward);
    }
}
#[test]
fn fixed_aura_binding_is_explicit_bounded_and_source_identified() {
    let mut evidence = audit();
    evidence.general_reputation_auras = Evidence::Present;
    let build = |evidence: Audit, effects| {
        Catalog::new_with_general_reputation_auras(
            rows(0),
            vec![],
            unit_config(),
            evidence,
            effects,
        )
    };
    assert!(Catalog::new(rows(0), vec![], unit_config(), evidence.clone()).is_err());
    assert!(build(evidence.clone(), vec![]).is_err());
    assert!(build(audit(), vec![aura(0, 10)]).is_err());
    let mut unknown = evidence.clone();
    unknown.general_reputation_auras = Evidence::Unknown;
    assert!(build(unknown, vec![aura(0, 10)]).is_err());
    for excluded in 0..3 {
        for value in [Evidence::Present, Evidence::Unknown] {
            let mut e = evidence.clone();
            match excluded {
                0 => e.faction_reputation_auras = value,
                1 => e.forced_reactions = value,
                _ => e.reputation_scripts = value,
            }
            assert!(build(e, vec![aura(0, 10)]).is_err());
        }
    }
    assert!(build(evidence.clone(), vec![aura(0, 10), aura(0, 20)]).is_err());
    let mut other_effect = aura(0, 20);
    other_effect.effect_index = 1;
    assert!(build(evidence.clone(), vec![aura(0, 10), other_effect]).is_ok());
    for field in [
        "spell_id",
        "effect_index",
        "aura_type",
        "attributes",
        "attributes_ex3",
        "caster_aura_state",
        "aura_interrupt_flags",
        "duration_ms",
        "source_spell_hash",
    ] {
        let mut encoded = serde_json::to_value(aura(0, 10)).unwrap();
        encoded[field] = match field {
            "spell_id" | "attributes" | "duration_ms" => serde_json::json!(0),
            "effect_index" => serde_json::json!(3),
            "aura_type" => serde_json::json!(190),
            "source_spell_hash" => serde_json::to_value([0u8; 32]).unwrap(),
            _ => serde_json::json!(1),
        };
        let changed = serde_json::from_value(encoded).unwrap();
        assert!(build(evidence.clone(), vec![changed]).is_err(), "{field}");
    }
    let effects: Vec<_> = (0..MAX_GENERAL_REPUTATION_AURAS)
        .map(|i| aura(i, 0))
        .collect();
    assert!(build(evidence.clone(), effects.clone()).is_ok());
    let mut too_many = effects;
    too_many.push(aura(MAX_GENERAL_REPUTATION_AURAS, 0));
    assert!(build(evidence, too_many).is_err());
}
#[test]
fn signed_aura_sum_checks_every_source_order_prefix() {
    let mut evidence = audit();
    evidence.general_reputation_auras = Evidence::Present;
    let build = |amounts: &[i32]| {
        Catalog::new_with_general_reputation_auras(
            rows(0),
            vec![],
            unit_config(),
            evidence.clone(),
            amounts
                .iter()
                .enumerate()
                .map(|(i, &n)| aura(i, n))
                .collect(),
        )
    };
    for amounts in [&[i32::MAX, 1, -1][..], &[i32::MIN, -1, 1][..]] {
        assert_eq!(
            build(amounts).unwrap_err(),
            Error::Numeric("general_reputation_aura_sum")
        );
    }
    assert!(build(&[i32::MAX, -1, 1]).is_ok());
    assert!(build(&[i32::MIN, 1, -1]).is_ok());
    let cat = build(&[i32::MAX]).unwrap();
    assert!(cat
        .calculate_gain(GainInput {
            source: Source::Spell,
            reputation: i32::MAX,
            faction: 100,
            player_level: 1,
            target_level: 0,
            no_aura_bonus: false
        })
        .is_err());
}
#[test]
fn aura_hash_binds_order_and_every_field_and_decoding_requires_list() {
    let cat = bound_catalog(rows(0), unit_config(), &[7, 3]);
    let state = cat.initialize(identity()).unwrap();
    let encoded = serde_json::to_value(&cat).unwrap();
    let restored: Catalog = serde_json::from_value(encoded.clone()).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored, cat);
    let mut missing = encoded.clone();
    missing
        .as_object_mut()
        .unwrap()
        .remove("general_reputation_auras");
    assert!(serde_json::from_value::<Catalog>(missing).is_err());
    let mut reversed = encoded.clone();
    reversed["general_reputation_auras"]
        .as_array_mut()
        .unwrap()
        .reverse();
    let reversed: Catalog = serde_json::from_value(reversed).unwrap();
    reversed.validate().unwrap();
    assert_ne!(cat.hash(), reversed.hash());
    assert!(state.validate(&reversed).is_err());
    for field in [
        "spell_id",
        "effect_index",
        "aura_type",
        "misc_value",
        "amount",
        "attributes",
        "attributes_ex3",
        "caster_aura_state",
        "aura_interrupt_flags",
        "duration_ms",
        "source_spell_hash",
    ] {
        let mut changed = encoded.clone();
        let value = &mut changed["general_reputation_auras"][0][field];
        if field == "source_spell_hash" {
            *value = serde_json::to_value([2u8; 32]).unwrap();
        } else {
            *value = serde_json::json!(value.as_i64().unwrap() + 1);
        }
        let changed: Catalog = serde_json::from_value(changed).unwrap();
        assert_ne!(cat.hash(), changed.hash(), "{field}");
        assert!(state.validate(&changed).is_err(), "{field}");
    }
}
#[test]
fn amplified_callback_failure_keeps_both_states_unchanged() {
    let cat = bound_catalog(rows(0), unit_config(), &[10]);
    let state = cat.initialize(identity()).unwrap();
    let before = state.clone();
    let host = ReentrantHost {
        triggered: false,
        level: 60,
        reject: true,
    };
    let original = host.clone();
    let result = state.reward_quest_with_host(
        &cat,
        QuestReward {
            factions: [100, 0, 0, 0, 0],
            values: [75, 0, 0, 0, 0],
            faction_flags: 0,
            quest_level: 60,
        },
        &host,
    );
    assert_eq!(result.unwrap_err(), Error::Callback("authored_failure"));
    assert_eq!(state, before);
    assert_eq!(host, original);
}

#[test]
fn canonical_fixed_aura_catalog_hash() {
    let mut evidence = audit();
    evidence.general_reputation_auras = Evidence::Present;
    let cat = Catalog::new_with_general_reputation_auras(
        vec![],
        vec![],
        unit_config(),
        evidence,
        vec![aura(0, 10)],
    )
    .unwrap();
    // Independent SHA256 of the documented little-endian field stream.
    assert_eq!(
        cat.hash(),
        [
            0x8b, 0x2e, 0xac, 0xd8, 0x4b, 0x08, 0xa6, 0x1a, 0xe8, 0xf9, 0x3d, 0xd0, 0x89, 0x7d,
            0x50, 0x3c, 0x88, 0xbf, 0xff, 0xb0, 0x99, 0xeb, 0xa3, 0x4d, 0xdf, 0xe7, 0xaf, 0x1c,
            0xa0, 0x0f, 0x44, 0x93,
        ]
    );
}
