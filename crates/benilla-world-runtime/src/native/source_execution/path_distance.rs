// SPDX-License-Identifier: GPL-3.0-or-later
//! Reference-only storage bytes and a nearest-lookup comparison, never gameplay inputs.
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub enum DistanceCaller {
    Start,
    End,
    RayHit,
    Stepped,
    RandomPoint,
}
impl TryFrom<u8> for DistanceCaller {
    type Error = &'static str;
    fn try_from(value: u8) -> std::result::Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Start),
            2 => Ok(Self::End),
            3 => Ok(Self::RayHit),
            4 => Ok(Self::Stepped),
            5 => Ok(Self::RandomPoint),
            _ => Err("unapproved distance caller"),
        }
    }
}
impl From<DistanceCaller> for u8 {
    fn from(value: DistanceCaller) -> Self {
        match value {
            DistanceCaller::Start => 1,
            DistanceCaller::End => 2,
            DistanceCaller::RayHit => 3,
            DistanceCaller::Stepped => 4,
            DistanceCaller::RandomPoint => 5,
        }
    }
}
impl DistanceCaller {
    pub fn declaration_line(self) -> u32 {
        // Keep both BuildPolyPath scalars distinct at their shared declaration; PathFinder.cpp:410,717,830,1400.
        match self {
            Self::Start | Self::End => 410,
            Self::RayHit => 717,
            Self::Stepped => 830,
            Self::RandomPoint => 1400,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DistanceReadOwner {
    pub caller: DistanceCaller,
    pub thread: u64,
    pub object_generation: u64,
    pub occurrence: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StorageRecord {
    caller: DistanceCaller,
    thread: u64,
    object_generation: u64,
    occurrence: u64,
    width_bytes: u8,
    bytes_le: String,
    source_event: u64,
}
struct StorageRead {
    owner: DistanceReadOwner,
    bits: u32,
    event: u64,
}
/// This tape is deliberately separate from AdmissibleInput and native navigation.
pub struct ReferenceDistanceStorage {
    held: ExecutionFile,
    threads: BTreeMap<u64, (Vec<StorageRead>, usize)>,
}
impl ReferenceDistanceStorage {
    pub fn open(root: &Path, stream: &InputStream) -> Result<Self> {
        require(
            stream.records <= 100_000,
            "execution.distance_record_budget",
        )?;
        let held = stream.receipt.open(root, 16_777_216)?;
        let mut raw = vec![0; held.len() as usize];
        held.read_into(0, &mut raw)?;
        require(
            raw.last() == Some(&b'\n'),
            "execution.distance_truncated_stream",
        )?;
        let mut threads = BTreeMap::<u64, (Vec<StorageRead>, usize)>::new();
        let mut count = 0;
        let mut event = 0;
        for line in raw[..raw.len() - 1].split(|b| *b == b'\n') {
            require(count < stream.records, "execution.distance_record_count")?;
            require(line.len() <= 1024, "execution.distance_record_budget")?;
            let record: StorageRecord = serde_json::from_slice(line)
                .map_err(|_| refuse("execution.distance_storage_schema"))?;
            let bits = storage_bits(record.width_bytes, &record.bytes_le)?;
            let owner = DistanceReadOwner {
                caller: record.caller,
                thread: record.thread,
                object_generation: record.object_generation,
                occurrence: record.occurrence,
            };
            validate_owner(&owner)?;
            let (reads, _) = threads.entry(owner.thread).or_default();
            require(
                owner.occurrence == reads.len() as u64 + 1,
                "execution.distance_occurrence",
            )?;
            require(
                record.source_event > event,
                "execution.distance_event_order",
            )?;
            event = record.source_event;
            reads.push(StorageRead { owner, bits, event });
            count += 1;
        }
        require(count == stream.records, "execution.distance_record_count")?;
        held.check()?;
        Ok(Self { held, threads })
    }
    // Restore only the exact first unwritten read; ReferenceChoices.h:328-350 in stop-candidate-05.
    pub fn read(&mut self, owner: &DistanceReadOwner, width: u8, event: u64) -> Result<u32> {
        validate_owner(owner)?;
        require(width == 4, "execution.distance_width")?;
        self.held.check()?;
        let (reads, cursor) = self
            .threads
            .get_mut(&owner.thread)
            .ok_or_else(|| refuse("execution.distance_thread"))?;
        let read = reads
            .get(*cursor)
            .ok_or_else(|| refuse("execution.distance_exhausted"))?;
        require(read.owner == *owner, "execution.distance_owner")?;
        require(read.event == event, "execution.distance_source_event")?;
        *cursor += 1;
        Ok(read.bits)
    }
    pub fn finish(&self) -> Result<()> {
        self.held.check()?;
        require(
            self.threads
                .values()
                .all(|(reads, cursor)| *cursor == reads.len()),
            "execution.distance_surplus",
        )
    }
}
fn validate_owner(owner: &DistanceReadOwner) -> Result<()> {
    require(
        owner.thread > 0 && owner.object_generation > 0 && owner.occurrence > 0,
        "execution.distance_owner",
    )
}
fn storage_bits(width: u8, text: &str) -> Result<u32> {
    require(
        width == 4 && text.len() == 8 && text.is_ascii(),
        "execution.distance_width",
    )?;
    let mut raw = [0; 4];
    for (byte, pair) in raw.iter_mut().zip(text.as_bytes().as_chunks::<2>().0) {
        let pair = std::str::from_utf8(pair).map_err(|_| refuse("execution.distance_encoding"))?;
        *byte = u8::from_str_radix(pair, 16).map_err(|_| refuse("execution.distance_encoding"))?;
    }
    Ok(u32::from_le_bytes(raw))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ReferencePriorDistance {
    Unwritten { recorded_bits: u32 },
    Defined { bits: u32 },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PathNearestObservation {
    pub owner: DistanceReadOwner,
    pub point_bits: [u32; 3],
    pub extent_bits: [u32; 3],
    pub include_flags: u16,
    pub exclude_flags: u16,
    pub area_cost_bits: Vec<u32>,
    pub status: u32,
    pub polygon: u64,
    pub closest_bits: [u32; 3],
    pub distance_bits: u32,
}
impl PathNearestObservation {
    fn validate(&self) -> Result<()> {
        validate_owner(&self.owner)?;
        require(
            self.area_cost_bits.len() == 64 && self.status & 0x40000000 != 0 && self.polygon != 0,
            "execution.distance_nearest_result",
        )?;
        require(
            self.distance_bits == nearest_distance(self.point_bits, self.closest_bits)?.to_bits(),
            "execution.distance_arithmetic",
        )
    }
}
// Preserve binary32 subtraction, multiply and left-associated sum; DetourCommon.h:217-223.
pub(super) fn nearest_distance(point: [u32; 3], closest: [u32; 3]) -> Result<f32> {
    let p = point.map(f32::from_bits);
    let q = closest.map(f32::from_bits);
    require(
        p.into_iter().chain(q).all(f32::is_finite),
        "execution.distance_coordinates",
    )?;
    let d = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
    let distance = ((d[0] * d[0] + d[1] * d[1]) + d[2] * d[2]).sqrt();
    require(distance.is_finite(), "execution.distance_arithmetic")?;
    Ok(distance)
}
pub(super) fn reference_append(prior: u32, distance: u32) -> bool {
    // Preserve signed zero, infinity and unordered NaN comparisons; PathFinder.cpp:362,377.
    let prior = f32::from_bits(prior);
    prior == 0.0 || f32::from_bits(distance) < prior
}
#[derive(Clone, Debug, Serialize)]
pub struct DistanceBranchComparison {
    pub owner: DistanceReadOwner,
    pub source_event: u64,
    pub reference_prior: ReferencePriorDistance,
    pub distance_bits: u32,
    pub reference_nearest: PathNearestObservation,
    pub native_nearest: Option<PathNearestObservation>,
    pub reference_appended: bool,
    pub native_appended: bool,
    pub approved_absent_distance_difference: bool,
}
impl DifferentialOracle {
    /// Both observations must come from their own geometry execution, not one cloned answer.
    pub fn compare_path_distance_lookup(
        source_event: u64,
        reference: &PathNearestObservation,
        prior: ReferencePriorDistance,
        native: &PathNearestObservation,
        native_prior_bits: Option<u32>,
        native_appended: bool,
    ) -> Result<DistanceBranchComparison> {
        require(source_event > 0, "execution.distance_source_event")?;
        reference.validate()?;
        native.validate()?;
        require(
            reference == native,
            "execution.distance_query_or_result_mismatch",
        )?;
        let reference_bits = match prior {
            ReferencePriorDistance::Unwritten { recorded_bits } => {
                require(
                    native_prior_bits.is_none(),
                    "execution.distance_unwritten_native_prior",
                )?;
                recorded_bits
            }
            ReferencePriorDistance::Defined { bits } => {
                require(
                    native_prior_bits == Some(bits),
                    "execution.distance_defined_prior_mismatch",
                )?;
                bits
            }
        };
        let distance = native.distance_bits;
        require(
            native_appended
                == native_prior_bits.is_none_or(|bits| reference_append(bits, distance)),
            "execution.distance_native_predicate",
        )?;
        let reference_appended = reference_append(reference_bits, reference.distance_bits);
        Ok(DistanceBranchComparison {
            owner: reference.owner.clone(),
            source_event,
            reference_prior: prior,
            distance_bits: distance,
            reference_nearest: reference.clone(),
            native_nearest: Some(native.clone()),
            reference_appended,
            native_appended,
            approved_absent_distance_difference: reference_appended != native_appended,
        })
    }
}

#[cfg(test)]
mod tests;
