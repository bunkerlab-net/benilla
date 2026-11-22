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

int32 WorldObject::CalculateSpellEffectValue(Unit const* target, SpellEntry const* spellProto, SpellEffectIndex effect_index, int32 const* effBasePoints, bool maximum, bool finalUse) const
{
    Unit const* unitCaster = dynamic_cast<Unit const*>(this);
    Player const* unitPlayer = (GetTypeId() == TYPEID_PLAYER) ? static_cast<Player const*>(this) : nullptr;

    uint8 comboPoints = unitPlayer ? unitPlayer->GetComboPoints() : 0;

    int32 baseDice = int32(spellProto->EffectBaseDice[effect_index]);
    float basePointsPerLevel = spellProto->EffectRealPointsPerLevel[effect_index];
    float randomPointsPerLevel = spellProto->EffectDicePerLevel[effect_index];
    float basePoints = effBasePoints
        ? *effBasePoints - baseDice
        : spellProto->EffectBasePoints[effect_index];

    float comboDamage = spellProto->EffectPointsPerComboPoint[effect_index];
    int32 randomPoints = spellProto->EffectDieSides[effect_index];
    if (unitCaster && basePointsPerLevel != 0.f)
    {
        int32 level = int32(unitCaster->GetSpellRank(spellProto) / 5);
        if (level > (int32)spellProto->maxLevel && spellProto->maxLevel > 0)
            level = (int32)spellProto->maxLevel;
        else if (level < (int32)spellProto->baseLevel)
            level = (int32)spellProto->baseLevel;
        level -= (int32)spellProto->spellLevel;
        basePoints += level * basePointsPerLevel;
        randomPoints += int32(level * randomPointsPerLevel);
    }

    switch (randomPoints)
    {
        case 0:
        case 1: basePoints += baseDice; break;              // range 1..1
        default:
        {
            if (maximum)
                basePoints += randomPoints;
            else
            {
                // range can have positive (1..rand) and negative (rand..1) values, so order its for irand
                int32 randvalue = baseDice >= randomPoints
                    ? irand(randomPoints, baseDice)
                    : irand(baseDice, randomPoints);

                basePoints += randvalue;
            }
            break;
        }
    }

    float value = basePoints;

    // random damage
    if (comboDamage != 0.0f && unitPlayer && target && (target->GetObjectGuid() == unitPlayer->GetComboTargetGuid() || IsOnlySelfTargeting(spellProto)))
        value += (int32)(comboDamage * comboPoints);

    if (unitCaster)
        if (Player* modOwner = unitCaster->GetSpellModOwner())
            modOwner->ApplySpellMod(spellProto->Id, SPELLMOD_ALL_EFFECTS, value, finalUse);

    if (unitCaster && unitCaster->IsCreature() && spellProto->HasAttribute(SPELL_ATTR_SCALES_WITH_CREATURE_LEVEL) && spellProto->spellLevel)
    {
        // TODO: Drastically beter than before, but still needs some additional aura scaling research
        bool damage = false;
        if (uint32 aura = spellProto->EffectApplyAuraName[effect_index])
        {
            switch (aura)
            {
                case SPELL_AURA_PERIODIC_DAMAGE:
                case SPELL_AURA_PERIODIC_LEECH:
                case SPELL_AURA_SCHOOL_ABSORB:
                    //   SPELL_AURA_PERIODIC_DAMAGE_PERCENT: excluded, abs values only
                case SPELL_AURA_POWER_BURN_MANA:
                case SPELL_AURA_PERIODIC_MANA_LEECH:
                    damage = true;
            }
        }
        else if (uint32 effect = spellProto->Effect[effect_index])
        {
            switch (effect)
            {
                case SPELL_EFFECT_SCHOOL_DAMAGE:
                case SPELL_EFFECT_POWER_DRAIN:
                case SPELL_EFFECT_ENVIRONMENTAL_DAMAGE:
                case SPELL_EFFECT_HEALTH_LEECH:
                case SPELL_EFFECT_HEAL:
                case SPELL_EFFECT_SUMMON:
                case SPELL_EFFECT_WEAPON_DAMAGE_NOSCHOOL:
                    //   SPELL_EFFECT_WEAPON_PERCENT_DAMAGE: excluded, abs values only
                case SPELL_EFFECT_WEAPON_DAMAGE:
                case SPELL_EFFECT_POWER_BURN:
                case SPELL_EFFECT_NORMALIZED_WEAPON_DMG:
                    damage = true;
            }
        }

        if (damage)
        {
            CreatureInfo const* cInfo = static_cast<Creature const*>(unitCaster)->GetCreatureInfo();
            CreatureClassLvlStats const* casterCLS = sObjectMgr.GetCreatureClassLvlStats(unitCaster->GetLevel(), cInfo->UnitClass);
            CreatureClassLvlStats const* spellCLS = sObjectMgr.GetCreatureClassLvlStats(spellProto->spellLevel, cInfo->UnitClass);
            if (casterCLS && spellCLS)
            {
                float CLSPowerCreature = casterCLS->BaseDamage;
                float CLSPowerSpell = spellCLS->BaseDamage;
                value = value * (CLSPowerCreature / CLSPowerSpell);
            }
        }
    }
    return value;
}
