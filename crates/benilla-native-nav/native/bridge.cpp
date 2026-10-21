// SPDX-License-Identifier: GPL-3.0-or-later
// Benilla wrapper around pinned Detour. Only the Rust validator may supply tile bytes.
#include "DetourNavMesh.h"
#include "DetourNavMeshQuery.h"
#include "DetourAlloc.h"
#include "DetourNode.h"
#include "DetourCommon.h"
#include "source_smoothing.h"
#include "source_direct.h"
#include "source_warm_cache.h"
#include "source_straight_path.h"
#include <climits>
#include <cstddef>
#include <cstring>
#include <cstdlib>
#include <new>
#include <limits>
#include <type_traits>
#include <bit>
#include <cfenv>
#include <cfloat>
#include <cmath>
#if defined(__x86_64__) || defined(_M_X64)
#include <xmmintrin.h>
#endif
static_assert(std::endian::native == std::endian::little, "little endian ABI");
static_assert(std::is_trivially_default_constructible<dtMeshTile>::value && std::is_trivially_destructible<dtMeshTile>::value, "C++20 tile lifetime");
static_assert(sizeof(dtMeshTile)==104 && alignof(dtMeshTile)==8, "native tile ABI");
static_assert(sizeof(void*) == 8 && sizeof(float) == 4 && sizeof(int) == 4, "ABI");
static_assert(std::numeric_limits<float>::is_iec559, "binary32");
static_assert(FLT_EVAL_METHOD == 0, "source binary32 evaluation without excess precision");
static_assert(sizeof(dtPolyRef)==8 && sizeof(dtTileRef)==8 && sizeof(dtNavMeshParams)==28, "ABI");
static_assert(sizeof(dtMeshHeader)==100 && sizeof(dtPoly)==32 && sizeof(dtLink)==16, "ABI");
static_assert(sizeof(dtPolyDetail)==12 && sizeof(dtBVNode)==16, "ABI");
static_assert(alignof(dtLink)==8 && alignof(dtMeshHeader)==4 && alignof(dtPoly)==4, "alignment");
static_assert(alignof(dtPolyDetail)==4 && alignof(dtBVNode)==4 && alignof(float)==4, "alignment");
static_assert(offsetof(dtPoly, flags)==28 && offsetof(dtPolyDetail, triBase)==4, "ABI");
static_assert(DT_NAVMESH_VERSION==7 && DT_VERTS_PER_POLYGON==6, "version");
static_assert(sizeof(BenillaWarmScan)==16 && alignof(BenillaWarmScan)==8 &&
    offsetof(BenillaWarmScan, minimumSquared)==8 && offsetof(BenillaWarmScan, distance)==12,
    "warm scan ABI");
static_assert(sizeof(BenillaSourceRayResult)==616 && alignof(BenillaSourceRayResult)==8 &&
    offsetof(BenillaSourceRayResult, hit)==592 && offsetof(BenillaSourceRayResult, normal)==596 &&
    offsetof(BenillaSourceRayResult, count)==608 && offsetof(BenillaSourceRayResult, status)==612,
    "straight ray ABI");
struct BenillaNav {
    dtNavMesh* mesh;
    dtNavMeshQuery* query;
    dtQueryFilter filter; // sliced query retains this pointer
    void* allocations[64];
    unsigned count;
    BenillaNav(): mesh(dtAllocNavMesh()), query(dtAllocNavMeshQuery()), allocations{}, count(0) {
        filter.setIncludeFlags(1); // CMaNGOS NAV_GROUND
        filter.setExcludeFlags(0); // validated tiles contain only ground
    }
    ~BenillaNav() {
        dtFreeNavMeshQuery(query);
        dtFreeNavMesh(mesh); // flags=0: never frees our aligned serialized buffers
        for (unsigned i=0;i<count;++i) std::free(allocations[i]);
    }
};
// Read-only attribution only. All coordinates below remain in Detour YZX order.
struct BenillaRetainedDetailHeight {
    unsigned status, height, triangleIndex, triangleCount, triangleTests;
    unsigned polygonIndex, polygonVertexCount;
    unsigned char triangle[4];
    unsigned vertexIndices[3], vertices[3][3];
    int tileCoordinates[3];
    unsigned boundsMin[3], boundsMax[3], agent[3];
    dtTileRef tileReference;
};
static_assert(sizeof(BenillaRetainedDetailHeight) == 136 &&
    alignof(BenillaRetainedDetailHeight) == 8 &&
    offsetof(BenillaRetainedDetailHeight, triangle) == 28 &&
    offsetof(BenillaRetainedDetailHeight, vertices) == 44 &&
    offsetof(BenillaRetainedDetailHeight, agent) == 116 &&
    offsetof(BenillaRetainedDetailHeight, tileReference) == 128, "detail witness ABI");
