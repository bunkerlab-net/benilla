void Creature::Update(const uint32 diff)
{
    switch (m_deathState)
    {
        case JUST_ALIVED:
            // Don't must be called, see Creature::SetDeathState JUST_ALIVED -> ALIVE promoting.
            sLog.outError("Creature (GUIDLow: %u Entry: %u ) in wrong state: JUST_ALIVED (4)", GetGUIDLow(), GetEntry());
            break;
        case JUST_DIED:
            // Don't must be called, see Creature::SetDeathState JUST_DIED -> CORPSE promoting.
            sLog.outError("Creature (GUIDLow: %u Entry: %u ) in wrong state: JUST_DEAD (1)", GetGUIDLow(), GetEntry());
            break;
        case DEAD:
        {
            if (m_respawnTime <= time(nullptr) && (!m_isSpawningLinked || GetMap()->GetCreatureLinkingHolder()->CanSpawn(this)))
            {
                DEBUG_FILTER_LOG(LOG_FILTER_AI_AND_MOVEGENSS, "Respawning...");
                m_respawnTime = 0;
                SetCanAggro(false);
                delete m_loot;
                m_loot = nullptr;

                // Clear possible auras having IsDeathPersistent() attribute
                RemoveAllAuras();

                SetUInt32Value(UNIT_DYNAMIC_FLAGS, UNIT_DYNFLAG_NONE);
                SetDeathState(JUST_ALIVED);

                // Call AI respawn virtual function
                if (AI())
                    AI()->JustRespawned();

                if (InstanceData* mapInstance = GetInstanceData())
                    mapInstance->OnCreatureRespawn(this);

                if (m_isCreatureLinkingTrigger)
                    GetMap()->GetCreatureLinkingHolder()->DoCreatureLinkingEvent(LINKING_EVENT_RESPAWN, this);

                if (GetCreatureGroup())
                    GetCreatureGroup()->TriggerLinkingEvent(CREATURE_GROUP_EVENT_RESPAWN, this);

                GetMap()->Add(this);

                if (GetObjectGuid().GetHigh() != HIGHGUID_PET)
                    if (uint16 poolid = sPoolMgr.IsPartOfAPool<Creature>(GetDbGuid()))
                        sPoolMgr.UpdatePool<Creature>(*GetMap()->GetPersistentState(), poolid, GetDbGuid());
            }
            break;
        }
        case CORPSE:
        {
            Unit::Update(diff);

            if (m_loot)
                m_loot->Update();

            if (IsCorpseExpired())
                RemoveCorpse();

            break;
        }
        case ALIVE:
        {
            Unit::Update(diff);

            // creature can be dead after Unit::Update call
            // CORPSE/DEAD state will processed at next tick (in other case death timer will be updated unexpectedly)
            if (!IsAlive())
                break;

            // Creature can be dead after unit update
            if (IsAlive())
                RegenerateAll(diff);

            break;
        }
        default:
            break;
    }
}
