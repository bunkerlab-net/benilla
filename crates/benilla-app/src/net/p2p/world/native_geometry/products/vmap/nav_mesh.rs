// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded CMaNGOS TerrainBuilder VMap mesh collection, before cleanVertices.
//! Source: 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, TerrainBuilder.cpp:559-770
//! and G3D Matrix3::{fromEulerAnglesXYZ,operator*}. See PROVENANCE.md.
//! The numeric profile is unfused native IEEE RNE/gradual underflow plus the
//! pinned bounded source trig adapter; arbitrary host-libm parity is not claimed.

use super::super::nav_generation::terrain_mesh::Mesh;
use super::{liquid::WmoLiquid, math::Matrix, Error, Spawn, VmapSnapshot};

const GRID_SIZE: f32 = 533.333_3;
const GRID_PART_SIZE: f32 = GRID_SIZE / 128.0;
// Aggregate local resource limits, not source geometry or admission rules.
const MAX_VERTICES: usize = 1_000_000;
const MAX_TRIANGLES: usize = 2_000_000;
const MAX_GROUP_VISITS: usize = 262_144;

#[derive(Default)]
struct Counts {
    solid_vertices: usize,
    solid_triangles: usize,
    liquid_vertices: usize,
    liquid_triangles: usize,
    groups: usize,
}

/// Append resident models once, in global spawn-slot and then source group order.
/// The caller supplies validated resident geometry; this creates no source proof,
/// tile coverage, navigation capability, or gameplay admission. Any failure leaves
/// all original mesh data intact, although successful reservations may change capacity.
pub(in super::super) fn append(
    snapshot: &VmapSnapshot,
    mesh: &mut Mesh,
    cancelled: &impl Fn() -> bool,
) -> Result<(), Error> {
    check(cancelled)?;
    require_numeric_profile()?;
    let counts = count(snapshot, mesh, cancelled)?;
    reserve(mesh, &counts)?;
    let checkpoint = (
        mesh.solid_vertices.len(),
        mesh.solid_triangles.len(),
        mesh.liquid_vertices.len(),
        mesh.liquid_triangles.len(),
        mesh.liquid_areas.len(),
    );
    let result = append_validated(snapshot, mesh, cancelled);
    if result.is_err() {
        mesh.solid_vertices.truncate(checkpoint.0);
        mesh.solid_triangles.truncate(checkpoint.1);
        mesh.liquid_vertices.truncate(checkpoint.2);
        mesh.liquid_triangles.truncate(checkpoint.3);
        mesh.liquid_areas.truncate(checkpoint.4);
    }
    result
}

fn count(
    snapshot: &VmapSnapshot,
    mesh: &Mesh,
    cancelled: &impl Fn() -> bool,
) -> Result<Counts, Error> {
    if mesh.liquid_areas.len() != mesh.liquid_triangles.len() {
        return Err(Error::Framing);
    }
    let mut counts = Counts::default();
    check_totals(mesh, &counts)?;
    for spawn in &snapshot.spawns {
        check(cancelled)?;
        let Some(spawn) = spawn else { continue };
        let model = snapshot.models.get(spawn.model).ok_or(Error::Dependency)?;
        for group in &model.groups {
            check(cancelled)?;
            counts.groups = add(counts.groups, 1)?;
            if counts.groups > MAX_GROUP_VISITS {
                return Err(Error::Budget);
            }
            counts.solid_vertices = add(counts.solid_vertices, group.vertices.len())?;
            counts.solid_triangles = add(counts.solid_triangles, group.triangles.len())?;
            if let Some(liquid) = &group.liquid {
                let (vertices, cells) = liquid_dimensions(liquid)?;
                counts.liquid_vertices = add(counts.liquid_vertices, vertices)?;
                // Reject oversized shapes before walking their flag arrays.
                check_totals(mesh, &counts)?;
                for (index, &flag) in liquid.flags[..cells].iter().enumerate() {
                    if index % 128 == 0 {
                        check(cancelled)?;
                    }
                    if flag & 0x0f != 0x0f {
                        counts.liquid_triangles = add(counts.liquid_triangles, 2)?;
                    }
                }
            }
            check_totals(mesh, &counts)?;
        }
    }
    check(cancelled)?;
    Ok(counts)
}

