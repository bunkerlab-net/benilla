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
// PathFinder.cpp 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, lines 795-1021/1446-1455.
// Source BuildPointPath smooth-branch postprocessing; NormalizePath is excluded upstream.
#include "source_point_path.h"
#include <cmath>

namespace {
struct Point { float x, y, z; };
bool zero(const Point& p) { return p.x == 0.0f && p.y == 0.0f && p.z == 0.0f; }
bool finite(const Point& p) {
    return std::isfinite(p.x) && std::isfinite(p.y) && std::isfinite(p.z) &&
        std::fabs(p.x) <= 100000.0f && std::fabs(p.y) <= 100000.0f && std::fabs(p.z) <= 100000.0f;
}
Point point(const float* p) { return {p[0], p[1], p[2]}; }
void store(float* out, const Point& p) { out[0] = p.x; out[1] = p.y; out[2] = p.z; }
float dist3DSqr(const Point& p1, const Point& p2) {
    const Point d = {p1.x - p2.x, p1.y - p2.y, p1.z - p2.z};
    return d.x * d.x + d.y * d.y + d.z * d.z;
}
bool inRange(const Point& p1, const Point& p2) {
    const Point d = {p1.x - p2.x, p1.y - p2.y, p1.z - p2.z};
    return (d.x * d.x + d.y * d.y) < 1.0f * 1.0f && fabsf(d.z) < 1.0f;
}
void shortcut(BenillaSourcePointPath* out, const Point& start, const Point& actual) {
    // PathFinder::clear changes active polygon count, never retained polygon storage.
    out->clear_cache = 1;
    out->point_count = 2;
    store(out->points[0], start);
    store(out->points[1], actual);
    out->path_type = 2; // PATHFIND_SHORTCUT; overwritten by each calling branch below.
}
}
extern "C" uint32_t benilla_source_point_path(const float* yzx, uint32_t count,
    uint32_t limit, uint32_t status, uint32_t path_type, const float* start_xyz,
    const float* end_xyz, uint32_t optimize, uint32_t force, BenillaSourcePointPath* out) {
    if (!out || !start_xyz || !end_xyz || (!yzx && count) || count > 74 ||
        limit < 2 || limit > 74 || count > limit || optimize > 1 || force > 1 ||
        (path_type != 1 && path_type != 4)) return 0;
    const Point start = point(start_xyz), end = point(end_xyz);
    if (!finite(start) || !finite(end)) return 0;
    Point points[74] = {}; // G3D::Vector3's default constructor also initializes zero.
    for (uint32_t i = 0; i < count; ++i) {
        points[i] = {yzx[i * 3 + 2], yzx[i * 3], yzx[i * 3 + 1]};
        if (!finite(points[i])) return 0;
        // Original zero sentinels can leave retained m_pathPoints slots after resize.
        // That prior vector is absent from this dependency: reject rather than invent it.
        if (optimize && count > 2 && zero(points[i])) return 2;
    }
    *out = {};
    out->path_type = path_type;
    Point actual = end; // calculate/setEndPosition precedes BuildPolyPath in this domain.
    if (count > 2 && optimize) {
        uint32_t tempPointCounter = 2;
        uint8_t cutLimit = 0;
        for (uint32_t i = 1; i < count - 1; ++i) {
            const Point p = points[i], p1 = points[i - 1], p2 = points[i + 1];
            const float lineLen = (p1.y - p2.y) * p.x + (p2.x - p1.x) * p.y +
                (p1.x * p2.y - p2.x * p1.y);
            if (fabs(lineLen) < 0.5f && cutLimit < 6) {
                points[i] = {0.0f, 0.0f, 0.0f};
                ++cutLimit;
            } else {
                ++tempPointCounter;
                cutLimit = 0;
            }
        }
        uint32_t pointPos = 0;
        for (uint32_t i = 0; i < count; ++i) {
            if (!zero(points[i])) {
                if (pointPos >= tempPointCounter) return 0;
                store(out->points[pointPos++], points[i]);
            }
        }
        if (pointPos != tempPointCounter) return 0;
        count = tempPointCounter;
    } else {
        for (uint32_t i = 0; i < count; ++i) store(out->points[i], points[i]);
    }
    out->point_count = count;
    // Source tests failure AFTER optimization, and before its optimized-count SHORT check.
    if (count < 2 || (status & (1u << 31))) {
        shortcut(out, start, actual);
        out->path_type = 8; // PATHFIND_NOPATH
    } else if (count == limit) {
        shortcut(out, start, actual);
        out->path_type = 32; // PATHFIND_SHORT
    } else {
        actual = point(out->points[count - 1]);
        // Actual end is captured before NormalizePath (inactive in the admitted domain).
        if (force && (!(out->path_type & 1) || !inRange(end, actual))) {
            if (dist3DSqr(actual, end) < 0.3f * dist3DSqr(start, end)) {
                actual = end;
                store(out->points[out->point_count - 1], end);
            } else {
                actual = end;
                shortcut(out, start, actual);
            }
            out->path_type = 1 | 16; // NORMAL | NOT_USING_PATH
        }
    }
    store(out->actual_end, actual);
    return 1;
}
