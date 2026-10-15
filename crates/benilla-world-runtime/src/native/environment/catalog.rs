// SPDX-License-Identifier: GPL-3.0-or-later
//! Complete, bounded LiquidType.dbc bytes shared by geometry and environment state.
//! CMaNGOS 8ec338a1 DBCStructure.h/DBCfmt.h: niii is Id, LiquidId, Type, SpellId.
//! Content identity is intrinsic to these bytes; source authenticity and admission are separate.
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const MAX_ROWS: usize = 65_536; // Local resource limits, not DBC or game rules.
const MAX_STRINGS: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiquidCatalogError {
    Format,
    Framing,
    Budget,
    Unsupported,
    Duplicate,
}

/// Exact uint32 fields retained from one complete WDBC row, including a real row zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiquidTypeRow {
    pub id: u32,
    pub liquid_id: u32,
    pub kind: u32,
    pub spell_id: u32,
}

/// Immutable rows and SHA-256 of the complete input, never a source provenance claim.
/// Only actual complete bytes can construct a catalog; missing rows remain absent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiquidCatalog {
    entries: BTreeMap<u32, LiquidTypeRow>,
    digest: [u8; 32],
    complete_profile: bool,
}

impl LiquidCatalog {
    /// Full uint32 ID/Type profile used by TerrainInfo. ID u32::MAX is unsupported.
    /// Type shifts remain the selected query branch's responsibility.
    pub fn from_wdbc(bytes: &[u8]) -> Result<Self, LiquidCatalogError> {
        Self::decode_profile(bytes, true)
    }

    /// Existing conservative GridMap profile: ID <= u16::MAX and Type <= 3.
    /// This still consumes the complete WDBC, including all rows and strings.
    pub fn from_legacy_wdbc(bytes: &[u8]) -> Result<Self, LiquidCatalogError> {
        Self::decode_profile(bytes, false)
    }

    fn decode_profile(bytes: &[u8], complete: bool) -> Result<Self, LiquidCatalogError> {
        use LiquidCatalogError as Error;
        if bytes.get(..4) != Some(b"WDBC") {
            return Err(Error::Format);
        }
        let rows = u32_at(bytes, 4)? as usize;
        let fields = u32_at(bytes, 8)?;
        let stride = u32_at(bytes, 12)?;
        let strings = u32_at(bytes, 16)? as usize;
        if rows > MAX_ROWS || strings > MAX_STRINGS {
            return Err(Error::Budget);
        }
        if fields != 4 || stride != 16 {
            return Err(Error::Unsupported);
        }
        let strings_at = rows
            .checked_mul(16)
            .and_then(|n| n.checked_add(20))
            .ok_or(Error::Framing)?;
        if strings == 0
            || strings_at.checked_add(strings) != Some(bytes.len())
            || bytes.get(strings_at) != Some(&0)
            || bytes.last() != Some(&0)
        {
            return Err(Error::Framing);
        }
        let mut entries = BTreeMap::new();
        for i in 0..rows {
            let at = 20 + i * 16;
            let id = u32_at(bytes, at)?;
            let kind = u32_at(bytes, at + 8)?;
            if id == u32::MAX || (!complete && (id > u16::MAX as u32 || kind > 3)) {
                return Err(Error::Unsupported);
            }
            let row = LiquidTypeRow {
                id,
                liquid_id: u32_at(bytes, at + 4)?,
                kind,
                spell_id: u32_at(bytes, at + 12)?,
            };
            if entries.insert(id, row).is_some() {
                return Err(Error::Duplicate);
            }
        }
        Ok(Self {
            entries,
            digest: Sha256::digest(bytes).into(),
            complete_profile: complete,
        })
    }

