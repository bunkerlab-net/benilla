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

void ThreatContainer::update(bool force, bool isPlayer)
{
    if ((iDirty || force || isPlayer) && iThreatList.size() > 1)
    {
        iThreatList.sort([&](const HostileReference* lhs, const HostileReference* rhs)->bool
        {
            Unit* owner = lhs->getSource()->getOwner();
            if (isPlayer)
            {
                Unit* left = lhs->getTarget();
                Unit* right = rhs->getTarget();
                if (left->IsPlayer() && !right->IsPlayer())
                    return true;
                if (!left->IsPlayer() && right->IsPlayer())
                    return false;
                bool attackLeft = owner->CanAttack(left);
                bool attackRight = owner->CanAttack(right);
                if (attackLeft && !attackRight)
                    return true;
                if (!attackLeft && attackRight)
                    return false;
            }
            if (lhs->GetTauntState() != rhs->GetTauntState())
                return lhs->GetTauntState() > rhs->GetTauntState();
            if (force)
            {
                bool first = owner->CanReachWithMeleeAttack(lhs->getTarget());
                bool second = owner->CanReachWithMeleeAttack(rhs->getTarget());
                if (first != second)
                    return first;
            }
            if (lhs->GetHostileState() != rhs->GetHostileState())
                return lhs->GetHostileState() > rhs->GetHostileState();
            return lhs->getThreat() > rhs->getThreat(); // reverse sorting
        });
    }
    iDirty = false;
}
