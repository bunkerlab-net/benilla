// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact loaded Map.dbc classification, not map-content or native admission.
use super::*;
use benilla_formats::MapCatalog;
use benilla_sim::map_classification::MapClassificationSelection;
pub use benilla_sim::map_classification::MapKind;
use serde::Serialize;

const POLICY: &str = "cmangos-8ec338a1-native-map-classification-v1";
/// Sealed selected source row. No public constructor accepts a map-type scalar.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NativeMapClassificationSource {
    policy: &'static str,
    selected: MapClassificationSelection,
}
impl NativeMapClassificationSource {
    pub fn map_id(&self) -> u32 {
        self.selected.map_id()
    }
    pub fn kind(&self) -> MapKind {
        self.selected.kind()
    }
    pub fn is_battleground(&self) -> bool {
        self.selected.is_battleground()
    }
    pub fn instanceable(&self) -> bool {
        self.selected.instanceable()
    }
    pub fn binding(&self) -> crate::Digest {
        crate::digest(b"benilla-native-map-classification-source-v1\0", self)
    }
    pub(super) fn validate(&self, profile: &Profile) -> Result<(), NativeError> {
        if self.policy != POLICY || !self.selected.validate() || self.map_id() != profile.map_id {
            return Err(NativeError::AuthorityBinding);
        }
        if let Some(death) = profile.combat.as_ref().and_then(|c| c.death.as_ref()) {
            self.validate_death(death.config)?;
        }
        Ok(())
    }
    pub(super) fn validate_death(
        &self,
        config: benilla_sim::player_death::DeathConfig,
    ) -> Result<(), NativeError> {
        if self.is_battleground() {
            return Err(NativeError::UnsupportedMechanic(
                "source battleground player death",
            ));
        }
        if config.map_instanceable != self.instanceable() {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
}
impl Profile {
    /// Bind the selected loaded row and cross-check any existing death map setting.
    /// Missing/duplicate/unknown source never implies common/non-BG.
    pub fn initialize_map_classification(&mut self, maps: &MapCatalog) -> Result<(), NativeError> {
        let selected = MapClassificationSelection::select(maps, self.map_id).ok_or(
            NativeError::AuthorityUnavailable("exact loaded map classification"),
        )?;
        let source = NativeMapClassificationSource {
            policy: POLICY,
            selected,
        };
        source.validate(self)?;
        if self
            .map_classification
            .as_ref()
            .is_some_and(|old| old != &source)
        {
            return Err(NativeError::InvalidProfile);
        }
        let mut next = self.clone();
        next.map_classification = Some(source);
        next.validate()?;
        *self = next;
        Ok(())
    }
}
