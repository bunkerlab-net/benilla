// SPDX-License-Identifier: GPL-3.0-or-later
//! Source dir_bin records for the selected map, without filesystem publication.
//! CMaNGOS pinned ModelInstance, WMOInstance and Doodad::ExtractSet writers.
use crate::{
    DoodadProjectionError, DoodadProjectionProfile, PlacementDisposition, PlacementInput,
    PreparedDiscovery, SourceError,
};
use sha2::{Digest, Sha256};
use std::fmt;

pub const MAX_PLACEMENT_RECORD_BYTES: usize = 128 * 1024 * 1024;
pub const PLACEMENT_RECORD_POLICY: &str = "cmangos-selected-dir-records-v1";

#[derive(Debug)]
pub enum PlacementRecordError {
    Cancelled,
    Limit,
    Allocation,
    RetainedIdentity,
    NonFinite,
    Projection(DoodadProjectionError),
    Source(SourceError),
}
impl fmt::Display for PlacementRecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Projection(e) => write!(f, "{e}"),
            Self::Source(e) => write!(f, "{e}"),
            Self::Cancelled => f.write_str("vmap.records_cancelled"),
            Self::Limit => f.write_str("vmap.records_limit"),
            Self::Allocation => f.write_str("vmap.records_allocation"),
            Self::RetainedIdentity => f.write_str("vmap.records_retained_identity"),
            Self::NonFinite => f.write_str("vmap.records_nonfinite"),
        }
    }
}
impl std::error::Error for PlacementRecordError {}

