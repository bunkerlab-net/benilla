/*
 * This file is part of the CMaNGOS Project. See AUTHORS file for Copyright information
 *
 * This program is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation; either version 2 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program; if not, write to the Free Software
 * Foundation, Inc., 59 Temple Place, Suite 330, Boston, MA  02111-1307  USA
 */

enum MovementFlags
{
    MOVEFLAG_NONE               = 0x00000000,
    MOVEFLAG_FORWARD            = 0x00000001,
    MOVEFLAG_BACKWARD           = 0x00000002,
    MOVEFLAG_STRAFE_LEFT        = 0x00000004,
    MOVEFLAG_STRAFE_RIGHT       = 0x00000008,
    MOVEFLAG_TURN_LEFT          = 0x00000010,
    MOVEFLAG_TURN_RIGHT         = 0x00000020,
    MOVEFLAG_PITCH_UP           = 0x00000040,
    MOVEFLAG_PITCH_DOWN         = 0x00000080,
    MOVEFLAG_WALK_MODE          = 0x00000100,               // Walking

    MOVEFLAG_LEVITATING         = 0x00000400,
    MOVEFLAG_FLYING             = 0x00000800,               // [-ZERO] is it really need and correct value
    MOVEFLAG_JUMPING            = 0x00002000,
    MOVEFLAG_FALLINGFAR         = 0x00004000,
    MOVEFLAG_SWIMMING           = 0x00200000,               // appears with fly flag also
    MOVEFLAG_SPLINE_ENABLED     = 0x00400000,               // [-ZERO] is it really need and correct value
    MOVEFLAG_CAN_FLY            = 0x00800000,               // [-ZERO] is it really need and correct value
    MOVEFLAG_FLYING_OLD         = 0x01000000,               // [-ZERO] is it really need and correct value

    MOVEFLAG_ONTRANSPORT        = 0x02000000,               // Used for flying on some creatures
    MOVEFLAG_SPLINE_ELEVATION   = 0x04000000,               // used for flight paths
    MOVEFLAG_ROOT               = 0x08000000,               // used for flight paths
    MOVEFLAG_WATERWALKING       = 0x10000000,               // prevent unit from falling through water
    MOVEFLAG_SAFE_FALL          = 0x20000000,               // active rogue safe fall spell (passive)
    MOVEFLAG_HOVER              = 0x40000000,

    MOVEFLAG_MASK_MOVING        =
        MOVEFLAG_FORWARD | MOVEFLAG_BACKWARD | MOVEFLAG_STRAFE_LEFT | MOVEFLAG_STRAFE_RIGHT |
        MOVEFLAG_PITCH_UP | MOVEFLAG_PITCH_DOWN | MOVEFLAG_FALLINGFAR |
        MOVEFLAG_SPLINE_ELEVATION,
    MOVEFLAG_MASK_MOVING_FORWARD = MOVEFLAG_FORWARD | MOVEFLAG_STRAFE_LEFT | MOVEFLAG_STRAFE_RIGHT | MOVEFLAG_JUMPING,
    MOVEFLAG_MASK_MOVING_OR_TURN = MOVEFLAG_MASK_MOVING | MOVEFLAG_TURN_LEFT | MOVEFLAG_TURN_RIGHT,
    MOVEFLAG_MASK_XY = MOVEFLAG_FORWARD | MOVEFLAG_BACKWARD | MOVEFLAG_STRAFE_LEFT | MOVEFLAG_STRAFE_RIGHT
};
