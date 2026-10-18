// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only, bounded capture of the exact terrain inputs selected by the writer.
//!
//! Captured buffers are immutable and retain every original member byte, including
//! bytes the terrain parser does not use. A fresh-chain recheck detects relevant
//! changes observed by that recheck. Neither capture nor recheck locks the install,
//! provides an atomic filesystem snapshot, or protects against hostile ABA changes
//! during/between reads. No filesystem-wide or client-build authenticity is claimed.

use anyhow::{ensure, Context, Result};
use benilla_formats::{Chain, ChainLimits};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub(crate) const MAX_MEMBER_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const MAX_CAPTURE_BYTES: usize = 192 * 1024 * 1024;
pub(crate) const MAX_ADT_MEMBERS: usize = 9;
pub(crate) const MAX_CAPTURE_MEMBERS: usize = 3 + 1 + MAX_ADT_MEMBERS;
const MAX_MEMBER_NAME_BYTES: usize = 192;
const MAX_HEADER_SCAN_BYTES: u64 = 1024 * 1024;
/// Conservative intersection of app and pinned extractor source resolution.
pub(crate) const SOURCE_POLICY: &str = "benilla-chain-and-cmangos-ad-fixed-five-exact-winner-v2";
const AD_ARCHIVES: &[&str] = &[
    "model.MPQ",
    "dbc.MPQ",
    "terrain.MPQ",
    "patch.MPQ",
    "patch-2.MPQ",
];
const DBC_MEMBERS: &[&str] = &[
    "DBFilesClient\\Map.dbc",
    "DBFilesClient\\AreaTable.dbc",
    "DBFilesClient\\LiquidType.dbc",
];

/// Informational provenance derived only from a successfully read member.
/// This is not a deserializable snapshot, admission token, or parser result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SourceMember {
    pub(crate) name: String,
    pub(crate) winning_archive: String,
    pub(crate) sha256: String,
    pub(crate) bytes: usize,
}

/// There is no constructor from metadata, digest, serialized output, or borrowed
/// bytes. Only capture can mint this value; callers only borrow its payloads.
pub(crate) struct CapturedSource {
    data: PathBuf,
    members: Vec<SourceMember>,
    payloads: Vec<Box<[u8]>>,
    binding_digest: String,
}

impl fmt::Debug for CapturedSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CapturedSource")
            .field("members", &self.members)
            .field("binding_digest", &self.binding_digest)
            .finish_non_exhaustive()
    }
}

impl CapturedSource {
    /// Capture an explicit subset of the three terrain DBCs, one WDT, and at most
    /// nine ADTs, all from a single map. The coordinator must derive terrain names
    /// from validated Map/WDT data; this function only enforces the input envelope.
    /// Smaller subsets support discovery, but cannot themselves establish a complete
    /// terrain product. Names use canonical `DBFilesClient\\` / `World\\Maps\\` paths.
    pub(crate) fn capture(data: &Path, names: &[impl AsRef<str>]) -> Result<Self> {
        let names = selected_names(names)?;
        ensure!(data.is_dir(), "source.data_directory_required");
        let data = data.canonicalize().context("source.data_directory")?;
        let chain = open_chain(&data)?;
        // Inspect all winning sizes before any member read/decompression.
        let plans = plan_members(&chain, &names)?;
        let mut members = Vec::with_capacity(plans.len());
        let mut payloads = Vec::with_capacity(plans.len());
        let mut binding = Sha256::new();
        binding.update(b"benilla-native-terrain-source-v1\0");
        hash_field(&mut binding, SOURCE_POLICY.as_bytes());
        hash_field(&mut binding, crate::SOURCE_PIN.as_bytes());
        binding.update((plans.len() as u64).to_le_bytes());
        for plan in plans {
            let bytes = read_member(&chain, &plan)?;
            hash_field(&mut binding, plan.name.as_bytes());
            hash_field(&mut binding, plan.winning_archive.as_bytes());
            hash_field(&mut binding, &bytes);
            members.push(SourceMember {
                name: plan.name,
                winning_archive: plan.winning_archive,
                sha256: format!("{:x}", Sha256::digest(&bytes)),
                bytes: bytes.len(),
            });
            payloads.push(bytes.into_boxed_slice());
        }
        let captured = Self {
            data,
            members,
            payloads,
            binding_digest: format!("{:x}", binding.finalize()),
        };
        captured.verify_ad_compatible()?;
        Ok(captured)
    }

