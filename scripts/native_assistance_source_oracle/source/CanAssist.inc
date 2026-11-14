bool Unit::CanAssist(const Unit* unit, bool /*ignoreFlags*/) const
{
    MANGOS_ASSERT(unit)

    // Original logic

    // We can't assist unselectable unit
    if (unit->HasFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_UNINTERACTIBLE))
        return false;

    // Exclude non-friendlies at this point
    if (GetReactionTo(unit) < REP_FRIENDLY)
        return false;

    // Pre-WotLK: backbone of lua UnitIsPVP(), a member of unit class client-side
    auto isPvPUI = [](Unit const * self)
    {
        if (Unit const* master = self->GetMaster())
        {
            if (self->HasFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_IMMUNE_TO_PLAYER))
                return false;
            return master->IsPvP();
        }
        return self->IsPvP();
    };

    // Detect player controlled unit and exit early
    if (unit->HasFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_PLAYER_CONTROLLED))
    {
        const Player* thisPlayer = GetControllingPlayer();
        const Player* unitPlayer = unit->GetControllingPlayer();

        if (thisPlayer && unitPlayer)
        {
            if (thisPlayer->GetGuidValue(PLAYER_DUEL_ARBITER) != unitPlayer->GetGuidValue(PLAYER_DUEL_ARBITER)
                || thisPlayer->GetUInt32Value(PLAYER_DUEL_TEAM) != unitPlayer->GetUInt32Value(PLAYER_DUEL_TEAM))
                return false;

            if (unitPlayer->IsPvPFreeForAll() && !thisPlayer->IsPvPFreeForAll())
                return false;
        }
        return true;
    }

    // If we continue here, unit is an npc. Detect if we are an npc too, so we can exit early
    if (!HasFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_PLAYER_CONTROLLED))
        return true;

    // Pre-TBC: We are left with player assisting an npc case here: can assist friendly NPCs with PVP flag
    if (isPvPUI(unit))
        return true;

    return false;
}
