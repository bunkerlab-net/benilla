// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-shaped, explicitly profiled doodad records with the generated-ID slot absent.
use crate::{
    doodad_transform_math as math, PlacementDisposition, PlacementInput, PreparedDiscovery,
};
use sha2::{Digest, Sha256};
use std::fmt;

/// Explicit preprocessing compatibility profile, never native-platform bit parity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DoodadProjectionProfile(());
impl DoodadProjectionProfile {
    /// Unknown/native-libm profiles fail closed rather than silently changing bytes.
    pub fn from_policy_id(id: &str) -> Result<Self, DoodadProjectionError> {
        if id != math::POLICY {
            return Err(DoodadProjectionError::PolicyMismatch);
        }
        if !cfg!(all(
            target_endian = "little",
            any(
                all(
                    target_os = "linux",
                    any(target_arch = "x86_64", target_arch = "aarch64")
                ),
                all(target_os = "windows", target_arch = "x86_64"),
                all(target_os = "macos", target_arch = "aarch64")
            )
        )) || !ieee_environment_matches()
        {
            return Err(DoodadProjectionError::UnsupportedNumericProfile);
        }
        Ok(Self(()))
    }
    pub fn policy_id(self) -> &'static str {
        math::POLICY
    }
    /// Required when combining preprocessing/peer policy declarations.
    pub fn require_match(self, declared: &str) -> Result<(), DoodadProjectionError> {
        Self::from_policy_id(declared).map(|_| ())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoodadProjectionError {
    PolicyMismatch,
    UnsupportedNumericProfile,
    CandidateIndex,
    RetainedIdentity,
    NumericDomain,
    AllocationFailed,
}
impl fmt::Display for DoodadProjectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::PolicyMismatch => "vmap.doodad_projection.policy_mismatch",
            Self::UnsupportedNumericProfile => "vmap.doodad_projection.unsupported_numeric_profile",
            Self::CandidateIndex => "vmap.doodad_projection.candidate_index",
            Self::RetainedIdentity => "vmap.doodad_projection.retained_identity",
            Self::NumericDomain => "vmap.doodad_projection.numeric_domain",
            Self::AllocationFailed => "vmap.doodad_projection.allocation",
        })
    }
}
impl std::error::Error for DoodadProjectionError {}

/// Validated per-candidate projection. No generated identity or map order exists.
/// Prefix is exactly 18 bytes before the source's uint32 generated ID; suffix
/// starts immediately after that absent word. Neither is a complete dir_bin record.
#[derive(Clone, PartialEq)]
pub struct DoodadProjection {
    profile: DoodadProjectionProfile,
    binding: [u8; 32],
    position: [f32; 3],
    rotation: [f32; 3],
    scale: f32,
    prefix: [u8; 18],
    suffix: Vec<u8>,
}
impl fmt::Debug for DoodadProjection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DoodadProjection")
            .field("policy", &self.profile.policy_id())
            .field("position", &self.position)
            .field("rotation", &self.rotation)
            .field("scale", &self.scale)
            .field("record_bytes_without_id", &(18 + self.suffix.len()))
            .finish_non_exhaustive()
    }
}
impl DoodadProjection {
    pub fn profile(&self) -> DoodadProjectionProfile {
        self.profile
    }
    /// Source epoch + pin + numeric, writer and reference-order policies.
    /// Peers must also compare the declared policies;
    /// a matching hash cannot establish content admission or a filesystem lease.
    pub fn preprocessing_binding(&self) -> &[u8; 32] {
        &self.binding
    }
    pub fn position(&self) -> [f32; 3] {
        self.position
    }
    pub fn rotation(&self) -> [f32; 3] {
        self.rotation
    }
    pub fn scale(&self) -> f32 {
        self.scale
    }
    pub fn prefix_before_generated_id(&self) -> &[u8; 18] {
        &self.prefix
    }
    /// Explicit raw-product interface: this includes the source model basename.
    pub fn suffix_after_generated_id(&self) -> &[u8] {
        &self.suffix
    }
    pub fn require_policy(&self, declared: &str) -> Result<(), DoodadProjectionError> {
        self.profile.require_match(declared)
    }
}

