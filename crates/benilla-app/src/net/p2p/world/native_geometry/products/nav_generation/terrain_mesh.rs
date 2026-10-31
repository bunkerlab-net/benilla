// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded port of pinned CMaNGOS TerrainBuilder's MAPSz1.4 triangle collection.
//! Source: 8ec338a1 contrib/mmap/src/TerrainBuilder.{cpp,h}; see PROVENANCE.md.
//! This transforms bytes only: no source, coverage, or gameplay admission is established.
use super::super::{u16_at, u32_at, Error};

const V9: usize = 129 * 129;
const V8: usize = 128 * 128;
const GRID_SIZE: f32 = 533.333_3;
const GRID_PART_SIZE: f32 = GRID_SIZE / 128.0;
const INVALID_LIQUID: f32 = -500.0;
const INVALID_LIQUID_MAX: f32 = 5000.0;
const MAX_HEIGHT: f32 = 1_000_000.0;
// Local aggregate resource limits, never CMaNGOS geometry or coverage rules.
const MAX_VERTICES: usize = 1_000_000;
const MAX_TRIANGLES: usize = 2_000_000;
const MAX_MAP_BYTES: usize = 40 + 520 + 16 + (V9 + V8) * 4 + 16 + 768 + V9 * 4 + 512;

#[derive(Clone, Copy, Debug)]
pub(super) enum Portion {
    Entire,
    Top,
    Left,
    Right,
    Bottom,
}
impl Portion {
    fn squares(self) -> std::iter::StepBy<std::ops::Range<usize>> {
        // getLoopVars: strips retain the source tile's complete vertex arrays.
        let (start, end, increment) = match self {
            Self::Entire => (0, V8, 1),
            Self::Top => (0, 128, 1),
            Self::Left => (0, V8 - 128 + 1, 128),
            Self::Right => (127, V8, 128),
            Self::Bottom => (V8 - 128, V8, 1),
        };
        (start..end).step_by(increment)
    }
}

#[derive(Debug, Default)]
pub(in super::super) struct Mesh {
    pub solid_vertices: Vec<[f32; 3]>,
    pub solid_triangles: Vec<[u32; 3]>,
    pub liquid_vertices: Vec<[f32; 3]>,
    pub liquid_triangles: Vec<[u32; 3]>,
    pub liquid_areas: Vec<u8>,
}

/// Append precisely the float/flat TerrainBuilder mesh before cleanVertices.
/// The caller validates TerrainTile and obtains bytes from genuine PreparedTerrain;
/// arbitrary bytes accepted here are never evidence of either source or coverage.
/// Source tile arguments are the loadMap pair, whose filename writes Y before X.
/// Any failure restores lengths and values; capacity changes are not observable mesh data.
pub(super) fn append_map(
    mesh: &mut Mesh,
    source_tile_x: u8,
    source_tile_y: u8,
    bytes: &[u8],
    portion: Portion,
    cancelled: &impl Fn() -> bool,
) -> Result<bool, Error> {
    check_cancelled(cancelled)?;
    if source_tile_x >= 64 || source_tile_y >= 64 {
        return Err(Error::Identity);
    }
    let map = MapView::decode(bytes, cancelled)?;
    let checkpoint = (
        mesh.solid_vertices.len(),
        mesh.solid_triangles.len(),
        mesh.liquid_vertices.len(),
        mesh.liquid_triangles.len(),
        mesh.liquid_areas.len(),
    );
    let result = append_validated(mesh, source_tile_x, source_tile_y, map, portion, cancelled);
    if result.is_err() {
        // Repair touches only this call's appended liquid vertices, never old vertices.
        mesh.solid_vertices.truncate(checkpoint.0);
        mesh.solid_triangles.truncate(checkpoint.1);
        mesh.liquid_vertices.truncate(checkpoint.2);
        mesh.liquid_triangles.truncate(checkpoint.3);
        mesh.liquid_areas.truncate(checkpoint.4);
    }
    result
}

