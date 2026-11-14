// SPDX-License-Identifier: GPL-3.0-or-later
// Authored minimal receiver/driver. Source method bodies are generated unchanged.
#include <array>
#include <cfenv>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <iostream>
#include <limits>
#include <string>

static_assert(sizeof(float) == sizeof(uint32_t));
static_assert(std::numeric_limits<float>::is_iec559);
static_assert(std::numeric_limits<float>::digits == 24);

float from_bits(uint32_t word) {
    float value;
    std::memcpy(&value, &word, sizeof(value));
    return value;
}
uint32_t to_bits(float value) {
    uint32_t word;
    std::memcpy(&word, &value, sizeof(word));
    return word;
}

enum DistanceCalculation {
    DIST_CALC_NONE,
    DIST_CALC_BOUNDING_RADIUS,
    DIST_CALC_COMBAT_REACH,
    DIST_CALC_COMBAT_REACH_WITH_MELEE,
};
enum { CONFIG_FLOAT_LISTEN_RANGE_SAY = 1 };
struct WorldConfig {
    float say_range;
    float getConfig(int selected) const {
        if (selected != CONFIG_FLOAT_LISTEN_RANGE_SAY) std::abort();
        return say_range;
    }
} sWorld;
struct VisibilityData {
    float distance;
    float GetVisibilityDistance() const { return distance; }
};
struct WorldObject {
    std::array<float, 3> position;
    float reach;
    VisibilityData visibility;
    float GetPositionX() const { return position[0]; }
    float GetPositionY() const { return position[1]; }
    float GetPositionZ() const { return position[2]; }
    float GetCombatReach() const { return reach; }
    const VisibilityData& GetVisibilityData() const { return visibility; }
    // These source switch arms are explicitly outside the test domain. Abort
    // rather than invent a bounding-radius or combined-melee implementation.
    float GetObjectBoundingRadius() const { std::abort(); }
    float GetCombinedCombatReach(const WorldObject*, bool) const { std::abort(); }
    float GetDistance(const WorldObject*, bool, DistanceCalculation) const;
    bool _IsWithinDist(const WorldObject*, float, bool) const;
    float ExtractedSayRange() const;
};

#include "source-excerpts.inc"

int main() {
    if (std::fesetround(FE_TONEAREST) != 0) return 2;
    std::string name;
    // name, body.xyz, speaker.xyz, body.reach, speaker.reach, range, visibility
    while (std::cin >> name) {
        std::array<uint32_t, 10> words{};
        for (auto& word : words) if (!(std::cin >> std::hex >> word)) return 3;
        WorldObject body{{from_bits(words[0]), from_bits(words[1]), from_bits(words[2])},
                         from_bits(words[6]), {from_bits(words[9])}};
        WorldObject speaker{{from_bits(words[3]), from_bits(words[4]), from_bits(words[5])},
                            from_bits(words[7]), {from_bits(words[9])}};
        sWorld.say_range = from_bits(words[8]);
        const float effective = speaker.ExtractedSayRange();
        // CameraDistWorker uses camera body as receiver and speaker as argument.
        const float squared = body.GetDistance(&speaker, true, DIST_CALC_NONE);
        const bool inside = body._IsWithinDist(&speaker, effective, true);
        std::cout << name << ' ' << std::hex << to_bits(effective) << ' '
                  << to_bits(squared) << ' ' << std::dec << inside << '\n';
    }
    return std::cin.eof() ? 0 : 4;
}
