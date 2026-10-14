#![cfg(test)]
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/core.rs"]
mod core;
#[allow(dead_code)] // Engine-only CoverageError variants are public in the world crate.
#[path = "../../benilla-world/src/terrain_stream/native_coverage/planar.rs"]
mod planar;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/products/mod.rs"]
mod products;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_navigation.rs"]
mod source_navigation;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_vmap.rs"]
mod source_vmap;

#[path = "../../benilla-world/src/terrain_stream/native_coverage/static_mesh.rs"]
mod static_mesh;

#[test]
fn parsed_authored_adt_drives_actual_movement_solver() {
    use benilla_sim::creature_ai::{self as ai, movement::*};
    fn chunk(magic: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut b = magic.to_vec();
        b.extend_from_slice(&(data.len() as u32).to_le_bytes());
        b.extend_from_slice(data);
        b
    }
    let mut bytes = vec![];
    for row in 0..16u32 {
        for col in 0..16u32 {
            let mut h = vec![0u8; 128];
            h[4..8].copy_from_slice(&col.to_le_bytes());
            h[8..12].copy_from_slice(&row.to_le_bytes());
            h[0x14..0x18].copy_from_slice(&128u32.to_le_bytes());
            h[0x68..0x6c]
                .copy_from_slice(&(-(row as f32) * benilla_formats::CHUNK_SIZE).to_le_bytes());
            h[0x6c..0x70]
                .copy_from_slice(&(-(col as f32) * benilla_formats::CHUNK_SIZE).to_le_bytes());
            h[0x70..0x74].copy_from_slice(&5.0f32.to_le_bytes());
            h.extend(chunk(b"TVCM", &vec![0; 145 * 4]));
            bytes.extend(chunk(b"KNCM", &h));
        }
    }
    struct Terrain(benilla_formats::TileMesh);
    impl core::Corridor for Terrain {
        fn certify(
            &mut self,
            from: [f32; 3],
            to: [f32; 3],
            body: core::Body,
        ) -> Result<(), core::Failure> {
            if !self.0.native_coverage_safe {
                return Err(core::Failure::MissingGeometry);
            }
            planar::prove_planar_tile(&self.0.chunks, (32, 32), from, to, body.radius.as_f32())
                .map_err(|_| core::Failure::MissingGeometry)
        }
    }
    let mut terrain = Terrain(benilla_formats::adt_to_tile_mesh(&bytes).unwrap());
    assert!(terrain.0.native_coverage_safe);
    let request = MovementRequest {
        request_id: 1,
        command_id: 1,
        creature_id: 1,
        map_id: 0,
        origin: ai::Point::new(-10.0, -10.0, 5.0).unwrap(),
        goal: ai::Point::new(-14.0, -10.0, 5.0).unwrap(),
        speed: ai::Real::new(7.0).unwrap(),
        elapsed_ms: 50,
        content_revision: Revision([1; 32]),
        geometry_revision: Revision([2; 32]),
        purpose: MovementPurpose::Home,
    };
    let body = core::Body {
        radius: ai::Real::new(0.5).unwrap(),
        height: ai::Real::new(2.0).unwrap(),
    };
    assert!(matches!(
        core::solve(&request, body, &mut terrain).unwrap().result,
        MovementResult::Progress { arrived: false, .. }
    ));
    // Unrelated nonplanar terrain in this same chunk cannot enter the proved footprint.
    terrain.0.chunks[0].positions[9][2] += 0.01;
    assert!(matches!(
        core::solve(&request, body, &mut terrain).unwrap().result,
        MovementResult::Progress { .. }
    ));
    terrain.0.chunks[0].positions[2 * 17 + 9 + 2][2] += 0.01;
    assert_eq!(
        core::solve(&request, body, &mut terrain),
        Err(core::Failure::MissingGeometry)
    );

    // The real parsed source and actual player solver cross an internal chunk seam.
    let origin = ai::Point::new(-32.0, -10.0, 5.0).unwrap();
    let destination = ai::Point::new(-35.0, -10.0, 5.0).unwrap();
    let query = benilla_world_runtime::native::Query::PlayerMovement {
        frame_origin: origin,
        accepted_path: vec![],
        destination,
        elapsed_ms: 500,
        speed: None,
    };
    let player = core::PlayerMotion {
        body,
        speed: ai::Real::new(7.0).unwrap(),
    };
    assert_eq!(
        core::solve_player(&query, player, &mut terrain),
        Ok(benilla_world_runtime::native::Facts::PlayerMovement { accepted: true })
    );
    terrain.0.chunks[16].positions[2][2] += 0.01;
    assert_eq!(
        core::solve_player(&query, player, &mut terrain),
        Err(core::Failure::MissingGeometry)
    );
}
