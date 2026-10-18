// SPDX-License-Identifier: GPL-3.0-or-later
//! Selected terrain production, never world-content compilation or admission.
//! All source reads use immutable captured buffers. A final fresh-chain comparison
//! detects changed selected inputs; it is not a filesystem lock or a live source lease.
mod diagnostic_id;
mod diagnostics;
mod source;
pub use diagnostic_id::diagnostic_id;
pub use diagnostics::{diagnose, DiagnosticReport};
pub mod writer;
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use source::CapturedSource;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
};

pub const SOURCE_PIN: &str = "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f";
const DBCS: [&str; 3] = [
    "DBFilesClient\\Map.dbc",
    "DBFilesClient\\AreaTable.dbc",
    "DBFilesClient\\LiquidType.dbc",
];

/// Terrain grid order, which is reversed relative to the ADT filename pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Tile {
    pub x: u8,
    pub y: u8,
}
#[derive(Serialize)]
struct Product {
    file: String,
    bytes: usize,
    sha256: String,
    tile: Tile,
}
#[derive(Serialize)]
struct Manifest<'a> {
    format: &'static str,
    source_pin: &'static str,
    source_resolution_policy: &'static str,
    writer_policy: &'static str,
    source_epoch: &'a str,
    live_install_lease: bool,
    gameplay_admitted: bool,
    map: u32,
    members: &'a [source::SourceMember],
    absent_members: &'a [source::AbsentSourceMember],
    products: &'a [Product],
}
/// Owns the exact consumed raw bytes and their generated products. No serialized
/// manifest can construct this value. Terrain output alone grants no geometry authority.
pub struct PreparedTerrain {
    source: CapturedSource,
    map: u32,
    products: Vec<(Product, Vec<u8>)>,
}
impl PreparedTerrain {
    pub fn source_epoch(&self) -> &str {
        self.source.binding_digest()
    }
    pub fn map_id(&self) -> u32 {
        self.map
    }
    pub fn tiles(&self) -> impl Iterator<Item = (Tile, &[u8])> {
        self.products
            .iter()
            .map(|(p, bytes)| (p.tile, bytes.as_slice()))
    }
    pub fn verify_unchanged(&self) -> Result<()> {
        self.source.verify_unchanged()
    }
    /// Publish a fresh private directory, manifest last. Failure never replaces an
    /// earlier export. A crash may leave an incomplete directory without a manifest.
    /// The manifest is an audit record, not a trusted loader capability.
    pub fn publish(self, output: &Path) -> Result<()> {
        self.publish_observed(output, |_| Ok(()))
    }
    fn publish_observed(
        self,
        output: &Path,
        mut observe: impl FnMut(PublishStage) -> Result<()>,
    ) -> Result<()> {
        let parent = output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let parent = parent.canonicalize().context("output.parent_must_exist")?;
        self.source.check_output_parent(&parent)?;
        let output = parent.join(output.file_name().context("output.directory_name")?);
        ensure!(!output.try_exists()?, "output.already_exists");
        self.source.verify_unchanged()?;
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(&output)
            .context("output.create_new_directory")?;
        builder.create(output.join("maps"))?;
        // Only paths produced internally from validated numeric map/tile IDs are written.
        for (product, bytes) in &self.products {
            write_new(&output.join(&product.file), bytes)?;
        }
        sync_dir(&output.join("maps"))?;
        observe(PublishStage::ProductsWritten)?;
        // Re-read staged bytes, then fresh source winners before manifest publication.
        for (product, bytes) in &self.products {
            verify_product(&output.join(&product.file), bytes)?;
        }
        self.source.verify_unchanged()?;
        let products: Vec<Product> = self
            .products
            .iter()
            .map(|(p, _)| Product {
                file: p.file.clone(),
                bytes: p.bytes,
                sha256: p.sha256.clone(),
                tile: p.tile,
            })
            .collect();
        let manifest = Manifest {
            format: "benilla-terrain-production-v3",
            source_pin: SOURCE_PIN,
            source_resolution_policy: source::SOURCE_POLICY,
            writer_policy: "cmangos-ad-f0-boundary-history-closure-v3",
            source_epoch: self.source_epoch(),
            live_install_lease: false,
            gameplay_admitted: false,
            map: self.map,
            members: self.source.members(),
            absent_members: self.source.absent_members(),
            products: &products,
        };
        observe(PublishStage::BeforeManifest)?;
        // The test observer may mutate inputs; production observer is inert.
        self.source.verify_unchanged()?;
        for (product, bytes) in &self.products {
            verify_product(&output.join(&product.file), bytes)?;
        }
        let pending_manifest = output.join("terrain.pending");
        write_new(&pending_manifest, &serde_json::to_vec(&manifest)?)?;
        // Atomic no-replace visibility of a completely written and synced marker.
        fs::hard_link(&pending_manifest, output.join("terrain.json"))?;
        // From here a complete marker is visible. An error cannot claim rollback.
        (|| -> Result<()> {
            observe(PublishStage::ManifestLinked)?;
            fs::remove_file(pending_manifest)?;
            sync_dir(&output)?;
            sync_dir(&parent)?;
            Ok(())
        })()
        .context("output.published_but_durability_unconfirmed")?;
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PublishStage {
    ProductsWritten,
    BeforeManifest,
    ManifestLinked,
}
fn verify_product(path: &Path, expected: &[u8]) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.file_type().is_file() && metadata.len() == expected.len() as u64,
        "output.changed_product"
    );
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let file = options.open(path)?;
    let opened = file.metadata()?;
    ensure!(
        opened.is_file() && opened.len() == expected.len() as u64,
        "output.changed_product"
    );
    let mut bytes = Vec::with_capacity(expected.len());
    file.take(expected.len() as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes == expected, "output.changed_product");
    Ok(())
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
fn sync_dir(path: &Path) -> Result<()> {
    #[cfg(unix)]
    fs::File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
/// Capture at most nine selected output tiles. Missing emitted liquid boundary
/// heights trigger a bounded, exact predecessor dependency closure. History inputs
/// do not become output tiles or certify a gameplay corridor.
pub fn prepare(data: &Path, map: u32, tiles: &[Tile]) -> Result<PreparedTerrain> {
    let ordered = validate_selection(map, tiles)?;
    let initial = CapturedSource::capture(data, &DBCS)?;
    let directory = writer::map_directory(initial.bytes(DBCS[0])?, map)?;
    let wdt = wdt_name(&directory);
    let selection = capture_wdt(data, &initial, &directory)?;
    drop(initial);
    for tile in &ordered {
        writer::validate_selection_wdt(selection.bytes(&wdt)?, tile.x, tile.y)?;
    }
    let mut source = capture_tiles(data, &selection, &directory, &ordered)?;
    drop(selection);
    let mut products = Vec::with_capacity(ordered.len());
    for tile in ordered {
        let (bytes, _) = produce_tile_with_history(&mut source, map, &directory, tile)?;
        let product = Product {
            file: format!("maps/{map:03}{:02}{:02}.map", tile.x, tile.y),
            bytes: bytes.len(),
            sha256: format!("{:x}", Sha256::digest(&bytes)),
            tile,
        };
        products.push((product, bytes));
    }
    source.verify_unchanged()?;
    Ok(PreparedTerrain {
        source,
        map,
        products,
    })
}

fn validate_selection(map: u32, tiles: &[Tile]) -> Result<Vec<Tile>> {
    ensure!(
        map <= 999 && (1..=9).contains(&tiles.len()),
        "selection.bounds"
    );
    ensure!(
        tiles.iter().all(|t| t.x < 64 && t.y < 64),
        "selection.tile_bounds"
    );
    let mut ordered = tiles.to_vec();
    ordered.sort();
    ensure!(
        ordered.windows(2).all(|pair| pair[0] != pair[1]),
        "selection.duplicate_tile"
    );
    Ok(ordered)
}

fn wdt_name(directory: &str) -> String {
    format!("World\\Maps\\{directory}\\{directory}.wdt")
}

fn adt_name(directory: &str, tile: Tile) -> String {
    format!(
        "World\\Maps\\{directory}\\{directory}_{}_{}.adt",
        tile.y, tile.x
    )
}

fn capture_wdt(data: &Path, initial: &CapturedSource, directory: &str) -> Result<CapturedSource> {
    let mut names: Vec<String> = DBCS.iter().map(|s| (*s).to_owned()).collect();
    names.push(wdt_name(directory));
    let selection = CapturedSource::capture(data, &names)?;
    selection.verify_contains(initial)?;
    initial.verify_unchanged()?;
    Ok(selection)
}

fn capture_tiles(
    data: &Path,
    selection: &CapturedSource,
    directory: &str,
    tiles: &[Tile],
) -> Result<CapturedSource> {
    let mut names: Vec<String> = DBCS.iter().map(|s| (*s).to_owned()).collect();
    names.push(wdt_name(directory));
    names.extend(tiles.iter().map(|&tile| adt_name(directory, tile)));
    let source = CapturedSource::capture(data, &names)?;
    source.verify_contains(selection)?;
    selection.verify_unchanged()?;
    Ok(source)
}

/// Private observations of a completed dependency search, never restore inputs.
#[derive(Default, Serialize)]
struct HistorySummary {
    consulted_adts: usize,
    consulted_wdts: usize,
    absent_members: usize,
    initial_zero_samples: usize,
}

fn produce_tile_with_history(
    source: &mut CapturedSource,
    map: u32,
    directory: &str,
    tile: Tile,
) -> Result<(Vec<u8>, Option<HistorySummary>)> {
    let wdt = wdt_name(directory);
    let adt = adt_name(directory, tile);
    let ordinary = writer::produce(
        source.bytes(DBCS[0])?,
        source.bytes(DBCS[1])?,
        source.bytes(DBCS[2])?,
        source.bytes(&wdt)?,
        source.bytes(&adt)?,
        map,
        tile.x,
        tile.y,
    );
    match ordinary {
        Ok(bytes) => Ok((bytes, None)),
        Err(error) if diagnostic_id(&error) == "terrain.liquid_history" => {
            let mut history = writer::boundary_history(source.bytes(&adt)?)?;
            let summary = resolve_history(source, map, tile, &mut history)?;
            let bytes = writer::produce_with_history(
                source.bytes(DBCS[0])?,
                source.bytes(DBCS[1])?,
                source.bytes(DBCS[2])?,
                source.bytes(&wdt)?,
                source.bytes(&adt)?,
                map,
                tile.x,
                tile.y,
                &history,
            )?;
            Ok((bytes, Some(summary)))
        }
        Err(error) => Err(error),
    }
}

fn resolve_history(
    source: &mut CapturedSource,
    map: u32,
    tile: Tile,
    history: &mut writer::BoundaryHistory,
) -> Result<HistorySummary> {
    let maps = writer::map_directories(source.bytes(DBCS[0])?)?;
    let selected = maps
        .iter()
        .position(|(id, _)| *id == map)
        .context("terrain.map_missing")?;
    let mut summary = HistorySummary::default();
    for index in (0..=selected).rev() {
        if history.is_resolved() {
            return Ok(summary);
        }
        let (_, directory) = &maps[index];
        let wdt = wdt_name(directory);
        summary.consulted_wdts += 1;
        if !source.capture_history_member(&wdt)? {
            summary.absent_members += 1;
            continue;
        }
        let prior_tiles = writer::history_wdt_tiles(source.bytes(&wdt)?)?;
        for (x, y) in prior_tiles.into_iter().rev() {
            let predecessor = Tile { x, y };
            if index == selected && predecessor >= tile {
                continue;
            }
            if history.is_resolved() {
                return Ok(summary);
            }
            let adt = adt_name(directory, predecessor);
            summary.consulted_adts += 1;
            if !source.capture_history_member(&adt)? {
                summary.absent_members += 1;
                continue;
            }
            history.apply_predecessor(source.bytes(&adt)?)?;
        }
    }
    // This is the only production zero-initialization seam: all earlier source
    // events have been exhausted, with every relevant presence/absence retained.
    summary.initial_zero_samples = history.unresolved().count();
    history.mark_proven_start_zero();
    Ok(summary)
}
