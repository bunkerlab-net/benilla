// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, Relations.cpp, Creature.cpp and Unit.cpp;
// upstream GPL-2.0-or-later, Copyright CMaNGOS contributors.
//! Current-state NPC/melee semantics. Spatial answers remain external witnesses.
//!
//! Pinned source phases: Relations.cpp:1144/1172 (sight/combat),
//! Creature.cpp:2271 (initiate), Unit.h:2386 (entry), and Unit.cpp:1203
//! (surviving post-health HandleDamageDealt) versus 2158/2219
//! (DealMeleeDamage entry after DealDamage, including miss/lethal returns).
//! These two melee callbacks are deliberately separate; neither inserts an
//! alive guard into CanEnterCombat. See COMBAT-AUTHORITY.md for raw-field closure
//! and the inherited retained-AI-target versus source GetVictim limitation.
use super::*;
use benilla_sim::creature_motion as motion;

const FORCE_ATTACKING_CAPABILITY: u32 = 0x0008_0000;
const IGNORE_FEIGN_DEATH: u32 = 0x0000_0100;
const NO_CRUSHING_BLOWS: u32 = 0x0000_0010;
const NO_WOUNDED_SLOWDOWN: u32 = 0x0000_0040;
const POLICY: &str = "cmangos-8ec338a1-native-combat-authority-v1";

