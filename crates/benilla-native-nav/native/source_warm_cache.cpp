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
// PathFinder.cpp 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, lines 306-383.
// Ordered lookup is split at each native query so Rust can check work/deadline.
#include "source_warm_cache.h"
#include "DetourCommon.h"
#include "DetourMath.h"
#include <limits>

void benilla_source_warm_scan_reset(BenillaWarmScan* scan) {
    scan->reference = 0;
    scan->minimumSquared = std::numeric_limits<float>::max();
    scan->distance = dtMathSqrtf(scan->minimumSquared);
}

dtStatus benilla_source_warm_scan(dtNavMeshQuery* query, dtPolyRef reference,
    const float* point, BenillaWarmScan* scan) {
    float closestPoint[3];
    const dtStatus status = query->closestPointOnPoly(reference, point, closestPoint, nullptr);
    if (dtStatusFailed(status)) return status;
    const float d = dtVdistSqr(point, closestPoint);
    if (d < scan->minimumSquared) {
        scan->minimumSquared = d;
        scan->reference = reference;
    }
    // In source this write occurs after the loop; keeping the observation current
    // changes no branch or arithmetic, and no intermediate state is authoritative.
    scan->distance = dtMathSqrtf(scan->minimumSquared);
    return status;
}

dtStatus benilla_source_warm_nearest(dtNavMeshQuery* query, const float* point,
    float extent, unsigned short include, unsigned short exclude,
    dtPolyRef* reference, float* distance) {
    dtQueryFilter filter;
    filter.setIncludeFlags(include);
    filter.setExcludeFlags(exclude);
    const float extents[3] = {extent, extent, extent};
    float closestPoint[3] = {0.0f, 0.0f, 0.0f};
    const dtStatus status = query->findNearestPoly(point, extents, &filter, reference, closestPoint);
    if (dtStatusSucceed(status) && *reference != 0)
        *distance = dtVdist(closestPoint, point);
    return status;
}
