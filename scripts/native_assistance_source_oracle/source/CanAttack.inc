bool Unit::CanAttack(const Unit* unit) const
{
    MANGOS_ASSERT(unit)

    // Original logic

    // Creatures cannot attack player ghosts, unless it is a specially flagged ghost creature
    if (GetTypeId() == TYPEID_UNIT && unit->GetTypeId() == TYPEID_PLAYER && static_cast<const Player*>(unit)->HasFlag(PLAYER_FLAGS, PLAYER_FLAGS_GHOST))
    {
        if (!(static_cast<const Creature*>(this)->GetCreatureInfo()->HasFlag(CreatureTypeFlags::VISIBLE_TO_GHOSTS)))
            return false;
    }

    // We can't attack unit when at least one of these flags is present on it:
    const uint32 mask = (UNIT_FLAG_SPAWNING | UNIT_FLAG_NOT_ATTACKABLE_1 | UNIT_FLAG_UNTARGETABLE | UNIT_FLAG_TAXI_FLIGHT | UNIT_FLAG_UNINTERACTIBLE);
    if (unit->HasFlag(UNIT_FIELD_FLAGS, mask))
        return false;

    // Cross-check immunity and sanctuary flags: this <-> unit
    const bool thisPlayerControlled = HasFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_PLAYER_CONTROLLED);
    if (thisPlayerControlled)
    {
        if (unit->HasFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_IMMUNE_TO_PLAYER))
            return false;
    }
    else if (unit->HasFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_IMMUNE_TO_NPC))
        return false;

    const bool unitPlayerControlled = unit->HasFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_PLAYER_CONTROLLED);
    if (unitPlayerControlled)
    {
        if (HasFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_IMMUNE_TO_PLAYER))
            return false;
    }
    else if (HasFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_IMMUNE_TO_NPC))
        return false;

    if (thisPlayerControlled || unitPlayerControlled)
    {
        if (thisPlayerControlled && unitPlayerControlled)
        {
            if (IsFriend(unit))
                return false;

            const Player* thisPlayer = GetControllingPlayer();
            if (!thisPlayer)
                return true;

            const Player* unitPlayer = unit->GetControllingPlayer();
            if (!unitPlayer)
                return true;

            if (thisPlayer->GetUInt32Value(PLAYER_DUEL_TEAM) && unitPlayer->GetUInt32Value(PLAYER_DUEL_TEAM) && thisPlayer->GetGuidValue(PLAYER_DUEL_ARBITER) == unitPlayer->GetGuidValue(PLAYER_DUEL_ARBITER))
                return true;

            if (unitPlayer->IsPvP())
                return true;

            if (thisPlayer->IsPvPFreeForAll() && unitPlayer->IsPvPFreeForAll())
                return true;

            return false;
        }
        return (!IsFriend(unit));
    }
    return (IsEnemy(unit) || unit->IsEnemy(this));
}