fn append_validated(
    mesh: &mut Mesh,
    tile_x: u8,
    tile_y: u8,
    map: MapView<'_>,
    portion: Portion,
    cancelled: &impl Fn() -> bool,
) -> Result<bool, Error> {
    let have_terrain = map.heights.is_some();
    let have_liquid = map.liquid.is_some();
    if !have_terrain && !have_liquid {
        // NO_HEIGHT means no terrain here, even when gridHeight is a useful flat value.
        return Ok(false);
    }
    let squares = portion.squares().len();
    reserve_mesh(mesh, have_terrain, have_liquid, squares)?;
    let solid_base = mesh.solid_vertices.len() as u32;
    let liquid_base = mesh.liquid_vertices.len() as u32;
    let x_offset = (f32::from(tile_x) - 32.0) * GRID_SIZE;
    let y_offset = (f32::from(tile_y) - 32.0) * GRID_SIZE;
    if let Some(heights) = map.heights {
        for i in 0..V9 + V8 {
            if i % 128 == 0 {
                check_cancelled(cancelled)?;
            }
            let height = number(heights, i * 4)?;
            let (index, size, center) = if i < V9 {
                (i, 129, false)
            } else {
                (i - V9, 128, true)
            };
            let x = x_offset + (index % size) as f32 * GRID_PART_SIZE;
            let y = y_offset + (index / size) as f32 * GRID_PART_SIZE;
            // Keep getHeightCoord's operation order, including signed zero.
            mesh.solid_vertices.push(if center {
                [
                    -(x + GRID_PART_SIZE / 2.0),
                    height,
                    -(y + GRID_PART_SIZE / 2.0),
                ]
            } else {
                [-x, height, -y]
            });
        }
    }
    if let Some(liquid) = map.liquid {
        for i in 0..V9 {
            if i % 128 == 0 {
                check_cancelled(cancelled)?;
            }
            mesh.liquid_vertices.push([
                -(x_offset + (i % 129) as f32 * GRID_PART_SIZE),
                liquid.original_height(i)?,
                -(y_offset + (i / 129) as f32 * GRID_PART_SIZE),
            ]);
        }
    }
    for (step, square) in portion.squares().enumerate() {
        if step % 128 == 0 {
            check_cancelled(cancelled)?;
        }
        let terrain = solid_triangles(square, solid_base);
        let liquids = liquid_triangles(square, liquid_base);
        for half in 0..2 {
            let mut use_terrain = have_terrain;
            let mut use_liquid = false;
            let mut area = 0;
            if let Some(liquid) = map.liquid.filter(|liquid| liquid.types.is_some()) {
                let flags = liquid.cell_flags(square);
                if flags & 0x10 != 0 {
                    use_terrain = false; // DEEP_WATER suppresses both types, even above ground.
                } else if flags & (0x08 | 0x02) != 0 {
                    use_liquid = true;
                    area = 9; // NAV_AREA_WATER
                } else if flags & (0x01 | 0x04) != 0 {
                    use_liquid = true;
                    area = 8; // NAV_AREA_MAGMA_SLIME
                }
                if use_liquid {
                    let mut height = 0.0f32;
                    let mut valid_count = 0u32;
                    for index in liquids[half] {
                        // Original values, equivalent to lverts_copy, never earlier repairs.
                        let h = liquid.original_height((index - liquid_base) as usize)?;
                        if h != INVALID_LIQUID && h < INVALID_LIQUID_MAX {
                            height += h;
                            valid_count += 1;
                        }
                    }
                    if valid_count > 0 && valid_count < 3 {
                        height /= valid_count as f32;
                        for index in liquids[half] {
                            let h = &mut mesh.liquid_vertices[index as usize][1];
                            // Source deliberately neither counts nor repairs exactly 5000.
                            if *h == INVALID_LIQUID || *h > INVALID_LIQUID_MAX {
                                *h = height;
                            }
                        }
                    }
                    if valid_count == 0 {
                        use_liquid = false;
                    }
                }
            }
            if use_terrain && map.is_hole(square)? {
                use_terrain = false;
            }
            if use_terrain && use_liquid {
                let mut max_liquid = INVALID_LIQUID;
                for index in liquids[half] {
                    let h = mesh.liquid_vertices[index as usize][1];
                    if max_liquid < h {
                        max_liquid = h;
                    }
                }
                let mut min_terrain = INVALID_LIQUID_MAX;
                for triangle in &terrain[half * 2..half * 2 + 2] {
                    for index in triangle {
                        let h = mesh.solid_vertices[*index as usize][1];
                        if min_terrain > h {
                            min_terrain = h;
                        }
                    }
                }
                if min_terrain > max_liquid {
                    use_liquid = false;
                }
            }
            if use_liquid {
                mesh.liquid_areas.push(area);
                mesh.liquid_triangles.push(liquids[half]);
            }
            if use_terrain {
                mesh.solid_triangles
                    .extend_from_slice(&terrain[half * 2..half * 2 + 2]);
            }
        }
    }
    check_cancelled(cancelled)?;
    // loadMap's final result is aggregate triangle presence, not this portion's delta.
    Ok(!mesh.solid_triangles.is_empty() || !mesh.liquid_triangles.is_empty())
}

