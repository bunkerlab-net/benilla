// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact, bounded VMap archive acquisition from the user's read-only install.
//!
//! CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f:
//! vmapexport.cpp:316-377 mounts terrain/model/texture/wmo/base, then patch and
//! numeric patch-2 through patch-99. The formatted misc path is never pushed.
//! mpq_libmpq.cpp:53,67-95 searches in reverse and stops on a <=1-byte dummy.
//!
//! Each successful lookup records a privately minted ordered receipt. Positive
//! bytes are immutable owned buffers; callers can discard each after parsing.
//! Fresh-chain verification replays every receipt, including absence and dummy
//! results, and checks the exact mounted archive order. This is not an atomic
//! filesystem snapshot, a live lease, or protection against hostile ABA changes.
//! No filesystem-wide integrity or client-build authenticity is claimed.

use benilla_mpq::{Archive, ClassicMemberProbe, ReadLimits};
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::{Path, PathBuf};

pub(crate) const SOURCE_POLICY: &str = "cmangos-vmap-numeric-1-through-99-reverse-first-hit-v1";
const SOURCE_PIN: &str = "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f";
pub(crate) const MAX_MEMBER_BYTES: usize = 64 * 1024 * 1024;
/// Conservative decoded-byte work charged twice per positive lookup: strict
/// codec/profile verification and payload read. A recheck has its own same cap.
pub(crate) const MAX_READ_WORK_BYTES: u64 = 32 * 1024 * 1024 * 1024;
pub(crate) const MAX_LOOKUPS: usize = 32_768;
const MAX_NAME_BYTES: usize = 512;
const MAX_INDEX_BYTES: usize = 64 * 1024 * 1024;
const MAX_HEADER_SCAN_BYTES: u64 = 1024 * 1024;
const MAX_DIRECTORY_ENTRIES: usize = 4096;
const MAX_ARCHIVES: usize = 104;
const BASE_ARCHIVES: [&str; 5] = [
    "terrain.MPQ",
    "model.MPQ",
    "texture.MPQ",
    "wmo.MPQ",
    "base.MPQ",
];

/// Stable, path-free errors. Neither user filesystem paths nor member text are
/// retained in the error or its source chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceError {
    Cancelled,
    DataDirectory,
    DirectoryRead,
    DirectoryLimit,
    ArchiveCase,
    ArchiveNotRegular,
    ArchiveOpen,
    ArchiveLimit,
    InvalidArchive,
    DummyListfile,
    HeaderVersion,
    HeaderExtension,
    HeaderSize,
    HeaderSector,
    HeaderIndex,
    IndexLimit,
    InvalidName,
    LookupLimit,
    MemberLimit,
    ReadWorkLimit,
    MemberProfile,
    MemberRead,
    Changed,
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = match self {
            Self::Cancelled => "cancelled",
            Self::DataDirectory => "data_directory",
            Self::DirectoryRead => "directory_read",
            Self::DirectoryLimit => "directory_limit",
            Self::ArchiveCase => "archive_case",
            Self::ArchiveNotRegular => "archive_not_regular",
            Self::ArchiveOpen => "archive_open",
            Self::ArchiveLimit => "archive_limit",
            Self::InvalidArchive => "invalid_archive",
            Self::DummyListfile => "dummy_listfile",
            Self::HeaderVersion => "header_version",
            Self::HeaderExtension => "header_extension",
            Self::HeaderSize => "header_size",
            Self::HeaderSector => "header_sector",
            Self::HeaderIndex => "header_index",
            Self::IndexLimit => "index_limit",
            Self::InvalidName => "invalid_name",
            Self::LookupLimit => "lookup_limit",
            Self::MemberLimit => "member_limit",
            Self::ReadWorkLimit => "read_work_limit",
            Self::MemberProfile => "member_profile",
            Self::MemberRead => "member_read",
            Self::Changed => "changed",
        };
        write!(f, "vmap.source.{code}")
    }
}
impl std::error::Error for SourceError {}
type Result<T> = std::result::Result<T, SourceError>;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct SourceMember {
    pub(crate) name: String,
    pub(crate) winning_archive: String,
    pub(crate) sha256: String,
    pub(crate) bytes: usize,
}

