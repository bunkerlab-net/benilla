// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef BENILLA_SOURCE_POINT_PATH_H
#define BENILLA_SOURCE_POINT_PATH_H
#include <cstdint>
struct BenillaSourcePointPath {
    float points[74][3]; // source owner XYZ, after OptimizePath
    float actual_end[3];
    uint32_t point_count, path_type, clear_cache;
};
// Pure bounded arithmetic, no refs or mesh access. 1 success, 2 omitted prior-point state,
// 0 invalid input. This private helper permits numeric source boundary observations only.
extern "C" uint32_t benilla_source_point_path(const float* yzx, uint32_t count,
    uint32_t limit, uint32_t status, uint32_t path_type, const float* start_xyz,
    const float* end_xyz, uint32_t optimize, uint32_t force, BenillaSourcePointPath* result);
#endif