static void* create_nav(const unsigned char* params, int nodes) {
    const unsigned short endian=1;
    if (*reinterpret_cast<const unsigned char*>(&endian)!=1) return nullptr;
    BenillaNav* nav=new(std::nothrow) BenillaNav;
    if (!nav) return nullptr;
    dtNavMeshParams p;
    std::memcpy(&p, params, sizeof p);
    if (!nav->mesh || !nav->query || nav->mesh->init(&p)!=DT_SUCCESS ||
        nav->query->init(nav->mesh, nodes)!=DT_SUCCESS) { delete nav; return nullptr; }
    return nav;
}
extern "C" {
// The bounded loader remains a separate ABI; source query capacity is never relabeled.
void* benilla_nav_create(const unsigned char* params) { return create_nav(params, 512); }
// MoveMap.cpp::loadMapInstance initializes the retained map-instance query with 1024.
void* benilla_nav_create_source1024(const unsigned char* params) { return create_nav(params, 1024); }
unsigned benilla_nav_query_capacity(const void* raw) {
    if (!raw) return 0;
    const auto* nav = static_cast<const BenillaNav*>(raw);
    const auto* pool = nav->query ? nav->query->getNodePool() : nullptr;
    if (!pool || nav->query->getAttachedNavMesh() != nav->mesh) return 0;
    return static_cast<unsigned>(pool->getMaxNodes());
}
void benilla_nav_free(void* raw) { delete static_cast<BenillaNav*>(raw); }
unsigned benilla_nav_add(void* raw,const unsigned char* bytes,int size) {
    BenillaNav* nav=static_cast<BenillaNav*>(raw);
    if (nav->count>=64) return DT_FAILURE;
    dtMeshHeader h;
    std::memcpy(&h,bytes,sizeof h);
    // Upstream uses dtAlign4 even for 8-aligned dtLink. Choose the tile base's
    // four-byte offset so links are aligned; all other serialized types align <=4.
    const size_t linksOffset=100+size_t(h.vertCount)*12+size_t(h.polyCount)*32;
    void* allocation=std::malloc(size_t(size)+8);
    if (!allocation) return DT_FAILURE|DT_OUT_OF_MEMORY;
    unsigned char* data=static_cast<unsigned char*>(allocation)+(linksOffset%8 ? 4:0);
    std::memcpy(data,bytes,size_t(size));
    const dtStatus status=nav->mesh->addTile(data,size,0,0,nullptr);
    if (status!=DT_SUCCESS) { std::free(allocation); return status; }
    nav->allocations[nav->count++]=allocation;
    return status;
}
// Complete nontransport PathFinder::HaveTile predicate, on this exact loaded mesh.
// The Rust boundary limits points/origins to 100000 and tile dimensions to GRID,
// so calcTileLoc's original binary32 floorf-to-int conversions remain bounded.
unsigned benilla_nav_source_tile_presence(const void* raw, const float* world,
    int* coordinates, unsigned char* present) {
    if (!raw || !world || !coordinates || !present) return DT_FAILURE | DT_INVALID_PARAM;
    // Fail closed without changing the caller's rounding/flush environment.
    if (std::fegetround() != FE_TONEAREST) return 0;
#if defined(__x86_64__) || defined(_M_X64)
    if ((_mm_getcsr() & 0xe040u) != 0) return 0; // rounding, FTZ, DAZ
#elif defined(__aarch64__)
    unsigned long long control = 0;
    __asm__ volatile("mrs %0, fpcr" : "=r"(control));
    if ((control & ((3ull << 22) | (1ull << 24) | (1ull << 19) | 3ull)) != 0) return 0;
#else
    return 0;
#endif
    for (int i = 0; i != 3; ++i)
        if (!std::isfinite(world[i]) || std::fabs(world[i]) > 100000.0f)
            return DT_FAILURE | DT_INVALID_PARAM;
    const auto* nav = static_cast<const BenillaNav*>(raw);
    const float point[3] = {world[1], world[2], world[0]};
    int tx = -1, ty = -1;
    nav->mesh->calcTileLoc(point, &tx, &ty);
    coordinates[0] = tx; coordinates[1] = ty;
    *present = tx != -1 && ty != -1 && nav->mesh->getTileAt(tx, ty, 0) != nullptr ? 1 : 0;
    return DT_SUCCESS;
}
unsigned benilla_nav_nearest(void* raw,const float* point,const float* extents,
                             dtPolyRef* ref,float* nearest,unsigned char* over) {
    BenillaNav* nav=static_cast<BenillaNav*>(raw);
    bool isOver=false;
    const dtStatus status=nav->query->findNearestPoly(point,extents,&nav->filter,ref,nearest,&isOver);
    *over=isOver?1:0;
    return status;
}
unsigned benilla_nav_begin(void* raw,dtPolyRef from,dtPolyRef to,const float* start,const float* end) {
    BenillaNav* nav=static_cast<BenillaNav*>(raw);
    // No ANY_ANGLE: no potentially unbounded raycast inside sliced search.
    return nav->query->initSlicedFindPath(from,to,start,end,&nav->filter,0);
}
unsigned benilla_nav_step(void* raw,int* done) {
    return static_cast<BenillaNav*>(raw)->query->updateSlicedFindPath(1,done);
}
unsigned benilla_nav_finish(void* raw,dtPolyRef* path,int* count) {
    return static_cast<BenillaNav*>(raw)->query->finalizeSlicedFindPath(path,count,256);
}
unsigned benilla_nav_straight(void* raw,const float* start,const float* end,
                              const dtPolyRef* path,int count,float* points,
                              unsigned char* flags,dtPolyRef* refs,int* npoints) {
    return static_cast<BenillaNav*>(raw)->query->findStraightPath(start,end,path,count,
                                                               points,flags,refs,npoints,256,0);
}
unsigned benilla_nav_source_nearest(void* raw,const float* point,const float* extents,
    unsigned short include,unsigned short exclude,dtPolyRef* ref,float* nearest,unsigned char* over) {
    BenillaNav* nav=static_cast<BenillaNav*>(raw);
    dtQueryFilter filter;
    filter.setIncludeFlags(include); filter.setExcludeFlags(exclude);
    bool isOver=false;
    const dtStatus status=nav->query->findNearestPoly(point,extents,&filter,ref,nearest,&isOver);
    *over=isOver?1:0;
    return status;
}
BenillaSourceDirect* benilla_nav_source_direct_create(void* raw,dtPolyRef from,dtPolyRef to,
    const float* start,const float* end,unsigned short include,unsigned short exclude,dtStatus* status) {
    if (!raw) {
        if (status) *status=DT_FAILURE|DT_INVALID_PARAM;
        return nullptr;
    }
    return benilla_source_direct_create(static_cast<BenillaNav*>(raw)->mesh,
        from,to,start,end,include,exclude,status);
}
BenillaSourceSmoothing* benilla_nav_source_create(void* raw,const float* start,const float* end,
    const dtPolyRef* path,unsigned count,unsigned limit,unsigned short include,unsigned short exclude) {
    return benilla_source_create(static_cast<BenillaNav*>(raw)->query,start,end,path,count,limit,include,exclude);
}
void benilla_nav_warm_scan_reset(BenillaWarmScan* scan) {
    benilla_source_warm_scan_reset(scan);
}
unsigned benilla_nav_warm_scan(void* raw,dtPolyRef reference,const float* point,BenillaWarmScan* scan) {
    return benilla_source_warm_scan(static_cast<BenillaNav*>(raw)->query,reference,point,scan);
}
unsigned benilla_nav_warm_nearest(void* raw,const float* point,float extent,
    unsigned short include,unsigned short exclude,dtPolyRef* reference,float* distance) {
    return benilla_source_warm_nearest(static_cast<BenillaNav*>(raw)->query,point,extent,
        include,exclude,reference,distance);
}
unsigned char benilla_nav_warm_valid_ref(void* raw,dtPolyRef reference) {
    return static_cast<BenillaNav*>(raw)->mesh->isValidPolyRef(reference) ? 1 : 0;
}

BenillaSourceRay* benilla_nav_source_ray_create(void* raw, dtPolyRef start,
    const float* from, const float* to, unsigned short include, unsigned short exclude) {
    if (!raw) return nullptr;
    return benilla_source_ray_create(static_cast<BenillaNav*>(raw)->mesh, start,
        from, to, include, exclude);
}
unsigned benilla_nav_straight_height(void* raw, dtPolyRef reference,
    const float* point, float* height) {
    return static_cast<BenillaNav*>(raw)->query->getPolyHeight(reference, point, height);
}

// DetourNavMesh.cpp::getPolyHeight's exact ground containment and first matching
// detail triangle, followed by the actual attached query call. This adds no
// query or movement policy: absence of a triangle rejects rather than certifying
// the source closest-detail-edge fallback. Rust supplies only genuine records.
// Result: 1 success, 0 environment unavailable, 2 malformed, 3 offmesh,
// 4 outside, 5 missing detail, 6 edge fallback, 7 budget, 8 query status,
// 9 nonfinite, 10 differing height. No allocation or mesh/query mutation.
unsigned benilla_nav_retained_detail_height(const void* raw, dtPolyRef reference,
    const float* point, unsigned maxTriangleTests, BenillaRetainedDetailHeight* out) {
    if (!raw || !point || !out) return 2;
    if (std::fegetround() != FE_TONEAREST) return 0;
#if defined(__x86_64__) || defined(_M_X64)
    if ((_mm_getcsr() & 0xe040u) != 0) return 0;
#elif defined(__aarch64__)
    unsigned long long control = 0;
    __asm__ volatile("mrs %0, fpcr" : "=r"(control));
    if ((control & ((3ull << 22) | (1ull << 24) | (1ull << 19) | 3ull)) != 0) return 0;
#else
    return 0;
#endif
    for (int axis = 0; axis != 3; ++axis)
        if (!std::isfinite(point[axis])) return 9;
    const auto* nav = static_cast<const BenillaNav*>(raw);
    if (!nav->mesh || !nav->query || nav->query->getAttachedNavMesh() != nav->mesh)
        return 2;
    const dtMeshTile* tile = nullptr;
    const dtPoly* poly = nullptr;
    if (nav->mesh->getTileAndPolyByRef(reference, &tile, &poly) != DT_SUCCESS ||
        !tile || !poly || !tile->header || !tile->verts || !tile->polys) return 2;
    if (poly->getType() == DT_POLYTYPE_OFFMESH_CONNECTION) return 3;
    if (poly->getType() != DT_POLYTYPE_GROUND) return 2;
    const auto* header = tile->header;
    const auto ip = static_cast<unsigned>(poly - tile->polys);
    if (header->polyCount < 1 || ip >= static_cast<unsigned>(header->polyCount) ||
        header->vertCount < 3 || poly->vertCount < 3 || poly->vertCount > DT_VERTS_PER_POLYGON)
        return 2;
    if (!tile->detailMeshes || !tile->detailTris || header->detailMeshCount < 1 ||
        ip >= static_cast<unsigned>(header->detailMeshCount)) return 5;
    const auto* pd = &tile->detailMeshes[ip];
    if (pd->triCount == 0) return 5;
    if (header->detailTriCount < 0 || header->detailVertCount < 0 ||
        pd->triBase > static_cast<unsigned>(header->detailTriCount) ||
        pd->triCount > static_cast<unsigned>(header->detailTriCount) - pd->triBase ||
        pd->vertBase > static_cast<unsigned>(header->detailVertCount) ||
        pd->vertCount > static_cast<unsigned>(header->detailVertCount) - pd->vertBase ||
        (pd->vertCount && !tile->detailVerts)) return 2;
    float vertices[DT_VERTS_PER_POLYGON * 3];
    for (int i = 0; i < poly->vertCount; ++i) {
        if (poly->verts[i] >= header->vertCount) return 2;
        const float* v = &tile->verts[poly->verts[i] * 3];
        for (int axis = 0; axis != 3; ++axis)
            if (!std::isfinite(v[axis])) return 9;
        dtVcopy(&vertices[i * 3], v);
    }
    if (!dtPointInPolygon(point, vertices, poly->vertCount)) return 4;
    for (unsigned j = 0; j < pd->triCount; ++j) {
        // Reserve the matching original query scan before inspecting any triangle.
        // A failure consumes no published witness; never call unbudgeted fallback.
        if (j + 1 > maxTriangleTests / 2) return 7;
        const unsigned char* triangle = &tile->detailTris[(pd->triBase + j) * 4];
        const float* v[3];
        unsigned indices[3];
        for (int k = 0; k != 3; ++k) {
            if (triangle[k] < poly->vertCount) {
                indices[k] = poly->verts[triangle[k]];
                v[k] = &tile->verts[indices[k] * 3];
            } else {
                const unsigned local = triangle[k] - poly->vertCount;
                if (local >= pd->vertCount) return 2;
                indices[k] = pd->vertBase + local;
                v[k] = &tile->detailVerts[indices[k] * 3];
            }
            for (int axis = 0; axis != 3; ++axis)
                if (!std::isfinite(v[k][axis])) return 9;
        }
        float captured = 0;
        if (!dtClosestHeightPointTriangle(point, v[0], v[1], v[2], captured)) continue;
        if (!std::isfinite(captured)) return 9;
        float height = point[1];
        const dtStatus status = nav->query->getPolyHeight(reference, point, &height);
        if (status != DT_SUCCESS) return 8;
        if (!std::isfinite(height)) return 9;
        if (std::bit_cast<unsigned>(height) != std::bit_cast<unsigned>(captured)) return 10;
        out->status = status;
        out->height = std::bit_cast<unsigned>(height);
        out->triangleIndex = j;
        out->triangleCount = pd->triCount;
        out->triangleTests = 2 * (j + 1);
        out->polygonIndex = ip;
        out->polygonVertexCount = poly->vertCount;
        std::memcpy(out->triangle, triangle, 4);
        for (int k = 0; k != 3; ++k) {
            out->vertexIndices[k] = indices[k];
            for (int axis = 0; axis != 3; ++axis)
                out->vertices[k][axis] = std::bit_cast<unsigned>(v[k][axis]);
        }
        out->tileCoordinates[0] = header->x;
        out->tileCoordinates[1] = header->y;
        out->tileCoordinates[2] = header->layer;
        for (int axis = 0; axis != 3; ++axis) {
            if (!std::isfinite(header->bmin[axis]) || !std::isfinite(header->bmax[axis])) return 9;
            out->boundsMin[axis] = std::bit_cast<unsigned>(header->bmin[axis]);
            out->boundsMax[axis] = std::bit_cast<unsigned>(header->bmax[axis]);
        }
        if (!std::isfinite(header->walkableHeight) || !std::isfinite(header->walkableRadius) ||
            !std::isfinite(header->walkableClimb)) return 9;
        out->agent[0] = std::bit_cast<unsigned>(header->walkableHeight);
        out->agent[1] = std::bit_cast<unsigned>(header->walkableRadius);
        out->agent[2] = std::bit_cast<unsigned>(header->walkableClimb);
        out->tileReference = nav->mesh->getTileRef(tile);
        return 1;
    }
    return 6;
}

}
