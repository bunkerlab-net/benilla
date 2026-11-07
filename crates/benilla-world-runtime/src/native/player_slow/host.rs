// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Object.cpp,
// Unit.cpp/Unit.h, SpellAuras.cpp and SpellMgr.cpp. Upstream GPL-2.0-or-later;
// Copyright CMaNGOS contributors. No warranty.
//! Explicit initial/current-host closure for the no-modifier slow-aura domain.
//! Source-shaped configuration is not producer certification or cast authority.
use crate::native::{InteractionControlSeed, NativeError, Profile, SpellSourceRows};
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlayerSlowCasterSource {
    pub spawn: u64,
    pub entry: u32,
    pub minimum_level: u8,
    pub maximum_level: u8,
    pub control: InteractionControlSeed,
    /// Effective creation flags, matched against the immutable NPC profile.
    pub unit_flags: u32,
    pub spell_list_id: u32,
    pub script_id: u32,
    pub ai_name: String,
    pub addon_auras: SpellSourceRows<u32>,
    pub spawn_addon_auras: SpellSourceRows<u32>,
    pub modifier_spells: SpellSourceRows<u32>,
    pub event_scripts: SpellSourceRows<u32>,
    pub spell_scripts: SpellSourceRows<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlayerSlowHostSource {
    pub max_player_level: u8,
    pub player_control: InteractionControlSeed,
    /// Complete applicable player spell/item/aura modifier sources. Initial
    /// passive metadata is independently validated from the source initializer.
    pub player_modifier_spells: SpellSourceRows<u32>,
    pub player_scripts: SpellSourceRows<String>,
    /// Complete per-spawn rows, including ordinary NPCs not scheduled to cast.
    pub npcs: SpellSourceRows<PlayerSlowCasterSource>,
}

fn empty<T>(rows: &SpellSourceRows<T>, dependency: &'static str) -> Result<(), NativeError> {
    match rows {
        SpellSourceRows::Unknown => Err(NativeError::AuthorityUnavailable(dependency)),
        SpellSourceRows::Complete(rows) if rows.is_empty() => Ok(()),
        SpellSourceRows::Complete(_) => Err(NativeError::UnsupportedMechanic(dependency)),
    }
}
fn independent(control: InteractionControlSeed) -> Result<(), NativeError> {
    if control.unit_state != 0
        || control.owner_guid != 0
        || control.charmer_guid != 0
        || control.mount_display_id != 0
        || control.shapeshift_form != 0
        || control.aura_flags != 0
    {
        return Err(NativeError::UnsupportedMechanic(
            "slow independent unmounted host control",
        ));
    }
    Ok(())
}

impl PlayerSlowHostSource {
    pub fn validate(&self, profile: &Profile) -> Result<(), NativeError> {
        if !(1..=63).contains(&self.max_player_level)
            || profile.player.level == 0
            || profile.player.level > self.max_player_level
            || profile
                .quest_progression
                .as_ref()
                .is_some_and(|g| g.maximum_player_level() != u32::from(self.max_player_level))
        {
            return Err(NativeError::UnsupportedMechanic("slow player level domain"));
        }
        independent(self.player_control)?;
        empty(
            &self.player_modifier_spells,
            "slow player spell/item/aura modifier closure",
        )?;
        empty(&self.player_scripts, "slow player script closure")?;
        // SPELL_AURA_ADD_FLAT_MODIFIER=107 and ADD_PCT_MODIFIER=108 can alter
        // ALL_EFFECTS, SPEED, DURATION or CHARGES. The source initializer already
        // narrows passives, but this boundary proves its own movement dependency.
        if profile.combat.as_ref().is_some_and(|genesis| {
            genesis.initial_auras.iter().any(|aura| {
                matches!(
                    aura.aura_type,
                    31 | 32 | 33 | 58 | 107 | 108 | 129 | 130 | 171 | 172 | 191
                )
            })
        }) {
            return Err(NativeError::UnsupportedMechanic(
                "slow initial player movement/spell modifiers",
            ));
        }
        let SpellSourceRows::Complete(rows) = &self.npcs else {
            return Err(NativeError::AuthorityUnavailable(
                "slow per-spawn caster source closure",
            ));
        };
        if rows.len() > 4096 {
            return Err(NativeError::TooLarge);
        }
        if rows.len() != profile.npcs.len() {
            return Err(NativeError::AuthorityUnavailable(
                "slow complete NPC caster coverage",
            ));
        }
        let mut spawns = BTreeSet::new();
        for source in rows {
            let npc = profile
                .npc(source.spawn)
                .ok_or(NativeError::InvalidProfile)?;
            if !spawns.insert(source.spawn)
                || source.entry != npc.entry
                || !(1..=63).contains(&source.minimum_level)
                || !(source.minimum_level..=63).contains(&source.maximum_level)
                || !(source.minimum_level..=source.maximum_level).contains(&npc.ai.level)
                || source.unit_flags != npc.health_regeneration.unit_flags
            {
                return Err(NativeError::InvalidProfile);
            }
            // The separate regeneration source proves ordinary/nonpet identity;
            // raw ownership/charm fields prove GetSpellModOwner cannot find a
            // controlling player for the NPC effect/duration/charge calculations.
            npc.health_regeneration
                .validate()
                .map_err(NativeError::CreatureRegeneration)?;
            independent(source.control)?;
            if source.unit_flags & 0x8 != 0
                || source.spell_list_id != 0
                || source.script_id != 0
                || !source.ai_name.is_empty()
            {
                return Err(NativeError::UnsupportedMechanic(
                    "slow ordinary nonscripted NPC caster",
                ));
            }
            empty(
                &source.addon_auras,
                "slow creature addon aura source closure",
            )?;
            empty(
                &source.spawn_addon_auras,
                "slow spawn addon aura source closure",
            )?;
            empty(
                &source.modifier_spells,
                "slow NPC spell modifier source closure",
            )?;
            empty(
                &source.event_scripts,
                "slow NPC event script source closure",
            )?;
            empty(
                &source.spell_scripts,
                "slow NPC spell script source closure",
            )?;
        }
        Ok(())
    }
}

#[cfg(any(test, feature = "synthetic-native-tests"))]
pub(crate) fn synthetic_host(profile: &Profile) -> PlayerSlowHostSource {
    let control = InteractionControlSeed {
        unit_state: 0,
        owner_guid: 0,
        charmer_guid: 0,
        mount_display_id: 0,
        shapeshift_form: 0,
        aura_flags: 0,
    };
    PlayerSlowHostSource {
        max_player_level: 63,
        player_control: control,
        player_modifier_spells: SpellSourceRows::Complete(vec![]),
        player_scripts: SpellSourceRows::Complete(vec![]),
        npcs: SpellSourceRows::Complete(
            profile
                .npcs
                .iter()
                .map(|npc| PlayerSlowCasterSource {
                    spawn: npc.ai.creature_id,
                    entry: npc.entry,
                    minimum_level: npc.ai.level,
                    maximum_level: npc.ai.level,
                    control,
                    unit_flags: npc.health_regeneration.unit_flags,
                    spell_list_id: 0,
                    script_id: 0,
                    ai_name: String::new(),
                    addon_auras: SpellSourceRows::Complete(vec![]),
                    spawn_addon_auras: SpellSourceRows::Complete(vec![]),
                    modifier_spells: SpellSourceRows::Complete(vec![]),
                    event_scripts: SpellSourceRows::Complete(vec![]),
                    spell_scripts: SpellSourceRows::Complete(vec![]),
                })
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source(host: &mut PlayerSlowHostSource) -> &mut PlayerSlowCasterSource {
        let SpellSourceRows::Complete(rows) = &mut host.npcs else {
            panic!("authored fixture")
        };
        &mut rows[0]
    }
    #[test]
    fn authored_host_is_bound_to_every_spawn_level_and_flags() {
        let profile = crate::native::synthetic_motion_fixture::profile();
        let host = synthetic_host(&profile);
        host.validate(&profile).unwrap();
        for mutation in 0..5 {
            let mut changed = host.clone();
            match mutation {
                0 => source(&mut changed).spawn += 100,
                1 => source(&mut changed).entry += 1,
                2 => source(&mut changed).minimum_level = 63,
                3 => source(&mut changed).unit_flags ^= 1,
                _ => changed.max_player_level = 0,
            }
            assert!(changed.validate(&profile).is_err());
        }
    }
    #[test]
    fn unknown_sources_and_modifier_or_control_hooks_never_mean_empty() {
        let profile = crate::native::synthetic_motion_fixture::profile();
        for mutation in 0..10 {
            let mut host = synthetic_host(&profile);
            match mutation {
                0 => host.npcs = SpellSourceRows::Unknown,
                1 => host.player_modifier_spells = SpellSourceRows::Unknown,
                2 => host.player_scripts = SpellSourceRows::Complete(vec!["script".into()]),
                3 => host.player_control.mount_display_id = 1,
                4 => source(&mut host).control.owner_guid = 1,
                5 => source(&mut host).control.charmer_guid = 1,
                6 => source(&mut host).spell_list_id = 1,
                7 => source(&mut host).addon_auras = SpellSourceRows::Unknown,
                8 => source(&mut host).spawn_addon_auras = SpellSourceRows::Complete(vec![1]),
                _ => source(&mut host).modifier_spells = SpellSourceRows::Complete(vec![1]),
            }
            assert!(host.validate(&profile).is_err());
        }
    }
}
