// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use MirrorTimerOperation as Op;
use MirrorTimerType as Ty;

fn machine() -> MirrorTimerMachine {
    MirrorTimerMachine::new(
        [1; 16],
        crate::PlayerId {
            owner: [2; 16],
            character: [3; 16],
        },
    )
    .unwrap()
}
fn restore(m: &MirrorTimerMachine, bytes: &[u8]) -> Result<MirrorTimerMachine, MirrorTimerError> {
    MirrorTimerMachine::restore(m.world, m.player, bytes)
}
fn assert_unchanged(m: &MirrorTimerMachine, old: &MirrorTimerMachine) {
    assert_eq!(m.checkpoint(), old.checkpoint());
    assert_eq!(m.records(), old.records());
    assert_eq!(m.journal_bytes, old.journal_bytes);
}

#[test]
fn constructors_own_all_four_source_types_without_starting_them() {
    let m = machine();
    for ty in Ty::ALL {
        let t = m.timer(ty);
        assert_eq!(t.timer_type(), ty);
        assert_eq!(t.scale(), -1);
        assert_eq!(t.spell_id(), 0);
        assert_eq!(t.status(), MirrorTimerStatus::Unchanged);
        assert!(!t.is_active());
        assert!(!t.is_frozen());
        assert!(!t.frozen_requested());
        assert!(!t.is_regenerating());
        assert_eq!(
            (
                t.duration(),
                t.tracker_current(),
                t.pulse_interval(),
                t.pulse_current()
            ),
            (0, 0, 0, 0)
        );
    }
    assert_eq!(m.sequence(), 0);
    assert!(m.records().is_empty());
}

#[test]
fn source_status_is_last_write_and_start_overloads_treat_freeze_differently() {
    let mut t = MirrorTimer::new(Ty::Breath);
    t.set_frozen(true);
    t.start(100, 7);
    assert_eq!(t.status(), MirrorTimerStatus::FullUpdate);
    assert!(t.is_frozen());
    assert_eq!(t.update(100), Ok(true));
    assert_eq!(t.remaining(), 100);
    t.start_remaining(60, 100, 9);
    assert!(!t.is_frozen());
    assert_eq!(t.remaining(), 60);
    // SetFrozen(false) overwrites Start's full update with a status update.
    assert_eq!(t.fetch_status(), MirrorTimerStatus::StatusUpdate);
    assert_eq!(t.fetch_status(), MirrorTimerStatus::Unchanged);
    t.set_duration(200);
    assert_eq!(t.status(), MirrorTimerStatus::FullUpdate);
    t.set_frozen(true);
    assert_eq!(t.status(), MirrorTimerStatus::StatusUpdate);
    t.set_scale(-3);
    assert_eq!(t.status(), MirrorTimerStatus::FullUpdate);
    assert_eq!(t.spell_id(), 9);
}

#[test]
fn expiration_is_instant_then_two_second_pulses_preserve_single_reset_backlog() {
    let mut t = MirrorTimer::new(Ty::Fatigue);
    t.start(1000, 0);
    assert_eq!(t.update(999), Ok(true));
    assert_eq!(t.update(1), Ok(false));
    assert_eq!((t.tracker_current(), t.pulse_current()), (1000, 0));
    assert_eq!(t.update(1999), Ok(true));
    assert_eq!(t.update(1), Ok(false));
    assert_eq!(t.pulse_current(), 0);
    assert_eq!(t.update(4500), Ok(false));
    assert_eq!(t.pulse_current(), 2500);
    assert_eq!(t.update(0), Ok(false));
    assert_eq!(t.pulse_current(), 500);
    assert_eq!(t.update(1499), Ok(true));
    assert_eq!(t.update(1), Ok(false));
    assert_eq!(t.pulse_current(), 0);
    t.start(1000, 0);
    assert_eq!(t.update(1500), Ok(false));
    // First-expiry excess does not enter the pulse tracker.
    assert_eq!(t.pulse_current(), 0);
}

#[test]
fn zero_duration_start_and_already_expired_start_do_not_invent_an_instant_tick() {
    let mut t = MirrorTimer::new(Ty::Environmental);
    t.start(0, 11);
    assert!(t.is_active());
    assert_eq!(t.update(0), Ok(true));
    assert_eq!(t.update(1999), Ok(true));
    assert_eq!(t.update(1), Ok(false));
    t.start_remaining(0, 100, 12);
    assert_eq!(t.update(0), Ok(true));
    assert_eq!(t.update(2000), Ok(false));
    t.set_duration(0);
    assert!(!t.is_active());
    assert_eq!((t.duration(), t.remaining(), t.spell_id()), (100, 100, 12));
}

