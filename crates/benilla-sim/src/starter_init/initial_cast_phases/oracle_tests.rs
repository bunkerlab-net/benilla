// SPDX-License-Identifier: GPL-3.0-or-later
//! The fixture is produced by hash-pinned unchanged C++ source, never Rust output.
use super::{tests::authored, *};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Fixture {
    schema_version: u32,
    source_pin: String,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    spells: Vec<Spell>,
    rolls: Vec<i32>,
    initial_weapon_mask: u32,
    initial_armor_mask: u32,
    mechanism_only: bool,
    source_only: bool,
    events: Vec<Value>,
    final_weapon_mask: u32,
    final_armor_mask: u32,
    rank_queries: u32,
    targetless_masks: Vec<u8>,
    self_masks: Vec<u8>,
}
#[derive(Deserialize)]
struct Spell {
    spell_id: u32,
    effects: [Effect; 3],
    item_class: i32,
    item_mask: u32,
    spell_rank: u32,
    max_level: u32,
    base_level: u32,
    spell_level: u32,
    resolved_cast_time_ms: i32,
    resolved_duration_ms: i32,
}
#[derive(Deserialize)]
struct Effect {
    opcode: u32,
    aura: u32,
    misc: i32,
    base_points: i32,
    base_dice: u32,
    die_sides: i32,
    target_a: u32,
    target_b: u32,
    points_per_level_bits: u32,
    dice_per_level_bits: u32,
    combo_points_bits: u32,
}

/// The independent oracle supplies execution-relevant cells, not a captured full
/// SpellEntry. All other cells below are explicitly authored test scaffolding.
fn input(spell: &Spell) -> AuthoredInitialCast {
    let mut cast = authored(if shape(spell.spell_id).is_ok() {
        spell.spell_id
    } else {
        204
    });
    cast.row.id = spell.spell_id;
    cast.row.words[0] = spell.spell_id;
    cast.row.words[27] = spell.max_level;
    cast.row.words[28] = spell.base_level;
    cast.row.words[29] = spell.spell_level;
    cast.row.words[58] = spell.item_class as u32;
    cast.row.words[59] = spell.item_mask;
    for (slot, effect) in spell.effects.iter().enumerate() {
        for (base, value) in [
            (61, effect.opcode),
            (64, effect.die_sides as u32),
            (67, effect.base_dice),
            (70, effect.dice_per_level_bits),
            (73, effect.points_per_level_bits),
            (76, effect.base_points as u32),
            (82, effect.target_a),
            (85, effect.target_b),
            (91, effect.aura),
            (106, effect.misc as u32),
            (112, effect.combo_points_bits),
        ] {
            cast.row.words[base + slot] = value;
        }
    }
    cast.row.duration_ms = Some(spell.resolved_duration_ms);
    let resolutions = cast.resolutions.as_mut().unwrap();
    resolutions.cast_time_ms = spell.resolved_cast_time_ms;
    resolutions.duration_ms = spell.resolved_duration_ms;
    cast
}

fn mechanism_plan(spells: &[Spell]) -> InitialCastPlan {
    let occurrences: Vec<_> = spells
        .iter()
        .enumerate()
        .map(|(ordinal, spell)| {
            let cast = input(spell);
            let effects = spell
                .effects
                .iter()
                .enumerate()
                .filter(|(_, e)| e.opcode != 0)
                .map(|(slot, e)| {
                    assert_eq!(e.points_per_level_bits, 0);
                    assert_eq!(e.dice_per_level_bits, 0);
                    assert_eq!(e.combo_points_bits, 0);
                    let phase = if e.target_a == 1 {
                        InitialEffectPhase::SelfHolder
                    } else {
                        InitialEffectPhase::Targetless
                    };
                    let amount = if matches!(e.die_sides, 0 | 1) {
                        InitialAmountOperation::Constant {
                            value: stats::constant_effect_amount(e.base_points, e.base_dice)
                                .unwrap(),
                        }
                    } else {
                        // The only mixed-phase probe's effect20 requests the same bounded
                        // 0..6 die. This bypass exists only in this non-exported test code.
                        assert_eq!(
                            (e.opcode, e.base_points, e.base_dice, e.die_sides),
                            (20, 0, 0, 6)
                        );
                        InitialAmountOperation::PendingRandomNoOp { low: 0, high: 6 }
                    };
                    InitialEffectOperation {
                        slot: slot as u8,
                        opcode: e.opcode,
                        phase,
                        aura: e.aura,
                        misc: e.misc,
                        amount: Some(amount),
                    }
                })
                .collect();
            InitialCastOccurrencePlan {
                ordinal: ordinal as u32,
                source_hash: cast.row.source_hash(),
                row: cast.row.clone(),
                provenance: InitialCastProvenance::Authored(Box::new(cast)),
                effects,
            }
        })
        .collect();
    let identity = plan_identity(&occurrences);
    InitialCastPlan {
        occurrences,
        identity,
    }
}

