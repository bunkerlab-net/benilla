// SPDX-License-Identifier: GPL-3.0-or-later
// Benilla-authored regression test for the marked Detour allocation safeguards.
// Uses no game data and no Cargo build. See run-allocation-failure.sh.
#include "DetourAlloc.h"
#include "DetourNavMesh.h"
#include "DetourNavMeshQuery.h"

#include <cstdio>
#include <cstdlib>

static size_t allocationCalls = 0;
static size_t failAt = 0;
static size_t liveAllocations = 0;

static void require(bool condition, const char* message)
{
	if (!condition)
	{
		std::fprintf(stderr, "FAIL: %s\n", message);
		std::exit(1);
	}
}

static void* faultAlloc(size_t size, dtAllocHint)
{
	++allocationCalls;
	if (failAt && allocationCalls == failAt)
		return 0;
	void* result = std::malloc(size);
	if (result)
		++liveAllocations;
	return result;
}

static void faultFree(void* ptr)
{
	if (ptr)
	{
		require(liveAllocations > 0, "allocation accounting underflow");
		--liveAllocations;
		std::free(ptr);
	}
}

static void resetFault(size_t ordinal = 0)
{
	allocationCalls = 0;
	failAt = ordinal;
}

static bool outOfMemory(dtStatus status)
{
	return dtStatusFailed(status) && dtStatusDetail(status, DT_OUT_OF_MEMORY);
}

int main()
{
	dtAllocSetCustom(faultAlloc, faultFree);
	dtNavMeshParams params = {};
	params.tileWidth = 1.0f;
	params.tileHeight = 1.0f;
	params.maxTiles = 64;
	params.maxPolys = 4096;

	// Both mesh-init allocation failures must permit safe destruction.
	for (size_t ordinal = 1; ordinal <= 2; ++ordinal)
	{
		resetFault();
		dtNavMesh* nav = dtAllocNavMesh();
		require(nav != 0, "allocate mesh object");
		resetFault(ordinal);
		require(outOfMemory(nav->init(&params)), "mesh init reports allocation failure");
		dtFreeNavMesh(nav);
		require(liveAllocations == 0, "mesh failure releases all allocations");
	}

	resetFault();
	dtNavMesh* nav = dtAllocNavMesh();
	require(nav && dtStatusSucceed(nav->init(&params)), "initialize empty mesh");
	const size_t meshAllocations = liveAllocations;

	resetFault(1);
	require(dtAllocNavMeshQuery() == 0, "query object allocation can fail");
	require(liveAllocations == meshAllocations, "failed query object has no allocation");

	// Discover counts, then exercise every individual allocation in fresh and
	// resized initialization, both with immediate destruction and with a retry.
	for (int resizing = 0; resizing <= 1; ++resizing)
	{
		resetFault();
		dtNavMeshQuery* probe = dtAllocNavMeshQuery();
		require(probe != 0, "allocate query probe");
		if (resizing)
			require(dtStatusSucceed(probe->init(nav, 64)), "initialize small query probe");
		resetFault();
		require(dtStatusSucceed(probe->init(nav, 512)), "initialize query probe");
		const size_t allocationCount = allocationCalls;
		require(allocationCount == (resizing ? 6u : 10u), "expected allocation coverage");
		dtFreeNavMeshQuery(probe);
		require(liveAllocations == meshAllocations, "query probe releases allocations");

		for (size_t ordinal = 1; ordinal <= allocationCount; ++ordinal)
		{
			for (int retry = 0; retry <= 1; ++retry)
			{
				resetFault();
				dtNavMeshQuery* query = dtAllocNavMeshQuery();
				require(query != 0, "allocate query");
				if (resizing)
					require(dtStatusSucceed(query->init(nav, 64)), "initialize small query");
				resetFault(ordinal);
				require(outOfMemory(query->init(nav, 512)), "query init reports allocation failure");
				if (retry)
				{
					resetFault();
					require(dtStatusSucceed(query->init(nav, 512)), "query init succeeds after allocation recovery");
					const float center[3] = {0, 0, 0};
					const float extents[3] = {1, 1, 1};
					float nearest[3] = {0, 0, 0};
					dtPolyRef nearestRef = 123;
					dtQueryFilter filter;
					require(dtStatusSucceed(query->findNearestPoly(center, extents, &filter, &nearestRef, nearest)), "recovered query is usable");
					require(nearestRef == 0, "empty mesh yields no nearest polygon");
				}
				dtFreeNavMeshQuery(query);
				require(liveAllocations == meshAllocations, "failed or recovered query releases allocations");
			}
		}
	}

	resetFault();
	dtNavMeshQuery* query = dtAllocNavMeshQuery();
	require(query != 0, "allocate validation query");
	require(dtStatusFailed(query->init(0, 512)), "reject null navmesh");
	require(dtStatusFailed(query->init(nav, 0)), "reject zero nodes");
	require(dtStatusFailed(query->init(nav, -1)), "reject negative nodes");
	for (int nodes = 1; nodes <= 4; ++nodes)
		require(dtStatusSucceed(query->init(nav, nodes)), "small positive node counts have nonzero hash capacity");
	dtFreeNavMeshQuery(query);
	dtFreeNavMesh(nav);
	require(liveAllocations == 0, "all allocations released");
	dtAllocSetCustom(0, 0);
	std::puts("PASS: mesh failures, 10 fresh and 6 resize query allocation failures, destruction/retry, and invalid/small node counts");
	return 0;
}
