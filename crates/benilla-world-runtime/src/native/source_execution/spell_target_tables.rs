// SPDX-License-Identifier: GPL-3.0-or-later
//! Pinned CMaNGOS Classic 8ec338a1 targeting tables, not live target selection.
use super::super::Serialize;

// SpellTargetDefines.h:114-133.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[repr(u32)]
pub enum SpellTargetImplicitType {
    Unknown = 0,
    LocationSrc = 1,
    LocationDest = 2,
    Gameobject = 3,
    Unit = 4,
    Player = 5,
    Corpse = 6,
    Lock = 7,
    None = 8,
    Item = 9,
    UnitDest = 10,
    Dynamic = 11,
    DynamicAny = 12,
    SpecialDest = 13,
    SpecialUnit = 14,
    SpecialScript = 15,
}

// SpellTargetDefines.h:135-142.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[repr(u32)]
pub enum SpellTargetEnumerator {
    Unknown = 0,
    Single = 1,
    Chain = 2,
    Aoe = 3,
    Cone = 4,
}

// SpellTargetDefines.h:144-152.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[repr(u32)]
pub enum SpellTargetFilter {
    Neutral = 0,
    Harmful = 1,
    Helpful = 2,
    Party = 3,
    Group = 4,
    Script = 5,
}

// SpellTargetDefines.h:154-165.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[repr(u32)]
pub enum SpellTargetFilterScheme {
    Random = 0,
    RandomChain = 1,
    Closest = 2,
    ClosestChain = 3,
    Furthest = 4,
    HighestHp = 5,
    LowestHpChain = 6,
    PrioritizeHealth = 7,
    PrioritizeMana = 8,
}

