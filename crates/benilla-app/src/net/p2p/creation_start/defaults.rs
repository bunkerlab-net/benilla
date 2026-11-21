// SPDX-License-Identifier: GPL-3.0-or-later
//! Eight starting-pose facts, checked against Classic-DB ec4f5961.
//! Original lookup code; no SQL, world export, client asset or native admission.
//! See docs/CLASSIC-EXPLORATION-STARTS.md and the adjacent retained source notices.
use benilla_profile::{Error, Position};

pub(super) fn position(race: u8) -> Result<Position, Error> {
    // Class validity is checked by CreationStart::load before choosing this pose.
    // Dwarves and gnomes share a location but not their initial facing.
    let (map_id, zone_id, position, orientation) = match race {
        1 => (0, 12, [-8949.95, -132.493, 83.5312], 0.0),
        2 => (1, 14, [-618.518, -4251.67, 38.718], 0.0),
        3 => (0, 1, [-6240.32, 331.033, 382.758], 6.17716),
        4 => (1, 141, [10311.3, 831.463, 1326.41], 5.48033),
        5 => (0, 85, [1676.35, 1677.45, 121.67], 2.70526),
        6 => (1, 215, [-2917.58, -257.98, 52.9968], 0.0),
        7 => (0, 1, [-6240.32, 331.033, 382.758], 0.0),
        8 => (1, 14, [-618.518, -4251.67, 38.718], 0.0),
        _ => return Err(Error::Invalid("unsupported racial starting location")),
    };
    Ok(Position {
        map_id,
        zone_id,
        position,
        orientation,
    })
}
