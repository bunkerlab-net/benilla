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

// SPDX-License-Identifier: GPL-3.0-or-later
// Altered extraction by the 2026 Benilla fork contributors, outside vendored Detour.
// Pinned CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f,
// dep/recastnavigation/Detour/Source/DetourNavMeshQuery.cpp SHA256
// fb3f040b35633ad01dfec2a18e1dd1433b10ede69780dddf65bd0a016af0183b.
// Copies direct findPath/getPathToNode, default filter arithmetic and ground-only
// getPortalPoints/getEdgeMidPoint; the outer loop is resumable, not sliced findPath.
// CMaNGOS MoveMap.cpp selects 1024 nodes; its dtNavMeshQuery::init selects hash 256.
#include "source_direct.h"
#include "DetourNode.h"
#include "DetourCommon.h"
#include "DetourAssert.h"
#include <new>
#include <initializer_list>

#ifdef DT_VIRTUAL_QUERYFILTER
#error "Source direct boundary requires the pinned default nonvirtual query filter"
#endif

// The original default methods are inline in another translation unit. Retain their
// exact expressions here, using public getters for the copied filter's private fields.
struct SourceDirectFilter : dtQueryFilter {
    bool passFilter(const dtPolyRef, const dtMeshTile*, const dtPoly* poly) const {
        return (poly->flags & getIncludeFlags()) != 0 && (poly->flags & getExcludeFlags()) == 0;
    }
    float getCost(const float* pa, const float* pb,
        const dtPolyRef, const dtMeshTile*, const dtPoly*,
        const dtPolyRef, const dtMeshTile*, const dtPoly* curPoly,
        const dtPolyRef, const dtMeshTile*, const dtPoly*) const {
        return dtVdist(pa, pb) * getAreaCost(curPoly->getArea());
    }
};
static const float H_SCALE = 0.999f;
struct BenillaSourceDirect {
    const dtNavMesh* m_nav;
    dtNodePool nodePool;
    dtNodeQueue openList;
    dtNodePool* m_nodePool;
    dtNodeQueue* m_openList;
    SourceDirectFilter ownedFilter;
    const SourceDirectFilter* filter;
    dtPolyRef startRef, endRef;
    float startPos[3], endPos[3];
    dtNode* lastBestNode;
    float lastBestNodeCost;
    bool outOfNodes;
    dtStatus status;
    unsigned expansions;
    BenillaSourceDirect(const dtNavMesh* mesh): m_nav(mesh), nodePool(1024, 256),
        openList(1024), m_nodePool(&nodePool), m_openList(&openList),
        filter(&ownedFilter), startRef(0), endRef(0), startPos{}, endPos{},
        lastBestNode(nullptr), lastBestNodeCost(0), outOfNodes(false), status(DT_FAILURE), expansions(0) {}
    BenillaSourceDirect(const BenillaSourceDirect&) = delete;
    BenillaSourceDirect& operator=(const BenillaSourceDirect&) = delete;
    dtStatus step();
    dtStatus getPathToNode(dtNode* endNode, dtPolyRef* path, int* pathCount, int maxPath) const;
    dtStatus getPortalPoints(dtPolyRef from, const dtPoly* fromPoly, const dtMeshTile* fromTile,
        dtPolyRef to, const dtPoly* toPoly, const dtMeshTile* toTile,
        float* left, float* right) const;
    dtStatus getEdgeMidPoint(dtPolyRef from, const dtPoly* fromPoly, const dtMeshTile* fromTile,
        dtPolyRef to, const dtPoly* toPoly, const dtMeshTile* toTile, float* mid) const;
};

