// SPDX-License-Identifier: GPL-3.0-or-later
//! Fixed-cache NORMAL calculate reconstruction, not Chase dispatch or restore authority.
//!
//! CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f PathFinder.cpp:306-383,
//! 508-519: retained lookup reads the full backing and active prefix. Only identity
//! transitions of that complete state may be omitted. Unlike Wander, an active
//! switch without an append is not elidable. Replay keeps the actual cold operation
//! and final original ordinal; no polygon reference is an input.
use super::*;
use crate::{LoadedSourceTilePresence, RetainedSourceQuery, SourceQuerySelectionError, TileInput};

#[path = "source_wander_and_chase_reconstruction.rs"]
mod wander_and_chase_reconstruction;
pub use wander_and_chase_reconstruction::*;

pub const SOURCE_CHASE_FIXED_CACHE_VERSION: u32 = 1;
pub const SOURCE_CHASE_FIXED_CACHE_MAX_OPERATIONS: usize = 2;
pub const SOURCE_CHASE_FIXED_CACHE_MAX_BYTES: usize = 111;
const HEADER_BYTES: usize = 47;
const OPERATION_BYTES: usize = 32;
const MAGIC: &[u8; 8] = b"BNCFCP01";

/// Inert original input; equal coordinates at different ordinals remain distinct.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceChaseFixedCacheOperation {
    pub ordinal: u64,
    pub from: [u32; 3],
    pub to: [u32; 3],
}

/// Native-only replay recipe for fixed nontransport/non-dungeon dry NORMAL state.
/// The modeled calculate mode is forceDestination=false, straightLine=true.
/// Owner construction, dispatch, inventory identity and original acceptance/count
/// require separate evidence; parsing this inert recipe establishes none of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceChaseFixedCacheRecipe {
    pub version: u32,
    pub compatibility_version: u32,
    pub slot: SourceMapQuerySlot,
    pub query_capacity: u32,
    pub point_limit: u32,
    pub config: SourceSamePolygonConfig,
    pub filter: SourceOwnerFilter,
    pub operation_count: u64,
    pub operations: Vec<SourceChaseFixedCacheOperation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceChaseFixedCacheError {
    Empty,
    Shape,
    UnsupportedLineage,
    Disabled,
    Native(SourceSamePolygonError),
    Selection(SourceQuerySelectionError),
}
impl From<SourceSamePolygonError> for SourceChaseFixedCacheError {
    fn from(value: SourceSamePolygonError) -> Self {
        Self::Native(value)
    }
}
impl From<Error> for SourceChaseFixedCacheError {
    fn from(value: Error) -> Self {
        Self::Native(value.into())
    }
}
impl From<SourceQuerySelectionError> for SourceChaseFixedCacheError {
    fn from(value: SourceQuerySelectionError) -> Self {
        Self::Selection(value)
    }
}

/// Comparison output only. No replay API accepts this state or imports its refs.
/// Historical before/lookup observations are intentionally not reconstructed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceChaseSemanticState {
    pub query_policy: &'static str,
    pub slot: SourceMapQuerySlot,
    pub query_capacity: u32,
    pub compatibility_id: &'static str,
    pub compatibility_version: u32,
    pub source_revision: &'static str,
    pub config: SourceSamePolygonConfig,
    pub filter: SourceOwnerFilter,
    pub point_limit: usize,
    pub initial_storage: Vec<u64>,
    pub initial_active_len: usize,
    pub retained: bool,
    pub from: [u32; 3],
    pub to: [u32; 3],
    pub actual_end: [u32; 3],
    pub storage: Vec<u64>,
    pub active_len: usize,
    pub path_type: u32,
    pub points: Vec<[u32; 3]>,
}
impl LoadedSourceSamePolygon {
    pub fn chase_semantic_state(&self) -> SourceChaseSemanticState {
        let compatibility = self.compatibility();
        SourceChaseSemanticState {
            query_policy: crate::SOURCE_MAP_QUERY_POLICY,
            slot: self.slot(),
            query_capacity: self.query_capacity(),
            compatibility_id: compatibility.compatibility_id(),
            compatibility_version: compatibility.compatibility_version(),
            source_revision: compatibility.source_revision(),
            config: self.config(),
            filter: compatibility.filter(),
            point_limit: compatibility.point_limit(),
            initial_storage: compatibility.initial_storage().to_vec(),
            initial_active_len: compatibility.initial_active_len(),
            retained: compatibility.is_retained(),
            from: compatibility.from(),
            to: compatibility.to(),
            actual_end: self.actual_end(),
            storage: compatibility.storage().to_vec(),
            active_len: compatibility.active_len(),
            path_type: self.path_type(),
            points: self.points().to_vec(),
        }
    }
}

