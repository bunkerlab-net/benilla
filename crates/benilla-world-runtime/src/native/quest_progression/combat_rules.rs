// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 World.cpp:443,464-476,661-662,703-704,743.
use super::*;
use benilla_sim::world_rules::CombatRules;

pub(super) fn validate(
    profile: &Profile,
    rules: &CombatRules,
    scripts: Option<&QuestScriptRules>,
) -> Result<(), NativeError> {
    rules.validate()?;
    let stock = CombatRules::cmangos_classic_stock();
    // These hosts are not implemented. Explicit configuration is not execution evidence.
    if rules.family_assistance_delay_ms != stock.family_assistance_delay_ms
        || rules.family_flee_assistance_radius_bits != stock.family_flee_assistance_radius_bits
        || rules.family_flee_delay_ms != stock.family_flee_delay_ms
        || rules.linking_aggro_delay_ms != stock.linking_aggro_delay_ms
        || rules.listen_yell_range_bits != stock.listen_yell_range_bits
        || rules.listen_text_emote_range_bits != stock.listen_text_emote_range_bits
    {
        return Err(NativeError::UnsupportedMechanic(
            "combat setting configured but no execution host",
        ));
    }
    if profile.npcs.is_empty()
        && (rules.aggro_rate_bits != stock.aggro_rate_bits
            || rules.leash_radius_bits != stock.leash_radius_bits)
        || profile.npcs.iter().any(|npc| {
            npc.ai.aggro_rate.bits() != rules.aggro_rate_bits
                || npc.ai.leash_radius.bits() != rules.leash_radius_bits
        })
    {
        return Err(NativeError::UnsupportedMechanic(
            "world combat rules/creature AI mismatch",
        ));
    }
    if let Some(motion) = &profile.source_motion {
        if motion.aggro_rate.bits() != rules.aggro_rate_bits
            || motion.creature_initial_aggro_delay_ms != rules.respawn_aggro_delay_ms
            || motion.relocation_aggro_delay_ms != rules.relocation_notify_delay_ms
        {
            return Err(NativeError::UnsupportedMechanic(
                "world combat rules/motion host mismatch",
            ));
        }
    } else if rules.respawn_aggro_delay_ms != stock.respawn_aggro_delay_ms
        || rules.relocation_notify_delay_ms != stock.relocation_notify_delay_ms
    {
        return Err(NativeError::UnsupportedMechanic(
            "combat notify delay configured but no motion host",
        ));
    }
    if let Some(assistance) = profile.combat_assistance() {
        if assistance.delay_ms() != rules.check_for_help_aggro_delay_ms
            || assistance.family_radius().bits() != rules.family_assistance_radius_bits
        {
            return Err(NativeError::UnsupportedMechanic(
                "world combat rules/assistance host mismatch",
            ));
        }
    } else if rules.check_for_help_aggro_delay_ms != stock.check_for_help_aggro_delay_ms
        || rules.family_assistance_radius_bits != stock.family_assistance_radius_bits
    {
        return Err(NativeError::UnsupportedMechanic(
            "family assistance configured but no assistance host",
        ));
    }
    match profile.peer_help_radius() {
        Some(radius) if radius.bits() == rules.check_for_help_radius_bits => {}
        None if rules.check_for_help_radius_bits == stock.check_for_help_radius_bits => {}
        _ => {
            return Err(NativeError::UnsupportedMechanic(
                "world combat rules/peer help host mismatch",
            ))
        }
    }
    let event_ai = profile.event_ai();
    if event_ai.is_some_and(|host| host.say_range().bits() != rules.listen_say_range_bits)
        || scripts.is_some_and(|host| host.say_range.bits() != rules.listen_say_range_bits)
        || (event_ai.is_none()
            && scripts.is_none()
            && rules.listen_say_range_bits != stock.listen_say_range_bits)
    {
        return Err(NativeError::UnsupportedMechanic(
            "world combat rules/text audience host mismatch",
        ));
    }
    Ok(())
}