/// No payload constructor, mutator, or deserializer exists outside acquisition.
pub(crate) struct CapturedMember {
    member: SourceMember,
    payload: Box<[u8]>,
}
impl CapturedMember {
    pub(crate) fn bytes(&self) -> &[u8] {
        debug_assert_eq!(self.member.bytes, self.payload.len());
        &self.payload
    }
    #[cfg(test)]
    pub(crate) fn member(&self) -> &SourceMember {
        &self.member
    }
}

pub(crate) enum Lookup {
    Present(CapturedMember),
    Absent,
    // Exact winning archive and <=1-byte size live in the private receipt.
    Dummy {},
}
impl fmt::Debug for Lookup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Present(_) => "Present",
            Self::Absent => "Absent",
            Self::Dummy { .. } => "Dummy",
        })
    }
}

#[derive(Clone, PartialEq, Eq)]
enum Outcome {
    Present(SourceMember),
    Absent,
    Dummy {
        winning_archive: String,
        bytes: usize,
    },
}

/// Receipt order is the original consumer call order, including repeated names.
/// Together with mount order, probe_count specifies every visited archive in
/// descending priority, through the first hit or complete absence.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LookupScope {
    Chain,
    Archive(usize),
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct LookupReceipt {
    scope: LookupScope,
    name: String,
    probe_count: usize,
    outcome: Outcome,
}

struct Chain {
    archives: Vec<Archive>,
    names: Vec<String>,
}

pub(crate) struct CapturedSource {
    data: PathBuf,
    chain: Chain,
    receipts: Vec<LookupReceipt>,
    read_work: u64,
}
impl fmt::Debug for CapturedSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CapturedSource")
            .field("archives", &self.chain.names.len())
            .field("lookups", &self.receipts.len())
            .field("read_work_bytes", &self.read_work)
            .finish_non_exhaustive()
    }
}

