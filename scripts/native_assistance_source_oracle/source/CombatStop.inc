void Unit::CombatStop(bool includingCast, bool includingCombo)
{
    AttackStop(true, includingCast, includingCombo);

    RemoveAllAttackers();
    DeleteThreatList();

    if (GetTypeId() != TYPEID_PLAYER)
    {
        ((Creature*)this)->SetNoCallAssistance(false);

        if (((Creature*)this)->GetTemporaryFactionFlags() & TEMPFACTION_RESTORE_COMBAT_STOP)
            ((Creature*)this)->ClearTemporaryFaction();
    }

    if (AI())
        AI()->CombatStop();

    GetCombatManager().StopCombatTimer();

    ClearInCombat();
}
