// SPDX-License-Identifier: GPL-3.0-or-later
// Source effect seam only: no owner/network command can install a player aura.
use super::*;
impl<A: Authority> Frame<'_, A> {
    pub(super) fn player_slow_update(&mut self) -> Result<(), NativeError> {
        if let Some(slow) = self.state.player_slow.as_mut() {
            slow.update(
                self.profile.tick_ms,
                self.state.now_ms,
                self.profile,
                &self.state.npcs,
                &mut self.events,
            )?;
        }
        self.player_auras_updated = true;
        Ok(())
    }
    // The future NPC SpellStart/EffectApplyAura host must call this after source hit/target
    // validation. Only authored fixture scheduling reaches it in the current milestone.
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    fn apply_player_slow(&mut self, caster: NpcId, spell_id: u32) -> Result<(), NativeError> {
        let npc = self
            .state
            .npcs
            .get(&caster.spawn)
            .filter(|npc| npc.id == caster)
            .ok_or(NativeError::UnknownTarget)?;
        if npc.health == 0 || matches!(npc.ai.phase(), ai::Phase::Despawned) {
            return Err(NativeError::UnsupportedMechanic(
                "dead slow-aura fixture caster",
            ));
        }
        let caster_level = self
            .profile
            .npc(caster.spawn)
            .ok_or(NativeError::InvalidState)?
            .ai
            .level;
        // EffectApplyAura refuses an ordinary dead target. This seam adds no cast/hit/RNG.
        if self.state.health == 0 {
            return Ok(());
        }
        self.state
            .player_slow
            .as_mut()
            .ok_or(NativeError::InvalidState)?
            .apply(
                self.profile,
                self.state.world,
                self.state.player,
                caster,
                caster_level,
                self.state.now_ms,
                self.player_auras_updated,
                spell_id,
                &mut self.events,
            )
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn synthetic_player_slow_effects(&mut self, after: bool) -> Result<(), NativeError> {
        let effects = self
            .profile
            .player_slow
            .as_ref()
            .map(|g| {
                g.fixture_effects
                    .iter()
                    .filter(|effect| {
                        effect.tick == self.state.tick && effect.after_player_update == after
                    })
                    .map(|effect| (effect.caster, effect.spell_id))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (caster, spell_id) in effects {
            self.apply_player_slow(caster, spell_id)?;
        }
        Ok(())
    }
}