impl CapturedSource {
    pub(crate) fn source_receipts(&self) -> impl Iterator<Item = crate::SourceReceipt<'_>> {
        self.receipts.iter().filter_map(|receipt| {
            if !matches!(receipt.scope, LookupScope::Chain) {
                return None;
            }
            let outcome = match &receipt.outcome {
                Outcome::Present(member) => crate::SourceOutcome::Present {
                    winning_archive: &member.winning_archive,
                    sha256: &member.sha256,
                    bytes: member.bytes,
                },
                Outcome::Absent => crate::SourceOutcome::Absent,
                Outcome::Dummy {
                    winning_archive,
                    bytes,
                } => crate::SourceOutcome::Dummy {
                    winning_archive,
                    bytes: *bytes,
                },
            };
            Some(crate::SourceReceipt::new(&receipt.name, outcome))
        })
    }

    pub(crate) fn open(data: &Path, cancel: &dyn Fn() -> bool) -> Result<Self> {
        check_cancel(cancel)?;
        if !data.is_dir() {
            return Err(SourceError::DataDirectory);
        }
        let data = data
            .canonicalize()
            .map_err(|_| SourceError::DataDirectory)?;
        let chain = open_chain(&data, cancel)?;
        Ok(Self {
            data,
            chain,
            receipts: Vec::new(),
            read_work: 0,
        })
    }

    pub(crate) fn read(&mut self, name: &str, cancel: &dyn Fn() -> bool) -> Result<Lookup> {
        check_cancel(cancel)?;
        validate_name(name)?;
        if self.receipts.len() >= MAX_LOOKUPS {
            return Err(SourceError::LookupLimit);
        }
        let (lookup, receipt) = read_lookup(
            &self.chain,
            name,
            LookupScope::Chain,
            &mut self.read_work,
            cancel,
        )?;
        self.receipts.push(receipt);
        Ok(lookup)
    }

    /// GetFileListTo opens each archive's own listfile, independently of the
    /// composite member winner. A zero/one-byte listfile is outside the safe
    /// supported tokenizer profile, never an MPQFile dummy or ordinary absence.
    pub(crate) fn read_from_archive(
        &mut self,
        index: usize,
        name: &str,
        cancel: &dyn Fn() -> bool,
    ) -> Result<Lookup> {
        check_cancel(cancel)?;
        if index >= self.chain.archives.len() {
            return Err(SourceError::InvalidArchive);
        }
        if name != "(listfile)" {
            return Err(SourceError::InvalidName);
        }
        if self.receipts.len() >= MAX_LOOKUPS {
            return Err(SourceError::LookupLimit);
        }
        let (lookup, receipt) = read_lookup(
            &self.chain,
            name,
            LookupScope::Archive(index),
            &mut self.read_work,
            cancel,
        )?;
        self.receipts.push(receipt);
        Ok(lookup)
    }

    pub(crate) fn read_count(&self) -> usize {
        self.receipts.len()
    }
    pub(crate) fn read_work_bytes(&self) -> u64 {
        self.read_work
    }
    pub(crate) fn archives(&self) -> &[String] {
        &self.chain.names
    }
    #[cfg(test)]
    pub(crate) fn receipts(&self) -> &[LookupReceipt] {
        &self.receipts
    }

    pub(crate) fn binding_digest(&self) -> String {
        let mut digest = Sha256::new();
        hash_field(&mut digest, b"benilla-native-vmap-source-v1");
        hash_field(&mut digest, SOURCE_PIN.as_bytes());
        hash_field(&mut digest, SOURCE_POLICY.as_bytes());
        digest.update((self.chain.names.len() as u64).to_le_bytes());
        for name in &self.chain.names {
            hash_field(&mut digest, name.as_bytes());
        }
        digest.update((self.receipts.len() as u64).to_le_bytes());
        for receipt in &self.receipts {
            match receipt.scope {
                LookupScope::Chain => hash_field(&mut digest, b"chain"),
                LookupScope::Archive(index) => {
                    hash_field(&mut digest, b"archive");
                    digest.update((index as u64).to_le_bytes());
                }
            }
            hash_field(&mut digest, receipt.name.as_bytes());
            digest.update((receipt.probe_count as u64).to_le_bytes());
            match &receipt.outcome {
                Outcome::Absent => hash_field(&mut digest, b"absent"),
                Outcome::Dummy {
                    winning_archive,
                    bytes,
                } => {
                    hash_field(&mut digest, b"dummy");
                    hash_field(&mut digest, winning_archive.as_bytes());
                    digest.update((*bytes as u64).to_le_bytes());
                }
                Outcome::Present(member) => {
                    hash_field(&mut digest, b"present");
                    hash_field(&mut digest, member.winning_archive.as_bytes());
                    digest.update((member.bytes as u64).to_le_bytes());
                    hash_field(&mut digest, member.sha256.as_bytes());
                }
            }
        }
        format!("{:x}", digest.finalize())
    }

    /// Reopen every index and replay receipts without retaining fresh payloads.
    /// Missing/new mounted archives are a change even if winners would coincide.
    pub(crate) fn verify_unchanged(&self, cancel: &dyn Fn() -> bool) -> Result<()> {
        check_cancel(cancel)?;
        let chain = open_chain(&self.data, cancel)?;
        if chain.names != self.chain.names {
            return Err(SourceError::Changed);
        }
        let mut work = 0;
        for expected in &self.receipts {
            check_cancel(cancel)?;
            let (_, actual) =
                read_lookup(&chain, &expected.name, expected.scope, &mut work, cancel)?;
            if actual != *expected {
                return Err(SourceError::Changed);
            }
        }
        Ok(())
    }
}

