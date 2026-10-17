// SPDX-License-Identifier: GPL-3.0-or-later
//! Shared counter and immutable source-owner binding, separate from arithmetic.
use super::*;

impl Profile {
    /// Retain complete source inputs; this does not authorize native world entry.
    pub fn initialize_creature_generations(
        &mut self,
        rows: benilla_sim::creature_init::Rows<CreatureGenerationSource>,
        cold_entry_order: Vec<u64>,
    ) -> Result<(), NativeError> {
        let catalog = CreatureGenerationCatalog::validate_sources(rows, cold_entry_order)?;
        let mut staged = self.clone();
        staged.creature_generations = Some(catalog);
        staged.validate()?;
        *self = staged;
        Ok(())
    }
}

pub(super) fn validate_prefix(
    profile: &Profile,
    state: &State,
    player_end: u64,
) -> Result<u64, NativeError> {
    let Some(catalog) = &profile.creature_generations else {
        if state
            .npcs
            .values()
            .any(|npc| npc.source_generation.is_some())
        {
            return Err(NativeError::AuthorityBinding);
        }
        return Ok(player_end);
    };
    if catalog.cold_entry_order().len() != state.npcs.len() {
        return Err(NativeError::AuthorityBinding);
    }
    let profile_identity = profile.digest();
    let mut cursor = player_end;
    let mut spans = Vec::new();
    for spawn in catalog.cold_entry_order() {
        let npc = state.npcs.get(spawn).ok_or(NativeError::AuthorityBinding)?;
        let owner = npc
            .source_generation
            .as_ref()
            .ok_or(NativeError::AuthorityBinding)?;
        if owner.npc_id() != npc.id
            || owner.world() != state.world
            || owner.profile_identity() != profile_identity
            || npc.ai.generation() != Some(npc.id.generation)
            || npc.ai.effective_profile() != &npc.effective_profile(profile)?.ai
            || npc.ai.mainhand_timer_limit_ms() != owner.result().mainhand_interval_ms.max(100)
            || npc.health > owner.result().stats.resources.max_health
            || (npc.health == 0) != owner.current_death_tick().is_some()
        {
            return Err(NativeError::AuthorityBinding);
        }
        let at_ms = |tick: u64| {
            tick.checked_mul(u64::from(profile.tick_ms))
                .and_then(|value| value.checked_add(profile.clock_origin_ms))
                .ok_or(NativeError::Limit)
        };
        if owner.current_death_tick().map(at_ms).transpose()? != npc.ai.death_at_ms()
            || owner.current_corpse_removed_tick().map(at_ms).transpose()?
                != npc.ai.despawned_at_ms()
        {
            return Err(NativeError::AuthorityBinding);
        }
        let loot_row = profile
            .quest_progression
            .as_ref()
            .and_then(|progression| progression.loot.as_ref())
            .and_then(|loot| loot.creatures.get(&owner.result().entry));
        for life in owner.history() {
            for call in life.corpse_timer_calls() {
                if loot_row.is_none_or(|row| {
                    row.no_loot || call.delay_ms != row.corpse_acceleration_decay_ms
                }) || call.at_ms < profile.clock_origin_ms
                    || !(call.at_ms - profile.clock_origin_ms)
                        .is_multiple_of(u64::from(profile.tick_ms))
                {
                    return Err(NativeError::AuthorityBinding);
                }
            }
        }
        owner.validate_corpse_timer(&npc.ai, profile.tick_ms, state.now_ms)?;
        for draw in owner.genesis_draws() {
            if draw.first_counter != cursor || draw.consumed == 0 {
                return Err(NativeError::InvalidCheckpoint);
            }
            cursor = cursor
                .checked_add(u64::from(draw.consumed))
                .ok_or(NativeError::Limit)?;
        }
        for (tick, draw) in owner.all_draws() {
            let end = draw
                .first_counter
                .checked_add(u64::from(draw.consumed))
                .ok_or(NativeError::Limit)?;
            if tick > state.tick || draw.consumed == 0 || end > state.rng_counter {
                return Err(NativeError::InvalidCheckpoint);
            }
            spans.push((draw.first_counter, end, tick));
        }
    }
    // Retained allocations share one counter domain, including independently
    // recomputed loot/EventAI records. A valid sample cannot own the same word twice.
    for npc in state.npcs.values() {
        if let Some(loot) = &npc.corpse_loot {
            let death_tick = loot
                .death_at_ms
                .checked_sub(profile.clock_origin_ms)
                .filter(|elapsed| elapsed.is_multiple_of(u64::from(profile.tick_ms)))
                .map(|elapsed| elapsed / u64::from(profile.tick_ms))
                .ok_or(NativeError::InvalidCheckpoint)?;
            if loot.first_counter < cursor
                || loot.end_counter < loot.first_counter
                || loot.end_counter > state.rng_counter
                || death_tick == 0
                || death_tick > state.tick
            {
                return Err(NativeError::InvalidCheckpoint);
            }
            let death = npc
                .source_generation
                .as_ref()
                .and_then(|owner| owner.historical(loot.corpse))
                .and_then(|life| life.death_draw())
                .ok_or(NativeError::InvalidCheckpoint)?;
            if loot.end_counter > death.first_counter {
                return Err(NativeError::InvalidCheckpoint);
            }
            if loot.first_counter != loot.end_counter {
                spans.push((loot.first_counter, loot.end_counter, death_tick));
            }
        }
    }
    if let Some(events) = &state.event_ai {
        for history in &events.history {
            if let EventAiHistory::EnterCombat(event) = history {
                for action in &event.actions {
                    for first in [Some(action.chance_counter), action.action_counter]
                        .into_iter()
                        .flatten()
                    {
                        let end = first.checked_add(1).ok_or(NativeError::Limit)?;
                        if first < cursor
                            || end > state.rng_counter
                            || event.callback.tick == 0
                            || event.callback.tick > state.tick
                        {
                            return Err(NativeError::InvalidCheckpoint);
                        }
                        spans.push((first, end, event.callback.tick));
                    }
                }
            }
        }
    }
    spans.sort_unstable();
    let mut previous_end = player_end;
    let mut previous_tick = 0;
    for (first, end, tick) in spans {
        if tick < previous_tick
            || first < previous_end
            || (tick == 0 && end > cursor)
            || (tick != 0 && first < cursor)
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        previous_end = end;
        previous_tick = tick;
    }
    Ok(cursor)
}

