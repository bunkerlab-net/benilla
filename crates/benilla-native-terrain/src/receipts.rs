// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only views of receipts already retained by a genuine terrain capture.

use crate::source::CapturedSource;

/// Borrowed evidence for one consumed name, never a constructor for a capture.
/// This is an observation from preparation, not a live-install lease or proof
/// that unrelated files in two installations agree.
///
/// Callers cannot construct receipts from supplied metadata:
/// ```compile_fail
/// use benilla_native_terrain::{SourceOutcome, SourceReceipt};
/// let receipt = SourceReceipt {
///     name: "DBFilesClient\\Map.dbc",
///     outcome: SourceOutcome::Absent,
/// };
/// ```
/// The borrowed view cannot mutate retained source evidence:
/// ```compile_fail
/// use benilla_native_terrain::PreparedTerrain;
/// fn change_name(prepared: &PreparedTerrain) {
///     let receipt = prepared.source_receipts().next().unwrap();
///     let name: &mut str = receipt.name();
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceReceipt<'a> {
    name: &'a str,
    outcome: SourceOutcome<'a>,
}

impl<'a> SourceReceipt<'a> {
    pub fn name(&self) -> &'a str {
        self.name
    }

    pub fn outcome(&self) -> SourceOutcome<'a> {
        self.outcome
    }
}

/// Captured dual-resolver presence or absence for a consumed canonical name.
/// Terrain rejects dummy hits; they cannot become either outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceOutcome<'a> {
    Present {
        winning_archive: &'a str,
        sha256: &'a str,
        bytes: usize,
    },
    Absent,
}

pub(crate) fn iter(source: &CapturedSource) -> impl Iterator<Item = SourceReceipt<'_>> {
    source
        .members()
        .iter()
        .map(|member| SourceReceipt {
            name: &member.name,
            outcome: SourceOutcome::Present {
                winning_archive: &member.winning_archive,
                sha256: &member.sha256,
                bytes: member.bytes,
            },
        })
        .chain(source.absent_members().iter().map(|member| SourceReceipt {
            name: &member.name,
            outcome: SourceOutcome::Absent,
        }))
}

