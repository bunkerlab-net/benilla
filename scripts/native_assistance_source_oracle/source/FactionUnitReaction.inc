static ReputationRank GetFactionReaction(FactionTemplateEntry const* thisTemplate, Unit const* unit)
{
    MANGOS_ASSERT(unit)

    // Original logic begins

    if (thisTemplate)
    {
        if (const FactionTemplateEntry* unitFactionTemplate = unit->GetFactionTemplateEntry())
        {
            if (unit->HasFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_PLAYER_CONTROLLED))
            {
                if (const Player* unitPlayer = unit->GetControllingPlayer())
                {
                    if (unitPlayer->HasFlag(PLAYER_FLAGS, PLAYER_FLAGS_CONTESTED_PVP) && thisTemplate->IsContestedGuardFaction())
                        return REP_HOSTILE;

                    if (const ReputationRank* rank = unitPlayer->GetReputationMgr().GetForcedRankIfAny(thisTemplate))
                        return (*rank);

                    const FactionEntry* thisFactionEntry = sFactionStore.LookupEntry(thisTemplate->faction);
                    if (thisFactionEntry && thisFactionEntry->HasReputation())
                    {
                        const ReputationMgr& reputationMgr = unitPlayer->GetReputationMgr();
                        return reputationMgr.GetRank(thisFactionEntry);
                    }
                }

            }
            // Default fallback if player-specific checks didn't catch anything: facton to faction
            return GetFactionReaction(thisTemplate, unitFactionTemplate);
        }
    }
    return REP_NEUTRAL;
}
