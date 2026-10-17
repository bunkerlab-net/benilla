// SPDX-License-Identifier: GPL-3.0-or-later
// Benilla-authored, standalone fault-injection regression test. Uses only the authored
// numeric two-tile fixture and the production bridge; no game assets or allocator hooks
// are exposed to Rust. Process-local overrides never run in the parallel Rust test suite.
#include "source_direct.h"
#include "DetourAlloc.h"
#include <bit>
#include <cstdio>
#include <cstdlib>
#include <cstdint>
#include <new>
#include <string>
#include <vector>

static size_t allocationCalls = 0, failAt = 0, liveAllocations = 0;
static size_t nothrowCalls = 0;
static bool failNothrow = false;

// Only the standalone test replaces this standard overload. Delegate successful
// allocations to ordinary new so production delete uses the matching allocator.
void* operator new(size_t size, const std::nothrow_t&) noexcept {
    ++nothrowCalls;
    if (failNothrow) return nullptr;
    try { return ::operator new(size); }
    catch (const std::bad_alloc&) { return nullptr; }
}
static void require(bool condition, const char* message) {
    if (!condition) { std::fprintf(stderr, "FAIL: %s\n", message); std::exit(1); }
}
static void* faultAlloc(size_t size, dtAllocHint) {
    ++allocationCalls;
    if (failAt && allocationCalls == failAt) return nullptr;
    void* result = std::malloc(size);
    if (result) ++liveAllocations;
    return result;
}
static void faultFree(void* p) {
    if (p) {
        require(liveAllocations > 0, "allocation accounting underflow");
        --liveAllocations;
        std::free(p);
    }
}
static void resetFault(size_t ordinal = 0) {
    allocationCalls = 0;
    failAt = ordinal;
    nothrowCalls = 0;
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
BenillaSourceDirect* benilla_nav_source_direct_create(void* raw, dtPolyRef from, dtPolyRef to,
    const float* start, const float* end, unsigned short include, unsigned short exclude,
    dtStatus* status);
}

static void recoveredSearch(void* nav, dtPolyRef a, dtPolyRef b,
    const float* start, const float* end, size_t baseline, int cancelAfter) {
    resetFault();
    dtStatus status = 0;
    BenillaSourceDirect* state = benilla_nav_source_direct_create(nav, a, b, start, end, 1, 0, &status);
    require(state && status == DT_IN_PROGRESS, "create usable independent search");
    require(allocationCalls == 4 && nothrowCalls == 1, "exactly four backing and one outer allocation");
    unsigned work = 0;
    while (status == DT_IN_PROGRESS && (cancelAfter < 0 || work < unsigned(cancelAfter))) {
        int done = 0;
        status = benilla_source_direct_step(state, &done);
        require(done == 1, "one original expansion per step");
        require(++work <= 4096, "bounded recovered search");
    }
    if (cancelAfter < 0) {
        require(status == DT_SUCCESS, "recovered search reaches target");
        dtPolyRef path[74] = {};
        int count = 0;
        require(benilla_source_direct_finish(state, path, &count, 74) == DT_SUCCESS,
            "recovered search finalizes successfully");
        require(count == 4 && path[0] == a && path[count - 1] == b, "authored four-polygon path");
    } else {
        require(status == DT_IN_PROGRESS, "cancellation discards pending search");
    }
    benilla_source_direct_free(state);
    require(liveAllocations == baseline, "complete or cancelled state releases all backing allocations");
}

int main(int argc, char** argv) {
    require(argc == 2, "supply authored fixture directory");
    const std::string fixtures = argv[1];
    const auto params = read(fixtures + "/authored.mmap");
    const auto left = read(fixtures + "/0003232.mmtile");
    const auto right = read(fixtures + "/0003231.mmtile");
    require(params.size() == 28 && left.size() > 120 && right.size() > 120, "authored framing");
    dtAllocSetCustom(faultAlloc, faultFree);
    resetFault();
    void* nav = benilla_nav_create(params.data()); require(nav, "create production mesh");
    require(benilla_nav_add(nav, left.data() + 20, int(left.size() - 20)) == DT_SUCCESS,
        "load authored left tile through production aligned bridge");
    require(benilla_nav_add(nav, right.data() + 20, int(right.size() - 20)) == DT_SUCCESS,
        "load authored right tile through production aligned bridge");
    const size_t baseline = liveAllocations;
    const float start[3] = {std::bit_cast<float>(uint32_t(0xc3fd5555)), 0,
        std::bit_cast<float>(uint32_t(0xc3fd5555))};
    const float end[3] = {std::bit_cast<float>(uint32_t(0x43d55554)), 0,
        std::bit_cast<float>(uint32_t(0xc3855555))};
    const float extents[3] = {5, 5, 5};
    float nearest[3] = {}; unsigned char over = 0; dtPolyRef a = 0, b = 0;
    require(benilla_nav_source_nearest(nav, start, extents, 1, 0, &a, nearest, &over) == DT_SUCCESS && a,
        "nearest start receipt");
    require(benilla_nav_source_nearest(nav, end, extents, 1, 0, &b, nearest, &over) == DT_SUCCESS && b && b != a,
        "nearest end receipt");
    recoveredSearch(nav, a, b, start, end, baseline, -1);
    for (size_t ordinal = 1; ordinal <= 4; ++ordinal) {
        resetFault(ordinal);
        dtStatus status = 0;
        BenillaSourceDirect* state = benilla_nav_source_direct_create(nav, a, b, start, end, 1, 0, &status);
        require(!state && status == (DT_FAILURE | DT_OUT_OF_MEMORY), "backing allocation failure reported");
        require(allocationCalls == 4 && nothrowCalls == 1, "all expected constructor allocations attempted");
        require(liveAllocations == baseline, "partial construction releases every successful backing allocation");
        recoveredSearch(nav, a, b, start, end, baseline, -1);
    }
    resetFault(); failNothrow = true;
    dtStatus status = 0;
    BenillaSourceDirect* state = benilla_nav_source_direct_create(nav, a, b, start, end, 1, 0, &status);
    require(!state && status == (DT_FAILURE | DT_OUT_OF_MEMORY), "outer allocation failure reported");
    require(nothrowCalls == 1 && allocationCalls == 0 && liveAllocations == baseline,
        "failed outer allocation performs no backing allocations");
    recoveredSearch(nav, a, b, start, end, baseline, -1);
    for (int cancelAfter = 0; cancelAfter <= 2; ++cancelAfter) {
        recoveredSearch(nav, a, b, start, end, baseline, cancelAfter);
        recoveredSearch(nav, a, b, start, end, baseline, -1);
    }
    resetFault();
    benilla_nav_free(nav);
    require(liveAllocations == 0, "all tracked Detour allocations released");
    dtAllocSetCustom(nullptr, nullptr);
    std::puts("PASS: all 4 direct backing allocation failures, outer nothrow failure, pending cancellation, successful reuse, zero live Detour allocations");
}