fn target(phase: InitialEffectPhase) -> &'static str {
    match phase {
        InitialEffectPhase::Targetless => "targetless",
        InitialEffectPhase::SelfHolder => "self",
        InitialEffectPhase::UnscheduledDetect => panic!("unscheduled effect emitted an event"),
    }
}
fn event(e: &InitialPhaseEvent) -> Value {
    match e {
        InitialPhaseEvent::CastStart { occurrence, .. } => {
            json!({"kind":"cast_start","spell_index":occurrence})
        }
        InitialPhaseEvent::CastComplete { occurrence, .. } => {
            json!({"kind":"cast_complete","spell_index":occurrence})
        }
        InitialPhaseEvent::IntegerDraw(d) => {
            json!({"kind":"draw","spell_index":d.request.occurrence,"slot":d.request.effect_slot,"low":d.request.low,"high":d.request.high,"value":d.value})
        }
        InitialPhaseEvent::Amount {
            occurrence,
            slot,
            phase,
            value,
        } => {
            json!({"kind":"amount","spell_index":occurrence,"slot":slot,"target":target(*phase),"value":value})
        }
        InitialPhaseEvent::Dispatch {
            occurrence,
            slot,
            phase,
            opcode,
        } => {
            json!({"kind":"dispatch","spell_index":occurrence,"slot":slot,"target":target(*phase),"value":opcode})
        }
        InitialPhaseEvent::HolderCreated { occurrence } => {
            json!({"kind":"holder_create","spell_index":occurrence})
        }
        InitialPhaseEvent::AuraConstructed { occurrence, aura } => {
            json!({"kind":"aura_construct","spell_index":occurrence,"slot":aura.effect_index,"value":aura.amount})
        }
        InitialPhaseEvent::AuraRegistered { occurrence, slot } => {
            json!({"kind":"aura_register","spell_index":occurrence,"slot":slot})
        }
        InitialPhaseEvent::AuraApplied { occurrence, slot } => {
            json!({"kind":"aura_apply","spell_index":occurrence,"slot":slot})
        }
    }
}

#[test]
fn unchanged_source_phase_oracle_matches_order_requests_amounts_and_masks() {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../../tests/fixtures/cmangos_initial_cast_phase_golden.json"
    ))
    .unwrap();
    assert_eq!(fixture.schema_version, 1);
    assert_eq!(
        fixture.source_pin,
        "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f"
    );
    let mut compared = 0;
    let mut rejected_source_only = 0;
    for case in fixture.cases {
        let casts: Vec<_> = case.spells.iter().map(input).collect();
        if case.source_only {
            assert!(
                InitialCastPlan::from_authored(&casts).is_err(),
                "{}",
                case.name
            );
            rejected_source_only += 1;
            continue;
        }
        assert_eq!(case.rank_queries, 0, "{}", case.name);
        // No rank value is passed to the implementation. Even rank0 constants
        // and Detect cannot accidentally acquire a fabricated rank dependency.
        assert!(case.spells.iter().all(|s| s.spell_rank == 0));
        let plan = if case.mechanism_only {
            assert!(
                InitialCastPlan::from_authored(&casts).is_err(),
                "{} widened admission",
                case.name
            );
            mechanism_plan(&case.spells)
        } else {
            InitialCastPlan::from_authored(&casts).unwrap()
        };
        let result = plan
            .evaluate(
                &case.rolls,
                InitialProficiencyMasks {
                    weapon: case.initial_weapon_mask,
                    armor: case.initial_armor_mask,
                },
            )
            .unwrap();
        assert_eq!(
            result.events().iter().map(event).collect::<Vec<_>>(),
            case.events,
            "{}",
            case.name
        );
        assert_eq!(
            result.final_masks(),
            InitialProficiencyMasks {
                weapon: case.final_weapon_mask,
                armor: case.final_armor_mask
            },
            "{}",
            case.name
        );
        assert_eq!(
            plan.occurrences()
                .iter()
                .map(InitialCastOccurrencePlan::targetless_mask)
                .collect::<Vec<_>>(),
            case.targetless_masks,
            "{}",
            case.name
        );
        assert_eq!(
            plan.occurrences()
                .iter()
                .map(InitialCastOccurrencePlan::self_mask)
                .collect::<Vec<_>>(),
            case.self_masks,
            "{}",
            case.name
        );
        plan.verify_evaluation(&result).unwrap();
        compared += 1;
    }
    assert_eq!(compared, 44);
    assert_eq!(rejected_source_only, 5);
}
