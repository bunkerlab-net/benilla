// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-bound, bounded discovery for one selected map, before VMap assembly.
//!
//! The selected physical Map.dbc record gates a WDT read; an accepted WDT causes
//! all 4096 ADT attempts in source x-major order, independently of MAIN. Every
//! consumed member, absent lookup, and winning dummy has a private ordered
//! receipt. The fresh-chain check detects changed inputs, not a filesystem lock.
//! Complete raw model products and raw client IDs are candidates only: this
//! crate publishes no files, generates no final encounter IDs, and grants no
//! map, BIH, collision-absence, or gameplay authority.

mod discovery;
mod doodad_projection;
// Reference-container profile is proof-only until its complete gates pass.
#[cfg(test)]
mod doodad_reference_order;
mod doodad_transform_math;
mod doodads;
mod source;

use benilla_native_vmap::{
    extract_m2_collision, extract_wmo_collision_from_source, M2Collision, WmoCollision,
    WmoGroupInput, MAX_WMO_GROUPS, MAX_WMO_TOTAL_BYTES, WMO_LIQUID_HEADER_ZERO_PADDING_V1,
};
use discovery::{DiscoveryEvent, ModelDependency, PlacementInputs};
use serde::Serialize;
use source::{CapturedMember, CapturedSource, Lookup};
use std::{
    collections::{BTreeSet, HashMap},
    fmt,
    path::Path,
};

pub use discovery::{DiscoveryError, M2Placement, ModelKind, PlacementInput, WmoPlacement};
pub use doodad_projection::{DoodadProjection, DoodadProjectionError, DoodadProjectionProfile};
pub use doodads::{DoodadError, DoodadSet, DoodadSpawn};
pub use source::SourceError;

pub const SOURCE_PIN: &str = "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f";
/// Raw products are bounded independently of archive read work and source memory.
pub const MAX_RETAINED_RAW_BYTES: usize = 256 * 1024 * 1024;
pub const MAX_PLACEMENTS: usize = 1_048_576;
pub const MAX_PRODUCTS: usize = 32_768;
pub const MAX_DEPENDENCY_EVENTS: usize = 262_144;
pub const MAX_GLOBAL_ROOT_EVENTS: usize = 32_768;
pub const MAX_RETAINED_DOODAD_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_DOODAD_CANDIDATES: usize = 1_048_576;

/// Numeric source discovery scope, never a collision-clearance conclusion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum MapDiscoveryState {
    WdtAbsent,
    WdtDummy,
    Scanned,
}

/// Source raw-output filtering, before assembly and without generated IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlacementDisposition {
    Emits,
    NoCollision,
    Destructible,
}

/// ADT filename coordinates. These are not terrain-product grid coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tile {
    pub x: u32,
    pub y: u32,
}

/// Original client record with its source-order product association.
/// No coordinate transformation or global encounter ID allocation is performed.
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    tile: Tile,
    product: usize,
    input: PlacementInput,
    disposition: PlacementDisposition,
}
impl Placement {
    pub fn tile(&self) -> Tile {
        self.tile
    }
    pub fn product_index(&self) -> usize {
        self.product
    }
    pub fn input(&self) -> &PlacementInput {
        &self.input
    }
    pub fn disposition(&self) -> PlacementDisposition {
        self.disposition
    }
}

/// Raw selected-set member. Iteration is a stable diagnostic presentation only;
/// it is not the upstream unordered-set order or a generated doodad ID.
#[derive(Clone, Debug, PartialEq)]
pub struct DoodadCandidate {
    pub wmo_placement_index: usize,
    pub spawn_index: u16,
    pub product_index: usize,
    pub spawn: DoodadSpawn,
    /// Original basename length before .mdx/.mdl shortening; no asset text.
    pub serialized_name_len: u32,
}

struct CapturedDoodads {
    root: doodads::RootDoodads,
    references: Vec<u16>,
}

enum Collision {
    M2EmptyHull,
    M2(Vec<u8>),
    Wmo(WmoCollision),
}

/// Complete source-compatible model output, or the explicit M2 zero-hull case.
/// Asset names are available for future assembly but never included in Debug or
/// the serializable summary. Neither this value nor its hash mints authority.
pub struct RawProduct {
    kind: ModelKind,
    member: String,
    extracted_name: String,
    collision: Collision,
    doodads: Option<CapturedDoodads>,
}
impl RawProduct {
    pub fn kind(&self) -> ModelKind {
        self.kind
    }
    pub fn source_member(&self) -> &str {
        &self.member
    }
    pub fn extracted_name(&self) -> &str {
        &self.extracted_name
    }
    pub fn raw_vmap(&self) -> Option<&[u8]> {
        match &self.collision {
            Collision::M2EmptyHull => None,
            Collision::M2(bytes) => Some(bytes),
            Collision::Wmo(wmo) => Some(&wmo.raw_vmap),
        }
    }
    /// Whether the source raw file's offset-eight count permits a placement.
    /// This is not a proof of geometric clearance or a gameplay decision.
    pub fn source_has_collision(&self) -> bool {
        match &self.collision {
            Collision::M2EmptyHull => false,
            Collision::M2(_) => true,
            Collision::Wmo(wmo) => wmo.collision_triangle_count != 0,
        }
    }
    pub fn wmo_group_counts(&self) -> Option<(u32, u32)> {
        match &self.collision {
            Collision::Wmo(wmo) => Some((wmo.declared_group_count, wmo.retained_group_count)),
            _ => None,
        }
    }
}
impl fmt::Debug for RawProduct {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawProduct")
            .field("kind", &self.kind)
            .field("bytes", &self.raw_vmap().map_or(0, <[u8]>::len))
            .field("source_has_collision", &self.source_has_collision())
            .finish_non_exhaustive()
    }
}

