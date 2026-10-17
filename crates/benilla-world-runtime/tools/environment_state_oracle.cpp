// SPDX-License-Identifier: GPL-3.0-or-later
// Authored reducer inputs only. Geometry and dependency bodies are NOT evaluated.
#include <array>
#include <cassert>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <limits>
#include <list>
#include <stdexcept>
#include <string>
#include <vector>
#include "Database/DBCStore.h"
#include "Maps/GridMapDefines.h"
// PINNED_DECLARATIONS
static DBCStorage<LiquidTypeEntry> sLiquidTypeStore(LiquidTypefmt);
static void require(bool value, const char* what) {
    if (!value) throw std::runtime_error(what);
}
static uint32 bits(float value) { uint32 word; std::memcpy(&word, &value, 4); return word; }
static float fromBits(uint32 word) { float value; std::memcpy(&value, &word, 4); return value; }
static void putFloat(std::ostream& out, float value) {
    out << '\t' << std::hex << std::setfill('0') << std::setw(8) << bits(value) << std::dec;
}
struct Audit {
    std::vector<std::string> calls;
    void add(const std::string& call) { calls.push_back(call); }
    std::string text() const {
        if (calls.empty()) return "-";
        std::string result;
        for (const auto& call : calls) { if (!result.empty()) result += ','; result += call; }
        return result;
    }
};
struct MirrorTimer {
    // PINNED_MIRROR_TYPES
    Type type;
    Audit* audit;
    // Intent only: no mirror duration, ticking, packets, damage or scale state.
    void SetScale(int32 value) {
        const char* name = type == FATIGUE ? "fatigue" : type == BREATH ? "breath" : "environmental";
        audit->add(std::string(name) + ':' + std::to_string(value));
    }
};
struct HostileRefManager {
    Audit* audit;
    void updateThreatTables() { audit->add("threat"); }
};
struct Facts {
    GridMapLiquidStatus status = LIQUID_MAP_IN_WATER;
    uint32 entry = 11, type_flags = MAP_LIQUID_TYPE_WATER;
    float level = 8, depth = 0, collision = 2;
    uint32 interval = 60000;
    bool has_aura = false;
};
struct TerrainInfo {
    Facts facts;
    unsigned queries = 0;
    // Authored bound liquid facts stand in for a successful geometry query.
    // The original environment reducer below is the unit under test.
    GridMapLiquidStatus getLiquidStatus(float, float, float, uint8 mask,
                                        GridMapLiquidData* out, float collision) {
        require(mask == MAP_ALL_LIQUIDS && bits(collision) == bits(facts.collision), "source query arguments");
        ++queries;
        if (facts.status) *out = {facts.type_flags, facts.entry, facts.level, facts.depth};
        return facts.status;
    }
};
struct Map { TerrainInfo terrain; TerrainInfo* GetTerrain() { return &terrain; } };
struct Player {
    Audit audit;
    uint8 m_environmentFlags = 0;
    uint32 m_positionStatusUpdateTimer = 0;
    LiquidTypeEntry const* m_lastLiquid = nullptr;
    Map map;
    HostileRefManager hostile{&audit};
    MirrorTimer m_mirrorTimers[MirrorTimer::NUM_TIMERS] = {
        {MirrorTimer::FATIGUE, &audit}, {MirrorTimer::BREATH, &audit},
        {MirrorTimer::FEIGNDEATH, &audit}, {MirrorTimer::ENVIRONMENTAL, &audit}};
    HostileRefManager& getHostileRefManager() { return hostile; }
    float GetCollisionHeight() const { return map.terrain.facts.collision; }
    // The interval is an explicit dependency input. The real config/multiplier
    // conversion and water-breathing aura lifecycle are outside this reducer.
    uint32 GetWaterBreathingInterval() {
        audit.add("interval:" + std::to_string(map.terrain.facts.interval));
        return map.terrain.facts.interval;
    }
    void RemoveAurasWithInterruptFlags(uint32 mask) { audit.add("interrupt:" + std::to_string(mask)); }
    void RemoveAurasDueToSpell(uint32 id) { audit.add("remove_spell:" + std::to_string(id)); }
    bool HasAura(uint32 id) {
        audit.add("has_aura:" + std::to_string(id) + ':' + std::to_string(map.terrain.facts.has_aura));
        return map.terrain.facts.has_aura;
    }
    void CastSpell(Player* target, uint32 id, uint32 flags) {
        require(target == this, "source self cast target");
        audit.add("cast_spell:" + std::to_string(id) + ':' + std::to_string(flags));
    }
    void SetEnvironmentFlags(EnvironmentFlags, bool);
    void UpdateTerainEnvironmentFlags(Map*, float, float, float);
    void UpdateEnvironmentTimer(uint32 diff) {
        // PINNED_UPDATE_TIMER
    }
    bool SetPositionEnvironment(Map* m, float x, float y, float z) {
        // PINNED_POSITION_GATE
        return true;
    }
};
// PINNED_ENVIRONMENT_METHODS

