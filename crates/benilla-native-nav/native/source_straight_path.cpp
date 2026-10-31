//
// Copyright (c) 2009-2010 Mikko Mononen memon@inside.org
//
// This software is provided 'as-is', without any express or implied
// warranty.  In no event will the authors be held liable for any damages
// arising from the use of this software.
// Permission is granted to anyone to use this software for any purpose,
// including commercial applications, and to alter it and redistribute it
// freely, subject to the following restrictions:
// 1. The origin of this software must not be misrepresented; you must not
//    claim that you wrote the original software. If you use this software
//    in a product, an acknowledgment in the product documentation would be
//    appreciated but is not required.
// 2. Altered source versions must be plainly marked as such, and must not be
//    misrepresented as being the original software.
// 3. This notice may not be removed or altered from any source distribution.
//

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
// Pinned PathFinder.cpp 673-850/942-986 and G3D Vector3 arithmetic; provenance in PROVENANCE.md.
// Ray traversal adapts pinned DetourNavMeshQuery::raycast with options=0, split only
// between original polygon iterations. Output saturation does not stop traversal.
#include "source_straight_path.h"
#include "DetourCommon.h"
#include <cfloat>
#include <cmath>
#include <new>

#ifdef DT_VIRTUAL_QUERYFILTER
#error "Source straight boundary requires the pinned default nonvirtual query filter"
#endif

