void Unit::SetDeathState(DeathState s)
{
    if (s == JUST_DIED)
    {
        RemoveAllAurasOnDeath();
        BreakCharmOutgoing();
        BreakCharmIncoming();
        RemoveMiniPet();
        UnsummonAllTotems();

        StopMoving(true);
        i_motionMaster.Clear(false, true);
        if (!CanFly() || !i_motionMaster.MoveFall())
            i_motionMaster.MoveIdle();

        GetCombatManager().StopEvade();

        ModifyAuraState(AURA_STATE_HEALTHLESS_20_PERCENT, false);
        // remove aurastates allowing special moves
        ClearAllReactives();
        ClearDiminishings();
    }
    else if (s == JUST_ALIVED)
    {
        RemoveFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_SKINNABLE);  // clear skinnable for creature and player (at battleground)
        ScheduleAINotify(GetTypeId() == TYPEID_UNIT ? sWorld.getConfig(CONFIG_UINT32_CREATURE_RESPAWN_AGGRO_DELAY) : 0, true);
    }

    if (s != ALIVE && s != JUST_ALIVED)
    {
        CombatStop();
        ClearComboPointHolders();                           // any combo points pointed to unit lost at it death

        InterruptNonMeleeSpells(false);
    }

    // TODO:: what is/was that?
    /*if (m_deathState != ALIVE && s == ALIVE)
    {
        //_ApplyAllAuraMods();
    }*/
    m_deathState = s;
}
