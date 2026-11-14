void Unit::SetInCombatState(bool PvP, Unit* enemy)
{
    // only alive units can be in combat
    if (!IsAlive())
        return;

    bool notInCombat = !HasFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_IN_COMBAT);
    bool creatureNotInCombat = GetTypeId() == TYPEID_UNIT && notInCombat;

    // For player itself and his pet during pvp combat enable own combat timer
    if (PvP || creatureNotInCombat)
        GetCombatManager().TriggerCombatTimer(PvP);

    SetFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_IN_COMBAT);

    if (HasCharmer() || !GetOwnerGuid().IsEmpty())
    {
        SetFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_PET_IN_COMBAT);
        if (enemy && (notInCombat || PvP))
        {
            // TODO: Unify this combat propagation with linking combat propagation in threat system
            Unit* controller = GetMaster();

            if (controller && controller->CanJoinInAttacking(enemy))
            {
                if (HasFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_PLAYER_CONTROLLED))
                {
                    controller->SetInCombatWith(enemy); // player only enters combat
                    enemy->AddThreat(controller);
                }
                else if (controller->IsInCombat())
                {
                    controller->AddThreat(enemy);
                    enemy->AddThreat(controller);
                    enemy->SetInCombatWith(controller);
                    if (PvP)
                        enemy->GetCombatManager().TriggerCombatTimer(controller);
                }
                else
                {
                    MANGOS_ASSERT(controller->AI()); // a player without UNIT_FLAG_PLAYER_CONTROLLED should always have AI
                    controller->AI()->AttackStart(enemy);
                }
            }
        }
    }

    // interrupt all delayed non-combat casts
    for (uint32 i = CURRENT_FIRST_NON_MELEE_SPELL; i < CURRENT_MAX_SPELL; ++i)
        if (Spell* spell = GetCurrentSpell(CurrentSpellTypes(i)))
            if (IsNonCombatSpell(spell->m_spellInfo))
                InterruptSpell(CurrentSpellTypes(i), false);

    if (notInCombat)
        GetCombatManager().OnCombatStart();

    if (creatureNotInCombat)
    {
        Creature* creature = static_cast<Creature*>(this);

        m_damageByOthers = 0;

        // clear stand state if set in addon - else script has to do it on its own
        CreatureDataAddon const* cainfo = creature->GetCreatureAddon();
        if (cainfo)
        {
            if (getStandState() == cainfo->standState)
                SetStandState(UNIT_STAND_STATE_STAND);
            if (cainfo->emote && GetUInt32Value(UNIT_NPC_EMOTESTATE) == cainfo->emote)
                HandleEmoteState(0);
        }

        if (creature->GetCombatStartPosition().IsEmpty())
            creature->SetCombatStartPosition(GetPosition(GetTransport()));

        if (!creature->CanAggro()) // if creature aggroed during initial ignoration period, clear the state
        {
            creature->SetCanAggro(true);
            AbortAINotifyEvent();
        }

        if (creature->AI())
            creature->AI()->EnterCombat(enemy);

        // can be overriden by spellcast on Aggro hook, hence must be done after EnterCombat hook
        if (!creature->GetCreatedBySpellId() && creature->GetSettings().HasFlag(CreatureStaticFlags::NO_MELEE_FLEE) && !creature->IsRooted() && !creature->IsInPanic() && !creature->IsNonMeleeSpellCasted(false) && enemy && enemy->IsPlayerControlled())
        {
            creature->AI()->DoFlee(30000);
            creature->AI()->SetAIOrder(ORDER_CRITTER_FLEE); // mark as critter flee for custom handling
        }

        // Some bosses are set into combat with zone
        if (GetMap()->IsDungeon() && (creature->GetCreatureInfo()->ExtraFlags & CREATURE_EXTRA_FLAG_AGGRO_ZONE) && enemy && enemy->IsControlledByPlayer())
            creature->SetInCombatWithZone();

        if (InstanceData* mapInstance = GetInstanceData())
            mapInstance->OnCreatureEnterCombat(creature);

        creature->SetCanCheckForHelp(false);
        creature->m_events.AddEvent(new UnitLambdaEvent(*creature, [](Unit& unit)
        {
            static_cast<Creature&>(unit).SetCanCheckForHelp(true);
        }), creature->m_events.CalculateTime(sWorld.getConfig(CONFIG_UINT32_CREATURE_CHECK_FOR_HELP_AGGRO_DELAY)));

        TriggerAggroLinkingEvent(enemy);
    }
}
