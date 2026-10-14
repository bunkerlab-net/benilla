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
