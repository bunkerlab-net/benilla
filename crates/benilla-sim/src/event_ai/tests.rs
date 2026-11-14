// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored numeric shapes only; no Classic-DB rows, text, or client data.
use super::*;

fn event(chance: u8, text_ids: [i32; 3]) -> AggroText {
    AggroText::new(41, chance, text_ids).unwrap()
}

fn two_text_holder() -> AggroTextHolder {
    AggroTextHolder::new(event(10, [101, 103, 0]))
}

fn evaluate(
    holder: &mut AggroTextHolder,
    words: &mut impl Iterator<Item = u32>,
) -> (Option<TextSelection>, usize) {
    assert_eq!(holder.begin_enter_combat(), Some(DrawRequest::Chance));
    match holder.consume_chance(words.next().unwrap()).unwrap() {
        ChanceResult::Failed => (None, 1),
        ChanceResult::ActionRequired => (
            Some(holder.consume_action(words.next().unwrap()).unwrap()),
            2,
        ),
    }
}

#[test]
fn semantic_shape_accepts_only_contiguous_positive_signed_text_ids() {
    for ids in [[1, 0, 0], [1, 2, 0], [1, 2, 3], [1, 1, 1], [i32::MAX, 0, 0]] {
        let shape = event(100, ids);
        assert_eq!(shape.event_id(), 41);
        assert_eq!(shape.chance(), 100);
        assert_eq!(shape.text_ids(), ids);
    }
    for ids in [
        [0, 0, 0],
        [0, 1, 0],
        [0, 0, 1],
        [1, 0, 2],
        [-1, 0, 0],
        [1, -1, 0],
        [1, 2, -1],
        [i32::MIN, 0, 0],
    ] {
        assert!(AggroText::new(41, 10, ids).is_err());
    }
    for chance in [0, 101, u8::MAX] {
        assert!(AggroText::new(41, chance, [1, 0, 0]).is_err());
    }
    assert!(AggroText::new(0, 10, [1, 0, 0]).is_err());
}

#[test]
fn source_chance_comparison_accepts_only_lower_residues() {
    for chance in 1..=100 {
        for residue in 0..100 {
            let mut holder = AggroTextHolder::new(event(chance, [101, 0, 0]));
            holder.begin_enter_combat();
            assert_eq!(
                holder.consume_chance(100 + residue).unwrap(),
                if residue < u32::from(chance) {
                    ChanceResult::ActionRequired
                } else {
                    ChanceResult::Failed
                }
            );
        }
    }
}

#[test]
fn full32_source_words_and_independent_action_word_preserve_draw_order() {
    // ProcessEvent: urand(); chance <= rnd % 100; urand(); TEXT rnd % 2.
    // Odd chance words and even action words catch accidental chance-word reuse.
    for (chance_word, action_word, expected) in [
        (0, 102, Some(101)),
        (9, 102, Some(101)),
        (109, 103, Some(103)),
        (10, 103, None),
        (99, 102, None),
        (u32::MAX, 102, None), // MAX % 100 == 95
        (u32::MAX - 95, u32::MAX, Some(103)),
        (u32::MAX - 86, u32::MAX - 1, Some(101)),
    ] {
        let mut holder = two_text_holder();
        let mut words = [chance_word, action_word, 777].into_iter();
        let (selection, count) = evaluate(&mut holder, &mut words);
        assert_eq!(selection.map(|selection| selection.text_id), expected);
        if expected.is_some() {
            assert_eq!(count, 2);
            assert_eq!(words.next(), Some(777));
        } else {
            assert_eq!(count, 1);
            assert_eq!(words.next(), Some(action_word));
        }
        assert_eq!(
            holder.snapshot().unwrap(),
            two_text_holder().snapshot().unwrap()
        );
    }
}

#[test]
fn chance100_single_text_still_consumes_both_full_words() {
    let mut holder = AggroTextHolder::new(event(100, [107, 0, 0]));
    let mut words = [u32::MAX, u32::MAX, 19].into_iter();
    let (selection, count) = evaluate(&mut holder, &mut words);
    assert_eq!(count, 2);
    assert_eq!(
        selection,
        Some(TextSelection {
            event_id: 41,
            text_id: 107,
            text_slot: 0,
        })
    );
    assert_eq!(words.next(), Some(19));
}