// The original inline filter lives in another translation unit. Preserve its exact
// default expression through public getters, as in the source direct boundary.
struct SourceStraightFilter : dtQueryFilter {
    bool passFilter(const dtPolyRef, const dtMeshTile*, const dtPoly* poly) const {
        return (poly->flags & getIncludeFlags()) != 0 && (poly->flags & getExcludeFlags()) == 0;
    }
};
struct BenillaSourceRay {
    const dtNavMesh* mesh;
    SourceStraightFilter filter;
    float from[3], to[3];
    dtPolyRef current;
    const dtMeshTile* tile;
    const dtPoly* poly;
    BenillaSourceRayResult out;
    bool done;
};
BenillaSourceRay* benilla_source_ray_create(const dtNavMesh* mesh, dtPolyRef start,
    const float* from, const float* to, unsigned short include, unsigned short exclude) {
    // Original legacy wrapper copies hitNormal even when the inner overload rejects
    // before initializing it. Never enter that undefined rejected-input domain.
    if (!mesh || !mesh->isValidPolyRef(start) || !from || !to ||
        !dtVisfinite(from) || !dtVisfinite(to)) return nullptr;
    auto* state = new(std::nothrow) BenillaSourceRay{};
    if (!state) return nullptr;
    state->mesh = mesh;
    state->filter.setIncludeFlags(include); state->filter.setExcludeFlags(exclude);
    dtVcopy(state->from, from); dtVcopy(state->to, to);
    state->current = start;
    mesh->getTileAndPolyByRefUnsafe(start, &state->tile, &state->poly);
    state->out.status = DT_SUCCESS;
    return state;
}
extern "C" void benilla_source_ray_free(BenillaSourceRay* state) { delete state; }
extern "C" void benilla_source_ray_result(const BenillaSourceRay* state, BenillaSourceRayResult* out) {
    *out = state->out;
}
extern "C" unsigned benilla_source_ray_step(BenillaSourceRay* state) {
    if (state->done) return 1;
    const auto* mesh = state->mesh;
    const auto& filter = state->filter;
    const auto* tile = state->tile;
    const auto* poly = state->poly;
    const dtMeshTile* nextTile = nullptr;
    const dtPoly* nextPoly = nullptr;
    const auto curRef = state->current;
    const float* startPos = state->from;
    const float* endPos = state->to;
    auto& out = state->out;
    auto& n = out.count;
    auto& status = out.status;
    float verts[DT_VERTS_PER_POLYGON*3+3];
		// Cast ray against current polygon.

		// Collect vertices.
		int nv = 0;
		for (int i = 0; i < (int)poly->vertCount; ++i)
		{
			dtVcopy(&verts[nv*3], &tile->verts[poly->verts[i]*3]);
			nv++;
		}

		float tmin, tmax;
		int segMin, segMax;
		if (!dtIntersectSegmentPoly2D(startPos, endPos, verts, nv, tmin, tmax, segMin, segMax))
		{
			// Could not hit the polygon, keep the old t and report hit.
			out.count = n;
			state->done = true; return 1;
		}

		// hitEdgeIndex is not consumed by the legacy no-cost caller.

		// Keep track of furthest t so far.
		if (tmax > out.hit)
			out.hit = tmax;

		// Store visited polygons.
		if (n < 74)
			out.path[n++] = curRef;
		else
			status |= DT_BUFFER_TOO_SMALL;

		// Ray end is completely inside the polygon.
		if (segMax == -1)
		{
			out.hit = FLT_MAX;
			out.count = n;

			state->done = true; return 1;
		}

		// Follow neighbours.
		dtPolyRef nextRef = 0;

		for (unsigned int i = poly->firstLink; i != DT_NULL_LINK; i = tile->links[i].next)
		{
			const dtLink* link = &tile->links[i];

			// Find link which contains this edge.
			if ((int)link->edge != segMax)
				continue;

			// Get pointer to the next polygon.
			nextTile = 0;
			nextPoly = 0;
			mesh->getTileAndPolyByRefUnsafe(link->ref, &nextTile, &nextPoly);

			// Skip off-mesh connections.
			if (nextPoly->getType() == DT_POLYTYPE_OFFMESH_CONNECTION)
				continue;

			// Skip links based on filter.
			if (!filter.passFilter(link->ref, nextTile, nextPoly))
				continue;

			// If the link is internal, just return the ref.
			if (link->side == 0xff)
			{
				nextRef = link->ref;
				break;
			}

			// If the link is at tile boundary,

			// Check if the link spans the whole edge, and accept.
			if (link->bmin == 0 && link->bmax == 255)
			{
				nextRef = link->ref;
				break;
			}

			// Check for partial edge links.
			const int v0 = poly->verts[link->edge];
			const int v1 = poly->verts[(link->edge+1) % poly->vertCount];
			const float* left = &tile->verts[v0*3];
			const float* right = &tile->verts[v1*3];

			// Check that the intersection lies inside the link portal.
			if (link->side == 0 || link->side == 4)
			{
				// Calculate link size.
				const float s = 1.0f/255.0f;
				float lmin = left[2] + (right[2] - left[2])*(link->bmin*s);
				float lmax = left[2] + (right[2] - left[2])*(link->bmax*s);
				if (lmin > lmax) dtSwap(lmin, lmax);

				// Find Z intersection.
				float z = startPos[2] + (endPos[2]-startPos[2])*tmax;
				if (z >= lmin && z <= lmax)
				{
					nextRef = link->ref;
					break;
				}
			}
			else if (link->side == 2 || link->side == 6)
			{
				// Calculate link size.
				const float s = 1.0f/255.0f;
				float lmin = left[0] + (right[0] - left[0])*(link->bmin*s);
				float lmax = left[0] + (right[0] - left[0])*(link->bmax*s);
				if (lmin > lmax) dtSwap(lmin, lmax);

				// Find X intersection.
				float x = startPos[0] + (endPos[0]-startPos[0])*tmax;
				if (x >= lmin && x <= lmax)
				{
					nextRef = link->ref;
					break;
				}
			}
		}


		if (!nextRef)
		{
			// No neighbour, we hit a wall.

			// Calculate hit normal.
			const int a = segMax;
			const int b = segMax+1 < nv ? segMax+1 : 0;
			const float* va = &verts[a*3];
			const float* vb = &verts[b*3];
			const float dx = vb[0] - va[0];
			const float dz = vb[2] - va[2];
			out.normal[0] = dz;
			out.normal[1] = 0;
			out.normal[2] = -dx;
			dtVnormalize(out.normal);

			out.count = n;
			state->done = true; return 1;
		}

    state->current = nextRef;
    state->tile = nextTile;
    state->poly = nextPoly;
    return 0;
}
namespace {
float squared(const float* a, const float* b) {
    const float x = a[0] - b[0], y = a[1] - b[1], z = a[2] - b[2];
    return x*x + y*y + z*z;
}
}
extern "C" unsigned benilla_source_straight_plan(const float* from, const float* to,
    unsigned max_points, unsigned* steps, float* diff) {
    if (!from || !to || !steps || !diff || !dtVisfinite(from) || !dtVisfinite(to) ||
        max_points < 2 || max_points > 1026) return 0;
    const float x = to[0] - from[0], y = to[1] - from[1], z = to[2] - from[2];
    const float length = ::sqrtf(x*x + y*y + z*z);
    if (!std::isfinite(length) || length > 4096.0f) return 0;
    if (!(length > 0.1f)) { *steps = 0; return 2; }
    const float stepCountFloat = length / 4.0f;
    // Separate operational domain, checked before the source float-to-uint conversion.
    if (!std::isfinite(stepCountFloat) ||
        stepCountFloat >= 4294967296.0f) return 0;
    unsigned stepCount = static_cast<unsigned>(stepCountFloat);
    const float stepPart = stepCountFloat - stepCount;
    if (stepCount > 1.0f && stepPart < 0.3f) --stepCount;
    if (stepCount > max_points - 2) return 0;
    *steps = stepCount;
    const float scale = 4.0f / length;
    diff[0] = x * scale; diff[1] = y * scale; diff[2] = z * scale;
    return 1;
}
extern "C" unsigned benilla_source_straight_hit(const float* from, const float* to,
    float hit, float width, float* adjusted, float* point) {
    hit = hit - width;
    *adjusted = hit;
    if (hit < 0.1f) return 0;
    dtVlerp(point, from, to, hit);
    return 1;
}
extern "C" unsigned benilla_source_straight_hit_distance(const float* from, const float* hit) {
    const float xyz[3] = {hit[2], hit[0], hit[1]};
    return squared(from, xyz) > 0.01 ? 1 : 0;
}
extern "C" void benilla_source_straight_advance(float* previous, const float* diff) {
    previous[0] += diff[0]; previous[1] += diff[1]; previous[2] += diff[2];
}
extern "C" unsigned benilla_source_straight_terminal(const float* from, const float* end,
    unsigned count, unsigned path_type, unsigned force, float* last,
    float* actual, unsigned* shortcut) {
    *shortcut = 0;
    if (count < 2) { *shortcut = 1; dtVcopy(actual, end); return 8; }
    if (count == 74) { *shortcut = 1; dtVcopy(actual, end); return 32; }
    dtVcopy(actual, last);
    const float x = end[0] - actual[0], y = end[1] - actual[1], z = end[2] - actual[2];
    const bool inRange = (x*x + y*y) < 1.0f * 1.0f && fabsf(z) < 1.0f;
    if (force && (!(path_type & 1) || !inRange)) {
        if (squared(actual, end) < 0.3f * squared(from, end)) {
            dtVcopy(actual, end); dtVcopy(last, end);
        } else { dtVcopy(actual, end); *shortcut = 1; }
        path_type = 1 | 16;
    }
    return path_type;
}
