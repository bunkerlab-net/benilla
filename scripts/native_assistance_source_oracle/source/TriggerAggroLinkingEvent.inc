void Unit::TriggerAggroLinkingEvent(Unit* enemy)
{
    if (!IsCreature() || !enemy)
        return;

    bool callAssistance; GuidVector receiverList;
    std::tie(callAssistance, receiverList) = static_cast<Creature*>(this)->MarkCallAssistanceOnPull(enemy);

    m_events.AddEvent(new UnitLambdaEvent(*this, [enemyGuid = enemy->GetObjectGuid(), creatureGroup = static_cast<Creature*>(this)->GetCreatureGroup(), callAssistance, receiverList](Unit& unit)
    {
        Unit* enemy = unit.GetMap()->GetUnit(enemyGuid);
        if (!enemy || !unit.IsInCombat())
            return;

        if (unit.IsLinkingEventTrigger())
            unit.GetMap()->GetCreatureLinkingHolder()->DoCreatureLinkingEvent(LINKING_EVENT_AGGRO, static_cast<Creature*>(&unit), enemy);

        if (creatureGroup) // if npc dies before event execution, group will be removed from him, however groups are persistent and safe to access like this
            creatureGroup->TriggerLinkingEvent(CREATURE_GROUP_EVENT_AGGRO, enemy);

        if (callAssistance)
            static_cast<Creature&>(unit).CallAssistanceOnPull(enemy, receiverList);
    }), m_events.CalculateTime(sWorld.getConfig(CONFIG_UINT32_CREATURE_CHECK_FOR_HELP_AGGRO_DELAY)));
}
