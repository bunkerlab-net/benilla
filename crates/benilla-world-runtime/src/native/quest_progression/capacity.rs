// SPDX-License-Identifier: GPL-3.0-or-later
//! Conservative admission budgets for the complete supplied progression closure.
use super::*;

const MAX_CALLS: usize = 4096;
// Keep half the existing envelope available for NPCs, execution witnesses,
// inventory, quests and other owner state. Final combined size is still checked
// transactionally by make_checkpoint; this does not enlarge its 8 MiB limit.
const MAX_PROGRESSION_HISTORY_BYTES: usize = MAX_BYTES / 2;

/// Same object/array/string shape, with every integer charged its widest JSON
/// signed-32 representation (all serialized history scalars are at most 32 bits).
/// This is only a byte-size proof, never generated game state.
fn wide_size(value: &impl Serialize) -> Result<usize, NativeError> {
    fn size(v: &serde_json::Value) -> usize {
        match v {
            serde_json::Value::Null => 4,
            serde_json::Value::Bool(_) => 5,
            serde_json::Value::Number(_) => 11,
            serde_json::Value::String(s) => {
                // A break can occur before the first level callback, when
                // otherwise identical skill rows still say Unchanged.
                if matches!(s.as_str(), "New" | "Changed" | "Unchanged" | "Deleted") {
                    11
                } else {
                    serde_json::to_string(s).unwrap().len()
                }
            }
            serde_json::Value::Array(v) => 2 + v.iter().map(|v| size(v) + 1).sum::<usize>(),
            serde_json::Value::Object(v) => {
                2 + v
                    .iter()
                    .map(|(k, v)| serde_json::to_string(k).unwrap().len() + 2 + size(v))
                    .sum::<usize>()
            }
        }
    }
    let value = serde_json::to_value(value).map_err(|_| NativeError::InvalidProfile)?;
    fn valid_width(v: &serde_json::Value) -> bool {
        match v {
            serde_json::Value::Number(n) => {
                n.as_u64().is_some_and(|n| n <= u64::from(u32::MAX))
                    || n.as_i64()
                        .is_some_and(|n| n >= i64::from(i32::MIN) && n <= i64::from(i32::MAX))
            }
            serde_json::Value::Array(v) => v.iter().all(valid_width),
            serde_json::Value::Object(v) => v.values().all(valid_width),
            _ => true,
        }
    }
    if !valid_width(&value) {
        return Err(NativeError::UnsupportedMechanic(
            "progression history scalar width",
        ));
    }
    Ok(size(&value))
}

