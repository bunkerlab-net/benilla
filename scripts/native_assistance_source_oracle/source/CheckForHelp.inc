void UnitAI::CheckForHelp(Unit* who, Unit* me, float distance)
{
    Unit* victim = who->getAttackerForHelper();

    if (!victim)
        return;

    if (me->IsInCombat() || !me->CanCallForAssistance() || !who->CanCallForAssistance() || !me->CanCheckForHelp() || !who->CanCheckForHelp())
        return;

    // pulling happens once panic/retreating ends
    // current theory is that help aggro is never done if owner has suspended AI function during CC
    if (who->hasUnitState(UNIT_STAT_RETREATING) || who->IsConfused() || who->IsStunned())
        return;

    if (me->CanInitiateAttack() && me->CanAttackOnSight(victim) && victim->isInAccessablePlaceFor(me) && victim->IsVisibleForOrDetect(me, me, false))
    {
        if (me->IsWithinDistInMap(who, distance) && me->IsWithinLOSInMap(who, true))
        {
            if (me->CanAssistInCombatAgainst(who, victim))
            {
                AttackStart(victim);
                if (who->AI() && who->AI()->GetAIOrder() == ORDER_FLEEING)
                    who->GetMotionMaster()->InterruptPanic();
            }
        }
    }
}
