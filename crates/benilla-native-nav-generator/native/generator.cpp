/*
 * This file is part of the CMaNGOS Project. See AUTHORS file for Copyright information
 *
 * This program is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation; either version 2 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program; if not, write to the Free Software
 * Foundation, Inc., 59 Temple Place, Suite 330, Boston, MA  02111-1307  USA
 */

// ADAPTED by Benilla: pinned MapBuilder stages over a bounded mesh-only process
// protocol. No asset paths, ambient configuration, file writes or network access.
#include "Recast.h"
#include "RecastAlloc.h"
#include "DetourAlloc.h"
#include "DetourNavMesh.h"
#include "DetourNavMeshBuilder.h"
#include <algorithm>
#include <array>
#include <bit>
#include <cfloat>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <limits>
#include <memory>
#include <new>
#include <stdexcept>
#include <vector>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif

namespace {
constexpr float GRID_SIZE = 533.33333f;
constexpr float BASE_UNIT_DIM = 0.2666666f;
constexpr int VERTEX_PER_TILE = 80;
constexpr int TILES_PER_MAP = 25;
constexpr std::size_t MAX_BYTES = 32 * 1024 * 1024;
constexpr std::size_t MAX_NATIVE_BYTES = 256 * 1024 * 1024;
constexpr unsigned char NAV_AREA_GROUND = 11;
constexpr unsigned char NAV_AREA_GROUND_STEEP = 10;
static_assert(sizeof(float) == 4 && sizeof(int) == 4 && sizeof(dtNavMeshParams) == 28);
static_assert(std::endian::native == std::endian::little, "source wire files require little endian");
static_assert(std::numeric_limits<float>::is_iec559);
static_assert(sizeof(dtMeshHeader) == 100 && sizeof(dtPoly) == 32 && sizeof(dtLink) == 16);
static_assert(alignof(dtLink) == 8 && alignof(dtMeshHeader) == 4 && alignof(dtPoly) == 4);
static_assert(alignof(dtPolyDetail) == 4 && alignof(dtBVNode) == 4 && alignof(float) == 4);

void require(bool condition, const char* reason) {
    if (!condition) throw std::runtime_error(reason);
}

// All hooks are private to this single-request executable. Throwing rather than
// returning null also protects upstream rcVector paths that do not check null.
// The intrusive registry reclaims any upstream local temporaries left by unwind.
struct alignas(std::max_align_t) Allocation {
    Allocation* previous;
    Allocation* next;
    std::size_t size;
};
struct AllocationState {
    Allocation* head = nullptr;
    std::size_t live = 0, peak = 0, limit = 0;
    bool exceeded = false;
} allocation;
void* boundedAlloc(std::size_t size) {
    size = std::max<std::size_t>(size, 1);
    if (size > allocation.limit || sizeof(Allocation) > allocation.limit - size ||
        allocation.live > allocation.limit - size - sizeof(Allocation)) {
        allocation.exceeded = true;
        throw std::bad_alloc();
    }
    const std::size_t total = size + sizeof(Allocation);
    auto* block = static_cast<Allocation*>(std::malloc(total));
    if (!block) throw std::bad_alloc();
    block->previous = nullptr;
    block->next = allocation.head;
    block->size = total;
    if (allocation.head) allocation.head->previous = block;
    allocation.head = block;
    allocation.live += total;
    allocation.peak = std::max(allocation.peak, allocation.live);
    // Zeroed payload makes partially initialized upstream owner objects safe to
    // unwind when an allocation throws before assigning their next pointer.
    std::memset(block + 1, 0, size);
    return block + 1;
}
void boundedFree(void* pointer) {
    if (!pointer) return;
    auto* block = static_cast<Allocation*>(pointer) - 1;
    if (block->previous) block->previous->next = block->next;
    else allocation.head = block->next;
    if (block->next) block->next->previous = block->previous;
    allocation.live -= block->size;
    std::free(block);
}
void* recastAlloc(std::size_t size, rcAllocHint) { return boundedAlloc(size); }
void* detourAlloc(std::size_t size, dtAllocHint) { return boundedAlloc(size); }
struct Hooks {
    explicit Hooks(std::size_t limit) {
        allocation = {};
        allocation.limit = limit;
        rcAllocSetCustom(recastAlloc, boundedFree);
        dtAllocSetCustom(detourAlloc, boundedFree);
    }
    ~Hooks() {
        while (allocation.head) boundedFree(allocation.head + 1);
        rcAllocSetCustom(nullptr, nullptr);
        dtAllocSetCustom(nullptr, nullptr);
    }
};

class Reader {
    const unsigned char* data;
    std::size_t length, position = 0;
public:
    Reader(const unsigned char* bytes, std::size_t size) : data(bytes), length(size) {
        require(size <= MAX_BYTES, "request exceeds byte limit");
    }
    const unsigned char* take(std::size_t size) {
        require(size <= length - position, "truncated request");
        const unsigned char* result = data + position;
        position += size;
        return result;
    }
    std::uint32_t u32() {
        std::uint32_t value;
        std::memcpy(&value, take(4), 4);
        return value;
    }
    std::uint64_t u64() {
        std::uint64_t value;
        std::memcpy(&value, take(8), 8);
        return value;
    }
    float f32() {
        float value;
        std::memcpy(&value, take(4), 4);
        require(std::isfinite(value), "nonfinite float");
        return value;
    }
    int boundedInt(unsigned limit, const char* message) {
        unsigned value = u32();
        require(value <= limit, message);
        return static_cast<int>(value);
    }
    void finish() { require(position == length, "trailing request bytes"); }
};
struct Mesh {
    std::vector<float> verts;
    std::vector<int> tris;
    int vertexCount() const { return static_cast<int>(verts.size() / 3); }
    int triangleCount() const { return static_cast<int>(tris.size() / 3); }
};
Mesh readMesh(Reader& input) {
    Mesh mesh;
    const unsigned nv = input.u32(), nt = input.u32();
    require(nv <= 500000 && nt <= 1000000, "mesh count exceeds limit");
    require((nv == 0) == (nt == 0), "unclean mesh: vertices and triangles must both be present");
    const unsigned char* vertexData = input.take(static_cast<std::size_t>(nv) * 12);
    const unsigned char* triangleData = input.take(static_cast<std::size_t>(nt) * 12);
    mesh.verts.resize(static_cast<std::size_t>(nv) * 3);
    mesh.tris.resize(static_cast<std::size_t>(nt) * 3);
    if (nv) std::memcpy(mesh.verts.data(), vertexData, static_cast<std::size_t>(nv) * 12);
    if (nt) std::memcpy(mesh.tris.data(), triangleData, static_cast<std::size_t>(nt) * 12);
    for (float v : mesh.verts) require(std::isfinite(v) && std::abs(v) <= 100000.0f, "mesh coordinate outside bounds");
    for (int index : mesh.tris) require(index >= 0 && static_cast<unsigned>(index) < nv, "triangle index outside vertex array");
    return mesh;
}
struct Request {
    unsigned mapId, tileX, tileY, usesLiquids;
    std::size_t budget;
    dtNavMeshParams map{};
    float bmin[3]{}, bmax[3]{};
    rcConfig config{};
    Mesh solid, liquid;
    std::vector<unsigned char> liquidAreas;
};
Request readRequest(const unsigned char* data, std::size_t size) {
    Reader input(data, size);
    require(std::memcmp(input.take(8), "BNAVIN01", 8) == 0, "invalid protocol magic");
    Request request;
    request.mapId = input.u32(); request.tileX = input.u32(); request.tileY = input.u32();
    request.usesLiquids = input.u32();
    require(request.mapId <= 999 && request.tileX < 64 && request.tileY < 64 && request.usesLiquids <= 1, "invalid map/tile identity or liquid boolean");
    const std::uint64_t budget = input.u64();
    require(budget > 0 && budget <= MAX_NATIVE_BYTES, "native allocation budget outside bounds");
    request.budget = static_cast<std::size_t>(budget);
    for (float& v : request.map.orig) { v = input.f32(); require(std::abs(v) <= 20000.0f, "map origin outside bounds"); }
    require(request.map.orig[1] == FLT_MIN, "source map origin Y must be FLT_MIN");
    request.map.tileWidth = input.f32(); request.map.tileHeight = input.f32();
    require(request.map.tileWidth == GRID_SIZE && request.map.tileHeight == GRID_SIZE, "source map tile size mismatch");
    request.map.maxTiles = input.boundedInt(4096, "map tile count exceeds limit");
    require(request.map.maxTiles > 0, "empty full-map tile union");
    const std::uint32_t maxPolys = input.u32();
    require(maxPolys == 0x80000000u, "source maxPolys raw bits mismatch");
    std::memcpy(&request.map.maxPolys, &maxPolys, 4);
    for (float& v : request.bmin) v = input.f32();
    for (float& v : request.bmax) v = input.f32();
    const float xmax = (32 - static_cast<int>(request.tileX)) * GRID_SIZE;
    const float zmax = (32 - static_cast<int>(request.tileY)) * GRID_SIZE;
    require(request.bmax[0] == xmax && request.bmin[0] == xmax - GRID_SIZE &&
            request.bmax[2] == zmax && request.bmin[2] == zmax - GRID_SIZE, "selected tile horizontal bounds differ from source");
    const bool boundedVertical = std::abs(request.bmin[1]) <= 100000.0f && std::abs(request.bmax[1]) <= 100000.0f &&
            request.bmax[1] >= request.bmin[1] && request.bmax[1] - request.bmin[1] <= 16000.0f;
    auto& c = request.config;
    c.tileSize = VERTEX_PER_TILE;
    c.cs = c.ch = BASE_UNIT_DIM;
    c.borderSize = input.boundedInt(32, "border size exceeds limit");
    c.walkableSlopeAngle = input.f32();
    require(c.walkableSlopeAngle >= 0 && c.walkableSlopeAngle < 90, "slope outside bounds");
    c.walkableHeight = input.boundedInt(255, "walkable height exceeds limit");
    require(c.walkableHeight >= 1, "walkable height must be positive");
    c.walkableClimb = input.boundedInt(255, "walkable climb exceeds limit");
    c.walkableRadius = input.boundedInt(32, "walkable radius exceeds limit");
    c.maxEdgeLen = input.boundedInt(4096, "max edge length exceeds limit");
    c.maxSimplificationError = input.f32();
    require(c.maxSimplificationError >= 0 && c.maxSimplificationError <= 100, "simplification error outside bounds");
    c.minRegionArea = rcSqr(input.boundedInt(1024, "minimum region area exceeds limit"));
    c.mergeRegionArea = rcSqr(input.boundedInt(1024, "merge region area exceeds limit"));
    c.maxVertsPerPoly = DT_VERTS_PER_POLYGON;
    c.detailSampleDist = input.f32(); c.detailSampleMaxError = input.f32(); c.liquidFlagMergeThreshold = input.f32();
    require(c.detailSampleDist == 0 || (c.detailSampleDist >= 0.1f && c.detailSampleDist <= 100), "detail sample distance outside bounds");
    require(c.detailSampleMaxError >= 0 && c.detailSampleMaxError <= 100, "detail sample error outside bounds");
    require(c.liquidFlagMergeThreshold >= 0 && c.liquidFlagMergeThreshold <= 100, "liquid merge threshold outside bounds");
    request.solid = readMesh(input); request.liquid = readMesh(input);
    const unsigned char* areas = input.take(request.liquid.triangleCount());
    request.liquidAreas.assign(areas, areas + request.liquid.triangleCount());
    for (auto area : request.liquidAreas) require(area == 8 || area == 9, "unsupported source liquid area");
    require(request.usesLiquids || request.liquidAreas.empty(), "liquid geometry present when liquid collection disabled");
    input.finish();
    if (!request.solid.verts.empty() || !request.liquid.verts.empty()) {
        require(boundedVertical, "vertical range outside limits");
        float low = FLT_MAX, high = -FLT_MAX;
        for (const Mesh* mesh : {&request.liquid, &request.solid}) {
            for (std::size_t i = 1; i < mesh->verts.size(); i += 3) {
                low = std::min(low, mesh->verts[i]); high = std::max(high, mesh->verts[i]);
            }
        }
        require(request.bmin[1] == low && request.bmax[1] == high, "vertical bounds do not match cleaned mesh");
    } else {
        // The original no-vertex bounds sentinel never enters rasterization.
        require(boundedVertical || (request.bmin[1] == FLT_MIN && request.bmax[1] == FLT_MAX), "invalid empty-mesh vertical bounds");
    }
    const float tx = (((request.bmin[0] + request.bmax[0]) / 2) - request.map.orig[0]) / GRID_SIZE;
    const float ty = (((request.bmin[2] + request.bmax[2]) / 2) - request.map.orig[2]) / GRID_SIZE;
    require(tx >= 0 && tx < 64 && ty >= 0 && ty < 64, "selected tile outside map origin domain");
    return request;
}

struct Tile {
    rcCompactHeightfield* chf = nullptr;
    rcHeightfield* solid = nullptr;
    rcContourSet* cset = nullptr;
    rcPolyMesh* pmesh = nullptr;
    rcPolyMeshDetail* dmesh = nullptr;
    ~Tile() {
        rcFreeCompactHeightfield(chf); rcFreeContourSet(cset); rcFreeHeightField(solid);
        rcFreePolyMesh(pmesh); rcFreePolyMeshDetail(dmesh);
    }
};

// Source rcModAlmostUnwalkableTriangles, with the same <= comparison and 50° pass.
void markSteep(const float* verts, const int* tris, int count, unsigned char* areas) {
    const float walkableThr = cosf(50.0f / 180.0f * RC_PI);
    for (int i = 0; i < count; ++i) {
        if (areas[i] & RC_WALKABLE_AREA) {
            const int* tri = &tris[i * 3];
            float e0[3], e1[3], norm[3];
            rcVsub(e0, &verts[tri[1] * 3], &verts[tri[0] * 3]);
            rcVsub(e1, &verts[tri[2] * 3], &verts[tri[0] * 3]);
            rcVcross(norm, e0, e1); rcVnormalize(norm);
            if (norm[1] <= walkableThr) areas[i] = NAV_AREA_GROUND_STEEP;
        }
    }
}
void buildCommonTile(rcContext& context, Tile& tile, const rcConfig& config, const Request& request) {
    const Mesh& solid = request.solid;
    const Mesh& liquid = request.liquid;
    tile.solid = rcAllocHeightfield();
    require(tile.solid && rcCreateHeightfield(&context, *tile.solid, config.width, config.height, config.bmin, config.bmax, config.cs, config.ch), "heightfield creation failed");
    std::vector<unsigned char> flags(solid.triangleCount(), NAV_AREA_GROUND);
    rcClearUnwalkableTriangles(&context, config.walkableSlopeAngle, solid.verts.data(), solid.vertexCount(), solid.tris.data(), solid.triangleCount(), flags.data());
    markSteep(solid.verts.data(), solid.tris.data(), solid.triangleCount(), flags.data());
    require(rcRasterizeTriangles(&context, solid.verts.data(), solid.vertexCount(), solid.tris.data(), flags.data(), solid.triangleCount(), *tile.solid, config.walkableClimb), "solid rasterization failed");
    rcFilterLowHangingWalkableObstacles(&context, config.walkableClimb, *tile.solid);
    rcFilterLedgeSpans(&context, config.walkableHeight, config.walkableClimb, *tile.solid);
    rcFilterWalkableLowHeightSpans(&context, config.walkableHeight, *tile.solid);
    if (!liquid.verts.empty())
        require(rcRasterizeTriangles(&context, liquid.verts.data(), liquid.vertexCount(), liquid.tris.data(), request.liquidAreas.data(), liquid.triangleCount(), *tile.solid, config.walkableClimb), "liquid rasterization failed");
    tile.chf = rcAllocCompactHeightfield();
    require(tile.chf && rcBuildCompactHeightfield(&context, config.walkableHeight, config.walkableClimb, *tile.solid, *tile.chf), "compact heightfield failed");
    require(rcErodeWalkableArea(&context, config.walkableRadius, *tile.chf), "erosion failed");
    require(rcMedianFilterWalkableArea(&context, *tile.chf), "median area filter failed");
    require(rcBuildDistanceField(&context, *tile.chf), "distance field failed");
    require(rcBuildRegions(&context, *tile.chf, config.borderSize, config.minRegionArea, config.mergeRegionArea), "region build failed");
    tile.cset = rcAllocContourSet();
    require(tile.cset && rcBuildContours(&context, *tile.chf, config.maxSimplificationError, config.maxEdgeLen, *tile.cset), "contour build failed");
    tile.pmesh = rcAllocPolyMesh();
    require(tile.pmesh && rcBuildPolyMesh(&context, *tile.cset, config.maxVertsPerPoly, *tile.pmesh), "polygon build failed");
    tile.dmesh = rcAllocPolyMeshDetail();
    require(tile.dmesh && rcBuildPolyMeshDetail(&context, *tile.pmesh, *tile.chf, config.detailSampleDist, config.detailSampleMaxError, *tile.dmesh), "detail mesh build failed");
    rcFreeHeightField(tile.solid); tile.solid = nullptr;
    rcFreeCompactHeightfield(tile.chf); tile.chf = nullptr;
    rcFreeContourSet(tile.cset); tile.cset = nullptr;
}

void append(std::vector<unsigned char>& out, const void* data, std::size_t size) {
    require(size <= MAX_BYTES - out.size(), "response exceeds byte limit");
    const auto* bytes = static_cast<const unsigned char*>(data);
    out.insert(out.end(), bytes, bytes + size);
}
std::vector<unsigned char> response(unsigned status, const Request& request, unsigned polygons,
                                    const unsigned char* navData = nullptr, unsigned navSize = 0) {
    std::vector<unsigned char> out;
    require(navSize <= MAX_BYTES - 80, "navmesh output exceeds byte limit");
    out.reserve(60 + (navData ? navSize + 20 : 0));
    append(out, "BNAVOUT1", 8);
    const std::uint32_t header[] = {status, 28, navData ? navSize + 20 : 0, polygons};
    append(out, header, sizeof(header));
    const std::uint64_t peak = allocation.peak;
    append(out, &peak, 8); append(out, &request.map, sizeof(request.map));
    if (navData) {
        const std::uint32_t mmapHeader[] = {0x4d4d4150, DT_NAVMESH_VERSION, 8, navSize, request.usesLiquids};
        append(out, mmapHeader, sizeof(mmapHeader)); append(out, navData, navSize);
    }
    return out;
}

std::vector<unsigned char> generate(const Request& request) {
    // This owner encloses navMesh's lifetime, including failure unwinds with flags=0.
    std::unique_ptr<unsigned char, decltype(&dtFree)> alignedOwner(nullptr, dtFree);
    bool initialized = false;
    const auto releaseNavMesh = [&initialized](dtNavMesh* mesh) {
        if (initialized) dtFreeNavMesh(mesh);
        else {
            // Pinned init sets m_maxTiles before allocating m_tiles; its destructor
            // cannot handle a throw there. This class has only scalar/pointer members.
            // Free its raw storage; Hooks reclaims any partial child allocations.
            dtFree(mesh);
        }
    };
    std::unique_ptr<dtNavMesh, decltype(releaseNavMesh)> navMesh(dtAllocNavMesh(), releaseNavMesh);
    require(navMesh && dtStatusSucceed(navMesh->init(&request.map)), "full-map Detour initialization failed");
    initialized = true;
    if (request.solid.verts.empty() && request.liquid.verts.empty()) return response(1, request, 0);
    rcContext context(false);
    rcConfig config = request.config;
    rcVcopy(config.bmin, request.bmin); rcVcopy(config.bmax, request.bmax);
    rcCalcGridSize(config.bmin, config.bmax, config.cs, &config.width, &config.height);
    std::array<Tile, TILES_PER_MAP * TILES_PER_MAP> tiles;
    rcConfig tileConfig = config;
    tileConfig.width = config.tileSize + config.borderSize * 2;
    tileConfig.height = config.tileSize + config.borderSize * 2;
    for (int y = 0; y < TILES_PER_MAP; ++y) {
        for (int x = 0; x < TILES_PER_MAP; ++x) {
            tileConfig.bmin[0] = config.bmin[0] + x * float(config.tileSize * config.cs);
            tileConfig.bmin[2] = config.bmin[2] + y * float(config.tileSize * config.cs);
            tileConfig.bmax[0] = config.bmin[0] + (x + 1) * float(config.tileSize * config.cs);
            tileConfig.bmax[2] = config.bmin[2] + (y + 1) * float(config.tileSize * config.cs);
            tileConfig.bmin[0] -= tileConfig.borderSize * tileConfig.cs;
            tileConfig.bmin[2] -= tileConfig.borderSize * tileConfig.cs;
            tileConfig.bmax[0] += tileConfig.borderSize * tileConfig.cs;
            tileConfig.bmax[2] += tileConfig.borderSize * tileConfig.cs;
            buildCommonTile(context, tiles[x + y * TILES_PER_MAP], tileConfig, request);
        }
    }
    std::array<rcPolyMesh*, TILES_PER_MAP * TILES_PER_MAP> meshes{};
    std::array<rcPolyMeshDetail*, TILES_PER_MAP * TILES_PER_MAP> details{};
    int count = 0;
    for (int y = 0; y < TILES_PER_MAP; ++y) {
        for (int x = 0; x < TILES_PER_MAP; ++x) {
            Tile& tile = tiles[x + y * TILES_PER_MAP];
            if (tile.pmesh) { meshes[count] = tile.pmesh; details[count++] = tile.dmesh; }
        }
    }
    std::unique_ptr<rcPolyMesh, decltype(&rcFreePolyMesh)> merged(rcAllocPolyMesh(), rcFreePolyMesh);
    require(merged && rcMergePolyMeshes(&context, meshes.data(), count, *merged), "polygon merge failed");
    std::unique_ptr<rcPolyMeshDetail, decltype(&rcFreePolyMeshDetail)> detail(rcAllocPolyMeshDetail(), rcFreePolyMeshDetail);
    require(detail && rcMergePolyMeshDetails(&context, details.data(), count, *detail), "detail merge failed");
    // The source frees all subtile results immediately after merging, before Detour creation.
    for (Tile& tile : tiles) {
        rcFreePolyMesh(tile.pmesh); tile.pmesh = nullptr;
        rcFreePolyMeshDetail(tile.dmesh); tile.dmesh = nullptr;
    }
    for (int i = 0; i < merged->npolys; ++i) {
        if (unsigned char area = merged->areas[i] & 0x3f)
            merged->flags[i] = area >= 8 ? 1 << (11 - area) : 1;
    }
    dtNavMeshCreateParams params{};
    params.verts = merged->verts; params.vertCount = merged->nverts;
    params.polys = merged->polys; params.polyAreas = merged->areas;
    params.polyFlags = merged->flags; params.polyCount = merged->npolys; params.nvp = merged->nvp;
    params.detailMeshes = detail->meshes; params.detailVerts = detail->verts; params.detailVertsCount = detail->nverts;
    params.detailTris = detail->tris; params.detailTriCount = detail->ntris;
    params.walkableHeight = BASE_UNIT_DIM * config.walkableHeight;
    params.walkableRadius = BASE_UNIT_DIM * config.walkableRadius;
    params.walkableClimb = BASE_UNIT_DIM * config.walkableClimb;
    params.tileX = (((request.bmin[0] + request.bmax[0]) / 2) - request.map.orig[0]) / GRID_SIZE;
    params.tileY = (((request.bmin[2] + request.bmax[2]) / 2) - request.map.orig[2]) / GRID_SIZE;
    rcVcopy(params.bmin, request.bmin); rcVcopy(params.bmax, request.bmax);
    params.cs = config.cs; params.ch = config.ch; params.tileLayer = 0; params.buildBvTree = true;
    require(params.nvp <= DT_VERTS_PER_POLYGON, "verts-per-polygon exceeds Detour limit");
    require(params.vertCount < 0xffff, "merged vertex count exceeds source limit");
    if (!params.vertCount || !params.verts) return response(1, request, params.polyCount);
    if (!params.polyCount || !params.polys) return response(1, request, params.polyCount);
    if (params.polyCount == TILES_PER_MAP * TILES_PER_MAP) return response(2, request, params.polyCount);
    require(params.detailMeshes && params.detailVerts && params.detailTris, "source detail mesh missing");
    unsigned char* data = nullptr;
    int dataSize = 0;
    require(dtCreateNavMeshData(&params, &data, &dataSize), "Detour tile creation failed");
    std::unique_ptr<unsigned char, decltype(&dtFree)> dataOwner(data, dtFree);
    require(dataSize > 0 && static_cast<std::size_t>(dataSize) <= MAX_BYTES - 80, "Detour output size outside limit");
    // Source serialization aligns blocks to four bytes even when dtLink requires
    // eight. Shift only the allocation base, retaining every serialized byte.
    dtMeshHeader header;
    std::memcpy(&header, data, sizeof(header));
    const std::size_t linksOffset = sizeof(dtMeshHeader) + std::size_t(header.vertCount) * 12 + std::size_t(header.polyCount) * sizeof(dtPoly);
    alignedOwner.reset(static_cast<unsigned char*>(dtAlloc(std::size_t(dataSize) + 8, DT_ALLOC_PERM)));
    unsigned char* alignedData = alignedOwner.get() + (linksOffset % 8 ? 4 : 0);
    require(reinterpret_cast<std::uintptr_t>(alignedData + linksOffset) % alignof(dtLink) == 0, "Detour link alignment failed");
    std::memcpy(alignedData, data, std::size_t(dataSize));
    dataOwner.reset();
    dtTileRef tileRef = 0;
    const dtStatus status = navMesh->addTile(alignedData, dataSize, 0, 0, &tileRef);
    require(tileRef && dtStatusSucceed(status), "generated tile failed Detour addTile");
    auto out = response(0, request, params.polyCount, alignedData, static_cast<unsigned>(dataSize));
    require(dtStatusSucceed(navMesh->removeTile(tileRef, nullptr, nullptr)), "generated tile removal failed");
    return out;
}
} // namespace

extern "C" int benilla_nav_generate(const unsigned char* bytes, std::size_t size) noexcept {
    try {
        const Request request = readRequest(bytes, size);
        Hooks hooks(request.budget);
        const auto result = generate(request);
#ifdef _WIN32
        require(_setmode(_fileno(stdout), _O_BINARY) != -1, "stdout binary mode failed");
#endif
        require(std::fwrite(result.data(), 1, result.size(), stdout) == result.size() && std::fflush(stdout) == 0, "stdout write failed");
        return 0;
    } catch (const std::bad_alloc&) {
        std::fputs(allocation.exceeded ? "native-navigation: native allocation budget exceeded\n" : "native-navigation: allocation failed\n", stderr);
    } catch (const std::exception& error) {
        std::fprintf(stderr, "native-navigation: %.240s\n", error.what());
    } catch (...) {
        std::fputs("native-navigation: unknown native failure\n", stderr);
    }
    return 2;
}