static void writeStores(const std::filesystem::path& root) {
    for (unsigned store = 0; store < 2; ++store) {
        const auto dir = root / "environment-state" / ("store" + std::to_string(store));
        std::filesystem::create_directories(dir);
        // Full authored rows, intentionally unsorted. Never copied from a game.
        std::vector<std::array<uint32,4>> rows{{15,953,1,101}, {12,935,0,202},
            {65536,959,3,404}, {14,947,1,0}, {11,923,3,101}, {13,941,2,303}};
        if (store == 0) rows.insert(rows.begin() + 2, {0,901,3,909});
        const std::array<uint32,5> header{0x43424457, uint32(rows.size()), 4, 16, 1};
        std::ofstream out(dir / "LiquidType.dbc.test-fixture", std::ios::binary);
        out.write(reinterpret_cast<const char*>(header.data()), sizeof(header));
        for (const auto& row : rows) out.write(reinterpret_cast<const char*>(row.data()), sizeof(row));
        out.put('\0');
        require(bool(out), "write authored LiquidType WDBC");
    }
}
static void loadStore(const std::filesystem::path& root, unsigned store) {
    sLiquidTypeStore.Clear();
    const auto file = root / "environment-state" / ("store" + std::to_string(store)) / "LiquidType.dbc.test-fixture";
    require(sLiquidTypeStore.Load(file.c_str()), "original LiquidType WDBC loader");
    require(sLiquidTypeStore.GetFieldCount() == 4 && sizeof(LiquidTypeEntry) == DBCFileLoader::GetFormatRecordSize(LiquidTypefmt), "original LiquidType packed ABI");
}
static void dbcObservations(const std::filesystem::path& root) {
    std::ofstream out(root / "environment-state-dbc.tsv");
    out << "# Original DBCStorage/DBCFileLoader lookup over complete authored WDBC rows\n"
        "# store\tlookup_id\tpresent\tid\tliquid_id\ttype\tspell_id\n";
    for (unsigned store = 0; store < 2; ++store) {
        loadStore(root, store);
        for (uint32 id : {0u, 1u, 11u, 12u, 13u, 14u, 15u, 77u, 65536u, 65537u}) {
            const auto* row = sLiquidTypeStore.LookupEntry(id);
            out << store << '\t' << id << '\t' << (row != nullptr);
            if (row) out << '\t' << row->Id << '\t' << row->LiquidId << '\t' << row->Type << '\t' << row->SpellId;
            else out << "\t-\t-\t-\t-";
            out << '\n';
        }
    }
}
static std::string lastEntry(const Player& p) { return p.m_lastLiquid ? std::to_string(p.m_lastLiquid->Id) : "-"; }
static size_t rows = 0;
struct Case {
    std::string name;
    unsigned store, step = 0;
    Player player;
    std::ostream& out;
    Case(const std::filesystem::path& root, std::ostream& output, std::string label,
         unsigned selected = 0, uint8 flags = 0, uint32 timer = 0, int64 last = -1)
        : name(std::move(label)), store(selected), out(output) {
        loadStore(root, store);
        player.m_environmentFlags = flags;
        player.m_positionStatusUpdateTimer = timer;
        if (last >= 0) {
            player.m_lastLiquid = sLiquidTypeStore.LookupEntry(uint32(last));
            require(player.m_lastLiquid != nullptr, "authored initial last-liquid exists");
        }
    }
    void emit(const char* phase, const char* pose, uint32 diff, const Facts& facts) {
        player.audit.calls.clear(); player.map.terrain.queries = 0; player.map.terrain.facts = facts;
        out << name << '\t' << step++ << '\t' << store << '\t' << phase << '\t' << pose
            << '\t' << diff << '\t' << uint32(facts.status) << '\t' << facts.entry << '\t' << facts.type_flags;
        putFloat(out, facts.level); putFloat(out, facts.depth); putFloat(out, facts.collision);
        out << '\t' << facts.interval << '\t' << facts.has_aura << '\t' << unsigned(player.m_environmentFlags)
            << '\t' << player.m_positionStatusUpdateTimer << '\t' << lastEntry(player);
        if (std::string(phase) == "update") player.UpdateEnvironmentTimer(diff);
        else require(player.SetPositionEnvironment(&player.map, 10.f + float(step), 20.f, 30.f), "source position gate success");
        out << '\t' << unsigned(player.m_environmentFlags) << '\t' << player.m_positionStatusUpdateTimer
            << '\t' << lastEntry(player) << '\t' << player.map.terrain.queries << '\t' << player.audit.text() << '\n';
        ++rows;
    }
    void position(const Facts& facts, const char* pose = "same") { emit("position", pose, 0, facts); }
    void update(uint32 diff) { emit("update", "none", diff, player.map.terrain.facts); }
    void refresh(const Facts& facts, const char* pose = "move") { update(100); position(facts, pose); }
};
static Facts facts(GridMapLiquidStatus status, uint32 entry, uint32 types) {
    Facts f; f.status = status; f.entry = entry; f.type_flags = types; return f;
}
static void scenarios(const std::filesystem::path& root, std::ostream& out) {
    const auto water = facts(LIQUID_MAP_UNDER_WATER, 11, MAP_LIQUID_TYPE_WATER);
    const auto magma = facts(LIQUID_MAP_IN_WATER, 12, MAP_LIQUID_TYPE_MAGMA);
    const auto dry = facts(LIQUID_MAP_NO_WATER, 0, 0);
    {
        Case c(root,out,"first_refresh_99_100_multiple_positions");
        c.position(water,"first"); c.position(magma,"move"); c.position(dry,"move");
        c.update(99); c.position(magma,"turn"); c.update(1); c.position(magma,"same");
        c.position(dry,"teleport"); c.update(100); c.position(dry,"same");
    }
    {
        Case c(root,out,"stationary_expiry_does_not_query");
        c.position(water,"first"); c.update(100); c.update(1000); c.update(0);
        c.position(dry,"same");
    }
    {
        Case c(root,out,"water_magma_slime_dry_selective_retention");
        c.position(water,"first"); c.refresh(magma);
        c.refresh(facts(LIQUID_MAP_WATER_WALK,13,MAP_LIQUID_TYPE_SLIME));
        c.refresh(dry); c.refresh(dry);
    }
    for (unsigned store : {0u,1u}) {
        Case c(root,out,"zero_missing_replacement_store"+std::to_string(store),store);
        c.position(water,"first"); c.refresh(facts(LIQUID_MAP_IN_WATER,0,MAP_LIQUID_TYPE_WATER));
        c.refresh(water); c.refresh(facts(LIQUID_MAP_IN_WATER,77,MAP_LIQUID_TYPE_WATER));
        c.refresh(water); c.refresh(facts(LIQUID_MAP_IN_WATER,15,MAP_LIQUID_TYPE_OCEAN));
        c.refresh(facts(LIQUID_MAP_IN_WATER,14,MAP_LIQUID_TYPE_OCEAN));
        c.refresh(facts(LIQUID_MAP_IN_WATER,0,MAP_LIQUID_TYPE_WATER));
        c.refresh(facts(LIQUID_MAP_IN_WATER,65536,MAP_LIQUID_TYPE_WATER));
        c.refresh(facts(LIQUID_MAP_ABOVE_WATER,65536,MAP_LIQUID_TYPE_WATER));
        c.refresh(dry);
    }
    {
        Case c(root,out,"existing_row_zero_pointer",0,0,0,0);
        c.position(facts(LIQUID_MAP_IN_WATER,0,MAP_LIQUID_TYPE_WATER),"first");
    }
    {
        Case c(root,out,"all_callback_order",0,0,0,12);
        auto f = water; f.type_flags = MAP_ALL_LIQUIDS | MAP_LIQUID_TYPE_DEEP_WATER;
        c.position(f,"first"); c.refresh(dry);
    }
    // Every possible environment byte establishes mask early-out and retention,
    // including the unused high bit. No invalid liquid/status inputs are used.
    for (unsigned before = 0; before < 256; ++before) {
        Case c(root,out,"dry_mask_"+std::to_string(before),0,uint8(before),0,11);
        c.position(dry,"first");
    }
    // Separate dependency answers are explicit. Cast/remove never mutate them.
    unsigned ordinal = 0;
    for (uint32 status : {1u,2u,4u,8u})
        for (uint32 types : {0u,1u,2u,4u,8u,16u,32u,63u})
            for (uint32 entry : {0u,11u,12u,14u,15u,77u,65536u})
                for (uint32 interval : {0u,60000u})
                    for (bool aura : {false,true}) {
                        const uint8 prior = uint8((ordinal % 4 == 0) ? 0 : (ordinal % 4 == 1) ? 127 : (ordinal % 4 == 2) ? 128 : 255);
                        const int64 last = ordinal % 3 == 0 ? -1 : ordinal % 3 == 1 ? 11 : 14;
                        Case c(root,out,"matrix_"+std::to_string(ordinal++),0,prior,0,last);
                        auto f = facts(GridMapLiquidStatus(status),entry,types);
                        f.interval = interval; f.has_aura = aura; c.position(f,"first");
                    }
    // Binary32 points straddle the original double-expression threshold.
    ordinal = 0;
    for (float collision : {0.f,fromBits(0x3f800001),fromBits(0x3f800002),2.f,2.03128f,64.00001f})
        for (float depth : {-16.f,0.f,1.f,256.f}) {
            const float rounded = float(double(depth) + double(collision) * 0.75);
            for (float level : {std::nextafter(rounded,-INFINITY),rounded,std::nextafter(rounded,INFINITY)}) {
                Case c(root,out,"high_liquid_double_threshold_"+std::to_string(ordinal++));
                auto f = water; f.collision = collision; f.depth = depth; f.level = level;
                c.position(f,"first");
            }
        }
    for (uint32 timer : {0u,1u,99u,100u,0xffffffffu})
        for (uint32 diff : {0u,1u,99u,100u,101u,0xffffffffu}) {
            Case c(root,out,"timer_"+std::to_string(timer)+"_"+std::to_string(diff),0,127,timer,11);
            c.update(diff);
        }
}
int main(int argc, char** argv) {
    try {
        require(argc == 2, "output directory argument");
        require(sizeof(float) == 4 && std::numeric_limits<float>::is_iec559, "IEEE binary32 profile");
        const std::filesystem::path root(argv[1]);
        writeStores(root); dbcObservations(root);
        std::ofstream out(root / "environment-state.tsv");
        out << "# Exact Player environment reducer and post-relocation 100ms gate; CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f\n"
            "# Authored geometry facts and dependency answers; callbacks are intents, not aura/threat/mirror implementations\n"
            "# Floats are hexadecimal binary32; '-' is null last entry or empty callbacks; pose labels do not execute relocation\n"
            "# case\tstep\tstore\tphase\tpose\tdiff_ms\tstatus\tentry\ttype_flags\tlevel_bits\tdepth_bits\tcollision_bits\tbreathing_interval\thas_aura\tbefore_flags\tbefore_timer_ms\tbefore_last_entry\tafter_flags\tafter_timer_ms\tafter_last_entry\tquery_count\tcallbacks\n";
        scenarios(root,out); require(bool(out), "write source observations");
        std::cout << "Source observations: " << rows << " reducer/timer steps, 20 DBC lookups, 2 complete authored WDBC files\n";
    } catch (const std::exception& e) { std::cerr << e.what() << '\n'; return 1; }
}
