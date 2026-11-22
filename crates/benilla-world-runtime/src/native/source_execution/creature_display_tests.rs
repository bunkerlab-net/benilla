// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_sim::creature_init::DisplayChoice;
use benilla_sim::source_rng::{DrawCursor, Mt19937, SourceRandom};
#[test]
fn display_choice_draws_even_with_no_models_and_retains_source_signed_boundary() {
    // Creature.cpp:674-690 wraps uint32 sums then checks signed subtraction at :678.
    let choices = [DisplayChoice {
        id: 0,
        probability: u32::MAX,
    }; 4];
    let mut engine = Mt19937::from_seed(5489);
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    let mut random = SourceRandom::new(7, &mut engine, &mut cursor, &mut sink).unwrap();
    assert_eq!(
        draw_source_creature_display(choices, &mut random).unwrap(),
        (0, 0)
    );
    assert_eq!(random.cursor().raw, 1);
    let mut choices = choices;
    choices[0] = DisplayChoice {
        id: 10,
        probability: 0x80000000,
    };
    assert_eq!(
        draw_source_creature_display(choices, &mut random)
            .unwrap_err()
            .id,
        "execution.creature.display_signed_overflow"
    );
    assert_eq!(random.cursor().raw, 1);
    choices[0].probability = 0x80000001;
    assert_eq!(
        draw_source_creature_display(choices, &mut random).unwrap(),
        (0, 0)
    );
    choices[0].probability = 0x7fffffff;
    assert_eq!(
        draw_source_creature_display(choices, &mut random)
            .unwrap()
            .0,
        10
    );
}
#[test]
fn display_choice_ignores_absent_models_and_preserves_probability_order() {
    // Creature.cpp:675,683-689 excludes absent IDs and consumes the signed irand in template order.
    let choices = [
        DisplayChoice {
            id: 0,
            probability: u32::MAX,
        },
        DisplayChoice {
            id: 10,
            probability: 25,
        },
        DisplayChoice {
            id: 11,
            probability: 75,
        },
        DisplayChoice {
            id: 12,
            probability: 0,
        },
    ];
    let mut engine = Mt19937::from_seed(5489);
    let mut cursor = DrawCursor::default();
    let mut events = Vec::new();
    let mut sink = |event| events.push(event);
    let mut random = SourceRandom::new(7, &mut engine, &mut cursor, &mut sink).unwrap();
    assert_eq!(
        draw_source_creature_display(choices, &mut random).unwrap(),
        (11, 81)
    );
    assert_eq!(random.cursor().raw, 1);
    assert!(events.iter().all(|event| event.stream == 7));
}