/// Safe diagnostics: numeric identifiers and counts only, never install paths,
/// archive member names, model text, placements, or a serialized witness.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DiscoverySummary {
    pub source_pin: &'static str,
    pub acquisition_policy: &'static str,
    pub listfile_policy: &'static str,
    pub writer_policy: &'static str,
    pub liquid_header_policy: &'static str,
    pub source_epoch: String,
    pub map_id: u32,
    pub map_record_ordinal: usize,
    pub state: MapDiscoveryState,
    pub archive_count: usize,
    pub member_lookups: usize,
    pub read_work_bytes: u64,
    pub adt_attempts: usize,
    pub adts_present: usize,
    pub adts_absent: usize,
    pub adts_dummy: usize,
    pub dependency_events: usize,
    pub global_root_events: usize,
    pub prefix_maps: usize,
    pub prefix_adt_attempts: usize,
    pub products: usize,
    pub m2_raw_products: usize,
    pub m2_no_collision_products: usize,
    pub wmo_raw_products: usize,
    pub wmo_liquid_groups: usize,
    pub placements: usize,
    pub emitting_placements: usize,
    pub no_collision_placements: usize,
    pub destructible_placements: usize,
    pub retained_raw_bytes: usize,
    pub retained_doodad_bytes: usize,
    pub doodad_extraction_events: usize,
    pub doodad_candidates: usize,
}

/// Owns privately captured source provenance and complete discovery products.
/// There is no public constructor, deserializer, supplied hash, or publish API.
pub struct PreparedDiscovery {
    source: CapturedSource,
    summary: DiscoverySummary,
    placements: Vec<Placement>,
    products: Vec<RawProduct>,
    doodad_candidates: Vec<DoodadCandidate>,
}
impl PreparedDiscovery {
    pub fn map_id(&self) -> u32 {
        self.summary.map_id
    }
    pub fn summary(&self) -> &DiscoverySummary {
        &self.summary
    }
    pub fn placements(&self) -> &[Placement] {
        &self.placements
    }
    pub fn doodad_candidates(&self) -> &[DoodadCandidate] {
        &self.doodad_candidates
    }
    pub fn products(&self) -> &[RawProduct] {
        &self.products
    }
    /// Diagnostic digest only; cannot construct a witness or prove a live lease.
    pub fn source_epoch(&self) -> String {
        self.source.binding_digest()
    }
    pub fn verify_unchanged(&self, cancelled: impl Fn() -> bool) -> Result<(), Error> {
        self.source
            .verify_unchanged(&cancelled)
            .map_err(Error::Source)
    }
}
impl fmt::Debug for PreparedDiscovery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedDiscovery")
            .field("summary", &self.summary)
            .finish_non_exhaustive()
    }
}

/// Safe location and bounded work counters for a failed preparation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum PreparationStage {
    OpenSource,
    LiquidTypes,
    GlobalWmoLists,
    GlobalWmoRoot,
    MapMetadata,
    PrefixMap,
    SelectedWdt,
    AdtTraversal,
    Verification,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct FailureProgress {
    pub member_lookups: usize,
    pub read_work_bytes: u64,
    pub complete_raw_products: usize,
    pub adt_attempts: usize,
}

