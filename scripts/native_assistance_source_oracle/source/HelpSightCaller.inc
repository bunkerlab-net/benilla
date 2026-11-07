    if (who->GetObjectGuid().IsCreature() && who->IsInCombat())
        CheckForHelp(who, m_unit, sWorld.getConfig(CONFIG_FLOAT_CREATURE_CHECK_FOR_HELP_RADIUS));
