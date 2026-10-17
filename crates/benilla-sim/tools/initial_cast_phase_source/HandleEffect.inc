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

void Spell::HandleEffect(Unit* pUnitTarget, Item* pItemTarget, GameObject* pGOTarget, SpellEffectIndex i, float DamageMultiplier)
{
    if (m_effectTriggerChance[i] != -1)
    {
        if (m_effectTriggerChance[i] == 0 || irand(1, 100) > m_effectTriggerChance[i])
            return;
    }

    unitTarget = pUnitTarget;
    itemTarget = pItemTarget;
    gameObjTarget = pGOTarget;

    uint8 eff = m_spellInfo->Effect[i];

    if (IsEffectWithImplementedMultiplier(eff))
    {
        m_healingPerEffect[i] = 0;
        m_damagePerEffect[i] = 0;
        damage = CalculateSpellEffectValue(i, unitTarget);
    }
    else
        damage = int32(CalculateSpellEffectValue(i, unitTarget) * DamageMultiplier);

    DEBUG_FILTER_LOG(LOG_FILTER_SPELL_CAST, "Spell %u Effect%d : %u Targets: %s, %s, %s",
                     m_spellInfo->Id, i, eff,
                     unitTarget ? unitTarget->GetGuidStr().c_str() : "-",
                     itemTarget ? itemTarget->GetGuidStr().c_str() : "-",
                     gameObjTarget ? gameObjTarget->GetGuidStr().c_str() : "-");

    if (eff < MAX_SPELL_EFFECTS)
    {
        OnEffectExecute(i);
        damagePerEffect[i] = damage;
        (*this.*SpellEffects[eff])(i);
    }
    else
        sLog.outError("WORLD: Spell FX %d > MAX_SPELL_EFFECTS ", eff);

    if (IsEffectWithImplementedMultiplier(eff))
    {
        if (m_healingPerEffect[i] > 0)
            m_healing += int32(m_healingPerEffect[i] * DamageMultiplier);
        else if (m_damagePerEffect[i] > 0)
            m_damage += int32(m_damagePerEffect[i] * DamageMultiplier);
    }
}
bool Spell::IsEffectWithImplementedMultiplier(uint32 effectId) const
{
    // TODO: extend this for all effects that do damage and healing
    switch (effectId)
    {
        case SPELL_EFFECT_SCHOOL_DAMAGE:
        case SPELL_EFFECT_HEAL:
        case SPELL_EFFECT_POWER_BURN:
        case SPELL_EFFECT_HEAL_MECHANICAL:
        // weapon based
        case SPELL_EFFECT_WEAPON_DAMAGE_NOSCHOOL:
        case SPELL_EFFECT_WEAPON_PERCENT_DAMAGE:
        case SPELL_EFFECT_WEAPON_DAMAGE:
        case SPELL_EFFECT_NORMALIZED_WEAPON_DMG:
            return true;
        default:
            return false;
    }
}
