// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded dry subset of CMaNGOS ad `-f 0`, revision
//! 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, contrib/extractor/System.cpp:
//! ReadMapDBC, ReadAreaTableDBC, ConvertADT and ExtractMapsFromMpq.
//! Copyright (C) Continued-MaNGOS Project; GPL-2.0-or-later upstream.
//!
//! These functions consume complete owned-by-caller raw members. They do not resolve
//! archives, authenticate an installation, or certify gameplay/static-world coverage.
//! Framing/resource restrictions deliberately reject unsafe or unsupported source
//! inputs rather than reproduce upstream's unchecked pointers and stale globals.

use anyhow::{ensure, Context, Result};
use std::collections::{BTreeMap, BTreeSet};

const MAX_MEMBER: usize = 64 * 1024 * 1024;
const MAX_DBC_ROWS: usize = 65_536;
const MAX_DBC_ID: u32 = 1_000_000;
const MAX_STRINGS: usize = 1024 * 1024;
const CELLS: usize = 256;

fn word(bytes: &[u8], at: usize) -> Result<u32> {
    let end = at.checked_add(4).context("terrain.offset_overflow")?;
    Ok(u32::from_le_bytes(
        bytes
            .get(at..end)
            .context("terrain.truncated_word")?
            .try_into()?,
    ))
}

fn half(bytes: &[u8], at: usize) -> Result<u16> {
    let end = at.checked_add(2).context("terrain.offset_overflow")?;
    Ok(u16::from_le_bytes(
        bytes
            .get(at..end)
            .context("terrain.truncated_half")?
            .try_into()?,
    ))
}

fn number(bytes: &[u8], at: usize) -> Result<f32> {
    let value = f32::from_bits(word(bytes, at)?);
    ensure!(value.is_finite(), "terrain.nonfinite_height");
    Ok(value)
}

struct Dbc<'a> {
    records: &'a [u8],
    strings: &'a [u8],
    stride: usize,
}

impl<'a> Dbc<'a> {
    fn read(bytes: &'a [u8], fields: usize) -> Result<Self> {
        ensure!((20..=MAX_MEMBER).contains(&bytes.len()), "terrain.dbc_size");
        ensure!(bytes.get(..4) == Some(b"WDBC"), "terrain.dbc_magic");
        let rows = word(bytes, 4)? as usize;
        let strings = word(bytes, 16)? as usize;
        let stride = fields * 4;
        ensure!(rows <= MAX_DBC_ROWS, "terrain.dbc_rows");
        ensure!((1..=MAX_STRINGS).contains(&strings), "terrain.dbc_strings");
        ensure!(
            word(bytes, 8)? as usize == fields && word(bytes, 12)? as usize == stride,
            "terrain.dbc_layout"
        );
        let strings_at = 20 + rows * stride;
        ensure!(strings_at + strings == bytes.len(), "terrain.dbc_framing");
        ensure!(
            bytes[strings_at] == 0 && bytes.last() == Some(&0),
            "terrain.dbc_nul"
        );
        Ok(Self {
            records: &bytes[20..strings_at],
            strings: &bytes[strings_at..],
            stride,
        })
    }

    fn rows(&self) -> impl Iterator<Item = &[u8]> {
        self.records.chunks_exact(self.stride)
    }

    fn string(&self, offset: u32) -> Result<&str> {
        let tail = self
            .strings
            .get(offset as usize..)
            .context("terrain.dbc_string_offset")?;
        let end = tail
            .iter()
            .position(|&b| b == 0)
            .context("terrain.dbc_string_nul")?;
        std::str::from_utf8(&tail[..end]).context("terrain.dbc_string_utf8")
    }
}

/// Directory from the unique selected raw Map.dbc row (fields 0 and 1).
/// A safe single path component, bounded by upstream's 64-byte map-name storage.
pub fn map_directory(map_dbc: &[u8], map: u32) -> Result<String> {
    let dbc = Dbc::read(map_dbc, 42).context("terrain.Map")?;
    let mut ids = BTreeSet::new();
    let mut selected = None;
    for row in dbc.rows() {
        let id = word(row, 0)?;
        ensure!(ids.insert(id), "terrain.duplicate_map_id");
        let directory = dbc.string(word(row, 4)?)?;
        ensure!(
            !directory.is_empty()
                && directory.len() < 64
                && directory
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
            "terrain.map_directory"
        );
        if id == map {
            selected = Some(directory.to_owned());
        }
    }
    selected.context("terrain.map_missing")
}

#[derive(Clone, Copy)]
struct Chunk<'a> {
    at: usize,
    magic: [u8; 4],
    data: &'a [u8],
}

