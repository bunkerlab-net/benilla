// SPDX-License-Identifier: GPL-3.0-or-later
// Authored standalone shell around unchanged pinned CMaNGOS source excerpts.
// See README.md for every shim boundary. It does not implement real map traversal.
#include "source/EventProcessor.h"
#include <cassert>
#include <chrono>
#include <cstring>
#include <functional>
#include <iostream>
#include <limits>
#include <list>
#include <memory>
#include <sstream>
#include <stdexcept>
#include <string>
#include <tuple>
#include <utility>
#include <vector>

class Unit;
class Creature;
using ObjectGuid = uint64;
using GuidVector = std::vector<ObjectGuid>;
using CreatureList = std::list<Creature*>;
using TimePoint = std::chrono::system_clock::time_point;
using UnitVisibility = int;
using SpellSchools = int;
using CurrentSpellTypes = int;
enum DeathState { ALIVE, JUST_DIED, CORPSE, DEAD, JUST_ALIVED };
enum {
#include "source/NoCallAssistFlag.inc"
    CREATURE_EXTRA_FLAG_AGGRO_ZONE = 0x40000000
};
enum { TYPEID_PLAYER, TYPEID_UNIT, UNIT_FIELD_FLAGS, UNIT_FLAG_IN_COMBAT = 1,
       UNIT_FLAG_PET_IN_COMBAT = 2, UNIT_FLAG_PLAYER_CONTROLLED = 4,
       UNIT_FLAG_SKINNABLE = 8, UNIT_STAND_STATE_STAND = 0, UNIT_NPC_EMOTESTATE = 9,
       CURRENT_FIRST_NON_MELEE_SPELL = 1, CURRENT_MAX_SPELL = 4, ORDER_CRITTER_FLEE = 1,
       TEMPFACTION_RESTORE_COMBAT_STOP = 1, TEMPFACTION_RESTORE_RESPAWN = 2,
       LINKING_EVENT_AGGRO = 1, CREATURE_GROUP_EVENT_AGGRO, LINKING_EVENT_RESPAWN,
       CREATURE_GROUP_EVENT_RESPAWN, LINKING_EVENT_DESPAWN,
       UNIT_DYNAMIC_FLAGS, UNIT_DYNFLAG_NONE = 0, HIGHGUID_PET = 1,
       AI_EVENT_CALL_ASSISTANCE = 2, UNIT_NPC_FLAGS, UNIT_NPC_FLAG_NONE = 0,
       UNIT_CREATED_BY_SPELL, UNIT_STAT_ALL_STATE, AURA_STATE_HEALTHLESS_20_PERCENT,
       CREATURE_LOOT_STATUS_NONE = 0, VISIBILITY_REMOVE_CORPSE = 42 };
