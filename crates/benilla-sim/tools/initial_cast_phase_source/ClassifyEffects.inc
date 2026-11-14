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

        // figure out what targeting dynamic effects should use
        for (uint32 effIdx = 0; effIdx < MAX_EFFECT_INDEX; ++effIdx)
        {
            if (!spellInfo->Effect[effIdx])
                continue;

            uint32 effect = spellInfo->Effect[effIdx];
            uint32 targetA = spellInfo->EffectImplicitTargetA[effIdx];
            uint32 targetB = spellInfo->EffectImplicitTargetB[effIdx];
            if (SpellEffectInfoTable[effect].requiredTarget != TARGET_TYPE_DYNAMIC && SpellEffectInfoTable[effect].requiredTarget != TARGET_TYPE_DYNAMIC_ANY)
                data.implicitType[effIdx] = SpellEffectInfoTable[effect].requiredTarget;
            else
            {
                if (SpellTargetInfoTable[targetA].type == TARGET_TYPE_LOCK || SpellTargetInfoTable[targetB].type == TARGET_TYPE_LOCK)
                {
                    data.implicitType[effIdx] = TARGET_TYPE_LOCK;
                    continue;
                }
                if (SpellTargetInfoTable[targetA].type == TARGET_TYPE_CORPSE || SpellTargetInfoTable[targetB].type == TARGET_TYPE_CORPSE)
                {
                    data.implicitType[effIdx] = TARGET_TYPE_CORPSE;
                    continue;
                }
                if (SpellTargetInfoTable[targetA].type == TARGET_TYPE_GAMEOBJECT || SpellTargetInfoTable[targetB].type == TARGET_TYPE_GAMEOBJECT)
                {
                    data.implicitType[effIdx] = TARGET_TYPE_GAMEOBJECT;
                    continue;
                }
                if ((SpellTargetInfoTable[targetA].type == TARGET_TYPE_LOCATION_DEST && SpellTargetInfoTable[targetB].type == TARGET_TYPE_UNIT)
                        || (SpellTargetInfoTable[targetA].type == TARGET_TYPE_UNIT && SpellTargetInfoTable[targetB].type == TARGET_TYPE_LOCATION_DEST))
                {
                    data.implicitType[effIdx] = TARGET_TYPE_UNIT_DEST;
                    continue;
                }
                if (SpellTargetInfoTable[targetA].type == TARGET_TYPE_UNIT || SpellTargetInfoTable[targetB].type == TARGET_TYPE_UNIT)
                {
                    data.implicitType[effIdx] = TARGET_TYPE_UNIT;
                    continue;
                }
                if (SpellTargetInfoTable[targetA].type == TARGET_TYPE_LOCATION_DEST || SpellTargetInfoTable[targetB].type == TARGET_TYPE_LOCATION_DEST ||
                    SpellEffectInfoTable[effect].requiredTarget == TARGET_TYPE_DYNAMIC_ANY) // some effects cant be none
                {
                    data.implicitType[effIdx] = TARGET_TYPE_LOCATION_DEST;
                    continue;
                }
                if ((SpellTargetInfoTable[targetA].type == TARGET_TYPE_LOCATION_SRC || targetA == TARGET_NONE) && targetB == TARGET_NONE)
                {
                    data.implicitType[effIdx] = TARGET_TYPE_NONE;
                    continue;
                }
                sLog.outError("Spell %u effect index %u failed to pick type for dynamic effect targeting type.", i, effIdx);
            }
        }
