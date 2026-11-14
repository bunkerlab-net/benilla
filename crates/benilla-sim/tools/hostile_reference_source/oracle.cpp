// SPDX-License-Identifier: GPL-3.0-or-later
// Authored recording host. All reference transitions and list mutations below
// execute unchanged pinned CMaNGOS fragments. This is not a world/combat server.
#include "Common.h"
#include <algorithm>
#include <cstring>
#include <iostream>
#include <map>
#include <memory>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>

// Inherit the exact upstream event storage/constructors. The one substituted
// event type adds a trace only when the original processThreatEvent dispatch
// invokes setThreatManager; the original setter still runs first.
#include "Entities/UnitEvents.h"
static std::vector<std::string> dispatched;
static void record_dispatched(const ThreatRefStatusChangeEvent& event);
class RecordedThreatRefStatusChangeEvent : public ThreatRefStatusChangeEvent {
public:
    using ThreatRefStatusChangeEvent::ThreatRefStatusChangeEvent;
    void setThreatManager(ThreatManager* manager) {
        ThreatRefStatusChangeEvent::setThreatManager(manager);
        record_dispatched(*this);
    }
};
#define ThreatRefStatusChangeEvent RecordedThreatRefStatusChangeEvent

// Test-only observation of existing private/protected storage. This changes
// access checks only; the retained headers and executed methods stay unchanged.
#define private public
#define protected public
#include "Combat/ThreatManager.h"
#include "Combat/HostileRefManager.h"
#undef protected
#undef private

enum UnitState {
#include "FeignDeathStateConstant.inc"
};
enum class CreatureStaticFlags2 : uint32 {
#include "IgnoreFeignDeathFlagConstant.inc"
};
static CreatureStaticFlags2 operator&(CreatureStaticFlags2 lhs, CreatureStaticFlags2 rhs) {
    return CreatureStaticFlags2(uint32(lhs) & uint32(rhs));
}
struct CreatureSettings {
    CreatureStaticFlags2 m_flags2 = CreatureStaticFlags2(0);
    bool HasFlag(CreatureStaticFlags2 flag) const;
};
#include "HasStaticFlag.inc"

struct CombatData {
    ThreatManager threatManager;
    HostileRefManager hostileRefManager;
    explicit CombatData(Unit* owner) : threatManager(owner), hostileRefManager(owner) {}
};
class Unit {
public:
    ObjectGuid guid;
    uint32 m_state = 0;
    CombatData data;
    unsigned aggro_callbacks = 0;
    explicit Unit(ObjectGuid identity) : guid(identity), data(this) {}
    virtual ~Unit() = default;
    CombatData* GetCombatData() { return &data; }
    const CombatData* GetCombatData() const { return &data; }
    ObjectGuid GetObjectGuid() const { return guid; }
    virtual bool IsIgnoringFeignDeath() const { return false; }
    bool IsOfflineTarget(Unit* victim) const;
    void DeleteThreatList();
#include "HasUnitState.inc"
#include "SuccessfulFeignDeath.inc"
#include "UnitReferences.inc"
    // Authored fixed-domain dependencies, not implementations of combat rules.
    // Targets are unowned ordinary Players, with normal priority and in melee.
    Unit* GetMaster() const { return nullptr; }
    bool IsAlive() const { return true; }
    bool IsInCombat() const { throw std::runtime_error("combat state outside oracle"); }
    bool CanJoinInAttacking(Unit*) const { throw std::runtime_error("pet-owner branch outside oracle"); }
    virtual bool IsPlayer() const { return false; }
    bool IsIgnoringRangedTargets() const { return false; }
    bool CanReachWithMeleeAttack(Unit*) const { return true; }
    bool CanAttack(Unit*) const { throw std::runtime_error("player priority outside oracle"); }
    bool IsSuppressedTarget(Unit*) const { throw std::runtime_error("suppression outside oracle"); }
    void TriggerAggroLinkingEvent(Unit*) { ++aggro_callbacks; }
};
class Player : public Unit {
public:
    using Unit::Unit;
    bool IsPlayer() const override { return true; }
    bool IsGameMaster() const { return false; }
};
class Creature : public Unit {
public:
    CreatureSettings m_settings;
    using Unit::Unit;
    bool IsIgnoringFeignDeath() const override;
};
#include "IgnoreFeignDeath.inc"
#include "IsOfflineTarget.inc"
#include "DeleteThreatList.inc"