enum class CreatureStaticFlags { NO_MELEE_FLEE, CREATOR_LOOT };
enum class CreatureStaticFlags3 { FOREVER_CORPSE_DURATION };
struct ConfigFloat {};
struct ConfigUint {};
struct ConfigBool {};
constexpr ConfigFloat CONFIG_FLOAT_CREATURE_FAMILY_ASSISTANCE_RADIUS{};
constexpr ConfigUint CONFIG_UINT32_CREATURE_CHECK_FOR_HELP_AGGRO_DELAY{};
constexpr int CONFIG_UINT32_CREATURE_RESPAWN_AGGRO_DELAY = 1;
constexpr ConfigBool CONFIG_BOOL_SAVE_RESPAWN_TIME_IMMEDIATELY{};
struct World {
    float fallback = 10;
    uint32 delay = 5;
    float getConfig(ConfigFloat) const { return fallback; }
    uint32 getConfig(ConfigUint) const { return delay; }
    uint32 getConfig(int) const { return 0; }
    bool getConfig(ConfigBool) const { return false; }
} sWorld;
struct Log { template<class... T> void outError(T...) {} } sLog;
#define DEBUG_FILTER_LOG(...) ((void)0)
#define MANGOS_ASSERT(x) assert(x)
[[noreturn]] void excluded(const char* name) { throw std::runtime_error(std::string("excluded shim reached: ") + name); }
uint32 float_bits(float x) { uint32 word; std::memcpy(&word, &x, sizeof word); return word; }
float from_bits(uint32 x) { float value; std::memcpy(&value, &x, sizeof value); return value; }
const char* state_name(DeathState state) {
    switch (state) { case ALIVE: return "alive"; case CORPSE: return "corpse";
        case DEAD: return "dead"; case JUST_DIED: return "just_died"; case JUST_ALIVED: return "just_alived"; }
    excluded("death state");
}
struct EventObservation {
    uint64 id, add_clock, deadline;
    std::string kind;
    bool executed = false, aborted = false;
};
struct Trace {
    std::string name;
    uint64 map_clock = 0;
    std::vector<EventObservation> scheduled;
    std::vector<uint64> executed, aborted, destroyed_without_execute;
    std::vector<uint32> query_radius_bits;
    std::vector<bool> enemy_lookups;
    std::vector<ObjectGuid> received_events, handled_assistance;
    std::vector<std::string> hooks;
    std::vector<std::pair<uint64, bool>> help_after_execute;
    uint64 next_id = 1, assistance_index = 0;
} trace;
class ObservedEvent : public BasicEvent {
public:
    BasicEvent* inner;
    std::size_t record;
    ObservedEvent(BasicEvent* event, std::size_t index) : inner(event), record(index) {}
    ~ObservedEvent() override {
        const auto& event = trace.scheduled.at(record);
        if (!event.executed) trace.destroyed_without_execute.push_back(event.id);
        delete inner;
    }
    bool Execute(uint64 clock, uint32 diff) override;
    void Abort(uint64 clock) override {
        auto& event = trace.scheduled.at(record);
        event.aborted = true;
        trace.aborted.push_back(event.id);
        inner->Abort(clock); // unchanged UnitLambdaEvent inherits BasicEvent::Abort
    }
    bool IsDeletable() const override { return inner->IsDeletable(); }
};
class ObservedProcessor : public EventProcessor {
public:
    void AddEvent(BasicEvent* event, uint64 deadline, bool set_addtime = true) {
        // Names are observation labels for the exact two insertion sites, whose
        // source placement is guarded. The queue and callbacks are source code.
        const char* kind = trace.assistance_index++ % 2 == 0 ? "ReenableHelp" : "AggroLinking";
        add_observed(event, deadline, kind, set_addtime);
    }
    void add_observed(BasicEvent* event, uint64 deadline, const char* kind, bool set_addtime = true) {
        std::size_t index = trace.scheduled.size();
        trace.scheduled.push_back({trace.next_id++, m_time, deadline, kind, false, false});
        EventProcessor::AddEvent(new ObservedEvent(event, index), deadline, set_addtime);
    }
    void set_initial_clock(uint64 clock) { assert(GetEvents().empty()); m_time = clock; }
};
struct EmptyGuid {
    bool IsEmpty() const { return true; }
};
struct Position {
    bool empty = true;
    bool IsEmpty() const { return empty; }
};
struct Settings {
    template<class T> bool HasFlag(T) const { return false; }
};
struct Spell { int* m_spellInfo = nullptr; };
struct CreatureDataAddon { int standState = 0; uint32 emote = 0; };
struct CreatureInfo { uint32 ExtraFlags = 2; float CallForHelp = 5; uint32 DamageSchool = 0, NpcFlags = 0; };
struct CreatureData { uint32 GetRandomRespawnTime() const { excluded("database respawn selection"); } };
struct ObjectManager { const CreatureData* GetCreatureData(uint32) const { return nullptr; } } sObjectMgr;
struct Loot { void Update() { excluded("loot update"); } };
struct MotionMaster {
    void Clear(bool, bool) {}
    bool MoveFall() { excluded("flying death"); }
    void MoveIdle() {}
    void Initialize() {}
};
struct CombatManager {
    void TriggerCombatTimer(bool) {}
    void TriggerCombatTimer(Unit*) { excluded("controller combat timer"); }
    void OnCombatStart() { trace.hooks.push_back("combat_start"); }
    void StopCombatTimer() {}
    void StopEvade() {}
};
struct AIShim {
    ObjectGuid guid;
    explicit AIShim(ObjectGuid value = 0) : guid(value) {}
    void EnterCombat(Unit*) { trace.hooks.push_back("enter_combat"); }
    void CombatStop() { trace.hooks.push_back("combat_stop"); }
    void JustRespawned() { trace.hooks.push_back("just_respawned"); }
    void CorpseRemoved(uint32&) { trace.hooks.push_back("corpse_removed"); }
    void ReceiveAIEvent(int, Creature*, Unit*, uint32) { trace.received_events.push_back(guid); }
    void HandleAssistanceCall(Creature*, Unit*) { trace.handled_assistance.push_back(guid); }
    void AttackStart(Unit*) { excluded("controller attack"); }
    void DoFlee(int) { excluded("critter flee"); }
    void SetAIOrder(int) { excluded("critter order"); }
};
struct CreatureGroup {
    void TriggerLinkingEvent(int, Unit* = nullptr) { excluded("creature group"); }
    bool IsRespawnOverriden() { excluded("group respawn override"); }
    uint32 GetRandomRespawnTime() { excluded("group respawn time"); }
};
struct LinkingHolder {
    void DoCreatureLinkingEvent(int, Creature*, Unit* = nullptr) { excluded("creature linking"); }
    bool CanSpawn(Creature*) { excluded("spawn linking"); }
};
struct InstanceData {
    void OnCreatureEnterCombat(Creature*) { excluded("instance enter combat"); }
    void OnCreatureRespawn(Creature*) { excluded("instance respawn"); }
    void OnCreatureDespawn(Creature*) { excluded("instance despawn"); }
};
struct SpawnManager { void AddCreature(uint32) { excluded("new spawning system"); } };
struct PersistentState {};
struct Map {
    bool enemy_present = true, receivers_present = true;
    Unit* enemy = nullptr;
    CreatureList selected_receivers;
    bool IsDungeon() const { return false; }
    Unit* GetUnit(ObjectGuid) {
        trace.enemy_lookups.push_back(enemy_present);
        return enemy_present ? enemy : nullptr;
    }
    Creature* GetAnyTypeCreature(ObjectGuid);
    LinkingHolder* GetCreatureLinkingHolder() { excluded("linking holder"); }
    TimePoint GetCurrentClockTime() const { return TimePoint(std::chrono::milliseconds(trace.map_clock)); }
    void Add(Creature*) { trace.hooks.push_back("map_add_respawn"); }
    PersistentState* GetPersistentState() { excluded("persistent state"); }
    SpawnManager& GetSpawnManager() { excluded("spawn manager"); }
    void CreatureRelocation(Creature*, float, float, float, float) {}
} map;
struct PoolManager {
    template<class T> uint16 IsPartOfAPool(uint32) { return 0; }
    template<class T> void UpdatePool(PersistentState&, uint16, uint32) { excluded("pool update"); }
} sPoolMgr;
struct HighGuid {
    ObjectGuid value;
    uint32 GetHigh() const { return 0; }
    operator ObjectGuid() const { return value; }
};
class Unit {
public:
    uint32 flags = 0, m_damageByOthers = 0;
    DeathState m_deathState = ALIVE;
    ObjectGuid guid;
    bool creature = true;
    AIShim ai;
    CombatManager manager;
    MotionMaster i_motionMaster;
    ObservedProcessor m_events;
    explicit Unit(ObjectGuid value = 1) : guid(value), ai(value) {}
    virtual ~Unit() = default;
    virtual bool CanCallForAssistance() const { return false; }
    virtual bool CanCheckForHelp() const { return false; }
    void SetInCombatState(bool PvP, Unit* enemy);
    void TriggerAggroLinkingEvent(Unit* enemy);
    void CombatStop(bool includingCast = true, bool includingCombo = true);
    void SetDeathState(DeathState state);
    void Update(uint32 diff) {
#include "source/UnitUpdateWorldGuard.inc"
#include "source/UnitUpdateEventQueue.inc"
    }
    bool IsInWorld() const { return true; }
    bool IsCreature() const { return creature; }
    bool IsAlive() const { return m_deathState == ALIVE; }
    int GetTypeId() const { return creature ? TYPEID_UNIT : TYPEID_PLAYER; }
    HighGuid GetObjectGuid() const { return {guid}; }
    bool IsInCombat() const { return HasFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_IN_COMBAT); }
    bool HasFlag(int, uint32 value) const { return (flags & value) != 0; }
    void SetFlag(int, uint32 value) { flags |= value; }
    void RemoveFlag(int, uint32 value) { flags &= ~value; }
    void ClearInCombat() { RemoveFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_IN_COMBAT); }
    bool HasCharmer() const { return false; }
    EmptyGuid GetOwnerGuid() const { return {}; }
    Unit* GetMaster() { excluded("master"); }
    bool CanJoinInAttacking(Unit*) { excluded("controller combat"); }
    void SetInCombatWith(Unit*) { excluded("controller combat"); }
    void AddThreat(Unit*) { excluded("controller threat"); }
    CombatManager& GetCombatManager() { return manager; }
    AIShim* AI() { return &ai; }
    Map* GetMap() { return &map; }
    bool IsLinkingEventTrigger() { return false; }
    InstanceData* GetInstanceData() { return nullptr; }
    Spell* GetCurrentSpell(int) { return nullptr; }
    bool IsNonCombatSpell(int*) { excluded("noncombat spell"); }
    void InterruptSpell(int, bool) { excluded("interrupt noncombat spell"); }
    int getStandState() { excluded("stand addon"); }
    void SetStandState(int) { excluded("stand addon"); }
    uint32 GetUInt32Value(int) { return 0; }
    void SetUInt32Value(int, uint32) {}
    void HandleEmoteState(int) { excluded("emote addon"); }
    Position GetPosition(void*) { return {false}; }
    void* GetTransport() { return nullptr; }
    void AbortAINotifyEvent() { trace.hooks.push_back("abort_initial_notify"); }
    bool IsPlayerControlled() { return !creature; }
    bool IsControlledByPlayer() { return !creature; }
    void AttackStop(bool, bool, bool) {}
    void RemoveAllAttackers() {}
    void DeleteThreatList() {}
    void RemoveAllAurasOnDeath() {}
    void BreakCharmOutgoing() {}
    void BreakCharmIncoming() {}
    void RemoveMiniPet() {}
    void UnsummonAllTotems() {}
    void StopMoving(bool) {}
    bool CanFly() { return false; }
    void ModifyAuraState(int, bool) {}
    void ClearAllReactives() {}
    void ClearDiminishings() {}
    void ScheduleAINotify(uint32, bool) { trace.hooks.push_back("respawn_notify_shim_excluded_from_queue"); }
    void ClearComboPointHolders() {}
    void InterruptNonMeleeSpells(bool) {}
};
class Creature : public Unit {
public:
    bool m_checkForHelp, m_AlreadyCallAssistance, m_canCallForAssistance;
    bool can_aggro = false, m_isSpawningLinked = false, m_isCreatureLinkingTrigger = false;
    bool m_respawnOverriden = false, m_respawnOverrideOnce = false;
    uint32 m_respawnDelay = 25, m_corpseDelay = 60;
    time_t m_respawnTime = std::numeric_limits<time_t>::max();
    TimePoint m_corpseExpirationTime;
    Loot* m_loot = nullptr;
    int m_lootStatus = CREATURE_LOOT_STATUS_NONE;
    ObjectGuid m_killer = 0;
    Settings m_settings;
    CreatureInfo info;
    Position combat_start;
    explicit Creature(ObjectGuid value = 1) : Unit(value),
#include "source/InitialCheckForHelp.inc"
        ,
#include "source/InitialAlreadyCalled.inc"
        ,
#include "source/InitialCanCallAssistance.inc"
        {}
#include "source/SetNoCallAssistance.inc"
#include "source/CanCallForAssistance.inc"
#include "source/SetCanCallForAssistance.inc"
#include "source/CanCheckForHelp.inc"
#include "source/SetCanCheckForHelp.inc"
    std::pair<bool, GuidVector> MarkCallAssistanceOnPull(Unit* enemy);
    void CallAssistanceOnPull(Unit* enemy, GuidVector const& receivers);
    void Update(uint32 diff);
    void SetDeathState(DeathState state);
    void RemoveCorpse(bool inPlace = false);
    void InitAssistanceFlag() { const CreatureInfo* cinfo = &info;
#include "source/InitCanCallAssistance.inc"
    }
    CreatureInfo* GetCreatureInfo() { return &info; }
    CreatureGroup* GetCreatureGroup() { return nullptr; }
    const CreatureDataAddon* GetCreatureAddon() { return nullptr; }
    Position GetCombatStartPosition() { return combat_start; }
    void SetCombatStartPosition(Position value) { combat_start = value; }
    bool CanAggro() { return can_aggro; }
    void SetCanAggro(bool value) { can_aggro = value; }
    uint32 GetCreatedBySpellId() { return 0; }
    Settings& GetSettings() { return m_settings; }
    bool IsRooted() { return false; }
    bool IsInPanic() { return false; }
    bool IsNonMeleeSpellCasted(bool) { return false; }
    void SetInCombatWithZone() { excluded("zone combat"); }
    uint32 GetTemporaryFactionFlags() { return 0; }
    void ClearTemporaryFaction() { excluded("temporary faction"); }
    uint32 GetGUIDLow() { return static_cast<uint32>(guid); }
    uint32 GetEntry() { return 6; } // inert diagnostics only; no entry-6 world claim
    uint32 GetDbGuid() { return GetGUIDLow(); }
    void RemoveAllAuras() {}
    bool IsCorpseExpired() { return false; }
    void RegenerateAll(uint32) {}
    bool IsWorldBoss() { return false; }
    void SaveRespawnTime() { excluded("save respawn time"); }
    bool IsUsingNewSpawningSystem() { return false; }
    void SetTargetGuid(ObjectGuid) {}
    void Unmount() {}
    void ClearCreatureGroup() {}
    Unit* GetOwner() { return nullptr; }
    void StartCooldown(Unit*) { excluded("created by spell cooldown"); }
    void clearUnitState(uint32) {}
    void SetWalk(bool, bool) {}
    void ResetEntry(bool) { InitAssistanceFlag(); }
    void ResetSpellHitCounter() {}
    Unit* GetCreator() { return nullptr; }
    void SetLootRecipient(Unit*) {}
    void SetMeleeDamageSchool(SpellSchools) {}
    void LoadCreatureAddon(bool) {}
    DeathState GetDeathState() { return m_deathState; }
    std::string GetGuidStr() { return std::to_string(guid); }
    void UpdateObjectVisibility() {}
    void InterruptMoving() {}
    void GetRespawnCoord(float& x, float& y, float& z, float* o) { x = y = z = *o = 0; }
    UnitVisibility GetVisibility() { return 0; }
    void SetVisibility(UnitVisibility) {}
    void AddObjectToRemoveList() { excluded("remove list"); }
};
Creature* owner = nullptr;
Creature* Map::GetAnyTypeCreature(ObjectGuid guid) {
    if (!receivers_present) return nullptr;
    for (Creature* receiver : selected_receivers) if (receiver->guid == guid) return receiver;
    return nullptr;
}
namespace MaNGOS {
struct AnyAssistCreatureInRangeCheck {
    AnyAssistCreatureInRangeCheck(Creature*, Unit*, float) {}
};
template<class Check> struct CreatureListSearcher {
    CreatureList& result;
    CreatureListSearcher(CreatureList& target, Check&) : result(target) {}
};
}
namespace Cell {
template<class Searcher> void VisitAllObjects(Creature*, Searcher& searcher, float radius) {
    trace.query_radius_bits.push_back(float_bits(radius));
    searcher.result = map.selected_receivers; // authored already-selected controls only
}
}
// Deterministic authored host wall clock for source respawn eligibility.
time_t oracle_wall_time(time_t*) { return 1000; }
#define time oracle_wall_time
#include "source/UnitLambdaEvent.inc"
#include "source/SetInCombatState.inc"
#include "source/TriggerAggroLinkingEvent.inc"
#include "source/MarkCallAssistanceOnPull.inc"
#include "source/CallAssistanceOnPull.inc"
#include "source/CombatStop.inc"
#include "source/UnitSetDeathState.inc"
#include "source/CreatureSetDeathState.inc"
#include "source/RemoveCorpse.inc"
#include "source/CreatureUpdate.inc"
#undef time
bool ObservedEvent::Execute(uint64 clock, uint32 diff) {
    // The event executes unchanged UnitLambdaEvent::Execute and captured source
    // lambda, while the unedited EventProcessor owns due checks/order/deletion.
    auto& event = trace.scheduled.at(record);
    event.executed = true;
    const uint64 id = event.id;
    trace.executed.push_back(id);
    bool result = inner->Execute(clock, diff);
    trace.help_after_execute.emplace_back(id, owner->CanCheckForHelp());
    return result;
}
class EntryEvent : public BasicEvent {
public:
    bool Execute(uint64, uint32) override { owner->SetInCombatState(false, map.enemy); return true; }
};
void quoted(const std::string& text) { std::cout << '"' << text << '"'; }
template<class T> void array(const std::vector<T>& values) {
    std::cout << '[';
    for (std::size_t i = 0; i < values.size(); ++i) { if (i) std::cout << ','; std::cout << values[i]; }
    std::cout << ']';
}
void observation(const std::string& phase) {
    std::cout << "{\"case\":"; quoted(trace.name);
    std::cout << ",\"phase\":"; quoted(phase);
    std::cout << ",\"map_clock\":" << trace.map_clock << ",\"queue_clock\":" << owner->m_events.CalculateTime(0)
        << ",\"death_state\":\"" << state_name(owner->m_deathState) << "\",\"owner_in_combat\":" << owner->IsInCombat()
        << ",\"can_check_for_help\":" << owner->CanCheckForHelp() << ",\"already_called\":" << owner->m_AlreadyCallAssistance
        << ",\"can_call_assistance\":" << owner->CanCallForAssistance() << ",\"queued\":[";
    bool first = true;
    for (auto pair : owner->m_events.GetEvents()) {
        if (!first) std::cout << ',';
        first = false;
        auto* wrapper = dynamic_cast<ObservedEvent*>(pair.second);
        if (!wrapper) excluded("unobserved queued event");
        const auto& event = trace.scheduled.at(wrapper->record);
        std::cout << "{\"id\":" << event.id << ",\"kind\":\"" << event.kind
            << "\",\"add_clock\":" << pair.second->m_addTime << ",\"deadline\":" << pair.first << '}';
    }
    std::cout << "],\"scheduled\":[";
    first = true;
    for (const auto& event : trace.scheduled) {
        if (!first) std::cout << ',';
        first = false;
        std::cout << "{\"id\":" << event.id << ",\"kind\":\"" << event.kind << "\",\"add_clock\":"
            << event.add_clock << ",\"deadline\":" << event.deadline << '}';
    }
    std::cout << "],\"query_count\":" << trace.query_radius_bits.size() << ",\"query_radius_bits\":";
    array(trace.query_radius_bits);
    std::cout << ",\"enemy_lookup_results\":"; array(trace.enemy_lookups);
    std::cout << ",\"receiver_ai_events\":"; array(trace.received_events);
    std::cout << ",\"receiver_assistance_calls\":"; array(trace.handled_assistance);
    std::cout << ",\"executed\":"; array(trace.executed);
    std::cout << ",\"aborted\":"; array(trace.aborted);
    std::cout << ",\"destroyed_without_execute\":"; array(trace.destroyed_without_execute);
    std::cout << ",\"help_after_execute\":[";
    first = true;
    for (auto event : trace.help_after_execute) {
        if (!first) std::cout << ',';
        first = false;
        std::cout << "{\"id\":" << event.first << ",\"can_check_for_help\":" << event.second << '}';
    }
    std::cout << "],\"hooks\":[";
    for (std::size_t i = 0; i < trace.hooks.size(); ++i) { if (i) std::cout << ','; quoted(trace.hooks[i]); }
    std::cout << "]}\n";
}
int main() {
    std::cout << std::boolalpha;
    Unit enemy(77);
    enemy.creature = false;
    std::unique_ptr<Creature> creature;
    std::vector<std::unique_ptr<Creature>> receivers;
    std::string line;
    try {
        while (std::getline(std::cin, line)) {
            std::istringstream in(line);
            std::string command;
            in >> command;
            if (command == "case") {
                creature.reset(); // source destructor teardown before resetting observer
                receivers.clear();
                trace = Trace{};
                map = Map{};
                map.enemy = &enemy;
                uint32 flags, help, fallback, delay;
                uint64 queue, map_clock;
                unsigned already, count;
                in >> trace.name >> flags >> help >> fallback >> delay >> queue >> map_clock >> already >> count;
                if (!in) throw std::runtime_error("invalid case input");
                creature = std::make_unique<Creature>();
                owner = creature.get();
                owner->info.ExtraFlags = flags;
                owner->info.CallForHelp = from_bits(help);
                owner->InitAssistanceFlag();
                if (already) owner->SetNoCallAssistance(true);
                owner->m_events.set_initial_clock(queue);
                trace.map_clock = map_clock;
                sWorld.delay = delay;
                sWorld.fallback = from_bits(fallback);
                for (unsigned i = 0; i < count; ++i) {
                    ObjectGuid guid; in >> guid;
                    receivers.push_back(std::make_unique<Creature>(guid));
                    map.selected_receivers.push_back(receivers.back().get());
                }
                observation("initial");
                continue;
            }
            if (!owner) throw std::runtime_error("operation without case");
            if (command == "entry") owner->SetInCombatState(false, &enemy);
            else if (command == "update") {
                uint32 diff; in >> diff;
                trace.map_clock += diff;
                owner->Update(diff);
            } else if (command == "map_advance") {
                uint64 delta; in >> delta; trace.map_clock += delta;
            } else if (command == "enemy_present") {
                unsigned present; in >> present; map.enemy_present = present != 0;
            } else if (command == "receivers_present") {
                unsigned present; in >> present; map.receivers_present = present != 0;
            } else if (command == "combat_stop") owner->CombatStop();
            else if (command == "die") owner->SetDeathState(JUST_DIED);
            else if (command == "remove_corpse") owner->RemoveCorpse();
            else if (command == "respawn") { owner->m_respawnTime = 0; owner->Update(0); }
            else if (command == "teardown") { unsigned force; in >> force; owner->m_events.KillAllEvents(force != 0); }
            else if (command == "mark_abort") { for (auto event : owner->m_events.GetEvents()) event.second->to_Abort = true; }
            else if (command == "callback_entry") owner->m_events.add_observed(new EntryEvent(), owner->m_events.CalculateTime(0), "EntryCallback");
            else if (command == "sentinel") owner->m_events.add_observed(new BasicEvent(), owner->m_events.CalculateTime(sWorld.delay), "PriorEvent");
            else throw std::runtime_error("unknown operation: " + command);
            observation(line);
        }
        creature.reset();
        return 0;
    } catch (const std::exception& error) {
        std::cerr << error.what() << '\n';
        return 1;
    }
}
