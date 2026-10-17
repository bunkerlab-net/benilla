bool Unit::CanAssistInCombatAgainst(Unit const* who, Unit const* enemy) const
{
    MANGOS_ASSERT(who)
    MANGOS_ASSERT(enemy)

    if (GetMap()->Instanceable()) // in dungeons nothing else needs to be evaluated
        return CanJoinInAttacking(enemy);

    if (IsInCombat()) // if fighting something else, do not assist
        return false;

    if (CanAssist(who) && CanAttackOnSight(enemy))
        return true;

    return false;
}