// Authored explicit lookup table: no hidden distance, map or terrain admission.
static std::map<ObjectGuid, Unit*> objects;
namespace ObjectAccessor {
static Unit* GetUnit(const Unit&, ObjectGuid guid) {
    auto found = objects.find(guid);
    return found == objects.end() ? nullptr : found->second;
}
}
void ThreatManager::addThreat(Unit*, float, bool, SpellSchoolMask, const SpellEntry*) {
    throw std::runtime_error("full threat calculation outside oracle");
}
#include "HostileReference.inc"
#include "ThreatContainerStorage.inc"
#include "ThreatContainerUpdate.inc"
#include "SelectNextVictim.inc"
#include "ThreatManagerConstruction.inc"
#include "AddThreatDirectly.inc"
#include "GetHostileTarget.inc"
#include "SetCurrentVictim.inc"
#include "ProcessThreatEvent.inc"
#include "HostileRefManagerConstruction.inc"
#include "HostileRefManagerState.inc"
#include "HostileRefManagerFirst.inc"
#undef ThreatRefStatusChangeEvent

static void record_dispatched(const ThreatRefStatusChangeEvent& event) {
    switch (event.getType()) {
        case UEV_THREAT_REF_THREAT_CHANGE: dispatched.push_back("T"); break;
        case UEV_THREAT_REF_ONLINE_STATUS:
            dispatched.push_back(event.getReference()->isOnline() ? "O1" : "O0");
            break;
        case UEV_THREAT_REF_REMOVE_FROM_LIST: dispatched.push_back("R"); break;
        case UEV_THREAT_REF_ASSECCIBLE_STATUS:
            dispatched.push_back(event.getReference()->isAccessable() ? "X1" : "X0");
            break;
        default: throw std::runtime_error("event outside bounded source corpus");
    }
}

static uint32 bits(float value) {
    uint32 result;
    std::memcpy(&result, &value, sizeof(result));
    return result;
}
static float from_bits(uint32 value) {
    float result;
    std::memcpy(&result, &value, sizeof(result));
    return result;
}
struct Host {
    // Targets outlive the owner so source clear/unlink never uses a dead target.
    std::map<ObjectGuid, std::unique_ptr<Unit>> targets;
    Creature owner{1000};
    ThreatManager& manager() { return owner.getThreatManager(); }
    Unit& target(ObjectGuid guid) {
        auto found = targets.find(guid);
        if (found == targets.end()) {
            auto made = std::make_unique<Player>(guid);
            objects[guid] = made.get();
            found = targets.emplace(guid, std::move(made)).first;
        }
        return *found->second;
    }
    HostileReference* reference(ObjectGuid guid) {
        Unit& unit = target(guid);
        auto& online = manager().iThreatContainer;
        auto& offline = manager().iThreatOfflineContainer;
        HostileReference* result = online.getReferenceByTarget(&unit);
        return result ? result : offline.getReferenceByTarget(&unit);
    }
    ~Host() {
        // Exact source unlink requires a linked target. Invalid references are
        // explicitly removed (the source target-destruction path) before clear.
        auto refs = manager().iThreatOfflineContainer.getThreatList();
        for (auto* ref : refs) if (!ref->isValid()) {
            ref->removeReference();
            delete ref;
        }
        manager().clearReferences();
        objects.clear();
    }
    void snapshot(const std::string& command, bool offline) {
        auto& m = manager();
        std::cout << command << " | " << offline << ' '
                  << (m.getCurrentVictim() ? m.getCurrentVictim()->getUnitGuid() : 0)
                  << ' ' << m.iThreatContainer.isDirty() << ' '
                  << m.iThreatContainer.getThreatList().size() << ' '
                  << m.iThreatOfflineContainer.getThreatList().size() << ' '
                  << owner.aggro_callbacks;
        // Stable identity order records membership independently of sort order.
        for (const auto& [guid, unit] : targets) {
            if (auto* ref = reference(guid)) {
                const bool in_online = std::find(m.getThreatList().begin(), m.getThreatList().end(), ref) != m.getThreatList().end();
                assert(in_online == ref->isOnline());
                assert(ref->getSource() == &m);
                assert(ref->isValid() == ref->isInList());
                std::cout << ' ' << guid << ':' << bits(ref->getThreat()) << ':'
                          << ref->isValid() << ':' << ref->isOnline() << ':'
                          << ref->isAccessable() << ':' << unit->getHostileRefManager().getSize();
            } else {
                assert(unit->getHostileRefManager().isEmpty());
            }
        }
        std::cout << " | ";
        if (dispatched.empty()) std::cout << '-';
        for (std::size_t index = 0; index < dispatched.size(); ++index) {
            if (index) std::cout << ',';
            std::cout << dispatched[index];
        }
        std::cout << '\n';
    }
};