/// Complete selected-map record bytes only. Not map BIHs or a live source lease.
/// The raw-byte accessor contains captured model basenames; do not log it.
pub struct PlacementRecords {
    bytes: Vec<u8>,
    count: usize,
    binding: [u8; 32],
}
impl fmt::Debug for PlacementRecords {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PlacementRecords")
            .field("records", &self.count)
            .field("bytes", &self.bytes.len())
            .finish_non_exhaustive()
    }
}
impl PlacementRecords {
    #[cfg(test)]
    pub(crate) fn fixture(bytes: Vec<u8>, count: usize) -> Self {
        Self {
            bytes,
            count,
            binding: [0; 32],
        }
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn record_count(&self) -> usize {
        self.count
    }
    pub fn preprocessing_binding(&self) -> &[u8; 32] {
        &self.binding
    }
    pub fn policy_id(&self) -> &'static str {
        PLACEMENT_RECORD_POLICY
    }
}
impl PreparedDiscovery {
    pub fn placement_records(
        &self,
        profile: DoodadProjectionProfile,
        cancelled: impl Fn() -> bool,
    ) -> Result<PlacementRecords, PlacementRecordError> {
        profile
            .require_match(profile.policy_id())
            .map_err(PlacementRecordError::Projection)?;
        let mut bytes = Vec::new();
        let mut count = 0;
        let mut child = 0;
        for (index, parent) in self.placements.iter().enumerate() {
            check_cancel(&cancelled)?;
            if parent.disposition == PlacementDisposition::Emits {
                let id = parent
                    .generated_id
                    .ok_or(PlacementRecordError::RetainedIdentity)?;
                let name = self
                    .products
                    .get(parent.product)
                    .ok_or(PlacementRecordError::RetainedIdentity)?
                    .extracted_name
                    .as_bytes();
                let (position, rotation, scale, bounds, name_set, flags) = match &parent.input {
                    PlacementInput::M2(v) => (
                        [v.position[2], v.position[0], v.position[1]],
                        v.rotation,
                        f32::from(v.scale) / 1024.0_f32,
                        None,
                        0_u16,
                        1_u32,
                    ),
                    PlacementInput::Wmo(v) => (
                        crate::doodad_projection::parent_position(v.position, parent.disposition),
                        v.rotation,
                        1.0,
                        Some(v.bounds.map(|p| [p[2], p[0], p[1]])),
                        v.name_set,
                        4,
                    ),
                };
                let finite = position
                    .into_iter()
                    .chain(rotation)
                    .chain([scale])
                    .chain(bounds.into_iter().flatten().flatten())
                    .all(f32::is_finite);
                if !finite {
                    return Err(PlacementRecordError::NonFinite);
                }
                reserve_record(
                    &mut bytes,
                    54 + if bounds.is_some() { 24 } else { 0 } + name.len(),
                )?;
                for value in [
                    self.map_id(),
                    parent.tile.x,
                    parent.tile.y,
                    flags
                        | if parent.tile.x == 65 && parent.tile.y == 65 {
                            2
                        } else {
                            0
                        },
                ] {
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
                bytes.extend_from_slice(&name_set.to_le_bytes());
                bytes.extend_from_slice(&id.to_le_bytes());
                for value in position.into_iter().chain(rotation).chain([scale]) {
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
                for value in bounds.into_iter().flatten().flatten() {
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
                let name_len =
                    u32::try_from(name.len()).map_err(|_| PlacementRecordError::Limit)?;
                bytes.extend_from_slice(&name_len.to_le_bytes());
                bytes.extend_from_slice(name);
                count += 1;
            } else if parent.generated_id.is_some() {
                return Err(PlacementRecordError::RetainedIdentity);
            }
            while let Some(candidate) = self.doodad_candidates.get(child) {
                if candidate.wmo_placement_index > index {
                    break;
                }
                if candidate.wmo_placement_index != index {
                    return Err(PlacementRecordError::RetainedIdentity);
                }
                check_cancel(&cancelled)?;
                let projected = self
                    .project_doodad(child, profile)
                    .map_err(PlacementRecordError::Projection)?;
                reserve_record(&mut bytes, 22 + projected.suffix_after_generated_id().len())?;
                bytes.extend_from_slice(projected.prefix_before_generated_id());
                bytes.extend_from_slice(&candidate.generated_id.to_le_bytes());
                bytes.extend_from_slice(projected.suffix_after_generated_id());
                count += 1;
                child += 1;
            }
        }
        if child != self.doodad_candidates.len() {
            return Err(PlacementRecordError::RetainedIdentity);
        }
        check_cancel(&cancelled)?;
        self.source
            .verify_unchanged(&cancelled)
            .map_err(PlacementRecordError::Source)?;
        let mut hash = Sha256::new();
        for policy in [
            PLACEMENT_RECORD_POLICY,
            crate::SOURCE_PIN,
            self.summary.acquisition_policy,
            self.summary.listfile_policy,
            self.summary.liquid_header_policy,
            self.summary.writer_policy,
            self.summary.encounter_id_policy,
            self.summary.doodad_reference_order_policy,
            profile.policy_id(),
            &self.summary.source_epoch,
        ] {
            hash.update(policy.as_bytes());
            hash.update([0]);
        }
        for chunk in bytes.chunks(64 * 1024) {
            check_cancel(&cancelled)?;
            hash.update(chunk);
        }
        Ok(PlacementRecords {
            bytes,
            count,
            binding: hash.finalize().into(),
        })
    }
}
fn reserve_record(bytes: &mut Vec<u8>, additional: usize) -> Result<(), PlacementRecordError> {
    reserve_bounded(bytes, additional, MAX_PLACEMENT_RECORD_BYTES)
}
fn reserve_bounded(
    bytes: &mut Vec<u8>,
    additional: usize,
    maximum: usize,
) -> Result<(), PlacementRecordError> {
    let required = bytes
        .len()
        .checked_add(additional)
        .filter(|n| *n <= maximum)
        .ok_or(PlacementRecordError::Limit)?;
    if required > bytes.capacity() {
        // Bound our growth request instead of Vec's unconstrained doubling.
        let target = bytes
            .capacity()
            .saturating_mul(2)
            .max(required)
            .min(maximum);
        bytes
            .try_reserve_exact(target - bytes.len())
            .map_err(|_| PlacementRecordError::Allocation)?;
    }
    if bytes.capacity() > maximum {
        return Err(PlacementRecordError::Limit);
    }
    Ok(())
}
fn check_cancel(cancelled: &dyn Fn() -> bool) -> Result<(), PlacementRecordError> {
    if cancelled() {
        Err(PlacementRecordError::Cancelled)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn record_budget_rejects_overflow_and_over_limit_without_partial_growth() {
        let mut bytes = vec![0; 8];
        assert!(matches!(
            reserve_bounded(&mut bytes, usize::MAX, 16),
            Err(PlacementRecordError::Limit)
        ));
        assert!(matches!(
            reserve_bounded(&mut bytes, 9, 16),
            Err(PlacementRecordError::Limit)
        ));
        assert_eq!(bytes.len(), 8);
        reserve_bounded(&mut bytes, 8, 16).unwrap();
        assert_eq!(bytes.len(), 8);
        assert!(bytes.capacity() <= 16);
        bytes.resize(16, 0);
        assert!(matches!(
            reserve_bounded(&mut bytes, 1, 16),
            Err(PlacementRecordError::Limit)
        ));
    }
    #[test]
    fn map_index_matches_original_container_numeric_rows_and_rejects_bad_internal_streams() {
        let rows = [
            (1u32, 0u32, 9u32, 11u32),
            (0, 0, 3, 33),
            (0, 0, 9, 22),
            (0, 0, 3, 44),
            (65, 65, 1, 55),
            (1, 0, 2, 66),
        ];
        let mut bytes = Vec::new();
        for (x, y, id, payload) in rows {
            let start = bytes.len();
            bytes.resize(start + 55, 0);
            for (at, value) in [
                (0, 7),
                (4, x),
                (8, y),
                (12, 1),
                (18, id),
                (22, payload),
                (50, 1),
            ] {
                bytes[start + at..start + at + 4].copy_from_slice(&value.to_le_bytes());
            }
            bytes[start + 54] = b'A';
        }
        let mut records = PlacementRecords {
            bytes,
            count: 6,
            binding: [0; 32],
        };
        let index = records.map_spawn_index(|| false).unwrap();
        let unique: Vec<_> = index
            .unique_records()
            .map(|(id, b)| (id, u32::from_le_bytes(b[22..26].try_into().unwrap())))
            .collect();
        assert_eq!(unique, [(1, 55), (2, 66), (3, 33), (9, 11)]);
        assert_eq!(
            index.tile_references().collect::<Vec<_>>(),
            [(0, 3), (0, 9), (0, 3), (65536, 9), (65536, 2), (4259905, 1)]
        );
        records.count = crate::MAX_INDEX_RECORDS + 1;
        assert_eq!(
            records.map_spawn_index(|| false).unwrap_err(),
            crate::MapSpawnIndexError::Limit
        );
        records.count = 6;
        records.bytes.pop();
        assert_eq!(
            records.map_spawn_index(|| false).unwrap_err(),
            crate::MapSpawnIndexError::Record
        );
        records.bytes.push(b'A');
        records.bytes[55..59].copy_from_slice(&8u32.to_le_bytes());
        assert_eq!(
            records.map_spawn_index(|| false).unwrap_err(),
            crate::MapSpawnIndexError::Record
        );
    }
    #[test]
    fn map_index_rejects_all_internal_framing_mismatches_and_accepts_empty() {
        let empty = PlacementRecords {
            bytes: Vec::new(),
            count: 0,
            binding: [0; 32],
        };
        let index = empty.map_spawn_index(|| false).unwrap();
        assert_eq!((index.unique_count(), index.tile_reference_count()), (0, 0));
        let mut one = vec![0u8; 55];
        one[12..16].copy_from_slice(&1u32.to_le_bytes());
        one[18..22].copy_from_slice(&1u32.to_le_bytes());
        one[50..54].copy_from_slice(&1u32.to_le_bytes());
        one[54] = b'A';
        let mut oversized = one.clone();
        oversized[50..54].copy_from_slice(&u32::MAX.to_le_bytes());
        let mut wmo = one.clone();
        wmo[12..16].copy_from_slice(&4u32.to_le_bytes());
        let mut trailing = one.clone();
        trailing.push(0);
        for (bytes, count) in [
            (vec![0; 3], 1),
            (one[..18].to_vec(), 1),
            (one.clone(), 0),
            (one.clone(), 2),
            (trailing, 1),
            (oversized, 1),
            (wmo, 1),
        ] {
            let records = PlacementRecords {
                bytes,
                count,
                binding: [0; 32],
            };
            assert_eq!(
                records.map_spawn_index(|| false).unwrap_err(),
                crate::MapSpawnIndexError::Record
            );
        }
    }
}