BenillaSourceDirect* benilla_source_direct_create(const dtNavMesh* mesh,
    dtPolyRef startRef, dtPolyRef endRef, const float* startPos, const float* endPos,
    unsigned short include, unsigned short exclude, dtStatus* status) {
    if (!status) return nullptr;
    *status = DT_FAILURE | DT_INVALID_PARAM;
    if (!mesh || !mesh->isValidPolyRef(startRef) || !mesh->isValidPolyRef(endRef) ||
        !startPos || !dtVisfinite(startPos) || !endPos || !dtVisfinite(endPos) ||
        include != 1 || exclude != 0) return nullptr;
    const dtMeshTile* tile = nullptr;
    const dtPoly* poly = nullptr;
    for (dtPolyRef ref : {startRef, endRef}) {
        if (dtStatusFailed(mesh->getTileAndPolyByRef(ref, &tile, &poly)) ||
            poly->getType() != DT_POLYTYPE_GROUND || poly->flags != 1 || poly->getArea() != 11)
            return nullptr;
    }
    BenillaSourceDirect* s = new(std::nothrow) BenillaSourceDirect(mesh);
    if (!s || !s->nodePool.isValid() || !s->openList.isValid()) {
        delete s;
        *status = DT_FAILURE | DT_OUT_OF_MEMORY;
        return nullptr;
    }
    s->ownedFilter.setIncludeFlags(include);
    s->ownedFilter.setExcludeFlags(exclude);
    s->startRef = startRef; s->endRef = endRef;
    dtVcopy(s->startPos, startPos); dtVcopy(s->endPos, endPos);
    if (startRef == endRef) {
        s->status = *status = DT_SUCCESS;
        return s;
    }
    s->m_nodePool->clear();
    s->m_openList->clear();
    dtNode* startNode = s->m_nodePool->getNode(startRef);
    dtVcopy(startNode->pos, startPos);
    startNode->pidx = 0;
    startNode->cost = 0;
    startNode->total = dtVdist(startPos, endPos) * H_SCALE;
    startNode->id = startRef;
    startNode->flags = DT_NODE_OPEN;
    s->m_openList->push(startNode);
    s->lastBestNode = startNode;
    s->lastBestNodeCost = startNode->total;
    s->status = *status = DT_IN_PROGRESS;
    return s;
}
extern "C" void benilla_source_direct_free(BenillaSourceDirect* state) { delete state; }

