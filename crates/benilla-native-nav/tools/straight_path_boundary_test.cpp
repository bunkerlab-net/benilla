// SPDX-License-Identifier: GPL-3.0-or-later
// Benilla-authored standalone warm-straight bounds and fault tests. All geometry is
// synthetic. Allocator overrides stay in this executable, never the Rust test suite.
#include "source_straight_path.h"
#include "DetourAlloc.h"
#include <sanitizer/asan_interface.h>
#include <array>
#include <bit>
#include <cfloat>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstdint>
#include <cstddef>
#include <cstring>
#include <limits>
#include <new>
#include <string>
#include <vector>

static_assert(sizeof(BenillaSourceRayResult) == 616 && alignof(BenillaSourceRayResult) == 8);
static_assert(offsetof(BenillaSourceRayResult, hit) == 592 &&
    offsetof(BenillaSourceRayResult, count) == 608 && offsetof(BenillaSourceRayResult, status) == 612);

static size_t allocationCalls = 0, liveAllocations = 0, nothrowCalls = 0;
static bool failNothrow = false;
void* operator new(size_t size, const std::nothrow_t&) noexcept {
    ++nothrowCalls;
    if (failNothrow) return nullptr;
    try { return ::operator new(size); }
    catch (const std::bad_alloc&) { return nullptr; }
}
static void require(bool condition, const char* message) {
    if (!condition) { std::fprintf(stderr, "FAIL: %s\n", message); std::exit(1); }
}
static void* trackedAlloc(size_t size, dtAllocHint) {
    ++allocationCalls;
    void* result = std::malloc(size);
    if (result) ++liveAllocations;
    return result;
}
static void trackedFree(void* p) {
    if (p) {
        require(liveAllocations > 0, "allocation accounting underflow");
        --liveAllocations;
        std::free(p);
    }
}
static void resetAllocationCounters() {
    allocationCalls = nothrowCalls = 0;
    failNothrow = false;
}
static std::vector<unsigned char> read(const std::string& path) {
    FILE* f = std::fopen(path.c_str(), "rb"); require(f, "open authored fixture");
    require(std::fseek(f, 0, SEEK_END) == 0, "seek fixture");
    const long size = std::ftell(f);
    require(size > 0 && size <= 16 * 1024 * 1024, "bounded fixture size");
    require(std::fseek(f, 0, SEEK_SET) == 0, "rewind fixture");
    std::vector<unsigned char> bytes(static_cast<size_t>(size));
    require(std::fread(bytes.data(), 1, bytes.size(), f) == bytes.size(), "read fixture");
    require(std::fclose(f) == 0, "close fixture");
    return bytes;
}
extern "C" {
void* benilla_nav_create(const unsigned char* params);
void benilla_nav_free(void* raw);
unsigned benilla_nav_add(void* raw, const unsigned char* bytes, int size);
unsigned benilla_nav_source_nearest(void* raw, const float* point, const float* extents,
    unsigned short include, unsigned short exclude, dtPolyRef* ref, float* nearest,
    unsigned char* over);
BenillaSourceRay* benilla_nav_source_ray_create(void* raw, dtPolyRef start,
    const float* from, const float* to, unsigned short include, unsigned short exclude);
unsigned benilla_nav_straight_height(void* raw, dtPolyRef reference,
    const float* point, float* height);
}
static uint32_t bits(float value) { return std::bit_cast<uint32_t>(value); }
static float word(uint32_t value) { return std::bit_cast<float>(value); }
static bool equalPoint(const float* a, const float* b) {
    return std::memcmp(a, b, 3 * sizeof(float)) == 0;
}
static void scalarBounds() {
    const float from[3] = {0, 0, 0};
    unsigned steps = 0;
    float diff[3] = {};
    const auto plan = [&](float length, unsigned capacity = 1026) {
        const float to[3] = {length, 0, 0};
        return benilla_source_straight_plan(from, to, capacity, &steps, diff);
    };
    require(!benilla_source_straight_plan(nullptr, from, 1026, &steps, diff) &&
        !benilla_source_straight_plan(from, nullptr, 1026, &steps, diff) &&
        !benilla_source_straight_plan(from, from, 1026, nullptr, diff) &&
        !benilla_source_straight_plan(from, from, 1026, &steps, nullptr),
        "null plan inputs and outputs rejected");
    for (unsigned capacity : {0u, 1u, 1027u, std::numeric_limits<unsigned>::max()})
        require(plan(0, capacity) == 0, "short source path cannot bypass capacity domain");
    require(plan(0) == 2 && steps == 0, "zero-length source shortcut");
    require(plan(word(0x3dcccccc)) == 2 && steps == 0, "length below source 0.1f");
    require(plan(0.1f) == 2 && steps == 0, "length equal to source 0.1f");
    require(plan(word(0x3dccccce)) == 1 && steps == 0, "length above source 0.1f");
    require(plan(4) == 1 && steps == 1 && diff[0] == 4, "four retains duplicate endpoint step");
    require(plan(8) == 1 && steps == 1, "eight merges final step");
    require(plan(4.1f, 3) == 1 && steps == 1 && plan(8, 3) == 1 && steps == 1,
        "capacity counts source truncation and final-step merging");
    require(plan(4096, 1025) == 1 && steps == 1023 && plan(4096, 1024) == 0,
        "tight capacity admits exact source sample count at length bound");
    require(plan(word(0x41133333)) == 1 && steps == 1, "remainder just below source 0.3f");
    require(plan(word(0x41133334)) == 1 && steps == 2, "remainder just above source 0.3f");
    require(plan(4096) == 1 && steps == 1023 && steps + 2 <= 1026,
        "inclusive 4096 length has bounded source sample count");
    require(plan(std::nextafter(4096.0f, 0.0f)) == 1 && steps + 2 <= 1026,
        "length immediately below operational limit");
    require(plan(std::nextafter(4096.0f, INFINITY)) == 0, "reject length above 4096");
    require(plan(5000, std::numeric_limits<unsigned>::max()) == 0,
        "oversized capacity cannot bypass 4096 length domain");
    require(plan(4, 1027) == 0 && plan(4, std::numeric_limits<unsigned>::max()) == 0,
        "reject capacity above 1026");
    require(plan(4, 0) == 0 && plan(4, 1) == 0 && plan(4, 2) == 0 && plan(4, 3) == 1,
        "point capacity checks precede source sampling");
    for (float length : {INFINITY, -INFINITY, word(0x7fc00000), FLT_MAX,
            17179869184.0f, std::nextafter(17179869184.0f, 0.0f), 1e19f}) {
        require(plan(length) == 0 && plan(length, std::numeric_limits<unsigned>::max()) == 0,
            "reject nonfinite length, overflowing magnitude and unsafe float-to-u32 range");
    }
    for (float invalid : {INFINITY, -INFINITY, word(0x7fc00000)}) {
        for (unsigned axis = 0; axis < 3; ++axis) {
            float endpoint[3] = {0, 0, 0}; endpoint[axis] = invalid;
            require(!benilla_source_straight_plan(endpoint, from, 1026, &steps, diff) &&
                !benilla_source_straight_plan(from, endpoint, 1026, &steps, diff),
                "every nonfinite plan endpoint component rejected");
        }
    }
    // Hit and width are source ray fractions: subtract the full width before lerp.
    const float to[3] = {80, 16, 40};
    float adjusted = 0, point[3] = {};
    require(benilla_source_straight_hit(from, to, 0.5f, 0.125f, &adjusted, point) == 1,
        "accepted clipped hit");
    const float expected[3] = {30, 6, 15};
    require(adjusted == 0.375f && equalPoint(point, expected), "full width subtracts from t");
    for (uint32_t value : {0x3dccccccu, 0x3dcccccdu, 0x3dccccceu}) {
        point[0] = point[1] = point[2] = -77;
        const unsigned status = benilla_source_straight_hit(from, to, word(value), 0,
            &adjusted, point);
        require(bits(adjusted) == value && status == unsigned(value >= 0x3dcccccd),
            "hit rejection is strictly below 0.1f");
        if (!status) require(point[0] == -77 && point[1] == -77 && point[2] == -77,
            "rejected hit does not publish a point");
    }
    const float close[3] = {0.08f, 0, 0.06f}; // Detour YZX; squared result rounds below double .01.
    require(benilla_source_straight_hit_distance(from, close) == 0,
        "squared float .01 is below original double 0.01 threshold");
    const float far[3] = {0, 0, 0.1f};
    require(benilla_source_straight_hit_distance(from, far) == 1,
        "next representable squared distance above double 0.01");
    const float justBelow[3] = {0, 0, word(0x3dcccccc)};
    require(benilla_source_straight_hit_distance(from, justBelow) == 0,
        "squared distance below double 0.01");
    float previous[3] = {16777216.0f, 2, -3};
    const float advance[3] = {1, 4, 8};
    benilla_source_straight_advance(previous, advance);
    const float advanced[3] = {16777216.0f, 6, 5};
    require(equalPoint(previous, advanced), "scalar float accumulation preserves rounding");

    const float end[3] = {100, 0, 0};
    for (unsigned count : {0u, 1u, 2u, 73u, 74u, 75u, 1026u}) {
        float last[3] = {80, 0, 0}, actual[3] = {};
        unsigned shortcut = 99;
        const unsigned result = benilla_source_straight_terminal(from, end, count, 4, 0,
            last, actual, &shortcut);
        require(result == (count < 2 ? 8u : count == 74 ? 32u : 4u),
            "source terminal limit is exactly 74, never >=74");
        require(shortcut == unsigned(count < 2 || count == 74), "source shortcut selection");
        require(equalPoint(actual, shortcut ? end : last), "source terminal actual end");
        require(last[0] == 80, "unforced terminal preserves final sample");
    }
    for (float lastX : {80.0f, 0.0f}) {
        float last[3] = {lastX, 0, 0}, actual[3] = {};
        unsigned shortcut = 99;
        require(benilla_source_straight_terminal(from, end, 75, 4, 1, last, actual, &shortcut) == 17,
            "force terminal preserves count above 74");
        require(equalPoint(actual, end) && shortcut == unsigned(lastX == 0),
            "force near/far source shortcut choice");
        require(last[0] == (lastX == 0 ? 0 : 100), "force only overwrites near final sample");
    }
}
static void* load(const std::string& fixtures, const char* tile) {
    const auto params = read(fixtures + "/direct-corridor/params.test-fixture");
    const auto bytes = read(fixtures + tile);
    require(params.size() == 28 && bytes.size() > 120, "authored framing");
    void* nav = benilla_nav_create(params.data()); require(nav, "create production mesh");
    require(benilla_nav_add(nav, bytes.data() + 20, int(bytes.size() - 20)) == DT_SUCCESS,
        "load authored tile through production aligned bridge");
    return nav;
}
static dtPolyRef nearest(void* nav, const float* point) {
    const float extent[3] = {1, 1, 1};
    dtPolyRef ref = 0; float output[3] = {}; unsigned char over = 0;
    require(benilla_nav_source_nearest(nav, point, extent, 1, 0, &ref, output, &over) == DT_SUCCESS && ref,
        "nearest valid authored reference");
    return ref;
}
static void disposeRay(BenillaSourceRay* ray) {
    benilla_source_ray_free(ray);
    // Public ASan inspection, without dereferencing the freed opaque object. This
    // verifies outer-state disposal even where sandbox ptrace prevents LeakSanitizer.
    require(__asan_address_is_poisoned(ray), "private outer ray allocation is released");
}
static BenillaSourceRayResult complete(void* nav, dtPolyRef start, const float* from,
    const float* to, unsigned expectedWork, unsigned expectedCount, unsigned expectedStatus) {
    const size_t baseline = liveAllocations;
    resetAllocationCounters();
    auto* ray = benilla_nav_source_ray_create(nav, start, from, to, 1, 0);
    require(ray && nothrowCalls == 1 && allocationCalls == 0,
        "one owned ray allocation with no Detour backing allocations");
    struct Guarded { uint64_t before; BenillaSourceRayResult value; uint64_t after; } out{};
    out.before = 0x123456789abcdef0ULL; out.after = 0xfedcba9876543210ULL;
    unsigned work = 0, done;
    do {
        require(++work <= 4096, "caller bounds ray work at 4096 polygons");
        done = benilla_source_ray_step(ray);
        require(done <= 1, "ray step returns pending or finished");
        benilla_source_ray_result(ray, &out.value);
        require(out.before == 0x123456789abcdef0ULL && out.after == 0xfedcba9876543210ULL,
            "ray result fixed-buffer canaries");
        require(out.value.count <= 74, "ray result stays within 74 entries while traversal continues");
    } while (!done);
    require(work == expectedWork && out.value.count == expectedCount && out.value.status == expectedStatus,
        "source exact work, result count and status");
    BenillaSourceRayResult final{};
    require(benilla_source_ray_step(ray) == 1, "completed step remains terminal");
    benilla_source_ray_result(ray, &final);
    require(std::memcmp(&final, &out.value, sizeof(final)) == 0, "terminal step preserves result");
    disposeRay(ray);
    require(liveAllocations == baseline && allocationCalls == 0, "ray disposal preserves mesh allocation baseline");
    return final;
}
static void rayBoundsAndFaults(const std::string& fixtures) {
    void* nav = load(fixtures, "/warm-cache/precision.test-fixture");
    const float from[3] = {5, 0, 5}, to[3] = {15, 0, 5};
    const auto start = nearest(nav, from);
    const size_t baseline = liveAllocations;
    resetAllocationCounters();
    require(!benilla_source_ray_create(nullptr, start, from, to, 1, 0), "null source mesh rejected");
    require(!benilla_nav_source_ray_create(nullptr, start, from, to, 1, 0), "null bridge mesh rejected");
    for (dtPolyRef ref : {dtPolyRef(0), dtPolyRef(1), std::numeric_limits<dtPolyRef>::max()})
        require(!benilla_nav_source_ray_create(nav, ref, from, to, 1, 0),
            "invalid reference rejected before legacy uninitialized hitNormal domain");
    require(!benilla_nav_source_ray_create(nav, start, nullptr, to, 1, 0) &&
        !benilla_nav_source_ray_create(nav, start, from, nullptr, 1, 0), "null endpoints rejected");
    for (float invalid : {INFINITY, -INFINITY, word(0x7fc00000)}) {
        for (unsigned axis = 0; axis < 3; ++axis) {
            float point[3] = {5, 0, 5}; point[axis] = invalid;
            require(!benilla_nav_source_ray_create(nav, start, point, to, 1, 0) &&
                !benilla_nav_source_ray_create(nav, start, from, point, 1, 0),
                "every nonfinite endpoint component rejected before ray allocation");
        }
    }
    require(nothrowCalls == 0 && allocationCalls == 0 && liveAllocations == baseline,
        "invalid ray domain performs no allocations");
    failNothrow = true;
    require(!benilla_nav_source_ray_create(nav, start, from, to, 1, 0), "ray outer allocation failure");
    require(nothrowCalls == 1 && allocationCalls == 0 && liveAllocations == baseline,
        "failed ray construction has no retained backing allocations");
    const auto result = complete(nav, start, from, to, 2, 2, DT_SUCCESS);
    require(result.hit == FLT_MAX && result.path[0] == start && result.path[1] == start + 1,
        "recovery after allocation failure reaches neighboring polygon");
    require(result.normal[0] == 0 && result.normal[1] == 0 && result.normal[2] == 0,
        "no-hit normal initialized by original valid ray semantics");
    // ASan verifies the sole outer allocation is poisoned after each private disposal.
    for (unsigned cancelAfter : {0u, 1u}) {
        resetAllocationCounters();
        auto* ray = benilla_nav_source_ray_create(nav, start, from, to, 1, 0);
        require(ray, "create pending ray for cancellation");
        for (unsigned i = 0; i < cancelAfter; ++i)
            require(benilla_source_ray_step(ray) == 0, "cancel only a pending ray");
        disposeRay(ray);
        require(liveAllocations == baseline, "pending cancellation keeps immutable mesh alive");
        complete(nav, start, from, to, 2, 2, DT_SUCCESS);
    }
    const float hitTo[3] = {5, 2, 25};
    const auto hit = complete(nav, start, from, hitTo, 1, 1, DT_SUCCESS);
    require(hit.hit == 0.25f && hit.normal[0] == 0 && hit.normal[1] == 0 && hit.normal[2] == -1,
        "wall hit fraction and normal");
    float height = 123;
    require(benilla_nav_straight_height(nav, 0, from, &height) == (DT_FAILURE | DT_INVALID_PARAM) && height == 123,
        "rejected height query preserves initialized source output");
    benilla_source_ray_free(nullptr);
    benilla_nav_free(nav);
    require(liveAllocations == 0, "precision mesh and query allocations released");

    nav = load(fixtures, "/direct-corridor/long600.test-fixture");
    constexpr float grid = 533.3333f, cell = grid / 600;
    const float longFrom[3] = {-grid + cell * 0.5f, 0, -grid + cell * 50};
    const float longTo[3] = {-grid + cell * 80.5f, 0, -grid + cell * 50};
    const auto longStart = nearest(nav, longFrom);
    for (unsigned count : {73u, 74u, 75u}) {
        const float boundaryTo[3] = {-grid + cell * (float(count) - 0.5f), 0, -grid + cell * 50};
        const auto boundary = complete(nav, longStart, longFrom, boundaryTo, count,
            count < 74 ? count : 74, count > 74 ? DT_SUCCESS | DT_BUFFER_TOO_SMALL : DT_SUCCESS);
        require(boundary.hit == FLT_MAX, "ray capacity neighbors all reach their actual endpoint");
    }
    const auto saturated = complete(nav, longStart, longFrom, longTo, 81, 74, DT_SUCCESS | DT_BUFFER_TOO_SMALL);
    require(saturated.hit == FLT_MAX && saturated.path[0] == longStart &&
        saturated.path[73] == longStart + 73, "ray buffer saturation does not stop at polygon 74");
    benilla_nav_free(nav);
    require(liveAllocations == 0, "long mesh and all query allocations released");
}
int main(int argc, char** argv) {
    require(argc == 2, "supply authored fixture directory");
    scalarBounds();
    dtAllocSetCustom(trackedAlloc, trackedFree);
    rayBoundsAndFaults(argv[1]);
    dtAllocSetCustom(nullptr, nullptr);
    std::puts("PASS: warm-straight scalar bounds, exact thresholds/74, width subtraction, invalid ray inputs, allocation failure, ASan-verified pending disposal, 73/74/75/81-polygon traversal with 74-ref saturation, zero live Detour allocations");
}
