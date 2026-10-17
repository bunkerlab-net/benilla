enum GridMapLiquidStatus
{
    LIQUID_MAP_NO_WATER = 0x00000000,
    LIQUID_MAP_ABOVE_WATER = 0x00000001,
    LIQUID_MAP_WATER_WALK = 0x00000002,
    LIQUID_MAP_IN_WATER = 0x00000004,
    LIQUID_MAP_UNDER_WATER = 0x00000008
};

// defined in DBC and left shifted for flag usage
#define MAP_LIQUID_TYPE_NO_WATER    0x00
#define MAP_LIQUID_TYPE_MAGMA       0x01
#define MAP_LIQUID_TYPE_OCEAN       0x02
#define MAP_LIQUID_TYPE_SLIME       0x04
#define MAP_LIQUID_TYPE_WATER       0x08

#define MAP_ALL_LIQUIDS   (MAP_LIQUID_TYPE_WATER | MAP_LIQUID_TYPE_MAGMA | MAP_LIQUID_TYPE_OCEAN | MAP_LIQUID_TYPE_SLIME)

#define MAP_LIQUID_TYPE_DEEP_WATER  0x10
#define MAP_LIQUID_TYPE_WMO_WATER   0x20

struct GridMapLiquidData
{
    uint32 type_flags;
    uint32 entry;
    float level;
    float depth_level;
};
