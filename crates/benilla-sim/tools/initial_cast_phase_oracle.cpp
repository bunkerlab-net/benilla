// SPDX-License-Identifier: GPL-3.0-or-later
// Authored adapters surround unchanged, hash-locked CMaNGOS source. See initial_cast_phase_source/README.md.
#include <array>
#include <cassert>
#include <cfenv>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <limits>
#include <sstream>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>
using uint8 = uint8_t;
using uint32 = uint32_t;
using int32 = int32_t;
using SpellEffectIndex = uint32;
using ObjectGuid = uint32;
constexpr int MAX_EFFECT_INDEX = 3;
// PINNED_TargetDefines
// PINNED_EffectDefines
struct SpellTargetInfo {
    char const* name;
    SpellTargetImplicitType type;
    SpellTargetFilter filter;
    SpellTargetEnumerator enumerator;
    SpellTargetLOS los;
    SpellTargetInfo(char const* name = "", SpellTargetImplicitType type = TARGET_TYPE_UNKNOWN,
        SpellTargetFilter filter = TARGET_NEUTRAL, SpellTargetEnumerator enumerator = TARGET_ENUMERATOR_UNKNOWN,
        SpellTargetLOS los = TARGET_LOS_CASTER);
};
// PINNED_TargetTables
struct SpellTargetingData {
    SpellTargetImplicitType implicitType[3]{};
    uint8 targetMask[3][2]{{1, 1}, {2, 2}, {4, 4}};
    std::pair<bool, bool> ignoredTargets[3]{};
};
struct SpellTargetMgr {
    static SpellTargetingData data;
    static SpellTargetingData& GetSpellTargetingData(uint32) { return data; }
    static bool CanEffectBeFilledWithMask(uint32, uint32, uint32) { throw std::runtime_error("entry target mask unsupported"); }
};
SpellTargetingData SpellTargetMgr::data;
constexpr int TYPEID_PLAYER = 1, SPELLMOD_ALL_EFFECTS = 0, SPELLMOD_EFFECT_PAST_FIRST = 1;
constexpr int SPELL_ATTR_SCALES_WITH_CREATURE_LEVEL = 0;
constexpr int SPELL_ATTR_EX2_FAIL_ON_ALL_TARGETS_IMMUNE = 1, SPELL_ATTR_EX_REQUIRE_ALL_TARGETS = 2;
constexpr int ITEM_CLASS_WEAPON = 2, ITEM_CLASS_ARMOR = 4;
constexpr int SPELL_MISS_NONE = 0, SPELL_MISS_IMMUNE = 1, SPELL_MISS_IMMUNE2 = 2, SPELL_FAILED_IMMUNE = 3;
constexpr uint32 PROC_EX_NONE = 0, PROC_FLAG_NONE = 0, PROC_FLAG_ON_TRAP_ACTIVATION = 1;
enum { SPELL_AURA_PERIODIC_DAMAGE, SPELL_AURA_PERIODIC_LEECH, SPELL_AURA_SCHOOL_ABSORB,
       SPELL_AURA_POWER_BURN_MANA, SPELL_AURA_PERIODIC_MANA_LEECH };
