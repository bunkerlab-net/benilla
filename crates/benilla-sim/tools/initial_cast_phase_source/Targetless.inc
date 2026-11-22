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

void Spell::DoAllTargetlessEffects(bool dest)
{
    uint32 procAttacker;
    uint32 procVictim;
    uint32 procEx = PROC_EX_NONE;

    uint32 effectMask;
    if (dest) // can have delay
    {
        effectMask = m_destTargetInfo.effectMask &~ m_destTargetInfo.effectMaskProcessed;
        m_destTargetInfo.effectMaskProcessed = m_destTargetInfo.effectMask;
        for (uint32 j = 0; j < MAX_EFFECT_INDEX; ++j)
        {
            if ((effectMask & (1 << j)) != 0)
                HandleEffect(nullptr, nullptr, nullptr, SpellEffectIndex(j));
        }
    }
    else // always immediate
    {
        effectMask = m_targetlessMask;
        for (uint32 j = 0; j < MAX_EFFECT_INDEX; ++j)
        {
            if ((effectMask & (1 << j)) != 0)
                HandleEffect(nullptr, nullptr, nullptr, SpellEffectIndex(j));
        }

        // dest effects that are immediate
        uint32 destMaskImmediate = m_destTargetInfo.effectMask;
        for (uint32 i = 0; i < MAX_EFFECT_INDEX; ++i)
            if (!IsEffectHandledImmediatelySpellLaunch(m_spellInfo, SpellEffectIndex(i)))
                destMaskImmediate &= ~(destMaskImmediate & (1 << i));

        for (uint32 j = 0; j < MAX_EFFECT_INDEX; ++j)
        {
            if ((destMaskImmediate & (1 << j)) != 0)
                HandleEffect(nullptr, nullptr, nullptr, SpellEffectIndex(j));
        }
        m_destTargetInfo.effectMaskProcessed = destMaskImmediate;
    }

    if (effectMask)
    {
        OnHit(SPELL_MISS_NONE);

        if (m_caster)
        {
            PrepareMasksForProcSystem(effectMask, procAttacker, procVictim, m_trueCaster, unitTarget);
            Unit::ProcDamageAndSpell(ProcSystemArguments(m_caster, procAttacker & PROC_FLAG_ON_TRAP_ACTIVATION ? m_targets.getUnitTarget() : nullptr, m_caster ? procAttacker : uint32(PROC_FLAG_NONE), procVictim, procEx, 0, 0, m_attackType, m_spellInfo, this));
        }
    }
}
