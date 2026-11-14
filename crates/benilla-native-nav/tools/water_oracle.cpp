// SPDX-License-Identifier: GPL-3.0-or-later
// Authored adjacent ground/water polygons, queried by the pinned original Detour.
#include "DetourNavMesh.h"
#include "DetourNavMeshQuery.h"
#include "source_nav_flags.h"
#include <cassert>
#include <cstdio>
#include <cstring>
#include <fstream>
#include <iterator>
#include <string>
#include <vector>
static std::vector<unsigned char> read(const std::string& path) {
    std::ifstream f(path, std::ios::binary); assert(f);
    return {std::istreambuf_iterator<char>(f), std::istreambuf_iterator<char>()};
}
static void write(const std::string& path, const void* data, size_t n) {
    FILE* f = fopen(path.c_str(), "wb"); assert(f);
    assert(fwrite(data, n, 1, f) == 1); assert(fclose(f) == 0);
}
int main(int argc, char** argv) {
    assert(argc == 3);
    const std::string fixtures = argv[1], out = argv[2];
    auto params = read(fixtures + "/authored.mmap");
    auto tile = read(fixtures + "/0003232.mmtile");
    dtMeshHeader h; memcpy(&h, tile.data() + 20, sizeof h);
    const size_t polys = 120 + size_t(h.vertCount) * 12;
    // The existing two connected quads become ground, water. Other islands stay ground.
    const unsigned short water = NAV_WATER;
    memcpy(tile.data() + polys + 32 + 28, &water, sizeof water);
    tile[polys + 32 + 31] = NAV_AREA_WATER;
    write(out + "/mixed.mmtile", tile.data(), tile.size());
    dtNavMeshParams p; memcpy(&p, params.data(), sizeof p);
    const size_t linkOffset = 100 + size_t(h.vertCount) * 12 + size_t(h.polyCount) * 32;
    std::vector<unsigned char> storage(tile.size() + 8);
    auto* data = storage.data() + (linkOffset % 8 ? 4 : 0);
    memcpy(data, tile.data() + 20, tile.size() - 20);
    auto right = read(fixtures + "/0003231.mmtile");
    dtMeshHeader rh; memcpy(&rh, right.data() + 20, sizeof rh);
    const size_t rightLinks = 100 + size_t(rh.vertCount) * 12 + size_t(rh.polyCount) * 32;
    std::vector<unsigned char> rightStorage(right.size() + 8);
    auto* rightData = rightStorage.data() + (rightLinks % 8 ? 4 : 0);
    memcpy(rightData, right.data() + 20, right.size() - 20);
    dtNavMesh mesh; assert(mesh.init(&p) == DT_SUCCESS);
    assert(mesh.addTile(data, int(tile.size() - 20), 0, 0, nullptr) == DT_SUCCESS);
    assert(mesh.addTile(rightData, int(right.size() - 20), 0, 0, nullptr) == DT_SUCCESS);
    dtNavMeshQuery query; assert(query.init(&mesh, 512) == DT_SUCCESS);
    FILE* f = fopen((out + "/cases.tsv").c_str(), "w"); assert(f);
    fprintf(f, "# label include exclude water_ref route_status route_count cross_status cross_count\n");
    const float grid = 533.3333f;
    const float ground[3] = {-grid + 20 * grid / 100, 0, -grid + 20 * grid / 100};
    const float wet[3] = {-grid + 80 * grid / 100, 0, -grid + 20 * grid / 100};
    const float beyond[3] = {20 * grid / 100, 2, -grid + 20 * grid / 100};
    const float ext[3] = {5,5,5};
    struct Case { const char* label; unsigned short include, exclude; };
    // PathFinder.cpp:1064-1083, without PLAYERBOTS. The final two isolate exclude precedence.
    for (auto c : {Case{"walker", NAV_GROUND, 0},
                   Case{"swimmer", NAV_GROUND | NAV_WATER | NAV_MAGMA_SLIME, 0},
                   Case{"player", NAV_GROUND | NAV_WATER, 0},
                   Case{"exclude_water", NAV_GROUND | NAV_WATER, NAV_WATER},
                   Case{"exclude_ground", NAV_GROUND | NAV_WATER, NAV_GROUND}}) {
        dtQueryFilter filter; filter.setIncludeFlags(c.include); filter.setExcludeFlags(c.exclude);
        dtPolyRef a = 0, b = 0; float an[3]{}, bn[3]{};
        assert(query.findNearestPoly(ground,ext,&filter,&a,an) == DT_SUCCESS);
        assert(query.findNearestPoly(wet,ext,&filter,&b,bn) == DT_SUCCESS);
        dtStatus status = 0; dtPolyRef corridor[256]{}; int count = 0;
        if (a && b) status = query.findPath(a,b,an,bn,&filter,corridor,&count,256);
        dtPolyRef end = 0; float en[3]{};
        assert(query.findNearestPoly(beyond,ext,&filter,&end,en) == DT_SUCCESS);
        dtStatus cross = 0; int crossCount = 0;
        if (a && end) cross = query.findPath(a,end,an,en,&filter,corridor,&crossCount,256);
        fprintf(f, "%s %u %u %llu %u %d %u %d\n",c.label,c.include,c.exclude,
                static_cast<unsigned long long>(b),status,count,cross,crossCount);
    }
    assert(fclose(f) == 0);
}