/// Fail-closed preparation errors. Asset text and filesystem paths are omitted.
pub enum Error {
    Preparation {
        stage: PreparationStage,
        global_root_ordinal: Option<usize>,
        progress: FailureProgress,
        cause: Box<Error>,
    },
    Source(SourceError),
    Discovery(DiscoveryError),
    Doodad(DoodadError),
    M2(benilla_native_vmap::Error),
    Wmo(benilla_native_vmap::WmoError),
    Cancelled,
    MapMissing,
    DuplicateMap,
    RequiredMemberAbsent,
    RequiredMemberDummy,
    NameCollision,
    UnresolvedWmoReference,
    UnsupportedPrefixMapDirectory,
    GlobalRootLimit,
    DependencyOrder,
    DependencyLimit,
    ProductLimit,
    PlacementLimit,
    RetainedOutputLimit,
    WmoInputLimit,
    InvalidWmoEnvelope,
    UnsupportedWmoDoodadClosure,
    InvalidDoodadEnvelope,
    DoodadNameIdentity,
    DoodadBudget,
    UnsupportedWmoLiquid,
    SourceChangedDuringPreparation,
    AllocationFailed,
}
/// Public failure telemetry contains categories and numeric progress only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FailureDiagnostic {
    pub category: String,
    pub stage: Option<PreparationStage>,
    pub global_root_ordinal: Option<usize>,
    pub progress: Option<FailureProgress>,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.diagnostic().category)
    }
}
impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.diagnostic().fmt(f)
    }
}
impl std::error::Error for Error {}
impl Error {
    pub fn category(&self) -> &Self {
        match self {
            Self::Preparation { cause, .. } => cause.category(),
            other => other,
        }
    }
    pub fn diagnostic(&self) -> FailureDiagnostic {
        let category = match self.category() {
            Self::Source(error) => error.to_string(),
            Self::Discovery(error) => error.code().to_owned(),
            Self::Doodad(error) => error.code().to_owned(),
            Self::M2(error) => m2_category(error).to_owned(),
            Self::Wmo(error) => wmo_category(error).to_owned(),
            other => match other {
                Self::Cancelled => "vmap.cancelled",
                Self::MapMissing => "vmap.map_missing",
                Self::DuplicateMap => "vmap.duplicate_map",
                Self::RequiredMemberAbsent => "vmap.required_member_absent",
                Self::RequiredMemberDummy => "vmap.required_member_dummy",
                Self::NameCollision => "vmap.flat_name_collision",
                Self::UnresolvedWmoReference => "vmap.unresolved_wmo_reference",
                Self::UnsupportedPrefixMapDirectory => "vmap.unsupported_prefix_map_directory",
                Self::GlobalRootLimit => "vmap.global_root_limit",
                Self::DependencyOrder => "vmap.dependency_order",
                Self::DependencyLimit => "vmap.dependency_limit",
                Self::ProductLimit => "vmap.product_limit",
                Self::PlacementLimit => "vmap.placement_limit",
                Self::RetainedOutputLimit => "vmap.retained_output_limit",
                Self::WmoInputLimit => "vmap.wmo_input_limit",
                Self::InvalidWmoEnvelope => "vmap.invalid_wmo_envelope",
                Self::UnsupportedWmoDoodadClosure => "vmap.unsupported_wmo_doodad_closure",
                Self::InvalidDoodadEnvelope => "vmap.invalid_doodad_envelope",
                Self::DoodadNameIdentity => "vmap.doodad_name_identity",
                Self::DoodadBudget => "vmap.doodad_budget",
                Self::UnsupportedWmoLiquid => "vmap.unsupported_wmo_liquid",
                Self::SourceChangedDuringPreparation => "vmap.source_changed_during_preparation",
                Self::AllocationFailed => "vmap.allocation_failed",
                _ => unreachable!(),
            }
            .to_owned(),
        };
        let (stage, global_root_ordinal, progress) = match self {
            Self::Preparation {
                stage,
                global_root_ordinal,
                progress,
                ..
            } => (Some(*stage), *global_root_ordinal, Some(*progress)),
            _ => (None, None, None),
        };
        FailureDiagnostic {
            category,
            stage,
            global_root_ordinal,
            progress,
        }
    }
}
fn m2_category(error: &benilla_native_vmap::Error) -> &'static str {
    use benilla_native_vmap::Error::*;
    match error {
        InputLimit { .. } => "vmap.m2.input_limit",
        TruncatedHeader { .. } => "vmap.m2.truncated_header",
        UnsupportedMagic(_) => "vmap.m2.unsupported_magic",
        UnsupportedVersion(_) => "vmap.m2.unsupported_version",
        CountLimit { .. } => "vmap.m2.count_limit",
        IncompleteTriangle { .. } => "vmap.m2.incomplete_triangle",
        ArrayBeforeHeader { .. } => "vmap.m2.array_before_header",
        ArrayRangeOverflow(_) => "vmap.m2.array_range_overflow",
        TruncatedArray(_) => "vmap.m2.truncated_array",
        IndexOutOfRange { .. } => "vmap.m2.index_out_of_range",
        NonFiniteVertex { .. } => "vmap.m2.non_finite_vertex",
        OutputSizeOverflow => "vmap.m2.output_size_overflow",
        AllocationFailed => "vmap.m2.allocation_failed",
    }
}
fn wmo_category(error: &benilla_native_vmap::WmoError) -> &'static str {
    use benilla_native_vmap::WmoError::*;
    match error {
        InputLimit { .. } => "vmap.wmo.input_limit",
        TotalInputLimit { .. } => "vmap.wmo.total_input_limit",
        GroupCountLimit { .. } => "vmap.wmo.group_count_limit",
        GroupCountMismatch { .. } => "vmap.wmo.group_count_mismatch",
        GroupOrder { .. } => "vmap.wmo.group_order",
        ChunkLimit { .. } => "vmap.wmo.chunk_limit",
        TruncatedChunkHeader { .. } => "vmap.wmo.truncated_chunk_header",
        TruncatedChunk { .. } => "vmap.wmo.truncated_chunk",
        MissingChunk { .. } => "vmap.wmo.missing_chunk",
        DuplicateChunk { .. } => "vmap.wmo.duplicate_chunk",
        UnexpectedChunk { .. } => "vmap.wmo.unexpected_chunk",
        InvalidChunkSize { .. } => "vmap.wmo.invalid_chunk_size",
        UnsupportedVersion { .. } => "vmap.wmo.unsupported_version",
        CountLimit { .. } => "vmap.wmo.count_limit",
        TriangleIndexCount { .. } => "vmap.wmo.triangle_index_count",
        IndexOutOfRange { .. } => "vmap.wmo.index_out_of_range",
        NonFinite { .. } => "vmap.wmo.non_finite",
        ReversedBounds { .. } => "vmap.wmo.reversed_bounds",
        NegativeGroupName { .. } => "vmap.wmo.negative_group_name",
        UnterminatedGroupName { .. } => "vmap.wmo.unterminated_group_name",
        UnsupportedLiquid { .. } => "vmap.wmo.unsupported_liquid",
        InvalidSourceRootMember => "vmap.wmo.invalid_source_member",
        InvalidLiquidDimensions { .. } => "vmap.wmo.invalid_liquid_dimensions",
        LiquidSizeOverflow { .. } => "vmap.wmo.liquid_size_overflow",
        NonFiniteLiquidHeight { .. } => "vmap.wmo.non_finite_liquid_height",
        OutputLimit { .. } => "vmap.wmo.output_limit",
        AllocationFailed => "vmap.wmo.allocation_failed",
    }
}
impl From<SourceError> for Error {
    fn from(e: SourceError) -> Self {
        Self::Source(e)
    }
}
impl From<DiscoveryError> for Error {
    fn from(e: DiscoveryError) -> Self {
        Self::Discovery(e)
    }
}

