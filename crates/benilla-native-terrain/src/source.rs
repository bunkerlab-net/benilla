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
/// A dependency search must fail closed at this distinct-name limit, never treat
/// an unvisited prefix as process-initial state. Positive and absent lookups count.
pub(crate) const MAX_HISTORY_LOOKUPS: usize = 4096;
const MAX_MEMBER_NAME_BYTES: usize = 192;
const MAX_HEADER_SCAN_BYTES: u64 = 1024 * 1024;
/// Conservative intersection of app and pinned extractor source resolution.
pub(crate) const SOURCE_POLICY: &str = "benilla-chain-and-cmangos-ad-fixed-five-exact-winner-v3";
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

/// Only a checkpoint can mint cancellation. Anyhow contexts preserve the marker;
/// an unrelated source error is never reclassified by polling after it fails.
#[derive(Debug)]
struct Cancelled;

impl fmt::Display for Cancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("terrain source freshness check cancelled")
    }
}

impl std::error::Error for Cancelled {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceOperation {
    PathKind,
    DirectoryOpen,
    DirectoryEntry,
    ArchiveMetadata,
    HeaderOpen,
    HeaderMetadata,
    HeaderSeek,
    HeaderRead,
    ChainOpen,
    HeaderSelection,
    MemberLookup,
    MemberSize,
    MemberProfile,
    MemberRead,
    MemberCompare,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceStage {
    Start,
    App,
    Independent,
    Before(SourceOperation),
    After(SourceOperation),
    Complete,
}

struct SourceCheck<'a> {
    cancelled: &'a dyn Fn() -> bool,
    #[cfg(test)]
    observe: Option<&'a dyn Fn(SourceStage)>,
}

impl<'a> SourceCheck<'a> {
    fn new(cancelled: &'a dyn Fn() -> bool) -> Self {
        Self {
            cancelled,
            #[cfg(test)]
            observe: None,
        }
    }

    fn checkpoint(&self, stage: SourceStage) -> Result<()> {
        #[cfg(not(test))]
        let _ = stage;
        #[cfg(test)]
        if let Some(observe) = self.observe {
            observe(stage);
        }
        if (self.cancelled)() {
            return Err(Cancelled.into());
        }
        Ok(())
    }

    /// A bounded synchronous operation cannot be interrupted while running.
    /// Preserve its failure without a second poll; poll again only on success.
    fn run<T>(&self, operation: SourceOperation, action: impl FnOnce() -> Result<T>) -> Result<T> {
        self.checkpoint(SourceStage::Before(operation))?;
        let value = action()?;
        self.checkpoint(SourceStage::After(operation))?;
        Ok(value)
    }
}

fn freshness_result(result: Result<()>) -> std::result::Result<(), crate::TerrainFreshnessError> {
    result.map_err(|error| {
        if error.downcast_ref::<Cancelled>().is_some() {
            crate::TerrainFreshnessError::Cancelled
        } else {
            crate::TerrainFreshnessError::Source(error)
        }
    })
}

/// Informational provenance derived only from a successfully read member.
/// This is not a deserializable snapshot, admission token, or parser result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SourceMember {
    pub(crate) name: String,
    pub(crate) winning_archive: String,
    pub(crate) sha256: String,
    pub(crate) bytes: usize,
}

/// A canonical terrain name absent from both the app chain and the pinned
/// extractor's fixed archive order. Malformed, deleted, dummy, or unreadable hits
/// cannot mint this receipt. Like positive receipts, this is informational only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct AbsentSourceMember {
    pub(crate) name: String,
}

/// There is no constructor from metadata, digest, serialized output, or borrowed
/// bytes. Only capture can mint this value; callers only borrow its payloads.
pub(crate) struct CapturedSource {
    data: PathBuf,
    members: Vec<SourceMember>,
    payloads: Vec<Box<[u8]>>,
    absent_members: Vec<AbsentSourceMember>,
    history_names: BTreeSet<String>,
    binding_digest: String,
}