fn reserve_mesh(mesh: &mut Mesh, terrain: bool, liquid: bool, squares: usize) -> Result<(), Error> {
    let terrain_vertices = if terrain { V9 + V8 } else { 0 };
    let liquid_vertices = if liquid { V9 } else { 0 };
    let terrain_triangles = if terrain { squares * 4 } else { 0 };
    let liquid_triangles = if liquid { squares * 2 } else { 0 };
    let vertices = mesh
        .solid_vertices
        .len()
        .checked_add(mesh.liquid_vertices.len())
        .and_then(|n| n.checked_add(terrain_vertices + liquid_vertices))
        .ok_or(Error::Budget)?;
    let triangles = mesh
        .solid_triangles
        .len()
        .checked_add(mesh.liquid_triangles.len())
        .and_then(|n| n.checked_add(terrain_triangles + liquid_triangles))
        .ok_or(Error::Budget)?;
    if vertices > MAX_VERTICES || triangles > MAX_TRIANGLES {
        return Err(Error::Budget);
    }
    if mesh.liquid_triangles.len() != mesh.liquid_areas.len() {
        return Err(Error::Framing);
    }
    mesh.solid_vertices
        .try_reserve_exact(terrain_vertices)
        .map_err(|_| Error::Budget)?;
    mesh.liquid_vertices
        .try_reserve_exact(liquid_vertices)
        .map_err(|_| Error::Budget)?;
    mesh.solid_triangles
        .try_reserve_exact(terrain_triangles)
        .map_err(|_| Error::Budget)?;
    mesh.liquid_triangles
        .try_reserve_exact(liquid_triangles)
        .map_err(|_| Error::Budget)?;
    mesh.liquid_areas
        .try_reserve_exact(liquid_triangles)
        .map_err(|_| Error::Budget)?;
    Ok(())
}

fn solid_triangles(square: usize, base: u32) -> [[u32; 3]; 4] {
    let corner = (square + square / 128) as u32 + base;
    let center = (V9 + square) as u32 + base;
    // Spot enum is TOP=1, RIGHT=2, LEFT=3, BOTTOM=4; reverse each source winding.
    [
        [center, corner + 1, corner],
        [center, corner + 130, corner + 1],
        [corner + 129, center, corner],
        [corner + 129, corner + 130, center],
    ]
}

fn liquid_triangles(square: usize, base: u32) -> [[u32; 3]; 2] {
    let corner = (square + square / 128) as u32 + base;
    [
        [corner + 130, corner + 1, corner],
        [corner + 129, corner + 130, corner],
    ]
}

