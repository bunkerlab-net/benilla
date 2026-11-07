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

HostileReference* ThreatContainer::selectNextVictim(Unit* attacker, HostileReference* currentVictim)
{
    HostileReference* currentRef = nullptr;
    bool found = false;
    bool suppressRanged = attacker->IsIgnoringRangedTargets();
    bool currentVictimInMelee = true;
    if (suppressRanged && currentVictim)
        currentVictimInMelee = attacker->CanReachWithMeleeAttack(currentVictim->getTarget());

    ThreatList::const_iterator lastRef = iThreatList.end();
    --lastRef;

    for (ThreatList::const_iterator iter = iThreatList.begin(); iter != iThreatList.end() && !found;)
    {
        currentRef = (*iter);

        Unit* target = currentRef->getTarget();
        MANGOS_ASSERT(target); // if the ref has status online the target must be there!

        bool isInMelee = attacker->CanReachWithMeleeAttack(target);
        if (currentVictim) // select 1.3/1.1 better target in comparison current target
        {
            // normal case: pCurrentRef is still valid and most hated
            if (currentVictim == currentRef)
            {
                if (suppressRanged && !currentVictimInMelee)
                {
                    ++iter;
                    continue;
                }
                found = true;
                break;
            }

            if (currentRef->GetTauntState() > currentVictim->GetTauntState()) // taunt overrides root skipping
            {
                found = true;
                break;
            }

            if (currentRef->getTarget()->IsPlayer() && !currentVictim->getTarget()->IsPlayer())
            {
                found = true;
                break;
            }

            if (suppressRanged) // suppress ranged when rooted
            {
                if (!isInMelee) // if current ref is not in melee - skip it
                {
                    ++iter;
                    continue;
                }
                else if (!currentVictimInMelee)
                {
                    found = true;
                    break;
                }
            }

            if (currentRef->GetHostileState() > currentVictim->GetHostileState())
            {
                found = true;
                break;
            }

            // list sorted and and we check current target, then this is best case
            if (currentRef->getThreat() <= 1.1f * currentVictim->getThreat())
            {
                currentRef = currentVictim;
                found = true;
                break;
            }

            if (currentRef->getThreat() > 1.3f * currentVictim->getThreat() ||
                (currentRef->getThreat() > 1.1f * currentVictim->getThreat() && isInMelee))
            {
                // implement 110% threat rule for targets in melee range
                found = true;                           // and 130% rule for targets in ranged distances
                break;                                  // for selecting alive targets
            }
        }
        else if (!suppressRanged || isInMelee) // select any
        {
            found = true;
            break;
        }
        ++iter;
    }
    if (!found)
        currentRef = nullptr;

    return currentRef;
}
