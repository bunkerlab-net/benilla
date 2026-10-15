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
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program; if not, write to the Free Software
 * Foundation, Inc., 59 Temple Place, Suite 330, Boston, MA 02111-1307 USA
 */
// GPL-3.0-or-later adaptation by the 2026 Benilla fork contributors.
// PathFinder.cpp 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, lines 1141-1348/1438-1444.
// Source arithmetic and helper quirks retained; ground-only loop split at primitive calls.
#include "source_smoothing.h"
#include "DetourCommon.h"
#include "DetourMath.h"
#include <cstring>
#include <new>

extern "C" uint32_t benilla_source_in_range(const float* v1, const float* v2, float r, float h) {
    const float dx = v2[0] - v1[0];
    const float dy = v2[1] - v1[1];
    const float dz = v2[2] - v1[2];
    return (dx * dx + dz * dz) < r * r && fabsf(dy) < h;
}
extern "C" uint32_t benilla_source_fixup(dtPolyRef* path, uint32_t npath, uint32_t maxPath,
                                        const dtPolyRef* visited, uint32_t nvisited) {
    int32_t furthestPath = -1;
    int32_t furthestVisited = -1;
    for (int32_t i = npath - 1; i >= 0; --i) {
        bool found = false;
        for (int32_t j = nvisited - 1; j >= 0; --j) {
            if (path[i] == visited[j]) {
                furthestPath = i;
                furthestVisited = j;
                found = true;
            }
        }
        if (found) break;
    }
    if (furthestPath == -1 || furthestVisited == -1) return npath;
    uint32_t req = nvisited - furthestVisited;
    uint32_t orig = uint32_t(furthestPath + 1) < npath ? furthestPath + 1 : npath;
    uint32_t size = npath > orig ? npath - orig : 0;
    if (req + size > maxPath) size = maxPath - req;
    if (req >= maxPath) return npath;
    if (size) std::memmove(path + req, path + orig, size * sizeof(dtPolyRef));
    for (uint32_t i = 0; i < req; ++i) path[i] = visited[(nvisited - 1) - i];
    return req + size;
}
struct BenillaSourceSmoothing {
    dtNavMeshQuery* query;
    dtQueryFilter filter;
    float start[3], end[3], iter[3], target[3], steer[3], move[3], result[3];
    float points[74 * 3];
    dtPolyRef path[74], visited[16];
    uint32_t npolys, nvisited, limit, count, phase, iterations;
    unsigned char steerFlag;
    BenillaSourceSmoothing(): query(nullptr), start{}, end{}, iter{}, target{}, steer{},
        move{}, result{}, points{}, path{}, visited{}, npolys(0), nvisited(0),
        limit(0), count(0), phase(0), iterations(0), steerFlag(0) {}
};
static bool finite_point(const float* p) {
    for (int i=0;i<3;++i) if (!dtMathIsfinite(p[i]) || fabsf(p[i]) > 100000.0f) return false;
    return true;
}
BenillaSourceSmoothing* benilla_source_create(dtNavMeshQuery* query,
    const float* start, const float* end, const dtPolyRef* path, uint32_t count,
    uint32_t limit, unsigned short include, unsigned short exclude) {
    if (!query || count == 0 || count > limit || limit < 2 || limit > 74 || include != 1 || exclude != 0)
        return nullptr;
    BenillaSourceSmoothing* s = new(std::nothrow) BenillaSourceSmoothing;
    if (!s) return nullptr;
    s->query = query;
    s->filter.setIncludeFlags(include);
    s->filter.setExcludeFlags(exclude);
    dtVcopy(s->start,start); dtVcopy(s->end,end);
    std::memcpy(s->path,path,count * sizeof(dtPolyRef));
    s->npolys=count; s->limit=limit;
    return s;
}
extern "C" void benilla_source_free(BenillaSourceSmoothing* s) { delete s; }
static uint32_t complete(BenillaSourceSmoothing* s, BenillaSourceStep* out, uint32_t stop) {
    out->stop=stop;
    out->source_status=s->count < s->limit ? DT_SUCCESS : DT_FAILURE;
    out->point_count=s->count;
    s->phase=5;
    return 1;
}
extern "C" uint32_t benilla_source_step(BenillaSourceSmoothing* s, BenillaSourceStep* out) {
    *out={s->phase,0,s->iterations,0,0,s->count};
    if (s->phase == 0 || s->phase == 1) {
        const bool first=s->phase == 0;
        out->status=s->query->closestPointOnPolyBoundary(
            s->path[first ? 0 : s->npolys-1], first ? s->start : s->end, first ? s->iter : s->target);
        if (dtStatusFailed(out->status)) {
            out->stop=4; out->source_status=DT_FAILURE; out->point_count=0; s->phase=5;
            return 1;
        }
        if (!finite_point(first ? s->iter : s->target)) return 2;
        if (!first) { dtVcopy(s->points,s->iter); s->count=1; }
        ++s->phase;
        return 0;
    }
    if (s->phase == 2) {
        ++s->iterations; out->iterations=s->iterations;
        float steerPath[3 * 3]={}; unsigned char flags[3]={}; dtPolyRef refs[3]={}; int nsteer=0;
        out->status=s->query->findStraightPath(s->iter,s->target,s->path,int(s->npolys),
                                             steerPath,flags,refs,&nsteer,3);
        // PathFinder checks failure, not exact SUCCESS: BUFFER_TOO_SMALL is expected here.
        if (!nsteer || dtStatusFailed(out->status)) return complete(s,out,2);
        if (nsteer < 0 || nsteer > 3) return 2;
        int ns=0;
        while (ns<nsteer) {
            if ((flags[ns] & DT_STRAIGHTPATH_OFFMESH_CONNECTION) ||
                !benilla_source_in_range(&steerPath[ns*3],s->iter,0.3f,1000.0f)) break;
            ++ns;
        }
        if (ns>=nsteer) return complete(s,out,2);
        dtVcopy(s->steer,&steerPath[ns*3]);
        s->steer[1]=s->iter[1];
        s->steerFlag=flags[ns];
        // Off-mesh products are excluded by validation; do not emulate their owner branch.
        if (!finite_point(s->steer) || (s->steerFlag & ~3u) != 0) return 2;
        const bool endOfPath=(s->steerFlag & DT_STRAIGHTPATH_END)!=0;
        float delta[3]; dtVsub(delta,s->steer,s->iter);
        float len=dtMathSqrtf(dtVdot(delta,delta));
        if (endOfPath && len < 4.0f) len=1.0f;
        else len=4.0f/len;
        dtVmad(s->move,s->iter,delta,len);
        if (!finite_point(s->move)) return 2;
        s->phase=3;
        return 0;
    }
    if (s->phase == 3) {
        int nvisited=0;
        out->status=s->query->moveAlongSurface(s->path[0],s->iter,s->move,&s->filter,
                                              s->result,s->visited,&nvisited,16);
        // Both successful returns copy initialized bestPos; truncated visited lists are
        // source-defined and remain bounded by 16. Actual failure may leave outputs unwritten.
        if ((out->status != DT_SUCCESS && out->status != (DT_SUCCESS | DT_BUFFER_TOO_SMALL)) ||
            nvisited < 0 || nvisited > 16 || !finite_point(s->result)) return 2;
        s->nvisited=uint32_t(nvisited);
        s->npolys=benilla_source_fixup(s->path,s->npolys,s->limit,s->visited,s->nvisited);
        if (s->npolys == 0 || s->npolys > 74) return 2;
        s->phase=4;
        return 0;
    }
    if (s->phase == 4) {
        out->status=s->query->getPolyHeight(s->path[0],s->result,&s->result[1]);
        // Ground height failure returns before writing; successful move already initialized
        // all result components. Preserve the source's existing-Y +0.5 fallback exactly.
        if ((out->status != DT_SUCCESS && out->status != (DT_FAILURE | DT_INVALID_PARAM)) ||
            !finite_point(s->result)) return 2;
        s->result[1] += 0.5f;
        dtVcopy(s->iter,s->result);
        if (!finite_point(s->iter)) return 2;
        if ((s->steerFlag & DT_STRAIGHTPATH_END) &&
            benilla_source_in_range(s->iter,s->steer,0.3f,1.0f)) {
            dtVcopy(s->iter,s->target);
            dtVcopy(&s->points[s->count*3],s->iter); ++s->count;
            return complete(s,out,1);
        }
        dtVcopy(&s->points[s->count*3],s->iter); ++s->count;
        if (s->count >= s->limit) return complete(s,out,3);
        s->phase=2;
        return 0;
    }
    return 2;
}
extern "C" void benilla_source_copy(const BenillaSourceSmoothing* s, float* points,
                                    dtPolyRef* path, uint32_t* count) {
    std::memcpy(points,s->points,sizeof(s->points));
    std::memcpy(path,s->path,sizeof(s->path));
    *count=s->npolys;
}