struct MapView<'a> {
    heights: Option<&'a [u8]>,
    holes: &'a [u8],
    liquid: Option<LiquidView<'a>>,
}
impl<'a> MapView<'a> {
    fn decode(bytes: &'a [u8], cancelled: &impl Fn() -> bool) -> Result<Self, Error> {
        if bytes.len() > MAX_MAP_BYTES {
            return Err(Error::Budget);
        }
        if bytes.get(..8) != Some(b"MAPSz1.4") {
            return Err(Error::Format);
        }
        let area_at = u32_at(bytes, 8)? as usize;
        let area_len = u32_at(bytes, 12)? as usize;
        let height_at = u32_at(bytes, 16)? as usize;
        let height_len = u32_at(bytes, 20)? as usize;
        let liquid_at = u32_at(bytes, 24)? as usize;
        let liquid_len = u32_at(bytes, 28)? as usize;
        let holes_at = u32_at(bytes, 32)? as usize;
        let holes_len = u32_at(bytes, 36)? as usize;
        let height_end = height_at.checked_add(height_len).ok_or(Error::Framing)?;
        let liquid_end = height_end.checked_add(liquid_len).ok_or(Error::Framing)?;
        if area_at != 40
            || !matches!(area_len, 8 | 520)
            || height_at != 40 + area_len
            || (liquid_at == 0) != (liquid_len == 0)
            || (liquid_len > 0 && liquid_at != height_end)
            || holes_at != liquid_end
            || holes_len != 512
            || holes_at.checked_add(holes_len) != Some(bytes.len())
        {
            return Err(Error::Framing);
        }
        let area = bytes.get(area_at..height_at).ok_or(Error::Framing)?;
        if area.get(..4) != Some(b"AREA") {
            return Err(Error::Format);
        }
        match (u16_at(area, 4)?, area_len) {
            (0, 520) | (1, 8) => (),
            (0 | 1, _) => return Err(Error::Framing),
            _ => return Err(Error::Unsupported),
        }
        let height = bytes.get(height_at..height_end).ok_or(Error::Framing)?;
        if height.get(..4) != Some(b"MHGT") {
            return Err(Error::Format);
        }
        let flags = u32_at(height, 4)?;
        if flags > 1 {
            // Current admitted producer uses -f 0. Quantized heights need independent evidence.
            return Err(Error::Unsupported);
        }
        let low = number(height, 8)?;
        let high = number(height, 12)?;
        if low > high {
            return Err(Error::Format);
        }
        let heights = match (flags, height_len) {
            (1, 16) if low == high => None,
            (0, n) if n == 16 + (V9 + V8) * 4 => {
                let values = height.get(16..).ok_or(Error::Framing)?;
                for i in 0..V9 + V8 {
                    if i % 128 == 0 {
                        check_cancelled(cancelled)?;
                    }
                    let value = number(values, i * 4)?;
                    if value < low || value > high {
                        return Err(Error::Format);
                    }
                }
                Some(values)
            }
            _ => return Err(Error::Framing),
        };
        let liquid = if liquid_len == 0 {
            None
        } else {
            Some(LiquidView::decode(
                bytes.get(liquid_at..liquid_end).ok_or(Error::Framing)?,
                cancelled,
            )?)
        };
        let holes = bytes.get(holes_at..).ok_or(Error::Framing)?;
        Ok(Self {
            heights,
            holes,
            liquid,
        })
    }

    fn is_hole(&self, square: usize) -> Result<bool, Error> {
        let row = square / 128;
        let col = square % 128;
        let cell = row / 8 * 16 + col / 8;
        let mask = 1 << (row % 8 / 2 * 4 + col % 8 / 2);
        Ok(u16_at(self.holes, cell * 2)? & mask != 0)
    }
}

