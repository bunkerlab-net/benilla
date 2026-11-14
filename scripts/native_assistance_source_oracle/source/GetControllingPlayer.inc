Player const* Unit::GetControllingPlayer(bool ignoreCharms/* = false*/) const
{
    // Mode selector: normal or permanent (UI point of view, ignore charms)
    ObjectGuid const& (Unit::* getter) () const = (ignoreCharms ? &Unit::GetOwnerGuid : &Unit::GetMasterGuid);

    // Original logic begins

    // Pre-TBC variant
    if (ObjectGuid const& masterGuid = (this->*getter)())
    {
        if (Unit const* master = ObjectAccessor::GetUnit(*this, masterGuid))
        {
            if (master->GetTypeId() == TYPEID_PLAYER)
                return static_cast<Player const*>(master);
        }
    }
    else if (GetTypeId() == TYPEID_PLAYER)
        return static_cast<Player const*>(this);
    return nullptr;
}