fn chunk_at(bytes: &[u8], at: usize) -> Result<Chunk<'_>> {
    let header_end = at.checked_add(8).context("terrain.chunk_overflow")?;
    let header = bytes.get(at..header_end).context("terrain.chunk_header")?;
    let end = header_end
        .checked_add(word(header, 4)? as usize)
        .context("terrain.chunk_overflow")?;
    Ok(Chunk {
        at,
        magic: header[..4].try_into()?,
        data: bytes
            .get(header_end..end)
            .context("terrain.chunk_payload")?,
    })
}

fn chunks(bytes: &[u8], start: usize, max: usize, normal_padding: bool) -> Result<Vec<Chunk<'_>>> {
    ensure!(bytes.len() <= MAX_MEMBER, "terrain.member_budget");
    let mut out = Vec::new();
    let mut at = start;
    while at < bytes.len() {
        ensure!(out.len() < max, "terrain.chunk_count");
        let chunk = chunk_at(bytes, at)?;
        at += 8 + chunk.data.len();
        // Vanilla MCNR's 435-byte payload has 13 trailing bytes outside its size.
        if normal_padding && chunk.magic == *b"RNCM" && chunk.data.len() == 435 {
            at = at.checked_add(13).context("terrain.normal_padding")?;
            ensure!(at <= bytes.len(), "terrain.normal_padding");
        }
        out.push(chunk);
    }
    ensure!(at == bytes.len(), "terrain.chunk_framing");
    Ok(out)
}

/// Validate source WDT selection for output-grid `(tile_x, tile_y)`.
/// The output `MMMXXYY.map` selects MAIN[row=XX][column=YY], hence the input
/// member is `World\\Maps\\D\\D_YY_XX.adt`. Upstream checks the whole exist word.
pub fn validate_selection_wdt(wdt: &[u8], tile_x: u8, tile_y: u8) -> Result<()> {
    ensure!(tile_x < 64 && tile_y < 64, "terrain.tile_coordinate");
    let records = chunks(wdt, 0, 5, false)?;
    ensure!(records.len() >= 4, "terrain.wdt_required_records");
    ensure!(
        records[0].magic == *b"REVM"
            && records[0].data.len() == 4
            && word(records[0].data, 0)? == 18,
        "terrain.wdt_version"
    );
    ensure!(
        records[1].magic == *b"DHPM"
            && records[1].data.len() == 32
            && word(records[1].data, 0)? == 0,
        "terrain.wdt_global_or_header"
    );
    ensure!(
        records[2].magic == *b"NIAM" && records[2].data.len() == 64 * 64 * 8,
        "terrain.wdt_main"
    );
    ensure!(
        records[3].magic == *b"OMWM" && records[3].data.is_empty(),
        "terrain.wdt_global_wmo"
    );
    if records.len() == 5 {
        ensure!(
            records[4].magic == *b"FDOM" && records[4].data.is_empty(),
            "terrain.wdt_unsupported_record"
        );
    }
    let slot = usize::from(tile_x) * 64 + usize::from(tile_y);
    ensure!(
        word(records[2].data, slot * 8)? != 0,
        "terrain.wdt_tile_absent"
    );
    Ok(())
}

fn area_flags(area_dbc: &[u8]) -> Result<BTreeMap<u32, u16>> {
    let dbc = Dbc::read(area_dbc, 25).context("terrain.AreaTable")?;
    let mut flags = BTreeMap::new();
    for row in dbc.rows() {
        let id = word(row, 0)?;
        ensure!(id <= MAX_DBC_ID, "terrain.dbc_id_budget");
        for field in 11..=18 {
            dbc.string(word(row, field * 4)?)?;
        }
        // Upstream writes a uint32 column into uint16 storage; preserve narrowing
        // and physical-row last-write-wins, including the reserved 0xffff value.
        flags.insert(id, word(row, 12)? as u16);
    }
    Ok(flags)
}

fn validate_liquid_table(liquid_dbc: &[u8]) -> Result<()> {
    let dbc = Dbc::read(liquid_dbc, 4).context("terrain.LiquidType")?;
    for row in dbc.rows() {
        ensure!(word(row, 0)? <= MAX_DBC_ID, "terrain.dbc_id_budget");
    }
    Ok(())
}

struct Cell<'a> {
    base: f32,
    heights: Option<&'a [u8]>,
    area: u32,
    holes: u16,
}