#[derive(Clone, Copy)]
struct LiquidView<'a> {
    types: Option<&'a [u8]>,
    heights: Option<&'a [u8]>,
    offset_x: usize,
    offset_y: usize,
    width: usize,
    height: usize,
    level: f32,
}
impl<'a> LiquidView<'a> {
    fn decode(bytes: &'a [u8], cancelled: &impl Fn() -> bool) -> Result<Self, Error> {
        let header = bytes.get(..16).ok_or(Error::Framing)?;
        if header.get(..4) != Some(b"MLIQ") {
            return Err(Error::Format);
        }
        let flags = header[4];
        if flags & !3 != 0 || header[5] & !0x3f != 0 {
            return Err(Error::Unsupported);
        }
        let offset_x = usize::from(header[8]);
        let offset_y = usize::from(header[9]);
        let width = usize::from(header[10]);
        let height = usize::from(header[11]);
        if offset_x >= 128
            || offset_y >= 128
            || width == 0
            || height == 0
            || offset_x + width > 129
            || offset_y + height > 129
        {
            return Err(Error::Format);
        }
        let heights_at = if flags & 1 == 0 { 16 + 768 } else { 16 };
        let height_bytes = if flags & 2 == 0 {
            width * height * 4
        } else {
            0
        };
        if heights_at + height_bytes != bytes.len() {
            return Err(Error::Framing);
        }
        let level = number(bytes, 12)?;
        let types = if flags & 1 == 0 {
            // Entries are unused by TerrainBuilder, but their complete bytes are framed above.
            let types = bytes.get(16 + 512..16 + 768).ok_or(Error::Framing)?;
            if types.iter().any(|flags| flags & !0x3f != 0) {
                return Err(Error::Unsupported);
            }
            Some(types)
        } else {
            // NO_TYPE fills source flags but never sets liquid_type_loaded.
            None
        };
        let heights = if flags & 2 == 0 {
            let values = bytes.get(heights_at..).ok_or(Error::Framing)?;
            for i in 0..width * height {
                if i % 128 == 0 {
                    check_cancelled(cancelled)?;
                }
                number(values, i * 4)?;
            }
            Some(values)
        } else {
            None
        };
        Ok(Self {
            types,
            heights,
            offset_x,
            offset_y,
            width,
            height,
            level,
        })
    }

    fn original_height(self, index: usize) -> Result<f32, Error> {
        let Some(heights) = self.heights else {
            return Ok(self.level);
        };
        let row = index / 129;
        let col = index % 129;
        if row < self.offset_y
            || row >= self.offset_y + self.height
            || col < self.offset_x
            || col >= self.offset_x + self.width
        {
            return Ok(INVALID_LIQUID);
        }
        number(
            heights,
            ((row - self.offset_y) * self.width + col - self.offset_x) * 4,
        )
    }

    fn cell_flags(self, square: usize) -> u8 {
        // Only called for a validated complete 256-byte type array.
        self.types
            .map_or(0, |types| types[square / 128 / 8 * 16 + square % 128 / 8])
    }
}

fn number(bytes: &[u8], at: usize) -> Result<f32, Error> {
    let value = f32::from_bits(u32_at(bytes, at)?);
    if !value.is_finite() || value.abs() > MAX_HEIGHT {
        return Err(Error::Format);
    }
    Ok(value)
}

