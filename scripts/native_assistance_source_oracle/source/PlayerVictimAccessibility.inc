bool Unit::isInAccessablePlaceFor(Unit const* unit) const
{
    if (IsInWater())
        return unit->CanSwim();
    return unit->CanWalk() || unit->CanFly();
}
