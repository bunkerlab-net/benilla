void Creature::SetDeathState(DeathState s)
{
    if (s == JUST_DIED)
    {
        if (!m_respawnOverriden)
        {
            if (GetCreatureGroup() && GetCreatureGroup()->IsRespawnOverriden())
                m_respawnDelay = GetCreatureGroup()->GetRandomRespawnTime();
            else if (CreatureData const* data = sObjectMgr.GetCreatureData(GetDbGuid()))
                m_respawnDelay = data->GetRandomRespawnTime();
        }
        else if (m_respawnOverrideOnce)
            m_respawnOverriden = false;

        if (m_settings.HasFlag(CreatureStaticFlags3::FOREVER_CORPSE_DURATION))
            m_corpseExpirationTime = GetMap()->GetCurrentClockTime() + std::chrono::hours(24*7);
        else
            m_corpseExpirationTime = GetMap()->GetCurrentClockTime() + std::chrono::seconds(m_corpseDelay); // the max/default time for corpse decay (before creature is looted/AllLootRemovedFromCorpse() is called)
        m_respawnTime = time(nullptr) + m_respawnDelay; // respawn delay (spawntimesecs)

        // always save boss respawn time at death to prevent crash cheating
        if (sWorld.getConfig(CONFIG_BOOL_SAVE_RESPAWN_TIME_IMMEDIATELY) || IsWorldBoss())
            SaveRespawnTime();

        if (IsUsingNewSpawningSystem())
        {
            m_respawnTime = std::numeric_limits<time_t>::max();
            if (m_respawnDelay && s == JUST_DIED && !GetCreatureGroup())
                GetMap()->GetSpawnManager().AddCreature(GetDbGuid());
        }
    }

    Unit::SetDeathState(s);

    if (s == JUST_DIED)
    {
        SetTargetGuid(ObjectGuid());                        // remove target selection in any cases (can be set at aura remove in Unit::SetDeathState)
        SetUInt32Value(UNIT_NPC_FLAGS, UNIT_NPC_FLAG_NONE);
        Unmount();
        ClearCreatureGroup();

        if (GetUInt32Value(UNIT_CREATED_BY_SPELL)) // optimization
            if (Unit* owner = GetOwner())
                StartCooldown(owner);

        Unit::SetDeathState(CORPSE);
    }

    if (s == JUST_ALIVED)
    {
        clearUnitState(static_cast<uint32>(UNIT_STAT_ALL_STATE));

        Unit::SetDeathState(ALIVE);

        SetWalk(true, true);
        ResetEntry(true);

        m_killer = ObjectGuid();

        ResetSpellHitCounter();

        if (GetSettings().HasFlag(CreatureStaticFlags::CREATOR_LOOT))
            SetLootRecipient(GetCreator());
        else
            SetLootRecipient(nullptr);
        if (GetTemporaryFactionFlags() & TEMPFACTION_RESTORE_RESPAWN)
            ClearTemporaryFaction();

        SetMeleeDamageSchool(SpellSchools(GetCreatureInfo()->DamageSchool));

        // Dynamic flags may be adjusted by spells. Clear them
        // first and let spell from *addon apply where needed.
        SetUInt32Value(UNIT_DYNAMIC_FLAGS, UNIT_DYNFLAG_NONE);
        LoadCreatureAddon(true);

        // Flags after LoadCreatureAddon. Any spell in *addon
        // will not be able to adjust these.
        SetUInt32Value(UNIT_NPC_FLAGS, GetCreatureInfo()->NpcFlags);
        RemoveFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_SKINNABLE);

        i_motionMaster.Initialize();
    }
}
