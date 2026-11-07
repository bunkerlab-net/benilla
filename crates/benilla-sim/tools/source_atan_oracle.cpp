// SPDX-License-Identifier: GPL-3.0-or-later
// Authored scalar harness. PINNED markers are replaced only with original text.
#include <cmath>
#include <cfloat>
#include <cfenv>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <limits>
#include <type_traits>
#include <vector>
#include <xmmintrin.h>
#include "ObjectPosSelector.h"

// PINNED_CONSTANTS

// Global atan(float-expression) has the audited original-TU double signature.
static_assert(std::is_same_v<decltype(atan(1.0f)), double>);
static_assert(sizeof(float) == 4 && sizeof(double) == 8 && sizeof(int) == 4);
static_assert(FLT_EVAL_METHOD == 0);
static_assert(std::numeric_limits<float>::is_iec559);
static_assert(std::numeric_limits<double>::is_iec559);

static float from_bits(uint32_t word) {
    float result;
    std::memcpy(&result, &word, 4);
    return result;
}
static uint32_t bits(float value) {
    uint32_t result;
    std::memcpy(&result, &value, 4);
    return result;
}
static uint64_t bits(double value) {
    uint64_t result;
    std::memcpy(&result, &value, 8);
    return result;
}
static bool nonnegative_finite(uint32_t word) {
    return (word & 0x7fffffff) < 0x7f800000 &&
        (!(word & 0x80000000) || (word & 0x7fffffff) == 0);
}

// PINNED_SELECTOR_METHODS

int main() {
    // No FTZ/DAZ or non-nearest environment can silently change this profile.
    if (std::fesetround(FE_TONEAREST) != 0 || std::fegetround() != FE_TONEAREST ||
        (_mm_getcsr() & (0x8040 | 0x6000)) != 0) return 2;
    if (bits(M_PI_F) != 0x40490fdb || bits(DEFAULT_WORLD_OBJECT_SIZE) != 0x3ec72b02)
        return 3;
    char operation[16];
    while (std::scanf("%15s", operation) == 1) {
        uint32_t first = 0, second = 0;
        if (std::scanf("%x", &first) != 1) return 4;
        if (std::strcmp(operation, "atan") == 0) {
            if (!(nonnegative_finite(first) || first == 0x7f800000)) return 5;
            const double answer = atan(static_cast<double>(from_bits(first)));
            std::printf("%016llx %08x\n", static_cast<unsigned long long>(bits(answer)),
                        bits(static_cast<float>(answer)));
        } else if (std::strcmp(operation, "selector") == 0) {
            if (std::scanf("%x", &second) != 1 ||
                !nonnegative_finite(first) || !nonnegative_finite(second)) return 6;
            const float size = from_bits(first), distance = from_bits(second);
            ObjectPosSelector selector(0, 0, distance, size, nullptr);
            if (!selector.m_UsedAreaLists[USED_POS_PLUS].empty() ||
                !selector.m_UsedAreaLists[USED_POS_MINUS].empty()) return 7;
            // Diagnostic ratio only. The original constructor above computes
            // the observed half-angle independently using its unchanged body.
            float repaired_size = size == 0.0f ? DEFAULT_WORLD_OBJECT_SIZE : size;
            float repaired_distance = distance == 0.0f ? DEFAULT_WORLD_OBJECT_SIZE : distance;
            const float ratio = OCCUPY_POS_ANGLE_ATAN_FACTOR * repaired_size / repaired_distance;
            selector.InitializeAngle();
            std::vector<uint32_t> angles;
            float angle = from_bits(0x7fc00000);
            while (selector.NextAngle(angle)) {
                angles.push_back(bits(angle));
                // A guard on this experiment, never a substituted source return.
                if (angles.size() > 628) return 8;
            }
            if (selector.NextUsedAngle(angle)) return 9;
            std::printf("%08x %08x %08x %08x %zu", bits(ratio),
                        bits(selector.m_searchedForReqHAngle),
                        bits(selector.m_stepAngle[USED_POS_PLUS]),
                        bits(selector.m_stepAngle[USED_POS_MINUS]), angles.size());
            for (uint32_t word : angles) std::printf(" %08x", word);
            std::putchar('\n');
        } else return 10;
    }
    return std::ferror(stdin) ? 11 : 0;
}
