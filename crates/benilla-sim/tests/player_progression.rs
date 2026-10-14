// SPDX-License-Identifier: GPL-3.0-or-later
// Synthetic arithmetic probes, not native database rows or a production host.
use benilla_sim::{
    player_progression::*,
    starter_init::{Attributes, ModifierSnapshot, Rows, SkillLineRow, SkillRaceClassRow},
};
const NEUTRAL: [u32; 4] = [0, 0x3f800000, 0, 0x3f800000];
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ProbeHost {
    calls: Vec<Event>,
    fail_at: Option<usize>,
    block_preflight: bool,
}
impl Host for ProbeHost {
    fn preflight(&self, _: &Catalog, _: &Snapshot) -> Result<()> {
        if self.block_preflight {
            Err(Error::Host("unresolved native host"))
        } else {
            Ok(())
        }
    }
    fn apply(&mut self, event: &Event, _: &Snapshot) -> Result<()> {
        self.calls.push(event.clone());
        if self.fail_at == Some(self.calls.len() - 1) {
            Err(Error::Host("injected after mutation"))
        } else {
            Ok(())
        }
    }
}
fn audit() -> Audit {
    Audit {
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
fn rows(level: u32, profile: u32) -> (LevelStatsRow, ClassLevelStatsRow, XpRow) {
    let attributes = Attributes(std::array::from_fn(|i| {
        12 + 2 * level as i32 + 3 * i as i32
    }));
    (
        LevelStatsRow {
            race: 1,
            class: 8,
            level,
            attributes,
        },
        ClassLevelStatsRow {
            class: 8,
            level,
            base_health: if profile != 1 {
                40 + 7 * level
            } else {
                200 - 7 * level
            },
            base_mana: if profile != 1 {
                55 + 11 * level
            } else {
                300 - 11 * level
            },
        },
        XpRow {
            level,
            xp_for_next_level: 100 + 37 * level,
        },
    )
}
fn skill(case: u32, level: u32) -> (SkillState, SkillDependency) {
    let (id, category, value, maximum, step) = match case {
        2 => (60002, 8, 1, 1, 0),
        3 => (60003, 7, 3, (level * 5) as u16, 1),
        4 => (60004, 10, 7, 300, 0),
        5 => (60005, 11, 3, 75, 1),
        6 => (60006, 12, 3, (level * 5) as u16, 0),
        10 => (95, 6, 3, (level * 5) as u16, 0),
        12 => (473, 6, 1, 1, 0),
        13 => (60001, 6, 3, 1, 2),
        _ => (60001, 6, 3, (level * 5) as u16, 2),
    };
    (
        SkillState {
            id,
            position: 0,
            value,
            maximum,
            step,
            permanent_bonus: 0,
            temporary_bonus: 0,
            status: match case {
                7 => SkillStatus::Deleted,
                11 => SkillStatus::New,
                _ => SkillStatus::Unchanged,
            },
        },
        SkillDependency {
            skill: id,
            line: if case == 8 {
                None
            } else {
                Some(SkillLineRow { id, category })
            },
            race_class: Rows::Complete(vec![SkillRaceClassRow {
                row_id: 1,
                skill: id,
                race_mask: if case == 9 { 2 } else { 1 },
                class_mask: 128,
                flags: if matches!(case, 1 | 9 | 11 | 13) {
                    16
                } else {
                    0
                },
                tier: 0,
            }]),
        },
    )
}
fn catalog(profile: u32, cap: u32, maximize: bool, _global: u16, case: u32) -> Catalog {
    let rows: Vec<_> = (1..=9.min(cap))
        .map(|l| {
            let mut r = rows(l, profile);
            if l >= cap {
                r.2.xp_for_next_level = 0;
            }
            r
        })
        .collect();
    Catalog::new(
        1,
        8,
        Rules::from_classic_config(cap, maximize).unwrap(),
        rows.iter().map(|r| r.0).collect(),
        rows.iter().map(|r| r.1).collect(),
        rows.iter().map(|r| r.2).collect(),
        vec![skill(case, 1).1],
    )
    .unwrap()
}
fn snapshot(cat: &Catalog, level: u32, profile: u32, case: u32, alive: bool, xp: u32) -> Snapshot {
    let (a, c, x) = rows(level, profile);
    let mut modifiers = ModifierSnapshot {
        create: a.attributes,
        create_health: c.base_health,
        create_mana: c.base_mana,
        attributes: [NEUTRAL; 5],
        health: NEUTRAL,
        mana: NEUTRAL,
        resistances: [NEUTRAL; 7],
    };
    let bits = |v: [f32; 4]| v.map(f32::to_bits);
    match profile {
        2 => {
            for (i, row) in modifiers.attributes.iter_mut().enumerate() {
                *row = bits([3.0 + i as f32, 1.05, -2.0, 1.17]);
            }
            modifiers.health = bits([7.0, 1.10, -3.0, 1.07]);
            modifiers.mana = bits([11.0, 0.95, 5.0, 1.03]);
            modifiers.resistances[0] = bits([13.0, 1.0, 7.0, 1.15]);
        }
        3 => {
            modifiers.attributes = [
                bits([0.0, 1.0, -100.0, 1.0]),
                bits([0.0, 1.0, 0.0, 0.0]),
                bits([1.25, 0.5, 2.25, 1.125]),
                bits([-3.0, 1.01, 0.5, 0.93]),
                bits([0.0, 1.0, 0.0, -0.5]),
            ];
            modifiers.health = bits([1.25, 0.75, -1.5, 1.25]);
            modifiers.mana = bits([2.5, 1.125, -1.25, 0.875]);
            modifiers.resistances[0] = bits([5.5, 0.9, -3.25, 1.375]);
        }
        4 => {
            modifiers.health = bits([0.5, 1.0, 0.0, 1.0]);
            modifiers.mana = bits([-0.5, 1.0, 0.0, 1.0]);
        }
        _ => {}
    }
    let numeric = NumericState::from_modifiers(
        level,
        &modifiers,
        if alive { 1 } else { 0 },
        [1, 7, 0, 1, 0],
    )
    .unwrap();
    Snapshot {
        policy: POLICY_ID.into(),
        catalog_identity: cat.identity(),
        race: 1,
        class: 8,
        level,
        alive,
        xp,
        next_level_xp: if level >= cat.rules().max_player_level {
            0
        } else {
            x.xp_for_next_level
        },
        played_time_level: 77,
        session_level: level,
        free_talent_points: 0,
        used_talent_points: 0,
        modifiers,
        numeric,
        skills: vec![skill(case, level).0],
        audit: audit(),
    }
}
fn state(cat: &Catalog, level: u32, profile: u32, case: u32, alive: bool, xp: u32) -> State {
    State::restore(cat, snapshot(cat, level, profile, case, alive, xp)).unwrap()
}
fn trace(event: &Event) -> String {
    match event {
        Event::NonKillXpLog { .. } => "xp_log".into(),
        Event::LevelUpPacket { .. } => "level_packet".into(),
        Event::NextLevelXp(_) => "next_xp".into(),
        Event::PlayedTimeReset => "played_reset".into(),
        Event::Level(_) => "level".into(),
        Event::SkillValue { .. } => "skill_value".into(),
        Event::SkillTrainedSpells { .. } => "skill_trained".into(),
        Event::SkillDefenseBonuses => "defense".into(),
        Event::CreateStat { index, .. } => format!("create_stat{index}"),
        Event::CreateHealth(_) => "create_health".into(),
        Event::CreateMana(_) => "create_mana".into(),
        Event::InitTalentForLevel => "talents".into(),
        Event::Stat { index, .. } => format!("stat{index}"),
        Event::Armor(_) => "armor".into(),
        Event::Resistance { school, .. } => format!("resist{school}"),
        Event::MaxHealth(_) => "max_health".into(),
        Event::MaxPower { power, .. } => format!("max_power{power}"),
        Event::Health(_) => "health".into(),
        Event::Power { power, .. } => format!("power{power}"),
        Event::SessionLevel(_) => "session_level".into(),
        Event::Xp(_) => "xp".into(),
        Event::Hook(h) => match h {
            StatHook::AttackPowerAndDamage { ranged: false } => "attack_melee".into(),
            StatHook::AttackPowerAndDamage { ranged: true } => "attack_ranged".into(),
            StatHook::AllCritPercentages => "crit".into(),
            StatHook::AllSpellCritChances => "spell_crit".into(),
            StatHook::DefenseBonuses => "defense".into(),
            StatHook::SpellDamageBonus => "spell_damage".into(),
            StatHook::ManaRegen => "mana_regen".into(),
            StatHook::WeaponHitChance { attack } => format!("hit{attack}"),
        },
    }
}
fn csv<T: std::fmt::Display>(values: impl IntoIterator<Item = T>) -> String {
    values
        .into_iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(",")
}
#[test]
fn extracted_cpp_source_progression() {
    let mut count = 0;
    let mut invalid_rage = 0;
    for line in include_str!("fixtures/player_progression_cmangos.tsv")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let f: Vec<_> = line.split('\t').collect();
        let n = |i: usize| f[i].parse::<u32>().unwrap();
        let (level, cap, old_xp, amount, alive, profile, maximize, global, case) = (
            n(1),
            n(2),
            n(3),
            n(4),
            n(5) != 0,
            n(6),
            n(7) != 0,
            n(8) as u16,
            n(9),
        );
        assert_eq!(
            global,
            Rules::from_classic_config(cap, maximize)
                .unwrap()
                .global_skill_max,
            "{line}"
        );
        let cat = catalog(profile, cap, maximize, global, case);
        let mut initial = snapshot(&cat, level, profile, case, alive, old_xp);
        initial.numeric.health = n(10);
        initial.numeric.powers[0] = n(11);
        initial.numeric.powers[1] = n(12);
        initial.numeric.powers[3] = n(13);
        if initial.numeric.powers[1] > 1000 {
            invalid_rage += 1;
            assert!(State::restore(&cat, initial).is_err());
            count += 1;
            continue;
        }
        let initial = State::restore(&cat, initial).unwrap_or_else(|e| panic!("{e}: {line}"));
        let tx = if f[0] == "X" {
            give_xp(&cat, &initial, &ProbeHost::default(), amount)
        } else if f[0] == "L" {
            give_level(&cat, &initial, &ProbeHost::default(), amount)
        } else {
            update_skills_for_level(&cat, &initial, &ProbeHost::default(), amount != 0)
        }
        .unwrap_or_else(|e| panic!("{e}: {line}"));
        let s = tx.state.view();
        assert_eq!(
            [
                s.level,
                s.xp,
                s.next_level_xp,
                s.modifiers.create_health,
                s.modifiers.create_mana
            ],
            std::array::from_fn(|i| n(14 + i)),
            "{line}"
        );
        assert_eq!(csv(s.modifiers.create.0), f[19], "{line}");
        assert_eq!(csv(s.numeric.attributes.0), f[20], "{line}");
        assert_eq!(
            [
                s.numeric.health,
                s.numeric.max_health,
                s.numeric.powers[0],
                s.numeric.max_powers[0],
                s.numeric.powers[1],
                s.numeric.max_powers[1],
                s.numeric.powers[3],
                s.numeric.max_powers[3],
                s.numeric.resistances[0] as u32,
                s.skills[0].value.into(),
                s.skills[0].maximum.into(),
                s.skills[0].step.into()
            ],
            std::array::from_fn(|i| n(21 + i)),
            "{line}"
        );
        assert_eq!(
            match s.skills[0].status {
                SkillStatus::Unchanged => 0,
                SkillStatus::Changed => 1,
                SkillStatus::New => 2,
                SkillStatus::Deleted => 3,
            },
            n(33),
            "{line}"
        );
        assert_eq!(
            [s.played_time_level, s.session_level, s.free_talent_points],
            [n(34), n(35), n(36)],
            "{line}"
        );
        let log = tx
            .events
            .iter()
            .find_map(|e| {
                if let Event::NonKillXpLog { xp } = e.event {
                    Some(xp.to_string())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "-".into());
        assert_eq!(log, f[37], "{line}");
        let packets = tx
            .events
            .iter()
            .filter_map(|e| {
                if let Event::LevelUpPacket { words } = e.event {
                    Some(csv(words))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(";");
        assert_eq!(
            if packets.is_empty() { "-" } else { &packets },
            f[38],
            "{line}"
        );
        let expected_trace = if f[39] == "-" { "" } else { f[39] };
        assert_eq!(
            tx.events
                .iter()
                .map(|e| trace(&e.event))
                .collect::<Vec<_>>()
                .join(","),
            expected_trace,
            "{line}"
        );
        State::restore(&cat, tx.state.snapshot()).unwrap();
        count += 1;
    }
    assert_eq!(count, 881);
    assert_eq!(invalid_rage, 3);
}
#[test]
fn zero_dead_cap_overflow_and_surplus_follow_source() {
    for (alive, cap, xp) in [(true, 60, 0), (false, 60, 100), (true, 1, 100)] {
        let cat = catalog(0, cap, false, 300, 0);
        let s = state(&cat, 1, 0, 0, alive, 7);
        let h = ProbeHost::default();
        let tx = give_xp(&cat, &s, &h, xp).unwrap();
        assert_eq!(tx.state, s);
        assert_eq!(tx.host, h);
        assert!(tx.events.is_empty());
    }
    let cat = catalog(0, 60, false, 300, 0);
    let s = state(&cat, 1, 0, 0, true, u32::MAX - 10);
    let tx = give_xp(&cat, &s, &ProbeHost::default(), 20).unwrap();
    assert_eq!((tx.state.view().level, tx.state.view().xp), (1, 9));
    assert_eq!(tx.events.len(), 2);
    let cat = catalog(0, 2, false, 300, 0);
    let s = state(&cat, 1, 0, 0, true, 0);
    let tx = give_xp(&cat, &s, &ProbeHost::default(), 10_000).unwrap();
    assert_eq!((tx.state.view().level, tx.state.view().xp), (2, 9863));
    let again = give_xp(&cat, &tx.state, &tx.host, 9000).unwrap();
    assert_eq!(again.state, tx.state);
    assert!(again.events.is_empty());
}
#[test]
fn callback_views_and_every_host_failure_are_transactional() {
    let cat = catalog(0, 60, true, 300, 10);
    let original = state(&cat, 1, 0, 10, true, 17);
    let h = ProbeHost::default();
    let tx = give_xp(&cat, &original, &h, 500).unwrap();
    assert_eq!(tx.state.view().level, 3);
    let skill = tx
        .events
        .iter()
        .find(|e| matches!(e.event, Event::SkillTrainedSpells { .. }))
        .unwrap();
    assert_eq!(
        (
            skill.view.level,
            skill.view.xp,
            skill.view.skills[0].maximum
        ),
        (2, 17, 10)
    );
    assert_eq!(
        skill.view.modifiers.create,
        original.view().modifiers.create
    );
    let init = tx
        .events
        .iter()
        .find(|e| e.event == Event::InitTalentForLevel)
        .unwrap();
    assert_eq!(init.view.modifiers.create, rows(2, 0).0.attributes);
    assert_eq!(
        init.view.numeric.attributes,
        original.view().numeric.attributes
    );
    let update = tx
        .events
        .iter()
        .find(|e| e.event == Event::Hook(StatHook::AttackPowerAndDamage { ranged: false }))
        .unwrap();
    assert_eq!(update.view.numeric.attributes, rows(2, 0).0.attributes);
    assert_eq!(
        update.view.numeric.max_health,
        original.view().numeric.max_health
    );
    assert_eq!(tx.events[0].view, original.snapshot());
    for fail in 0..tx.events.len() {
        let host = ProbeHost {
            fail_at: Some(fail),
            ..ProbeHost::default()
        };
        let before = host.clone();
        assert_eq!(
            give_xp(&cat, &original, &host, 500),
            Err(Error::Host("injected after mutation"))
        );
        assert_eq!(host, before);
        assert_eq!(original.view().level, 1);
    }
    let blocked = ProbeHost {
        block_preflight: true,
        ..ProbeHost::default()
    };
    assert_eq!(
        give_xp(&cat, &original, &blocked, 500),
        Err(Error::Host("unresolved native host"))
    );
}
#[test]
fn missing_levels_or_unsupported_tenth_level_roll_back_everything() {
    let cat = catalog(0, 60, true, 300, 0);
    let start = state(&cat, 1, 0, 0, true, 0);
    let host = ProbeHost::default();
    assert_eq!(
        give_xp(&cat, &start, &host, 100_000),
        Err(Error::Unsupported("mage levels outside 1–9"))
    );
    let (a, b, c) = rows(1, 0);
    let partial = Catalog::new(
        1,
        8,
        cat.rules(),
        vec![a],
        vec![b],
        vec![c],
        vec![skill(0, 1).1],
    )
    .unwrap();
    let s = state(&partial, 1, 0, 0, true, 0);
    assert!(matches!(
        give_xp(&partial, &s, &host, 137),
        Err(Error::Missing { id: 2, .. })
    ));
    assert!(host.calls.is_empty());
    assert_eq!(s.view().xp, 0);
}
#[test]
fn direct_give_level_keeps_xp_and_rejects_undefined_negative_stat_packet() {
    let cat = catalog(1, 60, false, 300, 0);
    let s = state(&cat, 1, 1, 0, false, 98);
    let tx = give_level(&cat, &s, &ProbeHost::default(), 2).unwrap();
    assert_eq!(tx.state.view().xp, 98);
    assert_eq!(tx.state.view().numeric.health, 0);
    let Event::LevelUpPacket { words } = tx.events[0].event else {
        panic!()
    };
    assert_eq!(words[1], (-7i32) as u32);
    assert_eq!(words[2], (-11i32) as u32);
    assert!(matches!(
        give_level(&cat, &tx.state, &tx.host, 1),
        Err(Error::Numeric("negative float level-up stat delta"))
    ));
    assert!(give_level(&cat, &tx.state, &tx.host, 2)
        .unwrap()
        .events
        .is_empty());
}
#[test]
fn explicit_static_modifiers_recompute_without_reusing_level_one() {
    let cat = catalog(0, 60, false, 300, 0);
    let mut s = snapshot(&cat, 1, 0, 0, true, 0);
    s.modifiers.attributes[2] = [5f32.to_bits(), 1f32.to_bits(), 0, 1.25f32.to_bits()];
    s.modifiers.health = [
        10f32.to_bits(),
        1f32.to_bits(),
        3f32.to_bits(),
        1f32.to_bits(),
    ];
    s.modifiers.resistances[0] = [0, 1f32.to_bits(), 8f32.to_bits(), 1f32.to_bits()];
    s.numeric = NumericState::from_modifiers(1, &s.modifiers, 2, [3, 7, 0, 1, 0]).unwrap();
    let old = s.numeric.clone();
    let s = State::restore(&cat, s).unwrap();
    let tx = give_level(&cat, &s, &ProbeHost::default(), 2).unwrap();
    let n = &tx.state.view().numeric;
    assert_eq!(old.attributes.0[2], 31);
    assert_eq!(n.attributes.0[2], 33);
    assert_eq!(n.max_health, 54 + 10 + 3 + 20 + 130);
    assert_eq!(n.health, n.max_health);
    assert_eq!(n.powers[0], n.max_powers[0]);
    assert_eq!(n.resistances[0], 19 * 2 + 8);
    assert_ne!(n.spell_crit_percent, old.spell_crit_percent);
    State::restore(&cat, tx.state.snapshot()).unwrap();
}
#[test]
fn restore_and_admission_fail_closed() {
    let cat = catalog(0, 60, false, 300, 0);
    let good = snapshot(&cat, 1, 0, 0, true, 0);
    let mutate: [fn(&mut Snapshot); 9] = [
        |s| s.catalog_identity[0] ^= 1,
        |s| s.next_level_xp += 1,
        |s| s.modifiers.create_health += 1,
        |s| s.numeric.max_health += 1,
        |s| s.skills.clear(),
        |s| s.audit.pet_or_controlled_unit = Evidence::Unknown,
        |s| s.audit.unsupported_item_effects = Evidence::Present,
        |s| s.free_talent_points = 1,
        |s| s.audit.extra_spell_crit_or_regen_modifiers = Evidence::Present,
    ];
    for (i, f) in mutate.into_iter().enumerate() {
        let mut s = good.clone();
        f(&mut s);
        if i == 4 {
            assert!(State::restore(&cat, s).is_ok());
        } else {
            assert!(State::restore(&cat, s).is_err(), "{i}");
        }
    }
    let mut bad = good;
    bad.modifiers.health[1] = f32::NAN.to_bits();
    assert!(State::restore(&cat, bad).is_err());
    let (a, b, c) = rows(1, 0);
    let mut dep = skill(0, 1).1;
    dep.race_class = Rows::Missing;
    assert!(matches!(
        Catalog::new(1, 8, cat.rules(), vec![a], vec![b], vec![c], vec![dep]),
        Err(Error::Missing {
            table: "SkillRaceClassInfo",
            ..
        })
    ));
    assert_ne!(cat.identity(), catalog(0, 60, true, 300, 0).identity());
}
#[test]
fn armor_round_trip_rejection_rolls_back_multilevel_progression() {
    // The extracted C++ oracle's verify_armor_round_trip asserts these exact
    // cells drift at agility 7*26459, then alter a later integer armor result.
    let mut all: Vec<_> = (1..=9).map(|l| rows(l, 0)).collect();
    for (i, row) in all.iter_mut().enumerate() {
        row.0.attributes.0[1] = match i {
            0 | 1 => 1,
            2 => 7,
            _ => 10,
        };
    }
    let cat = Catalog::new(
        1,
        8,
        Rules::from_classic_config(60, false).unwrap(),
        all.iter().map(|r| r.0).collect(),
        all.iter().map(|r| r.1).collect(),
        all.iter().map(|r| r.2).collect(),
        vec![skill(0, 1).1],
    )
    .unwrap();
    let mut initial = snapshot(&cat, 1, 0, 0, true, 0);
    initial.modifiers.create.0[1] = 1;
    initial.modifiers.attributes[1][1] = 26459f32.to_bits();
    initial.modifiers.resistances[0][2] = 0x42155800;
    initial.modifiers.resistances[0][3] = 0x3f52266f;
    initial.numeric =
        NumericState::from_modifiers(1, &initial.modifiers, 1, [1, 7, 0, 1, 0]).unwrap();
    let start = State::restore(&cat, initial.clone()).unwrap();
    let host = ProbeHost::default();
    let second = give_level(&cat, &start, &host, 2).unwrap();
    assert_eq!(second.state.view().level, 2);
    assert_eq!(second.state.view().modifiers.resistances[0][2], 0x42155800);
    // A complete level has been staged before the subsequent level's armor
    // update fails. Neither its state, callbacks nor events can be committed.
    assert_eq!(
        give_xp(&cat, &start, &host, 1000),
        Err(Error::Numeric("non-reversible armor modifier"))
    );
    assert_eq!(start.snapshot(), initial);
    assert!(host.calls.is_empty());
    assert_eq!(
        give_level(&cat, &second.state, &second.host, 3),
        Err(Error::Numeric("non-reversible armor modifier"))
    );
    let mut invalid_initial = initial.modifiers;
    invalid_initial.create.0[1] = 7;
    assert_eq!(
        NumericState::from_modifiers(3, &invalid_initial, 1, [1, 7, 0, 1, 0]),
        Err(Error::Numeric("non-reversible armor modifier"))
    );
}
#[test]
fn skills_preserve_bonus_step_status_and_first_matching_info() {
    let base = catalog(0, 60, false, 3, 0);
    let mut s = snapshot(&base, 1, 0, 0, true, 0);
    s.skills[0].value = 18;
    s.skills[0].permanent_bonus = 5;
    s.skills[0].temporary_bonus = -2;
    let s = State::restore(&base, s).unwrap();
    let tx = give_level(&base, &s, &ProbeHost::default(), 2).unwrap();
    let skill = &tx.state.view().skills[0];
    assert_eq!(
        (
            skill.value,
            skill.maximum,
            skill.step,
            skill.permanent_bonus,
            skill.temporary_bonus
        ),
        (18, 10, 2, 5, -2)
    );
    assert_eq!(skill.status, SkillStatus::Changed);
    for case in [2, 4, 5, 6, 7, 8, 12, 13] {
        let cat = catalog(0, 60, true, 300, case);
        let s = state(&cat, 1, 0, case, true, 0);
        let tx = give_level(&cat, &s, &ProbeHost::default(), 2).unwrap();
        assert_eq!(tx.state.view().skills, s.view().skills, "case{case}");
    }
    let cat = catalog(0, 60, false, 300, 11);
    let s = state(&cat, 1, 0, 11, true, 0);
    let tx = give_level(&cat, &s, &ProbeHost::default(), 2).unwrap();
    assert_eq!(tx.state.view().skills[0].status, SkillStatus::New);
    let all: Vec<_> = (1..=9).map(|l| rows(l, 0)).collect();
    let mut dep = crate::skill(0, 1).1;
    dep.race_class = Rows::Complete(vec![
        SkillRaceClassRow {
            row_id: 8,
            skill: 60001,
            race_mask: 1,
            class_mask: 128,
            flags: 16,
            tier: 0,
        },
        SkillRaceClassRow {
            row_id: 2,
            skill: 60001,
            race_mask: 0,
            class_mask: 128,
            flags: 16,
            tier: 0,
        },
        SkillRaceClassRow {
            row_id: 4,
            skill: 60001,
            race_mask: 1,
            class_mask: 128,
            flags: 0,
            tier: 0,
        },
    ]);
    let cat = Catalog::new(
        1,
        8,
        base.rules(),
        all.iter().map(|r| r.0).collect(),
        all.iter().map(|r| r.1).collect(),
        all.iter().map(|r| r.2).collect(),
        vec![dep],
    )
    .unwrap();
    let s = state(&cat, 1, 0, 0, true, 0);
    let tx = give_level(&cat, &s, &ProbeHost::default(), 2).unwrap();
    assert_eq!(tx.state.view().skills[0].value, 3);
}

#[test]
fn source_config_and_row_widths_are_not_inferred_defaults() {
    assert!(Rules::from_classic_config(0, false).is_err());
    assert!(Rules::from_classic_config(101, false).is_err());
    for (cap, skillmax) in [(1, 5), (60, 300), (61, 307), (100, 600)] {
        assert_eq!(
            Rules::from_classic_config(cap, false)
                .unwrap()
                .global_skill_max,
            skillmax
        );
    }
    let (a, b, c) = rows(1, 0);
    let mut wrong = Rules::from_classic_config(60, false).unwrap();
    wrong.global_skill_max = 5;
    assert_eq!(
        Catalog::new(1, 8, wrong, vec![a], vec![b], vec![c], vec![]),
        Err(Error::Invalid("progression rules"))
    );
    let rules = Rules::from_classic_config(60, false).unwrap();
    for level in [1, 2, 9] {
        let (mut absent, class, xp) = rows(level, 0);
        absent.attributes.0[0] = 0;
        assert_eq!(
            Catalog::new(1, 8, rules, vec![absent], vec![class], vec![xp], vec![]),
            Err(Error::Invalid("player_levelstats row")),
            "zero strength is the source loader's missing-row sentinel at level {level}"
        );
    }
    let mut wide = b;
    wide.base_health = 65536;
    assert!(Catalog::new(1, 8, rules, vec![a], vec![wide], vec![c], vec![]).is_err());
    let mut wide = b;
    wide.base_mana = 65536;
    assert!(Catalog::new(1, 8, rules, vec![a], vec![wide], vec![c], vec![]).is_err());
    let cap = Rules::from_classic_config(1, false).unwrap();
    assert!(Catalog::new(1, 8, cap, vec![a], vec![b], vec![c], vec![]).is_err());
    let zero = XpRow {
        level: 1,
        xp_for_next_level: 0,
    };
    assert!(Catalog::new(1, 8, cap, vec![a], vec![b], vec![zero], vec![]).is_ok());
    let (a, b, c) = rows(2, 0);
    assert!(Catalog::new(1, 8, cap, vec![a], vec![b], vec![c], vec![]).is_err());
}
