// SPDX-License-Identifier: GPL-3.0-or-later
// Benilla wrapper around pinned Detour. Only the Rust validator may supply tile bytes.
#include "DetourNavMesh.h"
#include "DetourNavMeshQuery.h"
#include "DetourAlloc.h"
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
extern "C" {
void* benilla_nav_create(const unsigned char* params) {
    const unsigned short endian=1;
    if (*reinterpret_cast<const unsigned char*>(&endian)!=1) return nullptr;
    BenillaNav* nav=new(std::nothrow) BenillaNav;
    if (!nav) return nullptr;
    dtNavMeshParams p;
    std::memcpy(&p, params, sizeof p);
    if (!nav->mesh || !nav->query || nav->mesh->init(&p)!=DT_SUCCESS ||
        nav->query->init(nav->mesh, 512)!=DT_SUCCESS) { delete nav; return nullptr; }
    return nav;
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

}
