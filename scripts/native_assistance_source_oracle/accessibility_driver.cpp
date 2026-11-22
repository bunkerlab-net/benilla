// SPDX-License-Identifier: GPL-3.0-or-later
// Authored observer around unchanged pinned source. No terrain or runtime inputs.
#include <cstdint>
#include <iostream>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>
using uint32 = std::uint32_t;
static std::vector<std::string> leaves;
static bool capability(const char* name, bool value) {
    leaves.emplace_back(std::string(name) + ":1:" + (value ? "true" : "false"));
    return value;
}
#include "source/AccessibilityEnvironmentFlags.inc"
struct ObservedEnvironmentFlags {
    uint32 value = 0;
    operator uint32() const {
        leaves.emplace_back("cached_environment_flags:77:" + std::to_string(value));
        return value;
    }
};
class Unit {
public:
    virtual ~Unit() = default;
#include "source/AccessibilityUnitWaterDispatch.inc"
    bool isInAccessablePlaceFor(Unit const* unit) const;
    virtual bool CanSwim() const = 0;
    virtual bool CanWalk() const = 0;
    virtual bool CanFly() const = 0;
};
bool Unit::IsInWater() const {
    throw std::runtime_error("excluded Unit terrain-water dispatch");
}
class Player : public Unit {
public:
    ObservedEnvironmentFlags m_environmentFlags;
#include "source/AccessibilityPlayerWater.inc"
    bool CanSwim() const override { throw std::runtime_error("wrong victim capability query"); }
    bool CanWalk() const override { throw std::runtime_error("wrong victim capability query"); }
    bool CanFly() const override { throw std::runtime_error("wrong victim capability query"); }
};
class Helper : public Unit {
public:
    bool swim = false, walk = false, fly = false;
    bool CanSwim() const override { return capability("can_swim", swim); }
    bool CanWalk() const override { return capability("can_walk", walk); }
    bool CanFly() const override { return capability("can_fly", fly); }
};
#include "source/PlayerVictimAccessibility.inc"
int main() {
    std::cout << std::boolalpha;
    std::string line;
    try {
        while (std::getline(std::cin, line)) {
            std::istringstream input(line);
            std::string name, extra;
            Player player;
            Helper helper;
            if (!(input >> name >> player.m_environmentFlags.value >> helper.swim >> helper.walk >> helper.fly)
                || (input >> extra))
                throw std::runtime_error("invalid accessibility input");
            // Deliberately call the original Unit body through a base pointer.
            // Its virtual IsInWater call must reach Player's exact cache accessor.
            Unit const* victim = &player;
            leaves.clear();
            bool result = victim->isInAccessablePlaceFor(&helper);
            std::cout << "{\"case\":\"" << name << "\",\"accessible\":" << result << ",\"leaves\":[";
            for (std::size_t i = 0; i < leaves.size(); ++i) {
                if (i) std::cout << ',';
                std::cout << '"' << leaves[i] << '"';
            }
            std::cout << "]}\n";
        }
    } catch (std::exception const& error) {
        std::cerr << error.what() << '\n';
        return 1;
    }
}
