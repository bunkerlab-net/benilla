// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded input replay for the immutable selected dry Wander lineage.
//!
//! CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, PathFinder.cpp:306-383,
//! 506-519,1351-1425: preserve every ordered tail appender, the latest NORMAL,
//! latest non-MinimumDistance endpoint writer, and final operation. The union
//! reconstructs complete semantic state, including inactive storage. It does not
//! reconstruct omitted before-observations, owner revisions, RNG, time or ACKs.
use super::*;

pub const SOURCE_WANDER_RECONSTRUCTION_VERSION: u32 = 1;
pub const SOURCE_WANDER_MAX_RECONSTRUCTION_OPERATIONS: usize =
    crate::SOURCE_WARM_CACHE_MAX_STORAGE - crate::SOURCE_WARM_CACHE_POINT_LIMIT + 3;
const MAX_APPEND_OPERATIONS: usize =
    crate::SOURCE_WARM_CACHE_MAX_STORAGE - crate::SOURCE_WARM_CACHE_POINT_LIMIT;

/// Inert operation input; ordinals describe the original accepted input sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceWanderReconstructionOperation {
    pub ordinal: u64,
    pub from: [u32; 3],
    pub raw_end: [u32; 3],
}

/// Inert, serializable-by-field recipe. No references, cache, loaded handles or
/// authority are imported. Deserializers must bound allocation independently;
/// replay validates the complete shape before executing the first operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceWanderReconstructionRecipe {
    pub version: u32,
    pub compatibility_version: u32,
    pub slot: SourceMapQuerySlot,
    pub query_capacity: u32,
    pub config: SourceSamePolygonConfig,
    pub filter: SourceOwnerFilter,
    pub operation_count: u64,
    pub operations: Vec<SourceWanderReconstructionOperation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceWanderReconstructionError {
    Empty,
    Shape,
    UnsupportedLineage,
    Native(SourceWanderError),
}
impl From<SourceWanderError> for SourceWanderReconstructionError {
    fn from(value: SourceWanderError) -> Self {
        Self::Native(value)
    }
}
impl From<Error> for SourceWanderReconstructionError {
    fn from(value: Error) -> Self {
        Self::Native(value.into())
    }
}

/// Comparison-only complete native semantic output. Replay never accepts this
/// structure and never installs its references. A caller must compare every field
/// with the expected persisted output before granting separate owner authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceWanderSemanticState {
    pub slot: SourceMapQuerySlot,
    pub query_capacity: u32,
    pub config: SourceSamePolygonConfig,
    pub filter: SourceOwnerFilter,
    pub from: [u32; 3],
    pub raw_end: [u32; 3],
    pub actual_end: [u32; 3],
    pub storage: Vec<u64>,
    pub active_len: usize,
    pub path_type: u32,
    pub points: Vec<[u32; 3]>,
    pub no_path_reason: Option<SourceWanderNoPathReason>,
}
impl LoadedSourceWanderTerminal {
    pub fn from(&self) -> [u32; 3] {
        self.from
    }
    pub fn semantic_state(&self) -> SourceWanderSemanticState {
        SourceWanderSemanticState {
            slot: self.slot(),
            query_capacity: self.query_capacity(),
            config: self.config(),
            filter: self.filter(),
            from: self.from,
            raw_end: self.raw_end(),
            actual_end: self.actual_end(),
            storage: self.storage().to_vec(),
            active_len: self.active_len(),
            path_type: self.path_type(),
            points: self.points().to_vec(),
            no_path_reason: self.no_path_reason(),
        }
    }
}

