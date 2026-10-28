// SPDX-License-Identifier: GPL-3.0-or-later
//! Owner-captured reachable historical operands, not an authenticated trajectory.
//! No stored permission bit replaces retained source relationship predicates.
use super::state::{Lifecycle, MeleeFields};
use crate::native::interactions::player_melee::{defense_controls, MeleeRelationContext};
use crate::native::quest_progression::MeleeExecutionBinding;
use crate::native::{ai, equipment, NativeError, Npc, Orientation, Profile, State};
use crate::NpcId;
use benilla_sim::creature_motion::GroundMovementFlags;
use benilla_sim::player_melee as melee;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceContext {
    binding: MeleeExecutionBinding,
    phase: super::state::Phase,
    #[serde(deserialize_with = "crate::native::creature_collision_width::required_option")]
    generic_slot: Option<super::casting::GenericSlot>,
    relation: MeleeRelationContext,
    equipment: equipment::EquipmentSnapshot,
    #[serde(deserialize_with = "crate::native::creature_collision_width::required_option")]
    credited_generation: Option<u64>,
    player_position: ai::Point,
    player_orientation: Orientation,
    player_movement: GroundMovementFlags,
    target_position: ai::Point,
    target_orientation: u32,
    target_moving: bool,
    target_running: bool,
    target_phase: ai::Phase,
    #[serde(deserialize_with = "crate::native::creature_collision_width::required_option")]
    predecessor_victim: Option<NpcId>,
    predecessor_remaining: [u32; 2],
    predecessor_error: u8,
}

pub(super) struct ReplayedContext {
    pub predecessor: MeleeFields,
    pub player_casting: bool,
    pub input: melee::MeleeInput,
    pub player_health: u32,
    pub target_health: u32,
    pub target_evading: bool,
    pub in_range: bool,
    pub facing: bool,
    pub can_attack_now: bool,
    pub can_attack_in_combat: bool,
    pub victim_can_attack: bool,
}

impl SourceContext {
    pub(super) fn capture(
        profile: &Profile,
        state: &State,
        target: &Npc,
        predecessor: MeleeFields,
        phase: super::state::Phase,
    ) -> Result<Self, NativeError> {
        super::validate_supported_concurrency(profile, state)?;
        let combat = state.combat.as_ref().ok_or(NativeError::InvalidState)?;
        let current = MeleeFields {
            lifecycle: Lifecycle {
                melee_attacking: combat.melee_timers.melee_attacking(),
                victim: combat.melee_timers.victim(),
            },
            remaining_ms: combat.melee_timers.remaining_ms(),
            last_error: state
                .player_melee
                .as_ref()
                .ok_or(NativeError::InvalidState)?
                .last_error,
        };
        if current != predecessor {
            return Err(NativeError::InvalidState);
        }
        let motion = &target
            .source_motion
            .as_ref()
            .ok_or(NativeError::InvalidState)?
            .motion;
        let pose = motion.pose();
        if pose.position != target.ai.position() {
            return Err(NativeError::InvalidState);
        }
        let context = Self {
            phase,
            binding: MeleeExecutionBinding::capture(profile, state)?,
            generic_slot: super::casting::GenericSlot::capture(state)?,
            relation: MeleeRelationContext::capture(profile, state, target)?,
            equipment: combat.equipment.snapshot(),
            credited_generation: state
                .quest_progression
                .as_ref()
                .ok_or(NativeError::InvalidState)?
                .credited_generations
                .get(&target.id.spawn)
                .copied(),
            player_position: state.position,
            player_orientation: state.orientation,
            player_movement: state
                .player_movement_flags
                .ok_or(NativeError::InvalidState)?,
            target_position: pose.position,
            target_orientation: pose.orientation.bits(),
            target_moving: motion.is_moving(),
            target_running: motion.moving_and_running(),
            target_phase: target.ai.phase(),
            predecessor_victim: predecessor.lifecycle.victim,
            predecessor_remaining: predecessor.remaining_ms,
            predecessor_error: predecessor.last_error,
        };
        context.replay(profile, state)?;
        Ok(context)
    }

    pub(super) fn credited_generation(&self) -> Option<u64> {
        self.credited_generation
    }

    pub(super) fn before_equipment(&self) -> &equipment::EquipmentSnapshot {
        &self.equipment
    }

    pub(super) fn binding(&self) -> &MeleeExecutionBinding {
        &self.binding
    }

    pub(super) fn phase(&self) -> super::state::Phase {
        self.phase
    }

    pub(super) fn captured_ms(&self) -> u64 {
        self.binding.captured_ms()
    }

    pub(super) fn target(&self) -> NpcId {
        self.relation.target()
    }

    pub(super) fn replay_work(&self) -> u64 {
        self.binding
            .replay_work()
            .saturating_add(64)
            .saturating_add(
                self.generic_slot
                    .as_ref()
                    .map_or(0, super::casting::GenericSlot::replay_work),
            )
    }

