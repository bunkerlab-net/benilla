bool Unit::CanAttackOnSight(Unit const* target) const
{
    MANGOS_ASSERT(target)

    // Do not aggro on a unit which is moving home at the moment
    if (target->GetCombatManager().IsEvadingHome())
        return false;

    // Do not aggro while a successful feign death is active
    if (!IsIgnoringFeignDeath() && target->IsFeigningDeathSuccessfully())
        return false;

    // Pets in disabled state (e.g. when player is mounted) do not draw aggro on sight
    // TODO: Fix for temporary pets and charms
    if (target->GetTypeId() == TYPEID_UNIT && static_cast<Creature const*>(target)->IsPet() && static_cast<Pet const*>(target)->HasActionsDisabled())
        return false;

    return (CanAttack(target) && IsEnemy(target));
}