/// Opaque transactional capture. Clone to stage an accepted operation and commit
/// the clone only after the separate owner's ACK. Tokens authenticate the exact
/// native predecessor, even if another terminal has identical coordinates/state.
/// This capture does not itself establish owner acceptance or revision authority.
#[derive(Clone, Debug, Default)]
pub struct SourceWanderWitness {
    append_writers: Vec<SourceWanderReconstructionOperation>,
    last_normal: Option<SourceWanderReconstructionOperation>,
    last_endpoint: Option<SourceWanderReconstructionOperation>,
    last: Option<SourceWanderReconstructionOperation>,
    operation_count: u64,
    identity: Option<Rc<()>>,
    semantic: Option<SourceWanderSemanticState>,
}
impl SourceWanderWitness {
    pub fn operation_count(&self) -> u64 {
        self.operation_count
    }
    /// Capture only after successful native work, in the accepted owner's order.
    /// Rejection leaves the witness unchanged; a skipped or sibling predecessor
    /// cannot enter the chain by supplying an equal cache or equal coordinates.
    pub fn record(
        &mut self,
        terminal: &LoadedSourceWanderTerminal,
    ) -> Result<(), SourceWanderReconstructionError> {
        self.record_at(terminal, self.operation_count)
    }
    fn record_at(
        &mut self,
        terminal: &LoadedSourceWanderTerminal,
        ordinal: u64,
    ) -> Result<(), SourceWanderReconstructionError> {
        use SourceWanderReconstructionError::UnsupportedLineage;
        let operation_count = ordinal.checked_add(1).ok_or(UnsupportedLineage)?;
        if ordinal < self.operation_count {
            return Err(UnsupportedLineage);
        }
        let limit = crate::SOURCE_WARM_CACHE_POINT_LIMIT;
        let before = terminal.before_storage();
        let after = terminal.storage();
        if terminal.query_capacity() != 1024
            || terminal.config().normalize_z
            || terminal.config().ignore_normalization
            || !terminal.filter().is_supported_dry()
            || before.len() < limit
            || after.len() < before.len()
            || after.len() > crate::SOURCE_WARM_CACHE_MAX_STORAGE
            || before[1..limit].iter().any(|&reference| reference != 0)
            || after[1..limit].iter().any(|&reference| reference != 0)
            || after[limit..before.len()] != before[limit..]
        {
            return Err(UnsupportedLineage);
        }
        match (&self.identity, &self.semantic, &terminal.predecessor) {
            (None, None, None)
                if before == [0; crate::SOURCE_WARM_CACHE_POINT_LIMIT]
                    && terminal.before_active_len() == 0 => {}
            (Some(identity), Some(previous), Some(predecessor))
                if Rc::ptr_eq(identity, predecessor)
                    && previous.slot == terminal.slot()
                    && previous.query_capacity == terminal.query_capacity()
                    && previous.config == terminal.config()
                    && previous.filter == terminal.filter()
                    && previous.storage == before
                    && previous.active_len == terminal.before_active_len() => {}
            _ => return Err(UnsupportedLineage),
        }
        let normal = terminal.no_path_reason().is_none();
        if (normal
            && (terminal.path_type() != 1
                || terminal.active_len() != 1
                || terminal.points().len() != 2))
            || (!normal
                && (terminal.path_type() != 8
                    || terminal.active_len() != 0
                    || !terminal.points().is_empty()))
            || (after.len() > before.len() && self.append_writers.len() >= MAX_APPEND_OPERATIONS)
        {
            return Err(UnsupportedLineage);
        }
        let operation = SourceWanderReconstructionOperation {
            ordinal,
            from: terminal.from,
            raw_end: terminal.raw_end(),
        };
        if after.len() > before.len() {
            self.append_writers.push(operation);
        }
        if normal {
            self.last_normal = Some(operation);
        }
        if terminal.no_path_reason() != Some(SourceWanderNoPathReason::MinimumDistance) {
            self.last_endpoint = Some(operation);
        }
        self.last = Some(operation);
        self.operation_count = operation_count;
        self.identity = Some(Rc::clone(&terminal.identity));
        self.semantic = Some(terminal.semantic_state());
        Ok(())
    }
    pub fn recipe(
        &self,
    ) -> Result<SourceWanderReconstructionRecipe, SourceWanderReconstructionError> {
        let semantic = self
            .semantic
            .as_ref()
            .ok_or(SourceWanderReconstructionError::Empty)?;
        let mut operations = self.append_writers.clone();
        operations.extend(self.last_normal);
        operations.extend(self.last_endpoint);
        operations.extend(self.last);
        operations.sort_unstable_by_key(|operation| operation.ordinal);
        operations.dedup_by_key(|operation| operation.ordinal);
        Ok(SourceWanderReconstructionRecipe {
            version: SOURCE_WANDER_RECONSTRUCTION_VERSION,
            compatibility_version: SOURCE_WANDER_ENDPOINT_COMPATIBILITY_VERSION,
            slot: semantic.slot,
            query_capacity: semantic.query_capacity,
            config: semantic.config,
            filter: semantic.filter,
            operation_count: self.operation_count,
            operations,
        })
    }
}
impl SourceWanderReconstructionRecipe {
    pub fn validate(&self) -> Result<(), SourceWanderReconstructionError> {
        use SourceWanderReconstructionError::{Empty, Shape};
        if self.operations.is_empty() {
            return Err(Empty);
        }
        if self.version != SOURCE_WANDER_RECONSTRUCTION_VERSION
            || self.compatibility_version != SOURCE_WANDER_ENDPOINT_COMPATIBILITY_VERSION
            || self.query_capacity != 1024
            || self.config.normalize_z
            || self.config.ignore_normalization
            || !self.filter.is_supported_dry()
            || self.operations.len() > SOURCE_WANDER_MAX_RECONSTRUCTION_OPERATIONS
            || self.operation_count == 0
            || self
                .operations
                .last()
                .and_then(|last| last.ordinal.checked_add(1))
                != Some(self.operation_count)
            || self
                .operations
                .windows(2)
                .any(|pair| pair[0].ordinal >= pair[1].ordinal)
        {
            return Err(Shape);
        }
        for operation in &self.operations {
            crate::detour(operation.from)?;
            crate::detour(operation.raw_end)?;
        }
        Ok(())
    }
}

