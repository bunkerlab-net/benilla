// SPDX-License-Identifier: GPL-3.0-or-later
//! SpellTargetMgr initialization and lookups, CMaNGOS Classic 8ec338a1.
use super::{Refusal, SourceSpellStores, SpellScriptTarget, SpellTemplate};
use std::collections::BTreeMap;

#[path = "spell_target_tables.rs"]
mod tables;
pub use tables::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpellTargetError {
    MissingSpellUndefined {
        spell: u32,
    },
    EffectIndexUndefined {
        spell: u32,
        effect: usize,
    },
    EffectTableIndexUndefined {
        spell: u32,
        effect: usize,
        value: u32,
    },
    TargetTableIndexUndefined {
        spell: u32,
        effect: usize,
        right: usize,
        value: u32,
    },
    ScriptBoundsOwner {
        spell: u32,
        refusal: Refusal,
    },
}
impl std::fmt::Display for SpellTargetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SpellTargetError {}
type TargetResult<T> = Result<T, SpellTargetError>;

/// SpellTargets.h:45-50. Map value-initialization zeroes inactive implicit types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpellTargetingData {
    pub implicit_type: [SpellTargetImplicitType; 3],
    pub target_mask: [[u8; 2]; 3],
    pub ignored_targets: [[bool; 2]; 3],
    pub filtering_scheme: [[SpellTargetFilterScheme; 2]; 3],
}
impl Default for SpellTargetingData {
    fn default() -> Self {
        Self {
            implicit_type: [SpellTargetImplicitType::Unknown; 3],
            target_mask: [[1, 1], [2, 2], [4, 4]],
            ignored_targets: [[false; 2]; 3],
            filtering_scheme: [[SpellTargetFilterScheme::Random; 2]; 3],
        }
    }
}

/// Call after loaded script-target dispositions, World.cpp:1081-1084.
/// Unavailable bounds block only spells whose merging reads those bounds.
#[derive(Debug)]
pub struct SourceSpellTargetMgr {
    entries: BTreeMap<u32, TargetResult<SpellTargetingData>>,
}
impl SourceSpellTargetMgr {
    pub fn initialize(stores: &SourceSpellStores) -> Self {
        // SpellTargets.cpp:254-260 creates an entry even for effectless spells.
        Self {
            entries: stores
                .templates()
                .iter()
                .map(|(&id, spell)| {
                    (
                        id,
                        initialize_spell(spell, || {
                            stores.script_targets(id).map_err(|refusal| {
                                SpellTargetError::ScriptBoundsOwner { spell: id, refusal }
                            })
                        }),
                    )
                })
                .collect(),
        }
    }
    pub fn entries(&self) -> &BTreeMap<u32, TargetResult<SpellTargetingData>> {
        &self.entries
    }
    pub fn get_spell_targeting_data(&self, spell: u32) -> TargetResult<&SpellTargetingData> {
        // SpellTargets.cpp:235-238 dereferences end() for a missing spell.
        self.entries
            .get(&spell)
            .ok_or(SpellTargetError::MissingSpellUndefined { spell })?
            .as_ref()
            .map_err(Clone::clone)
    }
    pub fn get_jump_radius(_spell: u32) -> f32 {
        // SpellTargets.cpp:503-505; Globals/SharedDefines.h:1623. No lookup.
        10.0
    }
    pub fn can_effect_be_filled_with_mask(
        &self,
        spell: u32,
        effect: usize,
        mask: u32,
    ) -> TargetResult<bool> {
        // SpellTargets.cpp:469-484 indexes only after the spell lookup.
        let target_type = *self
            .get_spell_targeting_data(spell)?
            .implicit_type
            .get(effect)
            .ok_or(SpellTargetError::EffectIndexUndefined { spell, effect })?;
        use SpellTargetImplicitType::*;
        let flags = match target_type {
            SpecialDest | LocationDest => TARGET_FLAG_DEST_LOCATION,
            Gameobject => TARGET_FLAG_GAMEOBJECT | TARGET_FLAG_LOCKED,
            UnitDest => {
                TARGET_FLAG_UNIT_ALLY
                    | TARGET_FLAG_UNIT
                    | TARGET_FLAG_UNIT_ENEMY
                    | TARGET_FLAG_UNIT_DEAD
                    | TARGET_FLAG_DEST_LOCATION
            }
            Player | Unit => {
                TARGET_FLAG_UNIT_ALLY
                    | TARGET_FLAG_UNIT
                    | TARGET_FLAG_UNIT_ENEMY
                    | TARGET_FLAG_UNIT_DEAD
            }
            Corpse => TARGET_FLAG_CORPSE_ENEMY | TARGET_FLAG_CORPSE_ALLY,
            Lock => TARGET_FLAG_GAMEOBJECT | TARGET_FLAG_LOCKED | TARGET_FLAG_ITEM,
            Item => TARGET_FLAG_LOCKED | TARGET_FLAG_ITEM,
            _ => 0,
        };
        Ok(mask & flags != 0)
    }
    pub fn can_effect_consume_target(
        target: SpellTargetImplicitType,
        effect: SpellTargetImplicitType,
    ) -> bool {
        // SpellTargets.cpp:487-500 intentionally has only these two exclusions.
        target != SpellTargetImplicitType::Unit
            || !matches!(
                effect,
                SpellTargetImplicitType::LocationDest | SpellTargetImplicitType::None
            )
    }
    pub fn get_spell_targeting_filter_scheme(
        old: SpellTargetFilterScheme,
        spell: u32,
    ) -> SpellTargetFilterScheme {
        // SpellTargets.cpp:508-528 retains even expansion-era override IDs.
        match spell {
            26052 | 26180 | 30284 | 37144 | 37146 | 37148 | 37151 | 37152 | 37153 | 30469
            | 41294 => SpellTargetFilterScheme::Closest,
            42005 => SpellTargetFilterScheme::Furthest,
            _ => old,
        }
    }
}

