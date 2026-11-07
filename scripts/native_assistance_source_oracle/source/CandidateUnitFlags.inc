enum UnitFlags
{
    UNIT_FLAG_NONE                  = 0x00000000,
    UNIT_FLAG_UNK_0                 = 0x00000001,           // Movement checks disabled, likely paired with loss of client control packet. We use it to add custom cliffwalking to GM mode until actual usecases will be known.
    UNIT_FLAG_SPAWNING              = 0x00000002,           // not attackable
    UNIT_FLAG_CLIENT_CONTROL_LOST   = 0x00000004,           // Generic unspecified loss of control initiated by server script, movement checks disabled, paired with loss of client control packet.
    UNIT_FLAG_PLAYER_CONTROLLED     = 0x00000008,           // players, pets, totems, guardians, companions, charms, any units associated with players
    UNIT_FLAG_PET_RENAME            = 0x00000010,           // Old pet rename: moved to UNIT_FIELD_BYTES_2,2 in TBC+
    UNIT_FLAG_PET_ABANDON           = 0x00000020,           // Old pet abandon: moved to UNIT_FIELD_BYTES_2,2 in TBC+
    UNIT_FLAG_UNK_6                 = 0x00000040,
    UNIT_FLAG_IMMUNE_TO_PLAYER      = 0x00000100,           // Target is immune to players
    UNIT_FLAG_IMMUNE_TO_NPC         = 0x00000200,           // Target is immune to Non-Player Characters
    UNIT_FLAG_PVP                   = 0x00001000,
    UNIT_FLAG_SILENCED              = 0x00002000,           // silenced, 2.1.1
    UNIT_FLAG_PERSUADED             = 0x00004000,           // persuaded, 2.0.8
    UNIT_FLAG_SWIMMING              = 0x00008000,           // controls water swimming animation - TODO: confirm whether dynamic or static
    UNIT_FLAG_UNTARGETABLE          = 0x00010000,           // is not targetable by attack or spell
    UNIT_FLAG_PACIFIED              = 0x00020000,
    UNIT_FLAG_STUNNED               = 0x00040000,           // Unit is a subject to stun, turn and strafe movement disabled
    UNIT_FLAG_IN_COMBAT             = 0x00080000,
    UNIT_FLAG_TAXI_FLIGHT           = 0x00100000,           // Unit is on taxi, paired with a duplicate loss of client control packet (likely a legacy serverside hack). Disables any spellcasts not allowed in taxi flight client-side.
    UNIT_FLAG_CONFUSED              = 0x00400000,           // Unit is a subject to confused movement, movement checks disabled, paired with loss of client control packet.
    UNIT_FLAG_FLEEING               = 0x00800000,           // Unit is a subject to fleeing movement, movement checks disabled, paired with loss of client control packet.
    UNIT_FLAG_POSSESSED             = 0x01000000,           // Unit is under remote control by another unit, movement checks disabled, paired with loss of client control packet. New master is allowed to use melee attack and can't select this unit via mouse in the world (as if it was own character).
    UNIT_FLAG_UNINTERACTIBLE        = 0x02000000,
    UNIT_FLAG_SKINNABLE             = 0x04000000,
    UNIT_FLAG_AURAS_VISIBLE         = 0x08000000,           // magic detect
    UNIT_FLAG_UNK_28                = 0x10000000,
    UNIT_FLAG_PREVENT_ANIM          = 0x20000000,           // used in Feing Death spell
    UNIT_FLAG_SHEATHE               = 0x40000000,
    UNIT_FLAG_IMMUNE                = 0x80000000,

    // [-ZERO] TBC enumerations [?]
    UNIT_FLAG_NOT_ATTACKABLE_1      = 0x00000080,           // ?? (UNIT_FLAG_PVP_ATTACKABLE | UNIT_FLAG_NOT_ATTACKABLE_1) is NON_PVP_ATTACKABLE
    UNIT_FLAG_LOOTING               = 0x00000400,           // loot animation
    UNIT_FLAG_PET_IN_COMBAT         = 0x00000800,           // in combat?, 2.0.8
    UNIT_FLAG_DISARMED              = 0x00200000,           // disable melee spells casting..., "Required melee weapon" added to melee spells tooltip.
//[-ZERO]    UNIT_FLAG_MOUNT                 = 0x08000000,
};