impl PreparedDiscovery {
    /// Project one internally captured eligible candidate with an explicit
    /// numeric compatibility profile. Candidate order follows the declared
    /// reference profile, but an index is not a global generated object ID.
    pub fn project_doodad(
        &self,
        candidate_index: usize,
        profile: DoodadProjectionProfile,
    ) -> Result<DoodadProjection, DoodadProjectionError> {
        profile.require_match(profile.policy_id())?;
        let candidate = self
            .doodad_candidates
            .get(candidate_index)
            .ok_or(DoodadProjectionError::CandidateIndex)?;
        let parent = self
            .placements
            .get(candidate.wmo_placement_index)
            .ok_or(DoodadProjectionError::RetainedIdentity)?;
        let PlacementInput::Wmo(input) = &parent.input else {
            return Err(DoodadProjectionError::RetainedIdentity);
        };
        let product = self
            .products
            .get(candidate.product_index)
            .ok_or(DoodadProjectionError::RetainedIdentity)?;
        let position = parent_position(input.position, parent.disposition);
        let (position, rotation) = math::project(
            position,
            input.rotation,
            candidate.spawn.position,
            candidate.spawn.rotation,
        )
        .map_err(|_| DoodadProjectionError::NumericDomain)?;
        if !candidate.spawn.scale.is_finite() {
            return Err(DoodadProjectionError::NumericDomain);
        }
        let name = product.extracted_name.as_bytes();
        let recorded = candidate.serialized_name_len as usize;
        if recorded != name.len() && recorded != name.len() + 1 {
            return Err(DoodadProjectionError::RetainedIdentity);
        }
        let mut prefix = [0; 18];
        for (at, word) in [
            (0, self.map_id()),
            (4, parent.tile.x),
            (8, parent.tile.y),
            (
                12,
                1u32 | if parent.tile.x == 65 && parent.tile.y == 65 {
                    2
                } else {
                    0
                },
            ),
        ] {
            prefix[at..at + 4].copy_from_slice(&word.to_le_bytes());
        }
        // uint16 nameSet remains zero; MODD scale is copied without composition.
        let mut suffix = Vec::new();
        suffix
            .try_reserve_exact(32 + recorded)
            .map_err(|_| DoodadProjectionError::AllocationFailed)?;
        for value in position
            .into_iter()
            .chain(rotation)
            .chain([candidate.spawn.scale])
        {
            suffix.extend_from_slice(&value.to_le_bytes());
        }
        suffix.extend_from_slice(&candidate.serialized_name_len.to_le_bytes());
        suffix.extend_from_slice(name);
        if recorded > name.len() {
            suffix.push(0);
        }
        let mut digest = Sha256::new();
        digest.update(b"benilla-wmo-doodad-projection-binding-v2\0");
        digest.update(crate::SOURCE_PIN.as_bytes());
        digest.update([0]);
        digest.update(profile.policy_id().as_bytes());
        digest.update([0]);
        digest.update(self.summary.doodad_reference_order_policy.as_bytes());
        digest.update([0]);
        digest.update(self.summary.writer_policy.as_bytes());
        digest.update([0]);
        digest.update(self.summary.source_epoch.as_bytes());
        Ok(DoodadProjection {
            profile,
            binding: digest.finalize().into(),
            position,
            rotation,
            scale: candidate.spawn.scale,
            prefix,
            suffix,
        })
    }
}

fn parent_position(mut position: [f32; 3], disposition: PlacementDisposition) -> [f32; 3] {
    if disposition != PlacementDisposition::Emits {
        return position;
    }
    if position[0] == 0.0 && position[2] == 0.0 {
        position[0] = 533.333_3_f32 * 32.0;
        position[2] = 533.333_3_f32 * 32.0;
    }
    [position[2], position[0], position[1]]
}

#[cfg(test)]
#[path = "doodad_projection_oracle_tests.rs"]
mod tests;

// Dynamic probes avoid constant-folding away the active rounding/underflow mode.
// The declared profile requires RNE and gradual underflow for both widths.
fn ieee_environment_matches() -> bool {
    use std::hint::black_box as b;
    let half32 = b(f32::from_bits(0x3380_0000));
    let half64 = b(f64::from_bits(0x3ca0_0000_0000_0000));
    (b(1.0f32) + half32).to_bits() == 0x3f80_0000
        && (b(f32::from_bits(0x3f80_0001)) + half32).to_bits() == 0x3f80_0002
        && (b(1.0f64) + half64).to_bits() == 0x3ff0_0000_0000_0000
        && (b(f64::from_bits(0x3ff0_0000_0000_0001)) + half64).to_bits() == 0x3ff0_0000_0000_0002
        && (b(f32::MIN_POSITIVE) * b(0.5f32)).to_bits() == 0x0040_0000
        && (b(f64::MIN_POSITIVE) * b(0.5f64)).to_bits() == 0x0008_0000_0000_0000
        && (b(f32::from_bits(1)) * b(2.0f32)).to_bits() == 2
        && (b(f64::from_bits(1)) * b(2.0f64)).to_bits() == 2
}
