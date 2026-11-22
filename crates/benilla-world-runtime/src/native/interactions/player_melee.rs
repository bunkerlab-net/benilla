// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Relations.cpp and Unit.cpp.
//! Current owner-side predicates. No saved boolean grants attack permission.
use super::*;

fn validate_current_source(
    profile: &Profile,
    state: &State,
    target: &Npc,
) -> Result<(), NativeError> {
    let catalog = profile
        .interactions
        .as_ref()
        .ok_or(NativeError::InvalidState)?;
    catalog
        .combat_authority
        .as_ref()
        .ok_or(NativeError::InvalidState)?
        .validate(profile)?;
    let npc = profile
        .npc(target.id.spawn)
        .ok_or(NativeError::UnknownTarget)?;
    if state.npcs.get(&target.id.spawn).map(|n| n.id) != Some(target.id) {
        return Err(NativeError::AuthorityBinding);
    }
    CurrentView {
        profile,
        state,
        catalog,
        npc: CurrentNpc::from(target),
        source: catalog
            .npcs
            .get(&npc.entry)
            .ok_or(NativeError::InvalidState)?,
    }
    .with_relation_input(|_| Ok(()))
}
fn decision(value: relations::TargetDecision) -> Result<bool, NativeError> {
    match value {
        relations::TargetDecision::Allowed => Ok(true),
        relations::TargetDecision::Denied(_) => Ok(false),
        relations::TargetDecision::Unsupported(reason) => {
            Err(NativeError::InteractionRelation(reason))
        }
    }
}