fn liquid_dimensions(liquid: &WmoLiquid) -> Result<(usize, usize), Error> {
    let vertices = add(liquid.width, 1)?
        .checked_mul(add(liquid.height, 1)?)
        .ok_or(Error::Budget)?;
    let cells = liquid
        .width
        .checked_mul(liquid.height)
        .ok_or(Error::Budget)?;
    if vertices != liquid.heights.len() || cells != liquid.flags.len() {
        return Err(Error::Framing);
    }
    Ok((vertices, cells))
}

fn check_totals(mesh: &Mesh, counts: &Counts) -> Result<(), Error> {
    let vertices = add(
        add(mesh.solid_vertices.len(), mesh.liquid_vertices.len())?,
        add(counts.solid_vertices, counts.liquid_vertices)?,
    )?;
    let triangles = add(
        add(mesh.solid_triangles.len(), mesh.liquid_triangles.len())?,
        add(counts.solid_triangles, counts.liquid_triangles)?,
    )?;
    if vertices > MAX_VERTICES || triangles > MAX_TRIANGLES {
        Err(Error::Budget)
    } else {
        Ok(())
    }
}

fn reserve(mesh: &mut Mesh, counts: &Counts) -> Result<(), Error> {
    mesh.solid_vertices
        .try_reserve_exact(counts.solid_vertices)
        .map_err(|_| Error::Budget)?;
    mesh.solid_triangles
        .try_reserve_exact(counts.solid_triangles)
        .map_err(|_| Error::Budget)?;
    mesh.liquid_vertices
        .try_reserve_exact(counts.liquid_vertices)
        .map_err(|_| Error::Budget)?;
    mesh.liquid_triangles
        .try_reserve_exact(counts.liquid_triangles)
        .map_err(|_| Error::Budget)?;
    mesh.liquid_areas
        .try_reserve_exact(counts.liquid_triangles)
        .map_err(|_| Error::Budget)?;
    Ok(())
}

fn append_validated(
    snapshot: &VmapSnapshot,
    mesh: &mut Mesh,
    cancelled: &impl Fn() -> bool,
) -> Result<(), Error> {
    // The sparse vector retains the global ModelInstance array's source order.
    // Tile-file order and model-file order are not substitutes for this order.
    for spawn in &snapshot.spawns {
        check(cancelled)?;
        let Some(spawn) = spawn else { continue };
        let transform = Transform::new(spawn)?;
        let model = snapshot.models.get(spawn.model).ok_or(Error::Dependency)?;
        for group in &model.groups {
            check(cancelled)?;
            let base = mesh.solid_vertices.len() as u32;
            for (index, &vertex) in group.vertices.iter().enumerate() {
                if index % 128 == 0 {
                    check(cancelled)?;
                }
                mesh.solid_vertices.push(transform.vertex(vertex)?);
            }
            for (index, &triangle) in group.triangles.iter().enumerate() {
                if index % 128 == 0 {
                    check(cancelled)?;
                }
                if triangle.iter().any(|&i| i as usize >= group.vertices.len()) {
                    return Err(Error::Format);
                }
                let [a, b, c] = triangle.map(|i| i + base);
                mesh.solid_triangles.push(if spawn.navigation_m2 {
                    [c, b, a]
                } else {
                    [a, b, c]
                });
            }
            if let Some(liquid) = &group.liquid {
                append_liquid(mesh, liquid, &transform, cancelled)?;
            }
        }
    }
    check(cancelled)?;
    require_numeric_profile()
}

