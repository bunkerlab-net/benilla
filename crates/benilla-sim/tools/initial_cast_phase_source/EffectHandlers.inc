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

void Spell::EffectEmpty(SpellEffectIndex /*eff_idx*/)
{
    // NOT NEED ANY IMPLEMENTATION CODE, EFFECT POSISBLE USED AS MARKER OR CLIENT INFORM
}
void Spell::EffectProficiency(SpellEffectIndex /*eff_idx*/)
{
    if (!m_caster->IsPlayer())
        return;

    Player* player = static_cast<Player*>(m_caster);

    uint32 subClassMask = m_spellInfo->EquippedItemSubClassMask;
    if (m_spellInfo->EquippedItemClass == ITEM_CLASS_WEAPON && !(player->GetWeaponProficiency() & subClassMask))
    {
        player->AddWeaponProficiency(subClassMask);
        player->SendProficiency(ITEM_CLASS_WEAPON, player->GetWeaponProficiency());
    }
    if (m_spellInfo->EquippedItemClass == ITEM_CLASS_ARMOR && !(player->GetArmorProficiency() & subClassMask))
    {
        player->AddArmorProficiency(subClassMask);
        player->SendProficiency(ITEM_CLASS_ARMOR, player->GetArmorProficiency());
    }
}
void Spell::EffectApplyAura(SpellEffectIndex eff_idx)
{
    if (!unitTarget)
        return;

    // ghost spell check, allow apply any auras at player loading in ghost mode (will be cleanup after load)
    if ((!unitTarget->IsAlive() && !(IsDeathOnlySpell(m_spellInfo) || IsDeathPersistentSpell(m_spellInfo))) &&
        (!unitTarget->IsPlayer() || !((Player*)unitTarget)->GetSession()->PlayerLoading()))
        return;

    // GO auras have caster == nullptr
    Unit* caster = GetAffectiveCaster();

    DEBUG_FILTER_LOG(LOG_FILTER_SPELL_CAST, "Spell: Aura is: %u", m_spellInfo->EffectApplyAuraName[eff_idx]);

    Aura* aur = CreateAura(m_spellInfo, eff_idx, &damage, &m_currentBasePoints[eff_idx], m_spellAuraHolder, unitTarget, caster, m_CastItem, GetScriptValue());
    m_spellAuraHolder->AddAura(aur, eff_idx);
}
