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

void ThreatManager::processThreatEvent(ThreatRefStatusChangeEvent& threatRefStatusChangeEvent)
{
    threatRefStatusChangeEvent.setThreatManager(this);     // now we can set the threat manager

    HostileReference* hostileReference = threatRefStatusChangeEvent.getReference();

    switch (threatRefStatusChangeEvent.getType())
    {
        case UEV_THREAT_REF_THREAT_CHANGE:
            if ((getCurrentVictim() == hostileReference && threatRefStatusChangeEvent.getFValue() < 0.0f) ||
                    (getCurrentVictim() != hostileReference && threatRefStatusChangeEvent.getFValue() > 0.0f))
                setDirty(true);                             // the order in the threat list might have changed
            break;
        case UEV_THREAT_REF_ONLINE_STATUS:
            if (!hostileReference->isOnline())
            {
                if (hostileReference == getCurrentVictim())
                {
                    setCurrentVictim(nullptr);
                    setDirty(true);
                }
                iThreatContainer.remove(hostileReference);
                iThreatOfflineContainer.addReference(hostileReference);
            }
            else
            {
                if (getCurrentVictim() && hostileReference->getThreat() > (1.1f * getCurrentVictim()->getThreat()))
                    setDirty(true);
                iThreatContainer.addReference(hostileReference);
                iThreatOfflineContainer.remove(hostileReference);
            }
            break;
        case UEV_THREAT_REF_REMOVE_FROM_LIST:
            if (hostileReference == getCurrentVictim())
            {
                setCurrentVictim(nullptr);
                setDirty(true);
            }
            if (hostileReference->isOnline())
            {
                iThreatContainer.remove(hostileReference);
            }
            else
                iThreatOfflineContainer.remove(hostileReference);
            break;
        case UEV_THREAT_REF_SUPPRESSED_STATUS:
            // Clear suppressed on suppress change
            ClearSuppressed(hostileReference);
            setDirty(true);
            break;
    }
}

void ThreatManager::ClearSuppressed(HostileReference* except)
{
    for (HostileReference* const curRef : iThreatContainer.getThreatList())
        if (curRef->GetHostileState() == STATE_SUPPRESSED && curRef != except && !getOwner()->IsSuppressedTarget(curRef->getTarget()))
            curRef->SetHostileState(STATE_NORMAL);
}