fn read_cell<'a>(root: Chunk<'a>) -> Result<Cell<'a>> {
    ensure!(root.data.len() >= 128, "terrain.mcnk_header");
    let h = &root.data[..128];
    ensure!(
        word(h, 0)? & 0x3c == 0 && word(h, 0x60)? == 0 && word(h, 0x64)? == 0,
        "terrain.liquid_excluded"
    );
    // MCCV is a later-client representation outside this vanilla writer profile.
    ensure!(word(h, 0x74)? == 0, "terrain.mccv_excluded");
    let mut nested = BTreeMap::new();
    for record in chunks(root.data, 128, 8, true)? {
        ensure!(
            matches!(
                &record.magic,
                b"TVCM" | b"RNCM" | b"YLCM" | b"FRCM" | b"LACM" | b"HSCM" | b"ESCM"
            ),
            "terrain.mcnk_unsupported_or_liquid"
        );
        ensure!(
            nested.insert(record.magic, record).is_none(),
            "terrain.duplicate_subchunk"
        );
    }
    for (offset, magic) in [
        (0x14, *b"TVCM"),
        (0x18, *b"RNCM"),
        (0x1c, *b"YLCM"),
        (0x20, *b"FRCM"),
        (0x24, *b"LACM"),
        (0x2c, *b"HSCM"),
        (0x58, *b"ESCM"),
    ] {
        let relative = word(h, offset)? as usize;
        match nested.get(&magic) {
            Some(record) => ensure!(relative == record.at + 8, "terrain.subchunk_reference"),
            None => ensure!(relative == 0, "terrain.dangling_subchunk"),
        }
    }
    let heights = nested.get(b"TVCM").map(|record| record.data);
    if let Some(heights) = heights {
        ensure!(heights.len() == 145 * 4, "terrain.mcvt_length");
        for at in (0..heights.len()).step_by(4) {
            number(heights, at)?;
        }
    }
    // Only height is consumed by ad, but reject invalid header position floats too.
    number(h, 0x68)?;
    number(h, 0x6c)?;
    let base = number(h, 0x70)?;
    Ok(Cell {
        base,
        heights,
        area: word(h, 0x34)?,
        holes: half(h, 0x3c)?,
    })
}

fn read_cells(adt: &[u8]) -> Result<Vec<Cell<'_>>> {
    let records = chunks(adt, 0, 272, false)?;
    ensure!(records.len() >= 3, "terrain.adt_required_records");
    ensure!(
        records[0].magic == *b"REVM"
            && records[0].data.len() == 4
            && word(records[0].data, 0)? == 18,
        "terrain.adt_version"
    );
    ensure!(
        records[1].magic == *b"RDHM" && records[1].at == 12 && records[1].data.len() == 64,
        "terrain.adt_mhdr"
    );
    let mhdr = records[1].data;
    // adt_MCIN::getMCNK subtracts 84, not the actual MCIN location.
    ensure!(
        records[2].magic == *b"NICM"
            && records[2].at == 84
            && records[2].data.len() == CELLS * 16
            && word(mhdr, 4)? == 64,
        "terrain.canonical_mcin_required"
    );
    ensure!(word(mhdr, 0x28)? == 0, "terrain.mh2o_excluded");
    let mut singletons = BTreeMap::new();
    let mut physical = BTreeMap::new();
    for record in records.iter().copied() {
        if record.magic == *b"KNCM" {
            physical.insert(record.at, record);
        } else {
            ensure!(
                matches!(
                    &record.magic,
                    b"REVM"
                        | b"RDHM"
                        | b"NICM"
                        | b"XETM"
                        | b"XDMM"
                        | b"DIMM"
                        | b"OMWM"
                        | b"DIWM"
                        | b"FDDM"
                        | b"FDOM"
                ),
                "terrain.adt_unsupported_or_liquid"
            );
            ensure!(
                singletons.insert(record.magic, record).is_none(),
                "terrain.duplicate_chunk"
            );
        }
    }
    ensure!(physical.len() == CELLS, "terrain.cell_count");
    for (offset, magic) in [
        (4, *b"NICM"),
        (8, *b"XETM"),
        (12, *b"XDMM"),
        (16, *b"DIMM"),
        (20, *b"OMWM"),
        (24, *b"DIWM"),
        (28, *b"FDDM"),
        (32, *b"FDOM"),
    ] {
        let relative = word(mhdr, offset)? as usize;
        match singletons.get(&magic) {
            Some(record) => ensure!(relative + 20 == record.at, "terrain.mhdr_reference"),
            None => ensure!(relative == 0, "terrain.dangling_root_chunk"),
        }
    }
    ensure!(word(mhdr, 36)? == 0, "terrain.mfbo_excluded");
    let mcin = records[2].data;
    let mut selected = BTreeSet::new();
    let mut cells = Vec::with_capacity(CELLS);
    for slot in 0..CELLS {
        let at = word(mcin, slot * 16)? as usize;
        ensure!(selected.insert(at), "terrain.duplicate_mcin_reference");
        let root = *physical.get(&at).context("terrain.mcin_not_mcnk")?;
        ensure!(
            word(mcin, slot * 16 + 4)? as usize == root.data.len() + 8,
            "terrain.mcin_size"
        );
        cells.push(read_cell(root)?);
    }
    Ok(cells)
}

