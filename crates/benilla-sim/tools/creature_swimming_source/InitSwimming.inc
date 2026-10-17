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

    // check if we need to add swimming movement. TODO: i thing movement flags should be computed automatically at each movement of creature so we need a sort of UpdateMovementFlags() method
    if (cinfo->InhabitType & INHABIT_WATER &&               // check inhabit type water
            !(cinfo->ExtraFlags & CREATURE_EXTRA_FLAG_WALK_IN_WATER) &&  // check if creature is forced to walk (crabs, giant,...)
            GetMap()->GetTerrain()->IsSwimmable(m_respawnPos.x, m_respawnPos.y, m_respawnPos.z, GetCollisionHeight()))  // check if creature is in water and have enough space to swim
        m_movementInfo.AddMovementFlag(MOVEFLAG_SWIMMING);  // add swimming movement