#[cfg(test)]
mod tests {
    use super::SourceOutcome;
    use crate::{map_fixture::map_dbc, test_support::*};
    use crate::{prepare, PreparedTerrain, Tile, DBCS};
    use sha2::{Digest, Sha256};
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    const SELECTED: Tile = Tile { x: 31, y: 32 };
    const WDT: &str = "World\\Maps\\AuthoredOracle\\AuthoredOracle.wdt";
    const ADT: &str = "World\\Maps\\AuthoredOracle\\AuthoredOracle_32_31.adt";
    const HISTORY_WDT: &str = "World\\Maps\\Earlier\\Earlier.wdt";
    const HISTORY_ADT: &str = "World\\Maps\\Earlier\\Earlier_0_0.adt";
    const ABSENT_WDT: &str = "World\\Maps\\Absent\\Absent.wdt";
    const ABSENT_ADT: &str = "World\\Maps\\AuthoredOracle\\AuthoredOracle_31_31.adt";

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "native-terrain-receipts-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(root.join("Data")).unwrap();
            Self(root)
        }

        fn data(&self) -> PathBuf {
            self.0.join("Data")
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn entries(adt: &[u8]) -> Vec<Entry> {
        vec![
            raw_entry(
                DBCS[0],
                include_bytes!("../tools/fixtures/Map.dbc.test-fixture").to_vec(),
            ),
            raw_entry(
                DBCS[1],
                include_bytes!("../tools/fixtures/AreaTable.dbc.test-fixture").to_vec(),
            ),
            raw_entry(
                DBCS[2],
                include_bytes!("../tools/fixtures/LiquidType.dbc.test-fixture").to_vec(),
            ),
            raw_entry(
                WDT,
                include_bytes!("../tools/fixtures/source.wdt.test-fixture").to_vec(),
            ),
            raw_entry(ADT, adt.to_vec()),
        ]
    }

    fn wdt(tiles: &[Tile]) -> Vec<u8> {
        let mut bytes = include_bytes!("../tools/fixtures/source.wdt.test-fixture").to_vec();
        assert_eq!(&bytes[52..56], b"NIAM");
        bytes[60..60 + 64 * 64 * 8].fill(0);
        for tile in tiles {
            let offset = 60 + (usize::from(tile.x) * 64 + usize::from(tile.y)) * 8;
            bytes[offset..offset + 4].copy_from_slice(&1u32.to_le_bytes());
        }
        bytes
    }

    fn assert_exact_borrowed_receipts(prepared: &PreparedTerrain) {
        let source = &prepared.source;
        let receipts: Vec<_> = prepared.source_receipts().collect();
        assert_eq!(
            receipts.len(),
            source.members().len() + source.absent_members().len()
        );
        for (receipt, member) in receipts.iter().zip(source.members()) {
            assert!(std::ptr::eq(receipt.name(), member.name.as_str()));
            let SourceOutcome::Present {
                winning_archive,
                sha256,
                bytes,
            } = receipt.outcome()
            else {
                panic!("positive receipt must retain its outcome");
            };
            assert!(std::ptr::eq(
                winning_archive,
                member.winning_archive.as_str()
            ));
            assert!(std::ptr::eq(sha256, member.sha256.as_str()));
            assert_eq!(bytes, member.bytes);
        }
        for (receipt, member) in receipts[source.members().len()..]
            .iter()
            .zip(source.absent_members())
        {
            assert!(std::ptr::eq(receipt.name(), member.name.as_str()));
            assert_eq!(receipt.outcome(), SourceOutcome::Absent);
        }
    }

    #[test]
    fn positive_receipts_borrow_exact_winners_without_reading_or_rebinding() {
        let fixture = Fixture::new();
        let raw = include_bytes!("../tools/fixtures/flat.adt.test-fixture");
        let path = fixture.data().join("terrain.MPQ");
        fs::write(&path, archive(&entries(raw))).unwrap();
        let patch = fixture.data().join("patch.MPQ");
        fs::write(&patch, archive(&[raw_entry(ADT, raw.to_vec())])).unwrap();
        let prepared = prepare(&fixture.data(), 1, &[SELECTED]).unwrap();
        let epoch = prepared.source_epoch().to_owned();
        assert_eq!(prepared.source_policy(), crate::source::SOURCE_POLICY);
        assert_eq!(prepared.map_directory(), "AuthoredOracle");
        assert_eq!(prepared.source_receipts().count(), 5);
        assert_exact_borrowed_receipts(&prepared);
        let selected = prepared
            .source_receipts()
            .find(|receipt| receipt.name() == ADT)
            .unwrap();
        assert_eq!(
            selected.outcome(),
            SourceOutcome::Present {
                winning_archive: "patch.MPQ",
                sha256: &format!("{:x}", Sha256::digest(raw)),
                bytes: raw.len(),
            }
        );
        prepared.verify_unchanged().unwrap();
        fs::remove_file(path).unwrap();
        fs::remove_file(patch).unwrap();
        assert_exact_borrowed_receipts(&prepared);
        assert_eq!(prepared.source_epoch(), epoch);
        assert_eq!(prepared.map_directory(), "AuthoredOracle");
        assert!(prepared.verify_unchanged().is_err());
    }

    #[test]
    fn receipts_include_positive_and_absent_history_but_no_unconsumed_members() {
        let fixture = Fixture::new();
        let mut entries = entries(include_bytes!(
            "../tools/fixtures/liquid_history61_positive.adt.test-fixture"
        ));
        entries[0] = raw_entry(
            DBCS[0],
            map_dbc(&[(700, b"Earlier"), (800, b"Absent"), (1, b"AuthoredOracle")]),
        );
        entries[3] = raw_entry(WDT, wdt(&[Tile { x: 31, y: 31 }, SELECTED]));
        entries.extend([
            raw_entry(HISTORY_WDT, wdt(&[Tile { x: 0, y: 0 }])),
            raw_entry(
                HISTORY_ADT,
                include_bytes!("../tools/fixtures/liquid_water_flat.adt.test-fixture").to_vec(),
            ),
            raw_entry("World\\Maps\\Unused\\Unused.wdt", vec![0; 8]),
        ]);
        fs::write(
            fixture.data().join("terrain.MPQ"),
            version_one_archive(&entries),
        )
        .unwrap();
        let prepared = prepare(&fixture.data(), 1, &[SELECTED]).unwrap();
        assert_exact_borrowed_receipts(&prepared);
        let receipts: Vec<_> = prepared.source_receipts().collect();
        assert_eq!(receipts.len(), 9);
        for name in DBCS.into_iter().chain([WDT, ADT, HISTORY_WDT, HISTORY_ADT]) {
            assert!(matches!(
                receipts
                    .iter()
                    .find(|receipt| receipt.name() == name)
                    .unwrap()
                    .outcome(),
                SourceOutcome::Present { .. }
            ));
        }
        for name in [ABSENT_WDT, ABSENT_ADT] {
            assert_eq!(
                receipts
                    .iter()
                    .find(|receipt| receipt.name() == name)
                    .unwrap()
                    .outcome(),
                SourceOutcome::Absent
            );
        }
        assert_eq!(
            prepared.tiles().collect::<Vec<_>>(),
            [(
                SELECTED,
                include_bytes!("../tools/fixtures/liquid_history61_positive.map").as_slice()
            )]
        );
        prepared.verify_unchanged().unwrap();
    }
}