int main() {
    std::unique_ptr<Host> host;
    std::string line;
    while (std::getline(std::cin, line)) {
        std::istringstream in(line);
        std::string op;
        in >> op;
        ObjectGuid guid = 0;
        uint32 value = 0;
        bool offline = false;
        dispatched.clear();
        if (op == "N") {
            host.reset();
            host = std::make_unique<Host>();
        } else if (op == "A") {
            in >> guid >> value;
            auto& unit = host->target(guid);
            offline = host->owner.IsOfflineTarget(&unit);
            host->manager().addThreatDirectly(&unit, from_bits(value), false);
        } else if (op == "F") {
            in >> guid >> value;
            host->target(guid).m_state = value;
        } else if (op == "G") {
            in >> value;
            host->owner.m_settings.m_flags2 = CreatureStaticFlags2(value);
        } else if (op == "U") {
            in >> guid;
            offline = host->owner.IsOfflineTarget(&host->target(guid));
            host->reference(guid)->updateOnlineStatus();
        } else if (op == "T") {
            in >> guid;
            offline = host->owner.IsOfflineTarget(&host->target(guid));
            host->target(guid).getHostileRefManager().updateThreatTables();
        } else if (op == "V") {
            in >> guid;
            host->reference(guid)->invalidate();
            objects.erase(guid);
        } else if (op == "L") {
            in >> guid;
            objects[guid] = &host->target(guid);
        } else if (op == "X") {
            in >> guid >> value;
            host->reference(guid)->setAccessibleState(value != 0);
        } else if (op == "R") {
            in >> guid;
            auto* ref = host->reference(guid);
            ref->removeReference();
            delete ref;
        } else if (op == "C") {
            host->manager().clearReferences();
        } else if (op == "D") {
            in >> guid;
            // Authored lifecycle dispatch into the exact cleanup function;
            // no damage/death state machine or respawn engine is claimed.
            if (guid == 1000) host->owner.DeleteThreatList();
            else host->target(guid).DeleteThreatList();
        } else if (op == "S") {
            if (host->manager().isThreatListEmpty()) throw std::runtime_error("empty source selection is outside oracle");
            host->manager().getHostileTarget();
        } else if (op == "P") {
            in >> guid >> value;
            uint32 flags = 0;
            in >> flags;
            host->target(guid).m_state = value;
            host->owner.m_settings.m_flags2 = CreatureStaticFlags2(flags);
            offline = host->owner.IsOfflineTarget(&host->target(guid));
        } else {
            throw std::runtime_error("unknown source operation");
        }
        if (in.fail()) throw std::runtime_error("invalid source input");
        std::string extra;
        if (in >> extra) throw std::runtime_error("trailing source input");
        host->snapshot(line, offline);
    }
}
