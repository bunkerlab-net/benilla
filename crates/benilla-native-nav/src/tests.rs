// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::time::Duration;
const PARAMS: &[u8] = include_bytes!("../tools/fixtures/authored.mmap");
const LEFT: &[u8] = include_bytes!("../tools/fixtures/0003232.mmtile");
const RIGHT: &[u8] = include_bytes!("../tools/fixtures/0003231.mmtile");
fn mesh() -> Mesh {
    Mesh::load(
        PARAMS,
        &[
            TileInput {
                grid: [32, 32],
                bytes: LEFT,
            },
            TileInput {
                grid: [32, 31],
                bytes: RIGHT,
            },
        ],
    )
    .unwrap()
}
fn until() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
fn point(x: f32, y: f32, z: f32) -> [u32; 3] {
    [x, y, z].map(f32::to_bits)
}
#[test]
fn native_writer_bytes_and_direct_pinned_queries_match() {
    let mut mesh = mesh();
    let mut nearest_count = 0;
    let mut routes = 0;
    let mut refusals = 0;
    for line in include_str!("../tools/fixtures/oracle.tsv").lines() {
        let values: Vec<_> = line.split_whitespace().collect();
        let mut at = 0;
        let mut get = || {
            let s = values[at];
            at += 1;
            s
        };
        let mut hex = || u32::from_str_radix(get(), 16).unwrap();
        let a = [hex(), hex(), hex()];
        let b = [hex(), hex(), hex()];
        let nearest_status = hex();
        let ar: u64 = get().parse().unwrap();
        let p = [0; 3].map(|_| u32::from_str_radix(get(), 16).unwrap());
        let over = get() == "1";
        assert_eq!(nearest_status, SUCCESS);
        let nearest = mesh.nearest(a, 5., until());
        if ar != 0 {
            assert_eq!(
                nearest,
                Ok(Nearest {
                    polygon: ar,
                    point: p,
                    over
                })
            );
            nearest_count += 1;
        } else {
            assert_eq!(nearest, Err(Error::Unavailable));
        }
        let status = u32::from_str_radix(get(), 16).unwrap();
        let n: usize = get().parse().unwrap();
        let path: Vec<u64> = (0..n).map(|_| get().parse().unwrap()).collect();
        let straight = u32::from_str_radix(get(), 16).unwrap();
        let nc: usize = get().parse().unwrap();
        let mut corners = Vec::new();
        for _ in 0..nc {
            let p = [0; 3].map(|_| u32::from_str_radix(get(), 16).unwrap());
            let flags = get().parse().unwrap();
            let polygon = get().parse().unwrap();
            corners.push(Corner {
                point: p,
                flags,
                polygon,
            });
        }
        let route = mesh.route(a, b, until());
        if status == SUCCESS && straight == SUCCESS {
            let route = route.unwrap();
            assert_eq!(route.polygons, path);
            assert_eq!(route.corners, corners);
            routes += 1;
        } else if ar != 0 {
            assert!(route.is_err(), "incomplete source result promoted");
            refusals += 1;
        }
    }
    assert!(
        nearest_count > 100 && routes > 50 && refusals > 10,
        "{nearest_count} {routes} {refusals}"
    );
}
fn mutate(offset: usize, value: u32) -> Result<Mesh, Error> {
    let mut b = LEFT.to_vec();
    b[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    Mesh::load(
        PARAMS,
        &[TileInput {
            grid: [32, 32],
            bytes: &b,
        }],
    )
}
#[test]
fn every_truncated_prefix_rejects_before_native_loading() {
    for n in 0..LEFT.len() {
        assert!(
            Mesh::load(
                PARAMS,
                &[TileInput {
                    grid: [32, 32],
                    bytes: &LEFT[..n]
                }]
            )
            .is_err(),
            "prefix {n}"
        );
    }
}
#[test]
fn unsafe_native_counts_references_and_bounds_reject() {
    for (offset, value) in [
        (20 + 24, u32::MAX),
        (20 + 28, 0),
        (20 + 32, 0),
        (20 + 36, 0),
        (20 + 40, u32::MAX),
        (20 + 44, u32::MAX),
        (20 + 48, 0),
        (20 + 52, 1),
        (20 + 56, 0),
        (20 + 72, f32::NAN.to_bits()),
        (20 + 96, f32::MAX.to_bits()),
        (20 + 8, i32::MAX as u32),
        (20 + 100 + 14 * 12 + 4, 0xffff),
    ] {
        assert!(mutate(offset, value).is_err(), "accepted {offset} {value}");
    }
    let mut b = LEFT.to_vec();
    let poly = 20 + 100 + 14 * 12;
    b[poly + 31] = 9;
    assert!(Mesh::load(
        PARAMS,
        &[TileInput {
            grid: [32, 32],
            bytes: &b
        }]
    )
    .is_err());
}
#[test]
fn missing_middle_tiles_and_deadlines_never_yield_complete_routes() {
    let mut only = Mesh::load(
        PARAMS,
        &[TileInput {
            grid: [32, 32],
            bytes: LEFT,
        }],
    )
    .unwrap();
    let grid = validate::GRID;
    let a = point(-grid + 20. * grid / 100., -grid + 20. * grid / 100., 0.);
    let b = point(-grid + 20. * grid / 100., grid * 0.8, 8.);
    assert!(only.route(a, b, until()).is_err());
    let mut mesh = mesh();
    assert_eq!(mesh.route(a, b, Instant::now()), Err(Error::Deadline));
    assert_eq!(mesh.route_with_limit(a, b, until(), 0), Err(Error::Budget));
    assert!(mesh.route(a, b, until()).is_ok());
    assert_eq!(
        mesh.nearest(point(f32::NAN, 0., 0.), 5., until()),
        Err(Error::Format)
    );
    assert_eq!(mesh.nearest(a, f32::INFINITY, until()), Err(Error::Format));
}
#[test]
fn product_grid_and_native_origin_are_not_interchangeable() {
    assert!(Mesh::load(
        PARAMS,
        &[TileInput {
            grid: [31, 32],
            bytes: LEFT
        }]
    )
    .is_err());
    let mut p = PARAMS.to_vec();
    p[0..4].copy_from_slice(&5000f32.to_le_bytes());
    assert!(Mesh::load(
        &p,
        &[TileInput {
            grid: [32, 32],
            bytes: LEFT
        }]
    )
    .is_err());
    assert!(Mesh::load(
        PARAMS,
        &[
            TileInput {
                grid: [32, 32],
                bytes: LEFT
            },
            TileInput {
                grid: [32, 32],
                bytes: LEFT
            }
        ]
    )
    .is_err());
}

#[test]
fn malformed_detail_boundary_and_bv_escape_never_reach_native_code() {
    let links = 20 + 100 + 14 * 12 + 4 * 32;
    let details = links + 18 * 16;
    let tris = details + 4 * 12;
    let bv = tris + 8 * 4;
    let mut cases = Vec::new();
    let mut b = LEFT.to_vec();
    b[tris + 3] = 0;
    cases.push(b);
    let mut b = LEFT.to_vec();
    b[tris] = 200;
    cases.push(b);
    let mut b = LEFT.to_vec();
    b[details + 9] = 0;
    cases.push(b);
    let mut b = LEFT.to_vec();
    b[bv + 12..bv + 16].copy_from_slice(&i32::MIN.to_le_bytes());
    cases.push(b);
    let mut b = LEFT.to_vec();
    b[bv + 12..bv + 16].copy_from_slice(&(-2i32).to_le_bytes());
    cases.push(b);
    let mut b = LEFT.to_vec();
    b[bv + 16 + 12..bv + 16 + 16].copy_from_slice(&999u32.to_le_bytes());
    cases.push(b);
    let mut b = LEFT.to_vec();
    b[bv + 6..bv + 8].copy_from_slice(&0u16.to_le_bytes());
    cases.push(b);
    for b in cases {
        assert!(Mesh::load(
            PARAMS,
            &[TileInput {
                grid: [32, 32],
                bytes: &b
            }]
        )
        .is_err());
    }
}

#[test]
fn deterministic_mutations_preserve_safe_loader_and_query_boundary() {
    let mut accepted = 0;
    for i in 0..LEFT.len() {
        let mut b = LEFT.to_vec();
        b[i] ^= 1;
        if let Ok(mut mesh) = Mesh::load(
            PARAMS,
            &[TileInput {
                grid: [32, 32],
                bytes: &b,
            }],
        ) {
            // Any accepted mutation must still be safe for actual pinned native queries.
            let _ = mesh.nearest(point(-400., -400., 0.), 5., until());
            let _ = mesh.route(point(-400., -400., 0.), point(-400., -200., 0.), until());
            accepted += 1;
        }
    }
    assert!(accepted > 0);
}

#[test]
fn odd_and_even_vertex_layouts_are_both_natively_aligned() {
    let odd = include_bytes!("../tools/fixtures/0003232-odd.mmtile");
    let mut even = Mesh::load(
        PARAMS,
        &[TileInput {
            grid: [32, 32],
            bytes: LEFT,
        }],
    )
    .unwrap();
    let mut odd = Mesh::load(
        PARAMS,
        &[TileInput {
            grid: [32, 32],
            bytes: odd,
        }],
    )
    .unwrap();
    for p in [
        point(-400., -400., 0.),
        point(-400., -200., 0.),
        point(-400., -400., 10.),
    ] {
        assert_eq!(even.nearest(p, 5., until()), odd.nearest(p, 5., until()));
    }
}

#[test]
fn actual_native_node_exhaustion_and_path_truncation_are_errors() {
    for (n, bytes, detail) in [
        (
            300,
            include_bytes!("../tools/fixtures/long300.mmtile").as_slice(),
            16,
        ),
        (
            600,
            include_bytes!("../tools/fixtures/long600.mmtile").as_slice(),
            32,
        ),
    ] {
        let mut mesh = Mesh::load(
            PARAMS,
            &[TileInput {
                grid: [32, 32],
                bytes,
            }],
        )
        .unwrap();
        let grid = validate::GRID;
        let cs = grid / n as f32;
        let from = point(-grid + 50. * cs, -grid + 0.5 * cs, 0.);
        let to = point(-grid + 50. * cs, -grid + (n as f32 - 0.5) * cs, 0.);
        match mesh.route(from, to, until()) {
            Err(Error::Native(status)) => assert_ne!(status & detail, 0, "{n} {status:x}"),
            other => panic!("expected incomplete native result, got {other:?}"),
        }
        // Cleanup after both failure kinds leaves subsequent nearest/short routes valid.
        assert!(mesh.nearest(from, 5., until()).is_ok());
        assert!(mesh.route(from, from, until()).is_ok());
    }
}
