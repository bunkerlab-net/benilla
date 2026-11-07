// SPDX-License-Identifier: GPL-3.0-or-later
// Authored, deliberately bounded host. PINNED blocks remain verbatim source.
// Full unsupported branches are retained, but never admitted by this host.
#include <algorithm>
#include <array>
#include <cassert>
#include <cfenv>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <limits>
#include <set>
#include <stdexcept>
#include <string>
#include <tuple>
#include <utility>
#include <vector>

using uint8 = uint8_t;
using uint32 = uint32_t;
using uint64 = uint64_t;
using int32 = int32_t;
// PINNED_ATTRIBUTES
// PINNED_ATTRIBUTES_EX3
// PINNED_RESULTS
// PINNED_FACING
// PINNED_SPELL_MOD_OP
// PINNED_MOD_TYPES
// PINNED_MOD_PAIR
// PINNED_TIME_CONSTANTS
// PINNED_DISTANCE_ENUM
// PINNED_RANGE_FLAGS
// PINNED_ATTACK_TYPE
// PINNED_TARGET_FLAGS
// PINNED_TARGET_ENUM
// PINNED_TYPE_ID
// PINNED_OBJECT_FIELDS
// PINNED_UNIT_FIELDS
// PINNED_MELEE_LEEWAY
// PINNED_RANGE_ENTRY

[[noreturn]] void outside(const std::string& reason)
{
    throw std::runtime_error("outside bounded source host: " + reason);
}

// Only fields observed by the extracted functions; authored values, not DBC rows.
struct SpellEntry
{
    uint32 Id = 133, Attributes = 0, AttributesEx3 = 0;
    uint32 rangeIndex = 1, Targets = TARGET_FLAG_UNIT, SpellFamilyName = 3;
    // PINNED_ATTRIBUTE_GETTER
    // PINNED_ATTRIBUTE_EX3_GETTER
};
SpellEntry spellEntry;
struct SpellTemplate
{
    template<class T> const T* LookupEntry(uint32 id) const
    {
        if (id != spellEntry.Id) outside("unknown spell template");
        return &spellEntry;
    }
} sSpellTemplate;
struct RangeStore
{
    SpellRangeEntry row = {1, 0.0f, 30.0f, 0};
    bool present = true;
    const SpellRangeEntry* LookupEntry(uint32 id) const
    {
        if (id != row.ID) outside("unknown range index");
        return present ? &row : nullptr;
    }
} sSpellRangeStore;
// PINNED_GET_MIN
// PINNED_GET_MAX
// PINNED_NEXT_SWING