#[test]
fn regeneration_ignores_stored_freeze_and_stops_at_equality() {
    let mut t = MirrorTimer::new(Ty::Breath);
    t.start_remaining(10, 100, 13);
    t.set_frozen(true);
    t.set_scale(10);
    assert!(t.frozen_requested());
    assert!(!t.is_frozen());
    assert!(t.is_regenerating());
    t.fetch_status();
    t.set_scale(0);
    assert_eq!(t.scale(), 10);
    assert_eq!(t.fetch_status(), MirrorTimerStatus::StatusUpdate);
    t.set_frozen(false);
    // Source compares effective IsFrozen, so raw-bit removal has no new status.
    assert_eq!(t.fetch_status(), MirrorTimerStatus::Unchanged);
    assert_eq!(t.update(8), Ok(true));
    assert_eq!(t.tracker_current(), 10);
    assert_eq!(t.update(1), Ok(true));
    assert!(!t.is_active());
    assert_eq!(t.remaining(), 100);
    assert_eq!(t.fetch_status(), MirrorTimerStatus::StatusUpdate);
    t.start(900, 99);
    assert!(!t.is_active());
    assert_eq!((t.duration(), t.spell_id()), (100, 13));
}

#[test]
fn inactive_stop_and_zero_setters_preserve_nonactive_tracker_fields() {
    let mut t = MirrorTimer::new(Ty::FeignDeath);
    t.set_duration(100);
    t.set_remaining(30);
    let old = t.clone();
    t.stop();
    t.set_remaining(0);
    t.set_duration(0);
    assert_eq!(t, old);
    assert_eq!(t.tracker_current(), 70);
}

#[test]
fn defined_unsigned_subtraction_addition_multiplication_wrap_exactly() {
    let mut t = MirrorTimer::new(Ty::Breath);
    t.start_remaining(101, 100, 0);
    assert_eq!(t.tracker_current(), u32::MAX);
    assert_eq!(t.remaining(), 101);
    assert_eq!(t.update(1), Ok(true));
    assert_eq!(t.tracker_current(), 0);
    t.set_scale(-2);
    assert_eq!(t.update(1 << 31), Ok(true));
    assert_eq!(t.tracker_current(), 0);
    t.set_remaining(200);
    assert_eq!(t.tracker_current(), 100u32.wrapping_sub(200));
    assert_eq!(t.remaining(), 200);
    t.start_remaining(10, 100, 0);
    t.set_duration(50);
    assert_eq!(t.remaining(), 50u32.wrapping_sub(90));
    t.start(1, 0);
    t.set_scale(-1);
    assert_eq!(t.update(1), Ok(false));
    assert_eq!(t.update(u32::MAX - 1), Ok(false));
    assert_eq!(t.pulse_current(), u32::MAX - 2001);
    // Tracker addition itself wraps, so a subsequent expiry is not guaranteed.
    assert_eq!(t.update(u32::MAX), Ok(true));
    assert_eq!(t.tracker_current(), 0);
}

#[test]
fn int_min_rejects_only_reached_abs_and_never_changes_state_on_error() {
    let mut t = MirrorTimer::new(Ty::Breath);
    t.set_scale(i32::MIN);
    assert_eq!(t.update(0), Ok(true));
    t.start(100, 0);
    let old = t.clone();
    assert_eq!(t.update(0), Err(MirrorTimerError::UndefinedScaleMagnitude));
    assert_eq!(t, old);
    t.set_frozen(true);
    assert_eq!(t.update(u32::MAX), Ok(true));
    t.set_frozen(false);
    let old = t.clone();
    assert_eq!(t.update(1), Err(MirrorTimerError::UndefinedScaleMagnitude));
    assert_eq!(t, old);
}