/// Opaque capture of exact native predecessor identity and unchanged complete K.
/// Clone to stage a proposal; publish it only with the separate owner's acceptance.
/// This witness cannot authenticate owner acceptance, revision, or dispatch mode.
#[derive(Clone, Debug, Default)]
pub struct SourceChaseFixedCacheWitness {
    cold: Option<SourceChaseFixedCacheOperation>,
    last: Option<SourceChaseFixedCacheOperation>,
    operation_count: u64,
    identity: Option<Rc<()>>,
    semantic: Option<SourceChaseSemanticState>,
    disabled: bool,
}
impl SourceChaseFixedCacheWitness {
    pub fn operation_count(&self) -> u64 {
        self.operation_count
    }
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }
    /// Irreversibly leave this capture domain on a separately accepted unsupported
    /// branch, including NOPATH/INCOMPLETE or a non-straight retry. Provider errors
    /// are failed proposals, not accepted source NOPATH, and must not call this.
    pub fn disable(&mut self) {
        self.disabled = true;
    }
    /// An exact successor changing K disables this witness. A foreign, sibling or
    /// skipped proposal rejects atomically. Never reset after an accepted violation;
    /// continuing live calculations remains possible through their native handles.
    pub fn record(
        &mut self,
        terminal: &LoadedSourceSamePolygon,
    ) -> Result<(), SourceChaseFixedCacheError> {
        self.record_at(terminal, self.operation_count)
    }
    fn record_at(
        &mut self,
        terminal: &LoadedSourceSamePolygon,
        ordinal: u64,
    ) -> Result<(), SourceChaseFixedCacheError> {
        use SourceChaseFixedCacheError::{Disabled, UnsupportedLineage};
        if self.disabled {
            return Err(Disabled);
        }
        let compatibility = terminal.compatibility();
        let before = compatibility.before_storage();
        let after = compatibility.storage();
        let limit = crate::SOURCE_WARM_CACHE_POINT_LIMIT;
        match (&self.identity, &self.semantic, &terminal.predecessor) {
            (None, None, None)
                if ordinal == 0
                    && !compatibility.is_retained()
                    && before == [0; crate::SOURCE_WARM_CACHE_POINT_LIMIT]
                    && compatibility.before_active_len() == 0 => {}
            (Some(identity), Some(previous), Some(predecessor))
                if Rc::ptr_eq(identity, predecessor)
                    && ordinal >= self.operation_count
                    && compatibility.is_retained()
                    && previous.slot == terminal.slot()
                    && previous.query_capacity == terminal.query_capacity()
                    && previous.config == terminal.config()
                    && previous.filter == compatibility.filter()
                    && previous.storage == before
                    && previous.active_len == compatibility.before_active_len() => {}
            _ => return Err(UnsupportedLineage),
        }
        let valid = terminal.query_capacity() == 1024
            && !terminal.config().normalize_z
            && !terminal.config().ignore_normalization
            && compatibility.compatibility_version()
                == crate::SOURCE_COLD_DISTANCE_COMPATIBILITY_VERSION
            && compatibility.filter().is_supported_dry()
            && compatibility.point_limit() == limit
            && compatibility.initial_storage() == [0; crate::SOURCE_WARM_CACHE_POINT_LIMIT]
            && compatibility.initial_active_len() == 0
            && (limit..=crate::SOURCE_WARM_CACHE_MAX_STORAGE).contains(&after.len())
            && after[1..limit].iter().all(|&reference| reference == 0)
            && compatibility.active_len() == 1
            && compatibility.phase() == crate::SourceWarmPolyPhase::SamePolygon
            && compatibility.direct_status().is_none()
            && terminal.path_type() == 1
            && terminal.points() == [compatibility.from(), compatibility.to()]
            && terminal.actual_end() == compatibility.to()
            && self.semantic.as_ref().is_none_or(|previous| {
                previous.storage == after && previous.active_len == compatibility.active_len()
            });
        let operation_count = ordinal.checked_add(1).ok_or(UnsupportedLineage)?;
        if !valid {
            self.disable();
            return Err(UnsupportedLineage);
        }
        let operation = SourceChaseFixedCacheOperation {
            ordinal,
            from: compatibility.from(),
            to: compatibility.to(),
        };
        self.cold.get_or_insert(operation);
        self.last = Some(operation);
        self.operation_count = operation_count;
        self.identity = Some(Rc::clone(&terminal.identity));
        self.semantic = Some(terminal.chase_semantic_state());
        Ok(())
    }
    pub fn recipe(&self) -> Result<SourceChaseFixedCacheRecipe, SourceChaseFixedCacheError> {
        if self.disabled {
            return Err(SourceChaseFixedCacheError::Disabled);
        }
        let cold = self.cold.ok_or(SourceChaseFixedCacheError::Empty)?;
        let last = self.last.ok_or(SourceChaseFixedCacheError::Empty)?;
        let state = self
            .semantic
            .as_ref()
            .ok_or(SourceChaseFixedCacheError::Empty)?;
        let mut operations = vec![cold];
        if last.ordinal != cold.ordinal {
            operations.push(last);
        }
        let recipe = SourceChaseFixedCacheRecipe {
            version: SOURCE_CHASE_FIXED_CACHE_VERSION,
            compatibility_version: state.compatibility_version,
            slot: state.slot,
            query_capacity: state.query_capacity,
            point_limit: crate::SOURCE_WARM_CACHE_POINT_LIMIT as u32,
            config: state.config,
            filter: state.filter,
            operation_count: self.operation_count,
            operations,
        };
        recipe.validate()?;
        Ok(recipe)
    }
}