fn append_liquid(
    mesh: &mut Mesh,
    liquid: &WmoLiquid,
    transform: &Transform,
    cancelled: &impl Fn() -> bool,
) -> Result<(), Error> {
    let base = mesh.liquid_vertices.len() as u32;
    let verts_x = liquid.width + 1;
    let verts_y = liquid.height + 1;
    let area = match liquid.liquid_type & 3 {
        0 | 1 => 9, // NAV_AREA_WATER
        2 | 3 => 8, // Source spells this NAV_MAGMA_SLIME; its value is also 8.
        _ => unreachable!(),
    };
    // Source deliberately writes X-major vertices from Y-major height storage.
    for x in 0..verts_x {
        for y in 0..verts_y {
            if (x * verts_y + y).is_multiple_of(128) {
                check(cancelled)?;
            }
            let vertex = [
                finite(liquid.corner[0] + x as f32 * GRID_PART_SIZE)?,
                finite(liquid.corner[1] + y as f32 * GRID_PART_SIZE)?,
                liquid.heights[y * verts_x + x],
            ];
            mesh.liquid_vertices.push(transform.vertex(vertex)?);
        }
    }
    for x in 0..liquid.width {
        for y in 0..liquid.height {
            if (x * liquid.height + y).is_multiple_of(128) {
                check(cancelled)?;
            }
            if liquid.flags[x + y * liquid.width] & 0x0f == 0x0f {
                continue;
            }
            let square = x * liquid.height + y;
            let idx1 = (square + x) as u32 + base;
            let idx2 = (square + 1 + x) as u32 + base;
            let idx3 = (square + liquid.height + 1 + 1 + x) as u32 + base;
            let idx4 = (square + liquid.height + 1 + x) as u32 + base;
            // liqTris has [3,2,1],[4,3,1], then copies each as [1,2,0].
            mesh.liquid_triangles.push([idx2, idx1, idx3]);
            mesh.liquid_triangles.push([idx3, idx1, idx4]);
            mesh.liquid_areas.extend_from_slice(&[area, area]);
        }
    }
    Ok(())
}

struct Transform {
    rotation: Matrix,
    scale: f32,
    position: [f32; 3],
}
impl Transform {
    fn new(spawn: &Spawn) -> Result<Self, Error> {
        if !spawn.scale.is_finite() || !(0.001..=1000.0).contains(&spawn.scale) {
            return Err(Error::Unsupported);
        }
        Ok(Self {
            rotation: rotation(spawn.angles)?,
            scale: spawn.scale,
            position: [
                finite(spawn.position[0] - 32.0 * GRID_SIZE)?,
                finite(spawn.position[1] - 32.0 * GRID_SIZE)?,
                finite(spawn.position[2])?,
            ],
        })
    }

    fn vertex(&self, vertex: [f32; 3]) -> Result<[f32; 3], Error> {
        let mut value = [0.0; 3];
        for (axis, component) in value.iter_mut().enumerate() {
            let column = [
                self.rotation[0][axis],
                self.rotation[1][axis],
                self.rotation[2][axis],
            ];
            // G3D Vector3 * Matrix3, then scale, then position, in that order.
            *component = finite(finite(dot(vertex, column)? * self.scale)? + self.position[axis])?;
        }
        value[0] *= -1.0;
        value[1] *= -1.0;
        Ok([value[1], value[2], value[0]])
    }
}

fn rotation(angles: [f32; 3]) -> Result<Matrix, Error> {
    let [sx, cx] = sin_cos_negative_degrees(angles[2])?;
    let [sy, cy] = sin_cos_negative_degrees(angles[0])?;
    let [sz, cz] = sin_cos_negative_degrees(angles[1])?;
    let x = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
    let y = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
    let z = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
    multiply(x, multiply(y, z)?)
}

fn sin_cos_negative_degrees(angle: f32) -> Result<[f32; 2], Error> {
    if !angle.is_finite() || angle.abs() > 360.0 {
        return Err(Error::Unsupported);
    }
    // G3D::pi() is 3.1415926535898, not the standard library's PI constant.
    // The original expression is binary64 until the float parameter conversion.
    let radians = (f64::from_bits(0x4009_21fb_5444_2d28) * f64::from(angle) / -180.0) as f32;
    benilla_sim::source_trig::sin_cos_radians_bits_native(radians.to_bits())
        .map(|values| values.map(f32::from_bits))
        .map_err(|_| Error::Unsupported)
}

fn multiply(a: Matrix, b: Matrix) -> Result<Matrix, Error> {
    let mut result = [[0.0; 3]; 3];
    for (row, values) in result.iter_mut().enumerate() {
        for (col, value) in values.iter_mut().enumerate() {
            *value = dot(a[row], [b[0][col], b[1][col], b[2][col]])?;
        }
    }
    Ok(result)
}

fn dot(a: [f32; 3], b: [f32; 3]) -> Result<f32, Error> {
    let first = finite(a[0] * b[0])?;
    let second = finite(a[1] * b[1])?;
    let third = finite(a[2] * b[2])?;
    finite(finite(first + second)? + third)
}

fn finite(value: f32) -> Result<f32, Error> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(Error::Unsupported)
    }
}

