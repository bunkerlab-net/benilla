// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored complete product bytes → actual query → independent component only.
use super::super::super::{tests::Stage, TileId, VmapLiquidProducts};
use super::super::tests::{fixture, remap};
use super::*;
use benilla_world_runtime::{
    native::environment::{
        EnvironmentCallback, EnvironmentError, EnvironmentMachine, EnvironmentPose,
    },
    PlayerId,
};
use std::time::Duration;

fn dbc<const N: usize>(rows: &[[u32; N]]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for value in [rows.len() as u32, N as u32, N as u32 * 4, 1] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    for row in rows {
        for value in row {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    bytes.push(0);
    bytes
}
fn products() -> VmapLiquidProducts {
    let (spec, files) = fixture();
    let (spec, files) = remap(spec, files, TileId { x: 32, y: 32 });
    let stage = Stage::new(&files);
    // Real loader owns validated bytes; dropping the staging directory is intentional.
    VmapLiquidProducts::load(&stage.0, spec).unwrap()
}
fn snapshot(products: &VmapLiquidProducts, spell: u32) -> TerrainLiquidSnapshot<'_> {
    products
        .load_terrain_wet_liquid(
            &dbc(&[[1, 0, 3, spell]]),
            &dbc::<25>(&[]),
            &dbc::<20>(&[]),
            &dbc::<42>(&[]),
        )
        .unwrap()
}
fn pose(z: f32) -> EnvironmentPose {
    EnvironmentPose {
        position: [0, 0, z.to_bits()],
        collision_height: 2f32.to_bits(),
    }
}
fn player() -> PlayerId {
    PlayerId {
        owner: [2; 16],
        character: [3; 16],
    }
}
fn machine(snapshot: &TerrainLiquidSnapshot<'_>) -> EnvironmentMachine {
    EnvironmentMachine::new(
        snapshot.environment_binding(),
        [1; 16],
        player(),
        snapshot.environment_catalog().clone(),
    )
    .unwrap()
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

#[test]
fn loaded_product_observation_component_checkpoint_and_replay() {
    let products = products();
    let snapshot = snapshot(&products, 0);
    let mut machine = machine(&snapshot);
    let pose = pose(31.0);
    let query = machine.query_for_position(pose).unwrap().unwrap();
    let result = snapshot.query_environment(query, deadline()).unwrap();
    assert_eq!(result.status, LiquidStatus::AboveWater);
    assert_eq!(
        result.data.unwrap(),
        LiquidData {
            entry: 1,
            type_flags: 8,
            level: 30f32.to_bits(),
            depth_level: (-10f32).to_bits()
        }
    );
    machine.set_position_phase(pose, Some(result)).unwrap();
    assert_eq!(machine.state().flags(), 0x40);
    assert_eq!(machine.state().timer_ms(), 100);
    assert_eq!(machine.state().last_liquid(), Some(1));
    let checkpoint = machine.checkpoint();
    let bytes = checkpoint.to_bytes().unwrap();
    let mut restored = EnvironmentMachine::restore(
        snapshot.environment_binding(),
        [1; 16],
        player(),
        snapshot.environment_catalog().clone(),
        &bytes,
    )
    .unwrap();
    machine.discard_records(&checkpoint).unwrap();
    machine.update_phase(99).unwrap();
    assert_eq!(machine.state().timer_ms(), 1);
    assert!(machine.query_for_position(pose).unwrap().is_none());
    machine.set_position_phase(pose, None).unwrap();
    machine.update_phase(1).unwrap();
    let query2 = machine.query_for_position(pose).unwrap().unwrap();
    assert!(query2.ticket > query.ticket);
    let result2 = snapshot.query_environment(query2, deadline()).unwrap();
    machine.set_position_phase(pose, Some(result2)).unwrap();
    restored
        .replay_bytes(&machine.records_to_bytes().unwrap())
        .unwrap();
    assert_eq!(restored.state_digest(), machine.state_digest());
    assert_eq!(restored.state(), machine.state());
}

#[test]
fn actual_spell_or_submersion_callback_requirement_has_no_commit() {
    let products = products();
    for (spell, z, callback) in [
        (
            77,
            31.0,
            EnvironmentCallback::RemoveSpellAuras { spell_id: 77 },
        ),
        (0, 29.0, EnvironmentCallback::UpdateThreatTables),
    ] {
        let snapshot = snapshot(&products, spell);
        assert_eq!(
            snapshot.environment_catalog().get(1).unwrap().spell_id,
            spell
        );
        let mut machine = machine(&snapshot);
        let before = machine.checkpoint().to_bytes().unwrap();
        let query = machine.query_for_position(pose(z)).unwrap().unwrap();
        let result = snapshot.query_environment(query, deadline()).unwrap();
        assert_eq!(
            machine.set_position_phase(pose(z), Some(result)),
            Err(EnvironmentError::CallbacksUnavailable(callback))
        );
        assert_eq!(before, machine.checkpoint().to_bytes().unwrap());
        assert!(machine.records().is_empty());
    }
}

#[test]
fn changed_snapshot_and_changed_pose_cannot_commit() {
    let products = products();
    let snapshot = snapshot(&products, 0);
    let mut machine = machine(&snapshot);
    let query = machine.query_for_position(pose(31.0)).unwrap().unwrap();
    for field in 0..7 {
        let mut changed = query;
        match field {
            0 => changed.snapshot.map ^= 1,
            1 => changed.snapshot.source_epoch[0] ^= 1,
            2 => changed.snapshot.content[0] ^= 1,
            3 => changed.snapshot.inventory[0] ^= 1,
            4 => changed.snapshot.policy[0] ^= 1,
            5 => changed.snapshot.liquid_catalog[0] ^= 1,
            _ => changed.required_type = 0,
        }
        assert_eq!(
            snapshot.query_environment(changed, deadline()),
            Err(Error::Identity)
        );
    }
    let other = products
        .load_terrain_wet_liquid(
            &dbc(&[[1, 0, 3, 77]]),
            &dbc::<25>(&[]),
            &dbc::<20>(&[]),
            &dbc::<42>(&[]),
        )
        .unwrap();
    assert_eq!(
        other.query_environment(query, deadline()),
        Err(Error::Identity)
    );
    let result = snapshot.query_environment(query, deadline()).unwrap();
    let before = machine.checkpoint().to_bytes().unwrap();
    assert_eq!(
        machine.set_position_phase(pose(32.0), Some(result)),
        Err(EnvironmentError::Binding)
    );
    let mut stale = result;
    stale.query.ticket += 1;
    assert_eq!(
        machine.set_position_phase(pose(31.0), Some(stale)),
        Err(EnvironmentError::Binding)
    );
    assert_eq!(before, machine.checkpoint().to_bytes().unwrap());
    assert!(machine.records().is_empty());
}