    pub(crate) fn bytes(&self, name: &str) -> Result<&[u8]> {
        let index = self
            .members
            .binary_search_by(|member| member.name.as_str().cmp(name))
            .ok()
            .with_context(|| format!("source.not_captured:{name}"))?;
        Ok(&self.payloads[index])
    }

    pub(crate) fn members(&self) -> &[SourceMember] {
        &self.members
    }

    pub(crate) fn binding_digest(&self) -> &str {
        &self.binding_digest
    }

    /// Keep staged Map/WDT discovery bound to the later, complete capture.
    pub(crate) fn verify_contains(&self, prior: &Self) -> Result<()> {
        ensure!(
            self.data == prior.data,
            "source.discovery_directory_changed"
        );
        for (member, bytes) in prior.members.iter().zip(&prior.payloads) {
            let index = self
                .members
                .binary_search_by(|candidate| candidate.name.cmp(&member.name))
                .ok()
                .with_context(|| format!("source.discovery_missing:{}", member.name))?;
            ensure!(
                self.members[index] == *member && self.payloads[index].as_ref() == bytes.as_ref(),
                "source.discovery_changed:{}",
                member.name
            );
        }
        Ok(())
    }

    /// The coordinator passes its canonical output parent before creating files.
    pub(crate) fn check_output_parent(&self, canonical_parent: &Path) -> Result<()> {
        let install = self.data.parent().context("source.installation_parent")?;
        ensure!(
            !canonical_parent.starts_with(&self.data) && !canonical_parent.starts_with(install),
            "output.inside_source_installation"
        );
        Ok(())
    }

    /// Reopen the original canonical Data directory and compare every selected
    /// member's actual winning basename and exact bytes, not just metadata/hash.
    /// A new winner with equal bytes is a change. Missing/deleted/corrupt members
    /// fail closed, using the same bounded chain without falling back.
    ///
    /// This is an observation, not a filesystem lock or atomic snapshot guarantee.
    /// The writer must call it immediately before publishing its completion marker.
    pub(crate) fn verify_unchanged(&self) -> Result<()> {
        let chain = open_chain(&self.data).context("source.recheck_open")?;
        let names: Vec<_> = self
            .members
            .iter()
            .map(|member| member.name.clone())
            .collect();
        let plans = plan_members(&chain, &names).context("source.recheck_members")?;
        for ((member, captured), plan) in self.members.iter().zip(&self.payloads).zip(plans) {
            ensure!(
                member.winning_archive == plan.winning_archive,
                "source.winner_changed:{}",
                member.name
            );
            ensure!(
                member.bytes == plan.bytes,
                "source.bytes_changed:{}",
                member.name
            );
            // Only one fresh member is resident at a time in addition to capture.
            let fresh = read_member(&chain, &plan).context("source.recheck_read")?;
            ensure!(
                captured.as_ref() == fresh.as_slice(),
                "source.bytes_changed:{}",
                member.name
            );
        }
        self.verify_ad_compatible()
            .context("source.recheck_ad_profile")?;
        Ok(())
    }

    fn verify_ad_compatible(&self) -> Result<()> {
        // System.cpp CONF_mpq_list / LoadCommonMPQFiles skip absent fixed names;
        // mpq_libmpq.cpp pushes each archive to the front and uses the first hit.
        // Exact basenames avoid filesystem-dependent case aliases. This stricter
        // profile rejects unsupported app winners even if their bytes coincide.
        for (index, entry) in std::fs::read_dir(&self.data)?.enumerate() {
            ensure!(index < 4096, "source.ad_directory_entry_limit");
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if let Some(expected) = AD_ARCHIVES
                .iter()
                .find(|fixed| fixed.eq_ignore_ascii_case(name))
            {
                ensure!(name == *expected, "source.ad_archive_case_ambiguity");
            }
        }
        let mut resolved = vec![false; self.members.len()];
        for archive_name in AD_ARCHIVES.iter().rev() {
            let path = self.data.join(archive_name);
            let metadata = match std::fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error).context("source.ad_archive_metadata"),
            };
            ensure!(
                metadata.file_type().is_file(),
                "source.ad_regular_archive_required"
            );
            // One independently bounded source archive is held at a time, never
            // five separate full index budgets retained together.
            let chain = open_chain(&path).context("source.ad_open_archive")?;
            for (index, member) in self.members.iter().enumerate() {
                if resolved[index] {
                    continue;
                }
                if !chain
                    .verify_classic_ad_member(&member.name)
                    .with_context(|| format!("source.ad_member_profile:{}", member.name))?
                {
                    continue;
                }
                let archive = chain
                    .archive_for(&member.name)
                    .with_context(|| format!("source.ad_deleted:{}", member.name))?;
                let bytes = archive
                    .file_size(&member.name)
                    .context("source.ad_missing_size")? as usize;
                // MPQFile's <=1-byte dummy blocks lower archives too.
                ensure!(bytes > 1, "source.ad_dummy_member:{}", member.name);
                ensure!(
                    member.winning_archive == *archive_name,
                    "source.ad_winner_mismatch:{}",
                    member.name
                );
                ensure!(
                    bytes == member.bytes,
                    "source.ad_bytes_changed:{}",
                    member.name
                );
                let fresh = chain
                    .read(&member.name)
                    .with_context(|| format!("source.ad_read:{}", member.name))?;
                ensure!(
                    fresh.as_slice() == self.payloads[index].as_ref(),
                    "source.ad_bytes_changed:{}",
                    member.name
                );
                resolved[index] = true;
            }
        }
        ensure!(
            resolved.iter().all(|resolved| *resolved),
            "source.ad_missing_member"
        );
        Ok(())
    }
}

