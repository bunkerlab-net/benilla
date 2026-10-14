// SPDX-License-Identifier: GPL-3.0-or-later
// Original synthetic tests; no native database rows or production no-op host.
use super::*;
use crate::player_progression as pg;
use crate::starter_init::{Attributes, ModifierSnapshot};
const GUID: u64 = 0xf130_0000_0100_0001;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Probe {
    calls: usize,
    fail_at: Option<usize>,
    block_preflight: bool,
}
impl Probe {
    fn called(&mut self) -> Result<()> {
        self.calls += 1;
        if self.fail_at == Some(self.calls) {
            Err(Error::Host("injected failure after staged mutation"))
        } else {
            Ok(())
        }
    }
}
impl Host for Probe {
    fn preflight(&self, _: &Catalog, _: &Snapshot) -> Result<()> {
        Ok(())
    }
    fn apply(&mut self, _: &pg::Event, _: &Snapshot) -> Result<()> {
        self.called()
    }
}
impl KillHost for Probe {
    fn preflight_kill(&self, _: &Victim, _: &Rules, _: &KillContext, _: &RestState) -> Result<()> {
        if self.block_preflight {
            Err(Error::Host("unverified death/row/clock"))
        } else {
            Ok(())
        }
    }
    fn apply_kill(&mut self, _: &Event, _: &Snapshot, _: &RestState) -> Result<()> {
        self.called()
    }
}
fn audit() -> pg::Audit {
    pg::Audit {
        group_or_raid: Evidence::Absent,
        pet_or_controlled_unit: Evidence::Absent,
        used_talents: Evidence::Absent,
        shapeshift: Evidence::Absent,
        level_change_scripts: Evidence::Absent,
        dynamic_or_level_scaling_modifiers: Evidence::Absent,
        resource_or_stat_aura_hooks: Evidence::Absent,
        unsupported_item_effects: Evidence::Absent,
        non_mana_power_modifiers: Evidence::Absent,
        extra_spell_crit_or_regen_modifiers: Evidence::Absent,
    }
}
fn attributes(level: u32) -> Attributes {
    Attributes(std::array::from_fn(|i| {
        12 + 2 * level as i32 + 3 * i as i32
    }))
}
fn catalog(cap: u32) -> Catalog {
    Catalog::new(
        1,
        8,
        pg::Rules::from_classic_config(cap, false).unwrap(),
        (1..=cap)
            .map(|level| pg::LevelStatsRow {
                race: 1,
                class: 8,
                level,
                attributes: attributes(level),
            })
            .collect(),
        (1..=cap)
            .map(|level| pg::ClassLevelStatsRow {
                class: 8,
                level,
                base_health: 40 + 7 * level,
                base_mana: 55 + 11 * level,
            })
            .collect(),
        (1..=cap)
            .map(|level| pg::XpRow {
                level,
                xp_for_next_level: if level < cap { 100 + 37 * level } else { 0 },
            })
            .collect(),
        vec![],
    )
    .unwrap()
}
fn state(catalog: &Catalog, level: u32, alive: bool, xp: u32) -> State {
    let neutral = [0, ONE, 0, ONE];
    let modifiers = ModifierSnapshot {
        create: attributes(level),
        create_health: 40 + 7 * level,
        create_mana: 55 + 11 * level,
        attributes: [neutral; 5],
        health: neutral,
        mana: neutral,
        resistances: [neutral; 7],
    };
    let numeric =
        pg::NumericState::from_modifiers(level, &modifiers, u32::from(alive), [1, 7, 0, 1, 0])
            .unwrap();
    State::restore(
        catalog,
        Snapshot {
            policy: pg::POLICY_ID.into(),
            catalog_identity: catalog.identity(),
            race: 1,
            class: 8,
            level,
            alive,
            xp,
            next_level_xp: if level < catalog.rules().max_player_level {
                100 + 37 * level
            } else {
                0
            },
            played_time_level: 77,
            session_level: level,
            free_talent_points: 0,
            used_talent_points: 0,
            modifiers,
            numeric,
            skills: vec![],
            audit: audit(),
        },
    )
    .unwrap()
}
fn victim() -> Victim {
    Victim {
        guid: GUID,
        entry: 60_001,
        level: 1,
        experience_multiplier_bits: ONE,
        max_health: 100,
        damage_by_others: 0,
        is_totem: false,
        is_pet: false,
        is_no_xp: false,
        is_critter: false,
        is_elite: false,
        player_controlled: false,
    }
}
fn context() -> KillContext {
    KillContext {
        death_event: 1,
        player_guid: 1,
        responsible_player_guid: Some(1),
        original_loot_recipient_guid: Some(1),
        group_loot_recipient: Evidence::Absent,
        civilian_for_target: Evidence::Absent,
        racial_leader: Evidence::Absent,
        reputation: ReputationAdmission::ProvenAbsentOnKillRow { entry: 60_001 },
        clock: OnlineRestContract {
            rule: OnlineRestRule::SimulationContinuationWithoutSourceLogout,
            offline_intervals: Evidence::Absent,
            entered_rest_area: Evidence::Absent,
        },
    }
}
fn packet(victim_guid: u64, total: u32, base: u32, rate: u32) -> String {
    victim_guid
        .to_le_bytes()
        .into_iter()
        .chain(total.to_le_bytes())
        .chain([0])
        .chain(base.to_le_bytes())
        .chain(rate.to_le_bytes())
        .map(|v| format!("{v:02x}"))
        .collect()
}
fn trace(observations: &[Observation]) -> String {
    let mut out = Vec::new();
    for ob in observations {
        let tag = match ob.event {
            Event::RestStateByte(_) => "rest_byte",
            Event::RestExperience(_) => "rest_xp",
            Event::KillXpLog { .. } => "packet",
            Event::Progression(pg::Event::LevelUpPacket { .. }) => "level",
            Event::Progression(pg::Event::Xp(_)) => "xp",
            _ => continue,
        };
        out.push(format!(
            "{tag},{},{},{},{},{},{}",
            ob.view.level,
            ob.view.xp,
            ob.view.next_level_xp,
            ob.rest.bonus_bits,
            ob.rest.client_state,
            ob.rest.experience
        ));
        if let Event::KillXpLog {
            victim_guid,
            total_xp,
            base_xp,
            group_rate_bits,
        } = ob.event
        {
            out.push(packet(victim_guid, total_xp, base_xp, group_rate_bits));
        }
    }
    if out.is_empty() {
        "-".into()
    } else {
        out.join(";")
    }
}
#[test]
fn extracted_source_gain_rest_and_victim_xp_fixture() {
    let cat = catalog(9);
    let host = Probe::default();
    let mut count = [0; 4];
    for (line, raw) in include_str!("../../tests/fixtures/kill_xp_cmangos.tsv")
        .lines()
        .enumerate()
    {
        if raw.starts_with('#') {
            continue;
        }
        let cols: Vec<_> = raw.split('\t').collect();
        let n = |i: usize| cols[i].parse::<u32>().unwrap();
        match cols[0] {
            "G" => {
                let mut v = victim();
                v.level = n(2);
                v.experience_multiplier_bits = n(3);
                v.max_health = n(5);
                v.damage_by_others = n(6);
                v.is_no_xp = n(7) != 0;
                v.is_critter = n(8) != 0;
                assert_eq!(base_gain_bits(n(1), n(2)).unwrap(), n(9), "base line{line}");
                assert_eq!(
                    gain(
                        n(1),
                        v,
                        Rules {
                            kill_rate_bits: n(4)
                        }
                    )
                    .unwrap(),
                    n(10),
                    "gain line{line}"
                );
                count[0] += 1;
            }
            "X" => {
                let original = state(&cat, n(1), n(2) != 0, n(3));
                let rest = RestState {
                    bonus_bits: n(5),
                    client_state: n(6) as u8,
                    experience: u32_cast(scalar(n(5)).unwrap(), Round::TowardZero).unwrap(),
                };
                let tx = give_xp_inner(&cat, &original, rest, &host, n(4), GUID).unwrap();
                assert_eq!(
                    [
                        tx.state.view().level,
                        tx.state.view().xp,
                        tx.state.view().next_level_xp,
                        tx.rest.bonus_bits,
                        tx.rest.client_state.into(),
                        tx.rest.experience
                    ],
                    [n(8), n(9), n(10), n(11), n(12), n(13)],
                    "state line{line}"
                );
                assert_eq!(trace(&tx.events), cols[14], "trace line{line}");
                count[1] += 1;
            }
            "R" => {
                let interval = match n(3) {
                    0 => RestInterval::Online,
                    1 => RestInterval::OfflineRestPlace,
                    _ => RestInterval::OfflineWilderness,
                };
                assert_eq!(
                    compute_rest_bits(
                        cols[1].parse().unwrap(),
                        n(2),
                        interval,
                        RestRules {
                            ingame_bits: n(4),
                            offline_rest_place_bits: n(5),
                            offline_wilderness_bits: n(6)
                        }
                    )
                    .unwrap(),
                    n(7),
                    "rest line{line}"
                );
                count[2] += 1;
            }
            "S" => {
                let old = RestState {
                    client_state: n(2) as u8,
                    ..RestState::fresh_player()
                };
                let (rest, events) = set_rest_bonus(old, n(1), n(3), n(4), n(5)).unwrap();
                assert_eq!(
                    [rest.bonus_bits, rest.client_state.into(), rest.experience],
                    [n(6), n(7), n(8)],
                    "set rest line{line}"
                );
                assert_eq!(events.len(), cols[9].split(';').count());
                count[3] += 1;
            }
            _ => panic!("unrecognized fixture row"),
        }
    }
    assert_eq!(count, [26_649, 2_594, 576, 176]);
}
#[test]
fn ordinary_kill_awards_real_positive_xp_and_binds_source_proofs() {
    let cat = catalog(9);
    let state = state(&cat, 1, true, 100);
    let host = Probe::default();
    let tx = award(
        &cat,
        &state,
        RestState::fresh_player(),
        &host,
        Rules {
            kill_rate_bits: ONE,
        },
        victim(),
        context(),
    )
    .unwrap();
    assert_eq!(tx.state.view().level, 2);
    assert_eq!(tx.state.view().xp, 13);
    assert!(matches!(
        tx.events[2].event,
        Event::KillXpLog {
            total_xp: 50,
            base_xp: 50,
            ..
        }
    ));
    assert!(!tx
        .events
        .iter()
        .any(|e| matches!(e.event, Event::Progression(pg::Event::NonKillXpLog { .. }))));
    let mut blocked = context();
    blocked.reputation = ReputationAdmission::Unknown;
    assert!(award(
        &cat,
        &state,
        RestState::fresh_player(),
        &host,
        Rules {
            kill_rate_bits: ONE
        },
        victim(),
        blocked
    )
    .is_err());
    for field in 0..5 {
        let mut blocked = context();
        match field {
            0 => blocked.clock.entered_rest_area = Evidence::Unknown,
            1 => blocked.clock.offline_intervals = Evidence::Present,
            2 => blocked.group_loot_recipient = Evidence::Unknown,
            3 => blocked.civilian_for_target = Evidence::Present,
            _ => blocked.racial_leader = Evidence::Unknown,
        }
        assert!(blocked
            .validate(victim(), RestState::fresh_player())
            .is_err());
    }
    let mut blocked = context();
    blocked.original_loot_recipient_guid = Some(2);
    assert!(blocked
        .validate(victim(), RestState::fresh_player())
        .is_err());
    let mut zero = victim();
    zero.is_no_xp = true;
    assert!(award(
        &cat,
        &state,
        RestState::fresh_player(),
        &host,
        Rules {
            kill_rate_bits: ONE
        },
        zero,
        context()
    )
    .unwrap()
    .events
    .is_empty());
}
#[test]
fn complete_kill_transaction_rolls_back_every_callback_failure() {
    let cat = catalog(9);
    let original = state(&cat, 1, true, 100);
    let rest = RestState::fresh_player();
    let mut v = victim();
    v.experience_multiplier_bits = 10_f32.to_bits();
    let count = award(
        &cat,
        &original,
        rest,
        &Probe::default(),
        Rules {
            kill_rate_bits: ONE,
        },
        v,
        context(),
    )
    .unwrap()
    .host
    .calls;
    for fail in 1..=count {
        let host = Probe {
            fail_at: Some(fail),
            ..Probe::default()
        };
        let before = host.clone();
        let snapshot = original.snapshot();
        assert!(award(
            &cat,
            &original,
            rest,
            &host,
            Rules {
                kill_rate_bits: ONE
            },
            v,
            context()
        )
        .is_err());
        assert_eq!(host, before);
        assert_eq!(original.snapshot(), snapshot);
        assert_eq!(rest, RestState::fresh_player());
    }
}
#[test]
fn serde_roundtrip_and_numeric_rejection_do_not_invent_clock_or_rates() {
    let context = context();
    assert_eq!(
        serde_json::from_str::<KillContext>(&serde_json::to_string(&context).unwrap()).unwrap(),
        context
    );
    assert!(serde_json::from_str::<OnlineRestContract>(
        r#"{"offline_intervals":"Absent","entered_rest_area":"Absent"}"#
    )
    .is_err());
    for raw in [0x7f80_0000, 0x7fc0_0000, 1, 0xbf80_0000] {
        assert!(Rules {
            kill_rate_bits: raw
        }
        .validate()
        .is_err());
    }
    assert!(gain(
        1,
        victim(),
        Rules {
            kill_rate_bits: 0x7f7f_ffff
        }
    )
    .is_err());
    assert!(context
        .validate(
            victim(),
            RestState {
                bonus_bits: 1_f32.to_bits(),
                client_state: 2,
                experience: 1
            }
        )
        .is_err());
}
