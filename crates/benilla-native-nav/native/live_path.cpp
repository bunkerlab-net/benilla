// SPDX-License-Identifier: GPL-3.0-or-later
// PathFinder.cpp:678-743,837-866,1122-1138 on the selected live query.
#include "DetourNavMeshQuery.h"
#include "DetourCommon.h"
#include "source_smoothing.h"
#include <cstdint>
#include <climits>
#include <cmath>
extern "C" {
uint32_t benilla_live_have_tile(void* raw, const float* p) {
    const auto* q = static_cast<dtNavMeshQuery*>(raw);
    int x=-1,y=-1;
    const dtNavMesh* mesh=q->getAttachedNavMesh();
    mesh->calcTileLoc(p,&x,&y);
    return x != -1 && y != -1 && mesh->getTileAt(x,y,0) != nullptr;
}
uint32_t benilla_live_find_path(void* raw, uint64_t a, uint64_t b,
    const float* start, const float* end, uint16_t include, uint16_t exclude,
    uint64_t* path, int* count, int limit, uint32_t ray, float* hit) {
    auto* q=static_cast<dtNavMeshQuery*>(raw);
    dtQueryFilter filter; filter.setIncludeFlags(include); filter.setExcludeFlags(exclude);
    if (!ray) return q->findPath(a,b,start,end,&filter,path,count,limit);
    float normal[3]={};
    return q->raycast(a,start,end,&filter,hit,normal,path,count,limit);
}
uint32_t benilla_live_closest(void* raw, uint64_t ref, const float* p, float* out) {
    return static_cast<dtNavMeshQuery*>(raw)->closestPointOnPoly(ref,p,out,nullptr);
}
uint32_t benilla_live_height(void* raw, uint64_t ref, const float* p, float* out) {
    return static_cast<dtNavMeshQuery*>(raw)->getPolyHeight(ref,p,out);
}
uint32_t benilla_live_straight(void* raw, const float* start, const float* end,
    const uint64_t* path, int count, float* points, int* n, int limit) {
    return static_cast<dtNavMeshQuery*>(raw)->findStraightPath(start,end,path,count,points,nullptr,nullptr,n,limit);
}
void* benilla_live_smooth_create(void* raw, const float* start, const float* end,
    const uint64_t* path, uint32_t count, uint32_t limit, uint16_t include, uint16_t exclude, uint64_t* scratch, float* points) {
    return benilla_source_create_live(static_cast<dtNavMeshQuery*>(raw),start,end,path,count,limit,include,exclude,scratch,points);
}
}