/// Capture and discover one selected map using the exact pinned VMap archive
/// policy. Cancellation returns true to stop. Missing required model inputs are
/// stricter failures than upstream's silent placement omission. WDT absence and
/// dummy members retain an explicit gated outcome; ADT absence/dummy is receipted.
/// This function never writes to the install or publishes a partial product.
pub fn prepare(
    data_path: &Path,
    map_id: u32,
    cancelled: impl Fn() -> bool,
) -> Result<PreparedDiscovery, Error> {
    let source =
        CapturedSource::open(data_path, &cancelled).map_err(|cause| Error::Preparation {
            stage: PreparationStage::OpenSource,
            global_root_ordinal: None,
            progress: FailureProgress {
                member_lookups: 0,
                read_work_bytes: 0,
                complete_raw_products: 0,
                adt_attempts: 0,
            },
            cause: Box::new(Error::Source(cause)),
        })?;
    let mut builder = Builder::new(source, map_id, 0);
    let result = prepare_inner(&mut builder, map_id, &cancelled);
    result.map_err(|cause| builder.failure(cause))?;
    builder.finish()
}

fn prepare_inner(
    builder: &mut Builder,
    map_id: u32,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), Error> {
    builder.stage = PreparationStage::LiquidTypes;
    let liquid = required(
        builder
            .source
            .read("DBFilesClient\\LiquidType.dbc", cancelled)?,
    )?;
    discovery::parse_liquid_type_dbc(liquid.bytes())?;
    drop(liquid);
    // ExtractWmo visits each archive's own listfile, in reverse mount order.
    for archive in (0..builder.source.archives().len()).rev() {
        builder.stage = PreparationStage::GlobalWmoLists;
        let names = match builder
            .source
            .read_from_archive(archive, "(listfile)", cancelled)?
        {
            Lookup::Absent => continue,
            Lookup::Dummy { .. } => return Err(Error::RequiredMemberDummy),
            Lookup::Present(bytes) => discovery::parse_wmo_listfile(bytes.bytes())?,
        };
        for dependency in names {
            check_cancel(cancelled)?;
            if builder.summary.global_root_events == MAX_GLOBAL_ROOT_EVENTS {
                return Err(Error::GlobalRootLimit);
            }
            builder.global_root_ordinal = Some(builder.summary.global_root_events);
            builder.summary.global_root_events += 1;
            builder.stage = PreparationStage::GlobalWmoRoot;
            builder.capture(&dependency, cancelled)?;
        }
    }
    builder.global_root_ordinal = None;
    builder.stage = PreparationStage::MapMetadata;
    let map_bytes = required(builder.source.read("DBFilesClient\\Map.dbc", cancelled)?)?;
    let maps = discovery::parse_map_dbc(map_bytes.bytes())?;
    drop(map_bytes);
    let mut selected = maps.iter().enumerate().filter(|(_, map)| map.id == map_id);
    let (selected_ordinal, _) = selected.next().ok_or(Error::MapMissing)?;
    if selected.next().is_some() {
        return Err(Error::DuplicateMap);
    }
    builder.summary.map_record_ordinal = selected_ordinal;
    // Earlier physical rows may seed the M2 flat-file cache used by this map.
    // Rows after the selected row have no effect on the prepared source prefix.
    for (ordinal, map) in maps.iter().enumerate().take(selected_ordinal + 1) {
        let selected = ordinal == selected_ordinal;
        builder.stage = if selected {
            PreparationStage::SelectedWdt
        } else {
            PreparationStage::PrefixMap
        };
        let wdt_member = map.wdt_member().map_err(|error| {
            if selected {
                Error::Discovery(error)
            } else {
                Error::UnsupportedPrefixMapDirectory
            }
        })?;
        if !selected {
            builder.summary.prefix_maps += 1;
        }
        match builder.source.read(&wdt_member, cancelled)? {
            Lookup::Absent => {
                if selected {
                    builder.summary.state = MapDiscoveryState::WdtAbsent;
                }
            }
            Lookup::Dummy { .. } => {
                if selected {
                    builder.summary.state = MapDiscoveryState::WdtDummy;
                }
            }
            Lookup::Present(bytes) => {
                let inputs = discovery::parse_wdt(bytes.bytes())?;
                drop(bytes);
                builder.consume(inputs, Tile { x: 65, y: 65 }, selected, cancelled)?;
                builder.stage = if selected {
                    PreparationStage::AdtTraversal
                } else {
                    PreparationStage::PrefixMap
                };
                for adt in map.adt_members()? {
                    check_cancel(cancelled)?;
                    if selected {
                        builder.summary.adt_attempts += 1;
                    } else {
                        builder.summary.prefix_adt_attempts += 1;
                    }
                    match builder.source.read(&adt.member, cancelled)? {
                        Lookup::Absent => {
                            if selected {
                                builder.summary.adts_absent += 1;
                            }
                        }
                        Lookup::Dummy { .. } => {
                            if selected {
                                builder.summary.adts_dummy += 1;
                            }
                        }
                        Lookup::Present(bytes) => {
                            if selected {
                                builder.summary.adts_present += 1;
                            }
                            let inputs = discovery::parse_adt(bytes.bytes())?;
                            // Parse and drop each ADT before reading the next one.
                            drop(bytes);
                            builder.consume(
                                inputs,
                                Tile { x: adt.x, y: adt.y },
                                selected,
                                cancelled,
                            )?;
                        }
                    }
                }
            }
        }
    }
    builder.stage = PreparationStage::Verification;
    builder.source.verify_unchanged(cancelled)?;
    Ok(())
}