dtStatus BenillaSourceDirect::step() {
    // The caller checks pending status; each call consumes one original outer expansion.
    dtAssert(status == DT_IN_PROGRESS && !m_openList->empty());
	// Remove node from open list and put it in closed list.
	dtNode* bestNode = m_openList->pop();
	bestNode->flags &= ~DT_NODE_OPEN;
	bestNode->flags |= DT_NODE_CLOSED;

	// Reached the goal, stop searching.
	if (bestNode->id == endRef)
	{
		lastBestNode = bestNode;
		return status = DT_SUCCESS;
	}

	// Get current poly and tile.
	// The API input has been cheked already, skip checking internal data.
	const dtPolyRef bestRef = bestNode->id;
	const dtMeshTile* bestTile = 0;
	const dtPoly* bestPoly = 0;
	m_nav->getTileAndPolyByRefUnsafe(bestRef, &bestTile, &bestPoly);

	// Get parent poly and tile.
	dtPolyRef parentRef = 0;
	const dtMeshTile* parentTile = 0;
	const dtPoly* parentPoly = 0;
	if (bestNode->pidx)
		parentRef = m_nodePool->getNodeAtIdx(bestNode->pidx)->id;
	if (parentRef)
		m_nav->getTileAndPolyByRefUnsafe(parentRef, &parentTile, &parentPoly);

	for (unsigned int i = bestPoly->firstLink; i != DT_NULL_LINK; i = bestTile->links[i].next)
	{
		dtPolyRef neighbourRef = bestTile->links[i].ref;

		// Skip invalid ids and do not expand back to where we came from.
		if (!neighbourRef || neighbourRef == parentRef)
			continue;

		// Get neighbour poly and tile.
		// The API input has been cheked already, skip checking internal data.
		const dtMeshTile* neighbourTile = 0;
		const dtPoly* neighbourPoly = 0;
		m_nav->getTileAndPolyByRefUnsafe(neighbourRef, &neighbourTile, &neighbourPoly);

		if (!filter->passFilter(neighbourRef, neighbourTile, neighbourPoly))
			continue;

		// deal explicitly with crossing tile boundaries
		unsigned char crossSide = 0;
		if (bestTile->links[i].side != 0xff)
			crossSide = bestTile->links[i].side >> 1;

		// get the node
		dtNode* neighbourNode = m_nodePool->getNode(neighbourRef, crossSide);
		if (!neighbourNode)
		{
			outOfNodes = true;
			continue;
		}

		// If the node is visited the first time, calculate node position.
		if (neighbourNode->flags == 0)
		{
			// Validation guarantees ground adjacency. Reject rather than use an
			// unwritten midpoint if an internal invariant is ever violated.
			if (dtStatusFailed(getEdgeMidPoint(bestRef, bestPoly, bestTile,
								neighbourRef, neighbourPoly, neighbourTile,
								neighbourNode->pos)))
				return status = DT_FAILURE | DT_INVALID_PARAM;
		}

		// Calculate cost and heuristic.
		float cost = 0;
		float heuristic = 0;

		// Special case for last node.
		if (neighbourRef == endRef)
		{
			// Cost
			const float curCost = filter->getCost(bestNode->pos, neighbourNode->pos,
												  parentRef, parentTile, parentPoly,
												  bestRef, bestTile, bestPoly,
												  neighbourRef, neighbourTile, neighbourPoly);
			const float endCost = filter->getCost(neighbourNode->pos, endPos,
												  bestRef, bestTile, bestPoly,
												  neighbourRef, neighbourTile, neighbourPoly,
												  0, 0, 0);

			cost = bestNode->cost + curCost + endCost;
			heuristic = 0;
		}
		else
		{
			// Cost
			const float curCost = filter->getCost(bestNode->pos, neighbourNode->pos,
												  parentRef, parentTile, parentPoly,
												  bestRef, bestTile, bestPoly,
												  neighbourRef, neighbourTile, neighbourPoly);
			cost = bestNode->cost + curCost;
			heuristic = dtVdist(neighbourNode->pos, endPos)*H_SCALE;
		}

		const float total = cost + heuristic;

		// The node is already in open list and the new result is worse, skip.
		if ((neighbourNode->flags & DT_NODE_OPEN) && total >= neighbourNode->total)
			continue;
		// The node is already visited and process, and the new result is worse, skip.
		if ((neighbourNode->flags & DT_NODE_CLOSED) && total >= neighbourNode->total)
			continue;

		// Add or update the node.
		neighbourNode->pidx = m_nodePool->getNodeIdx(bestNode);
		neighbourNode->id = neighbourRef;
		neighbourNode->flags = (neighbourNode->flags & ~DT_NODE_CLOSED);
		neighbourNode->cost = cost;
		neighbourNode->total = total;

		if (neighbourNode->flags & DT_NODE_OPEN)
		{
			// Already in open, update node location.
			m_openList->modify(neighbourNode);
		}
		else
		{
			// Put the node in open list.
			neighbourNode->flags |= DT_NODE_OPEN;
			m_openList->push(neighbourNode);
		}

		// Update nearest node to target so far.
		if (heuristic < lastBestNodeCost)
		{
			lastBestNodeCost = heuristic;
			lastBestNode = neighbourNode;
		}
	}
    return status = m_openList->empty() ? DT_SUCCESS : DT_IN_PROGRESS;
}
extern "C" dtStatus benilla_source_direct_step(BenillaSourceDirect* s, int* done) {
    if (!done) return DT_FAILURE | DT_INVALID_PARAM;
    *done = 0;
    if (!s) return DT_FAILURE | DT_INVALID_PARAM;
    if (s->status != DT_IN_PROGRESS) return s->status;
    *done = 1;
    ++s->expansions;
    return s->step();
}

dtStatus BenillaSourceDirect::getPathToNode(dtNode* endNode, dtPolyRef* path, int* pathCount, int maxPath) const
{
	// Find the length of the entire path.
	dtNode* curNode = endNode;
	int length = 0;
	do
	{
        // Valid source parents form an acyclic chain. Bound the final walk even
        // if that invariant is violated; never publish a partial reconstruction.
        if (length >= 1024 || curNode->pidx > (unsigned)m_nodePool->getNodeCount())
            return DT_FAILURE | DT_INVALID_PARAM;
        length++;
        curNode = m_nodePool->getNodeAtIdx(curNode->pidx);
	} while (curNode);

	// If the path cannot be fully stored then advance to the last node we will be able to store.
	curNode = endNode;
	int writeCount;
	for (writeCount = length; writeCount > maxPath; writeCount--)
	{
		dtAssert(curNode);

		curNode = m_nodePool->getNodeAtIdx(curNode->pidx);
	}

	// Write path
	for (int i = writeCount - 1; i >= 0; i--)
	{
		dtAssert(curNode);

		path[i] = curNode->id;
		curNode = m_nodePool->getNodeAtIdx(curNode->pidx);
	}

	dtAssert(!curNode);

	*pathCount = dtMin(length, maxPath);

	if (length > maxPath)
		return DT_SUCCESS | DT_BUFFER_TOO_SMALL;

	return DT_SUCCESS;
}

