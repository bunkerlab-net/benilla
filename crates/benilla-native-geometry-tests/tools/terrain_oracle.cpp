// SPDX-License-Identifier: GPL-3.0-or-later
// Authored harness for hash-checked CMaNGOS GridMap methods injected by the generator.
// Original CMaNGOS grants/authors are retained in products/PROVENANCE.md's linked notices.
#include <cfenv>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
using uint16 = uint16_t;
constexpr float SIZE_OF_GRIDS = 533.33333f;
constexpr int MAP_RESOLUTION = 128;
constexpr float INVALID_HEIGHT_VALUE = -100000.0f;
static uint16 holetab_h[4] = {0x1111, 0x2222, 0x4444, 0x8888};
static uint16 holetab_v[4] = {0x000F, 0x00F0, 0x0F00, 0xF000};
struct GridMap {
    uint16 m_holes[16][16] = {};
    float corners[129 * 129] = {};
    float centers[128 * 128] = {};
    float* m_V9 = corners;
    float* m_V8 = centers;
    float m_gridHeight = 7.0f;
    bool isHole(int row, int col) const;
    float getHeightFromFlat(float x, float y) const;
    float getHeightFromFloat(float x, float y) const;
};
// PINNED_UPSTREAM_METHODS
static uint32_t bits(float f) { uint32_t b; std::memcpy(&b, &f, 4); return b; }
static float shape(int k, float x, float y, bool center) {
    switch (k) {
        case 0: return 7.0f;
        case 1: return x / 8.0f + y / 16.0f;
        case 2: return std::abs(x - 64.0f) / 4.0f;
        case 3: return ((x - 64.0f)*(x - 64.0f) + (y - 64.0f)*(y - 64.0f)) / 512.0f;
        case 4: return x < 64.0f ? 0.0f : 20.0f;
        case 5: return center ? 20.0f : 0.0f;
        default: return x / 8.0f + y / 16.0f;
    }
}
int main() {
    std::fesetround(FE_TONEAREST);
    std::puts("# shape tile_x tile_y world_x_bits world_y_bits height_bits; pinned CMaNGOS, authored grids");
    const float probes[] = {0.0f, .125f, .5f, .875f, 1.0f, 1.875f, 2.125f, 7.875f,
                            8.125f, 63.125f, 63.5f, 63.875f, 64.125f, 127.875f};
    const int tiles[][2] = {{0, 0}, {31, 32}, {32, 31}, {63, 63}};
    for (int k = 0; k < 8; ++k) {
        GridMap map;
        for (int i = 0; i < 129; ++i) for (int j = 0; j < 129; ++j)
            map.corners[i * 129 + j] = shape(k, float(i), float(j), false);
        for (int i = 0; i < 128; ++i) for (int j = 0; j < 128; ++j)
            map.centers[i * 128 + j] = shape(k, i + .5f, j + .5f, true);
        if (k >= 6) {
            map.m_holes[0][0] = 0x8001;
            map.m_holes[7][7] = 0x8421;
            map.m_holes[15][15] = 0x8000;
        }
        for (auto &tile : tiles) for (float x : probes) for (float y : probes) {
            float wx = (32.0f - (tile[0] + x / 128.0f)) * SIZE_OF_GRIDS;
            float wy = (32.0f - (tile[1] + y / 128.0f)) * SIZE_OF_GRIDS;
            float h = (k == 0 || k == 7) ? map.getHeightFromFlat(wx, wy) : map.getHeightFromFloat(wx, wy);
            std::printf("%d %d %d %08x %08x %08x\n", k, tile[0], tile[1], bits(wx), bits(wy), bits(h));
        }
    }
}