#[test]
fn three_text_action_uses_source_modulo_three_in_parameter_order() {
    for word in [0, 1, 2, 3, 4, 5, u32::MAX - 2, u32::MAX - 1, u32::MAX] {
        let mut holder = AggroTextHolder::new(event(100, [101, 103, 107]));
        holder.begin_enter_combat();
        holder.consume_chance(99).unwrap();
        let selected = holder.consume_action(word).unwrap();
        assert_eq!(selected.text_slot, (word % 3) as u8);
        assert_eq!(selected.text_id, [101, 103, 107][(word % 3) as usize]);
        assert_eq!(holder.pending_draw(), None);
    }
}

#[test]
fn full32_modulo_chance_keeps_the_source_small_bias() {
    let complete_hundreds = (u64::from(u32::MAX) + 1) / 100;
    let extra_residues = (u64::from(u32::MAX) + 1) % 100;
    assert_eq!(extra_residues, 96);
    assert_eq!(complete_hundreds * 10 + extra_residues.min(10), 429_496_730);
    assert_ne!(429_496_730_u64 * 100, (u64::from(u32::MAX) + 1) * 10);
}

#[test]
fn in_progress_guard_blocks_readiness_and_survives_reset_home_and_death() {
    let mut holder = two_text_holder();
    assert_eq!(holder.begin_enter_combat(), Some(DrawRequest::Chance));
    for expected in [DrawRequest::Chance, DrawRequest::Action] {
        let before = holder.clone();
        assert_eq!(holder.begin_enter_combat(), None);
        holder.reset();
        holder.reached_home();
        holder.died();
        assert_eq!(holder, before);
        assert_eq!(holder.pending_draw(), Some(expected));
        assert_eq!(holder.snapshot(), Err(Error::InProgress));
        if expected == DrawRequest::Chance {
            assert_eq!(holder.consume_chance(9), Ok(ChanceResult::ActionRequired));
        }
    }
    holder.consume_action(2).unwrap();
    assert_eq!(holder, two_text_holder());
}

#[test]
fn initialization_lifecycle_and_respawn_do_not_execute_or_store_a_one_shot() {
    let mut holder = two_text_holder();
    let initial = holder.snapshot().unwrap();
    holder.reset();
    holder.reached_home();
    holder.died();
    holder.respawn();
    assert_eq!(holder.snapshot().unwrap(), initial);
    for (chance, expected) in [(10, None), (9, Some(103)), (0, Some(103))] {
        let mut words = [chance, 1].into_iter();
        let (selection, _) = evaluate(&mut holder, &mut words);
        assert_eq!(selection.map(|selection| selection.text_id), expected);
        let saved = holder.snapshot().unwrap();
        assert!(saved.enabled);
        assert_eq!(saved.timer_ms, 0);
        assert!(!saved.in_progress);
        assert_eq!(saved.event_target, None);
        assert_eq!(saved, initial);
        holder.reached_home();
    }
    holder.begin_enter_combat();
    holder.respawn();
    assert_eq!(holder, two_text_holder());
    assert_eq!(holder.begin_enter_combat(), Some(DrawRequest::Chance));
}

#[test]
fn zero_listeners_cannot_change_draws_or_selection() {
    // Audience is intentionally absent from the reducer API. Selection remains
    // observable even when a host delivers to no listeners.
    let mut holder = two_text_holder();
    let mut words = [109, 12, 777].into_iter();
    let (selection, consumed) = evaluate(&mut holder, &mut words);
    let listeners: [u64; 0] = [];
    let delivered: Vec<_> = listeners
        .iter()
        .map(|listener| (listener, selection))
        .collect();
    assert!(delivered.is_empty());
    assert_eq!(selection.unwrap().text_id, 101);
    assert_eq!(consumed, 2);
    assert_eq!(words.next(), Some(777));
}