    pub(super) fn replay(
        &self,
        profile: &Profile,
        state: &State,
    ) -> Result<ReplayedContext, NativeError> {
        profile
            .player_melee
            .as_ref()
            .ok_or(NativeError::InvalidProfile)?
            .validate(profile)?;
        #[cfg(any(test, feature = "synthetic-native-tests"))]
        let operands = self.binding.validate(
            profile,
            state.quest_progression.as_ref(),
            state.now_ms,
            (profile.digest(), state.domain, state.world, state.player),
        )?;
        // Shipping native restore is closed. Only privately captured owner
        // contexts reach this branch, matching the existing spell binding split.
        #[cfg(not(any(test, feature = "synthetic-native-tests")))]
        let operands = self.binding.current_operands(profile)?;
        if operands.weapon_subclass == Some(20) {
            return Err(NativeError::UnsupportedMechanic(
                "player melee fishing-pole callback",
            ));
        }
        let combat_genesis = profile.combat.as_ref().ok_or(NativeError::InvalidProfile)?;
        #[cfg(any(test, feature = "synthetic-native-tests"))]
        let equipment = {
            let genesis = profile
                .quest_progression
                .as_ref()
                .ok_or(NativeError::InvalidCheckpoint)?;
            let current = state
                .quest_progression
                .as_ref()
                .ok_or(NativeError::InvalidCheckpoint)?;
            // validate above authenticates this owner-bound prefix. The current
            // combat recipe may reflect later armor removal or re-equipping.
            let recipe = self.binding.recipe_state(genesis, current)?;
            crate::native::quest_progression::equipment_movement::restore_equipment(
                combat_genesis,
                self.equipment.clone(),
                &recipe,
            )?
        };
        #[cfg(not(any(test, feature = "synthetic-native-tests")))]
        let equipment = equipment::EquipmentState::restore(
            self.equipment.clone(),
            &equipment::EquipmentState::from_snapshot(combat_genesis.equipment.clone())?,
        )?;
        let broken: Vec<_> = equipment
            .items()
            .filter(|item| item.is_broken())
            .map(|item| item.slot)
            .collect();
        if broken != self.binding.recipe().broken_slots {
            return Err(NativeError::InvalidCheckpoint);
        }
        let target = self.target();
        if self.credited_generation.is_some_and(|generation| {
            generation > target.generation
                || state
                    .quest_progression
                    .as_ref()
                    .and_then(|progression| {
                        progression.credited_generations.get(&target.spawn).copied()
                    })
                    .is_none_or(|current| current < generation)
        }) {
            return Err(NativeError::InvalidCheckpoint);
        }

        let owner = state
            .npcs
            .get(&target.spawn)
            .ok_or(NativeError::UnknownTarget)?;
        let projected = crate::native::interactions::spell_authority::historical_target_profile(
            profile,
            owner,
            target,
            self.captured_ms(),
        )?;
        let initialized = owner
            .historical_initialized_creature(target)
            .ok_or(NativeError::InvalidCheckpoint)?;
        if self.relation.player_health() > operands.maximum_health
            || self.relation.target_health() > initialized.stats.resources.max_health
            || self.target_running && !self.target_moving
            || self.target_phase == ai::Phase::Despawned
            || (self.relation.target_health() == 0) != (self.target_phase == ai::Phase::Corpse)
            || self.predecessor_remaining[0] > 1_000_000
            || self.predecessor_remaining[1] > 100
            || self.predecessor_error > 3
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        if let Some(predecessor) = self.predecessor_victim {
            let previous_owner = state
                .npcs
                .get(&predecessor.spawn)
                .ok_or(NativeError::InvalidCheckpoint)?;
            crate::native::interactions::spell_authority::historical_target_profile(
                profile,
                previous_owner,
                predecessor,
                self.captured_ms(),
            )?;
        }
        // Public source geometry routines validate finite bounded coordinates.
        self.player_position
            .distance_squared(self.target_position)?;
        if benilla_sim::facing::normalize_bits(self.player_orientation.bits())?
            != self.player_orientation.bits()
            || benilla_sim::facing::normalize_bits(self.target_orientation)?
                != self.target_orientation
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        let generations = profile
            .creature_generations
            .as_ref()
            .ok_or(NativeError::InvalidProfile)?;
        let raw = &generations
            .source(target.spawn)
            .ok_or(NativeError::InvalidProfile)?
            .input
            .template;
        let (can_react, disarmed) = defense_controls(profile, target.spawn)?;
        let front_facing = arc(
            self.player_position,
            self.player_orientation.bits(),
            self.target_position,
            benilla_sim::facing::PI_BITS,
        )? && arc(
            self.target_position,
            self.target_orientation,
            self.player_position,
            benilla_sim::facing::PI_BITS,
        )?;
        let strength = exact_nonnegative_float(initialized.stats.attributes[0])?;
        let input = melee::MeleeInput {
            attacker: melee::MageAttack {
                level: operands.level,
                weapon_skill: operands.effective_skill,
                minimum_damage: operands.minimum_damage,
                maximum_damage: operands.maximum_damage,
                critical_chance: operands.critical_chance,
                hit_chance: operands.hit_chance,
            },
            defender: melee::CreatureDefense {
                level: initialized.level,
                armor: initialized.stats.armor,
                can_react,
                // Capture excludes NPC casts/control auras. Player casting is
                // a separate swing gate; ordinary damage auras do not turn
                // the defender into a casting non-melee unit.
                casting_non_melee: false,
                can_dodge: true, // Unit ctor; selected policy excludes mutations.
                can_parry: raw.extra_flags & 4 == 0 && !disarmed,
                can_block: raw.extra_flags & 0x10 == 0,
                front_facing,
                shield_block_value: melee::npc_shield_block(initialized.level, strength)?,
            },
        };
        let in_range = melee::player_in_melee_range(melee::PlayerMeleeGeometry {
            player_position: self.player_position,
            target_position: self.target_position,
            player_reach: profile.player.combat_reach,
            target_reach: projected.ai.combat_reach,
            player_moving: self.player_movement.is_moving(),
            player_walking: self.player_movement.is_walking(),
            target_moving: self.target_moving,
            // Only moving+running affects leeway; stationary walk is unused.
            target_walking: self.target_moving && !self.target_running,
        })?;
        Ok(ReplayedContext {
            predecessor: MeleeFields {
                lifecycle: Lifecycle {
                    melee_attacking: self.predecessor_victim.is_some(),
                    victim: self.predecessor_victim,
                },
                remaining_ms: self.predecessor_remaining,
                last_error: self.predecessor_error,
            },
            input,
            player_casting: self
                .generic_slot
                .as_ref()
                .map(|slot| slot.blocks(profile, state, self.captured_ms(), self.phase))
                .transpose()?
                .unwrap_or(false),
            player_health: self.relation.player_health(),
            target_health: self.relation.target_health(),
            target_evading: self.target_phase == ai::Phase::ReturningHome,
            in_range,
            // Unit::UpdateMeleeAttackingState: 2 * M_PI_F / 3, binary32.
            facing: arc(
                self.player_position,
                self.player_orientation.bits(),
                self.target_position,
                0x4006_0a92,
            )?,
            can_attack_now: self.relation.can_attack_now(profile)?,
            can_attack_in_combat: self.relation.can_attack_in_combat(profile)?,
            victim_can_attack: self.relation.victim_can_attack(profile)?,
        })
    }
}

