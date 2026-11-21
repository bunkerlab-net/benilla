bool Unit::IsEnemy(Unit const* unit) const
{
    return (GetReactionTo(unit) < REP_UNFRIENDLY);
}