extern "C" dtStatus benilla_source_direct_finish(BenillaSourceDirect* s,
    dtPolyRef* path, int* pathCount, int maxPath) {
    if (!pathCount) return DT_FAILURE | DT_INVALID_PARAM;
    *pathCount = 0;
    if (!s || !path || maxPath < 1 || maxPath > 1024 || s->status != DT_SUCCESS)
        return DT_FAILURE | DT_INVALID_PARAM;
    if (s->startRef == s->endRef) {
        path[0] = s->startRef;
        *pathCount = 1;
        return DT_SUCCESS;
    }
    dtStatus status = s->getPathToNode(s->lastBestNode, path, pathCount, maxPath);
    if (s->lastBestNode->id != s->endRef)
        status |= DT_PARTIAL_RESULT;
    if (s->outOfNodes)
        status |= DT_OUT_OF_NODES;
    return status;
}

dtStatus BenillaSourceDirect::getPortalPoints(dtPolyRef from, const dtPoly* fromPoly, const dtMeshTile* fromTile,
										 dtPolyRef to, const dtPoly* toPoly, const dtMeshTile* toTile,
										 float* left, float* right) const
{
	// Find the link that points to the 'to' polygon.
	const dtLink* link = 0;
	for (unsigned int i = fromPoly->firstLink; i != DT_NULL_LINK; i = fromTile->links[i].next)
	{
		if (fromTile->links[i].ref == to)
		{
			link = &fromTile->links[i];
			break;
		}
	}
	if (!link)
		return DT_FAILURE | DT_INVALID_PARAM;

    // Ground-only admitted products cannot take the original off-mesh branches.
    if (fromPoly->getType() != DT_POLYTYPE_GROUND || toPoly->getType() != DT_POLYTYPE_GROUND)
        return DT_FAILURE | DT_INVALID_PARAM;
    (void)from;
    (void)toTile;

	// Find portal vertices.
	const int v0 = fromPoly->verts[link->edge];
	const int v1 = fromPoly->verts[(link->edge+1) % (int)fromPoly->vertCount];
	dtVcopy(left, &fromTile->verts[v0*3]);
	dtVcopy(right, &fromTile->verts[v1*3]);

	// If the link is at tile boundary, dtClamp the vertices to
	// the link width.
	if (link->side != 0xff)
	{
		// Unpack portal limits.
		if (link->bmin != 0 || link->bmax != 255)
		{
			const float s = 1.0f/255.0f;
			const float tmin = link->bmin*s;
			const float tmax = link->bmax*s;
			dtVlerp(left, &fromTile->verts[v0*3], &fromTile->verts[v1*3], tmin);
			dtVlerp(right, &fromTile->verts[v0*3], &fromTile->verts[v1*3], tmax);
		}
	}

	return DT_SUCCESS;
}

dtStatus BenillaSourceDirect::getEdgeMidPoint(dtPolyRef from, const dtPoly* fromPoly, const dtMeshTile* fromTile,
										 dtPolyRef to, const dtPoly* toPoly, const dtMeshTile* toTile,
										 float* mid) const
{
	float left[3], right[3];
	if (dtStatusFailed(getPortalPoints(from, fromPoly, fromTile, to, toPoly, toTile, left, right)))
		return DT_FAILURE | DT_INVALID_PARAM;
	mid[0] = (left[0]+right[0])*0.5f;
	mid[1] = (left[1]+right[1])*0.5f;
	mid[2] = (left[2]+right[2])*0.5f;
	return DT_SUCCESS;
}

extern "C" void benilla_source_direct_stats(const BenillaSourceDirect* s,
    unsigned* expansions, unsigned* nodes, unsigned* stateNodes) {
    if (!s || !expansions || !nodes || !stateNodes) return;
    *expansions = s->expansions;
    *nodes = unsigned(s->nodePool.getNodeCount());
    *stateNodes = 0;
    for (unsigned i = 1; i <= *nodes; ++i)
        if (s->nodePool.getNodeAtIdx(i)->state != 0) ++*stateNodes;
}
