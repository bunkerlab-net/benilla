// SPDX-License-Identifier: GPL-3.0-or-later
// Private loaded-mesh boundary. No public Rust API accepts polygon-reference input.
#ifndef BENILLA_SOURCE_DIRECT_H
#define BENILLA_SOURCE_DIRECT_H
#include "DetourNavMeshQuery.h"
struct BenillaSourceDirect;
// The immutable mesh must outlive the state. Endpoints and filter are copied.
BenillaSourceDirect* benilla_source_direct_create(const dtNavMesh* mesh,
    dtPolyRef startRef, dtPolyRef endRef, const float* startPos, const float* endPos,
    unsigned short include, unsigned short exclude, dtStatus* status);
extern "C" {
void benilla_source_direct_free(BenillaSourceDirect* state);
// Performs exactly one original direct findPath outer-loop expansion when pending.
dtStatus benilla_source_direct_step(BenillaSourceDirect* state, int* done);
// Only a completed search can be finalized. Private capacity is 1..1024, bounded by the
// node pool; callers provide that many output slots. Public Rust admission stays 2..74.
// The returned status is the direct source status.
dtStatus benilla_source_direct_finish(BenillaSourceDirect* state,
    dtPolyRef* path, int* pathCount, int maxPath);
// Read-only counters for source differential tests; never confer polygon authority.
void benilla_source_direct_stats(const BenillaSourceDirect* state,
    unsigned* expansions, unsigned* nodes, unsigned* stateNodes);
}
#endif