struct Builder {
    source: CapturedSource,
    summary: DiscoverySummary,
    placements: Vec<Placement>,
    products: Vec<RawProduct>,
    names: HashMap<String, usize>,
    doodad_candidates: Vec<DoodadCandidate>,
    stage: PreparationStage,
    global_root_ordinal: Option<usize>,
}
impl Builder {
    fn new(source: CapturedSource, map_id: u32, map_record_ordinal: usize) -> Self {
        Self {
            source,
            summary: DiscoverySummary {
                source_pin: SOURCE_PIN,
                acquisition_policy: source::SOURCE_POLICY,
                listfile_policy: discovery::LISTFILE_POLICY,
                writer_policy: "cmangos-vmaps05-liquid-doodad-dependencies-v3",
                liquid_header_policy: WMO_LIQUID_HEADER_ZERO_PADDING_V1,
                source_epoch: String::new(),
                map_id,
                map_record_ordinal,
                state: MapDiscoveryState::Scanned,
                archive_count: 0,
                member_lookups: 0,
                read_work_bytes: 0,
                adt_attempts: 0,
                adts_present: 0,
                adts_absent: 0,
                adts_dummy: 0,
                dependency_events: 0,
                global_root_events: 0,
                prefix_maps: 0,
                prefix_adt_attempts: 0,
                products: 0,
                m2_raw_products: 0,
                m2_no_collision_products: 0,
                wmo_raw_products: 0,
                wmo_liquid_groups: 0,
                placements: 0,
                emitting_placements: 0,
                no_collision_placements: 0,
                destructible_placements: 0,
                retained_raw_bytes: 0,
                retained_doodad_bytes: 0,
                doodad_extraction_events: 0,
                doodad_candidates: 0,
            },
            placements: Vec::new(),
            products: Vec::new(),
            names: HashMap::new(),
            doodad_candidates: Vec::new(),
            stage: PreparationStage::OpenSource,
            global_root_ordinal: None,
        }
    }
    fn consume(
        &mut self,
        inputs: PlacementInputs,
        tile: Tile,
        keep_placements: bool,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), Error> {
        let mut resolved = Vec::new();
        resolved
            .try_reserve_exact(inputs.dependencies.len())
            .map_err(|_| Error::AllocationFailed)?;
        resolved.resize(inputs.dependencies.len(), None);
        for event in inputs.events {
            check_cancel(cancelled)?;
            match event {
                DiscoveryEvent::ExtractModel(index) | DiscoveryEvent::ReferenceWmo(index) => {
                    if self.summary.dependency_events == MAX_DEPENDENCY_EVENTS {
                        return Err(Error::DependencyLimit);
                    }
                    self.summary.dependency_events += 1;
                    let dependency = inputs
                        .dependencies
                        .get(index)
                        .ok_or(Error::DependencyOrder)?;
                    let product = match event {
                        DiscoveryEvent::ExtractModel(_) => self.capture(dependency, cancelled)?,
                        DiscoveryEvent::ReferenceWmo(_) => {
                            let index = *self
                                .names
                                .get(&dependency.extracted_name.to_ascii_lowercase())
                                .ok_or(Error::UnresolvedWmoReference)?;
                            // The source uses only the flat name; our narrower
                            // identity envelope refuses a conflicting ADT path.
                            if self.products[index].kind != ModelKind::Wmo
                                || !self.products[index]
                                    .member
                                    .eq_ignore_ascii_case(&dependency.member)
                            {
                                return Err(Error::NameCollision);
                            }
                            index
                        }
                        DiscoveryEvent::Placement(_) => unreachable!(),
                    };
                    *resolved.get_mut(index).ok_or(Error::DependencyOrder)? = Some(product);
                }
                DiscoveryEvent::Placement(index) => {
                    if !keep_placements {
                        continue;
                    }
                    let input = inputs.placements.get(index).ok_or(Error::DependencyOrder)?;
                    let dependency = match input {
                        PlacementInput::M2(v) => v.dependency,
                        PlacementInput::Wmo(v) => v.dependency,
                    };
                    let product = resolved
                        .get(dependency)
                        .copied()
                        .flatten()
                        .ok_or(Error::DependencyOrder)?;
                    // WMOInstance checks MODF bit 0 before opening the raw model.
                    let disposition = if matches!(input, PlacementInput::Wmo(v) if v.flags & 1 != 0)
                    {
                        PlacementDisposition::Destructible
                    } else if !self.products[product].source_has_collision() {
                        PlacementDisposition::NoCollision
                    } else {
                        PlacementDisposition::Emits
                    };
                    if self.placements.len() == MAX_PLACEMENTS {
                        return Err(Error::PlacementLimit);
                    }
                    self.placements
                        .try_reserve(1)
                        .map_err(|_| Error::AllocationFailed)?;
                    self.placements.push(Placement {
                        tile,
                        product,
                        input: input.clone(),
                        disposition,
                    });
                    if let PlacementInput::Wmo(wmo) = input {
                        self.select_doodads(
                            self.placements.len() - 1,
                            product,
                            wmo.doodad_set,
                            cancelled,
                        )?;
                    }
                    match disposition {
                        PlacementDisposition::Emits => self.summary.emitting_placements += 1,
                        PlacementDisposition::NoCollision => {
                            self.summary.no_collision_placements += 1
                        }
                        PlacementDisposition::Destructible => {
                            self.summary.destructible_placements += 1
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn select_doodads(
        &mut self,
        placement: usize,
        product: usize,
        set_index: u16,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), Error> {
        let Some(metadata) = &self.products[product].doodads else {
            return Ok(());
        };
        let Some(set) = metadata.root.sets.get(set_index as usize) else {
            return Ok(());
        };
        let end = set
            .start_index
            .checked_add(set.count)
            .ok_or(Error::InvalidDoodadEnvelope)?;
        // This is set membership only. Sorted storage never stands for C++ unordered iteration.
        for &spawn_index in &metadata.references {
            check_cancel(cancelled)?;
            if (spawn_index as u32) < set.start_index || (spawn_index as u32) >= end {
                continue;
            }
            let spawn = &metadata.root.spawns[spawn_index as usize];
            let path = metadata
                .root
                .path_at(spawn.name_index)
                .map_err(Error::Doodad)?;
            let name = discovery::doodad_lookup_name(path)?;
            // ExtractSet opens the flat file now, after later global roots may seed it.
            let Some(&product_index) = self.names.get(&name.to_ascii_lowercase()) else {
                continue;
            };
            if self.products[product_index].extracted_name != name
                || self.products[product_index].kind != ModelKind::M2
            {
                return Err(Error::DoodadNameIdentity);
            }
            if !self.products[product_index].source_has_collision() {
                continue;
            }
            check_doodad_candidate_count(self.doodad_candidates.len())?;
            self.doodad_candidates
                .try_reserve(1)
                .map_err(|_| Error::AllocationFailed)?;
            self.doodad_candidates.push(DoodadCandidate {
                wmo_placement_index: placement,
                spawn_index,
                product_index,
                spawn: spawn.clone(),
                serialized_name_len: path
                    .rsplit(|&b| b == b'\\')
                    .next()
                    .unwrap_or_default()
                    .len() as u32,
            });
        }
        self.summary.doodad_candidates = self.doodad_candidates.len();
        Ok(())
    }
    fn capture(
        &mut self,
        dependency: &ModelDependency,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<usize, Error> {
        let key = dependency.extracted_name.to_ascii_lowercase();
        let existing = self.names.get(&key).copied();
        if let Some(index) = existing {
            let product = &self.products[index];
            if product.kind != dependency.kind
                || !product.member.eq_ignore_ascii_case(&dependency.member)
            {
                return Err(Error::NameCollision);
            }
            // FileExists caches only emitted files; M2::open's zero hull emits none.
            if !matches!(product.collision, Collision::M2EmptyHull) {
                return Ok(index);
            }
        }
        let captured = required(self.source.read(&dependency.member, cancelled)?)?;
        check_cancel(cancelled)?;
        let (collision, doodads) = match dependency.kind {
            ModelKind::M2 => (
                match extract_m2_collision(captured.bytes()).map_err(Error::M2)? {
                    M2Collision::NoCollision => Collision::M2EmptyHull,
                    M2Collision::RawVmap(bytes) => Collision::M2(bytes),
                },
                None,
            ),
            ModelKind::Wmo => {
                let (raw, metadata) = self.capture_wmo(dependency, &captured, cancelled)?;
                (Collision::Wmo(raw), Some(metadata))
            }
        };
        drop(captured);
        check_cancel(cancelled)?;
        if let Some(index) = existing {
            if !matches!(collision, Collision::M2EmptyHull) {
                return Err(Error::SourceChangedDuringPreparation);
            }
            return Ok(index);
        }
        if self.products.len() == MAX_PRODUCTS {
            return Err(Error::ProductLimit);
        }
        let product = RawProduct {
            kind: dependency.kind,
            member: dependency.member.clone(),
            extracted_name: dependency.extracted_name.clone(),
            collision,
            doodads,
        };
        let total = retained_total(
            self.summary.retained_raw_bytes,
            product.raw_vmap().map_or(0, <[u8]>::len),
        )?;
        self.products
            .try_reserve(1)
            .map_err(|_| Error::AllocationFailed)?;
        self.names
            .try_reserve(1)
            .map_err(|_| Error::AllocationFailed)?;
        match product.collision {
            Collision::M2EmptyHull => self.summary.m2_no_collision_products += 1,
            Collision::M2(_) => self.summary.m2_raw_products += 1,
            Collision::Wmo(_) => self.summary.wmo_raw_products += 1,
        }
        let index = self.products.len();
        self.products.push(product);
        self.names.insert(key, index);
        self.summary.retained_raw_bytes = total;
        Ok(index)
    }
    fn capture_wmo(
        &mut self,
        dependency: &ModelDependency,
        root: &CapturedMember,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(WmoCollision, CapturedDoodads), Error> {
        let mut metadata = doodads::parse_root_doodads(root.bytes()).map_err(Error::Doodad)?;
        let count = metadata.group_count;
        let mut valid_names = BTreeSet::new();
        // WMORoot::open extracts every MODN name before opening any group.
        for event in &metadata.events {
            check_cancel(cancelled)?;
            if self.summary.dependency_events == MAX_DEPENDENCY_EVENTS {
                return Err(Error::DependencyLimit);
            }
            self.summary.dependency_events += 1;
            self.summary.doodad_extraction_events += 1;
            let Some(model) = discovery::doodad_model_dependency(&event.path)? else {
                continue;
            };
            if let Some(&index) = self.names.get(&model.extracted_name.to_ascii_lowercase()) {
                if self.products[index].source_has_collision()
                    && self.products[index].extracted_name != model.extracted_name
                {
                    return Err(Error::DoodadNameIdentity);
                }
            }
            let index = self.capture(&model, cancelled)?;
            if self.products[index].source_has_collision() {
                if model.extracted_name != discovery::doodad_lookup_name(&event.path)? {
                    return Err(Error::DoodadNameIdentity);
                }
                valid_names.insert(event.name_index);
            }
        }
        if count > MAX_WMO_GROUPS {
            return Err(Error::Wmo(benilla_native_vmap::WmoError::GroupCountLimit {
                count,
                maximum: MAX_WMO_GROUPS,
            }));
        }
        let stem = dependency
            .member
            .get(
                ..dependency
                    .member
                    .len()
                    .checked_sub(4)
                    .ok_or(Error::InvalidWmoEnvelope)?,
            )
            .ok_or(Error::InvalidWmoEnvelope)?;
        let mut groups = Vec::new();
        groups
            .try_reserve_exact(count as usize)
            .map_err(|_| Error::AllocationFailed)?;
        let mut bytes = root.bytes().len();
        for group in 0..count {
            check_cancel(cancelled)?;
            let captured = required(
                self.source
                    .read(&format!("{stem}_{group:03}.wmo"), cancelled)?,
            )?;
            bytes = bytes
                .checked_add(captured.bytes().len())
                .filter(|&bytes| bytes <= MAX_WMO_TOTAL_BYTES)
                .ok_or(Error::WmoInputLimit)?;
            groups.push(captured);
        }
        let mut inputs = Vec::new();
        inputs
            .try_reserve_exact(groups.len())
            .map_err(|_| Error::AllocationFailed)?;
        for (index, group) in groups.iter().enumerate() {
            inputs.push(WmoGroupInput {
                group_index: index as u32,
                bytes: group.bytes(),
            });
        }
        // Only complete captured root/group closure reaches the existing writer.
        let collision =
            extract_wmo_collision_from_source(&dependency.member, root.bytes(), &inputs)
                .map_err(Error::Wmo)?;
        self.summary.wmo_liquid_groups += collision.liquid_group_count as usize;
        let mut references = BTreeSet::new();
        for (index, group) in groups.iter().enumerate() {
            check_cancel(cancelled)?;
            let group_references =
                doodads::parse_group_doodad_references(group.bytes()).map_err(Error::Doodad)?;
            if !collision.retained_group_indices.contains(&(index as u32)) {
                continue;
            }
            for reference in group_references {
                check_cancel(cancelled)?;
                let Some(spawn) = metadata.spawns.get(reference as usize) else {
                    continue;
                };
                if valid_names.contains(&spawn.name_index) {
                    references.insert(reference);
                }
            }
        }
        let mut resolved = Vec::new();
        resolved
            .try_reserve_exact(references.len())
            .map_err(|_| Error::AllocationFailed)?;
        resolved.extend(references);
        let retained = retained_doodad_total(
            self.summary.retained_doodad_bytes,
            metadata.sets.len(),
            metadata.spawns.len(),
            resolved.len(),
            metadata.paths.len(),
        )?;
        self.summary.retained_doodad_bytes = retained;
        metadata.events = Vec::new();
        metadata.names = Vec::new();
        Ok((
            collision,
            CapturedDoodads {
                root: metadata,
                references: resolved,
            },
        ))
    }
    fn failure(&self, cause: Error) -> Error {
        Error::Preparation {
            stage: self.stage,
            global_root_ordinal: self.global_root_ordinal,
            progress: FailureProgress {
                member_lookups: self.source.read_count(),
                read_work_bytes: self.source.read_work_bytes(),
                complete_raw_products: self.products.len(),
                adt_attempts: self.summary.adt_attempts + self.summary.prefix_adt_attempts,
            },
            cause: Box::new(cause),
        }
    }
    fn finish(mut self) -> Result<PreparedDiscovery, Error> {
        self.summary.source_epoch = self.source.binding_digest();
        self.summary.archive_count = self.source.archives().len();
        self.summary.member_lookups = self.source.read_count();
        self.summary.read_work_bytes = self.source.read_work_bytes();
        self.summary.products = self.products.len();
        self.summary.placements = self.placements.len();
        Ok(PreparedDiscovery {
            source: self.source,
            summary: self.summary,
            placements: self.placements,
            products: self.products,
            doodad_candidates: self.doodad_candidates,
        })
    }
}

fn check_doodad_candidate_count(count: usize) -> Result<(), Error> {
    if count >= MAX_DOODAD_CANDIDATES {
        Err(Error::DoodadBudget)
    } else {
        Ok(())
    }
}
fn retained_doodad_total(
    current: usize,
    sets: usize,
    spawns: usize,
    references: usize,
    paths: usize,
) -> Result<usize, Error> {
    sets.checked_mul(32)
        .and_then(|n| spawns.checked_mul(40).and_then(|m| n.checked_add(m)))
        .and_then(|n| references.checked_mul(2).and_then(|m| n.checked_add(m)))
        .and_then(|n| n.checked_add(paths))
        .and_then(|n| current.checked_add(n))
        .filter(|&n| n <= MAX_RETAINED_DOODAD_BYTES)
        .ok_or(Error::DoodadBudget)
}

fn retained_total(current: usize, additional: usize) -> Result<usize, Error> {
    current
        .checked_add(additional)
        .filter(|&total| total <= MAX_RETAINED_RAW_BYTES)
        .ok_or(Error::RetainedOutputLimit)
}

fn check_cancel(cancelled: &dyn Fn() -> bool) -> Result<(), Error> {
    if cancelled() {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
fn required(lookup: Lookup) -> Result<CapturedMember, Error> {
    match lookup {
        Lookup::Present(bytes) => Ok(bytes),
        Lookup::Absent => Err(Error::RequiredMemberAbsent),
        Lookup::Dummy { .. } => Err(Error::RequiredMemberDummy),
    }
}

#[cfg(test)]
mod tests;