fn check_cancel(cancel: &dyn Fn() -> bool) -> Result<()> {
    if cancel() {
        Err(SourceError::Cancelled)
    } else {
        Ok(())
    }
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > MAX_NAME_BYTES
        || !name
            .bytes()
            .all(|byte| (32..=126).contains(&byte) && !b"/:*?\"<>|".contains(&byte))
        || name
            .split('\\')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(SourceError::InvalidName);
    }
    Ok(())
}

fn archive_candidates() -> Vec<String> {
    let mut names: Vec<_> = BASE_ARCHIVES
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    names.push("patch.MPQ".to_owned());
    names.extend((2..=99).map(|number| format!("patch-{number}.MPQ")));
    names
}

fn open_chain(data: &Path, cancel: &dyn Fn() -> bool) -> Result<Chain> {
    check_cancel(cancel)?;
    let candidates = archive_candidates();
    let entries = std::fs::read_dir(data).map_err(|_| SourceError::DirectoryRead)?;
    for (index, entry) in entries.enumerate() {
        check_cancel(cancel)?;
        if index >= MAX_DIRECTORY_ENTRIES {
            return Err(SourceError::DirectoryLimit);
        }
        let entry = entry.map_err(|_| SourceError::DirectoryRead)?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if candidates
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(name) && candidate != name)
        {
            return Err(SourceError::ArchiveCase);
        }
    }
    let mut chain = Chain {
        archives: Vec::new(),
        names: Vec::new(),
    };
    let mut index_bytes = 0usize;
    for name in candidates {
        check_cancel(cancel)?;
        let path = data.join(&name);
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_file() => {}
            Ok(_) => return Err(SourceError::ArchiveNotRegular),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(SourceError::ArchiveOpen),
        }
        if chain.archives.len() >= MAX_ARCHIVES {
            return Err(SourceError::ArchiveLimit);
        }
        let archive = Archive::open_classic_bounded(
            &path,
            ReadLimits {
                index_bytes: MAX_INDEX_BYTES - index_bytes,
                sector_bytes: 64 * 1024,
                member_bytes: MAX_MEMBER_BYTES,
                header_scan_bytes: MAX_HEADER_SCAN_BYTES,
            },
        )
        .map_err(|_| SourceError::ArchiveOpen)?;
        validate_header(&archive)?;
        index_bytes = index_bytes
            .checked_add(archive.index_bytes())
            .ok_or(SourceError::IndexLimit)?;
        if index_bytes > MAX_INDEX_BYTES {
            return Err(SourceError::IndexLimit);
        }
        chain.archives.push(archive);
        chain.names.push(name);
    }
    Ok(chain)
}

/// All values below come from the safe index decode's same handle, including
/// physical length. libmpq normalizes declared header-size to 32/44; zero version
/// one extension is the supported intersection with the bounded 32-bit reader.
fn validate_header(archive: &Archive) -> Result<()> {
    let (offset, header) = archive.header_snapshot();
    let word = |at| u32::from_le_bytes(header[at..at + 4].try_into().expect("fixed header"));
    let half = |at| u16::from_le_bytes(header[at..at + 2].try_into().expect("fixed header"));
    let header_bytes = match half(12) {
        0 => 32u64,
        1 => {
            if archive.header_extension_snapshot() != Some([0; 12]) {
                return Err(SourceError::HeaderExtension);
            }
            44
        }
        _ => return Err(SourceError::HeaderVersion),
    };
    let archive_bytes = archive
        .file_len_snapshot()
        .checked_sub(offset)
        .ok_or(SourceError::HeaderSize)?;
    if offset % 512 != 0
        || u64::from(word(8)) != archive_bytes
        || offset + header_bytes > MAX_HEADER_SCAN_BYTES
    {
        return Err(SourceError::HeaderSize);
    }
    if half(14) > 7 {
        return Err(SourceError::HeaderSector);
    }
    let count = word(24);
    if !count.is_power_of_two() {
        return Err(SourceError::HeaderIndex);
    }
    let hash_start = u64::from(word(16));
    let block_start = u64::from(word(20));
    let hash_end = hash_start + u64::from(count) * 16;
    let block_end = block_start + u64::from(word(28)) * 16;
    if hash_start < header_bytes
        || block_start < header_bytes
        || hash_end > archive_bytes
        || block_end > archive_bytes
        || !(hash_end <= block_start || block_end <= hash_start)
    {
        return Err(SourceError::HeaderIndex);
    }
    Ok(())
}

