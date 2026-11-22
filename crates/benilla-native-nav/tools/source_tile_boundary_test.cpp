// SPDX-License-Identifier: GPL-3.0-or-later
// Authored Linux native-boundary tests, not an independent source oracle.
#include "DetourNavMesh.h"
#include <array>
#include <bit>
#include <cfenv>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <limits>
#include <string>
#include <vector>
#include <xmmintrin.h>
extern "C" {
void* benilla_nav_create(const unsigned char* params);
void benilla_nav_free(void* raw);
unsigned benilla_nav_add(void* raw, const unsigned char* bytes, int size);
unsigned benilla_nav_source_tile_presence(const void* raw, const float* world,
    int* coordinates, unsigned char* present);
}
static void require(bool value, const char* why) {
    if (!value) { std::fprintf(stderr, "FAIL: %s\n", why); std::exit(1); }
}
static std::vector<unsigned char> read(const std::string& path) {
    FILE* f = std::fopen(path.c_str(), "rb"); require(f, "open fixture");
    require(std::fseek(f, 0, SEEK_END) == 0, "seek fixture");
    const long size = std::ftell(f); require(size > 0 && size < 16777216, "fixture size");
    require(std::fseek(f, 0, SEEK_SET) == 0, "rewind fixture");
    std::vector<unsigned char> data(static_cast<size_t>(size));
    require(std::fread(data.data(), 1, data.size(), f) == data.size(), "read fixture");
    require(std::fclose(f) == 0, "close fixture"); return data;
}
int main(int argc, char** argv) {
    require(argc == 2, "fixture directory");
    const std::string root = argv[1];
    const auto params = read(root + "/authored.mmap");
    const auto tile = read(root + "/0003232.mmtile");
    void* mesh = benilla_nav_create(params.data()); require(mesh, "create mesh");
    require(benilla_nav_add(mesh, tile.data() + 20, static_cast<int>(tile.size() - 20)) == DT_SUCCESS, "add tile");
    const float point[3] = {-400, -10, 0};
    int coordinates[2] = {999, 999}; unsigned char present = 255;
    auto call = [&](const float* p = nullptr) {
        return benilla_nav_source_tile_presence(mesh, p ? p : point, coordinates, &present);
    };
    require(call() == DT_SUCCESS && coordinates[0] == 0 && coordinates[1] == 0 && present == 1, "loaded actual tile");
    const float absent[3] = {-400, 1, 0};
    require(call(absent) == DT_SUCCESS && coordinates[0] == 1 && coordinates[1] == 0 && present == 0, "absent original endpoint tile");
    for (float invalid : {std::numeric_limits<float>::infinity(), -std::numeric_limits<float>::infinity(),
                         std::numeric_limits<float>::quiet_NaN(), std::numeric_limits<float>::max(), 100001.f}) {
        for (int axis = 0; axis != 3; ++axis) {
            float p[3]; std::memcpy(p, point, sizeof p); p[axis] = invalid;
            require(call(p) == (DT_FAILURE | DT_INVALID_PARAM), "reject unsafe float before native cast");
        }
    }
    require(benilla_nav_source_tile_presence(nullptr, point, coordinates, &present) == (DT_FAILURE | DT_INVALID_PARAM), "null owner");
    require(benilla_nav_source_tile_presence(mesh, nullptr, coordinates, &present) == (DT_FAILURE | DT_INVALID_PARAM), "null point");
    require(benilla_nav_source_tile_presence(mesh, point, nullptr, &present) == (DT_FAILURE | DT_INVALID_PARAM), "null coordinates");
    require(benilla_nav_source_tile_presence(mesh, point, coordinates, nullptr) == (DT_FAILURE | DT_INVALID_PARAM), "null presence");
    const int originalRound = std::fegetround(); const unsigned originalControl = _mm_getcsr();
    require(originalRound == FE_TONEAREST && (originalControl & 0xe040u) == 0, "canonical test environment");
    for (int rounding : {FE_DOWNWARD, FE_UPWARD, FE_TOWARDZERO}) {
        require(std::fesetround(rounding) == 0, "set rounding test environment");
        coordinates[0] = coordinates[1] = 999; present = 255;
        require(call() == 0 && std::fegetround() == rounding, "reject rounding without changing it");
        require(coordinates[0] == 999 && coordinates[1] == 999 && present == 255, "no output on unsupported environment");
    }
    require(std::fesetround(originalRound) == 0, "restore rounding");
    for (unsigned control : {0x8000u, 0x40u, 0x8040u, 0x2000u, 0x4000u, 0x6000u}) {
        _mm_setcsr(originalControl | control);
        require(call() == 0 && _mm_getcsr() == (originalControl | control), "reject FTZ/DAZ/SSE rounding without changing it");
    }
    _mm_setcsr(originalControl);
    require(call() == DT_SUCCESS && present == 1, "usable after floating environment restored");
    benilla_nav_free(mesh);
    std::puts("source tile boundary: nulls, unsafe floats, rounding, FTZ/DAZ and recovery passed");
}
