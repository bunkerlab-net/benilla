std::pair<bool, GuidVector> Creature::MarkCallAssistanceOnPull(Unit* enemy)
{
    bool stored = m_AlreadyCallAssistance;
    SetNoCallAssistance(true);

    if (!CanCallForAssistance())
        return {false, GuidVector()};

    float radius = sWorld.getConfig(CONFIG_FLOAT_CREATURE_FAMILY_ASSISTANCE_RADIUS);
    if (GetCreatureInfo()->CallForHelp > 0)
        radius = GetCreatureInfo()->CallForHelp;

    CreatureList receiverList;
    MaNGOS::AnyAssistCreatureInRangeCheck u_check(this, enemy, radius);
    MaNGOS::CreatureListSearcher<MaNGOS::AnyAssistCreatureInRangeCheck> searcher(receiverList, u_check);
    Cell::VisitAllObjects(this, searcher, radius);
    GuidVector guids;
    for (Creature* creature : receiverList)
        guids.push_back(creature->GetObjectGuid());
    return {stored, guids};
}
