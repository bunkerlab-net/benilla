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

void HostileRefManager::setOnlineOfflineState(bool isOnline)
{
    HostileReference* ref = getFirst();
    while (ref != nullptr)
    {
        ref->setOnlineOfflineState(isOnline);
        ref = ref->next();
    }
}

void HostileRefManager::updateOnlineOfflineState(bool pIsOnline)
{
    if (pIsOnline && iOwner)
    {
        // Do not set online while feigning death in combat
        if (iOwner->IsFeigningDeathSuccessfully() && iOwner->IsInCombat())
            return;
    }
    setOnlineOfflineState(pIsOnline);
}

//=================================================
// The online / offline status is calculated and set

void HostileRefManager::updateThreatTables()
{
    HostileReference* ref = getFirst();
    while (ref)
    {
        ref->updateOnlineStatus();
        ref = ref->next();
    }
}

//=================================================
// The references are not needed anymore
// tell the source to remove them from the list and free the mem

void HostileRefManager::deleteReferences()
{
    HostileReference* ref = getFirst();
    while (ref)
    {
        HostileReference* nextRef = ref->next();
        ref->removeReference();
        delete ref;
        ref = nextRef;
    }
}
