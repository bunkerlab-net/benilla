ReputationRank Unit::GetReactionTo(Unit const* unit) const
{
    MANGOS_ASSERT(unit)

    // Original logic begins

    if (this == unit)
        return REP_FRIENDLY;

    if (HasFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_PLAYER_CONTROLLED))
    {
        const Player* thisPlayer = GetControllingPlayer();

        if (unit->HasFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_PLAYER_CONTROLLED))
        {
            const Player* unitPlayer = unit->GetControllingPlayer();

            if (!thisPlayer || !unitPlayer)
                return REP_NEUTRAL;

            // Pre-TBC same player check: not present clientside in this order, but in for optimization (same result achieved through same group check below)
            if (thisPlayer == unitPlayer)
                return REP_FRIENDLY;

            if (thisPlayer->GetUInt32Value(PLAYER_DUEL_TEAM) && unitPlayer->GetUInt32Value(PLAYER_DUEL_TEAM) && thisPlayer->GetGuidValue(PLAYER_DUEL_ARBITER) == unitPlayer->GetGuidValue(PLAYER_DUEL_ARBITER))
                return thisPlayer->GetUInt32Value(PLAYER_DUEL_TEAM) != unitPlayer->GetUInt32Value(PLAYER_DUEL_TEAM) ? REP_HOSTILE : REP_FRIENDLY;

            // Pre-WotLK group check: always, replaced with faction template check in WotLK
            if (thisPlayer->IsInGroup(unitPlayer))
                return REP_FRIENDLY;

            // Pre-WotLK FFA check, known limitation: FFA doesn't work with totem elementals both client-side and server-side
            if (thisPlayer->IsPvPFreeForAll() && unitPlayer->IsPvPFreeForAll())
                return REP_HOSTILE;
        }

        if (thisPlayer)
        {
            if (const FactionTemplateEntry* unitFactionTemplate = unit->GetFactionTemplateEntry())
            {
                if (const ReputationRank* rank = thisPlayer->GetReputationMgr().GetForcedRankIfAny(unitFactionTemplate))
                    return (*rank);

                const FactionEntry* unitFactionEntry = sFactionStore.LookupEntry(unitFactionTemplate->faction);

                // If the faction has reputation ranks available, "at war" and contested PVP flags decide outcome
                if (unitFactionEntry && unitFactionEntry->HasReputation())
                {
                    // Pre-TBC contested check: not present clientside in this order, but in for optimization (same result achieved through faction to unit check below)
                    if (thisPlayer->HasFlag(PLAYER_FLAGS, PLAYER_FLAGS_CONTESTED_PVP) && unitFactionTemplate->IsContestedGuardFaction())
                        return REP_HOSTILE;

                    return thisPlayer->GetReputationMgr().IsAtWar(unitFactionEntry) ? REP_HOSTILE : REP_FRIENDLY;
                }
            }
        }
    }
    // Default fallback if player-specific checks didn't catch anything: facton to unit
    ReputationRank reaction = GetFactionReaction(GetFactionTemplateEntry(), unit);

    // Persuation support
    if (reaction > REP_HOSTILE && reaction < REP_HONORED && (unit->HasFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_PERSUADED) || GetPersuadedGuid() == unit->GetObjectGuid()))
    {
        if (const FactionTemplateEntry* unitFactionTemplate = unit->GetFactionTemplateEntry())
        {
            const FactionEntry* unitFactionEntry = sFactionStore.LookupEntry(unitFactionTemplate->faction);
            if (unitFactionEntry && unitFactionEntry->HasReputation())
                reaction = ReputationRank(int32(reaction) + 1);
        }
    }
    return reaction;
}
