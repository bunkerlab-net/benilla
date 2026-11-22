// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored PoolManager mechanism fixtures, not recorded manager outcomes.
use super::*;
struct Fixed {
    members: Vec<PoolObject>,
    roll: f64,
    accessed: usize,
    shuffles: usize,
}
impl RollContext for Fixed {
    type Error = ();
    fn engine_access(&mut self) -> Result<(), ()> {
        self.accessed += 1;
        Ok(())
    }
    fn chance(&mut self) -> Result<f64, ()> {
        Ok(self.roll)
    }
    fn shuffle(&mut self, _: &mut [usize]) -> Result<(), ()> {
        self.shuffles += 1;
        Ok(())
    }
    fn object(&self, _: bool, index: usize) -> PoolObject {
        self.members[index]
    }
    fn can_spawn(&mut self, _: u32) -> Result<bool, ()> {
        Ok(true)
    }
    fn selected(&self, _: u32) -> Result<bool, ()> {
        Ok(false)
    }
}
#[test]
fn explicit_roll_is_not_cumulative_and_keeps_first_eligible_fallback() {
    // PoolManager.cpp:203-232,264-265 compares the same 75 against both chances.
    let mut context = Fixed {
        members: vec![PoolObject::new(7, 50.0, 0), PoolObject::new(8, 50.0, 1)],
        roll: 75.0,
        accessed: 0,
        shuffles: 0,
    };
    assert_eq!(roll_one(2, 0, 0, &mut context).unwrap().unwrap().guid, 7);
    assert_eq!((context.accessed, context.shuffles), (1, 1));
}