impl SourceChaseFixedCacheRecipe {
    pub fn validate(&self) -> Result<(), SourceChaseFixedCacheError> {
        use SourceChaseFixedCacheError::{Empty, Shape};
        let Some(first) = self.operations.first() else {
            return Err(Empty);
        };
        if self.version != SOURCE_CHASE_FIXED_CACHE_VERSION
            || self.compatibility_version != crate::SOURCE_COLD_DISTANCE_COMPATIBILITY_VERSION
            || self.query_capacity != 1024
            || self.point_limit != crate::SOURCE_WARM_CACHE_POINT_LIMIT as u32
            || self.config.normalize_z
            || self.config.ignore_normalization
            || !self.filter.is_supported_dry()
            || self.operation_count == 0
            || first.ordinal != 0
            || self.operations.len() != if self.operation_count == 1 { 1 } else { 2 }
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
            crate::source_map_coord_pair(operation.from, operation.to)
                .map_err(SourceQuerySelectionError::Coordinates)?;
            crate::detour(operation.from)?;
            crate::detour(operation.to)?;
        }
        Ok(())
    }
    /// Bounded native recipe wire format, not a current-checkpoint restore schema.
    pub fn encode(&self) -> Result<Vec<u8>, SourceChaseFixedCacheError> {
        self.validate()?;
        let size = self
            .operations
            .len()
            .checked_mul(OPERATION_BYTES)
            .and_then(|operations| HEADER_BYTES.checked_add(operations))
            .filter(|&size| size <= SOURCE_CHASE_FIXED_CACHE_MAX_BYTES)
            .ok_or(SourceChaseFixedCacheError::Shape)?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(size).map_err(|_| Error::Budget)?;
        bytes.extend(MAGIC);
        for value in [
            self.version,
            self.compatibility_version,
            self.slot.map_id,
            self.slot.instance_id,
            self.query_capacity,
            self.point_limit,
        ] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend([
            u8::from(self.config.normalize_z),
            u8::from(self.config.ignore_normalization),
        ]);
        bytes.extend(self.filter.include_flags.to_le_bytes());
        bytes.extend(self.filter.exclude_flags.to_le_bytes());
        bytes.extend(self.operation_count.to_le_bytes());
        bytes.push(self.operations.len() as u8);
        for operation in &self.operations {
            bytes.extend(operation.ordinal.to_le_bytes());
            for value in operation.from.into_iter().chain(operation.to) {
                bytes.extend(value.to_le_bytes());
            }
        }
        debug_assert_eq!(bytes.len(), size);
        Ok(bytes)
    }
    /// Checks the total size and operation count before allocating or interpreting
    /// coordinates. Trailing bytes, noncanonical booleans and arithmetic overflow fail.
    pub fn decode(bytes: &[u8]) -> Result<Self, SourceChaseFixedCacheError> {
        use SourceChaseFixedCacheError::Shape;
        if !(HEADER_BYTES..=SOURCE_CHASE_FIXED_CACHE_MAX_BYTES).contains(&bytes.len())
            || bytes.get(..8) != Some(MAGIC.as_slice())
        {
            return Err(Shape);
        }
        let mut bytes = &bytes[8..];
        fn take<const N: usize>(bytes: &mut &[u8]) -> Result<[u8; N], SourceChaseFixedCacheError> {
            let value = bytes.get(..N).ok_or(SourceChaseFixedCacheError::Shape)?;
            let value = value
                .try_into()
                .map_err(|_| SourceChaseFixedCacheError::Shape)?;
            *bytes = &bytes[N..];
            Ok(value)
        }
        let version = u32::from_le_bytes(take(&mut bytes)?);
        let compatibility_version = u32::from_le_bytes(take(&mut bytes)?);
        let slot = SourceMapQuerySlot {
            map_id: u32::from_le_bytes(take(&mut bytes)?),
            instance_id: u32::from_le_bytes(take(&mut bytes)?),
        };
        let query_capacity = u32::from_le_bytes(take(&mut bytes)?);
        let point_limit = u32::from_le_bytes(take(&mut bytes)?);
        let [normalize_z, ignore_normalization] = take(&mut bytes)?;
        if normalize_z > 1 || ignore_normalization > 1 {
            return Err(Shape);
        }
        let config = SourceSamePolygonConfig {
            normalize_z: normalize_z != 0,
            ignore_normalization: ignore_normalization != 0,
        };
        let filter = SourceOwnerFilter {
            include_flags: u16::from_le_bytes(take(&mut bytes)?),
            exclude_flags: u16::from_le_bytes(take(&mut bytes)?),
        };
        let operation_count = u64::from_le_bytes(take(&mut bytes)?);
        let [count] = take(&mut bytes)?;
        let count = usize::from(count);
        if !(1..=SOURCE_CHASE_FIXED_CACHE_MAX_OPERATIONS).contains(&count)
            || count.checked_mul(OPERATION_BYTES) != Some(bytes.len())
        {
            return Err(Shape);
        }
        let mut operations = Vec::new();
        operations
            .try_reserve_exact(count)
            .map_err(|_| Error::Budget)?;
        for _ in 0..count {
            let ordinal = u64::from_le_bytes(take(&mut bytes)?);
            let mut from = [0; 3];
            let mut to = [0; 3];
            for value in from.iter_mut().chain(&mut to) {
                *value = u32::from_le_bytes(take(&mut bytes)?);
            }
            operations.push(SourceChaseFixedCacheOperation { ordinal, from, to });
        }
        let recipe = Self {
            version,
            compatibility_version,
            slot,
            query_capacity,
            point_limit,
            config,
            filter,
            operation_count,
            operations,
        };
        recipe.validate()?;
        Ok(recipe)
    }
}