fn add(a: usize, b: usize) -> Result<usize, Error> {
    a.checked_add(b).ok_or(Error::Budget)
}

fn check(cancelled: &impl Fn() -> bool) -> Result<(), Error> {
    if cancelled() {
        Err(Error::Deadline)
    } else {
        Ok(())
    }
}

fn require_numeric_profile() -> Result<(), Error> {
    use std::hint::black_box as b;
    // Dynamic probes reject directed rounding, FTZ and DAZ without changing them.
    let half32 = b(f32::from_bits(0x3380_0000));
    let half64 = b(f64::from_bits(0x3ca0_0000_0000_0000));
    let supported = cfg!(all(
        target_endian = "little",
        any(
            all(
                target_os = "linux",
                any(target_arch = "x86_64", target_arch = "aarch64")
            ),
            all(target_os = "windows", target_arch = "x86_64"),
            all(target_os = "macos", target_arch = "aarch64")
        )
    )) && (b(1.0f32) + half32).to_bits() == 0x3f80_0000
        && (b(f32::from_bits(0x3f80_0001)) + half32).to_bits() == 0x3f80_0002
        && (b(1.0f64) + half64).to_bits() == 0x3ff0_0000_0000_0000
        && (b(f64::from_bits(0x3ff0_0000_0000_0001)) + half64).to_bits() == 0x3ff0_0000_0000_0002
        && (b(f32::MIN_POSITIVE) * b(0.5f32)).to_bits() == 0x0040_0000
        && (b(f64::MIN_POSITIVE) * b(0.5f64)).to_bits() == 0x0008_0000_0000_0000
        && (b(f32::from_bits(1)) * b(2.0f32)).to_bits() == 2
        && (b(f64::from_bits(1)) * b(2.0f64)).to_bits() == 2;
    if supported {
        Ok(())
    } else {
        Err(Error::Profile)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Cursor, ProductId};
    use super::*;
    use std::{cell::Cell, collections::BTreeMap};

    macro_rules! model_fixture {
        ($name:literal) => {
            (
                $name,
                include_str!(concat!(
                    "../../../../../../../../../tools/native-navigation/fixtures/models/",
                    $name,
                    "/inputs.json"
                )),
            )
        };
    }

    fn decode_hex(value: &str) -> Vec<u8> {
        assert_eq!(value.len() % 2, 0);
        (0..value.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&value[i..i + 2], 16).unwrap())
            .collect()
    }

    fn original_snapshot(input: &str) -> VmapSnapshot {
        let input: serde_json::Value = serde_json::from_str(input).unwrap();
        let mut rows = BTreeMap::new();
        for (name, bytes) in input["products"].as_object().unwrap() {
            let id = match name.as_str() {
                "000.vmtree" => ProductId::VmapTree,
                // The source loads only tileX=30,tileY=31 for these captures.
                "000_30_31.vmtile" => {
                    ProductId::VmapTile(super::super::super::TileId { x: 31, y: 30 })
                }
                name if name.ends_with(".vmtile") => continue,
                name => ProductId::from_vmap_source_name(
                    name.strip_suffix(".vmo").unwrap().as_bytes(),
                    true,
                )
                .unwrap(),
            };
            rows.insert(id, decode_hex(bytes.as_str().unwrap()));
        }
        let borrowed = rows
            .iter()
            .map(|(id, bytes)| (id.clone(), bytes.as_slice()))
            .collect();
        VmapSnapshot::load_rows(
            super::super::SnapshotBinding {
                map: 0,
                content: [7; 32],
                source_epoch: [7; 32],
                inventory: [7; 32],
                policy: [7; 32],
            },
            &borrowed,
            true,
        )
        .unwrap()
    }

    #[test]
    fn original_load_vmap_mesh_arrays_match_all_fourteen_independent_authored_captures() {
        use sha2::{Digest, Sha256};
        let manifest: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../../../../../tools/native-navigation/fixtures/models/manifest.json"
        ))
        .unwrap();
        let cases = [
            model_fixture!("filename_Hull_wmo"),
            model_fixture!("filename_Foo_lower_m2"),
            model_fixture!("filename_Foo_upper_m2"),
            model_fixture!("filename_Foo_m2_wmo"),
            model_fixture!("wmo_flagged_m2"),
            model_fixture!("signed_zero"),
            model_fixture!("rect_liquid_0"),
            model_fixture!("rect_liquid_1"),
            model_fixture!("rect_liquid_2"),
            model_fixture!("rect_liquid_3"),
            model_fixture!("rect_liquid_5"),
            model_fixture!("rect_liquid_7"),
            model_fixture!("rect_liquid_all_hidden"),
            model_fixture!("shared_slots_groups"),
        ];
        assert_eq!(manifest.as_object().unwrap().len(), cases.len());
        for (case, input) in cases {
            let snapshot = original_snapshot(input);
            let mut mesh = Mesh::default();
            append(&snapshot, &mut mesh, &|| false).unwrap();
            let expected = &manifest[case]["source-combined.bin"];
            let mut bytes = Vec::new();
            for (label, vertices, triangles) in [
                ("solid", &mesh.solid_vertices, &mesh.solid_triangles),
                ("liquid", &mesh.liquid_vertices, &mesh.liquid_triangles),
            ] {
                let array = &expected["arrays"][label];
                assert_eq!(
                    vertices.len() as u64,
                    array["vertex_count"].as_u64().unwrap(),
                    "{case} {label}"
                );
                assert_eq!(
                    triangles.len() as u64,
                    array["triangle_count"].as_u64().unwrap(),
                    "{case} {label}"
                );
                let vertex_bytes: Vec<_> = vertices
                    .iter()
                    .flatten()
                    .flat_map(|v| v.to_le_bytes())
                    .collect();
                let triangle_bytes: Vec<_> = triangles
                    .iter()
                    .flatten()
                    .flat_map(|v| v.to_le_bytes())
                    .collect();
                assert_eq!(
                    format!("{:x}", Sha256::digest(&vertex_bytes)),
                    array["vertices_sha256"].as_str().unwrap(),
                    "{case} {label} vertices"
                );
                assert_eq!(
                    format!("{:x}", Sha256::digest(&triangle_bytes)),
                    array["triangles_sha256"].as_str().unwrap(),
                    "{case} {label} triangles"
                );
                bytes.extend_from_slice(&(vertices.len() as u32).to_le_bytes());
                bytes.extend_from_slice(&(triangles.len() as u32).to_le_bytes());
                bytes.extend_from_slice(&vertex_bytes);
                bytes.extend_from_slice(&triangle_bytes);
            }
            bytes.extend_from_slice(&mesh.liquid_areas);
            assert_eq!(
                format!("{:x}", Sha256::digest(&mesh.liquid_areas)),
                expected["arrays"]["liquid"]["types_sha256"]
                    .as_str()
                    .unwrap(),
                "{case} liquid areas"
            );
            assert_eq!(
                bytes.len() as u64,
                expected["bytes"].as_u64().unwrap(),
                "{case}"
            );
            assert_eq!(
                format!("{:x}", Sha256::digest(&bytes)),
                expected["sha256"].as_str().unwrap(),
                "{case} combined binary"
            );
            let mut prefixed = seeded_mesh();
            append(&snapshot, &mut prefixed, &|| false).unwrap();
            assert_eq!(
                prefixed.solid_vertices[1..]
                    .iter()
                    .map(|v| v.map(f32::to_bits))
                    .collect::<Vec<_>>(),
                mesh.solid_vertices
                    .iter()
                    .map(|v| v.map(f32::to_bits))
                    .collect::<Vec<_>>(),
                "{case} prefixed solid vertices"
            );
            assert_eq!(
                prefixed.liquid_vertices[1..]
                    .iter()
                    .map(|v| v.map(f32::to_bits))
                    .collect::<Vec<_>>(),
                mesh.liquid_vertices
                    .iter()
                    .map(|v| v.map(f32::to_bits))
                    .collect::<Vec<_>>(),
                "{case} prefixed liquid vertices"
            );
            assert_eq!(
                prefixed.solid_triangles[1..],
                mesh.solid_triangles
                    .iter()
                    .map(|triangle| triangle.map(|i| i + 1))
                    .collect::<Vec<_>>(),
                "{case} prefixed solid indices"
            );
            assert_eq!(
                prefixed.liquid_triangles[1..],
                mesh.liquid_triangles
                    .iter()
                    .map(|triangle| triangle.map(|i| i + 1))
                    .collect::<Vec<_>>(),
                "{case} prefixed liquid indices"
            );
            assert_eq!(
                prefixed.liquid_areas[1..],
                mesh.liquid_areas,
                "{case} prefixed areas"
            );
        }
    }

    fn seeded_mesh() -> Mesh {
        Mesh {
            solid_vertices: vec![[1.0, -0.0, 3.0]],
            solid_triangles: vec![[0; 3]],
            liquid_vertices: vec![[4.0, 5.0, -0.0]],
            liquid_triangles: vec![[0; 3]],
            liquid_areas: vec![9],
        }
    }

    fn assert_seed(mesh: &Mesh) {
        assert_eq!(mesh.solid_vertices.len(), 1);
        assert_eq!(
            mesh.solid_vertices[0].map(f32::to_bits),
            [1.0, -0.0, 3.0].map(f32::to_bits)
        );
        assert_eq!(mesh.solid_triangles, [[0; 3]]);
        assert_eq!(mesh.liquid_vertices.len(), 1);
        assert_eq!(
            mesh.liquid_vertices[0].map(f32::to_bits),
            [4.0, 5.0, -0.0].map(f32::to_bits)
        );
        assert_eq!(mesh.liquid_triangles, [[0; 3]]);
        assert_eq!(mesh.liquid_areas, [9]);
    }

    fn named_spawn(name: &[u8], flags: u32) -> Spawn {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&flags.to_le_bytes());
        bytes.extend_from_slice(&7u16.to_le_bytes());
        bytes.extend_from_slice(&101u32.to_le_bytes());
        for value in [0.0f32; 6]
            .into_iter()
            .chain([1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0])
        {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&(name.len() as u32).to_le_bytes());
        bytes.extend_from_slice(name);
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let models = BTreeMap::from([(ProductId::from_vmap_source_name(name, false).unwrap(), 0)]);
        let mut cursor = Cursor::new(&bytes);
        let (slot, spawn) = Spawn::decode(&mut cursor, &models, false).unwrap();
        cursor.finish().unwrap();
        assert_eq!(slot, 0);
        spawn
    }

    #[test]
    fn source_m2_substring_is_exact_case_sensitive_and_independent_of_flags() {
        for (name, flip) in [
            ("Example.m2", true),
            ("Example.M2", true),
            ("Embedded.m2Suffix.wmo", true),
            ("Embedded.M2Suffix.wmo", true),
            ("Example.md2", false),
            ("Example.Md2", false),
            ("Example.wmo", false),
        ] {
            for flags in [4, 5] {
                assert_eq!(
                    named_spawn(name.as_bytes(), flags).navigation_m2,
                    flip,
                    "{name} {flags}"
                );
            }
        }
    }

    #[test]
    fn rectangular_liquid_retains_source_vertex_order_flags_winding_and_type_bits() {
        let transform = Transform {
            rotation: rotation([0.0; 3]).unwrap(),
            scale: 1.0,
            position: [0.0; 3],
        };
        let mut liquid = WmoLiquid {
            width: 2,
            height: 3,
            corner: [0.0, 0.0, 999.0],
            liquid_type: 0,
            heights: (0..12).map(|i| i as f32).collect(),
            flags: vec![0, 15, 0x20, 0, 0xff, 0x10],
        };
        for kind in 0..8 {
            liquid.liquid_type = kind;
            let mut mesh = seeded_mesh();
            append_liquid(&mut mesh, &liquid, &transform, &|| false).unwrap();
            assert_eq!(mesh.liquid_vertices.len(), 13);
            for x in 0..3 {
                for y in 0..4 {
                    assert_eq!(
                        mesh.liquid_vertices[1 + x * 4 + y].map(f32::to_bits),
                        [
                            -(y as f32 * GRID_PART_SIZE),
                            (y * 3 + x) as f32,
                            -(x as f32 * GRID_PART_SIZE)
                        ]
                        .map(f32::to_bits),
                        "{x},{y}"
                    );
                }
            }
            assert_eq!(
                mesh.liquid_triangles[1..],
                [
                    [2, 1, 6],
                    [6, 1, 5],
                    [3, 2, 7],
                    [7, 2, 6],
                    [7, 6, 11],
                    [11, 6, 10],
                    [8, 7, 12],
                    [12, 7, 11],
                ]
            );
            assert_eq!(
                mesh.liquid_areas[1..],
                [if kind & 3 < 2 { 9 } else { 8 }; 8]
            );
        }
    }

    #[test]
    fn numeric_domain_is_explicit_and_signed_zero_is_not_canonicalized() {
        require_numeric_profile().unwrap();
        assert_eq!(
            sin_cos_negative_degrees(0.0).unwrap()[0].to_bits(),
            (-0.0f32).to_bits()
        );
        assert_eq!(sin_cos_negative_degrees(-0.0).unwrap()[0].to_bits(), 0);
        for angle in [
            f32::NAN,
            f32::INFINITY,
            360.0f32.next_up(),
            (-360.0f32).next_down(),
        ] {
            assert_eq!(rotation([angle, 0.0, 0.0]), Err(Error::Unsupported));
        }
        assert!(rotation([360.0, -360.0, -0.0]).is_ok());
        assert_eq!(
            dot([f32::MAX, -f32::MAX, 0.0], [2.0, 2.0, 0.0]),
            Err(Error::Unsupported)
        );
        let mut spawn = named_spawn(b"Example.wmo", 4);
        spawn.position = [32.0 * GRID_SIZE, 32.0 * GRID_SIZE, 0.0];
        let transformed = Transform::new(&spawn).unwrap().vertex([0.0; 3]).unwrap();
        assert_eq!(transformed.map(f32::to_bits), [0x8000_0000, 0, 0x8000_0000]);
    }

    #[test]
    fn cancellation_and_numeric_failure_roll_back_every_mesh_array() {
        let snapshot = super::super::tests::snapshot();
        let mut mesh = seeded_mesh();
        assert_eq!(append(&snapshot, &mut mesh, &|| true), Err(Error::Deadline));
        assert_seed(&mesh);
        // Cancel after counting/reservation, after actual solid data was appended.
        let before = Cell::new(0);
        count(&snapshot, &mesh, &|| {
            before.set(before.get() + 1);
            false
        })
        .unwrap();
        let visits = Cell::new(0);
        let cancel_at = before.get() + 7;
        assert_eq!(
            append(&snapshot, &mut mesh, &|| {
                visits.set(visits.get() + 1);
                visits.get() >= cancel_at
            }),
            Err(Error::Deadline)
        );
        assert!(visits.get() >= cancel_at);
        assert_seed(&mesh);

        let mut snapshot = super::super::tests::snapshot();
        let last = snapshot.spawns.iter_mut().rev().flatten().next().unwrap();
        last.scale = f32::MAX;
        assert_eq!(
            append(&snapshot, &mut mesh, &|| false),
            Err(Error::Unsupported)
        );
        assert_seed(&mesh);
    }

    #[test]
    fn every_wet_collection_cancellation_checkpoint_restores_the_original_mesh() {
        let snapshot = original_snapshot(model_fixture!("shared_slots_groups").1);
        let mut successful = seeded_mesh();
        let visits = Cell::new(0usize);
        append(&snapshot, &mut successful, &|| {
            visits.set(visits.get() + 1);
            false
        })
        .unwrap();
        assert!(successful.liquid_triangles.len() > 1);
        assert!(successful.solid_triangles.len() > 1);
        for cancel_at in 1..=visits.get() {
            let mut mesh = seeded_mesh();
            let calls = Cell::new(0usize);
            assert_eq!(
                append(&snapshot, &mut mesh, &|| {
                    calls.set(calls.get() + 1);
                    calls.get() == cancel_at
                }),
                Err(Error::Deadline),
                "checkpoint {cancel_at}"
            );
            assert_seed(&mesh);
        }
    }

    #[test]
    fn resource_and_shape_limits_fail_before_append_and_absent_slots_are_skipped() {
        let mut snapshot = super::super::tests::snapshot();
        let mut mesh = seeded_mesh();
        mesh.liquid_areas.clear();
        assert_eq!(append(&snapshot, &mut mesh, &|| false), Err(Error::Framing));
        mesh.liquid_areas.push(9);
        mesh.solid_vertices.resize(MAX_VERTICES, [0.0; 3]);
        assert_eq!(append(&snapshot, &mut mesh, &|| false), Err(Error::Budget));
        assert_eq!(mesh.solid_vertices.len(), MAX_VERTICES);
        let mut mesh = seeded_mesh();
        snapshot.spawns.iter_mut().for_each(|spawn| *spawn = None);
        append(&snapshot, &mut mesh, &|| false).unwrap();
        assert_seed(&mesh);
    }
}