// SpellTargetDefines.h:167-172.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[repr(u32)]
pub enum SpellTargetLOS {
    Caster = 0,
    Src = 1,
    Dest = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpellTargetInfo {
    pub name: &'static str,
    pub target_type: SpellTargetImplicitType,
    pub filter: SpellTargetFilter,
    pub enumerator: SpellTargetEnumerator,
    pub los: SpellTargetLOS,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpellEffectInfo {
    pub name: &'static str,
    pub required_target: SpellTargetImplicitType,
    pub default_target: u32,
}

// SpellTargets.cpp:31-97; defaults SpellTargets.h:40 and SpellTargets.cpp:26-28.
pub const SPELL_TARGET_INFO_TABLE: [SpellTargetInfo; 64] = [
    SpellTargetInfo {
        name: "TARGET_NONE",
        target_type: SpellTargetImplicitType::Unknown,
        filter: SpellTargetFilter::Neutral,
        enumerator: SpellTargetEnumerator::Unknown,
        los: SpellTargetLOS::Caster,
    }, // 0
    SpellTargetInfo {
        name: "TARGET_UNIT_CASTER",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Helpful,
        enumerator: SpellTargetEnumerator::Single,
        los: SpellTargetLOS::Caster,
    }, // 1
    SpellTargetInfo {
        name: "TARGET_UNIT_ENEMY_NEAR_CASTER",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Harmful,
        enumerator: SpellTargetEnumerator::Chain,
        los: SpellTargetLOS::Caster,
    }, // 2
    SpellTargetInfo {
        name: "TARGET_UNIT_FRIEND_NEAR_CASTER",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Helpful,
        enumerator: SpellTargetEnumerator::Chain,
        los: SpellTargetLOS::Caster,
    }, // 3
    SpellTargetInfo {
        name: "TARGET_UNIT_NEAR_CASTER",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Neutral,
        enumerator: SpellTargetEnumerator::Chain,
        los: SpellTargetLOS::Caster,
    }, // 4
    SpellTargetInfo {
        name: "TARGET_UNIT_CASTER_PET",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Helpful,
        enumerator: SpellTargetEnumerator::Single,
        los: SpellTargetLOS::Caster,
    }, // 5
    SpellTargetInfo {
        name: "TARGET_UNIT_ENEMY",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Harmful,
        enumerator: SpellTargetEnumerator::Chain,
        los: SpellTargetLOS::Caster,
    }, // 6
    SpellTargetInfo {
        name: "TARGET_ENUM_UNITS_SCRIPT_AOE_AT_SRC_LOC",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Script,
        enumerator: SpellTargetEnumerator::Aoe,
        los: SpellTargetLOS::Src,
    }, // 7
    SpellTargetInfo {
        name: "TARGET_ENUM_UNITS_SCRIPT_AOE_AT_DEST_LOC",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Script,
        enumerator: SpellTargetEnumerator::Aoe,
        los: SpellTargetLOS::Dest,
    }, // 8
    SpellTargetInfo {
        name: "TARGET_LOCATION_CASTER_HOME_BIND",
        target_type: SpellTargetImplicitType::LocationDest,
        filter: SpellTargetFilter::Neutral,
        enumerator: SpellTargetEnumerator::Unknown,
        los: SpellTargetLOS::Caster,
    }, // 9
    SpellTargetInfo {
        name: "TARGET_LOCATION_CASTER_DIVINE_BIND_NYI",
        target_type: SpellTargetImplicitType::LocationDest,
        filter: SpellTargetFilter::Neutral,
        enumerator: SpellTargetEnumerator::Unknown,
        los: SpellTargetLOS::Caster,
    }, // 10
    SpellTargetInfo {
        name: "TARGET_PLAYER_NYI",
        target_type: SpellTargetImplicitType::Player,
        filter: SpellTargetFilter::Neutral,
        enumerator: SpellTargetEnumerator::Single,
        los: SpellTargetLOS::Caster,
    }, // 11
    SpellTargetInfo {
        name: "TARGET_PLAYER_NEAR_CASTER_NYI",
        target_type: SpellTargetImplicitType::Player,
        filter: SpellTargetFilter::Neutral,
        enumerator: SpellTargetEnumerator::Chain,
        los: SpellTargetLOS::Caster,
    }, // 12
    SpellTargetInfo {
        name: "TARGET_PLAYER_ENEMY_NYI",
        target_type: SpellTargetImplicitType::Player,
        filter: SpellTargetFilter::Harmful,
        enumerator: SpellTargetEnumerator::Single,
        los: SpellTargetLOS::Caster,
    }, // 13
    SpellTargetInfo {
        name: "TARGET_PLAYER_FRIEND_NYI",
        target_type: SpellTargetImplicitType::Player,
        filter: SpellTargetFilter::Helpful,
        enumerator: SpellTargetEnumerator::Single,
        los: SpellTargetLOS::Caster,
    }, // 14
    SpellTargetInfo {
        name: "TARGET_ENUM_UNITS_ENEMY_AOE_AT_SRC_LOC",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Harmful,
        enumerator: SpellTargetEnumerator::Aoe,
        los: SpellTargetLOS::Src,
    }, // 15
    SpellTargetInfo {
        name: "TARGET_ENUM_UNITS_ENEMY_AOE_AT_DEST_LOC",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Harmful,
        enumerator: SpellTargetEnumerator::Aoe,
        los: SpellTargetLOS::Dest,
    }, // 16
    SpellTargetInfo {
        name: "TARGET_LOCATION_DATABASE",
        target_type: SpellTargetImplicitType::LocationDest,
        filter: SpellTargetFilter::Script,
        enumerator: SpellTargetEnumerator::Unknown,
        los: SpellTargetLOS::Caster,
    }, // 17
    SpellTargetInfo {
        name: "TARGET_LOCATION_CASTER_DEST",
        target_type: SpellTargetImplicitType::LocationDest,
        filter: SpellTargetFilter::Neutral,
        enumerator: SpellTargetEnumerator::Unknown,
        los: SpellTargetLOS::Caster,
    }, // 18
    SpellTargetInfo {
        name: "TARGET_UNK_19",
        target_type: SpellTargetImplicitType::Unknown,
        filter: SpellTargetFilter::Neutral,
        enumerator: SpellTargetEnumerator::Unknown,
        los: SpellTargetLOS::Caster,
    }, // 19
    SpellTargetInfo {
        name: "TARGET_ENUM_UNITS_PARTY_WITHIN_CASTER_RANGE",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Party,
        enumerator: SpellTargetEnumerator::Aoe,
        los: SpellTargetLOS::Caster,
    }, // 20
    SpellTargetInfo {
        name: "TARGET_UNIT_FRIEND",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Helpful,
        enumerator: SpellTargetEnumerator::Single,
        los: SpellTargetLOS::Caster,
    }, // 21
    SpellTargetInfo {
        name: "TARGET_LOCATION_CASTER_SRC",
        target_type: SpellTargetImplicitType::LocationSrc,
        filter: SpellTargetFilter::Neutral,
        enumerator: SpellTargetEnumerator::Unknown,
        los: SpellTargetLOS::Caster,
    }, // 22
    SpellTargetInfo {
        name: "TARGET_GAMEOBJECT",
        target_type: SpellTargetImplicitType::Gameobject,
        filter: SpellTargetFilter::Neutral,
        enumerator: SpellTargetEnumerator::Single,
        los: SpellTargetLOS::Caster,
    }, // 23
    SpellTargetInfo {
        name: "TARGET_ENUM_UNITS_ENEMY_IN_CONE_24",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Harmful,
        enumerator: SpellTargetEnumerator::Cone,
        los: SpellTargetLOS::Caster,
    }, // 24
    SpellTargetInfo {
        name: "TARGET_UNIT",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Neutral,
        enumerator: SpellTargetEnumerator::Single,
        los: SpellTargetLOS::Caster,
    }, // 25
    SpellTargetInfo {
        name: "TARGET_LOCKED",
        target_type: SpellTargetImplicitType::Lock,
        filter: SpellTargetFilter::Neutral,
        enumerator: SpellTargetEnumerator::Single,
        los: SpellTargetLOS::Caster,
    }, // 26
    SpellTargetInfo {
        name: "TARGET_UNIT_CASTER_MASTER",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Helpful,
        enumerator: SpellTargetEnumerator::Single,
        los: SpellTargetLOS::Caster,
    }, // 27
    SpellTargetInfo {
        name: "TARGET_ENUM_UNITS_ENEMY_AOE_AT_DYNOBJ_LOC",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Harmful,
        enumerator: SpellTargetEnumerator::Aoe,
        los: SpellTargetLOS::Caster,
    }, // 28
    SpellTargetInfo {
        name: "TARGET_ENUM_UNITS_FRIEND_AOE_AT_DYNOBJ_LOC",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Helpful,
        enumerator: SpellTargetEnumerator::Aoe,
        los: SpellTargetLOS::Caster,
    }, // 29
    SpellTargetInfo {
        name: "TARGET_ENUM_UNITS_FRIEND_AOE_AT_SRC_LOC",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Helpful,
        enumerator: SpellTargetEnumerator::Aoe,
        los: SpellTargetLOS::Src,
    }, // 30
    SpellTargetInfo {
        name: "TARGET_ENUM_UNITS_FRIEND_AOE_AT_DEST_LOC",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Helpful,
        enumerator: SpellTargetEnumerator::Aoe,
        los: SpellTargetLOS::Dest,
    }, // 31
    SpellTargetInfo {
        name: "TARGET_LOCATION_UNIT_MINION_POSITION",
        target_type: SpellTargetImplicitType::LocationDest,
        filter: SpellTargetFilter::Neutral,
        enumerator: SpellTargetEnumerator::Unknown,
        los: SpellTargetLOS::Caster,
    }, // 32
    SpellTargetInfo {
        name: "TARGET_ENUM_UNITS_PARTY_AOE_AT_SRC_LOC",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Party,
        enumerator: SpellTargetEnumerator::Aoe,
        los: SpellTargetLOS::Src,
    }, // 33
    SpellTargetInfo {
        name: "TARGET_ENUM_UNITS_PARTY_AOE_AT_DEST_LOC",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Party,
        enumerator: SpellTargetEnumerator::Aoe,
        los: SpellTargetLOS::Dest,
    }, // 34
    SpellTargetInfo {
        name: "TARGET_UNIT_PARTY",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Party,
        enumerator: SpellTargetEnumerator::Single,
        los: SpellTargetLOS::Caster,
    }, // 35
    SpellTargetInfo {
        name: "TARGET_ENUM_UNITS_ENEMY_WITHIN_CASTER_RANGE",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Harmful,
        enumerator: SpellTargetEnumerator::Aoe,
        los: SpellTargetLOS::Caster,
    }, // 36
    SpellTargetInfo {
        name: "TARGET_UNIT_FRIEND_AND_PARTY",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Party,
        enumerator: SpellTargetEnumerator::Chain,
        los: SpellTargetLOS::Caster,
    }, // 37
    SpellTargetInfo {
        name: "TARGET_UNIT_SCRIPT_NEAR_CASTER",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Script,
        enumerator: SpellTargetEnumerator::Chain,
        los: SpellTargetLOS::Caster,
    }, // 38
    SpellTargetInfo {
        name: "TARGET_LOCATION_CASTER_FISHING_SPOT",
        target_type: SpellTargetImplicitType::LocationDest,
        filter: SpellTargetFilter::Neutral,
        enumerator: SpellTargetEnumerator::Unknown,
        los: SpellTargetLOS::Caster,
    }, // 39
    SpellTargetInfo {
        name: "TARGET_GAMEOBJECT_SCRIPT_NEAR_CASTER",
        target_type: SpellTargetImplicitType::Gameobject,
        filter: SpellTargetFilter::Script,
        enumerator: SpellTargetEnumerator::Chain,
        los: SpellTargetLOS::Caster,
    }, // 40
    SpellTargetInfo {
        name: "TARGET_LOCATION_CASTER_FRONT_RIGHT",
        target_type: SpellTargetImplicitType::LocationDest,
        filter: SpellTargetFilter::Neutral,
        enumerator: SpellTargetEnumerator::Unknown,
        los: SpellTargetLOS::Caster,
    }, // 41
    SpellTargetInfo {
        name: "TARGET_LOCATION_CASTER_BACK_RIGHT",
        target_type: SpellTargetImplicitType::LocationDest,
        filter: SpellTargetFilter::Neutral,
        enumerator: SpellTargetEnumerator::Unknown,
        los: SpellTargetLOS::Caster,
    }, // 42
    SpellTargetInfo {
        name: "TARGET_LOCATION_CASTER_BACK_LEFT",
        target_type: SpellTargetImplicitType::LocationDest,
        filter: SpellTargetFilter::Neutral,
        enumerator: SpellTargetEnumerator::Unknown,
        los: SpellTargetLOS::Caster,
    }, // 43
    SpellTargetInfo {
        name: "TARGET_LOCATION_CASTER_FRONT_LEFT",
        target_type: SpellTargetImplicitType::LocationDest,
        filter: SpellTargetFilter::Neutral,
        enumerator: SpellTargetEnumerator::Unknown,
        los: SpellTargetLOS::Caster,
    }, // 44
    SpellTargetInfo {
        name: "TARGET_UNIT_FRIEND_CHAIN_HEAL",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Helpful,
        enumerator: SpellTargetEnumerator::Chain,
        los: SpellTargetLOS::Caster,
    }, // 45
    SpellTargetInfo {
        name: "TARGET_LOCATION_SCRIPT_NEAR_CASTER",
        target_type: SpellTargetImplicitType::LocationDest,
        filter: SpellTargetFilter::Script,
        enumerator: SpellTargetEnumerator::Unknown,
        los: SpellTargetLOS::Caster,
    }, // 46
    SpellTargetInfo {
        name: "TARGET_LOCATION_CASTER_FRONT",
        target_type: SpellTargetImplicitType::LocationDest,
        filter: SpellTargetFilter::Neutral,
        enumerator: SpellTargetEnumerator::Unknown,
        los: SpellTargetLOS::Caster,
    }, // 47
    SpellTargetInfo {
        name: "TARGET_LOCATION_CASTER_BACK",
        target_type: SpellTargetImplicitType::LocationDest,
        filter: SpellTargetFilter::Neutral,
        enumerator: SpellTargetEnumerator::Unknown,
        los: SpellTargetLOS::Caster,
    }, // 48
    SpellTargetInfo {
        name: "TARGET_LOCATION_CASTER_LEFT",
        target_type: SpellTargetImplicitType::LocationDest,
        filter: SpellTargetFilter::Neutral,
        enumerator: SpellTargetEnumerator::Unknown,
        los: SpellTargetLOS::Caster,
    }, // 49
    SpellTargetInfo {
        name: "TARGET_LOCATION_CASTER_RIGHT",
        target_type: SpellTargetImplicitType::LocationDest,
        filter: SpellTargetFilter::Neutral,
        enumerator: SpellTargetEnumerator::Unknown,
        los: SpellTargetLOS::Caster,
    }, // 50
    SpellTargetInfo {
        name: "TARGET_ENUM_GAMEOBJECTS_SCRIPT_AOE_AT_SRC_LOC",
        target_type: SpellTargetImplicitType::Gameobject,
        filter: SpellTargetFilter::Script,
        enumerator: SpellTargetEnumerator::Aoe,
        los: SpellTargetLOS::Src,
    }, // 51
    SpellTargetInfo {
        name: "TARGET_ENUM_GAMEOBJECTS_SCRIPT_AOE_AT_DEST_LOC",
        target_type: SpellTargetImplicitType::Gameobject,
        filter: SpellTargetFilter::Script,
        enumerator: SpellTargetEnumerator::Aoe,
        los: SpellTargetLOS::Dest,
    }, // 52
    SpellTargetInfo {
        name: "TARGET_LOCATION_CASTER_TARGET_POSITION",
        target_type: SpellTargetImplicitType::LocationDest,
        filter: SpellTargetFilter::Neutral,
        enumerator: SpellTargetEnumerator::Unknown,
        los: SpellTargetLOS::Caster,
    }, // 53
    SpellTargetInfo {
        name: "TARGET_ENUM_UNITS_ENEMY_IN_CONE_54",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Harmful,
        enumerator: SpellTargetEnumerator::Cone,
        los: SpellTargetLOS::Caster,
    }, // 54
    SpellTargetInfo {
        name: "TARGET_LOCATION_CASTER_FRONT_LEAP",
        target_type: SpellTargetImplicitType::LocationDest,
        filter: SpellTargetFilter::Neutral,
        enumerator: SpellTargetEnumerator::Unknown,
        los: SpellTargetLOS::Caster,
    }, // 55
    SpellTargetInfo {
        name: "TARGET_ENUM_UNITS_RAID_WITHIN_CASTER_RANGE",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Group,
        enumerator: SpellTargetEnumerator::Aoe,
        los: SpellTargetLOS::Caster,
    }, // 56
    SpellTargetInfo {
        name: "TARGET_UNIT_RAID",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Group,
        enumerator: SpellTargetEnumerator::Single,
        los: SpellTargetLOS::Caster,
    }, // 57
    SpellTargetInfo {
        name: "TARGET_UNIT_RAID_NEAR_CASTER",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Group,
        enumerator: SpellTargetEnumerator::Chain,
        los: SpellTargetLOS::Caster,
    }, // 58
    SpellTargetInfo {
        name: "TARGET_ENUM_UNITS_FRIEND_IN_CONE",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Helpful,
        enumerator: SpellTargetEnumerator::Cone,
        los: SpellTargetLOS::Caster,
    }, // 59
    SpellTargetInfo {
        name: "TARGET_ENUM_UNITS_SCRIPT_IN_CONE_60",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Script,
        enumerator: SpellTargetEnumerator::Cone,
        los: SpellTargetLOS::Caster,
    }, // 60
    SpellTargetInfo {
        name: "TARGET_UNIT_RAID_AND_CLASS",
        target_type: SpellTargetImplicitType::Unit,
        filter: SpellTargetFilter::Group,
        enumerator: SpellTargetEnumerator::Chain,
        los: SpellTargetLOS::Caster,
    }, // 61
    SpellTargetInfo {
        name: "TARGET_PLAYER_RAID_NYI",
        target_type: SpellTargetImplicitType::Player,
        filter: SpellTargetFilter::Group,
        enumerator: SpellTargetEnumerator::Single,
        los: SpellTargetLOS::Caster,
    }, // 62
    SpellTargetInfo {
        name: "TARGET_LOCATION_UNIT_POSITION",
        target_type: SpellTargetImplicitType::LocationDest,
        filter: SpellTargetFilter::Neutral,
        enumerator: SpellTargetEnumerator::Unknown,
        los: SpellTargetLOS::Caster,
    }, // 63
];

// SpellTargets.cpp:99-231; defaults SpellTargets.h:40 and SpellTargets.cpp:26-28.
pub const SPELL_EFFECT_INFO_TABLE: [SpellEffectInfo; 130] = [
    SpellEffectInfo {
        name: "SPELL_EFFECT_NONE",
        required_target: SpellTargetImplicitType::Unknown,
        default_target: 0,
    }, // 0
    SpellEffectInfo {
        name: "SPELL_EFFECT_INSTAKILL",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 1
    SpellEffectInfo {
        name: "SPELL_EFFECT_SCHOOL_DAMAGE",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 6,
    }, // 2
    SpellEffectInfo {
        name: "SPELL_EFFECT_DUMMY",
        required_target: SpellTargetImplicitType::Dynamic,
        default_target: 0,
    }, // 3
    SpellEffectInfo {
        name: "SPELL_EFFECT_PORTAL_TELEPORT",
        required_target: SpellTargetImplicitType::Unknown,
        default_target: 0,
    }, // 4
    SpellEffectInfo {
        name: "SPELL_EFFECT_TELEPORT_UNITS",
        required_target: SpellTargetImplicitType::UnitDest,
        default_target: 0,
    }, // 5
    SpellEffectInfo {
        name: "SPELL_EFFECT_APPLY_AURA",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 1,
    }, // 6
    SpellEffectInfo {
        name: "SPELL_EFFECT_ENVIRONMENTAL_DAMAGE",
        required_target: SpellTargetImplicitType::None,
        default_target: 0,
    }, // 7
    SpellEffectInfo {
        name: "SPELL_EFFECT_POWER_DRAIN",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 8
    SpellEffectInfo {
        name: "SPELL_EFFECT_HEALTH_LEECH",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 9
    SpellEffectInfo {
        name: "SPELL_EFFECT_HEAL",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 10
    SpellEffectInfo {
        name: "SPELL_EFFECT_BIND",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 25,
    }, // 11
    SpellEffectInfo {
        name: "SPELL_EFFECT_PORTAL",
        required_target: SpellTargetImplicitType::Unknown,
        default_target: 0,
    }, // 12
    SpellEffectInfo {
        name: "SPELL_EFFECT_RITUAL_BASE",
        required_target: SpellTargetImplicitType::Unknown,
        default_target: 0,
    }, // 13
    SpellEffectInfo {
        name: "SPELL_EFFECT_RITUAL_SPECIALIZE",
        required_target: SpellTargetImplicitType::Unknown,
        default_target: 0,
    }, // 14
    SpellEffectInfo {
        name: "SPELL_EFFECT_RITUAL_ACTIVATE_PORTAL",
        required_target: SpellTargetImplicitType::Unknown,
        default_target: 0,
    }, // 15
    SpellEffectInfo {
        name: "SPELL_EFFECT_QUEST_COMPLETE",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 16
    SpellEffectInfo {
        name: "SPELL_EFFECT_WEAPON_DAMAGE_NOSCHOOL",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 17
    SpellEffectInfo {
        name: "SPELL_EFFECT_RESURRECT",
        required_target: SpellTargetImplicitType::Corpse,
        default_target: 25,
    }, // 18
    SpellEffectInfo {
        name: "SPELL_EFFECT_ADD_EXTRA_ATTACKS",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 19
    SpellEffectInfo {
        name: "SPELL_EFFECT_DODGE",
        required_target: SpellTargetImplicitType::None,
        default_target: 0,
    }, // 20
    SpellEffectInfo {
        name: "SPELL_EFFECT_EVADE",
        required_target: SpellTargetImplicitType::None,
        default_target: 0,
    }, // 21
    SpellEffectInfo {
        name: "SPELL_EFFECT_PARRY",
        required_target: SpellTargetImplicitType::None,
        default_target: 0,
    }, // 22
    SpellEffectInfo {
        name: "SPELL_EFFECT_BLOCK",
        required_target: SpellTargetImplicitType::None,
        default_target: 0,
    }, // 23
    SpellEffectInfo {
        name: "SPELL_EFFECT_CREATE_ITEM",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 25,
    }, // 24
    SpellEffectInfo {
        name: "SPELL_EFFECT_WEAPON",
        required_target: SpellTargetImplicitType::None,
        default_target: 0,
    }, // 25
    SpellEffectInfo {
        name: "SPELL_EFFECT_DEFENSE",
        required_target: SpellTargetImplicitType::None,
        default_target: 0,
    }, // 26
    SpellEffectInfo {
        name: "SPELL_EFFECT_PERSISTENT_AREA_AURA",
        required_target: SpellTargetImplicitType::SpecialDest,
        default_target: 0,
    }, // 27
    SpellEffectInfo {
        name: "SPELL_EFFECT_SUMMON",
        required_target: SpellTargetImplicitType::LocationDest,
        default_target: 18,
    }, // 28
    SpellEffectInfo {
        name: "SPELL_EFFECT_LEAP",
        required_target: SpellTargetImplicitType::UnitDest,
        default_target: 0,
    }, // 29
    SpellEffectInfo {
        name: "SPELL_EFFECT_ENERGIZE",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 30
    SpellEffectInfo {
        name: "SPELL_EFFECT_WEAPON_PERCENT_DAMAGE",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 31
    SpellEffectInfo {
        name: "SPELL_EFFECT_TRIGGER_MISSILE",
        required_target: SpellTargetImplicitType::DynamicAny,
        default_target: 0,
    }, // 32
    SpellEffectInfo {
        name: "SPELL_EFFECT_OPEN_LOCK",
        required_target: SpellTargetImplicitType::Lock,
        default_target: 0,
    }, // 33
    SpellEffectInfo {
        name: "SPELL_EFFECT_SUMMON_CHANGE_ITEM",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 1,
    }, // 34
    SpellEffectInfo {
        name: "SPELL_EFFECT_APPLY_AREA_AURA_PARTY",
        required_target: SpellTargetImplicitType::SpecialUnit,
        default_target: 0,
    }, // 35
    SpellEffectInfo {
        name: "SPELL_EFFECT_LEARN_SPELL",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 25,
    }, // 36
    SpellEffectInfo {
        name: "SPELL_EFFECT_SPELL_DEFENSE",
        required_target: SpellTargetImplicitType::None,
        default_target: 0,
    }, // 37
    SpellEffectInfo {
        name: "SPELL_EFFECT_DISPEL",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 38
    SpellEffectInfo {
        name: "SPELL_EFFECT_LANGUAGE",
        required_target: SpellTargetImplicitType::None,
        default_target: 0,
    }, // 39
    SpellEffectInfo {
        name: "SPELL_EFFECT_DUAL_WIELD",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 40
    SpellEffectInfo {
        name: "SPELL_EFFECT_SUMMON_WILD",
        required_target: SpellTargetImplicitType::LocationDest,
        default_target: 0,
    }, // 41
    SpellEffectInfo {
        name: "SPELL_EFFECT_SUMMON_GUARDIAN",
        required_target: SpellTargetImplicitType::LocationDest,
        default_target: 0,
    }, // 42
    SpellEffectInfo {
        name: "SPELL_EFFECT_TELEPORT_UNITS_FACE_CASTER",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 43
    SpellEffectInfo {
        name: "SPELL_EFFECT_SKILL_STEP",
        required_target: SpellTargetImplicitType::Unknown,
        default_target: 25,
    }, // 44
    SpellEffectInfo {
        name: "SPELL_EFFECT_ADD_HONOR",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 45
    SpellEffectInfo {
        name: "SPELL_EFFECT_SPAWN",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 1,
    }, // 46
    SpellEffectInfo {
        name: "SPELL_EFFECT_TRADE_SKILL",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 1,
    }, // 47
    SpellEffectInfo {
        name: "SPELL_EFFECT_STEALTH",
        required_target: SpellTargetImplicitType::Unknown,
        default_target: 0,
    }, // 48
    SpellEffectInfo {
        name: "SPELL_EFFECT_DETECT",
        required_target: SpellTargetImplicitType::Unknown,
        default_target: 0,
    }, // 49
    SpellEffectInfo {
        name: "SPELL_EFFECT_TRANS_DOOR",
        required_target: SpellTargetImplicitType::LocationDest,
        default_target: 18,
    }, // 50
    SpellEffectInfo {
        name: "SPELL_EFFECT_FORCE_CRITICAL_HIT",
        required_target: SpellTargetImplicitType::Unknown,
        default_target: 0,
    }, // 51
    SpellEffectInfo {
        name: "SPELL_EFFECT_GUARANTEE_HIT",
        required_target: SpellTargetImplicitType::Unknown,
        default_target: 0,
    }, // 52
    SpellEffectInfo {
        name: "SPELL_EFFECT_ENCHANT_ITEM",
        required_target: SpellTargetImplicitType::Item,
        default_target: 0,
    }, // 53
    SpellEffectInfo {
        name: "SPELL_EFFECT_ENCHANT_ITEM_TEMPORARY",
        required_target: SpellTargetImplicitType::Item,
        default_target: 0,
    }, // 54
    SpellEffectInfo {
        name: "SPELL_EFFECT_TAMECREATURE",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 55
    SpellEffectInfo {
        name: "SPELL_EFFECT_SUMMON_PET",
        required_target: SpellTargetImplicitType::LocationDest,
        default_target: 0,
    }, // 56
    SpellEffectInfo {
        name: "SPELL_EFFECT_LEARN_PET_SPELL",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 5,
    }, // 57
    SpellEffectInfo {
        name: "SPELL_EFFECT_WEAPON_DAMAGE",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 58
    SpellEffectInfo {
        name: "SPELL_EFFECT_OPEN_LOCK_ITEM",
        required_target: SpellTargetImplicitType::Lock,
        default_target: 0,
    }, // 59
    SpellEffectInfo {
        name: "SPELL_EFFECT_PROFICIENCY",
        required_target: SpellTargetImplicitType::None,
        default_target: 0,
    }, // 60
    SpellEffectInfo {
        name: "SPELL_EFFECT_SEND_EVENT",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 1,
    }, // 61
    SpellEffectInfo {
        name: "SPELL_EFFECT_POWER_BURN",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 62
    SpellEffectInfo {
        name: "SPELL_EFFECT_THREAT",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 63
    SpellEffectInfo {
        name: "SPELL_EFFECT_TRIGGER_SPELL",
        required_target: SpellTargetImplicitType::Dynamic,
        default_target: 0,
    }, // 64
    SpellEffectInfo {
        name: "SPELL_EFFECT_HEALTH_FUNNEL",
        required_target: SpellTargetImplicitType::Unknown,
        default_target: 0,
    }, // 65
    SpellEffectInfo {
        name: "SPELL_EFFECT_POWER_FUNNEL",
        required_target: SpellTargetImplicitType::Unknown,
        default_target: 0,
    }, // 66
    SpellEffectInfo {
        name: "SPELL_EFFECT_HEAL_MAX_HEALTH",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 67
    SpellEffectInfo {
        name: "SPELL_EFFECT_INTERRUPT_CAST",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 68
    SpellEffectInfo {
        name: "SPELL_EFFECT_DISTRACT",
        required_target: SpellTargetImplicitType::UnitDest,
        default_target: 0,
    }, // 69
    SpellEffectInfo {
        name: "SPELL_EFFECT_PULL",
        required_target: SpellTargetImplicitType::UnitDest,
        default_target: 0,
    }, // 70
    SpellEffectInfo {
        name: "SPELL_EFFECT_PICKPOCKET",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 71
    SpellEffectInfo {
        name: "SPELL_EFFECT_ADD_FARSIGHT",
        required_target: SpellTargetImplicitType::LocationDest,
        default_target: 0,
    }, // 72
    SpellEffectInfo {
        name: "SPELL_EFFECT_SUMMON_POSSESSED",
        required_target: SpellTargetImplicitType::LocationDest,
        default_target: 0,
    }, // 73
    SpellEffectInfo {
        name: "SPELL_EFFECT_SUMMON_TOTEM",
        required_target: SpellTargetImplicitType::LocationDest,
        default_target: 0,
    }, // 74
    SpellEffectInfo {
        name: "SPELL_EFFECT_HEAL_MECHANICAL",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 75
    SpellEffectInfo {
        name: "SPELL_EFFECT_SUMMON_OBJECT_WILD",
        required_target: SpellTargetImplicitType::LocationDest,
        default_target: 0,
    }, // 76
    SpellEffectInfo {
        name: "SPELL_EFFECT_SCRIPT_EFFECT",
        required_target: SpellTargetImplicitType::Dynamic,
        default_target: 0,
    }, // 77
    SpellEffectInfo {
        name: "SPELL_EFFECT_ATTACK",
        required_target: SpellTargetImplicitType::None,
        default_target: 0,
    }, // 78
    SpellEffectInfo {
        name: "SPELL_EFFECT_SANCTUARY",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 1,
    }, // 79
    SpellEffectInfo {
        name: "SPELL_EFFECT_ADD_COMBO_POINTS",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 80
    SpellEffectInfo {
        name: "SPELL_EFFECT_CREATE_HOUSE",
        required_target: SpellTargetImplicitType::LocationDest,
        default_target: 0,
    }, // 81
    SpellEffectInfo {
        name: "SPELL_EFFECT_BIND_SIGHT",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 82
    SpellEffectInfo {
        name: "SPELL_EFFECT_DUEL",
        required_target: SpellTargetImplicitType::UnitDest,
        default_target: 0,
    }, // 83
    SpellEffectInfo {
        name: "SPELL_EFFECT_STUCK",
        required_target: SpellTargetImplicitType::None,
        default_target: 0,
    }, // 84
    SpellEffectInfo {
        name: "SPELL_EFFECT_SUMMON_PLAYER",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 25,
    }, // 85
    SpellEffectInfo {
        name: "SPELL_EFFECT_ACTIVATE_OBJECT",
        required_target: SpellTargetImplicitType::Gameobject,
        default_target: 0,
    }, // 86
    SpellEffectInfo {
        name: "SPELL_EFFECT_SUMMON_TOTEM_SLOT1",
        required_target: SpellTargetImplicitType::LocationDest,
        default_target: 0,
    }, // 87
    SpellEffectInfo {
        name: "SPELL_EFFECT_SUMMON_TOTEM_SLOT2",
        required_target: SpellTargetImplicitType::LocationDest,
        default_target: 0,
    }, // 88
    SpellEffectInfo {
        name: "SPELL_EFFECT_SUMMON_TOTEM_SLOT3",
        required_target: SpellTargetImplicitType::LocationDest,
        default_target: 0,
    }, // 89
    SpellEffectInfo {
        name: "SPELL_EFFECT_SUMMON_TOTEM_SLOT4",
        required_target: SpellTargetImplicitType::LocationDest,
        default_target: 0,
    }, // 90
    SpellEffectInfo {
        name: "SPELL_EFFECT_THREAT_ALL",
        required_target: SpellTargetImplicitType::Unknown,
        default_target: 0,
    }, // 91
    SpellEffectInfo {
        name: "SPELL_EFFECT_ENCHANT_HELD_ITEM",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 92
    SpellEffectInfo {
        name: "SPELL_EFFECT_SUMMON_PHANTASM",
        required_target: SpellTargetImplicitType::LocationDest,
        default_target: 0,
    }, // 93
    SpellEffectInfo {
        name: "SPELL_EFFECT_SELF_RESURRECT",
        required_target: SpellTargetImplicitType::None,
        default_target: 0,
    }, // 94
    SpellEffectInfo {
        name: "SPELL_EFFECT_SKINNING",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 95
    SpellEffectInfo {
        name: "SPELL_EFFECT_CHARGE",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 96
    SpellEffectInfo {
        name: "SPELL_EFFECT_SUMMON_CRITTER",
        required_target: SpellTargetImplicitType::LocationDest,
        default_target: 0,
    }, // 97
    SpellEffectInfo {
        name: "SPELL_EFFECT_KNOCK_BACK",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 98
    SpellEffectInfo {
        name: "SPELL_EFFECT_DISENCHANT",
        required_target: SpellTargetImplicitType::Item,
        default_target: 0,
    }, // 99
    SpellEffectInfo {
        name: "SPELL_EFFECT_INEBRIATE",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 100
    SpellEffectInfo {
        name: "SPELL_EFFECT_FEED_PET",
        required_target: SpellTargetImplicitType::Item,
        default_target: 0,
    }, // 101
    SpellEffectInfo {
        name: "SPELL_EFFECT_DISMISS_PET",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 102
    SpellEffectInfo {
        name: "SPELL_EFFECT_REPUTATION",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 103
    SpellEffectInfo {
        name: "SPELL_EFFECT_SUMMON_OBJECT_SLOT1",
        required_target: SpellTargetImplicitType::LocationDest,
        default_target: 0,
    }, // 104
    SpellEffectInfo {
        name: "SPELL_EFFECT_SUMMON_OBJECT_SLOT2",
        required_target: SpellTargetImplicitType::LocationDest,
        default_target: 0,
    }, // 105
    SpellEffectInfo {
        name: "SPELL_EFFECT_SUMMON_OBJECT_SLOT3",
        required_target: SpellTargetImplicitType::LocationDest,
        default_target: 0,
    }, // 106
    SpellEffectInfo {
        name: "SPELL_EFFECT_SUMMON_OBJECT_SLOT4",
        required_target: SpellTargetImplicitType::LocationDest,
        default_target: 0,
    }, // 107
    SpellEffectInfo {
        name: "SPELL_EFFECT_DISPEL_MECHANIC",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 108
    SpellEffectInfo {
        name: "SPELL_EFFECT_SUMMON_DEAD_PET",
        required_target: SpellTargetImplicitType::LocationDest,
        default_target: 0,
    }, // 109
    SpellEffectInfo {
        name: "SPELL_EFFECT_DESTROY_ALL_TOTEMS",
        required_target: SpellTargetImplicitType::None,
        default_target: 0,
    }, // 110
    SpellEffectInfo {
        name: "SPELL_EFFECT_DURABILITY_DAMAGE",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 111
    SpellEffectInfo {
        name: "SPELL_EFFECT_SUMMON_DEMON",
        required_target: SpellTargetImplicitType::LocationDest,
        default_target: 0,
    }, // 112
    SpellEffectInfo {
        name: "SPELL_EFFECT_RESURRECT_NEW",
        required_target: SpellTargetImplicitType::Corpse,
        default_target: 0,
    }, // 113
    SpellEffectInfo {
        name: "SPELL_EFFECT_ATTACK_ME",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 114
    SpellEffectInfo {
        name: "SPELL_EFFECT_DURABILITY_DAMAGE_PCT",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 115
    SpellEffectInfo {
        name: "SPELL_EFFECT_SKIN_PLAYER_CORPSE",
        required_target: SpellTargetImplicitType::Corpse,
        default_target: 25,
    }, // 116
    SpellEffectInfo {
        name: "SPELL_EFFECT_SPIRIT_HEAL",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 117
    SpellEffectInfo {
        name: "SPELL_EFFECT_SKILL",
        required_target: SpellTargetImplicitType::None,
        default_target: 25,
    }, // 118
    SpellEffectInfo {
        name: "SPELL_EFFECT_APPLY_AREA_AURA_PET",
        required_target: SpellTargetImplicitType::SpecialUnit,
        default_target: 0,
    }, // 119
    SpellEffectInfo {
        name: "SPELL_EFFECT_TELEPORT_GRAVEYARD",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 120
    SpellEffectInfo {
        name: "SPELL_EFFECT_NORMALIZED_WEAPON_DMG",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 121
    SpellEffectInfo {
        name: "SPELL_EFFECT_122",
        required_target: SpellTargetImplicitType::Unknown,
        default_target: 0,
    }, // 122
    SpellEffectInfo {
        name: "SPELL_EFFECT_SEND_TAXI",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 25,
    }, // 123
    SpellEffectInfo {
        name: "SPELL_EFFECT_PULL_TOWARDS",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 124
    SpellEffectInfo {
        name: "SPELL_EFFECT_MODIFY_THREAT_PERCENT",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 125
    SpellEffectInfo {
        name: "SPELL_EFFECT_STEAL_BENEFICIAL_BUFF",
        required_target: SpellTargetImplicitType::Unit,
        default_target: 0,
    }, // 126
    SpellEffectInfo {
        name: "SPELL_EFFECT_PROSPECTING",
        required_target: SpellTargetImplicitType::Item,
        default_target: 0,
    }, // 127
    SpellEffectInfo {
        name: "SPELL_EFFECT_APPLY_AREA_AURA_FRIEND",
        required_target: SpellTargetImplicitType::SpecialUnit,
        default_target: 0,
    }, // 128
    SpellEffectInfo {
        name: "SPELL_EFFECT_APPLY_AREA_AURA_ENEMY",
        required_target: SpellTargetImplicitType::SpecialUnit,
        default_target: 0,
    }, // 129
];

// SpellTargetDefines.h:22-112.
pub const TARGET_NONE: u32 = 0;
pub const TARGET_UNIT_CASTER: u32 = 1;
pub const TARGET_UNIT_ENEMY_NEAR_CASTER: u32 = 2;
pub const TARGET_UNIT_FRIEND_NEAR_CASTER: u32 = 3;
pub const TARGET_UNIT_NEAR_CASTER: u32 = 4;
pub const TARGET_UNIT_CASTER_PET: u32 = 5;
pub const TARGET_UNIT_ENEMY: u32 = 6;
pub const TARGET_ENUM_UNITS_SCRIPT_AOE_AT_SRC_LOC: u32 = 7;
pub const TARGET_ENUM_UNITS_SCRIPT_AOE_AT_DEST_LOC: u32 = 8;
pub const TARGET_LOCATION_CASTER_HOME_BIND: u32 = 9;
pub const TARGET_LOCATION_CASTER_DIVINE_BIND_NYI: u32 = 10;
pub const TARGET_PLAYER_NYI: u32 = 11;
pub const TARGET_PLAYER_NEAR_CASTER_NYI: u32 = 12;
pub const TARGET_PLAYER_ENEMY_NYI: u32 = 13;
pub const TARGET_PLAYER_FRIEND_NYI: u32 = 14;
pub const TARGET_ENUM_UNITS_ENEMY_AOE_AT_SRC_LOC: u32 = 15;
pub const TARGET_ENUM_UNITS_ENEMY_AOE_AT_DEST_LOC: u32 = 16;
pub const TARGET_LOCATION_DATABASE: u32 = 17;
pub const TARGET_LOCATION_CASTER_DEST: u32 = 18;
pub const TARGET_UNK_19: u32 = 19;
pub const TARGET_ENUM_UNITS_PARTY_WITHIN_CASTER_RANGE: u32 = 20;
pub const TARGET_UNIT_FRIEND: u32 = 21;
pub const TARGET_LOCATION_CASTER_SRC: u32 = 22;
pub const TARGET_GAMEOBJECT: u32 = 23;
pub const TARGET_ENUM_UNITS_ENEMY_IN_CONE_24: u32 = 24;
pub const TARGET_UNIT: u32 = 25;
pub const TARGET_LOCKED: u32 = 26;
pub const TARGET_UNIT_CASTER_MASTER: u32 = 27;
pub const TARGET_ENUM_UNITS_ENEMY_AOE_AT_DYNOBJ_LOC: u32 = 28;
pub const TARGET_ENUM_UNITS_FRIEND_AOE_AT_DYNOBJ_LOC: u32 = 29;
pub const TARGET_ENUM_UNITS_FRIEND_AOE_AT_SRC_LOC: u32 = 30;
pub const TARGET_ENUM_UNITS_FRIEND_AOE_AT_DEST_LOC: u32 = 31;
pub const TARGET_LOCATION_UNIT_MINION_POSITION: u32 = 32;
pub const TARGET_ENUM_UNITS_PARTY_AOE_AT_SRC_LOC: u32 = 33;
pub const TARGET_ENUM_UNITS_PARTY_AOE_AT_DEST_LOC: u32 = 34;
pub const TARGET_UNIT_PARTY: u32 = 35;
pub const TARGET_ENUM_UNITS_ENEMY_WITHIN_CASTER_RANGE: u32 = 36;
pub const TARGET_UNIT_FRIEND_AND_PARTY: u32 = 37;
pub const TARGET_UNIT_SCRIPT_NEAR_CASTER: u32 = 38;
pub const TARGET_LOCATION_CASTER_FISHING_SPOT: u32 = 39;
pub const TARGET_GAMEOBJECT_SCRIPT_NEAR_CASTER: u32 = 40;
pub const TARGET_LOCATION_CASTER_FRONT_RIGHT: u32 = 41;
pub const TARGET_LOCATION_CASTER_BACK_RIGHT: u32 = 42;
pub const TARGET_LOCATION_CASTER_BACK_LEFT: u32 = 43;
pub const TARGET_LOCATION_CASTER_FRONT_LEFT: u32 = 44;
pub const TARGET_UNIT_FRIEND_CHAIN_HEAL: u32 = 45;
pub const TARGET_LOCATION_SCRIPT_NEAR_CASTER: u32 = 46;
pub const TARGET_LOCATION_CASTER_FRONT: u32 = 47;
pub const TARGET_LOCATION_CASTER_BACK: u32 = 48;
pub const TARGET_LOCATION_CASTER_LEFT: u32 = 49;
pub const TARGET_LOCATION_CASTER_RIGHT: u32 = 50;
pub const TARGET_ENUM_GAMEOBJECTS_SCRIPT_AOE_AT_SRC_LOC: u32 = 51;
pub const TARGET_ENUM_GAMEOBJECTS_SCRIPT_AOE_AT_DEST_LOC: u32 = 52;
pub const TARGET_LOCATION_CASTER_TARGET_POSITION: u32 = 53;
pub const TARGET_ENUM_UNITS_ENEMY_IN_CONE_54: u32 = 54;
pub const TARGET_LOCATION_CASTER_FRONT_LEAP: u32 = 55;
pub const TARGET_ENUM_UNITS_RAID_WITHIN_CASTER_RANGE: u32 = 56;
pub const TARGET_UNIT_RAID: u32 = 57;
pub const TARGET_UNIT_RAID_NEAR_CASTER: u32 = 58;
pub const TARGET_ENUM_UNITS_FRIEND_IN_CONE: u32 = 59;
pub const TARGET_ENUM_UNITS_SCRIPT_IN_CONE_60: u32 = 60;
pub const TARGET_UNIT_RAID_AND_CLASS: u32 = 61;
pub const TARGET_PLAYER_RAID_NYI: u32 = 62;
pub const TARGET_LOCATION_UNIT_POSITION: u32 = 63;
pub const TARGET_FLAG_SELF: u32 = 0x00000000;
pub const TARGET_FLAG_UNUSED1: u32 = 0x00000001;
pub const TARGET_FLAG_UNIT: u32 = 0x00000002;
pub const TARGET_FLAG_UNIT_RAID: u32 = 0x00000004;
pub const TARGET_FLAG_UNIT_PARTY: u32 = 0x00000008;
pub const TARGET_FLAG_ITEM: u32 = 0x00000010;
pub const TARGET_FLAG_SOURCE_LOCATION: u32 = 0x00000020;
pub const TARGET_FLAG_DEST_LOCATION: u32 = 0x00000040;
pub const TARGET_FLAG_UNIT_ENEMY: u32 = 0x00000080;
pub const TARGET_FLAG_UNIT_ALLY: u32 = 0x00000100;
pub const TARGET_FLAG_CORPSE_ENEMY: u32 = 0x00000200;
pub const TARGET_FLAG_UNIT_DEAD: u32 = 0x00000400;
pub const TARGET_FLAG_GAMEOBJECT: u32 = 0x00000800;
pub const TARGET_FLAG_TRADE_ITEM: u32 = 0x00001000;
pub const TARGET_FLAG_STRING: u32 = 0x00002000;
pub const TARGET_FLAG_LOCKED: u32 = 0x00004000;
pub const TARGET_FLAG_CORPSE_ALLY: u32 = 0x00008000;
pub const TARGET_FLAG_UNIT_MINIPET: u32 = 0x00010000;
pub const TARGET_TYPE_UNKNOWN: u32 = 0;
pub const TARGET_ENUMERATOR_UNKNOWN: u32 = 0;
pub const TARGET_NEUTRAL: u32 = 0;
pub const TARGET_LOS_CASTER: u32 = 0;
