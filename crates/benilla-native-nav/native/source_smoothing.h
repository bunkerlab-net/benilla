// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef BENILLA_SOURCE_SMOOTHING_H
#define BENILLA_SOURCE_SMOOTHING_H
#include "DetourNavMeshQuery.h"
#include <cstdint>
struct BenillaSourceSmoothing;
// One bounded native primitive per step; 0 pending, 1 source return, 2 unsafe result.
struct BenillaSourceStep {
    uint32_t kind, status, iterations, stop, source_status, point_count;
};
BenillaSourceSmoothing* benilla_source_create(dtNavMeshQuery* query,
    const float* start, const float* end, const dtPolyRef* path, uint32_t count,
    uint32_t limit, unsigned short include, unsigned short exclude);
extern "C" {
void benilla_source_free(BenillaSourceSmoothing* state);
uint32_t benilla_source_step(BenillaSourceSmoothing* state, BenillaSourceStep* result);
void benilla_source_copy(const BenillaSourceSmoothing* state, float* points,
    dtPolyRef* path, uint32_t* count);
// Numeric-only source helpers: these references are never used to query a mesh.
uint32_t benilla_source_fixup(dtPolyRef* path, uint32_t count, uint32_t capacity,
    const dtPolyRef* visited, uint32_t visited_count);
uint32_t benilla_source_in_range(const float* a, const float* b, float radius, float height);
}
#endif
