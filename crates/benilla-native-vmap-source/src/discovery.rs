// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded placement inputs, not assembled VMap instances.
//!
//! Source: CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f,
//! contrib/vmap_extractor/vmapextract/{vmapexport,wdtfile,adtfile,model,wmo}.cpp
//! and gameobject_extract.cpp::ExtractSingleModel. ParsMapFiles consumes Map.dbc
//! in physical row order, opens each WDT, then attempts all ADTs with x outermost.
//! WDT MAIN and ADT MMID/MWID do not select tiles or resolve name references here.
//! The source's actual MDDF/MODF IDs index the accumulated name arrays directly.
//!
//! The stricter envelope requires version 18, whole chunks/records, safe ASCII
//! member paths, nonempty name-table entries and finite numeric inputs. Empty
//! entries (including extra NUL padding) fail rather than being removed, because
//! removing one would change subsequent source name-array ordinals. This profile
//! can refuse otherwise harmless unreferenced empty names. It rejects global WMO input
//! rather than omitting it. WMO doodad expansion remains a coordinator obligation:
//! retaining a MODF is never proof that its selected doodad set is empty.
//! All transforms, unique IDs and MODF bounds below are original input values.
//! No GenerateUniqueObjectId call, raw-collision-dependent filtering, coordinate
//! permutation, worldspawn origin adjustment, or mapassembler bound is invented.

use anyhow::{ensure, Context, Result as InternalResult};
use std::collections::BTreeSet;
use std::fmt;

/// A fixed parser diagnostic; its text never contains an archive path or asset name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscoveryError {
    code: &'static str,
}

impl DiscoveryError {
    const fn new(code: &'static str) -> Self {
        Self { code }
    }

    pub fn code(self) -> &'static str {
        self.code
    }

    fn from_internal(error: anyhow::Error) -> Self {
        error
            .downcast_ref::<Self>()
            .copied()
            .unwrap_or(Self::new("vmap.discovery_internal"))
    }
}

impl fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code)
    }
}

impl std::error::Error for DiscoveryError {}

const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;
const MAX_MAP_ROWS: usize = 100_000;
const MAX_CHUNKS: usize = 100_000;
const MAX_NAMES: usize = 100_000;
const MAX_LISTFILE_ROOTS: usize = 32_768;
const MAX_PLACEMENTS: usize = 100_000;
const MAX_MEMBER_BYTES: usize = 511;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct MapInput {
    pub(crate) id: u32,
    /// Raw metadata. It becomes a path only through the checked member methods.
    pub(crate) directory: String,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct AdtMember {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) member: String,
}

impl MapInput {
    pub(crate) fn wdt_member(&self) -> Result<String, DiscoveryError> {
        self.check_directory()
            .map_err(DiscoveryError::from_internal)?;
        Ok(format!("World\\Maps\\{0}\\{0}.wdt", self.directory))
    }

    /// Call only after the corresponding WDT has been captured and parsed.
    /// Source WDT presence gates traversal; MAIN flags do not gate individual ADTs.
    pub(crate) fn adt_members(
        &self,
    ) -> Result<impl Iterator<Item = AdtMember> + '_, DiscoveryError> {
        self.check_directory()
            .map_err(DiscoveryError::from_internal)?;
        Ok((0..64).flat_map(move |x| {
            (0..64).map(move |y| AdtMember {
                x,
                y,
                member: format!("World\\Maps\\{0}\\{0}_{x}_{y}.adt", self.directory),
            })
        }))
    }

    fn check_directory(&self) -> InternalResult<()> {
        ensure!(
            !self.directory.is_empty()
                && self.directory.len() < 64
                && self
                    .directory
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')),
            DiscoveryError::new("vmap.map_directory")
        );
        Ok(())
    }
}

/// Read all metadata without treating unvisited map names as filesystem paths.
/// Field count/stride are the Classic build-5875 Map.dbc profile, not an inferred
/// schema. Invalid unused metadata still fails; unsafe unvisited paths do not.
pub(crate) fn parse_map_dbc(bytes: &[u8]) -> Result<Vec<MapInput>, DiscoveryError> {
    parse_map_dbc_inner(bytes).map_err(DiscoveryError::from_internal)
}

