// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f: Object.cpp,
// Unit.cpp, SpellMgr.cpp, SpellAuras.cpp, SpellStacking.cpp and DBCStructure.h.
// Copyright CMaNGOS contributors; upstream GPL-2.0-or-later. No warranty.
//! Explicit effective-source closure for a bounded ordinary decrease-speed aura.
//! Validation cannot certify a producer or authorize a cast/application.
use crate::native::{NativeError, SpellAuthoritySource, SpellHookSources, SpellSourceRows};
use crate::Digest;
use benilla_sim::classic_spell::{EffectValue, SpellFloat};
use serde::Serialize;
use std::collections::BTreeMap;

pub const PLAYER_SLOW_SOURCE_POLICY: &str = "cmangos-8ec338a1-player-slow-source-v2";
const DEBUFF: u32 = 0x0400_0000;
// Player::SetEnvironmentFlags: HIGH_LIQUID apply/clear only.
pub(super) const WATER_INTERRUPT_FLAGS: u32 = 0x180;
const MAX_DURATION_MS: u32 = 3_600_000;

/// Full SpellEntry after spell_template overrides plus dependencies not carried
/// by the existing spell authority input. Complete(empty) means a captured empty
/// result; Unknown never proves absence. These are source inputs, not capabilities.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlayerSlowSource {
    pub spell: SpellAuthoritySource,
    pub spell_script_target: SpellSourceRows<u32>,
    pub spell_template_scripts: SpellSourceRows<u32>,
    pub aura_scripts: SpellSourceRows<String>,
    pub linked_spells: SpellSourceRows<u32>,
    /// Applicable caster and target spell/item/aura modifier source IDs, including
    /// SPELLMOD_ALL_EFFECTS, SPEED, DURATION and CHARGES. Host genesis validation
    /// must additionally prove that current units match this closed source set.
    pub modifier_spells: SpellSourceRows<u32>,
}

/// No Deserialize or public fields: the only constructor checks every source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlayerSlowCatalog {
    policy: &'static str,
    sources: BTreeMap<u32, PlayerSlowSource>,
}

/// Derived holder inputs. Retaining client words does not make these fixture
/// inputs a verified client DBC or native projection/content capability.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SlowRecipe {
    pub spell_id: u32,
    pub effect_slot: u8,
    pub amount: i32,
    pub duration_ms: u32,
    pub max_duration_ms: u32,
    pub attributes: u32,
    pub attributes_ex3: u32,
    pub caster_aura_state: u32,
    pub binding: Digest,
    pub client_spell_words: Vec<u32>,
}

fn empty<T>(rows: &SpellSourceRows<T>, name: &'static str) -> Result<(), NativeError> {
    match rows {
        SpellSourceRows::Unknown => Err(NativeError::AuthorityUnavailable(name)),
        SpellSourceRows::Complete(rows) if rows.is_empty() => Ok(()),
        SpellSourceRows::Complete(_) => Err(NativeError::UnsupportedMechanic(name)),
    }
}
fn hooks(h: &SpellHookSources) -> Result<(), NativeError> {
    empty(&h.spell_area, "slow spell_area source closure")?;
    empty(&h.spell_script_names, "slow spell scripts source closure")?;
    empty(&h.conditions, "slow conditions source closure")?;
    empty(&h.spell_proc_event, "slow spell_proc_event source closure")?;
    empty(&h.spell_affect, "slow spell_affect source closure")?;
    empty(&h.spell_bonus_data, "slow spell_bonus_data source closure")?;
    empty(&h.spell_threat, "slow spell_threat source closure")?;
    empty(&h.spell_elixir, "slow spell_elixir source closure")?;
    // Unlike the old singleton spell-holder domain, distinct slow IDs coexist.
    // Group/rank comparisons are reachable and must have no unsupported rows.
    empty(&h.spell_group_membership, "slow spell group source closure")?;
    empty(&h.spell_rank_chain, "slow spell rank-chain source closure")
}

