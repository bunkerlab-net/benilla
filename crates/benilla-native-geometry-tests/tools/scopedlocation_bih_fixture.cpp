// SPDX-License-Identifier: GPL-3.0-or-later
// Reuse only the authored data holder/word printer; original pinned BIH.h owns
// all point traversal behavior. No point intersection algorithm is reproduced.
#define main authored_ray_bih_main
#include "vmap_bih_fixture.cpp"
#undef main
#include <cmath>

struct PointCallback {
    std::vector<uint32> seen;
    void operator()(const Vector3&, uint32 id) { seen.push_back(id); }
};
static void pointList(const std::vector<uint32>& words) {
    if (words.empty()) std::cout << '-';
    else list(words);
}
static void point(const std::string& name, const std::vector<uint32>& words,
                  const std::vector<uint32>& objects, const Vector3& position) {
    AuthoredBih bih(words, objects);
    PointCallback callback;
    bih.intersectPoint(position, callback);
    std::cout << name << '\t'; pointList(words);
    std::cout << '\t'; pointList(objects);
    for (float v : {position.x, position.y, position.z}) {
        std::cout << '\t'; word(bits(v));
    }
    std::cout << '\t'; pointList(callback.seen); std::cout << '\n';
}
int main() {
    if (std::fesetround(FE_TONEAREST) != 0) return 2;
    std::cout << "# Original CMaNGOS BIH::intersectPoint callback-order oracle, source pin 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f\n"
              << "# Authored fixed root [-2,2]^3; all words and binary32 values are hexadecimal\n"
              << "# name\ttree\tobjects\tx\ty\tz\tcallback_ids\n";
    const float infinity = std::numeric_limits<float>::infinity();
    const std::vector<std::pair<std::string, std::vector<uint32>>> trees = {
        {"leaf", {LEAF, 2, 0}},
        {"empty", {LEAF, 0, 0}},
        {"touch", split(0.f, 0.f)},
        {"gap", split(-.5f, .5f)},
        {"overlap", split(.5f, -.5f)},
        {"left", {3, bits(0.f), bits(infinity), LEAF, 1, 0}},
        {"right", {0, bits(-infinity), bits(0.f), LEAF, 1, 0}},
        {"clip", {BVH2 | 3, bits(-1.f), bits(1.f), LEAF, 1, 0}},
        {"clip_plane", {BVH2 | 3, bits(0.f), bits(0.f), LEAF, 1, 0}},
        {"empty_left", {3, bits(0.f), bits(0.f), LEAF, 0, 0, LEAF, 1, 0}},
        {"empty_right", {3, bits(0.f), bits(0.f), LEAF, 1, 0, LEAF | 1, 0, 0}},
        {"nested_overlap", {3, bits(.5f), bits(-.5f), (1u << 30) | 9, bits(.5f), bits(-.5f),
                            LEAF | 2, 1, 0, LEAF, 1, 0, LEAF | 1, 1, 0}}
    };
    for (const auto& shape : trees) {
        const auto objects = shape.first == "nested_overlap" ? std::vector<uint32>{2, 0, 1}
            : shape.first == "leaf" || shape.first == "touch" || shape.first == "gap" || shape.first == "overlap"
                ? std::vector<uint32>{1, 0} : shape.first == "empty" ? std::vector<uint32>{} : std::vector<uint32>{0};
        for (float x : {-2.0001f, -2.f, -1.5f, -1.f, -.5001f, -.5f, -.4999f,
                        -value(1), -0.f, 0.f, value(1), .4999f, .5f, .5001f, 1.f, 1.5f, 2.f, 2.0001f}) {
            point(shape.first + "_x_" + std::to_string(bits(x)), shape.second, objects, {x, 0, 0});
            point(shape.first + "_xy_" + std::to_string(bits(x)), shape.second, objects, {x, x, 0});
        }
    }
    for (unsigned axis = 0; axis < 3; ++axis)
        for (float coordinate : {-.5001f, -.5f, -.4999f, -value(1), -0.f, 0.f, value(1), .4999f, .5f, .5001f}) {
            Vector3 p(0, 0, 0); p[axis] = coordinate;
            point("axis_overlap_" + std::to_string(axis) + "_" + std::to_string(bits(coordinate)),
                  split(.5f, -.5f, axis), {1, 0}, p);
            point("axis_gap_" + std::to_string(axis) + "_" + std::to_string(bits(coordinate)),
                  split(-.5f, .5f, axis), {1, 0}, p);
        }
    return 0;
}
