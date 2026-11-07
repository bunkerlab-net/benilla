        creature->SetCanCheckForHelp(false);
        creature->m_events.AddEvent(new UnitLambdaEvent(*creature, [](Unit& unit)
        {
            static_cast<Creature&>(unit).SetCanCheckForHelp(true);
        }), creature->m_events.CalculateTime(sWorld.getConfig(CONFIG_UINT32_CREATURE_CHECK_FOR_HELP_AGGRO_DELAY)));

        TriggerAggroLinkingEvent(enemy);