/// Pinned libmpq archive_open(-1) selects the first signature on a 512-byte
/// boundary and normalizes version-zero header_size to 32 (mpq.c:59-148).
/// Indices and payloads still use the existing bounded reader.
fn verify_ad_archive_header(path: &Path) -> Result<(u64, [u8; 32])> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let mut file = options.open(path).context("source.ad_header_open")?;
    let metadata = file.metadata()?;
    ensure!(metadata.is_file(), "source.ad_regular_archive_required");
    let mut header = [0u8; 32];
    let mut offset = 0u64;
    loop {
        ensure!(offset + 4 <= metadata.len(), "source.ad_header_signature");
        ensure!(
            offset + 32 <= MAX_HEADER_SCAN_BYTES,
            "source.ad_header_scan_limit"
        );
        file.seek(SeekFrom::Start(offset))
            .context("source.ad_header_read")?;
        file.read_exact(&mut header[..4])
            .context("source.ad_header_read")?;
        if header[..4] == *b"MPQ\x1a" {
            ensure!(offset + 32 <= metadata.len(), "source.ad_header_truncated");
            file.read_exact(&mut header[4..])
                .context("source.ad_header_read")?;
            break;
        }
        // User-data pointers are not part of the pinned extractor's scan.
        offset += 512;
    }
    let word = |at| u32::from_le_bytes(header[at..at + 4].try_into().expect("fixed header"));
    let half = |at| u16::from_le_bytes(header[at..at + 2].try_into().expect("fixed header"));
    ensure!(half(12) == 0, "source.ad_header_version");
    let archive_bytes = metadata.len() - offset;
    ensure!(
        u64::from(word(8)) == archive_bytes,
        "source.ad_archive_size"
    );
    ensure!(half(14) <= 7, "source.ad_sector_size");
    let hash_count = word(24);
    ensure!(hash_count.is_power_of_two(), "source.ad_hash_count");
    let hash_bytes = u64::from(hash_count) * 16;
    let block_bytes = u64::from(word(28)) * 16;
    ensure!(
        hash_bytes + block_bytes <= 64 * 1024 * 1024,
        "source.ad_index_budget"
    );
    let hash_start = u64::from(word(16));
    let block_start = u64::from(word(20));
    let hash_end = hash_start + hash_bytes;
    let block_end = block_start + block_bytes;
    ensure!(
        hash_start >= 32
            && block_start >= 32
            && hash_end <= archive_bytes
            && block_end <= archive_bytes
            && (hash_end <= block_start || block_end <= hash_start),
        "source.ad_index_spans"
    );
    Ok((offset, header))
}

fn hash_field(hash: &mut Sha256, value: &[u8]) {
    hash.update((value.len() as u64).to_le_bytes());
    hash.update(value);
}

