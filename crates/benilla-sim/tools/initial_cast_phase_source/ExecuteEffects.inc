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

void Spell::ExecuteEffects(Unit* unitTarget, Item* itemTarget, GameObject* GOTarget, uint32 effectMask)
{
    Unit* affectiveCaster = GetAffectiveCaster();
    for (uint32 i = 0; i < MAX_EFFECT_INDEX; ++i)
    {
        if (effectMask & (1 << i))
        {
            HandleEffect(unitTarget, itemTarget, GOTarget, SpellEffectIndex(i), m_damageMultipliers[i]);
            if (m_applyMultiplierMask & (1 << i))
            {
                // Get multiplier
                float multiplier = m_spellInfo->DmgMultiplier[i];
                // Apply multiplier mods
                if (affectiveCaster)
                    if (Player* modOwner = affectiveCaster->GetSpellModOwner())
                        modOwner->ApplySpellMod(m_spellInfo->Id, SPELLMOD_EFFECT_PAST_FIRST, multiplier);
                m_damageMultipliers[i] *= multiplier;
            }
        }
    }
}
