bool Creature::CanInitiateAttack() const
{
    if (hasUnitState(UNIT_STAT_STUNNED | UNIT_STAT_FEIGN_DEATH))
        return false;

    if (!m_forceAttackingCapability && HasFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_SPAWNING | UNIT_FLAG_UNINTERACTIBLE))
        return false;

    if (!CanAggro())
        return false;

    return true;
}