fn initialize_spell<'a>(
    spell: &SpellTemplate,
    mut bounds: impl FnMut() -> TargetResult<&'a [SpellScriptTarget]>,
) -> TargetResult<SpellTargetingData> {
    use SpellTargetImplicitType::*;
    let effects = spell.effects();
    let targets = effects.map(|e| [e.target_a, e.target_b]);
    let chains = [
        spell.effectchaintarget1,
        spell.effectchaintarget2,
        spell.effectchaintarget3,
    ];
    let radii = [
        spell.effectradiusindex1,
        spell.effectradiusindex2,
        spell.effectradiusindex3,
    ];
    let mut required = [Unknown; 3];
    let mut info = [[SPELL_TARGET_INFO_TABLE[0]; 2]; 3];
    let mut data = SpellTargetingData::default();
    // SpellTargets.cpp:262-313. Zero effects skip all table indexing.
    for i in 0..3 {
        if effects[i].effect == 0 {
            continue;
        }
        required[i] = SPELL_EFFECT_INFO_TABLE
            .get(effects[i].effect as usize)
            .ok_or(SpellTargetError::EffectTableIndexUndefined {
                spell: spell.id,
                effect: i,
                value: effects[i].effect,
            })?
            .required_target;
        for right in 0..2 {
            info[i][right] = *SPELL_TARGET_INFO_TABLE
                .get(targets[i][right] as usize)
                .ok_or(SpellTargetError::TargetTableIndexUndefined {
                    spell: spell.id,
                    effect: i,
                    right,
                    value: targets[i][right],
                })?;
        }
        data.implicit_type[i] = if !matches!(required[i], Dynamic | DynamicAny) {
            required[i]
        } else {
            let [a, b] = info[i].map(|t| t.target_type);
            if a == Lock || b == Lock {
                Lock
            } else if a == Corpse || b == Corpse {
                Corpse
            } else if a == Gameobject || b == Gameobject {
                Gameobject
            } else if (a == LocationDest && b == Unit) || (a == Unit && b == LocationDest) {
                UnitDest
            } else if a == Unit || b == Unit {
                Unit
            } else if a == LocationDest || b == LocationDest || required[i] == DynamicAny {
                LocationDest
            } else if (a == LocationSrc || targets[i][0] == TARGET_NONE)
                && targets[i][1] == TARGET_NONE
            {
                None
            }
            // Source logs failure and retains value-initialized UNKNOWN, :311.
            else {
                Unknown
            }
        };
    }
    // SpellTargets.cpp:315-429. Mutations are visible to subsequent pairs.
    for source in 0..3 {
        if effects[source].effect == 0 {
            continue;
        }
        for source_right in 0..2 {
            if data.ignored_targets[source][source_right] || targets[source][source_right] == 0 {
                continue;
            }
            for target in source..3 {
                if effects[target].effect == 0 {
                    continue;
                }
                let first_right = if source == target {
                    source_right + 1
                } else {
                    0
                };
                for target_right in first_right..2 {
                    if data.ignored_targets[target][target_right]
                        || targets[target][target_right] == 0
                        || targets[source][source_right] != targets[target][target_right]
                    {
                        continue;
                    }
                    let target_info = info[target][target_right];
                    // The two exceptions ignore without extending targetMask, :371-375.
                    let exception = (data.implicit_type[source] == SpecialUnit
                        && target_info.target_type == Unit
                        && target_info.enumerator != SpellTargetEnumerator::Single)
                        || (data.implicit_type[target] == LocationDest
                            && target_info.target_type == Unit);
                    let ignore = if exception {
                        true
                    } else {
                        let merge = match target_info.target_type {
                            LocationSrc | LocationDest | Lock | Corpse => true,
                            Gameobject | Unit | Player => {
                                if target_info.enumerator == SpellTargetEnumerator::Single
                                    && SourceSpellTargetMgr::can_effect_consume_target(
                                        target_info.target_type,
                                        required[source],
                                    )
                                {
                                    true
                                } else if required[source] == None
                                    || (target_info.enumerator == SpellTargetEnumerator::Chain
                                        && chains[source] != chains[target])
                                    || radii[source] != radii[target]
                                {
                                    false
                                } else if target_info.filter == SpellTargetFilter::Script
                                    && source != target
                                {
                                    // SpellTargets.cpp:241-250,403-407; SpellMgr.h:1790.
                                    bounds()?.iter().all(|row| {
                                        (row.inverse_effect_mask & (1 << source) != 0)
                                            == (row.inverse_effect_mask & (1 << target) != 0)
                                    })
                                } else {
                                    true
                                }
                            }
                            _ => false,
                        };
                        if merge {
                            data.target_mask[source][source_right] |= 1 << target;
                        }
                        merge
                    };
                    if ignore {
                        data.ignored_targets[target][target_right] = true;
                    }
                }
            }
        }
    }
    // SpellTargets.cpp:430-465 does not skip ignored targets.
    for i in 0..3 {
        if effects[i].effect == 0 {
            continue;
        }
        for right in 0..2 {
            let target = targets[i][right];
            if target == 0 {
                continue;
            }
            let mut scheme = SpellTargetFilterScheme::Random;
            if info[i][right].enumerator == SpellTargetEnumerator::Chain {
                scheme = match target {
                    TARGET_UNIT_FRIEND_CHAIN_HEAL => SpellTargetFilterScheme::LowestHpChain,
                    TARGET_UNIT_ENEMY_NEAR_CASTER
                    | TARGET_UNIT_FRIEND_NEAR_CASTER
                    | TARGET_UNIT_NEAR_CASTER
                    | TARGET_UNIT_ENEMY => SpellTargetFilterScheme::ClosestChain,
                    TARGET_UNIT => SpellTargetFilterScheme::RandomChain,
                    _ => scheme,
                };
            }
            data.filtering_scheme[i][right] =
                SourceSpellTargetMgr::get_spell_targeting_filter_scheme(scheme, spell.id);
        }
    }
    Ok(data)
}

#[cfg(test)]
#[path = "spell_targets_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "spell_target_oracle_tests.rs"]
mod oracle_tests;