/// Fresh attempt identity. Constructing or cloning this token grants no query or
/// owner authority. Only actual replay can bind it to reconstructed native output.
#[derive(Clone, Debug)]
pub struct SourceWanderReconstructionChallenge(Rc<()>);
impl SourceWanderReconstructionChallenge {
    pub fn new() -> Self {
        Self(Rc::new(()))
    }
    pub fn same_attempt(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl Default for SourceWanderReconstructionChallenge {
    fn default() -> Self {
        Self::new()
    }
}

/// Opaque evidence of executing the bounded recipe on a real loaded allocation.
/// This does not prove the recipe's owner, original operation count, map inventory,
/// geometry, runtime authority, or equality with an expected checkpoint. The app
/// must establish those independently and compare complete semantic output.
///
/// ```compile_fail
/// use benilla_native_nav::{ReconstructedSourceWanderTerminal, SourceWanderReconstructionRecipe};
/// fn forge(recipe: SourceWanderReconstructionRecipe) -> ReconstructedSourceWanderTerminal {
///     recipe.into()
/// }
/// ```
#[derive(Debug)]
pub struct ReconstructedSourceWanderTerminal {
    retained: RetainedSourceQuery,
    challenge: SourceWanderReconstructionChallenge,
    terminal: LoadedSourceWanderTerminal,
    witness: SourceWanderWitness,
}
impl ReconstructedSourceWanderTerminal {
    pub fn matches_challenge(&self, challenge: &SourceWanderReconstructionChallenge) -> bool {
        self.challenge.same_attempt(challenge)
    }
    pub fn terminal(&self) -> &LoadedSourceWanderTerminal {
        &self.terminal
    }
    pub fn retained_query(&self) -> &RetainedSourceQuery {
        &self.retained
    }
    pub fn witness(&self) -> &SourceWanderWitness {
        &self.witness
    }
    pub fn check_query(&self, selected: &SelectedSourceQuery<'_>) -> Result<(), Error> {
        self.terminal.check_query(selected)
    }
    pub fn into_parts(
        self,
    ) -> (
        RetainedSourceQuery,
        LoadedSourceWanderTerminal,
        SourceWanderWitness,
    ) {
        (self.retained, self.terminal, self.witness)
    }
}
/// Read-only observation points in the real replay. Observers may reject but
/// cannot provide native answers, alter replay inputs, or manufacture certificates.
#[derive(Debug)]
pub enum SourceWanderReconstructionStage<'a> {
    Constructor,
    Prepared {
        ordinal: u64,
        prefix: &'a LoadedSourceWanderPrefix,
    },
    Selected {
        ordinal: u64,
    },
    Terminal {
        ordinal: u64,
        terminal: &'a LoadedSourceWanderTerminal,
    },
}
impl SourceMapQuery {
    /// Starts a fresh constructor cache on this actual query; callers establish
    /// fresh provider/inventory identity. No prior handle or stored refs are input.
    /// Uses the existing per-operation ceilings and a shared caller deadline.
    pub fn reconstruct_wander(
        &self,
        recipe: &SourceWanderReconstructionRecipe,
        challenge: &SourceWanderReconstructionChallenge,
        until: Instant,
    ) -> Result<ReconstructedSourceWanderTerminal, SourceWanderReconstructionError> {
        self.reconstruct_wander_observed(recipe, challenge, until, |_| Ok(()))
    }
    /// Runs the same native replay with source-ordered read-only observations.
    /// Separate geometry/owner boundaries can reject at the reached stage. A
    /// successful observer alone does not grant any authority to this certificate.
    pub fn reconstruct_wander_observed(
        &self,
        recipe: &SourceWanderReconstructionRecipe,
        challenge: &SourceWanderReconstructionChallenge,
        until: Instant,
        mut observer: impl FnMut(
            SourceWanderReconstructionStage<'_>,
        ) -> Result<(), SourceWanderReconstructionError>,
    ) -> Result<ReconstructedSourceWanderTerminal, SourceWanderReconstructionError> {
        recipe.validate()?;
        crate::deadline(until)?;
        if self.slot() != recipe.slot || self.query_capacity() != recipe.query_capacity {
            return Err(Error::Identity.into());
        }
        let mut retained = self.retain_default();
        observer(SourceWanderReconstructionStage::Constructor)?;
        let mut witness = SourceWanderWitness::default();
        let mut terminal = None;
        for operation in &recipe.operations {
            let mut prefix = retained.prepare_wander_same_polygon(
                terminal.as_ref(),
                recipe.compatibility_version,
                recipe.filter,
                recipe.config,
                SourceWanderBudget {
                    endpoint_lookup_primitives: SOURCE_WANDER_MAX_ENDPOINT_LOOKUP_PRIMITIVES,
                    build_lookup_primitives: SOURCE_WANDER_MAX_BUILD_LOOKUP_PRIMITIVES,
                    terminal_primitives: 1,
                },
                until,
            )?;
            observer(SourceWanderReconstructionStage::Prepared {
                ordinal: operation.ordinal,
                prefix: &prefix,
            })?;
            retained.select_wander_prefix(&mut prefix, until)?;
            observer(SourceWanderReconstructionStage::Selected {
                ordinal: operation.ordinal,
            })?;
            let selected =
                retained.continue_wander_prefix(&prefix, operation.from, operation.raw_end)?;
            let next = match selected.loaded_prepared_wander_no_path(&prefix, until) {
                Err(SourceWanderError::NotTooClose) => {
                    if !selected.tile_presence(until)?.both_present() {
                        return Err(SourceWanderReconstructionError::UnsupportedLineage);
                    }
                    selected.loaded_prepared_wander_same_polygon(&prefix, until)?
                }
                result => result?,
            };
            observer(SourceWanderReconstructionStage::Terminal {
                ordinal: operation.ordinal,
                terminal: &next,
            })?;
            witness.record_at(&next, operation.ordinal)?;
            terminal = Some(next);
        }
        // Redundant/noncanonical operations cannot smuggle an unbounded transcript
        // into the bounded union of actual append and last-writer observations.
        if witness.recipe()? != *recipe {
            return Err(SourceWanderReconstructionError::Shape);
        }
        crate::deadline(until)?;
        Ok(ReconstructedSourceWanderTerminal {
            retained,
            challenge: challenge.clone(),
            terminal: terminal.ok_or(SourceWanderReconstructionError::Empty)?,
            witness,
        })
    }
}