impl PlayerSlowCatalog {
    pub fn validate_sources(input: Vec<PlayerSlowSource>) -> Result<Self, NativeError> {
        if input.len() > 16 {
            return Err(NativeError::TooLarge);
        }
        let mut sources = BTreeMap::new();
        for source in input {
            source.validate()?;
            if sources.insert(source.spell.words[0], source).is_some() {
                return Err(NativeError::InvalidProfile);
            }
        }
        Ok(Self {
            policy: PLAYER_SLOW_SOURCE_POLICY,
            sources,
        })
    }
    pub fn validate(&self) -> Result<(), NativeError> {
        if self.policy != PLAYER_SLOW_SOURCE_POLICY || self.sources.len() > 16 {
            return Err(NativeError::InvalidProfile);
        }
        for (&id, source) in &self.sources {
            source.validate()?;
            if id != source.spell.words[0] {
                return Err(NativeError::InvalidProfile);
            }
        }
        Ok(())
    }
    pub fn source(&self, spell_id: u32) -> Option<&PlayerSlowSource> {
        self.sources.get(&spell_id)
    }
    pub(super) fn interrupt_flags(&self, spell_id: u32) -> Result<u32, NativeError> {
        self.source(spell_id)
            .map(|source| source.spell.words[22])
            .ok_or(NativeError::AuthorityUnavailable(
                "player decrease-speed effective spell source",
            ))
    }
    pub fn spell_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.sources.keys().copied()
    }
    pub fn recipe(&self, spell_id: u32, caster_level: u8) -> Result<SlowRecipe, NativeError> {
        let source = self
            .source(spell_id)
            .ok_or(NativeError::AuthorityUnavailable(
                "player decrease-speed effective spell source",
            ))?;
        let slot = source.validate()?;
        let amount = source.amount(slot, caster_level)?;
        let w = &source.spell.words;
        let duration_ms = source.duration()?;
        Ok(SlowRecipe {
            spell_id,
            effect_slot: slot as u8,
            amount,
            duration_ms,
            // SpellAuraHolder constructor assigns both to CalculateSpellDuration;
            // for the ordinary NPC branch Duration[2] does not replace this value.
            max_duration_ms: duration_ms,
            attributes: w[6],
            attributes_ex3: w[9],
            caster_aura_state: w[16],
            binding: crate::digest(b"benilla-player-slow-source-v2", source),
            client_spell_words: w.clone(),
        })
    }
}

