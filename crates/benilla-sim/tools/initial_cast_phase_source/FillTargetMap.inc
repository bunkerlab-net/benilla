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

void Spell::FillTargetMap()
{
    // TODO: ADD the correct target FILLS!!!!!!
    TempTargetingData targetingData;
    uint8 effToIndex[MAX_EFFECT_INDEX] = {0, 1, 2};         // Helper array, to link to another tmpUnitList, if the targets for both effects match
    for (uint32 i = 0; i < MAX_EFFECT_INDEX; ++i)
    {
        // not call for empty effect.
        // Also some spells use not used effect targets for store targets for dummy effect in triggered spells
        if (m_spellInfo->Effect[i] == SPELL_EFFECT_NONE)
            continue;

        auto& data = SpellTargetMgr::GetSpellTargetingData(m_spellInfo->Id);
        SpellTargetImplicitType effectTargetType = data.implicitType[i]; // prefilled data on load
        auto& targetMask = data.targetMask[i];
        auto& ignoredTargets = data.ignoredTargets[i];
        auto& filterScheme = m_filteringScheme[i];
        targetingData.chainTargetCount[i] = m_spellInfo->EffectChainTarget[SpellEffectIndex(i)];

        if (effectTargetType == TARGET_TYPE_SPECIAL_UNIT) // area auras need custom handling
        {
            uint32 targetA = m_spellInfo->EffectImplicitTargetA[i];
            uint32 targetB = m_spellInfo->EffectImplicitTargetB[i];
            bool hadTarget = false;
            // need to pick a single unit target if existant and use it for area aura owner
            if (targetA && !ignoredTargets.first)
            {
                if (SpellTargetInfoTable[targetA].type == TARGET_TYPE_UNIT && SpellTargetInfoTable[targetA].enumerator == TARGET_ENUMERATOR_SINGLE)
                    hadTarget = true;
                SetTargetMap(SpellEffectIndex(i), targetA, false, targetingData);
            }
            if (targetB && !ignoredTargets.second)
            {
                if (SpellTargetInfoTable[targetB].type == TARGET_TYPE_UNIT && SpellTargetInfoTable[targetB].enumerator == TARGET_ENUMERATOR_SINGLE)
                    hadTarget = true;
                if (SpellTargetInfoTable[targetB].enumerator != TARGET_ENUMERATOR_AOE)
                    SetTargetMap(SpellEffectIndex(i), targetB, true, targetingData);
            }
            if (!hadTarget && m_caster) // if no single targeting available use caster as default
                targetingData.data[i].tmpUnitList[false].push_back(m_caster);
        }
        else
        {
            uint32 targetA = m_spellInfo->EffectImplicitTargetA[i];
            uint32 targetB = m_spellInfo->EffectImplicitTargetB[i];
            if (targetA == TARGET_NONE && targetB == TARGET_NONE)
            {
                // if no targeting available, attempt to use entry mask
                if (m_spellInfo->Targets && SpellTargetMgr::CanEffectBeFilledWithMask(m_spellInfo->Id, i, m_spellInfo->Targets))
                    FillFromTargetFlags(targetingData, SpellEffectIndex(i));
                else if (uint32 defaultTarget = SpellEffectInfoTable[m_spellInfo->Effect[i]].defaultTarget) // else resort to default effect type if it exists
                    SetTargetMap(SpellEffectIndex(i), defaultTarget, false, targetingData);
            }
            else // normal case, use existing spell data
            {
                if (targetA && !ignoredTargets.first)
                    SetTargetMap(SpellEffectIndex(i), targetA, false, targetingData);
                if (targetB && !ignoredTargets.second)
                    SetTargetMap(SpellEffectIndex(i), targetB, true, targetingData);
                if (effectTargetType == TARGET_TYPE_UNIT_DEST) // special case - no unit target, but need to check for valid units
                    if (SpellTargetInfoTable[targetA].type != TARGET_TYPE_UNIT && SpellTargetInfoTable[targetB].type != TARGET_TYPE_UNIT) // no fill for unit out of targets
                        FillFromTargetFlags(targetingData, SpellEffectIndex(i)); // inefficient call, very rare, less code duplicity
            }
        }

        bool processedUnits = false; // TODO: Review area aura packet fills for area auras with AOE targets
        bool processedGOs = false; // likely purpose - animations
        switch (effectTargetType)
        {
            case TARGET_TYPE_NONE:
                m_targetlessMask |= (1 << i);
                break;
            case TARGET_TYPE_LOCATION_DEST:
            case TARGET_TYPE_SPECIAL_DEST:
                OnDestTarget();
                AddDestExecution(SpellEffectIndex(i));
                if (effectTargetType == TARGET_TYPE_SPECIAL_DEST)
                    if (!FillUnitTargets(targetingData, data, i))
                        return;
                break;
            case TARGET_TYPE_CORPSE: // can be unit and corpse
                if (!targetingData.data[i].tempCorpseList.empty())
                {
                    CorpseList& corpseTargetList = targetingData.data[i].tempCorpseList;
                    uint8 effectMask = targetMask[0] | targetMask[1];
                    for (Corpse* corpse : corpseTargetList)
                        AddCorpseTarget(corpse, effectMask);
                }
            case TARGET_TYPE_UNIT:
            case TARGET_TYPE_UNIT_DEST:
            case TARGET_TYPE_PLAYER: // for now player handled here
            case TARGET_TYPE_SPECIAL_UNIT:
                if (effectTargetType == TARGET_TYPE_UNIT_DEST)
                    OnDestTarget();
                processedUnits = true;
                if (!FillUnitTargets(targetingData, data, i))
                    return;
                break;
            case TARGET_TYPE_LOCK:
            case TARGET_TYPE_ITEM:
                if (targetingData.data[i].tempItemList.size() > 0) // Item case
                    for (Item* item : targetingData.data[i].tempItemList)
                        AddItemTarget(item, targetMask[0]); // no spell in all of vanilla through wotlk has item target in B
                if (effectTargetType == TARGET_TYPE_ITEM)
                    break;
                // [[fallthrough]]
            case TARGET_TYPE_GAMEOBJECT:
                processedGOs = true;
                for (uint8 rightTarget = 0; rightTarget < 2; ++rightTarget) // need to process target A and B separately due to effect masks
                {
                    bool ignored = rightTarget ? ignoredTargets.second : ignoredTargets.first;
                    if (ignored)
                        continue;
                    GameObjectList& goTargetList = targetingData.data[i].tmpGOList[rightTarget];
                    uint8 effectMask = targetMask[rightTarget];
                    if (!goTargetList.empty()) // GO case
                    {
                        for (auto iter = goTargetList.begin(); iter != goTargetList.end();)
                        {
                            if (OnCheckTarget(*iter, SpellEffectIndex(i)))
                                ++iter;
                            else
                                iter = goTargetList.erase(iter);
                        }
                        if (m_affectedTargetCount && goTargetList.size() > m_affectedTargetCount)
                        {
                            // remove random units from the map
                            while (goTargetList.size() > m_affectedTargetCount)
                            {
                                uint32 poz = urand(0, goTargetList.size() - 1);
                                for (auto itr = goTargetList.begin(); itr != goTargetList.end(); ++itr, --poz)
                                {
                                    if (!*itr) continue;

                                    if (!poz)
                                    {
                                        goTargetList.erase(itr);
                                        break;
                                    }
                                }
                            }
                        }
                        for (GameObject* go : goTargetList)
                            AddGOTarget(go, effectMask);
                    }
                }
                break;
            default:
                break;
        }
    }

    if (m_spellInfo->HasAttribute(SPELL_ATTR_EX2_FAIL_ON_ALL_TARGETS_IMMUNE))
    {
        bool allImmune = true;
        for (auto& ihit : m_UniqueTargetInfo)
        {
            if (ihit.missCondition != SPELL_MISS_IMMUNE && ihit.missCondition != SPELL_MISS_IMMUNE2)
            {
                allImmune = false;
                break;
            }
        }

        if (allImmune)
        {
            SendCastResult(SPELL_FAILED_IMMUNE); // guessed error
            finish(false);
            return;
        }
    }

    if (m_spellInfo->HasAttribute(SPELL_ATTR_EX_REQUIRE_ALL_TARGETS))
    {
        for (auto& ihit : m_UniqueTargetInfo)
        {
            if (ihit.targetGUID == m_targets.getUnitTargetGuid() && ihit.missCondition != SPELL_MISS_NONE)
            {
                for (auto& ihit : m_UniqueTargetInfo)
                {
                    ihit.effectHitMask = 0;
                    ihit.missCondition = SPELL_MISS_IMMUNE2;
                    ihit.effectDuration = 0;
                }
                m_duration = 0;
            }
        }
    }
}
