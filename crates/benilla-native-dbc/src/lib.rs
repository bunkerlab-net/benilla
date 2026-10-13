// SPDX-License-Identifier: GPL-3.0-or-later
//! Fixed, bounded, read-only extraction through the application's existing MPQ chain.
//! No separate archive/locale precedence, no asset download, and no gameplay activation.

use anyhow::{bail, ensure, Context, Result};
use benilla_formats::{Chain, ChainLimits};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;

pub const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_ALL_BYTES: usize = 192 * 1024 * 1024;
/// Header field count and actual record size are distinct for packed tables.
pub const TABLES: &[(&str, u32, u32)] = &[
    ("CharBaseInfo", 2, 2),
    ("CharStartOutfit", 41, 152),
    ("ChrClasses", 17, 68),
    ("ChrRaces", 29, 116),
    ("CreatureDisplayInfo", 12, 48),
    ("Faction", 37, 148),
    ("FactionTemplate", 14, 56),
    ("Map", 42, 168),
    ("SkillLine", 22, 88),
    ("SkillLineAbility", 15, 60),
    ("SkillRaceClassInfo", 8, 32),
    ("SkillTiers", 33, 132),
    ("Spell", 173, 692),
    ("SpellCastTimes", 4, 16),
    ("SpellCategory", 2, 8),
    ("SpellDuration", 4, 16),
    ("SpellFocusObject", 10, 40),
    ("SpellRadius", 4, 16),
    ("SpellRange", 22, 88),
    ("Talent", 21, 84),
];

#[derive(Debug, Clone, Serialize)]
pub struct ExtractedFile {
    pub file: String,
    /// Basename only; whole extracted bytes, not this label, bind the input.
    pub winning_archive: String,
    pub sha256: String,
    pub bytes: usize,
    pub records: u32,
    pub fields: u32,
    pub record_bytes: u32,
}
#[derive(Debug, Serialize)]
pub struct Manifest {
    pub format: &'static str,
    pub chain_policy: &'static str,
    pub client_build_claim: u32,
    pub client_build_authenticity_verified: bool,
    pub files: Vec<ExtractedFile>,
}

fn layout(bytes: &[u8], fields: u32, size: u32) -> Result<u32> {
    ensure!(
        (20..=MAX_FILE_BYTES).contains(&bytes.len()),
        "dbc.byte_limit_or_header"
    );
    ensure!(&bytes[..4] == b"WDBC", "dbc.magic");
    let word = |i| u32::from_le_bytes(bytes[i..i + 4].try_into().expect("bounded header"));
    let records = word(4);
    ensure!(records <= 100_000, "dbc.record_limit");
    ensure!(word(8) == fields && word(12) == size, "dbc.layout");
    let expected = 20_u64 + u64::from(records) * u64::from(size) + u64::from(word(16));
    ensure!(expected == bytes.len() as u64, "dbc.exact_length");
    ensure!(word(16) > 0 && bytes.last() == Some(&0), "dbc.string_block");
    let string_start = 20 + records as usize * size as usize;
    ensure!(bytes[string_start] == 0, "dbc.string_block");
    Ok(records)
}

/// Extract one explicit Data directory. Uses the app's same-precedence chain's
/// opt-in bounded API, including locale-bearing base archives and tombstones.
/// The caller chooses a new private output directory outside the input directory.
/// Missing/corrupt members fail before any output directory is created.
pub fn extract(data: &Path, output: &Path) -> Result<Manifest> {
    ensure!(data.is_dir(), "input.data_directory_required");
    let data = data.canonicalize().context("input.data_directory")?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent.canonicalize().context("output.parent_must_exist")?;
    ensure!(!parent.starts_with(&data), "output.inside_install_data");
    let name = output.file_name().context("output.directory_name")?;
    let output = parent.join(name);
    ensure!(!output.try_exists()?, "output.already_exists");
    let chain = Chain::open_bounded(
        &data,
        ChainLimits {
            index_bytes: 64 * 1024 * 1024,
            sector_bytes: 64 * 1024,
            member_bytes: MAX_FILE_BYTES,
            header_scan_bytes: 1024 * 1024,
            archives: 64,
            directory_entries: 4096,
        },
    )
    .context("input.open_patch_chain")?;
    let mut payloads = Vec::with_capacity(TABLES.len());
    let mut files = Vec::with_capacity(TABLES.len());
    let mut total = 0;
    for &(name, fields, size) in TABLES {
        let internal = format!("DBFilesClient\\{name}.dbc");
        let archive = chain
            .archive_for(&internal)
            .with_context(|| format!("input.missing:{name}"))?;
        let length = archive.file_size(&internal).context("input.missing_size")? as usize;
        ensure!(length <= MAX_FILE_BYTES, "input.member_byte_limit:{name}");
        total += length;
        ensure!(total <= MAX_ALL_BYTES, "input.aggregate_byte_limit");
        let bytes = archive
            .read_file(&internal)
            .with_context(|| format!("input.read:{name}"))?;
        ensure!(bytes.len() == length, "input.member_length_changed:{name}");
        let records =
            layout(&bytes, fields, size).with_context(|| format!("input.invalid:{name}"))?;
        let file = format!("{name}.dbc");
        files.push(ExtractedFile {
            file: file.clone(),
            winning_archive: archive
                .path()
                .file_name()
                .context("input.archive_basename")?
                .to_string_lossy()
                .into_owned(),
            sha256: format!("{:x}", Sha256::digest(&bytes)),
            bytes: bytes.len(),
            records,
            fields,
            record_bytes: size,
        });
        payloads.push((file, bytes));
    }
    let manifest = Manifest {
        format: "benilla-native-dbc-extraction-v3",
        chain_policy: "benilla-formats-vanilla-chain",
        client_build_claim: 5875,
        client_build_authenticity_verified: false,
        files,
    };
    publish(&output, &payloads, &serde_json::to_vec(&manifest)?)?;
    Ok(manifest)
}

