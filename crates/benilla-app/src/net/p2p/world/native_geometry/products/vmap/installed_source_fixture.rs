// SPDX-License-Identifier: GPL-3.0-or-later
//! Test-only dual-parser input: nine canonical dry guards and one exterior WMO.
//! Captured source products and the complete raw inventory are built independently.
use super::captured_map_tests::{self as captured, archive::raw_entry};
use benilla_formats::{Chain, NativeM2MapInventory};
use benilla_native_terrain::Tile;
use benilla_native_vmap_source::DoodadProjectionProfile;
use std::{collections::BTreeMap, path::Path};

pub(crate) const MAP_DIRECTORY: &str = "Selected";
pub(crate) const CENTER: (u32, u32) = (40, 40);
pub(crate) const HEIGHT: f32 = -50.;
pub(crate) const WDT: &str = "World\\Maps\\Selected\\Selected.wdt";
pub(crate) const EXTERIOR_ADT: &str = "World\\Maps\\Selected\\Selected_30_31.adt";

pub(crate) fn guard_tiles() -> Vec<(u32, u32)> {
    (39..=41)
        .flat_map(|y| (39..=41).map(move |x| (x, y)))
        .collect()
}

pub(crate) fn guard_source_tiles() -> Vec<Tile> {
    guard_tiles()
        .into_iter()
        .map(|(x, y)| Tile {
            x: y as u8,
            y: x as u8,
        })
        .collect()
}

pub(crate) fn support() -> ([f32; 3], [f32; 3]) {
    ([-4276., -4276., HEIGHT], [-4280., -4276., HEIGHT])
}

fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn put_word(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

/// Preserve the complete source MVER/MHDR/MCIN/MCVT envelope and relative offsets.
/// Source extraction uses MCIN; renderer placement uses each actual MCNK header.
fn guard_adt(tile: (u32, u32)) -> Vec<u8> {
    let mut bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../benilla-native-terrain/tools/fixtures/flat.adt.test-fixture"),
    )
    .unwrap();
    for slot in 0..256 {
        let header = word(&bytes, 92 + slot * 16) as usize + 8;
        let column = word(&bytes, header + 4);
        let row = word(&bytes, header + 8);
        let base = |tile, chunk| {
            ((32. - f64::from(tile)) * (1600. / 3.) - f64::from(chunk) * (100. / 3.)) as f32
        };
        put_word(&mut bytes, header + 0x68, base(tile.1, row).to_bits());
        put_word(&mut bytes, header + 0x6c, base(tile.0, column).to_bits());
        put_word(&mut bytes, header + 0x70, HEIGHT.to_bits());
    }
    bytes
}

pub(crate) fn entries() -> Vec<(String, Vec<u8>)> {
    let mut entries: Vec<_> = captured::inventory_compatibility_tests::compatible_entries()
        .into_iter()
        .map(|entry| (entry.name, entry.bytes))
        .collect();
    let wdt = &mut entries.iter_mut().find(|(name, _)| name == WDT).unwrap().1;
    assert_eq!(&wdt[52..56], b"NIAM");
    wdt[60..60 + 64 * 64 * 8].fill(0);
    for (x, y) in guard_tiles().into_iter().chain([(30, 31)]) {
        put_word(wdt, 60 + (y * 64 + x) as usize * 8, 1);
    }
    for tile in guard_tiles() {
        entries.push((
            format!("World\\Maps\\Selected\\Selected_{}_{}.adt", tile.0, tile.1),
            guard_adt(tile),
        ));
    }
    entries
}

/// Move the genuine exterior placement and its declared source bounds into the guard.
/// Its declaring ADT stays exterior; consumers must inspect transformed geometry.
pub(crate) fn overlapping_entries() -> Vec<(String, Vec<u8>)> {
    let mut entries = entries();
    let adt = &mut entries
        .iter_mut()
        .find(|(name, _)| name == EXTERIOR_ADT)
        .unwrap()
        .1;
    let placement = 20 + word(adt, 20 + 32) as usize + 8;
    let middle = 0.5_f32 * 64. * 533.333_3_f32;
    let target = [middle + 4276., HEIGHT - 1., middle + 4278.];
    for (axis, target) in target.into_iter().enumerate() {
        let old = f32::from_bits(word(adt, placement + 8 + axis * 4));
        let delta = target - old;
        for start in [8, 32, 44] {
            let at = placement + start + axis * 4;
            let value = f32::from_bits(word(adt, at)) + delta;
            put_word(adt, at, value.to_bits());
        }
    }
    entries
}

pub(crate) struct Install(captured::Install);
impl Install {
    pub(crate) fn new(entries: &[(String, Vec<u8>)]) -> Self {
        let entries: Vec<_> = entries
            .iter()
            .map(|(name, bytes)| raw_entry(name, bytes.clone()))
            .collect();
        Self(captured::Install::new(&entries))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0 .0
    }

    pub(crate) fn bytes(&self) -> BTreeMap<String, Vec<u8>> {
        self.0.bytes()
    }

    pub(crate) fn write(&self, entries: &[(String, Vec<u8>)]) {
        let entries: Vec<_> = entries
            .iter()
            .map(|(name, bytes)| raw_entry(name, bytes.clone()))
            .collect();
        self.0.write(&entries);
    }
}

pub(crate) fn install() -> Install {
    Install::new(&entries())
}

pub(crate) fn raw_inventory(install: &Install) -> NativeM2MapInventory {
    NativeM2MapInventory::load(&mut Chain::open(install.path()).unwrap(), MAP_DIRECTORY)
        .expect("complete strict ten-tile inventory")
}

pub(crate) fn profile() -> DoodadProjectionProfile {
    captured::profile()
}
