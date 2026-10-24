// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored player aura effect fixtures, not NPC SpellStart, rear hits, or stock spell1604.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyntheticSlowPhase {
    BeforePlayerUpdate,
    AfterPlayerUpdate,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyntheticSlowEffect {
    pub tick: u64,
    pub spell_id: u32,
    pub caster: crate::NpcId,
    pub phase: SyntheticSlowPhase,
}
pub fn with_effects(effects: Vec<SyntheticSlowEffect>) -> Profile {
    let mut profile = synthetic_motion_fixture::profile();
    profile.player_slow = Some(
        PlayerSlowGenesis::from_sources(
            vec![
                player_slow::source::synthetic_source(61004, -40, 400),
                player_slow::source::synthetic_source(61005, -60, 600),
            ],
            player_slow::host::synthetic_host(&profile),
        )
        .unwrap()
        .with_fixture_effects(effects),
    );
    profile
}
pub fn profile() -> Profile {
    with_effects(vec![SyntheticSlowEffect {
        tick: 1,
        spell_id: 61004,
        caster: crate::NpcId {
            spawn: 2,
            generation: 0,
        },
        phase: SyntheticSlowPhase::BeforePlayerUpdate,
    }])
}
pub fn world() -> NativeWorld {
    NativeWorld::synthetic_component_test(profile(), [8; 16], synthetic_fixture::player(), 1234)
        .unwrap()
}

/// Authored internal SpellStart triggers; distinct from the effect-only seam.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyntheticSlowTrigger {
    pub tick: u64,
    pub spell_id: u32,
    pub caster: crate::NpcId,
    pub after_npc_update: bool,
}
pub fn with_triggers(triggers: Vec<SyntheticSlowTrigger>) -> Profile {
    let mut profile = synthetic_combat_fixture::profile();
    profile.npcs[0].ai.mainhand_interval_ms = 5000;
    profile
        .source_motion
        .as_mut()
        .unwrap()
        .creature_initial_aggro_delay_ms = 0;
    profile.player_slow = Some(
        PlayerSlowGenesis::from_sources(
            vec![
                player_slow::source::synthetic_source(61004, -40, 400),
                player_slow::source::synthetic_source(61005, -60, 600),
            ],
            player_slow::host::synthetic_host(&profile),
        )
        .unwrap(),
    );
    profile.player_slow.as_mut().unwrap().fixture_triggers = triggers
        .into_iter()
        .map(|t| npc_spell::FixtureTrigger {
            tick: t.tick,
            spell_id: t.spell_id,
            caster: t.caster,
            after_npc_update: t.after_npc_update,
        })
        .collect();
    profile
}
#[derive(Default)]
pub struct SyntheticNpcSpellAuthority {
    pub motion: synthetic_motion_fixture::SyntheticMotionAuthority,
    pub checks: Vec<NpcSpellCheck>,
    pub fail: Option<NpcSpellCheck>,
    pub unknown: Option<NpcSpellCheck>,
    pub closure: Option<NpcSpellClosure>,
}
pub fn closed_spell_host() -> NpcSpellClosure {
    NpcSpellClosure {
        spell_magnets: InteractionEvidence::Absent,
        targeting_overrides: InteractionEvidence::Absent,
        cast_end_procs: InteractionEvidence::Absent,
        hit_procs: InteractionEvidence::Absent,
        npc_spell_hit_target_hooks: InteractionEvidence::Absent,
        player_spell_hit_hooks: InteractionEvidence::Absent,
        playerbot_hooks: InteractionEvidence::Absent,
        hostile_action_interrupted_auras: InteractionEvidence::Absent,
        target_not_standing: InteractionEvidence::Absent,
        nested_casts: InteractionEvidence::Absent,
        outer_proc_or_delayed_holders: InteractionEvidence::Absent,
        player_pet_callbacks: InteractionEvidence::Absent,
        map_encounter_credit: InteractionEvidence::Absent,
        cast_start_and_check_hooks: InteractionEvidence::Absent,
        current_noncombat_spells: InteractionEvidence::Absent,
    }
}
impl Authority for SyntheticNpcSpellAuthority {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        let Query::NpcSpell(query) = &request.query else {
            return self.motion.resolve(request);
        };
        let facts = match query {
            NpcSpellQuery::Closure(_) => {
                NpcSpellFacts::Closure(self.closure.clone().unwrap_or_else(closed_spell_host))
            }
            NpcSpellQuery::Check { check, .. } => {
                self.checks.push(*check);
                NpcSpellFacts::Check(if self.unknown == Some(*check) {
                    ai::movement::Check::Unknown
                } else if self.fail == Some(*check) || *check == NpcSpellCheck::CanAssist {
                    ai::movement::Check::Failed
                } else {
                    ai::movement::Check::Passed
                })
            }
        };
        Ok(AuthorityReply {
            request: request.clone(),
            facts: Facts::NpcSpell(facts),
        })
    }
}
