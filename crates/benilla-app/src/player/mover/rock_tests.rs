//! Authored geometry, not extracted Valley of Trials assets: exercise a closed rock over
//! separate terrain colliders through the full player mover at origin and Orc-scale coordinates.

use super::*;
use crate::player::{AIR_NUDGE_SPEED, CAPSULE_RADIUS};
use bevy::ecs::system::RunSystemOnce;
use std::time::Duration;

fn rock_world(origin: Vec3, terrain: bool) -> App {
    let mut app = App::new();
    app.init_resource::<benilla_world::collision::MoverTraceExclusions>();
    app.add_plugins((
        MinimalPlugins,
        bevy::transform::TransformPlugin,
        bevy::asset::AssetPlugin::default(),
        bevy::scene::ScenePlugin,
        PhysicsPlugins::new(bevy::app::PostUpdate),
    ));
    app.init_asset::<Mesh>();
    let mut add = |vertices: Vec<Vec3>, triangles| {
        app.world_mut().spawn((
            RigidBody::Static,
            Collider::trimesh(
                vertices.into_iter().map(|v| v + origin).collect(),
                triangles,
            ),
            Transform::default(),
        ));
    };
    if terrain {
        // Two independently baked, up-wound terrain colliders meet beneath the rock.
        for (left, right) in [(-40.0, 0.0), (0.0, 40.0)] {
            add(
                vec![
                    Vec3::new(left, 0.0, -40.0),
                    Vec3::new(right, 0.0, -40.0),
                    Vec3::new(right, 0.0, 40.0),
                    Vec3::new(left, 0.0, 40.0),
                ],
                vec![[0, 2, 1], [0, 3, 2]],
            );
        }
    }
    // A closed frustum with steep sides, a walkable top, and its base below the terrain.
    add(
        [(-0.2, 1.4), (0.8, 0.9)]
            .into_iter()
            .flat_map(|(y, r)| {
                [
                    Vec3::new(-r, y, -r),
                    Vec3::new(r, y, -r),
                    Vec3::new(r, y, r),
                    Vec3::new(-r, y, r),
                ]
            })
            .collect(),
        vec![
            [0, 1, 2],
            [0, 2, 3],
            [4, 6, 5],
            [4, 7, 6],
            [0, 4, 5],
            [0, 5, 1],
            [1, 5, 6],
            [1, 6, 2],
            [2, 6, 7],
            [2, 7, 3],
            [3, 7, 4],
            [3, 4, 0],
        ],
    );
    app.finish();
    app.cleanup();
    app.update();
    app
}

fn walk(mut app: App, start: Vec3, direction: Vec3, hz: u32) -> Vec<Vec3> {
    app.world_mut()
        .run_system_once(move |world: benilla_world::collision::WorldCollision| {
            let capsule = Collider::capsule(CAPSULE_RADIUS, CAPSULE_HEIGHT - 2.0 * CAPSULE_RADIUS);
            let mut player = Player {
                pos: start,
                ..Default::default()
            };
            let mut time = Time::default();
            (0..hz * 3)
                .map(|_| {
                    time.advance_by(Duration::from_secs_f32(1.0 / hz as f32));
                    let launch_y = player.pos.y;
                    let outcome = step(
                        &mut player,
                        &time,
                        &world,
                        &capsule,
                        true,
                        direction,
                        7.0,
                        false,
                        false,
                        None,
                        None,
                        AIR_NUDGE_SPEED,
                    );
                    // The controller advances the arc after the mover, tightening subsequent
                    // airborne ground probes and clearing frozen steering when landing.
                    player.advance_airborne_arc(
                        !outcome.held && (!outcome.grounded || outcome.jumped),
                        outcome.jumped,
                        time.elapsed_secs(),
                        launch_y,
                    );
                    player.pos
                })
                .collect()
        })
        .unwrap()
}

#[test]
fn walking_over_a_closed_rock_never_crosses_the_separate_terrain() {
    let orc = benilla_assets::coords::wow_to_bevy([-618.518, -4251.67, 38.718]);
    for origin in [Vec3::ZERO, orc] {
        for direction in [Vec3::NEG_Z, Vec3::new(1.0, 0.0, -1.0).normalize()] {
            for hz in [20, 60] {
                let frames = walk(
                    rock_world(origin, true),
                    origin - direction * 4.0,
                    direction,
                    hz,
                );
                for (frame, position) in frames.iter().enumerate() {
                    assert!(
                        position.is_finite() && position.y >= origin.y - 0.01,
                        "terrain penetration: origin={origin:?} dir={direction:?} hz={hz} \
                         frame={frame} position={position:?}"
                    );
                }
                let peak = frames.iter().map(|p| p.y - origin.y).fold(0.0, f32::max);
                assert!(peak > 0.5, "fixture never climbed the rock: peak={peak}");
                let end = *frames.last().unwrap();
                assert!(
                    (end - origin).dot(direction) > 3.0 && (end.y - origin.y).abs() < 0.05,
                    "fixture must leave the rock and land on terrain: origin={origin:?} \
                     dir={direction:?} hz={hz} end={end:?}"
                );
            }
        }
    }
}

#[test]
fn walking_off_a_closed_rock_without_terrain_still_falls() {
    let frames = walk(
        rock_world(Vec3::ZERO, false),
        Vec3::new(0.0, 0.82, 0.0),
        Vec3::NEG_Z,
        60,
    );
    assert!(
        frames.last().unwrap().y < -1.0,
        "missing terrain is a real drop, not an artificial height floor"
    );
}
