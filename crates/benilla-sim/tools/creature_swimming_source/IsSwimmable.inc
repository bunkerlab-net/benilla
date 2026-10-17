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

bool TerrainInfo::IsSwimmable(float x, float y, float z, float radius /*= 1.5f*/, GridMapLiquidData* data /*= 0*/) const
{
    // Check surface in x, y point for liquid
    if (CanCheckLiquidLevel(x, y))
    {
        GridMapLiquidData liquid_status;
        GridMapLiquidData* liquid_ptr = data ? data : &liquid_status;
        if (getLiquidStatus(x, y, z, MAP_ALL_LIQUIDS, liquid_ptr))
        {
            if (liquid_ptr->level - liquid_ptr->depth_level > radius) // is unit have enough space to swim
                return true;
        }
    }
    return false;
}