#[test]
fn checkpoint_retains_all_timers_raw_freeze_status_spell_and_pulse_remainder() {
    let mut m = machine();
    for ty in Ty::ALL {
        m.apply(
            ty,
            Op::Start {
                interval: 100,
                spell_id: ty as u32 + 10,
            },
        )
        .unwrap();
        m.apply(ty, Op::Update { diff: 100 }).unwrap();
        m.apply(
            ty,
            Op::Update {
                diff: 4501 + ty as u32,
            },
        )
        .unwrap();
        m.apply(ty, Op::SetFrozen { frozen: true }).unwrap();
    }
    m.apply(Ty::Breath, Op::SetScale { scale: 10 }).unwrap();
    let saved = m.checkpoint().to_bytes().unwrap();
    let reopened = restore(&m, &saved).unwrap();
    assert_eq!(reopened.checkpoint(), m.checkpoint());
    assert!(reopened.records().is_empty());
    assert_eq!(reopened.timer(Ty::Breath).pulse_current(), 2502);
    assert!(reopened.timer(Ty::Breath).frozen_requested());
    assert!(!reopened.timer(Ty::Breath).is_frozen());
    assert_eq!(
        reopened.timer(Ty::Breath).status(),
        MirrorTimerStatus::FullUpdate
    );
    assert_eq!(reopened.timer(Ty::FeignDeath).spell_id(), 12);
}

#[test]
fn standalone_replay_reexecutes_source_results_and_consumed_status() {
    let mut m = machine();
    let initial = m.checkpoint().to_bytes().unwrap();
    m.apply(
        Ty::Breath,
        Op::StartRemaining {
            current: 10,
            max: 100,
            spell_id: 77,
        },
    )
    .unwrap();
    assert_eq!(
        m.apply(Ty::Breath, Op::FetchStatus).unwrap(),
        MirrorTimerOutcome::Status(MirrorTimerStatus::FullUpdate)
    );
    assert_eq!(
        m.apply(Ty::Breath, Op::Update { diff: 10 }).unwrap(),
        MirrorTimerOutcome::Update(false)
    );
    m.apply(Ty::Environmental, Op::SetScale { scale: i32::MIN })
        .unwrap();
    assert_eq!(
        m.apply(Ty::Environmental, Op::Update { diff: 1 }).unwrap(),
        MirrorTimerOutcome::Update(true)
    );
    m.apply(Ty::Breath, Op::Update { diff: 4001 }).unwrap();
    let mut replay = restore(&m, &initial).unwrap();
    replay.replay_bytes(&m.records_to_bytes().unwrap()).unwrap();
    assert_eq!(replay.checkpoint(), m.checkpoint());
    assert_eq!(replay.records(), m.records());
    assert_eq!(
        replay.journal_bytes,
        replay.records_to_bytes().unwrap().len()
    );
}

#[test]
fn compacted_suffix_reopens_and_replays_without_losing_pending_status() {
    let mut m = machine();
    m.apply(
        Ty::Fatigue,
        Op::Start {
            interval: 10,
            spell_id: 8,
        },
    )
    .unwrap();
    let retained = m.checkpoint();
    let mut reopened = restore(&m, &retained.to_bytes().unwrap()).unwrap();
    m.discard_records(&retained).unwrap();
    m.apply(Ty::Fatigue, Op::FetchStatus).unwrap();
    m.apply(Ty::Fatigue, Op::Update { diff: 10 }).unwrap();
    let before = m.clone();
    assert_eq!(
        m.discard_records(&retained),
        Err(MirrorTimerError::InvalidCheckpoint)
    );
    assert_unchanged(&m, &before);
    reopened
        .replay_bytes(&m.records_to_bytes().unwrap())
        .unwrap();
    assert_eq!(reopened.checkpoint(), m.checkpoint());
    assert_eq!(reopened.records(), m.records());
}

#[test]
fn invalid_abs_journal_capacity_and_sequence_overflow_are_atomic() {
    let mut m = machine();
    m.apply(Ty::Breath, Op::SetScale { scale: i32::MIN })
        .unwrap();
    m.apply(
        Ty::Breath,
        Op::Start {
            interval: 100,
            spell_id: 7,
        },
    )
    .unwrap();
    let old = m.clone();
    assert_eq!(
        m.apply(Ty::Breath, Op::Update { diff: 0 }),
        Err(MirrorTimerError::UndefinedScaleMagnitude)
    );
    assert_unchanged(&m, &old);
    let mut m = machine();
    for _ in 0..MAX_MIRROR_TIMER_RECORDS {
        m.apply(Ty::Fatigue, Op::Stop).unwrap();
    }
    let old = m.clone();
    assert_eq!(
        m.apply(
            Ty::Breath,
            Op::Start {
                interval: 1,
                spell_id: 1
            }
        ),
        Err(MirrorTimerError::JournalFull)
    );
    assert_unchanged(&m, &old);
    let retained = m.checkpoint();
    m.discard_records(&retained).unwrap();
    assert_eq!(m.journal_bytes, 2);
    m.apply(
        Ty::Breath,
        Op::Start {
            interval: 1,
            spell_id: 1,
        },
    )
    .unwrap();
    m.sequence = u64::MAX;
    let old = m.clone();
    assert_eq!(
        m.apply(Ty::Breath, Op::FetchStatus),
        Err(MirrorTimerError::Limit)
    );
    assert_unchanged(&m, &old);
}