fn parse_map_dbc_inner(bytes: &[u8]) -> InternalResult<Vec<MapInput>> {
    ensure!(
        (20..=MAX_FILE_BYTES).contains(&bytes.len()),
        DiscoveryError::new("vmap.map_dbc_bytes")
    );
    ensure!(
        &bytes[..4] == b"WDBC",
        DiscoveryError::new("vmap.map_dbc_magic")
    );
    let count = word(bytes, 4) as usize;
    ensure!(
        count <= MAX_MAP_ROWS,
        DiscoveryError::new("vmap.map_dbc_rows")
    );
    ensure!(
        word(bytes, 8) == 42 && word(bytes, 12) == 168,
        DiscoveryError::new("vmap.map_dbc_layout")
    );
    let string_start = count
        .checked_mul(168)
        .and_then(|n| n.checked_add(20))
        .context(DiscoveryError::new("vmap.map_dbc_length"))?;
    let end = string_start
        .checked_add(word(bytes, 16) as usize)
        .context(DiscoveryError::new("vmap.map_dbc_length"))?;
    ensure!(
        end == bytes.len(),
        DiscoveryError::new("vmap.map_dbc_length")
    );
    let strings = &bytes[string_start..];
    ensure!(
        strings.first() == Some(&0) && strings.last() == Some(&0),
        DiscoveryError::new("vmap.map_dbc_strings")
    );
    let mut ids = BTreeSet::new();
    let mut maps = Vec::with_capacity(count);
    for row in bytes[20..string_start].as_chunks::<168>().0 {
        let id = word(row, 0);
        ensure!(ids.insert(id), DiscoveryError::new("vmap.duplicate_map_id"));
        let tail = strings
            .get(word(row, 4) as usize..)
            .context(DiscoveryError::new("vmap.map_dbc_string_reference"))?;
        let end = tail
            .iter()
            .position(|&b| b == 0)
            .context(DiscoveryError::new("vmap.map_dbc_string_nul"))?;
        ensure!(end < 64, DiscoveryError::new("vmap.map_metadata_length"));
        let directory = std::str::from_utf8(&tail[..end])
            .context(DiscoveryError::new("vmap.map_metadata_utf8"))?;
        maps.push(MapInput {
            id,
            directory: directory.to_owned(),
        });
    }
    Ok(maps)
}

/// Explicit source-compatibility boundary: preserve each captured byte and add
/// only a logical terminal NUL for GetFileListTo's otherwise undefined suffix read.
pub(crate) const LISTFILE_POLICY: &str = "cmangos-vmap-listfile-logical-nul-v1";

/// GetFileListTo token order, then ExtractWmo's case-sensitive substring selection
/// and ExtractSingleWmo's normalization/group-name filter. Duplicates are retained
/// for the coordinator's source-ordered flat-output cache decisions.
pub(crate) fn parse_wmo_listfile(bytes: &[u8]) -> Result<Vec<ModelDependency>, DiscoveryError> {
    parse_wmo_listfile_inner(bytes).map_err(DiscoveryError::from_internal)
}

/// The returned slices omit the byte removed by the pinned source. Embedded NUL
/// ends tokenization, including when additional captured bytes follow it.
pub(crate) fn tokenize_listfile(bytes: &[u8]) -> Result<Vec<&[u8]>, DiscoveryError> {
    tokenize_listfile_inner(bytes).map_err(DiscoveryError::from_internal)
}

fn tokenize_listfile_inner(bytes: &[u8]) -> InternalResult<Vec<&[u8]>> {
    ensure!(
        bytes.len() <= MAX_FILE_BYTES,
        DiscoveryError::new("vmap.listfile_bytes")
    );
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let mut cursor = 0;
    let mut counter = 0;
    let mut tokens = Vec::new();
    while cursor < end && counter < bytes.len() {
        // strtok skips all LF delimiters, including leading and repeated ones.
        while cursor < end && bytes[cursor] == b'\n' {
            cursor += 1;
        }
        if cursor == end {
            break;
        }
        ensure!(
            tokens.len() < 1_000_000,
            DiscoveryError::new("vmap.listfile_token_count")
        );
        let start = cursor;
        while cursor < end && bytes[cursor] != b'\n' {
            cursor += 1;
        }
        // A nonempty strtok token loses its final byte unconditionally. In
        // particular LF-only input loses a filename byte; it is not repaired.
        let token = &bytes[start..cursor - 1];
        counter += token.len() + 2;
        tokens.push(token);
    }
    Ok(tokens)
}

