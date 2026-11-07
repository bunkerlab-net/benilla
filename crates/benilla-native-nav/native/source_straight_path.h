// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef BENILLA_SOURCE_STRAIGHT_PATH_H
#define BENILLA_SOURCE_STRAIGHT_PATH_H
#include "DetourNavMeshQuery.h"
struct BenillaSourceRay;
struct BenillaSourceRayResult {
    dtPolyRef path[74];
    float hit, normal[3];
    unsigned count, status;
};
BenillaSourceRay* benilla_source_ray_create(const dtNavMesh* mesh, dtPolyRef start,
    const float* from, const float* to, unsigned short include, unsigned short exclude);
extern "C" {
void benilla_source_ray_free(BenillaSourceRay* state);
unsigned benilla_source_ray_step(BenillaSourceRay* state);
void benilla_source_ray_result(const BenillaSourceRay* state, BenillaSourceRayResult* out);
// World XYZ source arithmetic. Output initialized before every ignored height query.
unsigned benilla_source_straight_plan(const float* from, const float* to, unsigned max_points,
    unsigned* steps, float* diff);
unsigned benilla_source_straight_hit(const float* from_yzx, const float* to_yzx,
    float hit, float width, float* adjusted, float* point_yzx);
unsigned benilla_source_straight_hit_distance(const float* from_xyz, const float* hit_yzx);
void benilla_source_straight_advance(float* previous_xyz, const float* diff_xyz);
// Returns source terminal type, updates actual/final point, and sets shortcut flag.
unsigned benilla_source_straight_terminal(const float* from, const float* end,
    unsigned count, unsigned path_type, unsigned force, float* last,
    float* actual_end, unsigned* shortcut);
}
#endif