struct Position { float x = 0.0f, y = 0.0f, z = 0.0f; };
struct WorldObject
{
    Position m_position;
    uint8 m_objectTypeId = TYPEID_UNIT;
    virtual ~WorldObject() = default;
    // PINNED_GET_X
    // PINNED_GET_Y
    // PINNED_GET_Z
    // PINNED_GET_TYPE
    // PINNED_IS_PLAYER
    // PINNED_BASE_REACH
    float GetDistance(const WorldObject*, bool, DistanceCalculation) const;
    float GetDistance(float, float, float, DistanceCalculation) const { outside("location distance"); }
    float GetObjectBoundingRadius() const { outside("bounding-radius distance"); }
    float GetCombinedCombatReach(const WorldObject*, bool, float = 0.0f) const { outside("melee reach"); }
    bool HasInArc(const WorldObject*) const { outside("facing check"); }
    bool IsWithinLOS(float, float, float) const { outside("location LOS"); }
};
struct Player;
struct Unit : WorldObject
{
    std::array<float, UNIT_END> m_floatValues{};
    // PINNED_UNIT_REACH
    // Both actors move forward without walking: ordinary NPC targets still get no leeway.
    bool IsMovingForward() const { return true; }
    bool IsWalking() const { return false; }
    virtual Player* GetSpellModOwner() const { outside("nonplayer caster"); }
    Unit* GetPet() const { outside("pet target"); }
    Unit* GetCharm() const { outside("charm target"); }
};
struct ItemPrototype { float RangedModRange; };
struct Item { const ItemPrototype* GetProto() const { outside("ranged weapon prototype"); } };
// Never instantiated for ordinary probes; required only to compile excluded loop bodies.
struct SpellModifier
{
    SpellModOp op = SPELLMOD_RANGE;
    SpellModType type = SPELLMOD_FLAT;
    int32 value = 0, charges = 0;
    bool isFinite = false;
    uint32 spellId = 0;
    uint64 modId = 0;
};
struct Player : Unit
{
    std::array<std::vector<SpellModifier*>, MAX_SPELLMOD> m_spellMods{};
    std::set<SpellModifierPair>* m_consumedMods = nullptr;
    Player() { m_objectTypeId = TYPEID_PLAYER; }
    // PINNED_MOD_OWNER
    uint32 GetSpellClass() const { return spellEntry.SpellFamilyName; }
    bool IsAffectedBySpellmod(const SpellEntry*, SpellModifier*, std::set<SpellModifierPair>*) const { outside("nonempty modifiers"); }
    void RemoveAuraCharge(uint32) { outside("finite modifier consumption"); }
    Item* GetWeaponForAttack(WeaponAttackType) const { outside("ranged slot"); }
    template<class T> void ApplySpellMod(uint32, SpellModOp, T&, bool = true);
};
// PINNED_APPLY_MOD
struct Targets
{
    Unit* unit = nullptr;
    WorldObject* corpse = nullptr;
    uint32 m_targetMask = TARGET_FLAG_UNIT;
    Position m_destPos, m_srcPos;
    Unit* getUnitTarget() const { return unit; }
    WorldObject* getCorpseTarget() const { return corpse; }
    WorldObject* getGOTarget() const { outside("gameobject target"); }
};
struct SpellManager
{
    uint32 facing = 0;
    uint32 GetSpellFacingFlag(uint32 id) const
    {
        if (id != spellEntry.Id) outside("unknown facing metadata");
        return facing;
    }
} sSpellMgr;
bool HasSpellTarget(const SpellEntry*, SpellTarget) { outside("implicit pet target"); }
bool IsIgnoreLosSpell(const SpellEntry*) { outside("location LOS attribute"); }
struct Spell
{
    const SpellEntry* m_spellInfo = &spellEntry;
    WorldObject* m_trueCaster;
    Unit* m_caster;
    Targets m_targets;
    uint32 m_casttime = 1500;
    bool m_clientCast = true;
    float m_maxRange;
    Spell(Player& caster, Unit& target, float previous)
        : m_trueCaster(&caster), m_caster(&caster), m_maxRange(previous)
    {
        m_targets.unit = &target;
    }
    std::pair<float, float> GetMinMaxRange(bool);
    SpellCastResult CheckRange(bool);
};
// PINNED_GET_DISTANCE
// PINNED_GET_MIN_MAX_RANGE
// PINNED_CHECK_RANGE

float number(uint32 bits)
{
    float value;
    std::memcpy(&value, &bits, sizeof(value));
    return value;
}
uint32 bits(float value)
{
    uint32 result;
    std::memcpy(&result, &value, sizeof(result));
    return result;
}
void host_domain(const Spell& spell, const Player& caster)
{
    if (!sSpellRangeStore.present) outside("missing range row");
    if (sSpellRangeStore.row.Flags != 0) outside("melee/ranged flags");
    if (spellEntry.Attributes != 0 || spellEntry.AttributesEx3 != 0) outside("spell attributes");
    if (spell.m_casttime == 0) outside("instant release shortcut");
    if (!spell.m_targets.unit || spell.m_targets.unit == &caster ||
        spell.m_targets.unit->GetTypeId() != TYPEID_UNIT) outside("ordinary NPC unit target required");
    if (spell.m_targets.corpse) outside("corpse target");
    if (spell.m_targets.m_targetMask != TARGET_FLAG_UNIT || spellEntry.Targets != TARGET_FLAG_UNIT)
        outside("non-unit target mask");
    if (sSpellMgr.facing != 0) outside("facing metadata");
    for (const auto& modifiers : caster.m_spellMods)
        if (!modifiers.empty()) outside("nonempty modifier hooks");
}