impl PlayerSlowSource {
    fn duration(&self) -> Result<u32, NativeError> {
        let d = self
            .spell
            .duration
            .ok_or(NativeError::AuthorityUnavailable(
                "slow linked SpellDuration source row",
            ))?;
        if d.id == 0
            || d.id != self.spell.words[30]
            || d.base_ms == -1
            || d.maximum_ms == -1
            || !(1..=MAX_DURATION_MS).contains(&d.base_ms.unsigned_abs())
            || !(1..=MAX_DURATION_MS).contains(&d.maximum_ms.unsigned_abs())
        {
            return Err(NativeError::UnsupportedMechanic(
                "slow finite duration source",
            ));
        }
        // GetSpellDuration uses abs(Duration[0]); Duration[1] is not consulted by
        // CalculateSpellDuration. NPCs have no player combo-point interpolation.
        Ok(d.base_ms.unsigned_abs())
    }
    fn amount(&self, slot: usize, level: u8) -> Result<i32, NativeError> {
        if !(1..=63).contains(&level) {
            return Err(NativeError::UnsupportedMechanic("slow NPC caster level"));
        }
        let w = &self.spell.words;
        let value = EffectValue {
            base_points: w[76 + slot] as i32,
            base_dice: w[67 + slot] as i32,
            die_sides: w[64 + slot] as i32,
            points_per_level: SpellFloat::from_bits(w[73 + slot])?,
            dice_per_level: SpellFloat::from_bits(w[70 + slot])?,
            base_level: w[28] as u8,
            spell_level: w[29] as u8,
            max_level: w[27] as u8,
        };
        // Unit::GetSpellRank (ordinary NPC) starts at GetLevel, not level * 5.
        let rank = if value.max_level > 0 {
            u16::from(level).min(u16::from(value.max_level) * 5)
        } else {
            u16::from(level)
        };
        if value.roll_range(rank)?.is_some() {
            return Err(NativeError::UnsupportedMechanic(
                "slow random effect amount",
            ));
        }
        let amount = value.evaluate(rank, None)?;
        if !(-1_000_000..=-1).contains(&amount) {
            return Err(NativeError::UnsupportedMechanic(
                "slow nonnegative effect amount",
            ));
        }
        Ok(amount)
    }
    fn validate(&self) -> Result<usize, NativeError> {
        let raw = &self.spell;
        let w = &raw.words;
        if w.len() != 173 || w[0] == 0 || w[0] > u16::MAX.into() || w[1] > 6 {
            return Err(NativeError::InvalidProfile);
        }
        // Positive ID audit, not an open-ended claim that all hardcoded source
        // exceptions have been excluded by the morphology. 1604 supplies no
        // default values. 61004/61005 are explicitly authored fixture identities.
        if !matches!(w[0], 1604 | 61004 | 61005) {
            return Err(NativeError::UnsupportedMechanic(
                "audited slow spell ID domain",
            ));
        }
        hooks(&raw.hooks)?;
        empty(
            &self.spell_script_target,
            "slow spell_script_target source closure",
        )?;
        empty(
            &self.spell_template_scripts,
            "slow spell_template scripts source closure",
        )?;
        empty(
            &self.aura_scripts,
            "slow compiled aura script source closure",
        )?;
        empty(&self.linked_spells, "slow linked spell source closure")?;
        empty(
            &self.modifier_spells,
            "slow caster/target modifier source closure",
        )?;
        if raw.server_side != 0
            || raw.server_attributes != 0
            || raw.bonus_coefficients != [0; 3]
            || raw.attack_power_coefficients != [0; 3]
            || raw.category_flags != 0
            || raw.facing_flags != 0
        {
            return Err(NativeError::UnsupportedMechanic("slow server spell fields"));
        }
        // Every word is classified. Text/visual words are retained; priority119
        // is semantic (debuff limit), so it remains zero. All unlisted words must
        // be zero, including family/rank flags, mechanics, passive/death-persist,
        // proc/heartbeat, non-water interruptions, cost, cooldown, item and triggered effects.
        for (index, &word) in w.iter().enumerate() {
            let retained = matches!(index, 0 | 1 | 3 | 6 | 13 | 18 | 22 | 27..=30 | 36 | 38 | 58
                | 61..=114 | 115..=118 | 120..=155 | 164 | 166 | 167..=169);
            if !retained && word != 0 {
                return Err(NativeError::UnsupportedMechanic(
                    "slow raw spell semantic field",
                ));
            }
        }
        if w[22] & !WATER_INTERRUPT_FLAGS != 0 {
            return Err(NativeError::UnsupportedMechanic(
                "slow non-water aura interrupt flags",
            ));
        }
        if w[6] & !DEBUFF != 0
            || !matches!(w[13], 0 | 2)
            || w[27..=29].iter().any(|level| *level > 63)
            || w[58] != u32::MAX
            || w[164] > 1
        {
            return Err(NativeError::UnsupportedMechanic(
                "slow ordinary harmful spell",
            ));
        }
        if raw.cast_time.id == 0
            || raw.cast_time.id != w[18]
            || raw.cast_time.base_ms != 0
            || raw.cast_time.per_level_ms != 0
            || raw.cast_time.minimum_ms != 0
            || raw.range.id == 0
            || raw.range.id != w[36]
            || raw.range.flags != 0
            || raw.range.minimum.bits() & 0x8000_0000 != 0
            || raw.range.maximum.bits() & 0x8000_0000 != 0
            || raw.range.minimum.bits() > raw.range.maximum.bits()
        {
            return Err(NativeError::UnsupportedMechanic(
                "slow linked cast-time/range source",
            ));
        }
        self.duration()?;
        let mut active = None;
        for slot in 0..3 {
            if w[61 + slot] == 0 {
                if (64..=112).step_by(3).any(|base| w[base + slot] != 0) || w[167 + slot] != 0 {
                    return Err(NativeError::UnsupportedMechanic(
                        "slow nonempty empty effect",
                    ));
                }
                continue;
            }
            if active.replace(slot).is_some()
                || w[61 + slot] != 6
                || w[91 + slot] != 33
                || w[79 + slot] != 0
                || w[82 + slot] != 6
                || w[85 + slot] != 0
                || w[88 + slot] != 0
                || w[94 + slot] != 0
                || w[97 + slot] != 0
                || w[100 + slot] > 1
                || w[103 + slot] != 0
                || w[106 + slot] != 0
                || w[109 + slot] != 0
                || w[112 + slot] != 0
                || !matches!(w[167 + slot], 0 | 0x3f80_0000)
            {
                return Err(NativeError::UnsupportedMechanic(
                    "slow single nonperiodic enemy aura",
                ));
            }
        }
        let slot = active.ok_or(NativeError::UnsupportedMechanic(
            "missing decrease-speed aura",
        ))?;
        // Prove the complete admitted NPC-level domain, including dice-per-level
        // branches. No level is allowed to silently require an omitted RNG draw.
        for level in 1..=63 {
            self.amount(slot, level)?;
        }
        Ok(slot)
    }
}

