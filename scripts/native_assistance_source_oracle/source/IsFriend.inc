bool Unit::IsFriend(Unit const* unit) const
{
    return (GetReactionTo(unit) > REP_NEUTRAL);
}