fn parse_wmo_listfile_inner(bytes: &[u8]) -> InternalResult<Vec<ModelDependency>> {
    let mut dependencies = Vec::new();
    for token in tokenize_listfile(bytes)? {
        if !token.windows(4).any(|window| window == b".wmo") {
            continue;
        }
        safe_member(token)?;
        let mut member = token.to_vec();
        let plain = member
            .iter()
            .rposition(|&b| b == b'\\')
            .map_or(0, |i| i + 1);
        ensure!(
            member.len() - plain >= 4,
            DiscoveryError::new("vmap.listfile_root_name")
        );
        fix_name(&mut member[plain..]);
        fix_spaces(&mut member[plain..]);
        safe_member(&member)?;
        let basename = &member[plain..];
        // strncpy(cpy, last_underscore, 4) examines underscore plus three bytes,
        // with zero padding when shorter. Exactly three ASCII digits skips it.
        if let Some(underscore) = basename.iter().rposition(|&b| b == b'_') {
            let stop = (underscore + 4).min(basename.len());
            if basename[underscore..stop]
                .iter()
                .filter(|b| b.is_ascii_digit())
                .count()
                == 3
            {
                continue;
            }
        }
        // The source accepts any .wmo substring, then removes the final four
        // path bytes to derive groups. That wider extension profile is refused,
        // never silently filtered as though the source had ignored the entry.
        ensure!(
            member.ends_with(b".wmo"),
            DiscoveryError::new("vmap.listfile_root_extension")
        );
        ensure!(
            dependencies.len() < MAX_LISTFILE_ROOTS,
            DiscoveryError::new("vmap.listfile_root_count")
        );
        dependencies.push(ModelDependency {
            kind: ModelKind::Wmo,
            source_member: String::from_utf8(token.to_vec()).expect("ASCII listfile candidate"),
            extracted_name: String::from_utf8(member[plain..].to_vec()).expect("ASCII WMO name"),
            member: String::from_utf8(member).expect("ASCII WMO member"),
        });
    }
    Ok(dependencies)
}

/// ReadLiquidTypeTableDBC obtains its allocation bound from the *last physical
/// row*, then indexes it with every row ID. No sorted/max-ID replacement is used.
/// Classification field 3 is a uint32 converted to uint16 by the source; no
/// classification or duplicate-ID constraint is invented for this dry boundary.
pub(crate) fn parse_liquid_type_dbc(bytes: &[u8]) -> Result<(), DiscoveryError> {
    parse_liquid_type_dbc_inner(bytes).map_err(DiscoveryError::from_internal)
}