fn new_private(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}
fn sync_dir(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path; // Files are synced; directory durability is not asserted here.
    Ok(())
}
fn publish(output: &Path, payloads: &[(String, Vec<u8>)], manifest: &[u8]) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(output)
        .context("output.create_new_directory")?;
    let mut published = Vec::new();
    let result = (|| -> Result<()> {
        for (name, bytes) in payloads {
            ensure!(
                TABLES
                    .iter()
                    .any(|(table, _, _)| name == &format!("{table}.dbc")),
                "output.unreviewed_file"
            );
            let path = output.join(name);
            let mut file = new_private(&path)?;
            published.push(path);
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        // Complete marker last; a process crash can leave a private incomplete
        // directory, which must be retried with a new destination, never overwritten.
        let path = output.join("extraction.json");
        let mut file = new_private(&path)?;
        published.push(path);
        file.write_all(manifest)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        sync_dir(output)?;
        sync_dir(output.parent().context("output.parent")?)?;
        Ok(())
    })();
    if result.is_err() {
        // Remove only files this invocation created, never arbitrary user files.
        for path in published.iter().rev() {
            let _ = fs::remove_file(path);
        }
        let _ = fs::remove_dir(output);
        bail!("output.publication_failed; no complete extraction confirmed");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Temp(std::path::PathBuf);
    impl Temp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "benilla-native-dbc-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn packed_header_layouts_are_not_assumed_to_be_words() {
        for &(name, fields, size) in TABLES {
            let mut bytes = b"WDBC".to_vec();
            for word in [1u32, fields, size, 1] {
                bytes.extend(word.to_le_bytes());
            }
            bytes.resize(20 + size as usize + 1, 0);
            assert_eq!(layout(&bytes, fields, size).unwrap(), 1, "{name}");
            bytes.push(0);
            assert!(layout(&bytes, fields, size).is_err());
        }
    }
    #[test]
    fn publication_is_private_and_never_clobbers() {
        let temp = Temp::new();
        let dest = temp.0.join("new");
        let payloads = vec![("Map.dbc".into(), vec![1, 2, 3])];
        publish(&dest, &payloads, b"{}").unwrap();
        assert_eq!(fs::read(dest.join("Map.dbc")).unwrap(), [1, 2, 3]);
        assert!(publish(&dest, &payloads, b"changed").is_err());
        assert_eq!(fs::read(dest.join("extraction.json")).unwrap(), b"{}\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&dest).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(dest.join("Map.dbc"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
    #[test]
    fn unreviewed_output_name_cannot_escape_directory() {
        let temp = Temp::new();
        let dest = temp.0.join("new");
        assert!(publish(&dest, &[("../outside".into(), vec![1])], b"{}").is_err());
        assert!(!temp.0.join("outside").exists());
        assert!(!dest.exists());
    }
    #[test]
    fn input_missing_or_output_inside_data_creates_nothing() {
        let temp = Temp::new();
        let data = temp.0.join("Data");
        fs::create_dir(&data).unwrap();
        let output = data.join("forbidden");
        assert!(extract(&data, &output).is_err());
        assert!(!output.exists());
        let output = temp.0.join("outside");
        assert!(extract(&data, &output).is_err());
        assert!(!output.exists());
    }
}