fn arc(
    origin: ai::Point,
    orientation: u32,
    target: ai::Point,
    angle: u32,
) -> Result<bool, NativeError> {
    Ok(benilla_sim::facing::has_in_arc_bits(
        [origin.x.bits(), origin.y.bits()],
        orientation,
        [target.x.bits(), target.y.bits()],
        angle,
        false,
    )?)
}

fn exact_nonnegative_float(
    value: u32,
) -> Result<benilla_sim::classic_spell::SpellFloat, NativeError> {
    use benilla_sim::classic_spell::SpellFloat;
    if value > 1_000_000 {
        return Err(NativeError::UnsupportedMechanic("bounded NPC strength"));
    }
    if value == 0 {
        return Ok(SpellFloat::ZERO);
    }
    let exponent = 31 - value.leading_zeros();
    Ok(SpellFloat::from_bits(
        ((exponent + 127) << 23) | ((value << (23 - exponent)) & 0x007f_ffff),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_strength_conversion_preserves_source_integer_values() {
        for (integer, bits) in [
            (0, 0),
            (1, 0x3f80_0000),
            (20, 0x41a0_0000),
            (1_000_000, 0x4974_2400),
        ] {
            assert_eq!(exact_nonnegative_float(integer).unwrap().bits(), bits);
        }
        assert!(exact_nonnegative_float(1_000_001).is_err());
    }

    #[test]
    fn source_scheduler_arc_and_mutual_front_are_distinct() {
        let origin = ai::Point::new(0.0, 0.0, 0.0).unwrap();
        let east = ai::Point::new(1.0, 0.0, 0.0).unwrap();
        let west = ai::Point::new(-1.0, 0.0, 0.0).unwrap();
        assert!(arc(origin, 0, east, 0x4006_0a92).unwrap());
        assert!(!arc(origin, 0, west, 0x4006_0a92).unwrap());
        assert!(arc(origin, 0, east, benilla_sim::facing::PI_BITS).unwrap());
        assert!(!arc(east, 0, origin, benilla_sim::facing::PI_BITS).unwrap());
        assert!(arc(
            east,
            benilla_sim::facing::PI_BITS,
            origin,
            benilla_sim::facing::PI_BITS,
        )
        .unwrap());
    }
}
