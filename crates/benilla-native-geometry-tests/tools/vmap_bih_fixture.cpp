// SPDX-License-Identifier: GPL-3.0-or-later
// Authored BIH structures/rays only. The original pinned BIH::intersectRay
// determines every callback order and pruning result; no native parser is called.
#include <cfenv>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <iomanip>
#include <iostream>
#include <limits>
#include <sstream>
#include <string>
#include <vector>
#include "BIH.h"

static uint32 bits(float value) {
    uint32 result;
    std::memcpy(&result, &value, sizeof(result));
    return result;
}
static float value(uint32 word) {
    float result;
    std::memcpy(&result, &word, sizeof(result));
    return result;
}
static constexpr uint32 LEAF = 3u << 30;
static constexpr uint32 BVH2 = 1u << 29;

class AuthoredBih : public BIH {
public:
    AuthoredBih(const std::vector<uint32>& words, const std::vector<uint32>& indices) {
        tree = words;
        objects = indices;
        bounds = AABox(Vector3(-2.f, -2.f, -2.f), Vector3(2.f, 2.f, 2.f));
    }
};
struct Callback {
    unsigned mode;
    std::vector<uint32> seen;
    bool operator()(const Ray&, uint32 id, float& maximum, bool, bool) {
        seen.push_back(id);
        if (mode == 0) return false;
        const float distance = mode == 1 ? .25f : (id == 1 ? 2.f : 1.5f);
        if (distance < maximum) {
            maximum = distance;
            return true;
        }
        return false;
    }
};
static void word(uint32 v) {
    std::cout << std::hex << std::setw(8) << std::setfill('0') << v << std::dec;
}
static void list(const std::vector<uint32>& words) {
    for (size_t i = 0; i < words.size(); ++i) {
        if (i) std::cout << ',';
        word(words[i]);
    }
}
static void query(const std::string& name, const std::vector<uint32>& words,
                  const std::vector<uint32>& objects, Vector3 origin, Vector3 direction,
                  float maximum, unsigned mode = 0) {
    AuthoredBih bih(words, objects);
    Callback callback{mode, {}};
    const float original = maximum;
    bih.intersectRay(Ray(origin, direction), callback, maximum, false, false);
    std::cout << name << '\t';
    list(words);
    std::cout << '\t';
    list(objects);
    for (float v : {origin.x, origin.y, origin.z, direction.x, direction.y, direction.z, original}) {
        std::cout << '\t'; word(bits(v));
    }
    std::cout << '\t' << mode << '\t';
    list(callback.seen);
    std::cout << '\t'; word(bits(maximum));
    std::cout << '\n';
}
static std::vector<uint32> split(float left, float right, unsigned axis = 0) {
    return {(axis << 30) | 3, bits(left), bits(right), LEAF, 1, 0, LEAF | 1, 1, 0};
}
int main() {
    if (std::fesetround(FE_TONEAREST) != 0) return 2;
    std::cout << "# Original CMaNGOS BIH.h callback-order oracle, source pin 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f\n"
              << "# Authored fixed root [-2,2]^3; all words and binary32 values are hexadecimal\n"
              << "# name\ttree\tobjects\tox\toy\toz\tdx\tdy\tdz\tmax_distance\tcallback_mode\tcallback_ids\tresult_distance\n";
    const float infinity = std::numeric_limits<float>::infinity();
    const std::vector<std::pair<std::string, std::vector<uint32>>> trees = {
        {"leaf", {LEAF, 2, 0}},
        {"touch", split(0.f, 0.f)},
        {"gap", split(-.5f, .5f)},
        {"overlap", split(.5f, -.5f)},
        {"left", {3, bits(0.f), bits(infinity), LEAF, 1, 0}},
        {"right", {0, bits(-infinity), bits(0.f), LEAF, 1, 0}},
        {"clip", {BVH2 | 3, bits(-1.f), bits(1.f), LEAF, 1, 0}},
        {"clip_plane", {BVH2 | 3, bits(0.f), bits(0.f), LEAF, 1, 0}}
    };
    for (const auto& shape : trees) {
        const std::vector<uint32> objects = shape.first == "leaf" || shape.second.size() == 9
            ? std::vector<uint32>{1, 0} : std::vector<uint32>{0};
        for (unsigned sign = 0; sign < 2; ++sign) {
            const float zero = sign ? -0.f : 0.f;
            for (float x : {-1.5f, -1.f, -.5f, -0.f, .5f, 1.f, 1.5f}) {
                std::ostringstream name;
                name << shape.first << "_zero_" << sign << "_" << x;
                query(name.str(), shape.second, objects, Vector3(x, -1.f, 0.f), Vector3(zero, 1.f, 0.f), 10.f);
            }
            query(shape.first + "_subnormal_" + std::to_string(sign), shape.second, objects,
                  Vector3(0.f, -1.f, 0.f), Vector3(sign ? -value(1) : value(1), 1.f, 0.f), 10.f);
        }
        for (unsigned mode = 0; mode < 3; ++mode) {
            query(shape.first + "_forward_" + std::to_string(mode), shape.second, objects,
                  Vector3(-1.f, 0.f, 0.f), Vector3(1.f, 0.f, 0.f), 10.f, mode);
            query(shape.first + "_reverse_" + std::to_string(mode), shape.second, objects,
                  Vector3(1.f, 0.f, 0.f), Vector3(-1.f, 0.f, 0.f), 10.f, mode);
        }
    }
    for (unsigned axis = 0; axis < 3; ++axis) {
        for (unsigned sign = 0; sign < 2; ++sign) {
            Vector3 origin(0.f, 0.f, 0.f), direction(0.f, 0.f, 0.f);
            origin[axis] = sign ? 1.f : -1.f;
            direction[axis] = sign ? -1.f : 1.f;
            query("axis_" + std::to_string(axis) + "_" + std::to_string(sign), split(0.f, 0.f, axis),
                  {1, 0}, origin, direction, 10.f);
        }
    }
    for (unsigned sign = 0; sign < 2; ++sign) {
        for (unsigned step = 0; step < 2; ++step) {
            const float d = value(0x3727c619 + step);
            query("fuzzy_" + std::to_string(sign) + "_" + std::to_string(step), {LEAF, 1, 0}, {0},
                  Vector3(sign ? -3.f : 3.f, -1.f, 0.f), Vector3(sign ? -d : d, 1.f, 0.f), 10.f);
        }
    }
    query("root_outward", {LEAF, 1, 0}, {0}, Vector3(2.f, 0.f, 0.f), Vector3(1.f, 0.f, 0.f), 10.f);
    query("root_endpoint", {LEAF, 1, 0}, {0}, Vector3(-3.f, 0.f, 0.f), Vector3(1.f, 0.f, 0.f), 1.f);
    return 0;
}
