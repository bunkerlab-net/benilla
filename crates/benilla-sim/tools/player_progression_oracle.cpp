// SPDX-License-Identifier: GPL-3.0-or-later
// Synthetic adapters only. The generator inserts unchanged pinned CMaNGOS bodies.
// See generate_player_progression_fixture.py for input/schema/provenance boundaries.
#include <algorithm>
#include <array>
#include <cassert>
#include <cfenv>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <functional>
#include <iostream>
#include <map>
#include <sstream>
#include <string>
#include <unordered_map>
#include <vector>
using uint8 = uint8_t;
using uint16 = uint16_t;
using uint32 = uint32_t;
using int32 = int32_t;
static std::vector<std::string> events;
void emit(const std::string& event) { events.push_back(event); }
struct Logger { template<class... T> void outError(const char*, T...) { assert(false); } } sLog;
enum Stats { STAT_STRENGTH, STAT_AGILITY, STAT_STAMINA, STAT_INTELLECT, STAT_SPIRIT, MAX_STATS };
enum Powers : uint32 { POWER_MANA, POWER_RAGE, POWER_FOCUS, POWER_ENERGY, POWER_HAPPINESS, POWER_HEALTH = 0xfffffffe };
constexpr int MAX_POWERS = 5;
enum SpellSchools { SPELL_SCHOOL_NORMAL, SPELL_SCHOOL_HOLY, SPELL_SCHOOL_FIRE, SPELL_SCHOOL_NATURE, SPELL_SCHOOL_FROST, SPELL_SCHOOL_SHADOW, SPELL_SCHOOL_ARCANE, MAX_SPELL_SCHOOL };
enum UnitMods { UNIT_MOD_STAT_START = 0, UNIT_MOD_HEALTH = 5, UNIT_MOD_POWER_START, UNIT_MOD_RESISTANCE_START = UNIT_MOD_POWER_START + MAX_POWERS, UNIT_MOD_ARMOR = UNIT_MOD_RESISTANCE_START, UNIT_MOD_END = UNIT_MOD_RESISTANCE_START + MAX_SPELL_SCHOOL };
enum UnitModifierType { BASE_VALUE, BASE_PCT, TOTAL_VALUE, TOTAL_PCT, MODIFIER_TYPE_END };
enum SkillUpdateState { SKILL_UNCHANGED, SKILL_CHANGED, SKILL_NEW, SKILL_DELETED };
enum SkillRangeType { SKILL_RANGE_LANGUAGE, SKILL_RANGE_LEVEL, SKILL_RANGE_MONO, SKILL_RANGE_RANK, SKILL_RANGE_NONE };
constexpr uint32 SKILL_CATEGORY_LANGUAGES = 10, SKILL_CATEGORY_WEAPON = 6, SKILL_CATEGORY_ARMOR = 8, SKILL_CATEGORY_CLASS = 7, SKILL_CATEGORY_SECONDARY = 9, SKILL_CATEGORY_PROFESSION = 11, SKILL_CATEGORY_ATTRIBUTES = 5, SKILL_CATEGORY_GENERIC = 12;
constexpr uint32 SKILL_FIST_WEAPONS = 473, SKILL_POISONS = 40, SKILL_LOCKPICKING = 633, SKILL_FISHING = 356, SKILL_COOKING = 185, SKILL_FIRST_AID = 129, SKILL_RIDING = 762, SKILL_DEFENSE = 95, SKILL_FLAG_MAXIMIZED = 0x10, PLAYER_MAX_SKILLS = 127;
constexpr uint32 TYPEID_PLAYER = 4, TYPEID_UNIT = 3, HUNTER_PET = 1;
constexpr uint32 POWER_RAGE_DEFAULT = 1000, POWER_FOCUS_DEFAULT = 100, POWER_ENERGY_DEFAULT = 100, POWER_HAPPINESS_DEFAULT = 1000000;
constexpr uint32 PLAYER_XP = 100, PLAYER_NEXT_LEVEL_XP = 101, UNIT_FIELD_HEALTH = 1000, UNIT_FIELD_MAXHEALTH = 1001, UNIT_FIELD_POWER1 = 1100, UNIT_FIELD_MAXPOWER1 = 1200, UNIT_FIELD_STAT0 = 1300;
constexpr uint32 CONFIG_UINT32_MAX_PLAYER_LEVEL = 0, CONFIG_BOOL_ALWAYS_MAX_SKILL_FOR_LEVEL = 1, PLAYED_TIME_LEVEL = 0, SMSG_LEVELUP_INFO = 1;
constexpr uint32 BASE_ATTACK = 0, OFF_ATTACK = 1, RANGED_ATTACK = 2, SPELL_AURA_MOD_RESISTANCE_OF_STAT_PERCENT = 0, SPELL_AURA_MOD_SKILL = 1, SPELL_AURA_MOD_SKILL_TALENT = 2, SPELL_SCHOOL_MASK_NORMAL = 1;
constexpr uint32 GROUP_UPDATE_FLAG_CUR_HP = 0, GROUP_UPDATE_FLAG_PET_CUR_HP = 1, GROUP_UPDATE_FLAG_MAX_HP = 2, GROUP_UPDATE_FLAG_PET_MAX_HP = 3, GROUP_UPDATE_FLAG_CUR_POWER = 4, GROUP_UPDATE_FLAG_PET_CUR_POWER = 5, GROUP_UPDATE_FLAG_MAX_POWER = 6, GROUP_UPDATE_FLAG_PET_MAX_POWER = 7;
uint32 PLAYER_SKILL_INDEX(uint32 i) { return 2000 + i * 3; }
uint32 PLAYER_SKILL_VALUE_INDEX(uint32 i) { return 2001 + i * 3; }
uint32 PLAYER_SKILL_BONUS_INDEX(uint32 i) { return 2002 + i * 3; }
uint32 MAKE_PAIR32(uint16 lo, uint16 hi) { return uint32(lo) | (uint32(hi) << 16); }
uint32 MAKE_SKILL_VALUE(uint16 lo, uint16 hi) { return MAKE_PAIR32(lo, hi); }
uint16 SKILL_VALUE(uint32 n) { return uint16(n); }
uint16 SKILL_MAX(uint32 n) { return uint16(n >> 16); }
struct SkillStatusData { uint8 pos; SkillUpdateState uState; SkillStatusData(uint8 p, SkillUpdateState s) : pos(p), uState(s) {} };
using SkillStatusMap = std::unordered_map<uint32, SkillStatusData>;
struct SkillLineEntry { uint32 id; int32 categoryId; };
struct SkillRaceClassInfoEntry { uint32 skillId, raceMask, classMask, flags; };
struct SkillStore { std::map<uint32, SkillLineEntry> rows; const SkillLineEntry* LookupEntry(uint32 id) const { auto it = rows.find(id); return it == rows.end() ? nullptr : &it->second; } } sSkillLineStore;
struct SpellMgr { std::multimap<uint32, const SkillRaceClassInfoEntry*> rows; auto GetSkillRaceClassInfoMapBounds(uint32 id) const { return rows.equal_range(id); } } sSpellMgr;
struct World {
    uint32 cap = 9, maximize = 0;
    uint32 getConfig(uint32 key) const { assert(key < 2); return key ? maximize : cap; }
// PINNED_GLOBAL_SKILL_CAP
} sWorld;
// PINNED_LEVEL_STRUCT
// PINNED_CLASS_STRUCT
struct ObjectMgr {
    uint32 profile = 0;
    std::vector<uint32> mPlayerXPperLevel;
    void GetPlayerLevelInfo(uint32 race, uint32 cls, uint32 level, PlayerLevelInfo* info) const {
        assert(race == 1 && cls == 8 && level >= 1 && level <= 9);
        for (uint32 i = 0; i < MAX_STATS; ++i) info->stats[i] = uint8(12 + 2 * level + 3 * i);
    }
    void GetPlayerClassLevelInfo(uint32 cls, uint32 level, PlayerClassLevelInfo* info) const {
        assert(cls == 8 && level >= 1 && level <= 9);
        info->basehealth = uint16(profile == 1 ? 200 - 7 * level : 40 + 7 * level);
        info->basemana = uint16(profile == 1 ? 300 - 11 * level : 55 + 11 * level);
    }
    uint32 GetXPForLevel(uint32 level) const;
} sObjectMgr;
struct WorldPacket { uint32 opcode; std::vector<uint32> words; WorldPacket(uint32 op, uint32) : opcode(op) {} WorldPacket& operator<<(uint32 v) { words.push_back(v); return *this; } };
struct Session {
    uint32 level = 1;
    std::vector<std::vector<uint32>> packets;
    void SendPacket(const WorldPacket& packet) { assert(packet.opcode == SMSG_LEVELUP_INFO && packet.words.size() == 12); packets.push_back(packet.words); emit("level_packet"); }
    void SetCurrentPlayerLevel(uint32 v) { level = v; emit("session_level"); }
};
struct LoggedPlayed { uint32 value = 77; LoggedPlayed& operator=(uint32 v) { value = v; emit("played_reset"); return *this; } };
struct Modifier { int32 m_miscvalue{}, m_amount{}; };
struct Aura { Modifier* GetModifier() { assert(false); return nullptr; } void ApplyModifier(bool) { assert(false); } };
struct Object {
    std::map<uint32, uint32> fields;
    virtual ~Object() = default;
    uint32 GetUInt32Value(uint32 index) const { auto it = fields.find(index); return it == fields.end() ? 0 : it->second; }
    virtual void SetUInt32Value(uint32 index, uint32 value) {
        fields[index] = value;
        if (index == UNIT_FIELD_HEALTH) emit("health");
        else if (index == UNIT_FIELD_MAXHEALTH) emit("max_health");
        else if (index >= UNIT_FIELD_POWER1 && index < UNIT_FIELD_POWER1 + MAX_POWERS) emit("power" + std::to_string(index - UNIT_FIELD_POWER1));
        else if (index >= UNIT_FIELD_MAXPOWER1 && index < UNIT_FIELD_MAXPOWER1 + MAX_POWERS) emit("max_power" + std::to_string(index - UNIT_FIELD_MAXPOWER1));
        else if (index >= UNIT_FIELD_STAT0 && index < UNIT_FIELD_STAT0 + MAX_STATS) emit("stat" + std::to_string(index - UNIT_FIELD_STAT0));
        else assert(false);
    }
    void SetStatInt32Value(uint16, int32);
};
struct Unit : Object {
    uint32 level = 1, create_health = 0, create_mana = 0;
    float create_stats[MAX_STATS]{};
    int32 resist[MAX_SPELL_SCHOOL]{};
    float m_auraModifiersGroup[UNIT_MOD_END][MODIFIER_TYPE_END]{};
    Unit() { for (auto& row : m_auraModifiersGroup) row[BASE_PCT] = row[TOTAL_PCT] = 1.f; }
    uint32 GetTypeId() const { return TYPEID_PLAYER; }
    bool IsPlayer() const { return true; }
    bool HasCharmer() const { return false; }
    Unit* GetCharmer() { assert(false); return nullptr; }
    uint32 GetLevel() const { return level; }
    void SetLevel(uint32 v) { level = v; emit("level"); }
    float GetCreateStat(Stats stat) const { return create_stats[stat]; }
    void SetCreateStat(Stats stat, float v) { create_stats[stat] = v; emit("create_stat" + std::to_string(stat)); }
    float GetStat(Stats stat) const { return float(GetUInt32Value(UNIT_FIELD_STAT0 + stat)); }
    void SetStat(Stats stat, int32 v) { SetStatInt32Value(uint16(UNIT_FIELD_STAT0 + stat), v); }
    uint32 GetCreateHealth() const { return create_health; }
    uint32 GetCreateMana() const { return create_mana; }
    void SetCreateHealth(uint32 v) { create_health = v; emit("create_health"); }
    void SetCreateMana(uint32 v) { create_mana = v; emit("create_mana"); }
    uint32 GetHealth() const { return GetUInt32Value(UNIT_FIELD_HEALTH); }
    uint32 GetMaxHealth() const { return GetUInt32Value(UNIT_FIELD_MAXHEALTH); }
    uint32 GetPower(Powers p) const { return GetUInt32Value(UNIT_FIELD_POWER1 + p); }
    uint32 GetMaxPower(Powers p) const { return GetUInt32Value(UNIT_FIELD_MAXPOWER1 + p); }
    int32 GetCreateResistance(SpellSchools) const { return 0; }
    void SetArmor(int32 v) { resist[0] = v; emit("armor"); }
    void SetResistance(SpellSchools school, int32 v) { resist[school] = v; emit("resist" + std::to_string(school)); }
    uint32 GetCreatePowers(Powers) const;
    float GetModifierValue(UnitMods, UnitModifierType) const;
    float GetTotalStatValue(Stats) const;
    float GetTotalResistanceValue(SpellSchools) const;
    static float GetHealthBonusFromStamina(float); float GetHealthBonusFromStamina() const;
    static float GetManaBonusFromIntellect(float); float GetManaBonusFromIntellect() const;
    void UpdateMaxHealth(); void UpdateMaxPower(Powers);
    void SetHealth(uint32); void SetMaxHealth(uint32); void SetPower(Powers, uint32); void SetMaxPower(Powers, uint32);
};
struct Creature : Unit { bool IsPet() const { assert(false); return false; } };
struct Pet : Creature { uint32 getPetType() const { assert(false); return 0; } void UpdateDamagePhysical(uint32) { assert(false); } void SynchronizeLevelWithOwner() { assert(false); } };
struct Player : Unit {
    bool alive = true;
    uint32 free_talents = 0;
    LoggedPlayed m_Played_time[1];
    SkillStatusMap mSkillStatus;
    Session session;
    bool has_xp_log = false;
    uint32 xp_log = 0;
    bool IsAlive() const { return alive; }
    uint32 getRace() const { return 1; } uint32 getClass() const { return 8; }
    uint32 getRaceMask() const { return 1; } uint32 getClassMask() const { return 128; }
    uint16 GetSkillMaxForLevel() const { return uint16(GetLevel() * 5); }
    void* GetGroup() const { return nullptr; }
    void SetGroupUpdateFlag(uint32) { assert(false); }
    Session* GetSession() { return &session; }
    Pet* GetPet() { return nullptr; }
    uint32 GetXPRestBonus(uint32) { assert(false); return 0; }
    void SendLogXPGain(uint32 xp, Unit* victim, uint32 rest, float rate) { assert(victim == nullptr && rest == 0 && rate == 1.f); has_xp_log = true; xp_log = xp; emit("xp_log"); }
    void SetUInt32Value(uint32 index, uint32 value) override {
        if (index == PLAYER_XP) { fields[index] = value; emit("xp"); }
        else if (index == PLAYER_NEXT_LEVEL_XP) { fields[index] = value; emit("next_xp"); }
        else if (index >= PLAYER_SKILL_INDEX(0) && index <= PLAYER_SKILL_BONUS_INDEX(PLAYER_MAX_SKILLS - 1)) {
            fields[index] = value;
            emit((index - PLAYER_SKILL_INDEX(0)) % 3 == 0 ? "skill_id" : ((index - PLAYER_SKILL_INDEX(0)) % 3 == 1 ? "skill_value" : "skill_bonus"));
        } else Object::SetUInt32Value(index, value);
    }
    const std::vector<Aura*>& GetAurasByType(uint32) const { static std::vector<Aura*> empty; return empty; }
    void UpdateSkillTrainedSpells(uint16, uint16) { emit("skill_trained"); }
    void UpdateDefenseBonusesMod() { emit("defense"); }
    void InitTalentForLevel() { assert(level < 10 && free_talents == 0); emit("talents"); }
    void UpdateAttackPowerAndDamage(bool ranged = false) { emit(ranged ? "attack_ranged" : "attack_melee"); }
    void UpdateAllCritPercentages() { emit("crit"); }
    void UpdateAllSpellCritChances() { emit("spell_crit"); }
    void UpdateSpellDamageBonus() { emit("spell_damage"); }
    void UpdateManaRegen() { emit("mana_regen"); }
    void UpdateWeaponHitChances(uint32 attack) { emit("hit" + std::to_string(attack)); }
    const SkillRaceClassInfoEntry* GetSkillInfo(uint16, std::function<bool(const SkillRaceClassInfoEntry&)> = nullptr) const;
    void SetSkill(SkillStatusMap::iterator, uint16, uint16, uint16 = 0);
    void SetSkill(uint16, uint16, uint16, uint16 = 0);
    void UpdateSkillsForLevel(bool = false);
    void GiveXP(uint32, Creature*, float); void GiveLevel(uint32);
    void UpdateArmor(); void UpdateResistances(uint32); bool UpdateAllStats();
};
// PINNED_GET_XP_FOR_LEVEL
// PINNED_STAT_INT32
// PINNED_PRIMARY_PROFESSION
// PINNED_IS_PROFESSION
// PINNED_IS_RIDING
// PINNED_SKILL_RANGE
// PINNED_GET_SKILL_INFO
// PINNED_SET_SKILL_ITER
// PINNED_SET_SKILL_ID
// PINNED_UPDATE_SKILLS
// PINNED_GIVE_XP
// PINNED_GIVE_LEVEL
// PINNED_MODIFIER_VALUE
// PINNED_TOTAL_STAT
// PINNED_TOTAL_RESIST
// PINNED_CREATE_POWERS
// PINNED_SET_HEALTH
// PINNED_SET_MAX_HEALTH
// PINNED_SET_POWER
// PINNED_SET_MAX_POWER
// PINNED_HEALTH_BONUS
// PINNED_HEALTH_GETTER
// PINNED_MANA_BONUS
// PINNED_MANA_GETTER
// PINNED_MAX_HEALTH
// PINNED_MAX_POWER
// PINNED_ARMOR
// PINNED_RESISTANCES
// PINNED_UPDATE_ALL
struct Case {
    char kind = 'X'; uint32 level = 1, cap = 9, xp = 0, amount = 0;
    bool alive = true; uint32 profile = 0, maximize = 0;
    uint32 skill_case = 0, health = 17, mana = 19, rage = 271, energy = 17;
};
uint16 init_skill(Player& p, uint32 which, SkillRaceClassInfoEntry& info) {
    assert(which < 14);
    uint16 id = 60001, value = 3, max = uint16(p.level * 5), step = 2;
    int32 category = 6;
    SkillUpdateState status = SKILL_UNCHANGED;
    info = {id, 1, 128, 0};
    if (which == 1 || which == 9 || which == 11 || which == 13) info.flags = 16;
    if (which == 2) { id = 60002; category = 8; value = max = 1; step = 0; }
    if (which == 3) { id = 60003; category = 7; step = 1; }
    if (which == 4) { id = 60004; category = 10; value = 7; max = 300; step = 0; }
    if (which == 5) { id = 60005; category = 11; max = 75; step = 1; }
    if (which == 6) { id = 60006; category = 12; step = 0; }
    if (which == 7) status = SKILL_DELETED;
    if (which == 9) info.raceMask = 2;
    if (which == 10) { id = 95; step = 0; }
    if (which == 11) status = SKILL_NEW;
    if (which == 12) { id = 473; value = max = 1; step = 0; }
    if (which == 13) max = 1;
    info.skillId = id;
    sSkillLineStore.rows.clear();
    if (which != 8) sSkillLineStore.rows.emplace(id, SkillLineEntry{id, category});
    sSpellMgr.rows.clear(); sSpellMgr.rows.emplace(id, &info);
    p.mSkillStatus.emplace(id, SkillStatusData(0, status));
    p.fields[PLAYER_SKILL_INDEX(0)] = MAKE_PAIR32(id, step);
    p.fields[PLAYER_SKILL_VALUE_INDEX(0)] = MAKE_SKILL_VALUE(value, max);
    return id;
}
void apply_profile_modifiers(Unit& unit, uint32 profile) {
    auto set = [&](UnitMods mod, float base, float base_pct, float total, float total_pct) {
        auto& row = unit.m_auraModifiersGroup[mod];
        row[BASE_VALUE] = base; row[BASE_PCT] = base_pct; row[TOTAL_VALUE] = total; row[TOTAL_PCT] = total_pct;
    };
    if (profile == 2) {
        for (uint32 i = 0; i < MAX_STATS; ++i) set(UnitMods(i), float(3 + i), 1.05f, -2.f, 1.17f);
        set(UNIT_MOD_HEALTH, 7.f, 1.10f, -3.f, 1.07f);
        set(UnitMods(UNIT_MOD_POWER_START + POWER_MANA), 11.f, .95f, 5.f, 1.03f);
        set(UNIT_MOD_ARMOR, 13.f, 1.f, 7.f, 1.15f);
    } else if (profile == 3) {
        set(UnitMods(STAT_STRENGTH), 0.f, 1.f, -100.f, 1.f);
        set(UnitMods(STAT_AGILITY), 0.f, 1.f, 0.f, 0.f);
        set(UnitMods(STAT_STAMINA), 1.25f, .5f, 2.25f, 1.125f);
        set(UnitMods(STAT_INTELLECT), -3.f, 1.01f, .5f, .93f);
        set(UnitMods(STAT_SPIRIT), 0.f, 1.f, 0.f, -.5f);
        set(UNIT_MOD_HEALTH, 1.25f, .75f, -1.5f, 1.25f);
        set(UnitMods(UNIT_MOD_POWER_START + POWER_MANA), 2.5f, 1.125f, -1.25f, .875f);
        set(UNIT_MOD_ARMOR, 5.5f, .9f, -3.25f, 1.375f);
    } else if (profile == 4) {
        set(UNIT_MOD_HEALTH, .5f, 1.f, 0.f, 1.f);
        set(UnitMods(UNIT_MOD_POWER_START + POWER_MANA), -.5f, 1.f, 0.f, 1.f);
    }
}
void run(const Case& c) {
    assert(c.level >= 1 && c.level <= 9 && c.cap >= 1 && c.cap <= 100 && c.profile < 5 && c.level <= c.cap);
    sWorld.cap = c.cap; sWorld.maximize = c.maximize; sObjectMgr.profile = c.profile;
    sObjectMgr.mPlayerXPperLevel.assign(c.cap, 0);
    for (uint32 level = 1; level < c.cap; ++level) sObjectMgr.mPlayerXPperLevel[level] = 100 + 37 * level;
    Player p; apply_profile_modifiers(p, c.profile); p.level = c.level; p.session.level = c.level; p.alive = c.alive;
    PlayerLevelInfo info; PlayerClassLevelInfo class_info;
    sObjectMgr.GetPlayerLevelInfo(1, 8, c.level, &info); sObjectMgr.GetPlayerClassLevelInfo(8, c.level, &class_info);
    for (uint32 i = 0; i < MAX_STATS; ++i) p.create_stats[i] = info.stats[i];
    p.create_health = class_info.basehealth; p.create_mana = class_info.basemana;
    p.UpdateAllStats();
    p.fields[UNIT_FIELD_HEALTH] = c.health; p.fields[UNIT_FIELD_POWER1 + POWER_MANA] = c.mana;
    p.fields[UNIT_FIELD_POWER1 + POWER_RAGE] = c.rage; p.fields[UNIT_FIELD_POWER1 + POWER_ENERGY] = c.energy;
    p.fields[PLAYER_XP] = c.xp; p.fields[PLAYER_NEXT_LEVEL_XP] = sObjectMgr.GetXPForLevel(c.level);
    SkillRaceClassInfoEntry skill_info{}; uint16 skill = init_skill(p, c.skill_case, skill_info);
    events.clear();
    if (c.kind == 'X') p.GiveXP(c.amount, nullptr, 1.f);
    else if (c.kind == 'L') { assert(c.amount >= c.level && c.amount <= c.cap); p.GiveLevel(c.amount); }
    else { assert(c.kind == 'S'); p.UpdateSkillsForLevel(c.amount != 0); }
    std::cout << c.kind << '\t' << c.level << '\t' << c.cap << '\t' << c.xp << '\t' << c.amount << '\t' << c.alive << '\t' << c.profile << '\t' << c.maximize << '\t' << sWorld.GetConfigMaxSkillValue() << '\t' << c.skill_case << '\t' << c.health << '\t' << c.mana << '\t' << c.rage << '\t' << c.energy
              << '\t' << p.level << '\t' << p.GetUInt32Value(PLAYER_XP) << '\t' << p.GetUInt32Value(PLAYER_NEXT_LEVEL_XP) << '\t' << p.create_health << '\t' << p.create_mana << '\t';
    for (uint32 i = 0; i < MAX_STATS; ++i) { if (i) std::cout << ','; std::cout << int32(p.create_stats[i]); }
    std::cout << '\t';
    for (uint32 i = 0; i < MAX_STATS; ++i) { if (i) std::cout << ','; std::cout << int32(p.GetStat(Stats(i))); }
    std::cout << '\t' << p.GetHealth() << '\t' << p.GetMaxHealth() << '\t' << p.GetPower(POWER_MANA) << '\t' << p.GetMaxPower(POWER_MANA) << '\t' << p.GetPower(POWER_RAGE) << '\t' << p.GetMaxPower(POWER_RAGE) << '\t' << p.GetPower(POWER_ENERGY) << '\t' << p.GetMaxPower(POWER_ENERGY) << '\t' << p.resist[0];
    uint32 skill_value = p.GetUInt32Value(PLAYER_SKILL_VALUE_INDEX(0));
    std::cout << '\t' << SKILL_VALUE(skill_value) << '\t' << SKILL_MAX(skill_value) << '\t' << uint16(p.GetUInt32Value(PLAYER_SKILL_INDEX(0)) >> 16) << '\t' << p.mSkillStatus.at(skill).uState << '\t' << p.m_Played_time[0].value << '\t' << p.session.level << '\t' << p.free_talents << '\t';
    if (p.has_xp_log) std::cout << p.xp_log; else std::cout << '-';
    std::cout << '\t';
    if (p.session.packets.empty()) std::cout << '-';
    for (size_t i = 0; i < p.session.packets.size(); ++i) {
        if (i) std::cout << ';';
        for (size_t j = 0; j < p.session.packets[i].size(); ++j) { if (j) std::cout << ','; std::cout << p.session.packets[i][j]; }
    }
    std::cout << '\t';
    if (events.empty()) std::cout << '-';
    for (size_t i = 0; i < events.size(); ++i) { if (i) std::cout << ','; std::cout << events[i]; }
    std::cout << '\n';
}
void verify_armor_round_trip() {
    // Unchanged UpdateArmor has a persistent binary32 add/sub side effect.
    // Its successive-level result differs from retaining the starting cell.
    const auto from_bits = [](uint32 bits) { float v; std::memcpy(&v, &bits, sizeof(v)); return v; };
    const auto to_bits = [](float v) { uint32 bits; std::memcpy(&bits, &v, sizeof(bits)); return bits; };
    Player p;
    p.m_auraModifiersGroup[UNIT_MOD_ARMOR][TOTAL_VALUE] = from_bits(0x42155800);
    p.m_auraModifiersGroup[UNIT_MOD_ARMOR][TOTAL_PCT] = from_bits(0x3f52266f);
    p.fields[UNIT_FIELD_STAT0 + STAT_AGILITY] = 26459;
    p.UpdateArmor();
    assert(to_bits(p.m_auraModifiersGroup[UNIT_MOD_ARMOR][TOTAL_VALUE]) == 0x42155800);
    p.fields[UNIT_FIELD_STAT0 + STAT_AGILITY] = 7 * 26459;
    p.UpdateArmor();
    assert(to_bits(p.m_auraModifiersGroup[UNIT_MOD_ARMOR][TOTAL_VALUE]) == 0x42156000);
    p.fields[UNIT_FIELD_STAT0 + STAT_AGILITY] = 10 * 26459;
    p.UpdateArmor();
    assert(p.resist[SPELL_SCHOOL_NORMAL] == 434434);
    Player fixed;
    fixed.m_auraModifiersGroup[UNIT_MOD_ARMOR][TOTAL_VALUE] = from_bits(0x42155800);
    fixed.m_auraModifiersGroup[UNIT_MOD_ARMOR][TOTAL_PCT] = from_bits(0x3f52266f);
    fixed.fields[UNIT_FIELD_STAT0 + STAT_AGILITY] = 10 * 26459;
    fixed.UpdateArmor();
    assert(fixed.resist[SPELL_SCHOOL_NORMAL] == 434433);
}
int main() {
    assert(std::fesetround(FE_TONEAREST) == 0);
    verify_armor_round_trip();
    std::cout << "# Synthetic CMaNGOS source-extracted player progression; no game rows; upstream 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f\n"
              << "# kind\tstart_level\tcap\told_xp\tamount_or_target\talive\tprofile\tmaximize\tglobal_skill_cap\tskill_case\thealth_in\tmana_in\trage_in\tenergy_in\tout_level\tout_xp\tout_next_xp\tout_create_health\tout_create_mana\tout_base_stats_csv\tout_stats_csv\thealth\tmax_health\tmana\tmax_mana\trage\tmax_rage\tenergy\tmax_energy\tarmor\tskill_value\tskill_max\tskill_step\tskill_status\tout_played_time\tout_session_level\tout_free_talents\tout_xp_log\tpacket_rows\ttrace\n";
    for (uint32 profile : {0u, 1u}) for (uint32 level = 1; level <= 9; ++level) {
        uint32 next = 100 + 37 * level, remaining = 0;
        for (uint32 at = level; at < 9; ++at) remaining += 100 + 37 * at;
        for (uint32 xp : {0u, next - 1}) for (uint32 amount : {0u, 1u, next - 1, next, next + 1, remaining, remaining + 1, 0xffffffffu}) {
            Case c; c.profile = profile; c.level = level; c.xp = xp; c.amount = amount; run(c);
        }
        for (uint32 target = level; target <= 9; ++target) for (bool alive : {false, true}) {
            Case c; c.kind = 'L'; c.profile = profile; c.level = level; c.xp = 23; c.amount = target; c.alive = alive; c.health = alive ? 17 : 0; run(c);
        }
    }
    for (uint32 skill_case = 0; skill_case < 14; ++skill_case) for (uint32 maximize : {0u, 1u}) for (uint32 target : {2u, 5u, 9u}) {
        Case c; c.kind = 'L'; c.amount = target; c.skill_case = skill_case; c.maximize = maximize; run(c);
    }
    for (uint32 skill_case = 0; skill_case < 14; ++skill_case) for (uint32 maximize : {0u, 1u}) for (uint32 explicit_maximize : {0u, 1u}) {
        Case c; c.kind = 'S'; c.level = 5; c.amount = explicit_maximize; c.skill_case = skill_case; c.maximize = maximize; run(c);
    }
    for (uint32 level : {1u, 3u, 9u}) for (uint32 cap : {1u, 3u, 9u}) for (bool alive : {false, true}) for (uint32 amount : {0u, 1u, 5000u}) {
        if (level > cap) continue;
        Case c; c.level = level; c.cap = cap; c.alive = alive; c.health = alive ? 17 : 0; c.amount = amount; run(c);
    }
    for (uint32 level : {1u, 8u}) for (uint32 xp : {0xfffffffcu, 0xffffffffu}) for (uint32 amount : {1u, 8u, 0xffffffffu}) {
        Case c; c.level = level; c.xp = xp; c.amount = amount; run(c);
    }
    for (uint32 profile : {2u, 3u}) for (uint32 level = 1; level <= 9; ++level) {
        for (uint32 target = level; target <= 9; ++target) for (bool alive : {false, true}) {
            Case c; c.kind = 'L'; c.profile = profile; c.level = level; c.xp = 23; c.amount = target; c.alive = alive; c.health = alive ? 17 : 0; run(c);
        }
        if (level == 1 || level == 4 || level == 8) for (uint32 amount : {0u, 100 + 37 * level, 5000u}) {
            Case c; c.profile = profile; c.level = level; c.amount = amount; run(c);
        }
    }
    for (uint32 level : {1u, 4u, 8u}) for (char kind : {'L', 'X'}) {
        Case c; c.kind = kind; c.profile = 4; c.level = level; c.amount = kind == 'L' ? level + 1 : 100 + 37 * level; run(c);
    }
    for (uint32 cap : {60u, 61u, 100u}) for (char kind : {'L', 'X', 'S'}) {
        Case c; c.kind = kind; c.cap = cap; c.amount = kind == 'L' ? 2 : 1; run(c);
    }
    for (uint32 rage : {0u, 999u, 1000u, 1501u}) for (uint32 energy : {0u, 17u, 100u}) {
        Case c; c.kind = 'L'; c.amount = 2; c.rage = rage; c.energy = energy; run(c);
    }
}
