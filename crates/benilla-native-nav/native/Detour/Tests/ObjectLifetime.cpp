// SPDX-License-Identifier: GPL-3.0-or-later
// Benilla-authored C++20 lifetime/type-layout checks; no game data.
// N4861 [class.prop]/9: a trivial eligible constructor and a trivial,
// non-deleted destructor suffice for an implicit-lifetime class.
// https://timsong-cpp.github.io/cppwp/n4861/class.prop#9
// malloc starts implicit lifetimes; memset itself does not.
// https://timsong-cpp.github.io/cppwp/n4861/c.malloc
#include "DetourNavMesh.h"
#include "DetourNavMeshBuilder.h"
#include "DetourNavMeshQuery.h"
#include "DetourNode.h"

#include <cstddef>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <type_traits>

static_assert(__cplusplus >= 202002L, "the raw-storage bridge requires C++20");

// C++20 has no standard is_implicit_lifetime trait. These two properties
// establish the sufficient constructor/destructor branch of its definition.
template<class T>
constexpr bool hasTrivialLifetime = std::is_trivially_default_constructible<T>::value
	&& std::is_trivially_destructible<T>::value;

static_assert(hasTrivialLifetime<dtMeshTile>, "mesh tiles need implicit lifetime");
static_assert(std::is_standard_layout<dtMeshTile>::value, "mesh tile layout is fixed");
static_assert(!std::is_copy_constructible<dtMeshTile>::value, "mesh tiles remain noncopyable");
static_assert(!std::is_copy_assignable<dtMeshTile>::value, "mesh tiles remain nonassignable");

// The other raw-allocation/serialized public record types already satisfy
// stronger properties, including the byte-copy guarantee used for tile data.
#define CHECK_RAW_RECORD(T) \
	static_assert(hasTrivialLifetime<T>, #T " requires implicit lifetime"); \
	static_assert(std::is_trivially_copyable<T>::value, #T " requires byte-copyable storage"); \
	static_assert(std::is_standard_layout<T>::value, #T " requires standard layout")
CHECK_RAW_RECORD(dtPoly);
CHECK_RAW_RECORD(dtPolyDetail);
CHECK_RAW_RECORD(dtLink);
CHECK_RAW_RECORD(dtBVNode);
CHECK_RAW_RECORD(dtOffMeshConnection);
CHECK_RAW_RECORD(dtMeshHeader);
CHECK_RAW_RECORD(dtNavMeshParams);
CHECK_RAW_RECORD(dtNavMeshCreateParams);
CHECK_RAW_RECORD(dtNode);
CHECK_RAW_RECORD(dtRaycastHit);
CHECK_RAW_RECORD(dtNodeIndex);
CHECK_RAW_RECORD(dtNode*);
CHECK_RAW_RECORD(dtMeshTile*);
CHECK_RAW_RECORD(float);
CHECK_RAW_RECORD(unsigned char);
#undef CHECK_RAW_RECORD

// Reproduce the upstream member layout and its inaccessible, user-declared
// copy members. Compare every field, not just the aggregate size.
struct UpstreamMeshTileLayout
{
	unsigned int salt;
	unsigned int linksFreeList;
	dtMeshHeader* header;
	dtPoly* polys;
	float* verts;
	dtLink* links;
	dtPolyDetail* detailMeshes;
	float* detailVerts;
	unsigned char* detailTris;
	dtBVNode* bvTree;
	dtOffMeshConnection* offMeshCons;
	unsigned char* data;
	int dataSize;
	int flags;
	dtMeshTile* next;
private:
	UpstreamMeshTileLayout(const UpstreamMeshTileLayout&);
	UpstreamMeshTileLayout& operator=(const UpstreamMeshTileLayout&);
};

static_assert(!std::is_default_constructible<UpstreamMeshTileLayout>::value,
	"the upstream copy declaration suppresses its implicit default constructor");
static_assert(!std::is_aggregate<UpstreamMeshTileLayout>::value,
	"the upstream type cannot use the aggregate implicit-lifetime branch");
static_assert(sizeof(dtMeshTile) == sizeof(UpstreamMeshTileLayout), "tile size changed");
static_assert(alignof(dtMeshTile) == alignof(UpstreamMeshTileLayout), "tile alignment changed");
#define CHECK_TILE_OFFSET(member) \
	static_assert(offsetof(dtMeshTile, member) == offsetof(UpstreamMeshTileLayout, member), \
		"tile offset changed: " #member)
CHECK_TILE_OFFSET(salt);
CHECK_TILE_OFFSET(linksFreeList);
CHECK_TILE_OFFSET(header);
CHECK_TILE_OFFSET(polys);
CHECK_TILE_OFFSET(verts);
CHECK_TILE_OFFSET(links);
CHECK_TILE_OFFSET(detailMeshes);
CHECK_TILE_OFFSET(detailVerts);
CHECK_TILE_OFFSET(detailTris);
CHECK_TILE_OFFSET(bvTree);
CHECK_TILE_OFFSET(offMeshCons);
CHECK_TILE_OFFSET(data);
CHECK_TILE_OFFSET(dataSize);
CHECK_TILE_OFFSET(flags);
CHECK_TILE_OFFSET(next);
#undef CHECK_TILE_OFFSET

int main()
{
	// Exercise the exact array allocation pattern under ASan/UBSan, and check
	// the zero-pointer representation assumed by this supported target ABI.
	const size_t count = 64;
	void* storage = std::malloc(sizeof(dtMeshTile) * count);
	if (!storage)
		return 1;
	std::memset(storage, 0, sizeof(dtMeshTile) * count);
	dtMeshTile* tiles = static_cast<dtMeshTile*>(storage);
	for (size_t i = 0; i < count; ++i)
	{
		if (tiles[i].salt || tiles[i].linksFreeList || tiles[i].header
			|| tiles[i].polys || tiles[i].verts || tiles[i].links
			|| tiles[i].detailMeshes || tiles[i].detailVerts || tiles[i].detailTris
			|| tiles[i].bvTree || tiles[i].offMeshCons || tiles[i].data
			|| tiles[i].dataSize || tiles[i].flags || tiles[i].next)
		{
			std::free(storage);
			return 1;
		}
		tiles[i].salt = 1;
		tiles[i].next = i + 1 < count ? &tiles[i + 1] : 0;
	}
	std::free(storage);
	std::printf("PASS: C++20 raw-record lifetime traits, all tile field offsets, size=%zu alignment=%zu, malloc array initialization\n",
		sizeof(dtMeshTile), alignof(dtMeshTile));
	return 0;
}