/// Captured complete raw fields. Unrepresented bits reject rather than becoming
/// an assertion of absent script/control behavior. No deserializer is provided.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NpcCombatAuthoritySource {
    pub entry: u32,
    pub extra_flags: u32,
    pub static_flags1: u32,
    pub static_flags2: u32,
    pub static_flags3: u32,
    pub static_flags4: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CombatAuthorityCatalog {
    policy: &'static str,
    npcs: BTreeMap<u32, NpcCombatAuthoritySource>,
}
impl CombatAuthorityCatalog {
    pub(super) fn validate(&self, profile: &Profile) -> Result<(), NativeError> {
        if profile.source_motion.is_none() || profile.combat.is_none() {
            return Err(NativeError::UnsupportedMechanic(
                "combat authority requires source motion and player initialization",
            ));
        }
        if self.npcs.len() != profile.npcs.len() {
            return Err(NativeError::UnsupportedMechanic(
                "missing combat authority source rows",
            ));
        }
        for npc in &profile.npcs {
            let row = self
                .npcs
                .get(&npc.entry)
                .ok_or(NativeError::UnsupportedMechanic(
                    "missing combat authority source row",
                ))?;
            if row.static_flags1 != 0 || row.static_flags3 != 0 || row.static_flags4 != 0
                || row.extra_flags & !FORCE_ATTACKING_CAPABILITY != 0
                || row.static_flags2 & !(IGNORE_FEIGN_DEATH | NO_CRUSHING_BLOWS | NO_WOUNDED_SLOWDOWN) != 0
                // The only other admitted raw type flag is already owned by
                // Creature::RegenerateAll's INTERACTION_IN_COMBAT predicate.
                || npc.health_regeneration.creature_type_flags & !0x0004_0000 != 0
            {
                return Err(NativeError::UnsupportedMechanic(
                    "unmodeled combat authority creature flags",
                ));
            }
            let catalog = profile
                .interactions
                .as_ref()
                .ok_or(NativeError::InvalidProfile)?;
            let faction = catalog.factions.get(&npc.faction_template).ok_or(
                NativeError::UnsupportedMechanic("missing combat authority faction template"),
            )?;
            // Unit.cpp:6228 and DBCStructure.h:441. Faction zero is neutral even
            // when the template masks would otherwise describe an enemy.
            let neutral_to_all = if faction.faction == 0 {
                true
            } else if faction.reputation_list_id.is_some_and(|id| id >= 0) {
                false
            } else {
                faction.enemies.iter().all(|enemy| *enemy == 0)
                    && faction.enemy_group_mask == 0
                    && faction.friend_group_mask == 0
            };
            let combat = npc.combat.as_ref().ok_or(NativeError::UnsupportedMechanic(
                "combat authority requires creature melee profile",
            ))?;
            let motion = profile
                .source_motion
                .as_ref()
                .ok_or(NativeError::InvalidProfile)?;
            if npc.ai.neutral_to_all != neutral_to_all
                // UnitAI starts aggressive; all admitted raw flags exclude
                // CreatureAI's defensive/no-combat overrides and other AI kinds.
                || npc.ai.reaction != ai::Reaction::Aggressive
                || !motion.player_can_enter_combat || !motion.npc_can_enter_combat
                // Unit.cpp:3123 and Creature.cpp:2983 read these same raw bits.
                || combat.no_crushing_blows != (row.static_flags2 & NO_CRUSHING_BLOWS != 0)
                || motion.npc_policy.settings.no_wounded_slowdown != (row.static_flags2 & NO_WOUNDED_SLOWDOWN != 0)
                // TargetedMovementGenerator.cpp:145; the zero-only StaticFlags4
                // bound cannot represent disabled backpedal in another field.
                || !motion.npc_policy.settings.backpedal_enabled
            {
                return Err(NativeError::InvalidProfile);
            }
        }
        Ok(())
    }
}
impl Profile {
    /// Explicitly select this source host. All required rows must be present;
    /// selection is immutable, profile-bound, and never a native admission token.
    pub fn initialize_combat_authority(
        &mut self,
        rows: Vec<NpcCombatAuthoritySource>,
    ) -> Result<(), NativeError> {
        if rows.len() > 4096 {
            return Err(NativeError::TooLarge);
        }
        let mut npcs = BTreeMap::new();
        for row in rows {
            if npcs.insert(row.entry, row).is_some() {
                return Err(NativeError::InvalidProfile);
            }
        }
        let mut next = self.clone();
        next.interactions
            .as_mut()
            .ok_or(NativeError::UnsupportedMechanic(
                "combat authority requires interaction source catalog",
            ))?
            .combat_authority = Some(CombatAuthorityCatalog {
            policy: POLICY,
            npcs,
        });
        next.validate()?;
        *self = next;
        Ok(())
    }
    /// Unit::IsOfflineTarget reads successful feign state and Creature's ignore flag.
    /// The selected complete catalogs currently reject every feign-changing operation.
    pub(in crate::native) fn source_player_offline_target(
        &self,
        id: NpcId,
    ) -> Result<bool, NativeError> {
        let catalog = self
            .interactions
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        catalog.validate(self)?;
        let combat = catalog
            .combat_authority
            .as_ref()
            .ok_or(NativeError::UnsupportedMechanic(
                "hostile reference online status requires source combat authority",
            ))?;
        let npc = self.npc(id.spawn).ok_or(NativeError::UnknownTarget)?;
        let row = combat
            .npcs
            .get(&npc.entry)
            .ok_or(NativeError::InvalidState)?;
        Ok(player_offline_target(
            catalog.player.control.unit_state,
            row.static_flags2,
        ))
    }
    /// Validate the complete closure once for a whole manager/persistence pass.
    pub(in crate::native) fn source_player_offline_targets(
        &self,
    ) -> Result<BTreeMap<u64, bool>, NativeError> {
        let catalog = self
            .interactions
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        catalog.validate(self)?;
        let combat = catalog
            .combat_authority
            .as_ref()
            .ok_or(NativeError::UnsupportedMechanic(
                "hostile reference online status requires source combat authority",
            ))?;
        self.npcs
            .iter()
            .map(|npc| {
                let row = combat
                    .npcs
                    .get(&npc.entry)
                    .ok_or(NativeError::InvalidState)?;
                Ok((
                    npc.ai.creature_id,
                    player_offline_target(catalog.player.control.unit_state, row.static_flags2),
                ))
            })
            .collect()
    }
    /// Retained complete ExtraFlags for the separate IsIgnoringMMAP predicate.
    /// Full catalog validation also excludes player control and external scripts;
    /// capability/motion lifecycle closure is required by the configuration host.
    pub(in crate::native) fn source_pathfinding_extra_flags(
        &self,
        spawn_id: u64,
    ) -> Result<u32, NativeError> {
        let catalog = self
            .interactions
            .as_ref()
            .ok_or(NativeError::AuthorityUnavailable(
                "pathfinder interaction source catalog",
            ))?;
        catalog.validate(self)?;
        let combat = catalog
            .combat_authority
            .as_ref()
            .ok_or(NativeError::AuthorityUnavailable(
                "pathfinder complete combat authority flags",
            ))?;
        let npc = self.npc(spawn_id).ok_or(NativeError::UnknownTarget)?;
        let row = combat
            .npcs
            .get(&npc.entry)
            .ok_or(NativeError::AuthorityUnavailable(
                "pathfinder complete creature ExtraFlags",
            ))?;
        Ok(row.extra_flags)
    }
    pub(in crate::native) fn source_combat_authority(&self) -> bool {
        self.interactions
            .as_ref()
            .is_some_and(|c| c.combat_authority.is_some())
    }
}