/// Produce precisely one dry `MAPS/z1.4` member, without reading paths or trusting
/// caller-declared heights, areas, holes, liquid absence or precomputed products.
/// All three DBCs are loaded by upstream even when the selected tile is dry.
/// Output naming is `maps/{map:03}{tile_x:02}{tile_y:02}.map`; input ADT naming
/// reverses that coordinate pair. Map/WDT are validated here as well as by callers.
#[allow(clippy::too_many_arguments)]
pub fn produce(
    map_dbc: &[u8],
    area_dbc: &[u8],
    liquid_dbc: &[u8],
    wdt: &[u8],
    adt: &[u8],
    map: u32,
    tile_x: u8,
    tile_y: u8,
) -> Result<Vec<u8>> {
    map_directory(map_dbc, map)?;
    validate_selection_wdt(wdt, tile_x, tile_y)?;
    let flags = area_flags(area_dbc)?;
    validate_liquid_table(liquid_dbc)?;
    let cells = read_cells(adt)?;
    let mut areas = [0xffff_u16; CELLS];
    let mut v9 = vec![0.0_f32; 129 * 129];
    let mut v8 = vec![0.0_f32; 128 * 128];
    for (slot, cell) in cells.iter().enumerate() {
        if cell.area != 0 {
            areas[slot] = flags.get(&cell.area).copied().unwrap_or(0xffff);
        }
        let row = slot / 16;
        let col = slot % 16;
        // Source row-major MCIN order resets shared V9 borders, then adds MCVT.
        // MCNK ix/iy do not select placement or change the overwrite winner.
        // With no MCVT the base is assigned unchanged, including negative zero.
        for y in 0..=8 {
            for x in 0..=8 {
                let height = match cell.heights {
                    Some(heights) => cell.base + number(heights, (y * 17 + x) * 4)?,
                    None => cell.base,
                };
                ensure!(height.is_finite(), "terrain.height_overflow");
                v9[(row * 8 + y) * 129 + col * 8 + x] = height;
            }
        }
        for y in 0..8 {
            for x in 0..8 {
                let height = match cell.heights {
                    Some(heights) => cell.base + number(heights, (y * 17 + 9 + x) * 4)?,
                    None => cell.base,
                };
                ensure!(height.is_finite(), "terrain.height_overflow");
                v8[(row * 8 + y) * 128 + col * 8 + x] = height;
            }
        }
    }
    let mut max_height = -20000.0_f32;
    let mut min_height = 20000.0_f32;
    // Preserve source seed values, strict comparisons and V8-before-V9 scan order.
    for &height in v8.iter().chain(&v9) {
        if max_height < height {
            max_height = height;
        }
        if min_height > height {
            min_height = height;
        }
    }
    if min_height < -500.0 {
        for height in v8.iter_mut().chain(&mut v9) {
            if *height < -500.0 {
                *height = -500.0;
            }
        }
        min_height = -500.0;
        if max_height < -500.0 {
            max_height = -500.0;
        }
    }
    let full_area = areas.iter().any(|&value| value != areas[0]);
    let flat = max_height == min_height;
    let area_size = 8 + if full_area { 512 } else { 0 };
    let height_size = 16 + if flat { 0 } else { (129 * 129 + 128 * 128) * 4 };
    let hole_offset = 40 + area_size + height_size;
    let mut out = Vec::with_capacity(hole_offset + 512);
    out.extend_from_slice(b"MAPSz1.4");
    for value in [
        40,
        area_size,
        40 + area_size,
        height_size,
        0,
        0,
        hole_offset,
        512,
    ] {
        out.extend_from_slice(&(value as u32).to_le_bytes());
    }
    out.extend_from_slice(b"AREA");
    out.extend_from_slice(&u16::from(!full_area).to_le_bytes());
    out.extend_from_slice(&(if full_area { 0 } else { areas[0] }).to_le_bytes());
    if full_area {
        for area in areas {
            out.extend_from_slice(&area.to_le_bytes());
        }
    }
    out.extend_from_slice(b"MHGT");
    out.extend_from_slice(&u32::from(flat).to_le_bytes());
    out.extend_from_slice(&min_height.to_le_bytes());
    out.extend_from_slice(&max_height.to_le_bytes());
    if !flat {
        for height in v9.into_iter().chain(v8) {
            out.extend_from_slice(&height.to_le_bytes());
        }
    }
    for cell in cells {
        out.extend_from_slice(&cell.holes.to_le_bytes());
    }
    ensure!(out.len() == hole_offset + 512, "terrain.writer_framing");
    Ok(out)
}

#[cfg(test)]
mod tests;