pub(super) fn validate(
    g: &QuestProgressionGenesis,
    host: &ProgressionHost,
    final_view: &pg::Snapshot,
) -> Result<(), NativeError> {
    let training = host.training.snapshot();
    let defense = final_view
        .skills
        .iter()
        .find(|s| s.id == combat::DEFENSE_SKILL);
    let initial_defense = g
        .initial
        .view()
        .skills
        .iter()
        .find(|s| s.id == combat::DEFENSE_SKILL);
    // Defense values only increase and this domain has no skill reset/repair.
    // Gain >1 and automatic maximization can only reduce this upper bound.
    let defense_count = match (defense, initial_defense) {
        (Some(last), Some(first)) => usize::from(last.maximum.saturating_sub(first.value)),
        (None, None) => 0,
        _ => return Err(NativeError::InvalidProfile),
    };
    // The consecutive-level source probe fixes the defense value. Later real
    // combat may raise it between levels. Do not admit a new skill-threshold
    // learning branch that this probe has not executed. Non-auto repair rows
    // depend on the unchanged step, not defense value; masked rows are inert.
    let input = host.training.input();
    let init::Rows::Complete(rows) = &input.skill_abilities else {
        return Err(NativeError::InvalidProfile);
    };
    let race = 1u32 << (input.race - 1);
    let class = 1u32 << (input.class - 1);
    if let (Some(first), Some(last)) = (initial_defense, defense) {
        if rows.iter().any(|r| {
            r.skill == combat::DEFENSE_SKILL
                && (r.race_mask == 0 || r.race_mask & race != 0)
                && (r.class_mask == 0 || r.class_mask & class != 0)
                && r.learn_on_get_skill != 0
                && r.required_value > u32::from(first.value)
                && r.required_value <= u32::from(last.maximum)
        }) {
            return Err(NativeError::UnsupportedMechanic(
                "unproved defense-sensitive progression training",
            ));
        }
    }
    let recipe = g.initial_combat.recipe.rebase(
        final_view.level,
        final_view.modifiers.create,
        final_view.modifiers.create_health,
        final_view.modifiers.create_mana,
    )?;
    let breakable: Vec<_> = recipe
        .snapshot()
        .items
        .iter()
        .filter(|i| i.max_durability != 0)
        .map(|i| i.slot)
        .collect();
    let derived_count = host
        .derived_calls
        .len()
        .checked_add(defense_count)
        .and_then(|n| n.checked_add(breakable.len()))
        .ok_or(NativeError::TooLarge)?;
    if training.calls.len() > MAX_CALLS || derived_count > MAX_CALLS {
        return Err(NativeError::UnsupportedMechanic(
            "complete progression journal capacity",
        ));
    }
    let mut bytes = wide_size(&host.derived_calls)? + wide_size(&training)?;
    // Defense gains can alter digits in later callbacks. wide_size already
    // accounts for all integer widths. Learning at a later skill threshold may
    // add notifications; every notification requires an actually supplied spell.
    bytes = bytes
        .checked_add(
            training
                .calls
                .len()
                .checked_mul(host.training.input().spells.len())
                .and_then(|n| n.checked_mul(12))
                .ok_or(NativeError::TooLarge)?,
        )
        .ok_or(NativeError::TooLarge)?;
    let defense_call = DerivedCall::Defense {
        view: Box::new(final_view.clone()),
        inputs: host.inputs.clone(),
    };
    bytes = bytes
        .checked_add(
            defense_count
                .checked_mul(wide_size(&defense_call)?)
                .ok_or(NativeError::TooLarge)?,
        )
        .ok_or(NativeError::TooLarge)?;
    // Future books can include known rows not reached by the no-defense probe.
    // Account for every supplied row and actual availability reason, without
    // treating these sizing bounds as learned-state authority.
    for row in &host.training.input().spells {
        let reason = host
            .training
            .input()
            .active_support
            .iter()
            .find(|a| a.spell_id == row.id)
            .and_then(|a| a.unavailable_reason.as_ref());
        let known = init::KnownAbility {
            spell_id: row.id,
            active: true,
            availability: init::AbilityAvailability::Unavailable(
                reason.cloned().unwrap_or_default(),
            ),
        };
        bytes = bytes
            .checked_add(wide_size(&known)?)
            .ok_or(NativeError::TooLarge)?;
    }
    // Both recipe broken-slot lists plus the recompute disabled-weapon set
    // may already contain earlier breaks absent from this one-item probe.
    let broken_slots_extra = 3 * breakable.len() * 12;
    for slot in breakable {
        let before_recipe = recipe.snapshot();
        let live = init::Resources {
            health: final_view.numeric.max_health,
            max_health: final_view.numeric.max_health,
            mana: final_view.numeric.max_powers[0],
            max_mana: final_view.numeric.max_powers[0],
        };
        let mut after = recipe.clone();
        let recompute = after.set_item_broken(slot, true, live)?;
        let after_recipe = after.snapshot();
        let call = DerivedCall::ItemBreak {
            before: Box::new(final_view.clone()),
            before_inputs: g.inputs_for_recipe(&before_recipe),
            after_inputs: g.inputs_for_recipe(&after_recipe),
            before_recipe: Box::new(before_recipe),
            after_recipe: Box::new(after_recipe),
            slot,
            recompute: Box::new(recompute),
        };
        bytes = bytes
            .checked_add(wide_size(&call)? + broken_slots_extra)
            .ok_or(NativeError::TooLarge)?;
    }
    if bytes > MAX_PROGRESSION_HISTORY_BYTES {
        return Err(NativeError::UnsupportedMechanic(
            "complete progression encoded capacity",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_history_admission_rejects_count_and_encoded_capacity() {
        let (mut p, input, rows) =
            crate::native::synthetic_quest_fixture::setup_with_levels(60, 60, true, 0x3f800000);
        p.initialize_quest_progression(&input, rows).unwrap();
        let g = p.quest_progression.as_ref().unwrap();
        let mut probe = g.initial.clone();
        let mut host = ProgressionHost {
            training: g.training.clone(),
            derived: DerivedStats::from_snapshot(probe.view(), &g.initial_derived_inputs).unwrap(),
            derived_calls: vec![],
            inputs: g.initial_derived_inputs.clone(),
            audit: probe.view().audit,
        };
        for level in 2..=60 {
            let tx = pg::give_level(&g.levels, &probe, &host, level).unwrap();
            probe = tx.state;
            host = tx.host;
        }
        validate(g, &host, probe.view()).unwrap();
        let mut inflated = host.clone();
        inflated
            .derived_calls
            .resize(4096, host.derived_calls[0].clone());
        assert_eq!(
            validate(g, &inflated, probe.view()),
            Err(NativeError::UnsupportedMechanic(
                "complete progression journal capacity"
            ))
        );
        // Count remains within4096, but the exact admitted history shape cannot
        // reserve the byte budget. This sizes real callback records, not rows.
        let mut inflated = host;
        inflated
            .derived_calls
            .resize(3000, inflated.derived_calls[0].clone());
        assert_eq!(
            validate(g, &inflated, probe.view()),
            Err(NativeError::UnsupportedMechanic(
                "complete progression encoded capacity"
            ))
        );
        assert!(wide_size(&u64::MAX).is_err());
    }
}
