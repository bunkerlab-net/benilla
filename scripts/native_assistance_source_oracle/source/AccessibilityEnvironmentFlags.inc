enum EnvironmentFlags
{
    ENVIRONMENT_FLAG_NONE           = 0x00,
    ENVIRONMENT_FLAG_IN_WATER       = 0x01,                     // Swimming or standing in water
    ENVIRONMENT_FLAG_IN_MAGMA       = 0x02,                     // Swimming or standing in magma
    ENVIRONMENT_FLAG_IN_SLIME       = 0x04,                     // Swimming or standing in slime
    ENVIRONMENT_FLAG_HIGH_SEA       = 0x08,                     // Anywhere inside deep water area
    ENVIRONMENT_FLAG_UNDERWATER     = 0x10,                     // Swimming fully submerged in any liquid
    ENVIRONMENT_FLAG_HIGH_LIQUID    = 0x20,                     // In any liquid deep enough to be able to swim
    ENVIRONMENT_FLAG_LIQUID         = 0x40,                     // Anywhere indide area with any liquid

    ENVIRONMENT_MASK_LIQUID_HAZARD  = (ENVIRONMENT_FLAG_IN_MAGMA | ENVIRONMENT_FLAG_IN_SLIME),
    ENVIRONMENT_MASK_IN_LIQUID      = (ENVIRONMENT_FLAG_IN_WATER | ENVIRONMENT_MASK_LIQUID_HAZARD),
    ENVIRONMENT_MASK_LIQUID_FLAGS   = (ENVIRONMENT_FLAG_UNDERWATER | ENVIRONMENT_MASK_IN_LIQUID | ENVIRONMENT_FLAG_HIGH_SEA | ENVIRONMENT_FLAG_LIQUID | ENVIRONMENT_FLAG_HIGH_LIQUID),
};