    pub fn get(&self, id: u32) -> Option<LiquidTypeRow> {
        self.entries.get(&id).copied()
    }

    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }

    /// Whether the full uint32 profile was selected, not a source authenticity claim.
    /// Both profiles validate and hash the entire file; neither accepts partial rows.
    pub fn is_complete(&self) -> bool {
        self.complete_profile
    }
}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32, LiquidCatalogError> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or(LiquidCatalogError::Framing)?
            .try_into()
            .unwrap(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dbc(rows: &[[u32; 4]], strings: &[u8]) -> Vec<u8> {
        let mut bytes = b"WDBC".to_vec();
        for value in [rows.len() as u32, 4, 16, strings.len() as u32] {
            bytes.extend(value.to_le_bytes());
        }
        for row in rows {
            for value in row {
                bytes.extend(value.to_le_bytes());
            }
        }
        bytes.extend(strings);
        bytes
    }

    fn replace(bytes: &mut [u8], at: usize, value: u32) {
        bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn retains_uint32_spell_ids_and_distinguishes_missing_zero_and_zero_spell_rows() {
        let rows = [[0, 23, 3, 777], [22, 41, 2, 0], [70_000, 35, 31, u32::MAX]];
        let catalog = LiquidCatalog::from_wdbc(&dbc(&rows, b"\0")).unwrap();
        for [id, liquid_id, kind, spell_id] in rows {
            assert_eq!(
                catalog.get(id),
                Some(LiquidTypeRow {
                    id,
                    liquid_id,
                    kind,
                    spell_id
                })
            );
        }
        assert_eq!(catalog.get(1), None);
        assert_eq!(catalog.get(u32::MAX), None);
        assert!(catalog.is_complete());
    }

    #[test]
    fn full_byte_digest_binds_every_column_and_the_entire_string_block() {
        let bytes = dbc(&[[22, 23, 3, 456]], b"\0name\0");
        let catalog = LiquidCatalog::from_wdbc(&bytes).unwrap();
        assert_eq!(catalog.digest(), <[u8; 32]>::from(Sha256::digest(&bytes)));
        for at in [20, 24, 28, 32] {
            let mut changed = bytes.clone();
            replace(&mut changed, at, u32_at(&bytes, at).unwrap() + 1);
            let changed_catalog = LiquidCatalog::from_wdbc(&changed).unwrap();
            assert_ne!(catalog.digest(), changed_catalog.digest(), "column {at}");
        }
        let mut changed = bytes.clone();
        changed[37] = b'N';
        let changed_catalog = LiquidCatalog::from_wdbc(&changed).unwrap();
        assert_eq!(catalog.get(22), changed_catalog.get(22));
        assert_ne!(catalog.digest(), changed_catalog.digest());
        let legacy = LiquidCatalog::from_legacy_wdbc(&bytes).unwrap();
        assert!(!legacy.is_complete());
        assert_eq!(legacy.get(22), catalog.get(22));
        assert_eq!(legacy.digest(), catalog.digest());
    }

    #[test]
    fn exact_existing_framing_budgets_and_profile_error_order_are_preserved() {
        use LiquidCatalogError as Error;
        let bytes = dbc(&[[22, 23, 3, u32::MAX]], b"\0");
        for end in 0..bytes.len() {
            assert!(
                LiquidCatalog::from_wdbc(&bytes[..end]).is_err(),
                "prefix {end}"
            );
        }
        for (at, value, error) in [
            (4, 65_537, Error::Budget),
            (8, 3, Error::Unsupported),
            (12, 12, Error::Unsupported),
            (16, 0, Error::Framing),
            (16, 1_048_577, Error::Budget),
            (20, u32::MAX, Error::Unsupported),
        ] {
            let mut bad = bytes.clone();
            replace(&mut bad, at, value);
            assert_eq!(LiquidCatalog::from_wdbc(&bad), Err(error));
        }
        for at in [20, 28] {
            let mut wide = bytes.clone();
            replace(&mut wide, at, 70_000);
            assert!(LiquidCatalog::from_wdbc(&wide).is_ok());
            assert_eq!(
                LiquidCatalog::from_legacy_wdbc(&wide),
                Err(Error::Unsupported)
            );
        }
        let unsupported_then_duplicate = dbc(&[[22, 0, 4, 0], [22, 0, 0, 0]], b"\0");
        assert_eq!(
            LiquidCatalog::from_legacy_wdbc(&unsupported_then_duplicate),
            Err(Error::Unsupported)
        );
        assert_eq!(
            LiquidCatalog::from_wdbc(&unsupported_then_duplicate),
            Err(Error::Duplicate)
        );
        let duplicate_then_unsupported = dbc(&[[22, 0, 0, 0], [22, 0, 0, 0], [23, 0, 4, 0]], b"\0");
        assert_eq!(
            LiquidCatalog::from_legacy_wdbc(&duplicate_then_unsupported),
            Err(Error::Duplicate)
        );
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(LiquidCatalog::from_wdbc(&trailing), Err(Error::Framing));
        let mut bad_strings = bytes;
        *bad_strings.last_mut().unwrap() = 1;
        assert_eq!(LiquidCatalog::from_wdbc(&bad_strings), Err(Error::Framing));
    }
}
