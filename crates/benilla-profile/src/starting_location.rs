//! Position-only projection of an existing private Classic source-v2 capture.
//! No SQL, subprocess, compiler, native gameplay admission, or bundled world rows.
//! This deliberately does not validate unrelated world tables: use the existing
//! `classic-export.py validate` for the complete source-capture contract.
use crate::{validate_race_class, Error, Position};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path};

/// Same byte ceiling as the existing read-only source capture format.
pub const MAX_CAPTURE_BYTES: usize = 64 * 1024 * 1024;
const CORE: &str = "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f";
const DATABASE: &str = "ec4f596146be6467ea93c57397858e329e2db852";

/// A selected, bounded local start, not a validated or trusted world database.
/// The exact-byte digest identifies local input; it cannot establish its authorship.
#[derive(Clone, Debug)]
pub struct ClassicStart {
    race: u8,
    class: u8,
    position: Position,
    capture_sha256: [u8; 32],
}
impl ClassicStart {
    /// Read an explicitly configured local capture. Reject special files and final symlinks.
    /// Parent symlinks may be deliberately selected in local configuration; no file is written.
    pub fn read_path(path: &Path) -> Result<Self, Error> {
        let metadata = std::fs::symlink_metadata(path)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(Error::Invalid(
                "Classic start capture must be a regular file",
            ));
        }
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(0x00200000); // FILE_FLAG_OPEN_REPARSE_POINT
        }
        let file = options.open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() > MAX_CAPTURE_BYTES as u64 {
            return Err(Error::Invalid(
                "Classic start capture must be a bounded regular file",
            ));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(Error::Invalid(
                    "Classic start capture cannot be a reparse point",
                ));
            }
        }
        Self::read(file)
    }

    /// At most MAX_CAPTURE_BYTES + 1 bytes are consumed. No paths are opened here.
    pub fn read(reader: impl Read) -> Result<Self, Error> {
        let mut bytes = Vec::new();
        reader
            .take((MAX_CAPTURE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        Self::from_json(&bytes)
    }
    pub fn from_json(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_CAPTURE_BYTES {
            return Err(Error::Invalid(
                "Classic start capture exceeds its byte limit",
            ));
        }
        let capture: Capture = serde_json::from_slice(bytes)
            .map_err(|_| Error::Invalid("invalid Classic start capture"))?;
        if capture.format != "benilla-classic-source-slice-v2"
            || capture.core_revision != CORE
            || capture.database_revision != DATABASE
            || capture.client_build != 5875
            || capture.revision_claims_verified_against_installation
            || capture.selection.format_version != 2
            || !(1..=60).contains(&capture.selection.starter.level_cap)
            || !matches!(
                capture.consistency.as_str(),
                "transactional_snapshot" | "operator_quiescent_snapshot"
            )
            || capture.report.runnable
            || capture.report.capture_complete
        {
            return Err(Error::Invalid("unsupported Classic start source profile"));
        }
        let [row] = &capture.tables.playercreateinfo;
        validate_race_class(row.race, row.class)?;
        if (row.race, row.class)
            != (
                capture.selection.starter.race,
                capture.selection.starter.class,
            )
        {
            return Err(Error::Invalid(
                "Classic start does not match the capture selection",
            ));
        }
        // The captured row has only ASCII keys and numeric decimal strings. Sorted JSON
        // keys, compact separators and no floats match classic_export.core.canonical.
        let mut normal = serde_json::to_value(&capture.tables.playercreateinfo)
            .map_err(|_| Error::Invalid("invalid Classic start row"))?;
        normal.sort_all_objects();
        let normal =
            serde_json::to_vec(&normal).map_err(|_| Error::Invalid("invalid Classic start row"))?;
        let hash = format!("{:x}", Sha256::digest(&normal));
        if hash != capture.table_sha256.playercreateinfo {
            return Err(Error::Invalid("Classic start row checksum mismatch"));
        }
        let position = Position {
            map_id: row.map,
            zone_id: row.zone,
            position: [
                row.position_x.value()?,
                row.position_y.value()?,
                row.position_z.value()?,
            ],
            orientation: row.orientation.value()?,
        };
        position.validate()?;
        Ok(Self {
            race: row.race,
            class: row.class,
            position,
            capture_sha256: Sha256::digest(bytes).into(),
        })
    }
    pub fn position_for(&self, race: u8, class: u8) -> Result<Position, Error> {
        validate_race_class(race, class)?;
        if (race, class) != (self.race, self.class) {
            return Err(Error::Invalid("no captured start for this race/class"));
        }
        Ok(self.position)
    }
    pub fn capture_sha256(&self) -> [u8; 32] {
        self.capture_sha256
    }
}

// Only fields involved in this position projection are interpreted. Other capture
// fields are skipped by serde, never treated as validated gameplay input.
#[derive(Deserialize)]
struct Capture {
    format: String,
    core_revision: String,
    database_revision: String,
    client_build: u32,
    revision_claims_verified_against_installation: bool,
    selection: Selection,
    consistency: String,
    tables: Tables,
    table_sha256: Hashes,
    report: Report,
}
#[derive(Deserialize)]
struct Selection {
    format_version: u32,
    starter: Starter,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Starter {
    race: u8,
    class: u8,
    level_cap: u8,
}
#[derive(Deserialize)]
struct Tables {
    playercreateinfo: [Row; 1],
}
#[derive(Deserialize)]
struct Hashes {
    playercreateinfo: String,
}
#[derive(Deserialize)]
struct Report {
    runnable: bool,
    capture_complete: bool,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Row {
    race: u8,
    class: u8,
    map: u32,
    zone: u32,
    position_x: Decimal,
    position_y: Decimal,
    position_z: Decimal,
    orientation: Decimal,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Decimal {
    decimal: String,
}
impl Decimal {
    fn value(&self) -> Result<f32, Error> {
        let invalid = || Error::Invalid("invalid Classic start decimal");
        let text = self.decimal.as_str();
        let unsigned = text.strip_prefix('-').unwrap_or(text);
        let mut parts = unsigned.split('.');
        let whole = parts.next().ok_or_else(invalid)?;
        let fractional = parts.next();
        if text.len() > 96
            || whole.is_empty()
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || (whole.len() > 1 && whole.starts_with('0'))
            || parts.next().is_some()
            || fractional.is_some_and(|f| {
                f.is_empty() || f.ends_with('0') || !f.bytes().all(|b| b.is_ascii_digit())
            })
            || text == "-0"
        {
            return Err(invalid());
        }
        let value: f32 = text.parse().map_err(|_| invalid())?;
        if !value.is_finite() {
            return Err(invalid());
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests;