impl fmt::Debug for CapturedSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CapturedSource")
            .field("members", &self.members)
            .field("absent_members", &self.absent_members)
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
        let check = SourceCheck::new(&|| false);
        let names = selected_names(names)?;
        ensure!(data.is_dir(), "source.data_directory_required");
        let data = data.canonicalize().context("source.data_directory")?;
        let chain = open_chain(&data, &check)?;
        // Inspect all winning sizes before any member read/decompression.
        let plans = plan_members(&chain, &names, &check)?;
        let mut members = Vec::with_capacity(plans.len());
        let mut payloads = Vec::with_capacity(plans.len());
        for plan in plans {
            let bytes = read_member(&chain, &plan, &check)?;
            members.push(SourceMember {
                name: plan.name,
                winning_archive: plan.winning_archive,
                sha256: format!("{:x}", Sha256::digest(&bytes)),
                bytes: bytes.len(),
            });
            payloads.push(bytes.into_boxed_slice());
        }
        let mut captured = Self {
            data,
            members,
            payloads,
            absent_members: Vec::new(),
            history_names: BTreeSet::new(),
            binding_digest: String::new(),
        };
        captured.verify_ad_compatible(&check)?;
        captured.refresh_binding_digest();
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

    /// Explicit dual-resolver absence provenance, separate from positive members.
    pub(crate) fn absent_members(&self) -> &[AbsentSourceMember] {
        &self.absent_members
    }

    pub(crate) fn binding_digest(&self) -> &str {
        &self.binding_digest
    }

    /// Extend the immutable evidence set with one coordinator-derived history
    /// WDT/ADT, possibly from an earlier map. `false` means proven dual-resolver
    /// absence, never a failed load. Repeated names reuse their exact receipt.
    /// Every failure leaves both evidence and epoch unchanged.
    pub(crate) fn capture_history_member(&mut self, name: &str) -> Result<bool> {
        let check = SourceCheck::new(&|| false);
        history_name(name)?;
        if self.history_names.contains(name) {
            return Ok(self.bytes(name).is_ok());
        }
        ensure!(
            self.history_names.len() < MAX_HISTORY_LOOKUPS,
            "source.history_lookup_limit"
        );
        // The original selected inputs may have changed between selection and
        // starting traversal. Do not extend stale proofs into a new epoch.
        if self.history_names.is_empty() {
            self.verify_unchanged()
                .context("source.history_initial_recheck")?;
        }
        if self
            .members
            .binary_search_by(|member| member.name.as_str().cmp(name))
            .is_ok()
        {
            self.history_names.insert(name.to_owned());
            return Ok(true);
        }
        let chain = open_chain(&self.data, &check).context("source.history_open")?;
        let mut candidate = Self {
            data: self.data.clone(),
            members: Vec::new(),
            payloads: Vec::new(),
            absent_members: Vec::new(),
            history_names: BTreeSet::new(),
            binding_digest: String::new(),
        };
        // Plan and budget a readable app winner before any member decompression.
        // find_file_archive includes tombstones, which plan_members rejects.
        if chain.find_file_archive(name).is_some() {
            let plan = plan_members(&chain, &[name.to_owned()], &check)?
                .pop()
                .expect("one planned history member");
            check_history_byte_budget(&self.members, plan.bytes)?;
            ensure!(
                chain
                    .verify_classic_ad_member(name)
                    .with_context(|| format!("source.history_member_profile:{name}"))?,
                "source.history_resolver_mismatch:{name}"
            );
            let bytes = read_member(&chain, &plan, &check)?;
            candidate.members.push(SourceMember {
                name: plan.name,
                winning_archive: plan.winning_archive,
                sha256: format!("{:x}", Sha256::digest(&bytes)),
                bytes: bytes.len(),
            });
            candidate.payloads.push(bytes.into_boxed_slice());
        } else {
            verify_app_absence(&chain, name, &check)?;
            candidate.absent_members.push(AbsentSourceMember {
                name: name.to_owned(),
            });
        }
        candidate.verify_ad_compatible(&check)?;
        let present = !candidate.members.is_empty();
        if let Some(member) = candidate.members.pop() {
            let index = self
                .members
                .binary_search_by(|item| item.name.cmp(&member.name))
                .expect_err("history member is new");
            self.members.insert(index, member);
            self.payloads
                .insert(index, candidate.payloads.pop().expect("captured bytes"));
        } else {
            let absent = candidate.absent_members.pop().expect("captured absence");
            let index = self
                .absent_members
                .binary_search_by(|item| item.name.cmp(&absent.name))
                .expect_err("history absence is new");
            self.absent_members.insert(index, absent);
        }
        self.history_names.insert(name.to_owned());
        self.refresh_binding_digest();
        Ok(present)
    }

    fn refresh_binding_digest(&mut self) {
        // Keep the original positive-only epoch stable. Each extension rebinds
        // every retained byte in canonical name order, independent of query order.
        let mut binding = Sha256::new();
        binding.update(b"benilla-native-terrain-source-v1\0");
        hash_field(&mut binding, SOURCE_POLICY.as_bytes());
        hash_field(&mut binding, crate::SOURCE_PIN.as_bytes());
        binding.update((self.members.len() as u64).to_le_bytes());
        for (member, bytes) in self.members.iter().zip(&self.payloads) {
            hash_field(&mut binding, member.name.as_bytes());
            hash_field(&mut binding, member.winning_archive.as_bytes());
            hash_field(&mut binding, bytes);
        }
        if !self.absent_members.is_empty() {
            hash_field(&mut binding, b"dual-resolver-absent-members-v1");
            binding.update((self.absent_members.len() as u64).to_le_bytes());
            for absent in &self.absent_members {
                hash_field(&mut binding, b"absent");
                hash_field(&mut binding, absent.name.as_bytes());
            }
        }
        self.binding_digest = format!("{:x}", binding.finalize());
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
        for absent in &prior.absent_members {
            ensure!(
                self.absent_members
                    .binary_search_by(|candidate| candidate.name.cmp(&absent.name))
                    .is_ok(),
                "source.discovery_absence_changed:{}",
                absent.name
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
        self.verify_unchanged_checked(&SourceCheck::new(&|| false))
    }

    pub(crate) fn verify_unchanged_cancellable(
        &self,
        cancelled: impl Fn() -> bool,
    ) -> std::result::Result<(), crate::TerrainFreshnessError> {
        freshness_result(self.verify_unchanged_checked(&SourceCheck::new(&cancelled)))
    }

    fn verify_unchanged_checked(&self, check: &SourceCheck<'_>) -> Result<()> {
        check.checkpoint(SourceStage::Start)?;
        check.checkpoint(SourceStage::App)?;
        let chain = open_chain(&self.data, check).context("source.recheck_open")?;
        let names: Vec<_> = self
            .members
            .iter()
            .map(|member| member.name.clone())
            .collect();
        let plans = plan_members(&chain, &names, check).context("source.recheck_members")?;
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
            if self.history_names.contains(&member.name) {
                ensure!(
                    check
                        .run(SourceOperation::MemberProfile, || {
                            chain.verify_classic_ad_member(&member.name)
                        })
                        .with_context(|| format!(
                            "source.history_member_profile:{}",
                            member.name
                        ))?,
                    "source.history_resolver_mismatch:{}",
                    member.name
                );
            }
            // Only one fresh member is resident at a time in addition to capture.
            let fresh = read_member(&chain, &plan, check).context("source.recheck_read")?;
            check.run(SourceOperation::MemberCompare, || {
                ensure!(
                    captured.as_ref() == fresh.as_slice(),
                    "source.bytes_changed:{}",
                    member.name
                );
                Ok(())
            })?;
        }
        for absent in &self.absent_members {
            verify_app_absence(&chain, &absent.name, check).context("source.recheck_absence")?;
        }
        check.checkpoint(SourceStage::Independent)?;
        self.verify_ad_compatible(check)
            .context("source.recheck_ad_profile")?;
        check.checkpoint(SourceStage::Complete)?;
        Ok(())
    }

    fn verify_ad_compatible(&self, check: &SourceCheck<'_>) -> Result<()> {
        // System.cpp CONF_mpq_list / LoadCommonMPQFiles skip absent fixed names;
        // mpq_libmpq.cpp pushes each archive to the front and uses the first hit.
        // Exact basenames avoid filesystem-dependent case aliases. This stricter
        // profile rejects unsupported app winners even if their bytes coincide.
        let mut entries = check.run(SourceOperation::DirectoryOpen, || {
            Ok(std::fs::read_dir(&self.data)?)
        })?;
        let mut index = 0;
        while let Some(entry) = check.run(SourceOperation::DirectoryEntry, || {
            Ok(entries.next().transpose()?)
        })? {
            ensure!(index < 4096, "source.ad_directory_entry_limit");
            index += 1;
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
            let metadata =
                check.run(
                    SourceOperation::ArchiveMetadata,
                    || match std::fs::symlink_metadata(&path) {
                        Ok(metadata) => Ok(Some(metadata)),
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                        Err(error) => Err(error).context("source.ad_archive_metadata"),
                    },
                )?;
            let Some(metadata) = metadata else { continue };
            ensure!(
                metadata.file_type().is_file(),
                "source.ad_regular_archive_required"
            );
            // One independently bounded source archive is held at a time, never
            // five separate full index budgets retained together.
            let chain = open_chain(&path, check).context("source.ad_open_archive")?;
            for absent in &self.absent_members {
                ensure!(
                    !check
                        .run(SourceOperation::MemberProfile, || {
                            chain.verify_classic_ad_member(&absent.name)
                        })
                        .with_context(|| format!("source.ad_absence_profile:{}", absent.name))?,
                    "source.ad_absence_changed:{}",
                    absent.name
                );
                ensure!(
                    check.run(SourceOperation::MemberLookup, || {
                        Ok(chain.find_file_archive(&absent.name).is_none())
                    })?,
                    "source.ad_absence_changed:{}",
                    absent.name
                );
            }
            for (index, member) in self.members.iter().enumerate() {
                if resolved[index] {
                    continue;
                }
                if !check
                    .run(SourceOperation::MemberProfile, || {
                        chain.verify_classic_ad_member(&member.name)
                    })
                    .with_context(|| format!("source.ad_member_profile:{}", member.name))?
                {
                    continue;
                }
                let archive = check
                    .run(SourceOperation::MemberLookup, || {
                        chain.archive_for(&member.name)
                    })
                    .with_context(|| format!("source.ad_deleted:{}", member.name))?;
                let bytes = check.run(SourceOperation::MemberSize, || {
                    archive
                        .file_size(&member.name)
                        .context("source.ad_missing_size")
                })? as usize;
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
                let fresh = check
                    .run(SourceOperation::MemberRead, || chain.read(&member.name))
                    .with_context(|| format!("source.ad_read:{}", member.name))?;
                check.run(SourceOperation::MemberCompare, || {
                    ensure!(
                        fresh.as_slice() == self.payloads[index].as_ref(),
                        "source.ad_bytes_changed:{}",
                        member.name
                    );
                    Ok(())
                })?;
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
/// boundary and normalizes raw-version-zero/one header sizes to 32/44 (mpq.c:59-164).
/// Version one's extended offsets must be zero for the bounded 32-bit index reader.
#[derive(Debug, PartialEq, Eq)]
struct AdHeaderSnapshot {
    base: (u64, [u8; 32]),
    extension: Option<[u8; 12]>,
}

fn verify_ad_archive_header(path: &Path, check: &SourceCheck<'_>) -> Result<AdHeaderSnapshot> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let mut file = check.run(SourceOperation::HeaderOpen, || {
        options.open(path).context("source.ad_header_open")
    })?;
    let metadata = check.run(SourceOperation::HeaderMetadata, || Ok(file.metadata()?))?;
    ensure!(metadata.is_file(), "source.ad_regular_archive_required");
    let mut header = [0u8; 32];
    let mut offset = 0u64;
    loop {
        ensure!(offset + 4 <= metadata.len(), "source.ad_header_signature");
        ensure!(
            offset + 32 <= MAX_HEADER_SCAN_BYTES,
            "source.ad_header_scan_limit"
        );
        check.run(SourceOperation::HeaderSeek, || {
            file.seek(SeekFrom::Start(offset))
                .context("source.ad_header_read")
        })?;
        check.run(SourceOperation::HeaderRead, || {
            file.read_exact(&mut header[..4])
                .context("source.ad_header_read")
        })?;
        if header[..4] == *b"MPQ\x1a" {
            ensure!(offset + 32 <= metadata.len(), "source.ad_header_truncated");
            check.run(SourceOperation::HeaderRead, || {
                file.read_exact(&mut header[4..])
                    .context("source.ad_header_read")
            })?;
            break;
        }
        // User-data pointers are not part of the pinned extractor's scan.
        offset += 512;
    }
    let word = |at| u32::from_le_bytes(header[at..at + 4].try_into().expect("fixed header"));
    let half = |at| u16::from_le_bytes(header[at..at + 2].try_into().expect("fixed header"));
    let header_bytes = match half(12) {
        0 => 32,
        1 => 44,
        _ => anyhow::bail!("source.ad_header_version"),
    };
    let extension = if header_bytes == 44 {
        ensure!(offset + 44 <= metadata.len(), "source.ad_header_truncated");
        ensure!(
            offset + 44 <= MAX_HEADER_SCAN_BYTES,
            "source.ad_header_scan_limit"
        );
        let mut extension = [0u8; 12];
        check.run(SourceOperation::HeaderRead, || {
            file.read_exact(&mut extension)
                .context("source.ad_header_read")
        })?;
        // mpq_header_ex_s: u64 extended block table, u16 hash high, u16 block high.
        ensure!(extension == [0; 12], "source.ad_header_extension");
        Some(extension)
    } else {
        None
    };
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
        hash_start >= header_bytes
            && block_start >= header_bytes
            && hash_end <= archive_bytes
            && block_end <= archive_bytes
            && (hash_end <= block_start || block_end <= hash_start),
        "source.ad_index_spans"
    );
    Ok(AdHeaderSnapshot {
        base: (offset, header),
        extension,
    })
}

fn hash_field(hash: &mut Sha256, value: &[u8]) {
    hash.update((value.len() as u64).to_le_bytes());
    hash.update(value);
}

fn open_chain(data: &Path, check: &SourceCheck<'_>) -> Result<Chain> {
    // Validate fixed archives before the ordinary reader so header failures have
    // precise path-free diagnostics even when index decoding cannot begin.
    let is_directory = check.run(SourceOperation::PathKind, || Ok(data.is_dir()))?;
    let paths = if is_directory {
        AD_ARCHIVES.iter().map(|name| data.join(name)).collect()
    } else {
        vec![data.to_path_buf()]
    };
    let mut headers = Vec::new();
    for path in paths {
        let metadata =
            check.run(
                SourceOperation::ArchiveMetadata,
                || match std::fs::symlink_metadata(&path) {
                    Ok(metadata) => Ok(Some(metadata)),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound && is_directory => {
                        Ok(None)
                    }
                    Err(error) => Err(error).context("source.ad_archive_metadata"),
                },
            )?;
        let Some(metadata) = metadata else { continue };
        ensure!(
            metadata.file_type().is_file(),
            "source.ad_regular_archive_required"
        );
        let header = verify_ad_archive_header(&path, check)?;
        headers.push((path, header));
    }
    let chain = check.run(SourceOperation::ChainOpen, || {
        Chain::open_bounded(
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
        .context("source.open_patch_chain")
    })?;
    for archive in chain.archives() {
        check.run(SourceOperation::HeaderSelection, || {
            if let Some((_, expected)) = headers.iter().find(|(path, _)| path == archive.path()) {
                ensure!(
                    archive.header_snapshot() == expected.base
                        && archive.header_extension_snapshot() == expected.extension,
                    "source.ad_header_selection"
                );
            }
            Ok(())
        })?;
    }
    Ok(chain)
}

struct PlannedMember {
    name: String,
    winning_archive: String,
    bytes: usize,
}

fn plan_members(
    chain: &Chain,
    names: &[String],
    check: &SourceCheck<'_>,
) -> Result<Vec<PlannedMember>> {
    let mut total = 0usize;
    names
        .iter()
        .map(|name| {
            let archive = check
                .run(SourceOperation::MemberLookup, || chain.archive_for(name))
                .with_context(|| format!("source.missing_or_deleted:{name}"))?;
            let bytes = check.run(SourceOperation::MemberSize, || {
                archive.file_size(name).context("source.missing_size")
            })? as usize;
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

fn read_member(chain: &Chain, member: &PlannedMember, check: &SourceCheck<'_>) -> Result<Vec<u8>> {
    let bytes = check
        .run(SourceOperation::MemberRead, || chain.read(&member.name))
        .with_context(|| format!("source.read:{}", member.name))?;
    ensure!(
        bytes.len() == member.bytes,
        "source.member_length_changed:{}",
        member.name
    );
    Ok(bytes)
}

fn check_history_byte_budget(members: &[SourceMember], bytes: usize) -> Result<()> {
    let retained = members.iter().try_fold(0usize, |total, member| {
        total
            .checked_add(member.bytes)
            .context("source.aggregate_byte_limit")
    })?;
    ensure!(
        retained
            .checked_add(bytes)
            .is_some_and(|total| total <= MAX_CAPTURE_BYTES),
        "source.aggregate_byte_limit"
    );
    Ok(())
}

fn verify_app_absence(chain: &Chain, name: &str, check: &SourceCheck<'_>) -> Result<()> {
    // contains() alone collapses tombstones to false and ordinary lookup can skip
    // malformed hash entries. Both checks are required before recording absence.
    ensure!(
        !check
            .run(SourceOperation::MemberProfile, || chain
                .verify_classic_ad_member(name))
            .with_context(|| format!("source.absence_member_profile:{name}"))?,
        "source.absence_changed:{name}"
    );
    ensure!(
        check.run(SourceOperation::MemberLookup, || Ok(chain
            .find_file_archive(name)
            .is_none()))?,
        "source.absence_changed:{name}"
    );
    Ok(())
}

fn history_name(name: &str) -> Result<()> {
    ensure!(!DBC_MEMBERS.contains(&name), "source.history_member_kind");
    selected_names(&[name])?;
    Ok(())
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