#[cfg(any(test, feature = "synthetic-native-tests"))]
pub(crate) fn synthetic_source(spell_id: u32, amount: i32, duration_ms: u32) -> PlayerSlowSource {
    use crate::native::{SpellCastTimeSource, SpellDurationSource, SpellRangeSource};
    let mut words = vec![0; 173];
    words[0] = spell_id;
    words[6] = DEBUFF;
    words[13] = 2;
    words[18] = 61001;
    words[30] = 61002;
    words[36] = 61003;
    words[58] = u32::MAX;
    words[61] = 6;
    words[64] = 1;
    words[67] = 1;
    words[76] = amount.saturating_sub(1) as u32;
    words[82] = 6;
    words[91] = 33;
    words[100] = 1;
    words[167] = 0x3f80_0000;
    PlayerSlowSource {
        spell: SpellAuthoritySource {
            words,
            server_side: 0,
            server_attributes: 0,
            bonus_coefficients: [0; 3],
            attack_power_coefficients: [0; 3],
            cast_time: SpellCastTimeSource {
                id: 61001,
                base_ms: 0,
                per_level_ms: 0,
                minimum_ms: 0,
            },
            duration: Some(SpellDurationSource {
                id: 61002,
                base_ms: duration_ms as i32,
                per_level_ms: 0,
                maximum_ms: duration_ms as i32,
            }),
            range: SpellRangeSource {
                id: 61003,
                minimum: SpellFloat::ZERO,
                maximum: SpellFloat::ONE,
                flags: 0,
            },
            category_flags: 0,
            facing_flags: 0,
            hooks: SpellHookSources {
                spell_area: SpellSourceRows::Complete(vec![]),
                spell_script_names: SpellSourceRows::Complete(vec![]),
                conditions: SpellSourceRows::Complete(vec![]),
                spell_proc_event: SpellSourceRows::Complete(vec![]),
                spell_affect: SpellSourceRows::Complete(vec![]),
                spell_bonus_data: SpellSourceRows::Complete(vec![]),
                spell_threat: SpellSourceRows::Complete(vec![]),
                spell_elixir: SpellSourceRows::Complete(vec![]),
                spell_group_membership: SpellSourceRows::Complete(vec![]),
                spell_rank_chain: SpellSourceRows::Complete(vec![]),
            },
        },
        spell_script_target: SpellSourceRows::Complete(vec![]),
        spell_template_scripts: SpellSourceRows::Complete(vec![]),
        aura_scripts: SpellSourceRows::Complete(vec![]),
        linked_spells: SpellSourceRows::Complete(vec![]),
        modifier_spells: SpellSourceRows::Complete(vec![]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recipe_uses_raw_amount_and_npc_duration_not_stock_daze_defaults() {
        let mut source = synthetic_source(61004, -37, 1703);
        source.spell.duration.as_mut().unwrap().base_ms = -1703;
        source.spell.duration.as_mut().unwrap().maximum_ms = 9100;
        source.spell.duration.as_mut().unwrap().per_level_ms = 999;
        let catalog = PlayerSlowCatalog::validate_sources(vec![source.clone()]).unwrap();
        let recipe = catalog.recipe(61004, 10).unwrap();
        assert_eq!(
            (recipe.amount, recipe.duration_ms, recipe.max_duration_ms),
            (-37, 1703, 1703)
        );
        assert_eq!(recipe.client_spell_words, source.spell.words);
        assert!(catalog.recipe(1604, 10).is_err());
    }
    #[test]
    fn npc_effect_rank_is_level_before_source_division_by_five() {
        let mut source = synthetic_source(61004, -37, 1703);
        source.spell.words[73] = 0xbf80_0000; // -1.0 point per source level
        let catalog = PlayerSlowCatalog::validate_sources(vec![source]).unwrap();
        assert_eq!(catalog.recipe(61004, 10).unwrap().amount, -39);
        assert_eq!(catalog.recipe(61004, 11).unwrap().amount, -39);
        assert_eq!(catalog.recipe(61004, 15).unwrap().amount, -40);
    }
    #[test]
    fn rejects_unknown_and_nonempty_dependencies() {
        let mut source = synthetic_source(61004, -37, 1703);
        source.aura_scripts = SpellSourceRows::Unknown;
        assert!(matches!(
            PlayerSlowCatalog::validate_sources(vec![source]),
            Err(NativeError::AuthorityUnavailable(_))
        ));
        let mut source = synthetic_source(61004, -37, 1703);
        source.linked_spells = SpellSourceRows::Complete(vec![1]);
        assert!(PlayerSlowCatalog::validate_sources(vec![source]).is_err());
    }
    #[test]
    fn rejects_unrepresented_fields_and_lifecycle_branches() {
        for index in [
            4, 5, 7, 8, 9, 10, 16, 17, 22, 23, 24, 25, 26, 39, 119, 160, 161, 162,
        ] {
            let mut source = synthetic_source(61004, -37, 1703);
            source.spell.words[index] = 1;
            assert!(
                PlayerSlowCatalog::validate_sources(vec![source]).is_err(),
                "word {index}"
            );
        }
        for attributes in [0x40, 0x4000_0000, 0x8000_0000] {
            let mut source = synthetic_source(61004, -37, 1703);
            source.spell.words[6] |= attributes;
            assert!(PlayerSlowCatalog::validate_sources(vec![source]).is_err());
        }
    }
    #[test]
    fn only_source_water_interrupt_bits_expand_the_admitted_domain() {
        for id in [61004, 61005] {
            for mask in [0, 0x80, 0x100, 0x180] {
                let mut source = synthetic_source(id, -37, 1703);
                source.spell.words[22] = mask;
                let catalog = PlayerSlowCatalog::validate_sources(vec![source.clone()]).unwrap();
                assert_eq!(catalog.interrupt_flags(id).unwrap(), mask);
                assert_eq!(catalog.recipe(id, 10).unwrap().client_spell_words[22], mask);
                // Water flags cannot enable scripts, procs, linked spells or extra effects.
                for mutation in 0..6 {
                    let mut changed = source.clone();
                    match mutation {
                        0 => {
                            changed.aura_scripts = SpellSourceRows::Complete(vec!["script".into()])
                        }
                        1 => changed.spell.hooks.spell_proc_event = SpellSourceRows::Unknown,
                        2 => changed.linked_spells = SpellSourceRows::Complete(vec![61005]),
                        3 => changed.modifier_spells = SpellSourceRows::Complete(vec![61005]),
                        4 => changed.spell.words[62] = 6,
                        _ => changed.spell.words[79] = 7, // effect mechanic / diminishing returns
                    }
                    assert!(PlayerSlowCatalog::validate_sources(vec![changed]).is_err());
                }
            }
        }
        for bit in (0..32)
            .map(|n| 1_u32 << n)
            .filter(|bit| bit & WATER_INTERRUPT_FLAGS == 0)
        {
            let mut source = synthetic_source(61004, -37, 1703);
            source.spell.words[22] = bit | WATER_INTERRUPT_FLAGS;
            assert!(
                PlayerSlowCatalog::validate_sources(vec![source]).is_err(),
                "bit={bit:x}"
            );
        }
        let mut old =
            PlayerSlowCatalog::validate_sources(vec![synthetic_source(61004, -37, 1703)]).unwrap();
        old.policy = "cmangos-8ec338a1-player-slow-source-v1";
        assert_eq!(old.validate(), Err(NativeError::InvalidProfile));
    }
    #[test]
    fn rejects_missing_rows_extra_effects_periodicity_and_effect_rng() {
        let base = synthetic_source(61004, -37, 1703);
        let mut source = base.clone();
        source.spell.words.pop();
        assert!(PlayerSlowCatalog::validate_sources(vec![source]).is_err());
        let mut source = base.clone();
        source.spell.duration = None;
        assert!(PlayerSlowCatalog::validate_sources(vec![source]).is_err());
        for (index, value) in [(62, 6), (65, 1), (64, 2), (94, 100), (109, 133), (30, 99)] {
            let mut source = base.clone();
            source.spell.words[index] = value;
            assert!(
                PlayerSlowCatalog::validate_sources(vec![source]).is_err(),
                "word {index}"
            );
        }
        assert!(PlayerSlowCatalog::validate_sources(vec![synthetic_source(61004, 0, 1)]).is_err());
        assert!(PlayerSlowCatalog::validate_sources(vec![synthetic_source(61004, -1, 0)]).is_err());
    }
}
