// SPDX-License-Identifier: GPL-3.0-or-later
//! MotionMaster.cpp:83-94 uses the loaded group entry, not FormationData slots.
use super::*;

pub(super) fn census(spawns: &SourceEffectiveSpawns, groups: &SourceSpawnGroups) {
    // Columns: no group, group without formation, formation slot zero, other slot.
    let mut counts = [[0_usize; 4]; 2];
    for (key, data) in spawns.pre_terrain_map() {
        if key.kind == GridSpawnKind::Creature && matches!(data.row.movement, 2 | 4) {
            counts[usize::from(data.row.movement == 4)][0] += 1;
        }
    }
    for group in groups.definitions().unwrap().values() {
        if group.kind() != GridSpawnKind::Creature {
            continue;
        }
        for member in &group.members {
            let data = spawns
                .complete_data(SpawnKey {
                    kind: GridSpawnKind::Creature,
                    guid: member.guid,
                })
                .unwrap()
                .unwrap();
            if !matches!(data.row.movement, 2 | 4) {
                continue;
            }
            let movement = usize::from(data.row.movement == 4);
            let branch = if group.formation.is_none() {
                1
            } else if group.waypoint_formation_path(member.guid).is_some() {
                2
            } else {
                3
            };
            counts[movement][0] -= 1;
            counts[movement][branch] += 1;
        }
    }
    println!(
        "F6_FORMATION_PATH_CENSUS waypoint={:?} linear={:?} uninitialized_path_read_stock_reach=0",
        counts[0], counts[1]
    );
}
