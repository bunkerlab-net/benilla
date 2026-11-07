// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit host-selected health rate; not an observed external server setting.
use super::*;
use serde::Serialize;
const POLICY: &str = "cmangos-8ec338a1-native-player-health-regeneration-v1";

/// Immutable opt-in control, bound with all source dependencies by Profile::digest.
/// No default rate, deserialization constructor, admission token or independent clock.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlayerHealthRegenerationSource {
    policy: &'static str,
    rate: spell::SpellFloat,
}
impl PlayerHealthRegenerationSource {
    pub fn health_rate(&self) -> spell::SpellFloat {
        self.rate
    }
    pub(super) fn validate(&self, profile: &Profile) -> Result<(), NativeError> {
        if self.policy != POLICY
            || (self.rate.bits() & 0x8000_0000 != 0 && self.rate.bits() & 0x7fff_ffff != 0)
        {
            return Err(NativeError::InvalidProfile);
        }
        let combat = profile
            .combat
            .as_ref()
            .ok_or(NativeError::UnsupportedMechanic(
                "health regeneration source player",
            ))?;
        if combat.starter_identity.class != 8
            || !profile.has_source_spell_authority()
            || profile.quest_progression.is_none()
        {
            return Err(NativeError::UnsupportedMechanic(
                "health regeneration fresh mage, training and raw spell owners",
            ));
        }
        // The validated interaction/init whitelist excludes aura 88/116/161.
        // Training preserves those aura sets; admitted player effects are damage
        // only and NPC player holders are the separately validated aura-33 owner.
        // No new boolean assertion of absence is accepted here.
        profile
            .interactions
            .as_ref()
            .ok_or(NativeError::UnsupportedMechanic(
                "health regeneration interaction owner",
            ))?
            .validate(profile)?;
        Ok(())
    }
}
impl Profile {
    /// Select an explicit local host rate. This does not capture/authenticate a
    /// CMaNGOS config, alter an existing world's profile, or enable native entry.
    pub fn initialize_player_health_regeneration(
        &mut self,
        rate: spell::SpellFloat,
    ) -> Result<(), NativeError> {
        let source = PlayerHealthRegenerationSource {
            policy: POLICY,
            rate,
        };
        source.validate(self)?;
        if self
            .player_health_regeneration
            .as_ref()
            .is_some_and(|old| old != &source)
        {
            return Err(NativeError::InvalidProfile);
        }
        let mut next = self.clone();
        next.player_health_regeneration = Some(source);
        next.validate()?;
        *self = next;
        Ok(())
    }
}
