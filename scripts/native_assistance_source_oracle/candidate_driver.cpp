// SPDX-License-Identifier: GPL-3.0-or-later
// Authored declarations and leaf observations, unchanged pinned source predicates.
// Authored reference sequence is not a real map/cell traversal or geometry proof.
#include <cassert>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <list>
#include <map>
#include <memory>
#include <sstream>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>
using uint32 = std::uint32_t;
using int32 = std::int32_t;
using ObjectGuid = std::uint64_t;
#include "source/CandidateUnitFlags.inc"
#include "source/CandidateUnitState.inc"
#include "source/CandidateReputationRank.inc"
#define MANGOS_ASSERT(x) assert(x);
// Diagnostic selectors only; bit inputs use the unchanged UnitFlags enum above.
enum { TYPEID_PLAYER, TYPEID_UNIT, UNIT_FIELD_FLAGS, PLAYER_FLAGS,
       PLAYER_FLAGS_GHOST = 1, PLAYER_FLAGS_CONTESTED_PVP = 2,
       PLAYER_DUEL_ARBITER, PLAYER_DUEL_TEAM, SPELL_AURA_MOD_UNATTACKABLE };
enum class CreatureTypeFlags { VISIBLE_TO_GHOSTS };
[[noreturn]] void excluded(const char* name) {
    throw std::runtime_error(std::string("excluded candidate shim reached: ") + name);
}
std::vector<std::string> leaves;
void record(const std::string& value) { leaves.push_back(value); }
std::string num(ObjectGuid value) { return std::to_string(value); }
bool leaf(const std::string& name, ObjectGuid guid, bool value) {
    record(name + ":" + num(guid) + ":" + (value ? "true" : "false"));
    return value;
}
uint32 float_bits(float value) { uint32 bits; std::memcpy(&bits, &value, sizeof bits); return bits; }
float from_bits(uint32 bits) { float value; std::memcpy(&value, &bits, sizeof value); return value; }
class Unit;
class Creature;
class Player;
struct FactionTemplateEntry {
    uint32 faction = 0, factionGroupMask = 0, enemyGroupMask = 0, friendGroupMask = 0;
    uint32 enemyFaction[4]{}, friendFaction[4]{};
    bool IsContestedGuardFaction() const { excluded("contested guard faction"); }
};
struct FactionEntry { bool HasReputation() const { excluded("reputation faction"); } };
struct FactionStore {
    const FactionEntry* LookupEntry(uint32 faction) const {
        record("authored_absent_reputation_faction:" + num(faction));
        return nullptr;
    }
} sFactionStore;
struct ReputationMgr {
    const ReputationRank* GetForcedRankIfAny(const FactionTemplateEntry*) const {
        record("authored_absent_forced_reputation"); return nullptr;
    }
    ReputationRank GetRank(const FactionEntry*) const { excluded("reputation rank"); }
    bool IsAtWar(const FactionEntry*) const { excluded("reputation at war"); }
};
struct Map {
    bool instanceable = false;
    bool Instanceable() const { return leaf("instanceable", 0, instanceable); }
} world;
struct CombatManager {
    ObjectGuid guid;
    bool evading_home = false;
    bool IsEvadingHome() const { return leaf("evading_home", guid, evading_home); }
};
class WorldObject {
public:
    ObjectGuid guid;
    bool authored_within_range = true, authored_los = true;
    explicit WorldObject(ObjectGuid id) : guid(id) {}
    bool IsWithinDistInMap(const WorldObject* other, float range) const {
        record("range_query:" + num(guid) + ":" + num(other->guid) + ":" + num(float_bits(range)));
        return leaf("authored_range", other->guid, other->authored_within_range);
    }
    bool IsWithinLOSInMap(const WorldObject* other) const {
        record("los_query:" + num(guid) + ":" + num(other->guid));
        return leaf("authored_los", other->guid, other->authored_los);
    }
};
class Unit : public WorldObject {
public:
    uint32 flags = 0, states = 0;
    bool creature = true, has_faction = true, feigning = false, ignoring_feign = false;
    bool m_canEnterCombat = true, unattackable_aura = false;
    // Deliberately observed authored state: none of the outdoor predicates reads alive.
    bool authored_alive = true;
    ObjectGuid absent_master = 0;
    FactionTemplateEntry faction;
    CombatManager combat;
    explicit Unit(ObjectGuid id) : WorldObject(id), combat{id, false} {}
    virtual ~Unit() = default;
    ReputationRank GetReactionTo(Unit const*) const;
    bool IsEnemy(Unit const*) const;
    bool IsFriend(Unit const*) const;
    bool CanAttack(Unit const*) const;
    bool CanAssist(Unit const*, bool ignoreFlags = false) const;
    bool CanAttackOnSight(Unit const*) const;
    bool CanAssistInCombatAgainst(Unit const*, Unit const*) const;
    bool CanJoinInAttacking(Unit const*) const;
    virtual bool CanInitiateAttack() const { excluded("non-creature initiation"); }
    Player const* GetControllingPlayer(bool ignoreCharms = false) const;
#include "source/CanEnterCombat.inc"
    bool HasFlag(int field, uint32 mask) const {
        if (field == PLAYER_FLAGS) { record("authored_absent_player_flag:" + num(guid) + ":" + num(mask)); return false; }
        if (field != UNIT_FIELD_FLAGS) excluded("unknown unit field");
        return leaf("flags_" + num(mask), guid, (flags & mask) != 0);
    }
#include "source/CandidateIsInCombat.inc"
    bool hasUnitState(uint32 mask) const { return leaf("state_" + num(mask), guid, (states & mask) != 0); }
    int GetTypeId() const { return creature ? TYPEID_UNIT : TYPEID_PLAYER; }
    ObjectGuid GetObjectGuid() const { return guid; }
    ObjectGuid GetPersuadedGuid() const { return 0; }
    ObjectGuid const& GetOwnerGuid() const { return absent_master; }
    ObjectGuid const& GetMasterGuid() const { return absent_master; }
    Unit const* GetMaster() const { excluded("master/PVPUI"); }
    bool IsPvP() const { excluded("PVP state"); }
    const FactionTemplateEntry* GetFactionTemplateEntry() const {
        record("faction_template:" + num(guid)); return has_faction ? &faction : nullptr;
    }
    CombatManager const& GetCombatManager() const { return combat; }
    #include "source/IsFeigningDeathSuccessfully.inc"
    bool IsIgnoringFeignDeath() const { return leaf("ignoring_feign", guid, ignoring_feign); }
    bool HasAuraType(int) const { return leaf("unattackable_aura", guid, unattackable_aura); }
    Map const* GetMap() const { return &world; }
};
struct CreatureInfo {
    uint32 ExtraFlags = 2, Entry = 6;
    bool HasFlag(CreatureTypeFlags) const { excluded("ghost visibility"); }
};
class Creature : public Unit {
public:
    bool m_forceAttackingCapability = false, can_aggro = true, pet = false;
    CreatureInfo info;
    explicit Creature(ObjectGuid id) : Unit(id) {}
    bool CanInitiateAttack() const override;
    bool CanAggro() const { return leaf("can_aggro", guid, can_aggro); }
    bool IsPet() const { return leaf("is_pet", guid, pet); }
    const CreatureInfo* GetCreatureInfo() const { return &info; }
};
class Pet : public Creature {
public:
    bool actions_disabled = false;
    explicit Pet(ObjectGuid id) : Creature(id) { pet = true; }
    bool HasActionsDisabled() const { return leaf("pet_disabled", guid, actions_disabled); }
};
class Player : public Unit {
public:
    ReputationMgr reputation;
    explicit Player(ObjectGuid id) : Unit(id) { creature = false; flags = UNIT_FLAG_PLAYER_CONTROLLED; }
    ReputationMgr const& GetReputationMgr() const { return reputation; }
    ObjectGuid GetGuidValue(int) const { excluded("duel arbiter"); }
    uint32 GetUInt32Value(int) const { excluded("duel team"); }
    bool IsPvPFreeForAll() const { excluded("PVP FFA"); }
    bool IsInGroup(Player const*) const { excluded("player group"); }
};
namespace ObjectAccessor {
const Unit* GetUnit(Unit const&, ObjectGuid) { excluded("controlled master lookup"); }
}
using CreatureList = std::list<Creature*>;
// This stand-in supplies only an authored sequence to the exact Visit loop.
// It is not GridRefManager, loaded-cell selection, or source map membership.
struct AuthoredReference { Creature* source; Creature* getSource() const { return source; } };
using CreatureMapType = std::vector<AuthoredReference>;
template<class T> struct GridRefManager {};
namespace MaNGOS {
#include "source/AnyAssistCreatureInRangeCheckDecl.inc"
#include "source/CreatureListSearcherDecl.inc"
}
#include "source/GetControllingPlayer.inc"
#include "source/FactionTemplateReaction.inc"
#include "source/FactionUnitReaction.inc"
#include "source/UnitReaction.inc"
#include "source/IsEnemy.inc"
#include "source/IsFriend.inc"
#include "source/CanAttack.inc"
#include "source/CanAssist.inc"
#include "source/CanAttackOnSight.inc"
#include "source/CanAssistInCombatAgainst.inc"
#include "source/CanJoinInAttacking.inc"
#include "source/CreatureCanInitiateAttack.inc"
#include "source/AnyAssistCreatureInRangeCheck.inc"
#include "source/CreatureListSearcherVisit.inc"
void quoted(const std::string& text) { std::cout << '"' << text << '"'; }
void string_array(std::vector<std::string> const& values) {
    std::cout << '[';
    for (std::size_t i = 0; i < values.size(); ++i) { if (i) std::cout << ','; quoted(values[i]); }
    std::cout << ']';
}
struct ObservedCheck {
    MaNGOS::AnyAssistCreatureInRangeCheck& source;
    bool first = true;
    bool operator()(Creature* candidate) {
        leaves.clear();
        bool result = source(candidate);
        if (!first) std::cout << ',';
        first = false;
        std::cout << "{\"guid\":" << candidate->guid << ",\"accepted\":" << result << ",\"leaves\":";
        string_array(leaves); std::cout << '}';
        return result;
    }
};
int main() {
    std::cout << std::boolalpha;
    std::string line, name;
    std::map<ObjectGuid, std::unique_ptr<Unit>> units;
    try {
        while (std::getline(std::cin, line)) {
            std::istringstream input(line);
            std::string command; input >> command;
            if (command == "case") {
                units.clear(); world = Map{};
                input >> name >> world.instanceable;
            } else if (command == "unit") {
                ObjectGuid guid; std::string kind; input >> guid >> kind;
                std::unique_ptr<Unit> unit;
                if (kind == "player") unit = std::make_unique<Player>(guid);
                else if (kind == "creature") unit = std::make_unique<Creature>(guid);
                else if (kind == "pet") unit = std::make_unique<Pet>(guid);
                else excluded("unknown unit kind");
                uint32 extra_flags, entry;
                bool can_aggro, force, disabled;
                input >> unit->flags >> extra_flags >> entry >> unit->has_faction
                      >> unit->faction.faction >> unit->faction.factionGroupMask
                      >> unit->faction.enemyGroupMask >> unit->faction.friendGroupMask;
                for (auto& value : unit->faction.enemyFaction) input >> value;
                for (auto& value : unit->faction.friendFaction) input >> value;
                input >> unit->authored_within_range >> unit->authored_los >> unit->combat.evading_home
                      >> unit->feigning >> unit->ignoring_feign >> unit->m_canEnterCombat
                      >> unit->unattackable_aura >> unit->states >> can_aggro >> force >> disabled >> unit->authored_alive;
                if (unit->feigning) unit->states |= UNIT_STAT_FEIGN_DEATH;
                if (unit->creature) {
                    auto* creature = static_cast<Creature*>(unit.get());
                    creature->info = {extra_flags, entry}; creature->can_aggro = can_aggro;
                    creature->m_forceAttackingCapability = force;
                    if (kind == "pet") static_cast<Pet*>(unit.get())->actions_disabled = disabled;
                }
                if (!units.emplace(guid, std::move(unit)).second) excluded("duplicate unit definition");
            } else if (command == "visit") {
                uint32 radius, count; input >> radius >> count;
                CreatureMapType sequence;
                for (uint32 i = 0; i < count; ++i) {
                    ObjectGuid guid; input >> guid;
                    auto* creature = dynamic_cast<Creature*>(units.at(guid).get());
                    if (!creature) excluded("non-creature candidate");
                    sequence.push_back({creature});
                }
                Unit* owner = units.at(1).get();
                Unit* enemy = units.at(77).get();
                MaNGOS::AnyAssistCreatureInRangeCheck predicate(owner, enemy, from_bits(radius));
                ObservedCheck observer{predicate};
                CreatureList selected;
                MaNGOS::CreatureListSearcher<ObservedCheck> searcher(selected, observer);
                std::cout << "{\"case\":"; quoted(name); std::cout << ",\"evaluations\":[";
                searcher.Visit(sequence);
                std::cout << "],\"selected_guids\":[";
                bool first = true;
                for (Creature* candidate : selected) {
                    if (!first) std::cout << ',';
                    first = false; std::cout << candidate->guid;
                }
                std::cout << "],\"independent_predicate_controls\":[";
                first = true;
                for (const auto& pair : units) {
                    auto* candidate = dynamic_cast<Creature*>(pair.second.get());
                    if (!candidate) continue;
                    // Separate direct calls report source implications. These do
                    // not occur inside the traversal or its short-circuit trace.
                    leaves.clear();
                    const auto owner_reaction = candidate->GetReactionTo(owner);
                    const auto enemy_reaction = candidate->GetReactionTo(enemy);
                    const bool assist = candidate->CanAssist(owner);
                    const bool attack = candidate->CanAttack(enemy);
                    const bool is_enemy = candidate->IsEnemy(enemy);
                    const bool sight = candidate->CanAttackOnSight(enemy);
                    if (!first) std::cout << ',';
                    first = false;
                    std::cout << "{\"guid\":" << candidate->guid << ",\"owner_reaction\":" << owner_reaction
                        << ",\"enemy_reaction\":" << enemy_reaction << ",\"can_assist\":" << assist
                        << ",\"can_attack\":" << attack << ",\"is_enemy\":" << is_enemy
                        << ",\"can_attack_on_sight\":" << sight << '}';
                }
                std::cout << "]}\n";
            } else excluded("unknown candidate command");
            if (!input) throw std::runtime_error("invalid candidate input: " + line);
            std::string extra; if (input >> extra) throw std::runtime_error("trailing candidate input");
        }
    } catch (const std::exception& error) {
        std::cerr << error.what() << '\n'; return 1;
    }
}