/// Fresh local attempt identity, never a proof without actual native replay.
#[derive(Clone, Debug)]
pub struct SourceChaseFixedCacheChallenge(Rc<()>);
impl SourceChaseFixedCacheChallenge {
    pub fn new() -> Self {
        Self(Rc::new(()))
    }
    pub fn same_attempt(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl Default for SourceChaseFixedCacheChallenge {
    fn default() -> Self {
        Self::new()
    }
}

/// Opaque proof of one fresh loaded allocation, actual replay, and challenge.
/// The caller must still bind owner/current head/original revision, inventory and
/// complete expected semantics. No runtime owner state, RNG, timers or ACKs replay.
///
/// ```compile_fail
/// use benilla_native_nav::{ReconstructedSourceChaseFixedCache, SourceChaseFixedCacheRecipe};
/// fn forge(recipe: SourceChaseFixedCacheRecipe) -> ReconstructedSourceChaseFixedCache {
///     recipe.into()
/// }
/// ```
#[derive(Debug)]
pub struct ReconstructedSourceChaseFixedCache {
    retained: RetainedSourceQuery,
    terminal: LoadedSourceSamePolygon,
    witness: SourceChaseFixedCacheWitness,
    challenge: SourceChaseFixedCacheChallenge,
}
impl ReconstructedSourceChaseFixedCache {
    pub fn terminal(&self) -> &LoadedSourceSamePolygon {
        &self.terminal
    }
    pub fn retained_query(&self) -> &RetainedSourceQuery {
        &self.retained
    }
    pub fn witness(&self) -> &SourceChaseFixedCacheWitness {
        &self.witness
    }
    pub fn matches_challenge(&self, challenge: &SourceChaseFixedCacheChallenge) -> bool {
        self.challenge.same_attempt(challenge)
    }
    pub fn check_query(&self, selected: &SelectedSourceQuery<'_>) -> Result<(), Error> {
        self.terminal.check_query(selected)
    }
    pub fn into_parts(
        self,
    ) -> (
        RetainedSourceQuery,
        LoadedSourceSamePolygon,
        SourceChaseFixedCacheWitness,
    ) {
        (self.retained, self.terminal, self.witness)
    }
}

/// Read-only source-ordered native observations; an observer can only reject.
/// Constructor runs once. PreSelection exposes each retained input and the current
/// constructor query state before calculate selects it, allowing an independent
/// straight-gate recheck at TargetedMovementGenerator.cpp:498-504. Selected/Ready
/// allow ignore-bit and filter checks. Callbacks cannot certify owner dispatch,
/// supply native state, or alter the operation/constructor being replayed.
///
/// ```compile_fail
/// use benilla_native_nav::SourceChaseFixedCacheStage;
/// fn select_early(stage: SourceChaseFixedCacheStage<'_>) {
///     if let SourceChaseFixedCacheStage::PreSelection { operation, retained } = stage {
///         retained.select_for_calculate(retained.slot(), operation.from, operation.to);
///     }
/// }
/// ```
///
/// ```compile_fail
/// use benilla_native_nav::SourceChaseFixedCacheStage;
/// fn replace_input(stage: SourceChaseFixedCacheStage<'_>) {
///     if let SourceChaseFixedCacheStage::PreSelection { operation, .. } = stage {
///         operation.to = operation.from;
///     }
/// }
/// ```
#[derive(Debug)]
pub enum SourceChaseFixedCacheStage<'a> {
    Constructor {
        retained: &'a RetainedSourceQuery,
    },
    PreSelection {
        operation: &'a SourceChaseFixedCacheOperation,
        retained: &'a RetainedSourceQuery,
    },
    Selected {
        ordinal: u64,
        selected: &'a SelectedSourceQuery<'a>,
    },
    Ready {
        ordinal: u64,
        presence: &'a LoadedSourceTilePresence,
    },
    Terminal {
        ordinal: u64,
        terminal: &'a LoadedSourceSamePolygon,
    },
}
impl SourceMapQuery {
    /// Allocate a fresh unpublished genuine source1024 query and replay at most two
    /// calculations. Only inert inputs enter; a reused loaded query cannot be supplied.
    pub fn reconstruct_chase_fixed_cache(
        slot: SourceMapQuerySlot,
        params: &[u8],
        tiles: &[TileInput<'_>],
        recipe: &SourceChaseFixedCacheRecipe,
        challenge: &SourceChaseFixedCacheChallenge,
        until: Instant,
    ) -> Result<ReconstructedSourceChaseFixedCache, SourceChaseFixedCacheError> {
        Self::reconstruct_chase_fixed_cache_observed(
            slot,
            params,
            tiles,
            recipe,
            challenge,
            until,
            |_| Ok(()),
        )
    }
    pub fn reconstruct_chase_fixed_cache_observed(
        slot: SourceMapQuerySlot,
        params: &[u8],
        tiles: &[TileInput<'_>],
        recipe: &SourceChaseFixedCacheRecipe,
        challenge: &SourceChaseFixedCacheChallenge,
        until: Instant,
        observer: impl FnMut(SourceChaseFixedCacheStage<'_>) -> Result<(), SourceChaseFixedCacheError>,
    ) -> Result<ReconstructedSourceChaseFixedCache, SourceChaseFixedCacheError> {
        recipe.validate()?;
        crate::deadline(until)?;
        if slot != recipe.slot {
            return Err(Error::Identity.into());
        }
        let query = Self::load(slot, params, tiles)?;
        query.replay_chase_fixed_cache_observed(recipe, challenge, until, observer)
    }

    // Only fresh-loading factories in this module can call the shared replay.
    // In particular, no public API accepts an arbitrary already-loaded query.
    fn replay_chase_fixed_cache_observed(
        &self,
        recipe: &SourceChaseFixedCacheRecipe,
        challenge: &SourceChaseFixedCacheChallenge,
        until: Instant,
        mut observer: impl FnMut(
            SourceChaseFixedCacheStage<'_>,
        ) -> Result<(), SourceChaseFixedCacheError>,
    ) -> Result<ReconstructedSourceChaseFixedCache, SourceChaseFixedCacheError> {
        recipe.validate()?;
        crate::deadline(until)?;
        if self.slot() != recipe.slot || self.query_capacity() != recipe.query_capacity {
            return Err(Error::Identity.into());
        }
        let mut retained = self.retain_default();
        observer(SourceChaseFixedCacheStage::Constructor {
            retained: &retained,
        })?;
        let mut witness = SourceChaseFixedCacheWitness::default();
        let mut terminal = None;
        for operation in &recipe.operations {
            crate::deadline(until)?;
            // The straight gate follows lazy construction, before each calculate selection.
            observer(SourceChaseFixedCacheStage::PreSelection {
                operation,
                retained: &retained,
            })?;
            crate::deadline(until)?;
            let selected =
                retained.select_for_calculate(recipe.slot, operation.from, operation.to)?;
            observer(SourceChaseFixedCacheStage::Selected {
                ordinal: operation.ordinal,
                selected: &selected,
            })?;
            let presence = selected.tile_presence(until)?;
            if !presence.both_present() {
                return Err(SourceChaseFixedCacheError::UnsupportedLineage);
            }
            observer(SourceChaseFixedCacheStage::Ready {
                ordinal: operation.ordinal,
                presence: &presence,
            })?;
            let next = match terminal.as_ref() {
                None => selected.loaded_cold_same_polygon_with_filter(
                    recipe.compatibility_version,
                    recipe.filter,
                    recipe.config,
                    SourceSamePolygonBudget::default(),
                    until,
                )?,
                Some(prior) => selected.loaded_retained_same_polygon(
                    prior,
                    recipe.config,
                    SourceSamePolygonBudget {
                        lookup_primitives: SOURCE_RETAINED_SAME_POLYGON_MAX_LOOKUP_PRIMITIVES,
                        terminal_primitives: 1,
                    },
                    until,
                )?,
            };
            observer(SourceChaseFixedCacheStage::Terminal {
                ordinal: operation.ordinal,
                terminal: &next,
            })?;
            witness.record_at(&next, operation.ordinal)?;
            terminal = Some(next);
        }
        if witness.recipe()? != *recipe {
            return Err(SourceChaseFixedCacheError::Shape);
        }
        crate::deadline(until)?;
        Ok(ReconstructedSourceChaseFixedCache {
            retained,
            terminal: terminal.ok_or(SourceChaseFixedCacheError::Empty)?,
            witness,
            challenge: challenge.clone(),
        })
    }
}
