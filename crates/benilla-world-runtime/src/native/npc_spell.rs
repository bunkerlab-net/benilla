// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Spell.cpp / Unit.cpp.
// Copyright CMaNGOS Project contributors; upstream GPL-2.0-or-later. No warranty.
//! Bounded OLD_TRIGGERED, instant, damage-class NONE NPC spell lifetimes.
//! Source-shaped requests are not a native content/admission capability.
use super::*;
use serde::{Deserialize, Serialize};

pub const NPC_SPELL_POLICY: &str = "cmangos-8ec338a1-npc-triggered-instant-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NpcSpellCheck {
    TargetNotTaxiFlying,
    TargetNotUntargetable,
    TargetNotSpiritOfRedemption,
    CanAttackSpell,
    LocationAllowed,
    TargetGmVisible,
    TargetNotGm,
    FilterTargetNotUntargetable,
    EffectNotImmune,
    TargetNotEvading,
    SpellNotImmune,
    CanAttack,
    CanAssist,
    CasterDetectedByTarget,
}
/// Complete current-host closure, independent of the slow spell row's own hooks.
/// Present and Unknown both reject. No "triggered means no procs" shortcut exists.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcSpellClosure {
    pub spell_magnets: InteractionEvidence,
    pub targeting_overrides: InteractionEvidence,
    pub cast_end_procs: InteractionEvidence,
    pub hit_procs: InteractionEvidence,
    pub npc_spell_hit_target_hooks: InteractionEvidence,
    pub player_spell_hit_hooks: InteractionEvidence,
    pub playerbot_hooks: InteractionEvidence,
    pub hostile_action_interrupted_auras: InteractionEvidence,
    pub target_not_standing: InteractionEvidence,
    pub nested_casts: InteractionEvidence,
    pub outer_proc_or_delayed_holders: InteractionEvidence,
    pub player_pet_callbacks: InteractionEvidence,
    pub map_encounter_credit: InteractionEvidence,
    pub cast_start_and_check_hooks: InteractionEvidence,
    pub current_noncombat_spells: InteractionEvidence,
}
impl NpcSpellClosure {
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn validate(&self) -> Result<(), NativeError> {
        for (observed, dependency) in [
            (self.spell_magnets, "NPC spell magnet sources"),
            (self.targeting_overrides, "NPC spell targeting overrides"),
            (self.cast_end_procs, "NPC cast-end proc sources"),
            (self.hit_procs, "NPC spell hit proc sources"),
            (self.npc_spell_hit_target_hooks, "NPC SpellHitTarget hooks"),
            (self.player_spell_hit_hooks, "player SpellHit hooks"),
            (self.playerbot_hooks, "NPC spell playerbot hooks"),
            (
                self.hostile_action_interrupted_auras,
                "hostile-action interrupted player auras",
            ),
            (
                self.target_not_standing,
                "NPC spell target stand-state transition",
            ),
            (self.nested_casts, "NPC nested spell casts"),
            (
                self.outer_proc_or_delayed_holders,
                "NPC outer proc/delayed holders",
            ),
            (self.player_pet_callbacks, "NPC spell target pet callbacks"),
            (
                self.map_encounter_credit,
                "NPC spell encounter credit hooks",
            ),
            (
                self.cast_start_and_check_hooks,
                "NPC spell cast/check hooks",
            ),
            (
                self.current_noncombat_spells,
                "NPC spell current noncombat spells",
            ),
        ] {
            if observed != InteractionEvidence::Absent {
                return Err(NativeError::InteractionEvidence {
                    dependency,
                    observed,
                });
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcSpellRequest {
    pub witness: InteractionWitness,
    pub caster: crate::NpcId,
    pub target: crate::PlayerId,
    pub lifetime: NpcSpellLifetime,
    pub queue: benilla_sim::event_processor::EventProcessorSnapshot,
    pub caster_position: ai::Point,
    pub target_position: ai::Point,
    pub caster_health: u32,
    pub target_health: u32,
    pub caster_level: u8,
    pub target_level: u8,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum NpcSpellQuery {
    Closure(NpcSpellRequest),
    Check {
        context: NpcSpellRequest,
        check: NpcSpellCheck,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum NpcSpellFacts {
    Closure(NpcSpellClosure),
    Check(ai::movement::Check),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NpcSpellTerminal {
    BadTargets,
    LocationDenied,
    TargetFiltered,
    Evade,
    Immune,
    Hit { effect_applied: bool },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcSpellLifetime {
    pub cast_id: u64,
    pub event_id: u64,
    pub caster: crate::NpcId,
    pub target: crate::PlayerId,
    pub spell_id: u32,
    pub source_binding: crate::Digest,
    pub started_tick: u64,
    pub started_at_ms: u64,
    pub event_add_ms: u64,
    pub after_npc_update: bool,
    /// None exists only inside SpellStart's synchronous transaction. Checkpoints reject it.
    pub terminal: Option<NpcSpellTerminal>,
    pub applied_holder: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum NpcSpellEvent {
    Started {
        lifetime: NpcSpellLifetime,
    },
    Finished {
        lifetime: NpcSpellLifetime,
    },
    Destroyed {
        lifetime: NpcSpellLifetime,
        aborted: bool,
    },
}

impl PlayerSlowGenesis {
    pub(super) fn triggered_recipe(
        &self,
        spell: u32,
        level: u8,
    ) -> Result<player_slow::source::SlowRecipe, NativeError> {
        // Audited fixture entry only. 1604 remains a source-data ID, not a trigger admission.
        if !matches!(spell, 61004 | 61005) {
            return Err(NativeError::UnsupportedMechanic(
                "NPC triggered slow spell ID",
            ));
        }
        let recipe = self.catalog.recipe(spell, level)?;
        if recipe.client_spell_words[37] != 0
            || recipe.client_spell_words[164] != 0
            || recipe.client_spell_words[115] != 0
        {
            return Err(NativeError::UnsupportedMechanic(
                "NPC immediate invisible damage-class NONE spell",
            ));
        }
        Ok(recipe)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(super) struct FixtureTrigger {
    pub(super) tick: u64,
    pub(super) caster: crate::NpcId,
    pub(super) spell_id: u32,
    pub(super) after_npc_update: bool,
}
impl PlayerSlowGenesis {
    pub(super) fn validate_triggers(&self, profile: &Profile) -> Result<(), NativeError> {
        if self.fixture_triggers.len() > MAX_RECORDS
            || self
                .fixture_triggers
                .windows(2)
                .any(|w| (w[0].tick, w[0].after_npc_update) > (w[1].tick, w[1].after_npc_update))
        {
            return Err(NativeError::InvalidProfile);
        }
        if !self.fixture_triggers.is_empty()
            && profile.combat.as_ref().is_some_and(|g| {
                g.initial_auras
                    .iter()
                    .any(|a| self.catalog.source(a.spell_id).is_some())
            })
        {
            return Err(NativeError::UnsupportedMechanic(
                "NPC slow conflicts with initial passive source",
            ));
        }
        let mut counts = BTreeMap::<u64, usize>::new();
        for trigger in &self.fixture_triggers {
            let count = counts.entry(trigger.tick).or_default();
            *count += 1;
            if trigger.tick == 0
                || trigger.tick > crate::MAX_TICK
                || *count > MAX_CASTS
                || *count
                    + self
                        .fixture_effects
                        .iter()
                        .filter(|e| e.tick == trigger.tick)
                        .count()
                    > MAX_INPUTS_PER_TICK
            {
                return Err(NativeError::InvalidProfile);
            }
            let npc = profile
                .npc(trigger.caster.spawn)
                .ok_or(NativeError::InvalidProfile)?;
            self.triggered_recipe(trigger.spell_id, npc.ai.level)?;
        }
        Ok(())
    }
}
