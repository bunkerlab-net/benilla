// SPDX-License-Identifier: GPL-3.0-or-later
// Authored minimal host for unchanged extracted CMaNGOS predicates.
// This is not Creature::InitEntry, full ObjectMgr loading, or a runtime provider.
#include <cstdint>
#include <iostream>
#include <limits>
#include <sstream>
#include <stdexcept>
#include <string>

using uint32 = std::uint32_t;
#include "InhabitType.inc"
#include "MovementFlags.inc"

struct CreatureInfo {
    uint32 Entry;
    uint32 InhabitType;
};
struct MovementInfo {
    uint32 moveFlags;
#include "HasMovementFlag.inc"
};
struct Unit {
    virtual bool CanWalk() const = 0;
    virtual bool CanSwim() const = 0;
    virtual bool CanFly() const = 0;
    virtual ~Unit() = default;
};
struct Creature : Unit {
    CreatureInfo info;
    MovementInfo m_movementInfo;
    Creature(uint32 habitat, uint32 flags) : info{1, habitat}, m_movementInfo{flags} {}
    const CreatureInfo* GetCreatureInfo() const { return &info; }
#include "CreatureCapabilities.inc"
};
struct Log {
    uint32 count = 0;
    void outErrorDb(const char*, uint32, uint32) { ++count; }
} sLog;

static void repair(const CreatureInfo* cInfo) {
#include "RepairInhabitType.inc"
}

int main() {
    std::string line;
    while (std::getline(std::cin, line)) {
        std::istringstream input(line);
        char kind = 0;
        std::uint64_t raw = 0, flags = 0;
        input >> kind >> raw >> flags;
        if (input.fail() || raw > std::numeric_limits<uint32>::max()
            || flags > std::numeric_limits<uint32>::max()) {
            throw std::runtime_error("invalid unsigned source input");
        }
        std::string extra;
        if (input >> extra) throw std::runtime_error("trailing source input");
        Creature creature(static_cast<uint32>(raw), static_cast<uint32>(flags));
        sLog.count = 0;
        if (kind == 'R') {
            repair(creature.GetCreatureInfo());
        } else if (kind != 'P' || raw == 0 || raw > INHABIT_ANYWHERE) {
            throw std::runtime_error("predicate requires validated habitat");
        }
        std::cout << kind << ' ' << raw << ' ' << flags << ' '
                  << creature.info.InhabitType << ' ' << creature.CanWalk() << ' '
                  << creature.CanSwim() << ' ' << creature.CanFly() << ' '
                  << sLog.count << '\n';
    }
}