use crate::source_rng::{
    DrawCursor, EventKind, Mt19937, SeedObservations, SourceRandom, ThreadRng, TraceEvent,
};
use std::collections::BTreeSet;
struct Live {
    group: PoolGroup,
    blocked: BTreeSet<u32>,
    selected: BTreeSet<u32>,
    thread: ThreadRng,
    cursor: DrawCursor,
    seeds: usize,
    events: Vec<TraceEvent>,
}
impl Live {
    fn new(group: PoolGroup, cold: bool) -> Self {
        Self {
            group,
            blocked: BTreeSet::new(),
            selected: BTreeSet::new(),
            thread: ThreadRng::from_state(
                7,
                if cold {
                    None
                } else {
                    Some(Mt19937::default().into_state())
                },
            )
            .unwrap(),
            cursor: DrawCursor::default(),
            seeds: 0,
            events: vec![],
        }
    }
}
impl RollContext for Live {
    type Error = crate::source_rng::Error;
    fn engine_access(&mut self) -> Result<(), Self::Error> {
        let seeds = &mut self.seeds;
        self.thread.get_or_init(|| {
            *seeds += 1;
            SeedObservations {
                unix_seconds: 0,
                process_clock: 0,
            }
        });
        Ok(())
    }
    fn chance(&mut self) -> Result<f64, Self::Error> {
        self.engine_access()?;
        let stream = self.thread.thread_birth();
        let mut sink = |event| self.events.push(event);
        SourceRandom::new(
            stream,
            self.thread.get_or_init(|| SeedObservations {
                unix_seconds: 0,
                process_clock: 0,
            }),
            &mut self.cursor,
            &mut sink,
        )?
        .rand_chance()
    }
    fn shuffle(&mut self, indices: &mut [usize]) -> Result<(), Self::Error> {
        self.engine_access()?;
        let stream = self.thread.thread_birth();
        let mut sink = |event| self.events.push(event);
        SourceRandom::new(
            stream,
            self.thread.get_or_init(|| SeedObservations {
                unix_seconds: 0,
                process_clock: 0,
            }),
            &mut self.cursor,
            &mut sink,
        )?
        .shuffle(indices)
    }
    fn object(&self, explicit: bool, index: usize) -> PoolObject {
        if explicit {
            self.group.explicit[index]
        } else {
            self.group.equal[index]
        }
    }
    fn can_spawn(&mut self, guid: u32) -> Result<bool, Self::Error> {
        Ok(!self.blocked.contains(&guid))
    }
    fn selected(&self, guid: u32) -> Result<bool, Self::Error> {
        Ok(self.selected.contains(&guid))
    }
}
#[test]
fn empty_lists_still_initialize_the_actual_thread_engine_without_drawing() {
    // PoolManager.cpp:199 precedes both empty-list checks; Util.cpp:29-39.
    let mut context = Live::new(PoolGroup::default(), true);
    assert!(!context.thread.initialized());
    assert!(roll_one(0, 0, 0, &mut context).unwrap().is_none());
    assert!(context.thread.initialized());
    assert!(roll_one(0, 0, 0, &mut context).unwrap().is_none());
    assert_eq!((context.seeds, context.cursor.raw), (1, 0));
    assert!(context.events.is_empty());
}
#[test]
fn equal_shuffle_exclusion_selected_trigger_and_all_filtered_keep_exact_raw_cursor() {
    let group = PoolGroup {
        explicit: vec![],
        equal: (7..=9)
            .enumerate()
            .map(|(i, guid)| PoolObject::new(guid, 0.0, i as u64))
            .collect(),
    };
    for mode in 0..4 {
        let mut context = Live::new(group.clone(), false);
        context.group.equal[0].excluded = true;
        if mode == 1 || mode == 2 {
            context.selected.insert(9);
        }
        if mode == 3 {
            context.blocked.extend([8, 9]);
        }
        // GCC 12.2's paired shuffle consumes MT word 3499211612 and gives [7,9,8].
        let result = roll_one(0, 3, if mode == 2 { 9 } else { 0 }, &mut context).unwrap();
        assert_eq!(
            result.map(|v| v.guid),
            match mode {
                0 | 2 => Some(9),
                1 => Some(8),
                _ => None,
            }
        );
        assert_eq!(context.cursor.raw, 1);
        assert_eq!(
            context
                .events
                .iter()
                .filter_map(|event| if let EventKind::Raw { word, .. } = event.kind {
                    Some(word)
                } else {
                    None
                })
                .collect::<Vec<_>>(),
            [3499211612]
        );
    }
}
#[test]
fn explicit_fallback_can_lose_to_equal_member_and_double_roll_precedes_shuffles() {
    let group = PoolGroup {
        explicit: vec![PoolObject::new(7, 1.0, 0)],
        equal: vec![PoolObject::new(8, 0.0, 0)],
    };
    let mut context = Live::new(group.clone(), false);
    assert_eq!(roll_one(1, 1, 0, &mut context).unwrap().unwrap().guid, 8);
    assert_eq!(context.cursor.raw, 2);
    assert_eq!(
        context
            .events
            .iter()
            .filter_map(|event| if let EventKind::Raw { word, .. } = event.kind {
                Some(word)
            } else {
                None
            })
            .collect::<Vec<_>>(),
        [3499211612, 581869302]
    );
    let mut context = Live::new(group, false);
    context.group.equal[0].excluded = true;
    assert_eq!(roll_one(1, 1, 0, &mut context).unwrap().unwrap().guid, 7);
    context.group.explicit[0].excluded = true;
    assert!(roll_one(1, 1, 0, &mut context).unwrap().is_none());
}
#[test]
fn group_membership_integrity_and_first_only_mutations_match_source_lists() {
    let mut group = PoolGroup::default();
    group.add(PoolObject::new(7, -25.0, 0), 1);
    group.add(PoolObject::new(7, 75.0, 1), 1);
    assert!(group.valid_chances());
    assert_eq!(group.explicit[0].chance_bits, 25.0f32.to_bits());
    group.add(PoolObject::new(7, 0.0, 2), 1);
    group.exclude(7, true);
    assert!(group.equal[0].excluded);
    assert!(!group.explicit[0].excluded);
    group.remove_relation(7);
    assert!(group.equal.is_empty());
    assert_eq!(group.explicit.len(), 1);
    assert!(!group.valid_chances());
    group.add(PoolObject::new(8, 25.0, 3), 2);
    assert!(group.valid_chances());
}
#[test]
fn unsigned_subtraction_signed_conversion_and_increment_boundaries_are_literal() {
    // PoolManager.cpp:396,404. GCC int is 32-bit; only the signed increment can overflow.
    assert_eq!(spawn_count(0, 1, false), Ok(-1));
    assert_eq!(spawn_count(0, u32::MAX, false), Ok(1));
    assert_eq!(spawn_count(i32::MAX as u32 - 1, 0, true), Ok(i32::MAX));
    assert_eq!(
        spawn_count(i32::MAX as u32, 0, true),
        Err(Error::CountIncrementUndefined)
    );
    assert_eq!(spawn_count(i32::MAX as u32, 0, false), Ok(i32::MAX));
    assert_eq!(template_loop(65534), Ok(()));
    assert_eq!(template_loop(65535), Err(Error::TemplateLoopUnbounded));
}
