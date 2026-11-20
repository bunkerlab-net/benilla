// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;

const WALK: u32 = 0x100; // Object.h, MOVEFLAG_WALK_MODE.
const RUNNING: u32 = 0x0400_0000; // Unit.h, UNIT_STAT_RUNNING.

/// Opcodes.h:276,819-820. The largest owned body is a GUID and seven words.
#[derive(Debug)]
pub struct SourceMovementPacket {
    pub opcode: u16,
    bytes: [u8; 37],
    len: usize,
}
impl SourceMovementPacket {
    fn packed_guid(opcode: u16, guid: u64) -> Self {
        let mut packet = Self {
            opcode,
            bytes: [0; 37],
            len: 1,
        };
        for (index, byte) in guid.to_le_bytes().into_iter().enumerate() {
            if byte != 0 {
                packet.bytes[0] |= 1 << index;
                packet.bytes[packet.len] = byte;
                packet.len += 1;
            }
        }
        packet
    }

    fn word(&mut self, word: u32) {
        self.bytes[self.len..self.len + 4].copy_from_slice(&word.to_le_bytes());
        self.len += 4;
    }

    pub fn body(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

/// Object.cpp:1916-1921. The caller builds the packet before the world guard.
pub fn source_movement_send_to_set(
    in_world: bool,
    packet: SourceMovementPacket,
    broadcast: impl FnOnce(&SourceMovementPacket) -> Result<()>,
) -> Result<()> {
    if in_world {
        broadcast(&packet)
    } else {
        Ok(())
    }
}

/// Creature.cpp:2781-2803. The default state write precedes the equality check.
pub(super) fn source_set_creature_walk(
    state: &mut u32,
    flags: Option<&mut u32>,
    guid: u64,
    walk: bool,
    as_default: bool,
) -> Result<Option<SourceMovementPacket>> {
    if as_default {
        if walk {
            *state &= !RUNNING;
        } else {
            *state |= RUNNING;
        }
    }
    let flags = flags.ok_or_else(|| refuse("execution.movement.Creature_2792.walk_flags_owner"))?;
    if walk == (*flags & WALK != 0) {
        return Ok(None);
    }
    *flags ^= WALK;
    Ok(Some(SourceMovementPacket::packed_guid(
        if walk { 0x30e } else { 0x30d },
        guid,
    )))
}

/// Unit.cpp:731-737 and :266-301. Relocate keeps MovementInfo position with the world pose.
pub fn source_movement_heartbeat(
    guid: SourceGuid,
    movement: &SourceCreatureMovement,
    pose: SourcePose,
) -> Result<SourceMovementPacket> {
    let mut packet = SourceMovementPacket::packed_guid(0x0ee, guid.raw());
    let flags = movement
        .flags
        .ok_or_else(|| refuse("execution.movement.Unit_268.move_flags_owner"))?;
    packet.word(flags);
    packet.word(movement.server_time);
    for word in pose.0 {
        packet.word(word);
    }
    require(
        flags & 0x0200_0000 == 0,
        "execution.movement.Unit_277.transport_info_owner",
    )?;
    require(
        flags & 0x0020_0000 == 0,
        "execution.movement.Unit_285.swimming_pitch_owner",
    )?;
    packet.word(movement.fall_time);
    require(
        flags & 0x0000_2000 == 0,
        "execution.movement.Unit_292.jump_info_owner",
    )?;
    require(
        flags & 0x0400_0000 == 0,
        "execution.movement.Unit_300.spline_elevation_owner",
    )?;
    Ok(packet)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heartbeat_writes_packed_guid_flags_time_pose_and_fall_time() {
        let guid = SourceGuid::from_raw(0x0100_0000_0000_0100);
        let mut movement = SourceCreatureMovement::construct();
        movement.server_time = 0x1234_5678;
        movement.fall_time = 0x1122_3344;
        let pose = SourcePose([0x3f80_0000, 0x4000_0000, 0xc040_0000, 0x8000_0000]);
        let packet = source_movement_heartbeat(guid, &movement, pose).unwrap();
        assert_eq!(packet.opcode, 0x0ee);
        assert_eq!(
            packet.body(),
            [
                0x82, 1, 1, 0, 1, 0, 0, 0x78, 0x56, 0x34, 0x12, 0, 0, 0x80, 0x3f, 0, 0, 0, 0x40, 0,
                0, 0x40, 0xc0, 0, 0, 0, 0x80, 0x44, 0x33, 0x22, 0x11
            ]
        );
        for (flag, owner) in [
            (
                0x0200_0000,
                "execution.movement.Unit_277.transport_info_owner",
            ),
            (
                0x0020_0000,
                "execution.movement.Unit_285.swimming_pitch_owner",
            ),
            (0x0000_2000, "execution.movement.Unit_292.jump_info_owner"),
            (
                0x0400_0000,
                "execution.movement.Unit_300.spline_elevation_owner",
            ),
        ] {
            movement.flags = Some(0x100 | flag);
            assert_eq!(
                source_movement_heartbeat(guid, &movement, pose)
                    .unwrap_err()
                    .id,
                owner
            );
        }
    }

    #[test]
    fn set_walk_writes_default_state_even_when_packet_is_skipped() {
        let mut state = RUNNING;
        let mut flags = WALK;
        assert!(
            source_set_creature_walk(&mut state, Some(&mut flags), 0, true, true)
                .unwrap()
                .is_none()
        );
        assert_eq!(state, 0);
        let guid = 0x0100_0000_0000_1234;
        let run = source_set_creature_walk(&mut state, Some(&mut flags), guid, false, true)
            .unwrap()
            .unwrap();
        assert_eq!(state, RUNNING);
        assert_eq!(flags, 0);
        source_movement_send_to_set(false, run, |_| panic!("out-of-world set")).unwrap();
        let walk = source_set_creature_walk(&mut state, Some(&mut flags), guid, true, false)
            .unwrap()
            .unwrap();
        assert_eq!(state, RUNNING);
        assert_eq!(flags, WALK);
        source_movement_send_to_set(true, walk, |packet| {
            assert_eq!(
                (packet.opcode, packet.body()),
                (0x30e, &[0x83, 0x34, 0x12, 1][..])
            );
            Ok(())
        })
        .unwrap();
        let run = source_set_creature_walk(&mut state, Some(&mut flags), guid, false, false)
            .unwrap()
            .unwrap();
        assert_eq!(
            (run.opcode, run.body()),
            (0x30d, &[0x83, 0x34, 0x12, 1][..])
        );
    }
}