fn read_lookup(
    chain: &Chain,
    name: &str,
    scope: LookupScope,
    work: &mut u64,
    cancel: &dyn Fn() -> bool,
) -> Result<(Lookup, LookupReceipt)> {
    let range = match scope {
        LookupScope::Chain => 0..chain.archives.len(),
        LookupScope::Archive(index) => index..index + 1,
    };
    let probe_count = range.len();
    for (probed, index) in range.rev().enumerate() {
        let archive = &chain.archives[index];
        let archive_name = &chain.names[index];
        check_cancel(cancel)?;
        match archive
            .probe_classic_member(name)
            .map_err(|_| SourceError::MemberProfile)?
        {
            ClassicMemberProbe::Absent => continue,
            ClassicMemberProbe::Dummy(bytes) => {
                if matches!(scope, LookupScope::Archive(_)) {
                    return Err(SourceError::DummyListfile);
                }
                let bytes = bytes as usize;
                let winning_archive = archive_name.clone();
                return Ok((
                    Lookup::Dummy {},
                    LookupReceipt {
                        scope,
                        name: name.to_owned(),
                        probe_count: probed + 1,
                        outcome: Outcome::Dummy {
                            winning_archive,
                            bytes,
                        },
                    },
                ));
            }
            ClassicMemberProbe::Present(bytes) => {
                let bytes = bytes as usize;
                if bytes > MAX_MEMBER_BYTES {
                    return Err(SourceError::MemberLimit);
                }
                let next = work
                    .checked_add(bytes as u64 * 2)
                    .ok_or(SourceError::ReadWorkLimit)?;
                if next > MAX_READ_WORK_BYTES {
                    return Err(SourceError::ReadWorkLimit);
                }
                // Charge attempted work too: a failed/cancelled read cannot be
                // retried indefinitely without consuming the acquisition budget.
                *work = next;
                check_cancel(cancel)?;
                archive
                    .verify_classic_ad_member(name)
                    .map_err(|_| SourceError::MemberProfile)?;
                check_cancel(cancel)?;
                let payload = archive
                    .read_file(name)
                    .map_err(|_| SourceError::MemberRead)?;
                if payload.len() != bytes {
                    return Err(SourceError::MemberRead);
                }
                check_cancel(cancel)?;
                let member = SourceMember {
                    name: name.to_owned(),
                    winning_archive: archive_name.clone(),
                    sha256: format!("{:x}", Sha256::digest(&payload)),
                    bytes,
                };
                let receipt = LookupReceipt {
                    scope,
                    name: name.to_owned(),
                    probe_count: probed + 1,
                    outcome: Outcome::Present(member.clone()),
                };
                return Ok((
                    Lookup::Present(CapturedMember {
                        member,
                        payload: payload.into_boxed_slice(),
                    }),
                    receipt,
                ));
            }
        }
    }
    Ok((
        Lookup::Absent,
        LookupReceipt {
            scope,
            name: name.to_owned(),
            probe_count,
            outcome: Outcome::Absent,
        },
    ))
}

fn hash_field(hash: &mut Sha256, value: &[u8]) {
    hash.update((value.len() as u64).to_le_bytes());
    hash.update(value);
}

#[cfg(test)]
#[path = "source_tests.rs"]
pub(crate) mod tests;