#[test]
fn changed_replay_tail_rolls_back_valid_prefix_and_all_status_consumption() {
    let mut original = machine();
    original
        .apply(
            Ty::Breath,
            Op::Start {
                interval: 100,
                spell_id: 42,
            },
        )
        .unwrap();
    original.apply(Ty::Breath, Op::FetchStatus).unwrap();
    original
        .apply(Ty::Breath, Op::Update { diff: 100 })
        .unwrap();
    let mut value = serde_json::to_value(original.records()).unwrap();
    value[2]["outcome"] = serde_json::json!({"Update": true});
    // Serialize the typed schema so the rejection is semantic, not key ordering.
    let records: Vec<MirrorTimerRecord> = serde_json::from_value(value).unwrap();
    let bytes = serde_json::to_vec(&records).unwrap();
    let mut replay = machine();
    let old = replay.clone();
    assert_eq!(
        replay.replay_bytes(&bytes),
        Err(MirrorTimerError::ReplayMismatch)
    );
    assert_unchanged(&replay, &old);
    replay
        .replay_bytes(&original.records_to_bytes().unwrap())
        .unwrap();
    let old = replay.clone();
    assert_eq!(
        replay.replay_bytes(&original.records_to_bytes().unwrap()),
        Err(MirrorTimerError::ReplayMismatch)
    );
    assert_unchanged(&replay, &old);
}

#[test]
fn canonical_schema_identity_checksum_and_byte_limits_are_checked() {
    let m = machine();
    let saved = m.checkpoint().to_bytes().unwrap();
    let mut padded = vec![b' '];
    padded.extend_from_slice(&saved);
    assert!(matches!(
        restore(&m, &padded),
        Err(MirrorTimerError::InvalidCheckpoint)
    ));
    assert!(MirrorTimerMachine::restore([9; 16], m.player, &saved).is_err());
    assert!(MirrorTimerMachine::new([0; 16], m.player).is_err());
    assert!(MirrorTimerMachine::new(
        m.world,
        crate::PlayerId {
            owner: [0; 16],
            character: [3; 16]
        }
    )
    .is_err());
    assert!(MirrorTimerMachine::new(
        m.world,
        crate::PlayerId {
            owner: [2; 16],
            character: [0; 16]
        }
    )
    .is_err());
    let mut wrong = serde_json::to_value(m.checkpoint()).unwrap();
    wrong["timers"][0]["frozen"] = true.into();
    let wrong: MirrorTimerCheckpoint = serde_json::from_value(wrong).unwrap();
    assert!(restore(&m, &wrong.to_bytes().unwrap()).is_err());
    let mut unknown = serde_json::to_value(m.checkpoint()).unwrap();
    unknown["extra"] = 1.into();
    assert!(serde_json::from_value::<MirrorTimerCheckpoint>(unknown).is_err());
    let huge = vec![b' '; MAX_MIRROR_TIMER_BYTES + 1];
    assert!(matches!(
        restore(&m, &huge),
        Err(MirrorTimerError::TooLarge)
    ));
    let mut replay = machine();
    let old = replay.clone();
    assert_eq!(replay.replay_bytes(&huge), Err(MirrorTimerError::TooLarge));
    assert_eq!(
        replay.replay_bytes(b" []"),
        Err(MirrorTimerError::InvalidCheckpoint)
    );
    assert_unchanged(&replay, &old);
}

