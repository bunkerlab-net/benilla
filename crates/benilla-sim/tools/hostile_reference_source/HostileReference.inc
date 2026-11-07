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

HostileReference::HostileReference(Unit* unit, ThreatManager* threatManager, float threat) : 
    m_hostileState(STATE_NORMAL), m_tauntState(STATE_NONE)
{
    iThreat = threat;
    iFadeoutThreadReduction = 0.f;
    link(unit, threatManager);
    iUnitGuid = unit->GetObjectGuid();
    m_online = true;
    m_suppresabilityToggle = false;
    iAccessible = true;
}

//============================================================
// Tell our refTo (target) object that we have a link
void HostileReference::targetObjectBuildLink()
{
    getTarget()->addHatedBy(this);
}

//============================================================
// Tell our refTo (taget) object, that the link is cut
void HostileReference::targetObjectDestroyLink()
{
    getTarget()->removeHatedBy(this);
}

//============================================================
// Tell our refFrom (source) object, that the link is cut (Target destroyed)

void HostileReference::sourceObjectDestroyLink()
{
    setOnlineOfflineState(false);
}

//============================================================
// Inform the source, that the status of the reference changed

void HostileReference::fireStatusChanged(ThreatRefStatusChangeEvent& threatRefStatusChangeEvent)
{
    if (getSource())
        getSource()->processThreatEvent(threatRefStatusChangeEvent);
}

//============================================================

void HostileReference::addThreat(float mod)
{
    if (mod + iThreat < 0)
        mod = -iThreat;

    iThreat += mod;
    // If threat has changed, clear suppress
    if (m_suppresabilityToggle && mod != 0.f)
        SetHostileState(STATE_NORMAL);

    if (mod != 0.0f)
    {
        ThreatRefStatusChangeEvent event(UEV_THREAT_REF_THREAT_CHANGE, this, mod);
        fireStatusChanged(event);
    }
}

//============================================================
// check, if source can reach target and set the status

void HostileReference::updateOnlineStatus()
{
    bool online = false;
    if (!isValid())
    {
        if (Unit* target = ObjectAccessor::GetUnit(*getSourceUnit(), getUnitGuid()))
            link(target, getSource());
    }
    // only check for online status if
    // ref is valid
    if (isValid())
    {
        Unit* unit = getSourceUnit();
        Unit* target = getTarget();
        if (!unit->IsOfflineTarget(target))
            online = true;
    }
    setOnlineOfflineState(online);
}

//============================================================
// set the status and fire the event on status change

void HostileReference::setOnlineOfflineState(bool isOnline)
{
    if (m_online != isOnline)
    {
        m_online = isOnline;

        ThreatRefStatusChangeEvent event(UEV_THREAT_REF_ONLINE_STATUS, this);
        fireStatusChanged(event);
    }
}

//============================================================

void HostileReference::setAccessibleState(bool isAccessible)
{
    if (iAccessible != isAccessible)
    {
        iAccessible = isAccessible;

        ThreatRefStatusChangeEvent event(UEV_THREAT_REF_ASSECCIBLE_STATUS, this);
        fireStatusChanged(event);
    }
}

void HostileReference::SetHostileState(HostileState state)
{
    if (m_hostileState != state)
    {
        m_hostileState = state;

        ThreatRefStatusChangeEvent event(UEV_THREAT_REF_SUPPRESSED_STATUS, this);
        fireStatusChanged(event);
    }
    m_suppresabilityToggle = false;
}

//============================================================
// prepare the reference for deleting
// this is called be the target

void HostileReference::removeReference()
{
    invalidate();

    ThreatRefStatusChangeEvent event(UEV_THREAT_REF_REMOVE_FROM_LIST, this);
    fireStatusChanged(event);
}

//============================================================

Unit* HostileReference::getSourceUnit() const
{
    return getSource()->getOwner();
}

//============================================================
//================ ThreatContainer ===========================