/// Retained relationship operands for the explicitly untracked-faction slice.
/// These are bounded reachable context, not proof of a historical trajectory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::native) struct MeleeRelationContext {
    target: NpcId,
    player_health: u32,
    target_health: u32,
}
impl MeleeRelationContext {
    pub(in crate::native) fn capture(
        profile: &Profile,
        state: &State,
        target: &Npc,
    ) -> Result<Self, NativeError> {
        validate_current_source(profile, state, target)?;
        let context = Self {
            target: target.id,
            player_health: state.health,
            target_health: target.health,
        };
        context.with_input(profile, |_| Ok(()))?;
        Ok(context)
    }
    pub(in crate::native) fn target(&self) -> NpcId {
        self.target
    }
    pub(in crate::native) fn player_health(&self) -> u32 {
        self.player_health
    }
    pub(in crate::native) fn target_health(&self) -> u32 {
        self.target_health
    }
    fn with_input<T>(
        &self,
        profile: &Profile,
        f: impl FnOnce(&relations::SourcePlayerNpcInput<'_>) -> Result<T, NativeError>,
    ) -> Result<T, NativeError> {
        let catalog = profile
            .interactions
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let npc = profile
            .npc(self.target.spawn)
            .ok_or(NativeError::UnknownTarget)?;
        let source = catalog
            .npcs
            .get(&npc.entry)
            .ok_or(NativeError::InvalidState)?;
        let player = catalog
            .factions
            .get(&catalog.player.faction_template)
            .ok_or(NativeError::InvalidState)?;
        let target = catalog
            .factions
            .get(&npc.faction_template)
            .ok_or(NativeError::InvalidState)?;
        // Both branches are proven from retained Faction.dbc metadata. A missing
        // tracked standing never becomes a neutral or hostile default.
        if player.reputation_list_id.is_some_and(|id| id >= 0)
            || target.reputation_list_id.is_some_and(|id| id >= 0)
        {
            return Err(NativeError::UnsupportedMechanic(
                "player melee untracked faction context",
            ));
        }
        // IN_COMBAT and LOOTING are supported but unobserved by these exact
        // three predicates. Retain no fictitious historical combat/loot facts.
        let player_template = player.template();
        let target_template = target.template();
        let absent = relations::ContextState::Absent;
        f(&relations::SourcePlayerNpcInput {
            units: relations::PlayerNpcInput {
                player_faction: player.reference(&player_template),
                npc_faction: target.reference(&target_template),
                player: relations::UnitTargetState {
                    unit_flags: catalog.player.unit_flags,
                    alive: self.player_health != 0,
                    mounted: catalog.player.control.mount_display_id != 0,
                },
                npc: relations::UnitTargetState {
                    unit_flags: source.unit_flags,
                    alive: self.target_health != 0,
                    mounted: source.control.mount_display_id != 0,
                },
                context: relations::RelationContext {
                    control: absent,
                    pvp: absent,
                    reputation_overrides: absent,
                    immunity: absent,
                    special_state: absent,
                },
            },
            reputation: None,
        })
    }
    pub(in crate::native) fn can_attack_now(&self, profile: &Profile) -> Result<bool, NativeError> {
        self.with_input(profile, |input| {
            decision(relations::check_can_attack_now(
                input,
                relations::Side::Player,
            ))
        })
    }
    pub(in crate::native) fn can_attack_in_combat(
        &self,
        profile: &Profile,
    ) -> Result<bool, NativeError> {
        self.with_input(profile, |input| {
            decision(relations::check_can_attack_in_combat(
                input,
                relations::Side::Player,
            ))
        })
    }
    pub(in crate::native) fn victim_can_attack(
        &self,
        profile: &Profile,
    ) -> Result<bool, NativeError> {
        self.with_input(profile, |input| {
            decision(relations::check_can_attack(input, relations::Side::Npc))
        })
    }
}

pub(in crate::native) fn validate_policy(profile: &Profile) -> Result<(), NativeError> {
    let catalog = profile
        .interactions
        .as_ref()
        .ok_or(NativeError::InvalidProfile)?;
    catalog.validate(profile)?;
    for template in std::iter::once(catalog.player.faction_template)
        .chain(profile.npcs.iter().map(|npc| npc.faction_template))
    {
        let faction = catalog
            .factions
            .get(&template)
            .ok_or(NativeError::InvalidProfile)?;
        if faction.reputation_list_id.is_some_and(|id| id >= 0) {
            return Err(NativeError::UnsupportedMechanic(
                "player melee tracked faction history",
            ));
        }
    }
    Ok(())
}

/// Raw control is validated by the retained interaction closure. DISARMED is
/// explicitly permitted by that closure and must still suppress parry.
pub(in crate::native) fn defense_controls(
    profile: &Profile,
    spawn: u64,
) -> Result<(bool, bool), NativeError> {
    let catalog = profile
        .interactions
        .as_ref()
        .ok_or(NativeError::InvalidProfile)?;
    let npc = profile.npc(spawn).ok_or(NativeError::UnknownTarget)?;
    let source = catalog
        .npcs
        .get(&npc.entry)
        .ok_or(NativeError::InvalidProfile)?;
    source.control.validate()?;
    Ok((
        source.control.unit_state & CANNOT_REACT_OR_LOST_CONTROL == 0,
        source.unit_flags & unit_flags::DISARMED != 0,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn historical_static_flags_preserve_all_dynamic_combinations() {
        let profile = crate::native::synthetic_combat_fixture::profile();
        let context = MeleeRelationContext {
            target: NpcId {
                spawn: profile.npcs[0].ai.creature_id,
                generation: 0,
            },
            player_health: 1,
            target_health: 1,
        };
        context
            .with_input(&profile, |input| {
                let evaluate = |input: &relations::SourcePlayerNpcInput<'_>| {
                    [
                        relations::check_can_attack_now(input, relations::Side::Player),
                        relations::check_can_attack_in_combat(input, relations::Side::Player),
                        relations::check_can_attack(input, relations::Side::Npc),
                    ]
                };
                for player in [
                    0,
                    unit_flags::IN_COMBAT,
                    unit_flags::LOOTING,
                    unit_flags::IN_COMBAT | unit_flags::LOOTING,
                ] {
                    for npc in [0, unit_flags::IN_COMBAT] {
                        let mut dynamic = *input;
                        dynamic.units.player.unit_flags |= player;
                        dynamic.units.npc.unit_flags |= npc;
                        assert_eq!(evaluate(&dynamic), evaluate(input));
                    }
                }
                Ok(())
            })
            .unwrap();
    }
}