fn check_cancelled(cancelled: &impl Fn() -> bool) -> Result<(), Error> {
    if cancelled() {
        Err(Error::Deadline)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    macro_rules! terrain_fixture {
        ($name:literal) => {
            include_bytes!(concat!(
                "../../../../../../../../benilla-native-terrain/tools/fixtures/",
                $name,
                ".map"
            ))
        };
    }

    #[test]
    fn every_original_source_capture_matches_exact_mesh_array_hashes() {
        use sha2::{Digest, Sha256};
        use std::{fs, path::Path};
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let captures = root.join("tools/native-navigation/fixtures");
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(captures.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(
            manifest["source_pin"],
            "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f"
        );
        let hash = |bytes: &[u8]| format!("{:x}", Sha256::digest(bytes));
        for (case, record) in manifest["cases"].as_object().unwrap() {
            let mut mesh = Mesh::default();
            let terrain = record["terrain"].as_str().unwrap();
            let files = record["maps"].as_object().unwrap();
            let events = record["expected"]["terrain_file_events"]
                .as_array()
                .unwrap();
            let mut event_count = 0;
            for (name, x, y, portion, ridge) in [
                ("0003130.map", 30, 31, Portion::Entire, "ridge"),
                ("0003131.map", 31, 31, Portion::Left, "ridge_xplus"),
                ("0003129.map", 29, 31, Portion::Right, "ridge_xminus"),
                ("0003230.map", 30, 32, Portion::Top, "ridge_yplus"),
                ("0003030.map", 30, 30, Portion::Bottom, "ridge_yminus"),
            ] {
                let event = &events[event_count];
                event_count += 1;
                assert_eq!(event["basename"], name, "{case}");
                assert_eq!(event["opened"], files.contains_key(name), "{case}");
                let Some(expected_input_hash) = files.get(name) else {
                    continue;
                };
                let path = if terrain == "ridge" {
                    captures.join("authored").join(format!("{ridge}.map"))
                } else {
                    root.join("crates/benilla-native-terrain/tools/fixtures")
                        .join(format!("{terrain}.map"))
                };
                let bytes = fs::read(path).unwrap();
                assert_eq!(
                    hash(&bytes),
                    expected_input_hash.as_str().unwrap(),
                    "{case}/{name}"
                );
                let used = append_map(&mut mesh, x, y, &bytes, portion, &|| false).unwrap();
                if event_count == 1 && !used {
                    break;
                }
            }
            assert_eq!(event_count, events.len(), "{case}");
            let expected = &record["source-terrain.bin"];
            let mut raw = Vec::new();
            for (name, vertices, triangles) in [
                ("solid", &mesh.solid_vertices, &mesh.solid_triangles),
                ("liquid", &mesh.liquid_vertices, &mesh.liquid_triangles),
            ] {
                let arrays = &expected["arrays"][name];
                assert_eq!(
                    vertices.len() as u64,
                    arrays["vertex_count"].as_u64().unwrap(),
                    "{case}/{name}"
                );
                assert_eq!(
                    triangles.len() as u64,
                    arrays["triangle_count"].as_u64().unwrap(),
                    "{case}/{name}"
                );
                raw.extend_from_slice(&(vertices.len() as u32).to_le_bytes());
                raw.extend_from_slice(&(triangles.len() as u32).to_le_bytes());
                let start = raw.len();
                for vertex in vertices {
                    for value in vertex {
                        raw.extend_from_slice(&value.to_le_bytes());
                    }
                }
                assert_eq!(
                    hash(&raw[start..]),
                    arrays["vertices_sha256"].as_str().unwrap(),
                    "{case}/{name}"
                );
                let start = raw.len();
                for triangle in triangles {
                    for index in triangle {
                        raw.extend_from_slice(&index.to_le_bytes());
                    }
                }
                assert_eq!(
                    hash(&raw[start..]),
                    arrays["triangles_sha256"].as_str().unwrap(),
                    "{case}/{name}"
                );
            }
            raw.extend_from_slice(&mesh.liquid_areas);
            assert_eq!(
                hash(&mesh.liquid_areas),
                expected["arrays"]["liquid"]["types_sha256"]
                    .as_str()
                    .unwrap(),
                "{case}"
            );
            assert_eq!(
                raw.len() as u64,
                expected["bytes"].as_u64().unwrap(),
                "{case}"
            );
            assert_eq!(hash(&raw), expected["sha256"].as_str().unwrap(), "{case}");
        }
    }

    #[test]
    fn cardinal_strips_preserve_vertices_and_source_square_order() {
        let mut entire = Mesh::default();
        assert!(append_map(
            &mut entire,
            31,
            30,
            terrain_fixture!("varied"),
            Portion::Entire,
            &|| false
        )
        .unwrap());
        assert_eq!(entire.solid_vertices.len(), V9 + V8);
        assert_eq!(entire.solid_triangles.len(), V8 * 4);
        assert_eq!(
            entire.solid_triangles[..4],
            [
                [16641, 1, 0],
                [16641, 130, 1],
                [129, 16641, 0],
                [129, 130, 16641],
            ]
        );
        for portion in [Portion::Top, Portion::Left, Portion::Right, Portion::Bottom] {
            let mut strip = Mesh::default();
            assert!(append_map(
                &mut strip,
                31,
                30,
                terrain_fixture!("varied"),
                portion,
                &|| false
            )
            .unwrap());
            assert_eq!(strip.solid_vertices, entire.solid_vertices);
            assert_eq!(strip.solid_triangles.len(), 128 * 4);
            for (i, square) in portion.squares().enumerate() {
                assert_eq!(
                    strip.solid_triangles[i * 4..i * 4 + 4],
                    entire.solid_triangles[square * 4..square * 4 + 4]
                );
            }
        }
    }

    #[test]
    fn no_height_and_uniform_no_type_are_source_skips() {
        let mut mesh = Mesh::default();
        assert!(!append_map(
            &mut mesh,
            31,
            30,
            terrain_fixture!("flat"),
            Portion::Entire,
            &|| false
        )
        .unwrap());
        assert!(mesh.solid_vertices.is_empty());
        // MAP_LIQUID_NO_TYPE leaves the source's liquid_type_loaded false.
        assert!(!append_map(
            &mut mesh,
            31,
            30,
            terrain_fixture!("liquid_water_flat"),
            Portion::Entire,
            &|| false
        )
        .unwrap());
        assert_eq!(mesh.liquid_vertices.len(), V9);
        assert!(mesh.liquid_triangles.is_empty());
        assert!(mesh.liquid_vertices.iter().all(|vertex| vertex[1] == 23.5));
        assert!(append_map(
            &mut mesh,
            31,
            30,
            terrain_fixture!("varied"),
            Portion::Top,
            &|| false
        )
        .unwrap());
        // Early flat/no-liquid exit is false even when earlier calls supplied triangles.
        assert!(!append_map(
            &mut mesh,
            31,
            30,
            terrain_fixture!("flat"),
            Portion::Entire,
            &|| false
        )
        .unwrap());
        // Final result for a present-but-skipped liquid section is aggregate presence.
        assert!(append_map(
            &mut mesh,
            31,
            30,
            terrain_fixture!("liquid_water_flat"),
            Portion::Top,
            &|| false
        )
        .unwrap());
    }

    #[test]
    fn hole_mask_matches_every_source_cell_square() {
        let bytes = terrain_fixture!("nonflat_holes");
        let map = MapView::decode(bytes, &|| false).unwrap();
        let mut mesh = Mesh::default();
        append_map(&mut mesh, 31, 30, bytes, Portion::Entire, &|| false).unwrap();
        let mut at = 0;
        for square in 0..V8 {
            if !map.is_hole(square).unwrap() {
                assert_eq!(mesh.solid_triangles[at..at + 4], solid_triangles(square, 0));
                at += 4;
            }
        }
        assert!(at < V8 * 4);
        assert_eq!(mesh.solid_triangles.len(), at);
        assert_eq!(mesh.solid_vertices.len(), V9 + V8);
    }

    fn authored_liquid(heights: &[f32; 4]) -> Vec<u8> {
        // An adversarial parser/repair case, not independent source parity evidence.
        let flat = terrain_fixture!("flat");
        let height_end = u32_at(flat, 16).unwrap() as usize + u32_at(flat, 20).unwrap() as usize;
        let mut bytes = flat[..height_end].to_vec();
        let mut liquid = b"MLIQ".to_vec();
        liquid.extend_from_slice(&[0, 0, 0, 0, 0, 0, 2, 2]);
        liquid.extend_from_slice(&(-500.0f32).to_le_bytes());
        liquid.extend_from_slice(&[0; 512]);
        liquid.extend_from_slice(&[8; 256]);
        for height in heights {
            liquid.extend_from_slice(&height.to_le_bytes());
        }
        let liquid_len = liquid.len();
        bytes.extend(liquid);
        bytes.extend_from_slice(&[0; 512]);
        bytes[24..28].copy_from_slice(&(height_end as u32).to_le_bytes());
        bytes[28..32].copy_from_slice(&(liquid_len as u32).to_le_bytes());
        bytes[32..36].copy_from_slice(&((height_end + liquid_len) as u32).to_le_bytes());
        bytes
    }

    #[test]
    fn repair_uses_original_values_and_exact_threshold_asymmetry() {
        let bytes = authored_liquid(&[20.0, -500.0, 5000.0, 6000.0]);
        let mut mesh = Mesh::default();
        assert!(append_map(&mut mesh, 32, 32, &bytes, Portion::Entire, &|| false).unwrap());
        assert_eq!(mesh.liquid_vertices[1][1], 20.0);
        assert_eq!(mesh.liquid_vertices[130][1], 20.0);
        assert_eq!(mesh.liquid_vertices[129][1], 5000.0);
        // A repaired neighbor never makes an originally-all-invalid triangle valid.
        assert_eq!(mesh.liquid_triangles, [[130, 1, 0], [129, 130, 0]]);
        assert_eq!(mesh.liquid_areas, [9, 9]);
        assert_eq!(mesh.liquid_vertices[0][0].to_bits(), (-0.0f32).to_bits());
        assert_eq!(mesh.liquid_vertices[0][2].to_bits(), (-0.0f32).to_bits());
    }

    #[test]
    fn cancellation_after_liquid_repair_restores_existing_mesh() {
        let bytes = authored_liquid(&[20.0, -500.0, 5000.0, 6000.0]);
        let calls = Cell::new(0usize);
        let count_calls = || {
            calls.set(calls.get() + 1);
            false
        };
        append_map(
            &mut Mesh::default(),
            31,
            30,
            &bytes,
            Portion::Entire,
            &count_calls,
        )
        .unwrap();
        let last_check = calls.get();
        calls.set(0);
        let cancelled = || {
            calls.set(calls.get() + 1);
            calls.get() == last_check
        };
        let mut mesh = Mesh {
            liquid_vertices: vec![[1.0, -500.0, 3.0]],
            liquid_triangles: vec![[0, 0, 0]],
            liquid_areas: vec![8],
            ..Mesh::default()
        };
        assert_eq!(
            append_map(&mut mesh, 31, 30, &bytes, Portion::Entire, &cancelled),
            Err(Error::Deadline)
        );
        assert_eq!(mesh.liquid_vertices, [[1.0, -500.0, 3.0]]);
        assert_eq!(mesh.liquid_triangles, [[0, 0, 0]]);
        assert_eq!(mesh.liquid_areas, [8]);
    }

    #[test]
    fn unsupported_framing_and_nonfinite_inputs_leave_mesh_unchanged() {
        let original = terrain_fixture!("varied");
        let height_at = u32_at(original, 16).unwrap() as usize;
        for flag in [2u32, 4, 8] {
            let mut bytes = original.to_vec();
            bytes[height_at + 4..height_at + 8].copy_from_slice(&flag.to_le_bytes());
            assert_eq!(
                append_map(
                    &mut Mesh::default(),
                    31,
                    30,
                    &bytes,
                    Portion::Entire,
                    &|| false
                ),
                Err(Error::Unsupported)
            );
        }
        let mut nonfinite = original.to_vec();
        nonfinite[height_at + 16..height_at + 20].copy_from_slice(&f32::NAN.to_le_bytes());
        assert_eq!(
            append_map(
                &mut Mesh::default(),
                31,
                30,
                &nonfinite,
                Portion::Entire,
                &|| false
            ),
            Err(Error::Format)
        );
        for end in (0..40).chain([height_at, height_at + 15, original.len() - 1]) {
            let mut mesh = Mesh::default();
            assert!(append_map(
                &mut mesh,
                31,
                30,
                &original[..end],
                Portion::Entire,
                &|| false
            )
            .is_err());
            assert!(mesh.solid_vertices.is_empty());
        }
        let mut bad_offset = original.to_vec();
        bad_offset[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            append_map(
                &mut Mesh::default(),
                31,
                30,
                &bad_offset,
                Portion::Entire,
                &|| false
            ),
            Err(Error::Framing)
        );
    }

    #[test]
    fn cancellation_rolls_back_partial_append_and_resource_limits_are_explicit() {
        let calls = Cell::new(0);
        let mut mesh = Mesh {
            solid_vertices: vec![[1.0, 2.0, 3.0]],
            solid_triangles: vec![[0, 0, 0]],
            ..Mesh::default()
        };
        let cancelled = || {
            calls.set(calls.get() + 1);
            calls.get() > 270
        };
        assert_eq!(
            append_map(
                &mut mesh,
                31,
                30,
                terrain_fixture!("varied"),
                Portion::Entire,
                &cancelled
            ),
            Err(Error::Deadline)
        );
        assert_eq!(mesh.solid_vertices, [[1.0, 2.0, 3.0]]);
        assert_eq!(mesh.solid_triangles, [[0, 0, 0]]);
        assert!(mesh.liquid_vertices.is_empty());
        mesh.solid_vertices.resize(MAX_VERTICES, [0.0; 3]);
        assert_eq!(reserve_mesh(&mut mesh, true, false, V8), Err(Error::Budget));
        assert_eq!(mesh.solid_vertices.len(), MAX_VERTICES);
    }
}