fn parse_liquid_type_dbc_inner(bytes: &[u8]) -> InternalResult<()> {
    ensure!(
        (20..=MAX_FILE_BYTES).contains(&bytes.len()),
        DiscoveryError::new("vmap.liquid_dbc_bytes")
    );
    ensure!(
        &bytes[..4] == b"WDBC",
        DiscoveryError::new("vmap.liquid_dbc_magic")
    );
    let count = word(bytes, 4) as usize;
    let fields = word(bytes, 8) as usize;
    let stride = word(bytes, 12) as usize;
    ensure!(
        (1..=MAX_MAP_ROWS).contains(&count),
        DiscoveryError::new("vmap.liquid_dbc_rows")
    );
    ensure!(
        (4..=4096).contains(&fields) && fields * 4 == stride,
        DiscoveryError::new("vmap.liquid_dbc_layout")
    );
    let string_start = count
        .checked_mul(stride)
        .and_then(|n| n.checked_add(20))
        .context(DiscoveryError::new("vmap.liquid_dbc_length"))?;
    let end = string_start
        .checked_add(word(bytes, 16) as usize)
        .context(DiscoveryError::new("vmap.liquid_dbc_length"))?;
    ensure!(
        end == bytes.len(),
        DiscoveryError::new("vmap.liquid_dbc_length")
    );
    let records = &bytes[20..string_start];
    let max_id = word(records, (count - 1) * stride);
    // At most 2,000,002 source array bytes; no huge source allocation is implied.
    ensure!(
        max_id <= 1_000_000,
        DiscoveryError::new("vmap.liquid_id_limit")
    );
    for record in records.chunks_exact(stride) {
        ensure!(
            word(record, 0) <= max_id,
            DiscoveryError::new("vmap.liquid_last_row_not_max")
        );
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelKind {
    M2,
    Wmo,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ModelDependency {
    pub(crate) kind: ModelKind,
    /// Original complete name-table entry, before source fixnamen/fixname2.
    pub(crate) source_member: String,
    /// M2: exact source lookup name, including .mdx/.mdl -> .m2.
    /// WMO: only a candidate root path using the normalized output basename;
    /// source MWMO performs no archive lookup. WMO names require the coordinator's
    /// output-alias proof because the upstream extractor discovers WMO roots
    /// from archive lists before map scans.
    pub(crate) member: String,
    pub(crate) extracted_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct M2Placement {
    pub dependency: usize,
    pub name_index: u32,
    pub unique_id: u32,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    /// Raw uint16; ModelInstance derives its factor with scale / 1024.0f.
    pub scale: u16,
    pub flags: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WmoPlacement {
    pub dependency: usize,
    pub name_index: u32,
    pub unique_id: u32,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    /// Raw MODF bounds, before fixCoords. These are not assembled-model bounds.
    pub bounds: [[f32; 3]; 2],
    pub flags: u16,
    pub doodad_set: u16,
    pub name_set: u16,
    /// The pinned WMO writer reads and ignores this word, emitting scale 1.0.
    pub scale: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlacementInput {
    M2(M2Placement),
    Wmo(WmoPlacement),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiscoveryEvent {
    ExtractModel(usize),
    ReferenceWmo(usize),
    Placement(usize),
}

#[derive(Default, Clone, PartialEq)]
pub(crate) struct PlacementInputs {
    /// Every name, including unreferenced and duplicate entries, in source order.
    pub(crate) dependencies: Vec<ModelDependency>,
    pub(crate) placements: Vec<PlacementInput>,
    /// Indices preserve the interleaving of table and placement chunks/records.
    pub(crate) events: Vec<DiscoveryEvent>,
}

pub(crate) fn parse_wdt(bytes: &[u8]) -> Result<PlacementInputs, DiscoveryError> {
    parse_placements(bytes, true).map_err(DiscoveryError::from_internal)
}

pub(crate) fn parse_adt(bytes: &[u8]) -> Result<PlacementInputs, DiscoveryError> {
    parse_placements(bytes, false).map_err(DiscoveryError::from_internal)
}

fn parse_placements(bytes: &[u8], global: bool) -> InternalResult<PlacementInputs> {
    ensure!(
        (12..=MAX_FILE_BYTES).contains(&bytes.len()),
        DiscoveryError::new("vmap.placement_bytes")
    );
    let mut input = PlacementInputs::default();
    let mut models = Vec::new();
    let mut wmos = Vec::new();
    let mut at = 0;
    let mut chunks = 0;
    while at < bytes.len() {
        chunks += 1;
        ensure!(
            chunks <= MAX_CHUNKS,
            DiscoveryError::new("vmap.chunk_count")
        );
        let header_end = at
            .checked_add(8)
            .context(DiscoveryError::new("vmap.chunk_length"))?;
        let header = bytes
            .get(at..header_end)
            .context(DiscoveryError::new("vmap.chunk_header"))?;
        let end = header_end
            .checked_add(word(header, 4) as usize)
            .context(DiscoveryError::new("vmap.chunk_length"))?;
        let data = bytes
            .get(header_end..end)
            .context(DiscoveryError::new("vmap.chunk_payload"))?;
        let tag = &header[..4];
        // Actual wire fourcc bytes are reversed; source flipcc reverses them.
        if chunks == 1 {
            ensure!(tag == b"REVM", DiscoveryError::new("vmap.missing_version"));
        }
        match tag {
            b"REVM" => {
                ensure!(chunks == 1, DiscoveryError::new("vmap.duplicate_version"));
                ensure!(
                    data.len() == 4 && word(data, 0) == 18,
                    DiscoveryError::new("vmap.version")
                );
            }
            b"XDMM" if !global => {
                parse_names(data, ModelKind::M2, false, &mut models, &mut input)?;
            }
            b"OMWM" => {
                parse_names(data, ModelKind::Wmo, global, &mut wmos, &mut input)?;
            }
            b"FDDM" if !global => {
                ensure!(
                    data.len().is_multiple_of(36),
                    DiscoveryError::new("vmap.mddf_record_length")
                );
                for record in data.as_chunks::<36>().0 {
                    check_placement_count(&input)?;
                    let name_index = word(record, 0);
                    let dependency = *models
                        .get(name_index as usize)
                        .context(DiscoveryError::new("vmap.mddf_name_reference"))?;
                    let placement = M2Placement {
                        dependency,
                        name_index,
                        unique_id: word(record, 4),
                        position: vector(record, 8)?,
                        rotation: vector(record, 20)?,
                        scale: half(record, 32),
                        flags: half(record, 34),
                    };
                    input
                        .events
                        .push(DiscoveryEvent::Placement(input.placements.len()));
                    input.placements.push(PlacementInput::M2(placement));
                }
            }
            b"FDOM" => {
                ensure!(
                    data.len().is_multiple_of(64),
                    DiscoveryError::new("vmap.modf_record_length")
                );
                for record in data.as_chunks::<64>().0 {
                    check_placement_count(&input)?;
                    let name_index = word(record, 0);
                    let dependency = *wmos
                        .get(name_index as usize)
                        .context(DiscoveryError::new("vmap.modf_name_reference"))?;
                    let bounds = [vector(record, 32)?, vector(record, 44)?];
                    ensure!(
                        (0..3).all(|axis| bounds[0][axis] <= bounds[1][axis]),
                        DiscoveryError::new("vmap.modf_bounds_order")
                    );
                    let placement = WmoPlacement {
                        dependency,
                        name_index,
                        unique_id: word(record, 4),
                        position: vector(record, 8)?,
                        rotation: vector(record, 20)?,
                        bounds,
                        flags: half(record, 56),
                        doodad_set: half(record, 58),
                        name_set: half(record, 60),
                        scale: half(record, 62),
                    };
                    ensure!(!global, DiscoveryError::new("vmap.unsupported_global_wmo"));
                    input
                        .events
                        .push(DiscoveryEvent::Placement(input.placements.len()));
                    input.placements.push(PlacementInput::Wmo(placement));
                }
            }
            // MAIN, MMID, MWID and other complete chunks are not consumed by the
            // pinned placement extractor. In particular MAIN never filters ADTs.
            _ => {}
        }
        at = end;
    }
    Ok(input)
}

fn check_placement_count(input: &PlacementInputs) -> InternalResult<()> {
    ensure!(
        input.placements.len() < MAX_PLACEMENTS,
        DiscoveryError::new("vmap.placement_count")
    );
    Ok(())
}

fn parse_names(
    bytes: &[u8],
    kind: ModelKind,
    global: bool,
    names: &mut Vec<usize>,
    input: &mut PlacementInputs,
) -> InternalResult<()> {
    let mut at = 0;
    while at < bytes.len() {
        ensure!(
            input.dependencies.len() < MAX_NAMES,
            DiscoveryError::new("vmap.name_count")
        );
        let tail = &bytes[at..];
        let length = tail
            .iter()
            .position(|&b| b == 0)
            .context(DiscoveryError::new("vmap.name_nul"))?;
        let source_member = &tail[..length];
        safe_member(source_member)?;
        let mut member = source_member.to_vec();
        let plain = member
            .iter()
            .rposition(|&b| b == b'\\')
            .map_or(0, |at| at + 1);
        match kind {
            ModelKind::M2 => {
                fix_name(&mut member);
                fix_spaces(&mut member[plain..]);
                if member.ends_with(b".mdx") || member.ends_with(b".mdl") {
                    member.truncate(member.len() - 2);
                    member.push(b'2');
                }
                ensure!(
                    member.ends_with(b".m2"),
                    DiscoveryError::new("vmap.unsupported_model_extension")
                );
            }
            ModelKind::Wmo => {
                fix_name(&mut member[plain..]);
                if !global {
                    fix_spaces(&mut member[plain..]);
                }
                ensure!(
                    member.ends_with(b".wmo"),
                    DiscoveryError::new("vmap.unsupported_wmo_extension")
                );
            }
        }
        // All accepted bytes are ASCII; normalization only changes ASCII bytes.
        let extracted_name =
            String::from_utf8(member[plain..].to_vec()).expect("ASCII member normalization");
        let dependency = ModelDependency {
            kind,
            source_member: String::from_utf8(source_member.to_vec()).expect("ASCII member"),
            member: String::from_utf8(member).expect("ASCII member normalization"),
            extracted_name,
        };
        let index = input.dependencies.len();
        names.push(index);
        input.dependencies.push(dependency);
        input.events.push(match kind {
            ModelKind::M2 => DiscoveryEvent::ExtractModel(index),
            ModelKind::Wmo => DiscoveryEvent::ReferenceWmo(index),
        });
        at += length + 1;
    }
    Ok(())
}

/// WMORoot copies the original MODN path before mutating its MPQ buffer.
/// ExtractSingleModel rewrites only four exact extension spellings here.
pub(crate) fn doodad_model_dependency(
    raw: &[u8],
) -> Result<Option<ModelDependency>, DiscoveryError> {
    if raw.len() < 4 {
        return Ok(None);
    }
    safe_member(raw).map_err(DiscoveryError::from_internal)?;
    let mut member = raw.to_vec();
    if [b".mdx", b".MDX", b".mdl", b".MDL"]
        .iter()
        .any(|extension| member.ends_with(*extension))
    {
        member.truncate(member.len() - 2);
        member.push(b'2');
    }
    if !member.ends_with(b".m2") && !member.ends_with(b".M2") {
        return Err(DiscoveryError::new("vmap.unsupported_model_extension"));
    }
    let plain = member
        .iter()
        .rposition(|&b| b == b'\\')
        .map_or(0, |n| n + 1);
    Ok(Some(ModelDependency {
        kind: ModelKind::M2,
        source_member: String::from_utf8(raw.to_vec()).expect("ASCII member"),
        extracted_name: String::from_utf8(member[plain..].to_vec()).expect("ASCII basename"),
        member: String::from_utf8(member).expect("ASCII member"),
    }))
}

/// Later ExtractSet normalizes its retained original basename independently.
pub(crate) fn doodad_lookup_name(raw: &[u8]) -> Result<String, DiscoveryError> {
    safe_member(raw).map_err(DiscoveryError::from_internal)?;
    let plain = raw.iter().rposition(|&b| b == b'\\').map_or(0, |n| n + 1);
    let mut name = raw[plain..].to_vec();
    fix_name(&mut name);
    fix_spaces(&mut name);
    if name.ends_with(b".mdx") || name.ends_with(b".mdl") {
        name.truncate(name.len() - 2);
        name.push(b'2');
    }
    Ok(String::from_utf8(name).expect("ASCII basename"))
}

fn safe_member(bytes: &[u8]) -> InternalResult<()> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_MEMBER_BYTES,
        DiscoveryError::new("vmap.member_name_length")
    );
    ensure!(
        bytes.iter().all(|&b| b.is_ascii_alphanumeric()
            || matches!(b, b'\\' | b'_' | b'-' | b'.' | b' ' | b'(' | b')')),
        DiscoveryError::new("vmap.member_name_profile")
    );
    ensure!(
        bytes.split(|&b| b == b'\\').all(|part| !part.is_empty()
            && part != b"."
            && part != b".."
            && part.first() != Some(&b' ')
            && !matches!(part.last(), Some(b' ' | b'.'))),
        DiscoveryError::new("vmap.member_path")
    );
    Ok(())
}

/// adtfile.cpp::fixnamen uses the already-mutated previous character. ASCII
/// restriction makes C isalpha deterministic; the last three bytes are OR 0x20.
fn fix_name(bytes: &mut [u8]) {
    if bytes.len() < 3 {
        return;
    }
    let split = bytes.len() - 3;
    for at in 0..split {
        let previous_is_alpha = at > 0 && bytes[at - 1].is_ascii_alphabetic();
        if previous_is_alpha && bytes[at].is_ascii_uppercase() {
            bytes[at] |= 0x20;
        } else if !previous_is_alpha && bytes[at].is_ascii_lowercase() {
            bytes[at] &= !0x20;
        }
    }
    for byte in &mut bytes[split..] {
        *byte |= 0x20;
    }
}

fn fix_spaces(bytes: &mut [u8]) {
    if bytes.len() >= 3 {
        let split = bytes.len() - 3;
        for byte in &mut bytes[..split] {
            if *byte == b' ' {
                *byte = b'_';
            }
        }
    }
}

fn vector(bytes: &[u8], at: usize) -> InternalResult<[f32; 3]> {
    let values = [
        f32::from_bits(word(bytes, at)),
        f32::from_bits(word(bytes, at + 4)),
        f32::from_bits(word(bytes, at + 8)),
    ];
    ensure!(
        values.iter().all(|n| n.is_finite()),
        DiscoveryError::new("vmap.nonfinite_transform")
    );
    Ok(values)
}

fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().expect("bounded word"))
}

fn half(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(bytes[at..at + 2].try_into().expect("bounded halfword"))
}

#[cfg(test)]
#[path = "discovery_tests.rs"]
mod tests;
