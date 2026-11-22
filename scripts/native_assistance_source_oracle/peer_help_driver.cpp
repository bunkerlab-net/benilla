// SPDX-License-Identifier: GPL-3.0-or-later
// Authored declarations and observations around unchanged pinned CheckForHelp.
// Geometry, accessibility, visibility and AI endpoints are authored leaves only.
#include <cassert>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <list>
#include <map>
#include <memory>
#include <sstream>
#include <set>
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
#include "source/HelpAIOrders.inc"
#define MANGOS_ASSERT(x) assert(x);
// Diagnostic selectors only; bit inputs use the unchanged UnitFlags enum above.
enum { TYPEID_PLAYER, TYPEID_UNIT, UNIT_FIELD_FLAGS, PLAYER_FLAGS,
       PLAYER_FLAGS_GHOST = 1, PLAYER_FLAGS_CONTESTED_PVP = 2,
       PLAYER_DUEL_ARBITER, PLAYER_DUEL_TEAM, SPELL_AURA_MOD_UNATTACKABLE };
enum class CreatureTypeFlags { VISIBLE_TO_GHOSTS };
[[noreturn]] void excluded(const char* name) {
    throw std::runtime_error(std::string("excluded peer-help shim reached: ") + name);
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
class UnitAI;
struct ObservedBool {
    std::string name;
    ObjectGuid guid;
    bool value;
    operator bool() const { return leaf(name, guid, value); }
    ObservedBool& operator=(bool next) { value = next; return *this; }
};
struct ObservedVictim {
    ObjectGuid guid;
    Unit* value = nullptr;
    operator Unit*() const { record("get_victim:" + num(guid)); return value; }
};
struct ObservedOrder {
    ObjectGuid guid;
    AIOrders value = ORDER_NONE;
    operator AIOrders() const { record("ai_order:" + num(guid) + ":" + num(value)); return value; }
};
std::vector<ObjectGuid> attacks, panic_interrupts;
struct MotionMaster {
    ObjectGuid guid;
    void InterruptPanic() { record("interrupt_panic:" + num(guid)); panic_interrupts.push_back(guid); }
};
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
    bool IsWithinLOSInMap(const WorldObject* other, bool ignoreM2Model = false) const {
        record("los_query:" + num(guid) + ":" + num(other->guid) + ":" + (ignoreM2Model ? "true" : "false"));
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
    ObservedVictim m_attacking;
#include "source/HelpAttackerSet.inc"
    AttackerSet m_attackers;
    bool authored_accessible = true, authored_visible = true;
    UnitAI* authored_ai = nullptr;
    MotionMaster motion;
    explicit Unit(ObjectGuid id) : WorldObject(id), combat{id, false}, m_attacking{id, nullptr}, motion{id} {}
#include "source/HelpGetVictim.inc"
#include "source/GetAttackerForHelper.inc"
#include "source/HelpUnitCanCall.inc"
#include "source/HelpUnitCanCheck.inc"
#include "source/HelpIsConfused.inc"
#include "source/HelpIsStunned.inc"
    bool isInAccessablePlaceFor(Unit const* observer) const {
        record("accessibility_query:" + num(guid) + ":" + num(observer->guid));
        return leaf("authored_accessibility", guid, authored_accessible);
    }
    bool IsVisibleForOrDetect(Unit const* observer, Unit const* viewpoint, bool detect) const {
        record("visibility_query:" + num(guid) + ":" + num(observer->guid) + ":" + num(viewpoint->guid) + ":" + (detect ? "true" : "false"));
        return leaf("authored_visibility", guid, authored_visible);
    }
    UnitAI* AI() const { record("ai_query:" + num(guid)); return authored_ai; }
    MotionMaster* GetMotionMaster() { record("motion_query:" + num(guid)); return &motion; }
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
    float CallForHelp = 5.0f; // authored negative control, not read by CheckForHelp
    bool HasFlag(CreatureTypeFlags) const { excluded("ghost visibility"); }
};
class Creature : public Unit {
public:
    bool m_forceAttackingCapability = false, can_aggro = true, pet = false;
    CreatureInfo info;
    ObservedBool m_checkForHelp, m_canCallForAssistance;
    explicit Creature(ObjectGuid id) : Unit(id),
        m_checkForHelp{"can_check_help", id, true}, m_canCallForAssistance{"can_call", id, true} {}
#include "source/CanCheckForHelp.inc"
#include "source/SetCanCheckForHelp.inc"
#include "source/CanCallForAssistance.inc"
#include "source/SetCanCallForAssistance.inc"
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

class UnitAI {
public:
    ObservedOrder m_currentAIOrder;
    explicit UnitAI(ObjectGuid guid) : m_currentAIOrder{guid, ORDER_NONE} {}
#include "source/HelpGetAIOrder.inc"
    void CheckForHelp(Unit*, Unit*, float);
    void AttackStart(Unit* victim) { record("attack_start:" + num(victim->guid)); attacks.push_back(victim->guid); }
};
enum { CONFIG_FLOAT_CREATURE_CHECK_FOR_HELP_RADIUS };
struct WorldConfig {
    float radius = 0;
    void setConfigPos(int key, const char* name, float value) {
        if (key != CONFIG_FLOAT_CREATURE_CHECK_FOR_HELP_RADIUS || std::string(name) != "CreatureCheckForHelpRadius")
            excluded("unexpected config default");
        radius = value;
    }
    void initialize() {
#include "source/HelpWorldRadiusDefault.inc"
    }
} config;
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
#include "source/CheckForHelp.inc"
void quoted(const std::string& text) { std::cout << '"' << text << '"'; }
void string_array(std::vector<std::string> const& values) {
    std::cout << '[';
    for (std::size_t i = 0; i < values.size(); ++i) { if (i) std::cout << ','; quoted(values[i]); }
    std::cout << ']';
}
void guid_array(std::vector<ObjectGuid> const& values) {
    std::cout << '[';
    for (std::size_t i = 0; i < values.size(); ++i) { if (i) std::cout << ','; std::cout << values[i]; }
    std::cout << ']';
}
int main() {
    std::cout << std::boolalpha;
    std::string line, name;
    std::map<ObjectGuid, std::unique_ptr<Unit>> units;
    std::map<ObjectGuid, std::unique_ptr<UnitAI>> ais;
    try {
        while (std::getline(std::cin, line)) {
            std::istringstream input(line);
            std::string command; input >> command;
            if (command == "case") {
                units.clear(); ais.clear(); world = Map{}; config.initialize();
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
                    creature->info.ExtraFlags = extra_flags; creature->info.Entry = entry;
                    creature->can_aggro = can_aggro;
                    creature->m_forceAttackingCapability = force;
                    if (kind == "pet") static_cast<Pet*>(unit.get())->actions_disabled = disabled;
                }
                if (!units.emplace(guid, std::move(unit)).second) excluded("duplicate unit definition");
            } else if (command == "helper") {
                ObjectGuid guid, victim, attacker; bool check, call, accessible, visible, ai;
                uint32 order, help_radius;
                input >> guid >> check >> call >> victim >> attacker >> accessible >> visible >> ai >> order >> help_radius;
                auto& unit = *units.at(guid);
                unit.m_attacking.value = victim ? units.at(victim).get() : nullptr;
                unit.m_attackers.clear();
                if (attacker) unit.m_attackers.insert(units.at(attacker).get());
                unit.authored_accessible = accessible; unit.authored_visible = visible;
                if (auto* creature = dynamic_cast<Creature*>(&unit)) {
                    creature->SetCanCheckForHelp(check); creature->SetCanCallForAssistance(call);
                    creature->info.CallForHelp = from_bits(help_radius);
                }
                ais.emplace(guid, std::make_unique<UnitAI>(guid));
                if (order > ORDER_CUSTOM) excluded("unknown AI order");
                ais.at(guid)->m_currentAIOrder.value = static_cast<AIOrders>(order);
                unit.authored_ai = ai ? ais.at(guid).get() : nullptr;
            } else if (command == "set_check" || command == "set_call") {
                ObjectGuid guid; bool value; input >> guid >> value;
                auto& creature = dynamic_cast<Creature&>(*units.at(guid));
                if (command == "set_check") creature.SetCanCheckForHelp(value);
                else creature.SetCanCallForAssistance(value);
            } else if (command == "radius") {
                uint32 bits; input >> bits; config.radius = from_bits(bits);
            } else if (command == "observe") {
                std::string phase; input >> phase;
                auto& observer = dynamic_cast<Creature&>(*units.at(1));
                auto& target = dynamic_cast<Creature&>(*units.at(101));
                leaves.clear(); attacks.clear(); panic_interrupts.clear();
                ais.at(1)->CheckForHelp(&target, &observer, config.radius);
                std::cout << "{\"case\":"; quoted(name);
                std::cout << ",\"phase\":"; quoted(phase);
                std::cout << ",\"radius_bits\":" << float_bits(config.radius)
                    << ",\"observer_can_check\":" << observer.m_checkForHelp.value
                    << ",\"target_can_check\":" << target.m_checkForHelp.value
                    << ",\"observer_can_call\":" << observer.m_canCallForAssistance.value
                    << ",\"target_can_call\":" << target.m_canCallForAssistance.value
                    << ",\"leaves\":"; string_array(leaves);
                std::cout << ",\"attack_start_guids\":"; guid_array(attacks);
                std::cout << ",\"panic_interrupt_guids\":"; guid_array(panic_interrupts);
                std::cout << "}\n";
            } else excluded("unknown peer-help command");
            if (!input) throw std::runtime_error("invalid peer-help input: " + line);
            std::string extra; if (input >> extra) throw std::runtime_error("trailing peer-help input");
        }
    } catch (const std::exception& error) {
        std::cerr << error.what() << '\n'; return 1;
    }
}
