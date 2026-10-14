// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Object.cpp/MapManager.h and glibc 2.41 e_atan2f.c;
// upstream GPL-2.0-or-later / MIT. See benilla-sim/FACING.md.
//! Explicit immutable selection of the pinned current-pose numeric mechanism.
use super::*;
use benilla_sim::facing;

/// Immutable algorithm identity and normalized source hashes. No deserializer or
/// public constructor exists; selection never grants native content admission.
/// ```compile_fail
/// use benilla_world_runtime::native::FacingNumericPolicy;
/// let policy: FacingNumericPolicy = serde_json::from_str("{}").unwrap();
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FacingNumericPolicy {
    algorithm: &'static str,
    source_sha256: [&'static str; 3],
}
impl FacingNumericPolicy {
    pub(super) fn validate(&self, profile: &Profile) -> Result<(), NativeError> {
        if profile.source_motion.is_none() || profile.interactions.is_none() {
            return Err(NativeError::UnsupportedMechanic(
                "facing authority requires source motion and interaction initialization",
            ));
        }
        if self.algorithm != facing::POLICY || self.source_sha256 != facing::SOURCE_SHA256 {
            return Err(NativeError::InvalidProfile);
        }
        Ok(())
    }
}
impl Profile {
    /// Derive source Bearing, mutual MeleeFront and caster SpellFacing from the
    /// runtime's executing request. All other spatial authority stays external.
    pub fn initialize_facing_authority(&mut self) -> Result<(), NativeError> {
        let mut next = self.clone();
        next.interactions
            .as_mut()
            .ok_or(NativeError::UnsupportedMechanic(
                "facing authority requires source motion and interaction initialization",
            ))?
            .facing_authority = Some(FacingNumericPolicy {
            algorithm: facing::POLICY,
            source_sha256: facing::SOURCE_SHA256,
        });
        next.validate()?;
        *self = next;
        Ok(())
    }
    pub(in crate::native) fn source_facing_authority(&self) -> bool {
        self.interactions
            .as_ref()
            .is_some_and(|catalog| catalog.facing_authority.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn algorithm_and_each_source_hash_are_digest_bound_and_validated() {
        let mut profile = crate::native::synthetic_combat_fixture::profile();
        profile.initialize_facing_authority().unwrap();
        let expected = profile.digest();
        for field in 0..4 {
            let mut changed = profile.clone();
            let policy = changed
                .interactions
                .as_mut()
                .unwrap()
                .facing_authority
                .as_mut()
                .unwrap();
            if field == 0 {
                policy.algorithm = "unrecognized-algorithm";
            } else {
                policy.source_sha256[field - 1] = "unrecognized-source";
            }
            assert_ne!(changed.digest(), expected);
            assert_eq!(changed.validate(), Err(NativeError::InvalidProfile));
        }
    }
}