#[test]
fn out_of_order_or_repeated_words_cannot_change_holder_state() {
    let mut holder = two_text_holder();
    for request in [None, Some(DrawRequest::Chance), Some(DrawRequest::Action)] {
        assert_eq!(holder.pending_draw(), request);
        let before = holder.clone();
        let invalid = if request == Some(DrawRequest::Chance) {
            holder.consume_action(2).map(|_| ())
        } else {
            holder.consume_chance(9).map(|_| ())
        };
        assert!(matches!(invalid, Err(Error::WrongDrawStage { .. })));
        assert_eq!(holder, before);
        match request {
            None => assert_eq!(holder.begin_enter_combat(), Some(DrawRequest::Chance)),
            Some(DrawRequest::Chance) => {
                assert_eq!(holder.consume_chance(9), Ok(ChanceResult::ActionRequired));
            }
            Some(DrawRequest::Action) => {
                holder.consume_action(2).unwrap();
            }
        }
    }
    let before = holder.clone();
    assert!(holder.consume_action(3).is_err());
    assert_eq!(holder, before);
}

#[test]
fn snapshot_roundtrip_revalidates_explicit_quiescent_source_members() {
    let holder = two_text_holder();
    let snapshot = holder.snapshot().unwrap();
    let encoded = serde_json::to_string(&snapshot).unwrap();
    let decoded: AggroTextHolderSnapshot = serde_json::from_str(&encoded).unwrap();
    assert_eq!(
        AggroTextHolder::restore(*holder.event(), decoded).unwrap(),
        holder
    );
    assert!(AggroTextHolder::restore(event(10, [101, 103, 0]), decoded).is_ok());
    assert!(
        AggroTextHolder::restore(AggroText::new(42, 10, [101, 103, 0]).unwrap(), decoded).is_err()
    );
}

#[test]
fn malformed_restoration_never_normalizes_unsupported_holder_states() {
    let event = *two_text_holder().event();
    let good = two_text_holder().snapshot().unwrap();
    for bad in [
        AggroTextHolderSnapshot { version: 0, ..good },
        AggroTextHolderSnapshot { version: 2, ..good },
        AggroTextHolderSnapshot {
            event_id: 0,
            ..good
        },
        AggroTextHolderSnapshot {
            event_id: 42,
            ..good
        },
        AggroTextHolderSnapshot {
            timer_ms: 1,
            ..good
        },
        AggroTextHolderSnapshot {
            timer_ms: u32::MAX,
            ..good
        },
        AggroTextHolderSnapshot {
            enabled: false,
            ..good
        },
        AggroTextHolderSnapshot {
            in_progress: true,
            ..good
        },
        AggroTextHolderSnapshot {
            event_target: Some(0),
            ..good
        },
        AggroTextHolderSnapshot {
            event_target: Some(77),
            ..good
        },
    ] {
        assert!(AggroTextHolder::restore(event, bad).is_err(), "{bad:?}");
    }
}

#[test]
fn snapshot_decoder_rejects_unknown_omitted_and_wrongly_typed_members() {
    let good = serde_json::to_value(two_text_holder().snapshot().unwrap()).unwrap();
    for key in [
        "version",
        "event_id",
        "timer_ms",
        "enabled",
        "in_progress",
        "event_target",
    ] {
        let mut omitted = good.clone();
        omitted.as_object_mut().unwrap().remove(key);
        assert!(
            serde_json::from_value::<AggroTextHolderSnapshot>(omitted).is_err(),
            "{key}"
        );
    }
    let mut unknown = good.clone();
    unknown["pending_action"] = serde_json::json!("Action");
    assert!(serde_json::from_value::<AggroTextHolderSnapshot>(unknown).is_err());
    for (key, value) in [
        ("enabled", serde_json::json!(1)),
        ("event_id", serde_json::json!(-1)),
        ("timer_ms", serde_json::json!(4_294_967_296_u64)),
        ("event_target", serde_json::json!(-1)),
    ] {
        let mut malformed = good.clone();
        malformed[key] = value;
        assert!(serde_json::from_value::<AggroTextHolderSnapshot>(malformed).is_err());
    }
}
