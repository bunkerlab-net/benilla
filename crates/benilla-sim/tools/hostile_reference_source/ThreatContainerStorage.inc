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

void ThreatContainer::clearReferences()
{
    for (ThreatList::const_iterator i = iThreatList.begin(); i != iThreatList.end(); ++i)
    {
        (*i)->unlink();
        delete (*i);
    }
    iThreatList.clear();
}

//============================================================
// Return the HostileReference of nullptr, if not found

HostileReference* ThreatContainer::getReferenceByTarget(Unit* victim)
{
    if (!victim)
        return nullptr;

    ObjectGuid guid = victim->GetObjectGuid();

    for (ThreatList::const_iterator i = iThreatList.begin(); i != iThreatList.end(); ++i)
        if ((*i)->getUnitGuid() == guid)
            return (*i);

    return nullptr;
}

//============================================================
// Add the threat, if we find the reference

HostileReference* ThreatContainer::addThreat(Unit* victim, float threat)
{
    HostileReference* ref = getReferenceByTarget(victim);
    if (ref)
        ref->addThreat(threat);
    return ref;
}