/// Immutable borrow from the only staged NPC owner, including the AI currently
/// executing its callback. It cannot be constructed from a recorded reply.
pub(in crate::native) struct WorkingNpc<'a> {
    pub id: NpcId,
    pub health: u32,
    pub ai: &'a ai::SoloCreatureAi,
    pub motion: &'a NpcMotion,
    pub events: &'a NpcEvents,
}
impl<'a> WorkingNpc<'a> {
    pub(in crate::native) fn from_npc(npc: &'a Npc) -> Result<Self, NativeError> {
        Ok(Self {
            id: npc.id,
            health: npc.health,
            ai: &npc.ai,
            events: &npc.events,
            motion: npc
                .source_motion
                .as_ref()
                .ok_or(NativeError::InvalidState)?,
        })
    }
    pub(in crate::native) fn request(
        &self,
        profile_digest: crate::Digest,
        state: &State,
        moment: InteractionMoment,
        phase: CombatPhase,
        action: Option<u64>,
    ) -> Result<CombatRequest, NativeError> {
        if state.npcs.contains_key(&self.id.spawn) {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(CombatRequest {
            witness: CombatWitness {
                frame: witness(profile_digest, state, moment)?,
                working_npc: crate::digest(
                    b"benilla-native-combat-working-npc-v1\0",
                    &(
                        self.id,
                        self.health,
                        self.ai.checkpoint(),
                        self.motion.snapshot(),
                        self.events.snapshot(),
                    ),
                ),
            },
            action,
            phase,
            player_pose: motion::Pose {
                position: state.position,
                orientation: ai::Real::from_bits(state.orientation.bits())?,
            },
            creature_pose: self.motion.motion.pose(),
            player_health: state.health,
            creature_health: self.health,
            creature_phase: self.ai.phase(),
            creature_ai_target: self.ai.target(),
        })
    }
    fn view<'b>(
        &self,
        profile: &'b Profile,
        state: &'b State,
    ) -> Result<CurrentView<'b>, NativeError> {
        let catalog = profile
            .interactions
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let combat = catalog
            .combat_authority
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        combat.validate(profile)?;
        validate_abilities(state.abilities(profile))?;
        let npc = profile
            .npc(self.id.spawn)
            .ok_or(NativeError::InvalidState)?;
        if state.npcs.len() + 1 != profile.npcs.len() || state.npcs.contains_key(&self.id.spawn) {
            return Err(NativeError::AuthorityBinding);
        }
        let source = catalog
            .npcs
            .get(&npc.entry)
            .ok_or(NativeError::InvalidState)?;
        Ok(CurrentView {
            profile,
            state,
            catalog,
            npc: CurrentNpc {
                id: self.id,
                health: self.health,
                phase: self.ai.phase(),
            },
            source,
        })
    }
    pub(in crate::native) fn npc_semantics(
        &self,
        profile: &Profile,
        state: &State,
    ) -> Result<NpcSemanticFacts, NativeError> {
        let view = self.view(profile, state)?;
        let row = &view.catalog.combat_authority.as_ref().unwrap().npcs[&view.source.entry];
        let (can_attack_in_combat, can_attack_on_sight) = view.with_relation_input(|input| {
            Ok((
                decision(relations::check_can_attack_in_combat(
                    input,
                    relations::Side::Npc,
                ))?,
                decision(relations::check_can_attack_on_sight(
                    input,
                    relations::Side::Npc,
                    relations::OnSightState {
                        // This player has no home generator or feign-death transition;
                        // interaction source audits reject their external introduction.
                        target_evading_home: false,
                        source_ignores_feign_death: row.static_flags2 & IGNORE_FEIGN_DEATH != 0,
                        target_feigning_death_successfully: view.catalog.player.control.unit_state
                            & 0x4
                            != 0,
                    },
                ))?,
            ))
        })?;
        let flags = state
            .player_movement_flags
            .ok_or(NativeError::InvalidState)?;
        Ok(NpcSemanticFacts {
            can_attack_in_combat,
            can_attack_on_sight,
            can_initiate_attack: relations::can_initiate_attack(
                relations::CreatureInitiateAttackInput {
                    unit_state: view.source.control.unit_state,
                    unit_flags: view.npc_unit_flags(),
                    force_attacking_capability: row.extra_flags & FORCE_ATTACKING_CAPABILITY != 0,
                    can_aggro: self.events.can_aggro(),
                },
            ),
            stationary_melee_refresh_eligible: self
                .motion
                .motion
                .stationary_melee_refresh_eligible(flags.is_moving()),
            creature_moving_and_running: self.motion.motion.moving_and_running(),
        })
    }
    pub(in crate::native) fn combat_entry(
        &self,
        profile: &Profile,
    ) -> Result<MeleeEntryFacts, NativeError> {
        let genesis = profile
            .source_motion
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        Ok(MeleeEntryFacts {
            player_can_enter_combat: relations::can_enter_combat(relations::CombatEntryInput {
                can_enter_combat: genesis.player_can_enter_combat,
                evading_home: false,
            }),
            npc_can_enter_combat: relations::can_enter_combat(relations::CombatEntryInput {
                can_enter_combat: genesis.npc_can_enter_combat,
                evading_home: self.ai.phase() == ai::Phase::ReturningHome,
            }),
        })
    }
    pub(in crate::native) fn melee_combat(
        &self,
        profile: &Profile,
        state: &State,
    ) -> Result<MeleeCombatFacts, NativeError> {
        let view = self.view(profile, state)?;
        let genesis = profile
            .source_motion
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        Ok(MeleeCombatFacts {
            // HandleDamageDealt uses victim->CanAttack(dealer), not the NPC's
            // CanAttackInCombat or either direction's CanAttackOnSight.
            player_can_attack_npc: view.can_attack(relations::Side::Player)?
                == ai::movement::Check::Passed,
            player_can_enter_combat: relations::can_enter_combat(relations::CombatEntryInput {
                can_enter_combat: genesis.player_can_enter_combat,
                evading_home: false,
            }),
            npc_can_enter_combat: relations::can_enter_combat(relations::CombatEntryInput {
                can_enter_combat: genesis.npc_can_enter_combat,
                evading_home: self.ai.phase() == ai::Phase::ReturningHome,
            }),
        })
    }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CombatPhase {
    Sight,
    CombatReceipt,
    UnitPrelude,
    AfterMovement,
    MainhandFront,
    SurvivingMeleeDamage,
    AfterMeleeDamage,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatWitness {
    pub frame: InteractionWitness,
    pub working_npc: crate::Digest,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatRequest {
    pub phase: CombatPhase,
    pub witness: CombatWitness,
    pub action: Option<u64>,
    pub player_pose: motion::Pose,
    pub creature_pose: motion::Pose,
    pub player_health: u32,
    pub creature_health: u32,
    pub creature_phase: ai::Phase,
    /// Retained bounded AI target, not source Unit::GetVictim. Lethal cleanup
    /// detaches the active victim on hook return while this AI target may remain.
    pub creature_ai_target: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CombatQuery {
    NpcSemantics(CombatRequest),
    /// IsVisibleForOrDetect: creature detector, player detected object. Not LOS.
    NpcVisibility(CombatRequest),
    /// player->isInAccessablePlaceFor(creature), distinct from LOS or a route.
    NpcAccessibility(CombatRequest),
    /// creature->IsWithinLOSInMap(player, true), distinct from detectability.
    NpcLineOfSight(CombatRequest),
    /// CanDodgeInCombat: victim HasInArc(pi, attacker), then attacker HasInArc(pi,
    /// victim). Source short-circuits; NPC SetInFront does not replace either check.
    MeleeFront(CombatRequest),
    /// HandleDamageDealt after surviving health modification; no lethal callback.
    MeleeDamage(CombatRequest),
    /// DealMeleeDamage after DealDamage returns, including miss and lethal damage.
    MeleeCombatEntry(CombatRequest),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcSemanticFacts {
    pub can_attack_in_combat: bool,
    pub can_attack_on_sight: bool,
    pub can_initiate_attack: bool,
    pub stationary_melee_refresh_eligible: bool,
    pub creature_moving_and_running: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeleeCombatFacts {
    pub player_can_attack_npc: bool,
    pub player_can_enter_combat: bool,
    pub npc_can_enter_combat: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeleeEntryFacts {
    pub player_can_enter_combat: bool,
    pub npc_can_enter_combat: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CombatFacts {
    NpcSemantics(NpcSemanticFacts),
    NpcVisibility(ai::movement::Check),
    NpcAccessibility(ai::movement::Check),
    NpcLineOfSight(ai::movement::Check),
    MeleeFront(ai::movement::Check),
    MeleeDamage(MeleeCombatFacts),
    MeleeCombatEntry(MeleeEntryFacts),
}

// Unit.h IsFeigningDeathSuccessfully: UNIT_STAT_FEIGN_DEATH = 0x4.
fn player_offline_target(player_unit_state: u32, creature_static_flags2: u32) -> bool {
    creature_static_flags2 & IGNORE_FEIGN_DEATH == 0 && player_unit_state & 0x4 != 0
}

#[cfg(test)]
mod hostile_reference_tests {
    use super::*;
    #[test]
    fn offline_target_matches_exact_source_raw_bit_corpus() {
        let mut count = 0;
        for line in include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../benilla-sim/tests/fixtures/hostile_reference_cmangos.test-fixture"
        ))
        .lines()
        {
            if !line.starts_with("P ") {
                continue;
            }
            let (input, output) = line.split_once(" | ").unwrap();
            let input: Vec<_> = input.split_whitespace().collect();
            let expected = output.split_whitespace().next().unwrap() == "1";
            assert_eq!(
                player_offline_target(input[2].parse().unwrap(), input[3].parse().unwrap()),
                expected,
                "{line}"
            );
            count += 1;
        }
        assert_eq!(count, 528);
    }
    #[test]
    fn offline_target_uses_successful_feign_and_actual_ignore_flag_only() {
        assert!(!player_offline_target(0, 0));
        assert!(player_offline_target(0x4, 0));
        assert!(!player_offline_target(0x4, IGNORE_FEIGN_DEATH));
        assert!(!player_offline_target(0x8, 0));
        assert!(player_offline_target(0x4, NO_WOUNDED_SLOWDOWN));
    }
}
