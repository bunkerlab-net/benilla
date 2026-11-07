// SPDX-License-Identifier: GPL-3.0-or-later
// Private source-arithmetic primitives. Rust owns bounded ordered scans and storage.
#ifndef BENILLA_SOURCE_WARM_CACHE_H
#define BENILLA_SOURCE_WARM_CACHE_H
#include "DetourNavMeshQuery.h"
struct BenillaWarmScan {
    dtPolyRef reference;
    float minimumSquared;
    float distance;
};
void benilla_source_warm_scan_reset(BenillaWarmScan* scan);
dtStatus benilla_source_warm_scan(dtNavMeshQuery* query, dtPolyRef reference,
    const float* point, BenillaWarmScan* scan);
dtStatus benilla_source_warm_nearest(dtNavMeshQuery* query, const float* point,
    float extent, unsigned short include, unsigned short exclude,
    dtPolyRef* reference, float* distance);
#endif