pub(super) fn validate_other_sources(profile: &Profile) -> Result<(), NativeError> {
    let Some(generations) = &profile.creature_generations else {
        return Ok(());
    };
    for spawn in generations.cold_entry_order() {
        let source = generations
            .source(*spawn)
            .ok_or(NativeError::AuthorityBinding)?;
        if let Some(capabilities) = &profile.creature_capabilities {
            let capability = capabilities
                .source(*spawn)
                .ok_or(NativeError::AuthorityBinding)?;
            let benilla_sim::creature_init::Rows::Complete(raw_spawns) =
                &source.input.spawn_template
            else {
                return Err(NativeError::AuthorityBinding);
            };
            let CreatureCapabilityRows::Complete(capability_spawns) = &capability.spawn_templates
            else {
                return Err(NativeError::AuthorityBinding);
            };
            let ([raw], [captured]) = (raw_spawns.as_slice(), capability_spawns.as_slice()) else {
                return Err(NativeError::AuthorityBinding);
            };
            let benilla_sim::creature_init::Rows::Complete(raw_features) =
                &source.input.external_features
            else {
                return Err(NativeError::AuthorityBinding);
            };
            let CreatureCapabilityRows::Complete(capability_features) =
                &capability.external_features
            else {
                return Err(NativeError::AuthorityBinding);
            };
            if capability.spawn_id != source.spawn_id
                || capability.spawn_entry != source.spawn_entry
                || capability.requested_spawn_template_entry
                    != source.requested_spawn_template_entry
                || captured.entry != raw.entry
                || captured.spawn_flags != raw.spawn_flags
                || captured.relay_id != raw.relay_id
                || captured.string_id != raw.string_id
                || capability_features != raw_features
            {
                return Err(NativeError::AuthorityBinding);
            }
        }
    }
    Ok(())
}
