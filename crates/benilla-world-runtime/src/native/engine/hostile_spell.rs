// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Spell::DoAllEffectOnTarget/DoSpellHitOnUnit and Unit::CasterHitTargetWithSpell.
use super::*;
use benilla_sim::creature_motion as m;
impl<A: Authority> Frame<'_, A> {
    fn hostile_spell_request(
        &self,
        cast_id: u64,
        ability: u32,
        target: NpcId,
        success: bool,
        direction: SpellRelationDirection,
    ) -> Result<HostileSpellRequest, NativeError> {
        let g = self
            .profile
            .source_motion
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let npc = self
            .state
            .npcs
            .get(&target.spawn)
            .filter(|n| n.id == target)
            .ok_or(NativeError::UnknownTarget)?;
        Ok(HostileSpellRequest {
            witness: self.interaction_witness()?,
            cast_id,
            ability,
            success,
            direction,
            player_pose: m::Pose {
                position: self.state.position,
                orientation: ai::Real::from_bits(self.state.orientation.bits())?,
            },
            creature_pose: npc
                .source_motion
                .as_ref()
                .ok_or(NativeError::InvalidState)?
                .motion
                .pose(),
            player_health: self.state.health,
            creature_health: npc.health,
            creature_phase: npc.ai.phase(),
            // Unit.h::CanEnterCombat = m_canEnterCombat && !IsEvadingHome.
            // No admitted operation calls SetCanEnterCombat or gives player Home.
            player_can_enter_combat: g.player_can_enter_combat,
            creature_can_enter_combat: self.profile.npc_motion(target.spawn)?.npc_can_enter_combat
                && npc.ai.phase() != ai::Phase::ReturningHome,
        })
    }
    fn source_hit_relation(
        &mut self,
        target: NpcId,
        query: HostileSpellRequest,
    ) -> Result<bool, NativeError> {
        match self.ask(
            Some(target),
            Query::SourceMotion(MotionQuery::HostileSpellRelation(query)),
        )? {
            Facts::SourceMotion(MotionFacts::HostileSpellRelation(check)) => match check {
                ai::movement::Check::Passed => Ok(true),
                ai::movement::Check::Failed => Ok(false),
                ai::movement::Check::Unknown => Err(NativeError::AuthorityUnavailable(
                    "directional source CanAttack",
                )),
            },
            _ => Err(NativeError::AuthorityBinding),
        }
    }
    pub(super) fn source_hostile_spell_hit(
        &mut self,
        cast_id: u64,
        ability: u32,
        target: NpcId,
        success: bool,
        spec: &spell::MageProjectileSpec,
    ) -> Result<(), NativeError> {
        let policy = self
            .profile
            .source_motion
            .as_ref()
            .unwrap()
            .spell_hits
            .get(&ability)
            .ok_or(NativeError::AuthorityUnavailable(
                "raw hostile spell metadata",
            ))?;
        policy.validate(spec)?;
        if !policy.initial_threat(success) {
            return Ok(());
        }
        let query = self.hostile_spell_request(
            cast_id,
            ability,
            target,
            success,
            SpellRelationDirection::PlayerToCreature,
        )?;
        if !self.source_hit_relation(target, query.clone())? {
            return Ok(());
        }
        if !query.player_can_enter_combat || !query.creature_can_enter_combat {
            return Ok(());
        }
        let visible = match self.ask(
            Some(target),
            Query::SourceMotion(MotionQuery::HostileSpellVisibility(query)),
        )? {
            Facts::SourceMotion(MotionFacts::HostileSpellVisibility(check)) => match check {
                ai::movement::Check::Passed => true,
                ai::movement::Check::Failed => false,
                ai::movement::Check::Unknown => {
                    return Err(NativeError::AuthorityUnavailable(
                        "player detected by creature for hostile spell",
                    ));
                }
            },
            _ => return Err(NativeError::AuthorityBinding),
        };
        if !visible {
            return Ok(());
        }
        if self.state.health == 0 {
            return Err(NativeError::UnsupportedMechanic(
                "dead caster hostile-hit combat callbacks",
            ));
        }
        // Metadata excludes HOSTILE_ACTION-interrupted NPC holders and stealth.
        // This happens before aura installation and before the target health changes.
        self.source_combat_receipt(target, ai::CombatEvent::AttackedBy { target: SOLO_UNIT })?;
        if self.state.npcs[&target.spawn].ai.phase() != ai::Phase::Combat {
            return Err(NativeError::UnsupportedMechanic(
                "hostile-hit combat without an AI victim",
            ));
        }
        self.source_combat_receipt(
            target,
            ai::CombatEvent::CombatTimerTriggered { target: SOLO_UNIT },
        )?;
        self.enter_player_combat();
        Ok(())
    }
    pub(super) fn source_damage_threat_allowed(
        &mut self,
        cast_id: u64,
        ability: u32,
        target: NpcId,
    ) -> Result<bool, NativeError> {
        let policy = self
            .profile
            .source_motion
            .as_ref()
            .unwrap()
            .spell_hits
            .get(&ability)
            .ok_or(NativeError::AuthorityUnavailable(
                "raw damage threat metadata",
            ))?;
        if !policy.damage_threat() {
            return Ok(false);
        }
        let query = self.hostile_spell_request(
            cast_id,
            ability,
            target,
            true,
            SpellRelationDirection::CreatureToPlayer,
        )?;
        if !query.player_can_enter_combat || !query.creature_can_enter_combat {
            return Ok(false);
        }
        if !self.source_hit_relation(target, query)? {
            return Ok(false);
        }
        if self.state.npcs[&target.spawn].ai.phase() != ai::Phase::Combat {
            return Err(NativeError::UnsupportedMechanic(
                "damage threat before later AI victim selection",
            ));
        }
        Ok(true)
    }
}
