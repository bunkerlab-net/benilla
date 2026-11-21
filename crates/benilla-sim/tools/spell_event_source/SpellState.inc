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

enum SpellState
{
    SPELL_STATE_CREATED    = 0,                             // just created
    SPELL_STATE_TARGETING  = 1,                             // doing initial check
    SPELL_STATE_CASTING    = 2,                             // cast time delay period, non channeled spell
    SPELL_STATE_DELAYED    = 3,                             // spell is delayed (cast time pushed back) TODO: need to be implemented properly
    SPELL_STATE_TRAVELING  = 4,                             // spell casted but need time to hit target(s)
    SPELL_STATE_LANDING    = 5,                             // processing the effects
    SPELL_STATE_CHANNELING = 6,                             // channeled time period spell casting state
    SPELL_STATE_FINISHED   = 7,                             // cast finished to success or fail
};