struct Probe
{
    std::string name;
    Position origin, target;
    float minimum = 0.0f, maximum = 30.0f, caster_reach = 1.5f, target_reach = 1.5f;
};
void float_field(std::ostream& out, float value)
{
    out << '\t' << std::hex << std::setw(8) << std::setfill('0') << bits(value) << std::dec;
}
void emit(std::ostream& out, const Probe& probe, bool strict)
{
    Player caster;
    Unit target;
    caster.m_position = probe.origin;
    target.m_position = probe.target;
    caster.m_floatValues[UNIT_FIELD_COMBATREACH] = probe.caster_reach;
    target.m_floatValues[UNIT_FIELD_COMBATREACH] = probe.target_reach;
    sSpellRangeStore.row.minRange = probe.minimum;
    sSpellRangeStore.row.maxRange = probe.maximum;
    const float previous = number(0x42f6e979);
    Spell spell(caster, target, previous);
    host_domain(spell, caster);
    const auto range = spell.GetMinMaxRange(strict);
    const float distance = caster.GetDistance(&target, true, DIST_CALC_NONE);
    const auto result = spell.CheckRange(strict);
    const float saved = spell.m_maxRange;

    // Observe the local effective maximum without editing CheckRange or duplicating tolerance.
    // Same row/reaches/host, but the second source call sits exactly at the source minimum.
    caster.m_position = {};
    target.m_position = {range.first, 0.0f, 0.0f};
    Spell measurement(caster, target, previous);
    if (measurement.CheckRange(strict) != SPELL_CAST_OK)
        outside("maximum observation probe unexpectedly failed");
    out << probe.name << '\t' << strict;
    for (float value : {probe.origin.x, probe.origin.y, probe.origin.z,
                        probe.target.x, probe.target.y, probe.target.z,
                        probe.minimum, probe.maximum, probe.caster_reach, probe.target_reach,
                        range.first, measurement.m_maxRange, distance})
        float_field(out, value);
    out << '\t' << static_cast<uint32>(result);
    float_field(out, previous);
    float_field(out, saved);
    out << '\n';
}
void emit_pair(std::ostream& out, const Probe& probe)
{
    emit(out, probe, true);
    emit(out, probe, false);
}
void header(std::ostream& out, const std::string& domain)
{
    out << "# Pinned CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f; authored binary32 probes; " << domain << '\n';
    out << "# name\tstrict\torigin_x\torigin_y\torigin_z\ttarget_x\ttarget_y\ttarget_z\trow_min\trow_max\tcaster_reach\ttarget_reach\texpected_min\texpected_max\texpected_distance_squared\tresult\tm_max_range_before\tm_max_range_after\n";
}
void verify_float_environment()
{
    static_assert(sizeof(float) == 4 && std::numeric_limits<float>::is_iec559);
    if (std::fesetround(FE_TONEAREST) != 0 || std::fegetround() != FE_TONEAREST)
        outside("round-to-nearest ties-to-even unavailable");
    volatile float normal = number(0x00800000), half = 0.5f, tiny = number(1);
    volatile float one = 1.0f, half_ulp = number(0x33800000), odd = number(0x3f800001);
    if (bits(normal * half) != 0x00400000 || bits(tiny + tiny) != 2 ||
        bits(tiny * half) != 0 || bits(one + half_ulp) != 0x3f800000 ||
        bits(odd + half_ulp) != 0x3f800002)
        outside("binary32 RNE/gradual-underflow environment unavailable");
}
void unsupported(const std::string& mode)
{
    Player caster;
    Unit target;
    SpellModifier modifier;
    Spell spell(caster, target, 0.0f);
    if (mode == "melee") sSpellRangeStore.row.Flags = SPELL_RANGE_FLAG_MELEE;
    else if (mode == "ranged") sSpellRangeStore.row.Flags = SPELL_RANGE_FLAG_RANGED;
    else if (mode == "next_swing") spellEntry.Attributes = SPELL_ATTR_ON_NEXT_SWING;
    else if (mode == "ranged_slot") spellEntry.Attributes = SPELL_ATTR_USES_RANGED_SLOT;
    else if (mode == "instant") spell.m_casttime = 0;
    else if (mode == "player_target") target.m_objectTypeId = TYPEID_PLAYER;
    else if (mode == "self_target") spell.m_targets.unit = &caster;
    else if (mode == "corpse") spell.m_targets.corpse = &target;
    else if (mode == "gameobject") spell.m_targets.m_targetMask = TARGET_FLAG_GAMEOBJECT;
    else if (mode == "destination") spell.m_targets.m_targetMask = TARGET_FLAG_DEST_LOCATION;
    else if (mode == "source") spell.m_targets.m_targetMask = TARGET_FLAG_SOURCE_LOCATION;
    else if (mode == "facing") sSpellMgr.facing = SPELL_FACING_FLAG_INFRONT;
    else if (mode == "modifier") caster.m_spellMods[SPELLMOD_RANGE].push_back(&modifier);
    else if (mode == "missing_row") sSpellRangeStore.present = false;
    else outside("unknown unsupported probe");
    host_domain(spell, caster);
    throw std::runtime_error("unsupported probe accidentally admitted");
}

