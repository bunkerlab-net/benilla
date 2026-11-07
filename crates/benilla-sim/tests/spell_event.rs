// SPDX-License-Identifier: GPL-3.0-or-later
// Independent exact-source decisions; the C++ mock is not whole-host evidence.
use benilla_sim::spell_event::{
    plan_after_delayed, plan_after_update, SpellEventError, SpellEventPlan, SpellEventState,
};

const FIXTURE: &str = include_str!("fixtures/spell_event_cmangos.tsv");

#[test]
fn shared_spell_event_helper_matches_exact_cpp_decisions() {
    let mut compared = 0;
    let mut rejected_channeled = 0;
    for line in FIXTURE.lines().filter(|line| line.starts_with("O ")) {
        let row: Vec<i128> = line
            .split_whitespace()
            .skip(1)
            .map(|word| word.parse().unwrap())
            .collect();
        assert_eq!(row.len(), 12, "{line}");
        let now = u64::try_from(row[1]).unwrap();
        let start = u64::try_from(row[4]).unwrap();
        let state = if row[7] != 0 {
            SpellEventState::Channeled
        } else {
            match row[2] {
                2 => SpellEventState::Casting,
                4 => SpellEventState::Traveling {
                    delay_start_ms: (start != 0).then_some(start),
                    delay_moment_ms: u64::try_from(row[5]).unwrap(),
                },
                7 => SpellEventState::Finished {
                    deletable: row[3] != 0,
                },
                value => panic!("unexpected source state {value}: {line}"),
            }
        };
        let result = plan_after_update(now, state);
        if row[7] != 0 {
            assert_eq!(result, Err(SpellEventError::ChanneledUnsupported));
            rejected_channeled += 1;
            continue;
        }
        let mut plan = result.unwrap();
        if let SpellEventPlan::HandleDelayed { elapsed_ms } = plan {
            assert_eq!(elapsed_ms, now - start, "{line}");
            plan = plan_after_delayed(now, start, u64::try_from(row[6]).unwrap()).unwrap();
        }
        let (delete, new_start, deadline) = match plan {
            SpellEventPlan::Delete => (true, start, 0),
            SpellEventPlan::ReAdd {
                deadline_ms,
                set_delay_start_ms,
            } => (false, set_delay_start_ms.unwrap_or(start), deadline_ms),
            SpellEventPlan::HandleDelayed { .. } => unreachable!(),
        };
        assert_eq!(delete, row[9] != 0, "{line}");
        assert_eq!(i128::from(new_start), row[10], "{line}");
        assert_eq!(i128::from(deadline), row[11], "{line}");
        compared += 1;
    }
    assert_eq!(compared, 37);
    assert_eq!(rejected_channeled, 7);
}

#[test]
fn clock_errors_are_explicit_and_zero_delay_is_immediately_due() {
    assert_eq!(
        plan_after_update(u64::MAX, SpellEventState::Casting),
        Err(SpellEventError::TimeOverflow)
    );
    assert_eq!(
        plan_after_update(u64::MAX, SpellEventState::Finished { deletable: true }),
        Ok(SpellEventPlan::Delete)
    );
    assert_eq!(
        plan_after_update(
            u64::MAX,
            SpellEventState::Traveling {
                delay_start_ms: None,
                delay_moment_ms: 1,
            },
        ),
        Err(SpellEventError::TimeOverflow)
    );
    assert_eq!(
        plan_after_update(
            0,
            SpellEventState::Traveling {
                delay_start_ms: Some(0),
                delay_moment_ms: 0,
            },
        ),
        Ok(SpellEventPlan::ReAdd {
            deadline_ms: 0,
            set_delay_start_ms: Some(0),
        })
    );
    assert_eq!(
        plan_after_update(
            2,
            SpellEventState::Traveling {
                delay_start_ms: Some(3),
                delay_moment_ms: 0,
            },
        ),
        Err(SpellEventError::DelayStartInFuture)
    );
    assert_eq!(
        plan_after_delayed(5, 0, 1),
        Err(SpellEventError::DelayNotStarted)
    );
    assert_eq!(
        plan_after_delayed(5, 6, 1),
        Err(SpellEventError::DelayStartInFuture)
    );
    assert_eq!(
        plan_after_delayed(u64::MAX, 1, 0),
        Err(SpellEventError::TimeOverflow)
    );
    assert_eq!(
        plan_after_delayed(u64::MAX, u64::MAX - 1, 2),
        Err(SpellEventError::TimeOverflow)
    );
    assert_eq!(
        plan_after_delayed(100, 1, 2),
        Ok(SpellEventPlan::ReAdd {
            deadline_ms: 3,
            set_delay_start_ms: None,
        })
    );
}