#define DEBUG_FILTER_LOG(...) ((void)0)
struct Log { template<typename... A> void outError(A...) { throw std::runtime_error("source logged error"); } } sLog;
static uint32 spell_index = 0, active_slot = 0, rank_queries = 0;
static std::vector<int32> supplied_rolls;
static size_t next_roll = 0;
static std::vector<std::string> events;
void event(const char* kind, int slot = -1, const std::string& fields = "") {
    std::ostringstream out;
    out << "{\"kind\":\"" << kind << "\",\"spell_index\":" << spell_index;
    if (slot >= 0) out << ",\"slot\":" << slot;
    out << fields << '}';
    events.push_back(out.str());
}
std::string number(const char* key, int64_t value) { return std::string(",\"") + key + "\":" + std::to_string(value); }
std::string target_field(bool self) { return std::string(",\"target\":\"") + (self ? "self" : "targetless") + '"'; }
int32 irand(int32 low, int32 high) {
    if (next_roll >= supplied_rolls.size()) throw std::runtime_error("missing semantic irand outcome");
    int32 result = supplied_rolls[next_roll++];
    if (result < low || result > high) throw std::runtime_error("semantic irand outcome outside source bounds");
    event("draw", int(active_slot), number("low", low) + number("high", high) + number("value", result));
    return result;
}
uint32 urand(uint32, uint32) { throw std::runtime_error("unexpected urand"); }
struct SpellEntry {
    uint32 Id = 0, Targets = 0, Effect[3]{}, EffectImplicitTargetA[3]{}, EffectImplicitTargetB[3]{};
    uint32 EffectChainTarget[3]{}, EffectApplyAuraName[3]{}, EffectBaseDice[3]{};
    uint32 maxLevel = 0, baseLevel = 0, spellLevel = 0;
    int32 EffectMiscValue[3]{}, EffectBasePoints[3]{}, EffectDieSides[3]{};
    float EffectRealPointsPerLevel[3]{}, EffectDicePerLevel[3]{}, EffectPointsPerComboPoint[3]{};
    float DmgMultiplier[3]{1, 1, 1};
    int32 EquippedItemClass = 0;
    uint32 EquippedItemSubClassMask = 0, authored_rank = 0;
    bool HasAttribute(int) const { return false; }
// PINNED_SimpleValue
};
struct Unit;
struct Player;
struct SpellAuraHolder;
struct Aura;
struct WorldObject {
    virtual ~WorldObject() = default;
    int GetTypeId() const { return TYPEID_PLAYER; }
    ObjectGuid GetObjectGuid() const { return 1; }
    int32 CalculateSpellEffectValue(Unit const*, SpellEntry const*, SpellEffectIndex, int32 const*, bool, bool) const;
};
struct ProcSystemArguments { template<typename... A> explicit ProcSystemArguments(A...) {} };
struct Unit : WorldObject {
    uint32 GetSpellRank(SpellEntry const* entry) const { ++rank_queries; return entry->authored_rank; }
    uint32 GetLevel() const { return 1; }
    Player* GetSpellModOwner() const { return nullptr; }
    bool IsCreature() const { return false; }
    bool IsPlayer() const { return true; }
    bool IsAlive() const { return true; }
    static void ProcDamageAndSpell(ProcSystemArguments) {}
    void RegisterAuras(SpellAuraHolder* holder);
    void AddAuraToModList(Aura*);
};
struct Session { bool PlayerLoading() const { throw std::runtime_error("unexpected ghost/loading branch"); } };
struct Player : Unit {
    uint32 weapon_mask = 0, armor_mask = 0;
    uint8 GetComboPoints() const { return 0; }
    uint32 GetComboTargetGuid() const { return 0; }
    template<typename... A> void ApplySpellMod(A...) { throw std::runtime_error("unexpected spell mod"); }
    Session* GetSession() { throw std::runtime_error("unexpected player session"); }
    uint32 GetWeaponProficiency() const { return weapon_mask; }
    uint32 GetArmorProficiency() const { return armor_mask; }
    void AddWeaponProficiency(uint32 mask) { weapon_mask |= mask; }
    void AddArmorProficiency(uint32 mask) { armor_mask |= mask; }
    void SendProficiency(int, uint32) {}
};
struct CreatureInfo { uint32 UnitClass = 0; };
struct CreatureClassLvlStats { float BaseDamage = 0; };
struct Creature : Unit { const CreatureInfo* GetCreatureInfo() const { throw std::runtime_error("unexpected creature"); } };
struct ObjectMgr {
    const CreatureClassLvlStats* GetCreatureClassLvlStats(uint32, uint32) const { throw std::runtime_error("unexpected creature stats"); }
} sObjectMgr;
bool IsOnlySelfTargeting(SpellEntry const*) { throw std::runtime_error("unexpected combo scaling"); }
bool IsDeathOnlySpell(SpellEntry const*) { throw std::runtime_error("unexpected death branch"); }
bool IsDeathPersistentSpell(SpellEntry const*) { throw std::runtime_error("unexpected death branch"); }
struct Item {};
struct GameObject {};
struct Corpse {};
using UnitList = std::vector<Unit*>;
using GameObjectList = std::vector<GameObject*>;
using CorpseList = std::vector<Corpse*>;
struct TempTargetingData {
    struct Slot {
        UnitList tmpUnitList[2];
        GameObjectList tmpGOList[2];
        CorpseList tempCorpseList;
        std::vector<Item*> tempItemList;
    } data[3];
    uint32 chainTargetCount[3]{};
};
struct Aura {
    uint32 slot;
    int32 amount;
    void ApplyModifier(bool apply, bool real) {
        if (!apply || !real) throw std::runtime_error("unexpected aura removal");
        event("aura_apply", int(slot));
    }
};
struct SpellAuraHolder {
    std::array<Aura*, 3> m_auras{};
    ~SpellAuraHolder() { for (Aura* aura : m_auras) delete aura; }
    bool IsDeleted() const { return false; }
    Aura* GetAuraByEffectIndex(SpellEffectIndex slot) const { return m_auras[slot]; }
    void AddAura(Aura*, SpellEffectIndex);
    void ApplyAuraModifiers(bool, bool);
    void setDiminishGroup(int) {}
};
Aura* CreateAura(SpellEntry const*, SpellEffectIndex slot, int32* value, int32*, SpellAuraHolder*, Unit*, Unit*, Item*, int) {
    event("aura_construct", int(slot), number("value", *value));
    return new Aura{slot, *value};
}
SpellAuraHolder* CreateSpellAuraHolder(SpellEntry const*, Unit*, WorldObject*, Item*, SpellEntry const*) {
    event("holder_create");
    return new SpellAuraHolder;
}
void Unit::AddAuraToModList(Aura* aura) { event("aura_register", int(aura->slot)); }
void Unit::RegisterAuras(SpellAuraHolder* holder) {
// PINNED_RegisterAuras
}
// PINNED_AuraMethods
struct TargetInfo {
    ObjectGuid targetGUID = 1;
    uint8 effectHitMask = 0;
    int missCondition = SPELL_MISS_NONE, effectDuration = 0, diminishGroup = 0;
};
struct Spell;
using pEffect = void (Spell::*)(SpellEffectIndex);
extern pEffect SpellEffects[MAX_SPELL_EFFECTS];
struct Spell {
    SpellEntry const* m_spellInfo;
    Player* m_caster;
    WorldObject* m_trueCaster;
    Unit* unitTarget = nullptr;
    Item* itemTarget = nullptr;
    GameObject* gameObjTarget = nullptr;
    Item* m_CastItem = nullptr;
    SpellEntry const* m_triggeredBySpellInfo = nullptr;
    SpellAuraHolder* m_spellAuraHolder = nullptr;
    int32 m_currentBasePoints[3]{}, m_effectTriggerChance[3]{-1, -1, -1};
    int32 m_healingPerEffect[3]{}, m_damagePerEffect[3]{}, damagePerEffect[3]{};
    int32 damage = 0, m_healing = 0, m_damage = 0, m_duration = 0;
    uint32 m_targetlessMask = 0, m_affectedTargetCount = 0, m_applyMultiplierMask = 0;
    float m_damageMultipliers[3]{1, 1, 1};
    int m_attackType = 0;
    SpellTargetFilterScheme m_filteringScheme[3][2]{};
    struct { uint32 effectMask = 0, effectMaskProcessed = 0; } m_destTargetInfo;
    struct {
        Unit* getUnitTarget() { return nullptr; }
        ObjectGuid getUnitTargetGuid() { return 0; }
    } m_targets;
    std::vector<TargetInfo> m_UniqueTargetInfo;
    std::vector<int> m_UniqueItemInfo;
    Spell(SpellEntry const* entry, Player* player) : m_spellInfo(entry), m_caster(player), m_trueCaster(player) {
        for (uint32 i = 0; i < MAX_EFFECT_INDEX; ++i) {
            // Guard only: do not execute undefined original signed cache arithmetic.
            int64_t cached = int64_t(entry->EffectBasePoints[i]) + int64_t(entry->EffectBaseDice[i]);
            if (entry->EffectBaseDice[i] > uint32(INT32_MAX) || cached < INT32_MIN || cached > INT32_MAX)
                throw std::runtime_error("source cache arithmetic outside defined signed-i32 domain");
// PINNED_CacheValue
        }
    }
    void FillTargetMap();
    void DoAllTargetlessEffects(bool);
    void ExecuteEffects(Unit*, Item*, GameObject*, uint32);
    void HandleEffect(Unit*, Item*, GameObject*, SpellEffectIndex, float = 1.f);
    bool IsEffectWithImplementedMultiplier(uint32) const;
    void EffectEmpty(SpellEffectIndex);
    void EffectProficiency(SpellEffectIndex);
    void EffectApplyAura(SpellEffectIndex);
// GENERATED_UNREACHED_HANDLERS
    int32 CalculateSpellEffectValue(SpellEffectIndex slot, Unit* target) {
        active_slot = slot;
        return m_trueCaster->CalculateSpellEffectValue(target, m_spellInfo, slot, &m_currentBasePoints[slot], false, true);
    }
    void OnEffectExecute(SpellEffectIndex slot) {
        event("amount", int(slot), number("value", damage) + target_field(unitTarget != nullptr));
        // The unchanged HandleEffect immediately stores damage and invokes the original table.
        event("dispatch", int(slot), number("value", m_spellInfo->Effect[slot]) + target_field(unitTarget != nullptr));
    }
    void SetTargetMap(SpellEffectIndex slot, uint32 target, bool right, TempTargetingData& data) {
        if (target != TARGET_UNIT_CASTER) throw std::runtime_error("non-self target adapter reached");
        data.data[slot].tmpUnitList[right].push_back(m_caster);
    }
    bool FillUnitTargets(TempTargetingData& data, SpellTargetingData& targeting, uint32 slot) {
        // Synthetic singleton-target boundary. Dedupe/CheckTarget/immunity/hit/duration are not proved here.
        for (uint32 right = 0; right < 2; ++right)
            for (Unit* unit : data.data[slot].tmpUnitList[right]) {
                if (unit != m_caster) throw std::runtime_error("non-self unit adapter reached");
                if (m_UniqueTargetInfo.empty()) m_UniqueTargetInfo.push_back(TargetInfo{});
                m_UniqueTargetInfo[0].effectHitMask |= targeting.targetMask[slot][right];
            }
        return true;
    }
    void FillFromTargetFlags(TempTargetingData&, SpellEffectIndex) { throw std::runtime_error("target flags unsupported"); }
    void OnDestTarget() { throw std::runtime_error("destination unsupported"); }
    void AddDestExecution(SpellEffectIndex) { throw std::runtime_error("destination execution unsupported"); }
    void AddCorpseTarget(Corpse*, uint8) { throw std::runtime_error("corpse unsupported"); }
    void AddItemTarget(Item*, uint8) { throw std::runtime_error("item unsupported"); }
    void AddGOTarget(GameObject*, uint8) { throw std::runtime_error("gameobject unsupported"); }
    bool OnCheckTarget(GameObject*, SpellEffectIndex) { throw std::runtime_error("gameobject check unsupported"); }
    void SendCastResult(int) { throw std::runtime_error("unexpected cast failure"); }
    void finish(bool) { throw std::runtime_error("unexpected finish callback"); }
    void OnHit(int) {}
    template<typename... A> void PrepareMasksForProcSystem(uint32, uint32& attacker, uint32& victim, A...) { attacker = victim = 0; }
    Unit* GetAffectiveCaster() { return m_caster; }
    int GetScriptValue() { return 0; }
    bool IsDelayedSpell() const { return false; }
    void TakeCastItem() {}
    void HandleImmediateEffectExecution(TargetInfo*) {}
    void DoAllEffectOnTarget(int*) { throw std::runtime_error("item execution unsupported"); }
    void DoAllEffectOnTarget(TargetInfo* target) {
        Unit* unit = m_caster;
        uint32 effectMask = target->effectHitMask;
        bool aura = false;
        for (uint32 slot = 0; slot < 3; ++slot) {
            if (!(effectMask & (1 << slot))) continue;
            if (m_spellInfo->Effect[slot] != SPELL_EFFECT_APPLY_AURA)
                throw std::runtime_error("non-aura self effect unsupported");
            aura = true;
        }
        if (aura) {
// PINNED_HolderCreate
        if (m_spellAuraHolder) {
            unit->RegisterAuras(m_spellAuraHolder);
            delete m_spellAuraHolder;
            m_spellAuraHolder = nullptr;
        }
    }
    void RunImmediatePhases() {
// PINNED_ImmediatePhases
    }
};
bool IsEffectHandledImmediatelySpellLaunch(SpellEntry const*, SpellEffectIndex) { return false; }
// PINNED_EffectTable
// PINNED_FillTargetMap
// PINNED_Targetless
// PINNED_ExecuteEffects
// PINNED_HandleEffect
// PINNED_EffectHandlers
// PINNED_EffectAmount
float read_float_bits() {
    uint32 bits; std::cin >> bits;
    float value; std::memcpy(&value, &bits, sizeof(value));
    return value;
}
void classify(SpellEntry const* spellInfo) {
    auto& data = SpellTargetMgr::data;
    data = SpellTargetingData{};
    uint32 i = spellInfo->Id;
// PINNED_ClassifyEffects
}
int main() {
    static_assert(sizeof(float) == 4 && std::numeric_limits<float>::is_iec559, "requires IEEE binary32");
    assert(std::fesetround(FE_TONEAREST) == 0);
    try {
        uint32 case_count; std::cin >> case_count;
        for (uint32 case_index = 0; case_index < case_count; ++case_index) {
            uint32 spell_count, roll_count;
            Player player;
            std::cin >> spell_count >> roll_count >> player.weapon_mask >> player.armor_mask;
            supplied_rolls.resize(roll_count);
            for (auto& roll : supplied_rolls) std::cin >> roll;
            events.clear(); next_roll = 0; rank_queries = 0;
            std::vector<uint32> targetless_masks, self_masks;
            for (spell_index = 0; spell_index < spell_count; ++spell_index) {
                SpellEntry entry;
                std::cin >> entry.Id >> entry.EquippedItemClass >> entry.EquippedItemSubClassMask
                    >> entry.authored_rank >> entry.maxLevel >> entry.baseLevel >> entry.spellLevel;
                for (uint32 slot = 0; slot < 3; ++slot) {
                    std::cin >> entry.Effect[slot] >> entry.EffectApplyAuraName[slot] >> entry.EffectMiscValue[slot]
                        >> entry.EffectBasePoints[slot] >> entry.EffectBaseDice[slot] >> entry.EffectDieSides[slot]
                        >> entry.EffectImplicitTargetA[slot] >> entry.EffectImplicitTargetB[slot];
                    entry.EffectRealPointsPerLevel[slot] = read_float_bits();
                    entry.EffectDicePerLevel[slot] = read_float_bits();
                    entry.EffectPointsPerComboPoint[slot] = read_float_bits();
                    if (entry.Effect[slot] >= MAX_SPELL_EFFECTS || entry.EffectImplicitTargetA[slot] > 1 || entry.EffectImplicitTargetB[slot] > 1)
                        throw std::runtime_error("unsupported source adapter input");
                }
                if (!std::cin) throw std::runtime_error("malformed input");
                event("cast_start");
                classify(&entry);
                Spell spell(&entry, &player);
                spell.FillTargetMap();
                targetless_masks.push_back(spell.m_targetlessMask);
                self_masks.push_back(spell.m_UniqueTargetInfo.empty() ? 0 : spell.m_UniqueTargetInfo[0].effectHitMask);
                spell.RunImmediatePhases();
                event("cast_complete");
            }
            if (next_roll != supplied_rolls.size()) throw std::runtime_error("unused semantic irand outcome");
            std::cout << "{\"events\":[";
            for (size_t i = 0; i < events.size(); ++i) { if (i) std::cout << ','; std::cout << events[i]; }
            std::cout << "],\"final_weapon_mask\":" << player.weapon_mask << ",\"final_armor_mask\":" << player.armor_mask
                << ",\"rank_queries\":" << rank_queries << ",\"targetless_masks\":[";
            for (size_t i = 0; i < targetless_masks.size(); ++i) { if (i) std::cout << ','; std::cout << targetless_masks[i]; }
            std::cout << "],\"self_masks\":[";
            for (size_t i = 0; i < self_masks.size(); ++i) { if (i) std::cout << ','; std::cout << self_masks[i]; }
            std::cout << "]}\n";
        }
    } catch (std::exception const& error) { std::cerr << error.what() << '\n'; return 1; }
}
