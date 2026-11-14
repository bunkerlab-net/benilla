void Creature::RemoveCorpse(bool inPlace)
{
    if (!inPlace && !IsInWorld())
       return;

    if ((GetDeathState() != CORPSE))
        return;

    DEBUG_FILTER_LOG(LOG_FILTER_AI_AND_MOVEGENSS, "Removing corpse of %s ", GetGuidStr().c_str());

    m_corpseExpirationTime = TimePoint();
    SetDeathState(DEAD);
    UpdateObjectVisibility();

    delete m_loot;
    m_loot = nullptr;
    m_lootStatus = CREATURE_LOOT_STATUS_NONE;
    uint32 respawnDelay = 0;

    if (AI())
        AI()->CorpseRemoved(respawnDelay);

    if (m_isCreatureLinkingTrigger)
        GetMap()->GetCreatureLinkingHolder()->DoCreatureLinkingEvent(LINKING_EVENT_DESPAWN, this);

    if (InstanceData* mapInstance = GetInstanceData())
        mapInstance->OnCreatureDespawn(this);

    // script can set time (in seconds) explicit, override the original
    if (respawnDelay)
    {
        m_respawnTime = time(nullptr) + respawnDelay; // if we set a custom respawn time, we need to save it as well

        // always save boss respawn time at death to prevent crash cheating
        if (sWorld.getConfig(CONFIG_BOOL_SAVE_RESPAWN_TIME_IMMEDIATELY) || IsWorldBoss())
            SaveRespawnTime();
    }

    InterruptMoving();

    float x, y, z, o;
    GetRespawnCoord(x, y, z, &o);
    GetMap()->CreatureRelocation(this, x, y, z, o);

    // forced recreate creature object at clients
    UnitVisibility currentVis = GetVisibility();
    SetVisibility(VISIBILITY_REMOVE_CORPSE);
    UpdateObjectVisibility();
    SetVisibility(currentVis);                              // restore visibility state
    UpdateObjectVisibility();

    if (IsUsingNewSpawningSystem())
        AddObjectToRemoveList();
}
