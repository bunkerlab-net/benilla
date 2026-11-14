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

void ThreatManager::addThreatDirectly(Unit* victim, float threat, bool noNew)
{
    HostileReference* ref = iThreatContainer.addThreat(victim, threat);
    // Ref is not in the online refs, search the offline refs next
    if (!ref)
        ref = iThreatOfflineContainer.addThreat(victim, threat);

    if (!ref && !noNew) // there was no ref => create a new one
    {
        HostileReference* hostileReference = new HostileReference(victim, this, 0); // threat has to be 0 here
        iThreatContainer.addReference(hostileReference);
        hostileReference->addThreat(threat); // now we add the real threat
        Unit* owner = getOwner();
        owner->TriggerAggroLinkingEvent(victim);
        Unit* victim_owner = victim->GetMaster();
        if (victim_owner && victim_owner->IsAlive() && victim_owner->CanJoinInAttacking(getOwner()) && !victim_owner->hasUnitState(UNIT_STAT_FEIGN_DEATH))
            addThreat(victim_owner, 0.0f); // create a threat to the owner of a pet, if the pet attacks
        if (owner->IsOfflineTarget(victim) || victim->IsPlayer() && static_cast<Player*>(victim)->IsGameMaster())
            hostileReference->setOnlineOfflineState(false); // GM is always offline
    }
}
