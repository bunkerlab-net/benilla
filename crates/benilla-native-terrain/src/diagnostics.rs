// SPDX-License-Identifier: GPL-3.0-or-later
//! Path-free observations of the same bounded immutable inputs as production.
//! A report is informational, never a source capability or gameplay admission.
use crate::{
    adt_name, capture_tiles, capture_wdt, diagnostic_id, source::CapturedSource,
    validate_selection, wdt_name, writer, Tile, DBCS,
};
use anyhow::Result;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Default, Serialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    Passed,
    Failed,
    #[default]
    Unavailable,
}

#[derive(Default, Serialize)]
struct Stage {
    status: Status,
    diagnostic_id: Option<&'static str>,
}
impl Stage {
    fn record<T>(&mut self, result: Result<T>) -> Option<T> {
        match result {
            Ok(value) => {
                self.status = Status::Passed;
                self.diagnostic_id = None;
                Some(value)
            }
            Err(error) => {
                self.status = Status::Failed;
                self.diagnostic_id = Some(diagnostic_id(&error));
                None
            }
        }
    }

    fn passed(&self) -> bool {
        matches!(self.status, Status::Passed)
    }
}

#[derive(Default, Serialize)]
struct Stages {
    selection: Stage,
    initial_capture: Stage,
    map_table: Stage,
    selection_capture: Stage,
    full_capture: Stage,
    area_table: Stage,
    liquid_table: Stage,
    source_recheck: Stage,
}

#[derive(Default, Serialize)]
struct TileStages {
    wdt_selection: Stage,
    adt_structure: Stage,
    liquid: Stage,
    production: Stage,
}

#[derive(Serialize)]
struct MapTable {
    selected_row_ordinal: usize,
    total_rows: usize,
}

#[derive(Serialize)]
struct RawAdt {
    bytes: usize,
    sha256: String,
}

#[derive(Serialize)]
struct TileReport {
    x: u8,
    y: u8,
    raw_adt: Option<RawAdt>,
    active_mclq_cells: Option<usize>,
    liquid_rectangle: Option<writer::LiquidRectangle>,
    missing_liquid_heights: Option<writer::MissingLiquidHeights>,
    stages: TileStages,
}
impl TileReport {
    fn new(tile: Tile) -> Self {
        Self {
            x: tile.x,
            y: tile.y,
            raw_adt: None,
            active_mclq_cells: None,
            liquid_rectangle: None,
            missing_liquid_heights: None,
            stages: TileStages::default(),
        }
    }
}

/// Bounded serializable observations, without names, paths, raw errors, or assets.
/// Fields cannot be supplied by a caller to construct a captured source or product.
#[derive(Serialize)]
pub struct DiagnosticReport {
    format: &'static str,
    version: u32,
    map: u32,
    map_table: Option<MapTable>,
    source_epoch: Option<String>,
    live_install_lease: bool,
    gameplay_admitted: bool,
    stages: Stages,
    tiles: Vec<TileReport>,
}

/// Diagnose one through nine explicitly selected tiles without writing files.
/// Failed stages remain reportable; checks with unmet dependencies are unavailable.
/// A source epoch describes only an actual full capture, even if later checks fail.
/// A successful report is not a successful inspection or a live installation lease.
pub fn diagnose(data: &Path, map: u32, tiles: &[Tile]) -> DiagnosticReport {
    let mut report = DiagnosticReport {
        format: "benilla-terrain-diagnostic",
        version: 1,
        map,
        map_table: None,
        source_epoch: None,
        live_install_lease: false,
        gameplay_admitted: false,
        stages: Stages::default(),
        tiles: Vec::new(),
    };
    report.populate(data, tiles);
    report
}

impl DiagnosticReport {
    fn populate(&mut self, data: &Path, tiles: &[Tile]) {
        let Some(ordered) = self
            .stages
            .selection
            .record(validate_selection(self.map, tiles))
        else {
            return;
        };
        self.tiles = ordered.iter().copied().map(TileReport::new).collect();
        let Some(initial) = self
            .stages
            .initial_capture
            .record(CapturedSource::capture(data, &DBCS))
        else {
            return;
        };
        // These tables are independent of the selected directory and ADT writer.
        self.stages
            .area_table
            .record(initial.bytes(DBCS[1]).and_then(writer::area_flags));
        self.stages.liquid_table.record(
            initial
                .bytes(DBCS[2])
                .and_then(writer::validate_liquid_table),
        );
        let Some(map) = self.stages.map_table.record(
            initial
                .bytes(DBCS[0])
                .and_then(|bytes| writer::map_selection(bytes, self.map)),
        ) else {
            return;
        };
        self.map_table = Some(MapTable {
            selected_row_ordinal: map.selected_row_ordinal,
            total_rows: map.total_rows,
        });
        let Some(selection) =
            self.stages
                .selection_capture
                .record(capture_wdt(data, &initial, &map.directory))
        else {
            return;
        };
        drop(initial);
        let wdt = wdt_name(&map.directory);
        for tile in &mut self.tiles {
            tile.stages.wdt_selection.record(
                selection
                    .bytes(&wdt)
                    .and_then(|bytes| writer::validate_selection_wdt(bytes, tile.x, tile.y)),
            );
        }
        if !self
            .tiles
            .iter()
            .all(|tile| tile.stages.wdt_selection.passed())
        {
            return;
        }
        let Some(source) = self.stages.full_capture.record(capture_tiles(
            data,
            &selection,
            &map.directory,
            &ordered,
        )) else {
            return;
        };
        drop(selection);
        self.source_epoch = Some(source.binding_digest().to_owned());
        for tile in &mut self.tiles {
            let adt = adt_name(
                &map.directory,
                Tile {
                    x: tile.x,
                    y: tile.y,
                },
            );
            let result = source.bytes(&adt).and_then(|bytes| {
                tile.raw_adt = Some(RawAdt {
                    bytes: bytes.len(),
                    sha256: format!("{:x}", Sha256::digest(bytes)),
                });
                writer::diagnose_tile(bytes)
            });
            if let Some(observation) = tile.stages.adt_structure.record(result) {
                tile.active_mclq_cells = Some(observation.active_mclq_cells);
                tile.liquid_rectangle = observation.liquid_rectangle;
                tile.missing_liquid_heights = observation.missing_liquid_heights;
                tile.stages.liquid.record(observation.liquid_result);
            }
            if self.stages.area_table.passed()
                && self.stages.liquid_table.passed()
                && tile.stages.liquid.passed()
            {
                let result = (|| {
                    writer::produce(
                        source.bytes(DBCS[0])?,
                        source.bytes(DBCS[1])?,
                        source.bytes(DBCS[2])?,
                        source.bytes(&wdt)?,
                        source.bytes(&adt)?,
                        self.map,
                        tile.x,
                        tile.y,
                    )
                    .map(|_| ())
                })();
                tile.stages.production.record(result);
            }
        }
        self.stages.source_recheck.record(source.verify_unchanged());
    }
}