int main(int argc, char** argv)
{
    try
    {
        verify_float_environment();
        if (argc == 3 && std::string(argv[1]) == "--unsupported") unsupported(argv[2]);
        if (argc != 2) throw std::runtime_error("expected output directory");
        std::ofstream out(std::string(argv[1]) + "/spell-range.tsv");
        std::ofstream excluded(std::string(argv[1]) + "/spell-range-excluded.tsv");
        if (!out || !excluded) throw std::runtime_error("cannot open fixture output");
        header(out, "admitted raw helper bounds (typed runtime inputs are narrower)");
        header(excluded, "EXCLUDED numeric observations; no runtime parity admission");

        for (float minimum : {0.0f, 5.0f})
            for (float maximum : {10.0f, 27.0f, 30.0f, 100.0f})
                for (float reach : {0.0f, 1.5f, 3.75f})
                {
                    Probe p;
                    p.minimum = minimum; p.maximum = maximum;
                    p.caster_reach = reach; p.target_reach = 1.5f;
                    // Author edge coordinates from source outputs; check results remain source-produced.
                    Player caster; Unit target;
                    caster.m_floatValues[UNIT_FIELD_COMBATREACH] = p.caster_reach;
                    target.m_floatValues[UNIT_FIELD_COMBATREACH] = p.target_reach;
                    sSpellRangeStore.row.minRange = minimum; sSpellRangeStore.row.maxRange = maximum;
                    Spell source(caster, target, 0.0f);
                    const auto range = source.GetMinMaxRange(true);
                    target.m_position = {range.first, 0.0f, 0.0f};
                    if (source.CheckRange(false) != SPELL_CAST_OK) outside("edge preparation");
                    const std::array<float, 3> boundaries = {range.first, range.second, source.m_maxRange};
                    for (unsigned boundary = 0; boundary != boundaries.size(); ++boundary)
                        for (int neighbor = -1; neighbor <= 1; ++neighbor)
                            for (unsigned axis = 0; axis != 3; ++axis)
                            {
                                float edge = boundaries[boundary];
                                if (neighbor < 0) edge = std::nextafter(edge, -std::numeric_limits<float>::infinity());
                                if (neighbor > 0) edge = std::nextafter(edge, std::numeric_limits<float>::infinity());
                                p.target = {};
                                if (axis == 0) p.target.x = edge;
                                if (axis == 1) p.target.y = edge;
                                if (axis == 2) p.target.z = edge;
                                p.name = "boundary_" + std::to_string(bits(minimum)) + "_" + std::to_string(bits(maximum)) +
                                    "_" + std::to_string(bits(reach)) + "_" + std::to_string(boundary) +
                                    "_" + std::to_string(neighbor) + "_" + std::to_string(axis);
                                emit_pair(out, p);
                            }
                }
        emit_pair(out, {"sqrt_rounds_back_to_max", {}, {32.0f, 0.0f, 0.01f}, 0.0f, 32.0f, 0.0f, 0.0f});
        emit_pair(out, {"reach_sum_before_range", {}, {1.0f, 0.0f, 0.0f}, 1.0f, 1.0f, number(0x33800000), number(0x33800000)});
        emit_pair(out, {"3d_pythagorean_edge", {}, {3.0f, 4.0f, 12.0f}, 0.0f, 13.0f, 0.0f, 0.0f});
        emit_pair(out, {"signed_zero", {-0.0f, 0.0f, -0.0f}, {0.0f, -0.0f, 0.0f}, -0.0f, 0.0f, -0.0f, 0.0f});
        emit_pair(out, {"coordinate_bounds", {32760.0f, -32760.0f, 32760.0f}, {-32760.0f, 32760.0f, -32760.0f}, 0.0f, 1.0e6f, 0.0f, 0.0f});
        emit_pair(out, {"range_reach_bounds", {}, {32760.0f, 0.0f, 0.0f}, 1.0e6f, 1.0e6f, 1.0e12f, 1.0e12f});
        for (uint32 value : {1u, 2u, 0x007fffffu, 0x00800000u, 0x00800001u,
                             0x1a000000u, 0x1a3504f3u, 0x1a3504f4u, 0x1e3ce508u, 0x20000000u})
        {
            const float v = number(value);
            const std::string name = std::to_string(value);
            emit_pair(out, {"underflow_squared_" + name, {}, {v, v, v}, 0.0f, 1.0f, 0.0f, 0.0f});
            emit_pair(out, {"underflow_min_" + name, {}, {}, v, v, 0.0f, 0.0f});
            emit_pair(out, {"underflow_reach_" + name, {}, {}, 0.0f, 0.0f, v, v});
            emit_pair(out, {"underflow_tolerance_" + name, {}, {v, 0.0f, 0.0f}, 0.0f, v, 0.0f, 0.0f});
        }
        for (uint32 value : {0x41efffffu, 0x41f00000u, 0x41f00001u})
            emit_pair(out, {"tolerance_cap_" + std::to_string(value), {}, {33.0f, 0.0f, 0.0f}, 0.0f, number(value), 0.0f, 0.0f});
        // Deterministic, independently authored triples; assert every distance bit, not just classification.
        uint32 state = 0x5eed133u;
        auto next = [&state]() { state ^= state << 13; state ^= state >> 17; state ^= state << 5; return state; };
        for (unsigned i = 0; i != 512; ++i)
        {
            auto coordinate = [&next]() { return number((next() & 0x807fffffu) | ((100u + next() % 42u) << 23)); };
            Probe p;
            p.name = "rounding_triple_" + std::to_string(i);
            p.origin = {coordinate(), coordinate(), coordinate()};
            p.target = {coordinate(), coordinate(), coordinate()};
            p.minimum = i % 4 == 0 ? number(0x00800000u + (next() & 0x007fffffu)) : 0.0f;
            p.maximum = number((next() & 0x007fffffu) | ((126u + next() % 15u) << 23));
            p.caster_reach = number((next() & 0x007fffffu) | ((100u + next() % 31u) << 23));
            p.target_reach = number((next() & 0x007fffffu) | ((100u + next() % 31u) << 23));
            emit_pair(out, p);
        }
        // Actual source IEEE outcomes outside the admitted bounds, never Rust parity fixtures.
        const float largest = number(0x7f7fffff), inf = number(0x7f800000);
        emit_pair(excluded, {"overflow_coordinate_subtraction", {largest, 0.0f, 0.0f}, {-largest, 0.0f, 0.0f}, 0.0f, 30.0f, 0.0f, 0.0f});
        emit_pair(excluded, {"overflow_distance_square", {}, {1.0e20f, 0.0f, 0.0f}, 0.0f, 30.0f, 0.0f, 0.0f});
        emit_pair(excluded, {"overflow_reach_sum", {}, {1.0e20f, 0.0f, 0.0f}, 0.0f, 30.0f, largest, largest});
        emit_pair(excluded, {"overflow_range_square", {}, {1.0e20f, 0.0f, 0.0f}, 0.0f, 1.0e20f, 0.0f, 0.0f});
        emit_pair(excluded, {"nonfinite_coordinate", {inf, 0.0f, 0.0f}, {inf, 0.0f, 0.0f}, 0.0f, 30.0f, 0.0f, 0.0f});
        emit_pair(excluded, {"overbound_coordinate_finite", {}, {32761.0f, 0.0f, 0.0f}, 0.0f, 30.0f, 0.0f, 0.0f});
        out.close(); excluded.close();
        if (!out || !excluded) throw std::runtime_error("fixture output write failed");
    }
    catch (const std::exception& error)
    {
        std::cerr << error.what() << '\n';
        return 2;
    }
}