fn open_chain(data: &Path) -> Result<Chain> {
    // Validate fixed archives before the ordinary reader so header failures have
    // precise path-free diagnostics even when index decoding cannot begin.
    let paths = if data.is_dir() {
        AD_ARCHIVES.iter().map(|name| data.join(name)).collect()
    } else {
        vec![data.to_path_buf()]
    };
    let mut headers = Vec::new();
    for path in paths {
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) => ensure!(
                metadata.file_type().is_file(),
                "source.ad_regular_archive_required"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && data.is_dir() => continue,
            Err(error) => return Err(error).context("source.ad_archive_metadata"),
        }
        let header = verify_ad_archive_header(&path)?;
        headers.push((path, header));
    }
    let chain = Chain::open_bounded(
        data,
        ChainLimits {
            index_bytes: 64 * 1024 * 1024,
            sector_bytes: 64 * 1024,
            member_bytes: MAX_MEMBER_BYTES,
            header_scan_bytes: MAX_HEADER_SCAN_BYTES,
            archives: 64,
            directory_entries: 4096,
        },
    )
    .context("source.open_patch_chain")?;
    for archive in chain.archives() {
        if let Some((_, expected)) = headers.iter().find(|(path, _)| path == archive.path()) {
            ensure!(
                archive.header_snapshot() == *expected,
                "source.ad_header_selection"
            );
        }
    }
    Ok(chain)
}

struct PlannedMember {
    name: String,
    winning_archive: String,
    bytes: usize,
}

fn plan_members(chain: &Chain, names: &[String]) -> Result<Vec<PlannedMember>> {
    let mut total = 0usize;
    names
        .iter()
        .map(|name| {
            let archive = chain
                .archive_for(name)
                .with_context(|| format!("source.missing_or_deleted:{name}"))?;
            let bytes = archive.file_size(name).context("source.missing_size")? as usize;
            ensure!(bytes <= MAX_MEMBER_BYTES, "source.member_byte_limit:{name}");
            total = total
                .checked_add(bytes)
                .context("source.aggregate_byte_limit")?;
            ensure!(total <= MAX_CAPTURE_BYTES, "source.aggregate_byte_limit");
            let winning_archive = archive
                .path()
                .file_name()
                .and_then(|name| name.to_str())
                .context("source.archive_basename")?
                .to_owned();
            Ok(PlannedMember {
                name: name.clone(),
                winning_archive,
                bytes,
            })
        })
        .collect()
}

fn read_member(chain: &Chain, member: &PlannedMember) -> Result<Vec<u8>> {
    let bytes = chain
        .read(&member.name)
        .with_context(|| format!("source.read:{}", member.name))?;
    ensure!(
        bytes.len() == member.bytes,
        "source.member_length_changed:{}",
        member.name
    );
    Ok(bytes)
}

fn selected_names(names: &[impl AsRef<str>]) -> Result<Vec<String>> {
    ensure!(
        !names.is_empty() && names.len() <= MAX_CAPTURE_MEMBERS,
        "source.member_count_limit"
    );
    let mut selected = BTreeSet::new();
    let mut selected_map = None;
    let mut wdts = 0;
    let mut adts = 0;
    for name in names {
        let name = name.as_ref();
        ensure!(
            name.len() <= MAX_MEMBER_NAME_BYTES,
            "source.member_name_limit"
        );
        ensure!(selected.insert(name.to_owned()), "source.duplicate_member");
        if DBC_MEMBERS.contains(&name) {
            continue;
        }
        let parts: Vec<_> = name.split('\\').collect();
        ensure!(
            parts.len() == 4 && parts[0] == "World" && parts[1] == "Maps",
            "source.unselected_member_kind"
        );
        let map = parts[2];
        ensure!(
            !map.is_empty()
                && map.len() < 64
                && map
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'),
            "source.map_directory"
        );
        if let Some(previous) = selected_map {
            ensure!(previous == map, "source.multiple_maps");
        } else {
            selected_map = Some(map);
        }
        if parts[3] == format!("{map}.wdt") {
            wdts += 1;
            ensure!(wdts <= 1, "source.wdt_count_limit");
            continue;
        }
        let coordinates = parts[3]
            .strip_prefix(map)
            .and_then(|rest| rest.strip_prefix('_'))
            .and_then(|rest| rest.strip_suffix(".adt"))
            .and_then(|rest| rest.split_once('_'))
            .context("source.unselected_member_kind")?;
        for coordinate in [coordinates.0, coordinates.1] {
            let value: u8 = coordinate.parse().context("source.tile_coordinate")?;
            ensure!(
                value < 64 && coordinate == value.to_string(),
                "source.tile_coordinate"
            );
        }
        adts += 1;
        ensure!(adts <= MAX_ADT_MEMBERS, "source.adt_count_limit");
    }
    Ok(selected.into_iter().collect())
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