#[test]
fn checked_snapshot_restore_rejects_impossible_structural_state() {
    let t = MirrorTimer::new(Ty::Breath);
    for (field, value) in [
        ("scale", serde_json::json!(0)),
        ("timer_type", serde_json::json!("Fatigue")),
        ("pulse_interval", serde_json::json!(1)),
        ("pulse_current", serde_json::json!(1)),
        ("active", serde_json::json!(true)),
        ("status", serde_json::json!("FullUpdate")),
        ("spell_id", serde_json::json!(7)),
    ] {
        let mut value_state = serde_json::to_value(t.snapshot()).unwrap();
        value_state[field] = value;
        let snapshot: MirrorTimerSnapshot = serde_json::from_value(value_state).unwrap();
        assert_eq!(
            MirrorTimer::restore(snapshot, Ty::Breath),
            Err(MirrorTimerError::InvalidCheckpoint),
            "{field}"
        );
    }
    let mut m = machine();
    m.timers[0].set_frozen(true);
    // Even a fresh valid checksum cannot invent a noninitial sequence-zero state.
    assert!(restore(&m, &m.checkpoint().to_bytes().unwrap()).is_err());
}

fn oracle_state(timer: &MirrorTimer) -> [String; 12] {
    [
        timer.scale().to_string(),
        timer.spell_id().to_string(),
        (timer.status() as u32).to_string(),
        u8::from(timer.is_active()).to_string(),
        u8::from(timer.frozen_requested()).to_string(),
        timer.duration().to_string(),
        timer.tracker_current().to_string(),
        timer.pulse_interval().to_string(),
        timer.pulse_current().to_string(),
        timer.remaining().to_string(),
        u8::from(timer.is_frozen()).to_string(),
        u8::from(timer.is_regenerating()).to_string(),
    ]
}

#[test]
fn all_exact_source_oracle_rows_match_owned_state_returns_and_restore() {
    let mut previous_case = "";
    let mut timer = MirrorTimer::new(Ty::Fatigue);
    let mut expected_step = 0;
    let mut cases = 0;
    let mut rows = 0;
    let mut undefined = 0;
    for line in include_str!("../../../../tools/fixtures/mirror-timer.tsv")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(fields.len(), 32, "{line}");
        let ty = Ty::ALL[fields[2].parse::<usize>().unwrap()];
        if previous_case != fields[0] {
            previous_case = fields[0];
            timer = MirrorTimer::new(ty);
            expected_step = 0;
            cases += 1;
        }
        assert_eq!(fields[1].parse::<usize>().unwrap(), expected_step, "{line}");
        expected_step += 1;
        assert_eq!(timer.timer_type(), ty, "{line}");
        assert_eq!(
            oracle_state(&timer).as_slice(),
            &fields[8..20],
            "before: {line}"
        );
        let arg = |n: usize| fields[n].parse::<u32>().unwrap();
        let returned = match fields[3] {
            "inspect" => "-".to_owned(),
            "start" => {
                timer.start(arg(4), arg(5));
                "-".into()
            }
            "start_remaining" => {
                timer.start_remaining(arg(4), arg(5), arg(6));
                "-".into()
            }
            "stop" => {
                timer.stop();
                "-".into()
            }
            "set_remaining" => {
                timer.set_remaining(arg(4));
                "-".into()
            }
            "set_duration" => {
                timer.set_duration(arg(4));
                "-".into()
            }
            "set_frozen" => {
                timer.set_frozen(arg(4) != 0);
                "-".into()
            }
            "set_scale" => {
                timer.set_scale(fields[4].parse().unwrap());
                "-".into()
            }
            "fetch_status" => (timer.fetch_status() as u32).to_string(),
            "update" => match timer.update(arg(4)) {
                Ok(value) => u8::from(value).to_string(),
                Err(MirrorTimerError::UndefinedScaleMagnitude) => {
                    undefined += 1;
                    "undefined_abs".into()
                }
                Err(other) => panic!("unexpected {other}: {line}"),
            },
            unknown => panic!("unexpected operation {unknown}"),
        };
        assert_eq!(returned, fields[7], "return: {line}");
        assert_eq!(
            oracle_state(&timer).as_slice(),
            &fields[20..32],
            "after: {line}"
        );
        assert_eq!(
            MirrorTimer::restore(timer.snapshot(), ty).unwrap(),
            timer,
            "restore: {line}"
        );
        rows += 1;
    }
    assert!(rows >= 9000);
    assert!(cases >= 1300);
    assert!(undefined >= 200);
}
