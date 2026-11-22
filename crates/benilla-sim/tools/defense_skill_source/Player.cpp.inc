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

void Player::UpdateCombatSkills(uint32 procEx, WeaponAttackType attType, bool defence)
{
    const uint16 skillId = (defence ? SKILL_DEFENSE : GetWeaponSkillIdForAttack(attType));
    const uint16 skill = GetSkillValuePure(skillId);
    const uint16 skillMax = GetSkillMaxPure(skillId);
    const int32 room = int32(skillMax - skill);
    const int32 level = GetLevel();

    // Skill already capped or invalid state: nothing to gain
    if (skillMax == 0 || level == 0 || skill >= skillMax)
        return;

    // skillGapLogGrowth controls the slope of the logarithmic catch-up portion
    const double skillGapLogGrowth = 0.22 * (1.0 + (12.0 / level));
    // levelCapScale normalizes the curve across realm caps and anchors the seam at room = 7
    // TODO: Figure out if scaling of 295 to 300 is same in vanilla, tbc and wotlk when level 60
    const double levelCapScale = 1;

    double baseChance = 0.0;

    // Piecewise curve: logarithmic growth for catch-up, quadratic decay near the cap
    if (room >= 7)
        baseChance = (7.0 / skillMax) * levelCapScale
                   + skillGapLogGrowth * std::log((room + 5.0) / 12.0);
    else
        baseChance = (room * room) / (7.0 * skillMax) * levelCapScale;

    baseChance = std::clamp(baseChance, 0.0, 1.0);

    // Weapon skills gain a small bonus from intellect
    double intellectBonus = 0.0;
    if (!defence)
    {
        const double maxIntellectBonus = 0.10;
        const double intellect = GetStat(STAT_INTELLECT);
        // TODO: Figure out actual intellect scaling contribution - likely a per level value like everything else and not linearly
        const int levelBracket =    (level <= 60) ? 60 :
                                    (level <= 70) ? 70 :
                                    (level <= 80) ? 80 : 255;

        const double intellectMax = (level <= 60) ? 750 :
                                    (level <= 70) ? 1500 :
                                    (level <= 80) ? 3000 : 6000; // default fallback

        intellectBonus = maxIntellectBonus *
                         (intellect / intellectMax) *
                         (static_cast<double>(levelBracket) / static_cast<double>(level));

        intellectBonus = std::clamp(intellectBonus, 0.0, maxIntellectBonus);
    }

    // Final skill-up probability
    const float finalChance = std::clamp(baseChance + intellectBonus, 0.0, 1.0) * 100.0f;

    if (roll_chance_f(finalChance))
    {
        if (defence)
            UpdateDefense(procEx);
        else
            UpdateWeaponSkill(attType);
    }
}
void Player::UpdateDefense(uint32 procEx)
{
    uint32 defense_skill_gain = sWorld.getConfig(CONFIG_UINT32_SKILL_GAIN_DEFENSE);

    if (defense_skill_gain == 0)
        return;

    UpdateSkill(SKILL_DEFENSE, defense_skill_gain);
    UpdateDefenseBonusesMod();
}
void Player::UpdateSkill(uint16 id, uint16 diff)
{
    if (!id)
        return;

    if (diff == 0)
        return;

    SkillStatusMap::iterator itr = mSkillStatus.find(id);
    if (itr == mSkillStatus.end())
        return;

    SkillStatusData& skillStatus = itr->second;
    if (skillStatus.uState == SKILL_DELETED)
        return;

    uint32 valueIndex = PLAYER_SKILL_VALUE_INDEX(skillStatus.pos);
    uint32 data = GetUInt32Value(valueIndex);
    uint32 value = SKILL_VALUE(data);
    uint32 max = SKILL_MAX(data);

    if ((!max) || (!value) || (value >= max))
        return;

    uint32 new_value = value + diff;
    if (new_value > max)
        new_value = max;

    SetUInt32Value(valueIndex, MAKE_SKILL_VALUE(new_value, max));

    if (skillStatus.uState != SKILL_NEW)
        skillStatus.uState = SKILL_CHANGED;

    return;
}
