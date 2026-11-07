bool Unit::CanJoinInAttacking(Unit const* enemy) const
{
    if (!CanEnterCombat())
        return false;

    if (!CanInitiateAttack())
        return false;

    if (IsFeigningDeathSuccessfully())
        return false;

    if (HasAuraType(SPELL_AURA_MOD_UNATTACKABLE))
        return false;

    if (!CanAttack(enemy))
        return false;

    return true;
}
