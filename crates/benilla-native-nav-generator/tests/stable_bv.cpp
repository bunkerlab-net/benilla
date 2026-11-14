// SPDX-License-Identifier: Zlib
// Standalone regression for the actual generator builder, never the reader copy.
#ifdef NDEBUG
#undef NDEBUG
#endif
#include "../native/Detour/Source/DetourNavMeshBuilder.cpp"
#include <algorithm>
#include <cassert>
#include <cstddef>
#include <fstream>
#include <iterator>
#include <new>
#include <vector>

struct alignas(std::max_align_t) Block { Block* previous; Block* next; size_t bytes; };
static Block* head = nullptr;
static size_t live = 0, peak = 0, budget = SIZE_MAX;
static int calls = 0, failAt = 0;
static bool throwing = false;
static void* allocate(size_t bytes, dtAllocHint)
{
    ++calls;
    if (calls == failAt || bytes > budget || sizeof(Block) > budget - bytes ||
        live > budget - bytes - sizeof(Block))
    {
        if (throwing) throw std::bad_alloc();
        return nullptr;
    }
    Block* block = static_cast<Block*>(malloc(sizeof(Block) + bytes));
    assert(block);
    *block = {nullptr, head, sizeof(Block) + bytes};
    if (head) head->previous = block;
    head = block;
    live += block->bytes;
    peak = std::max(peak, live);
    memset(block + 1, 0, bytes);
    return block + 1;
}
static void release(void* pointer)
{
    if (!pointer) return;
    Block* block = static_cast<Block*>(pointer) - 1;
    if (block->previous) block->previous->next = block->next;
    else head = block->next;
    if (block->next) block->next->previous = block->previous;
    live -= block->bytes;
    free(block);
}
static void reset()
{
    assert(!head && live == 0);
    calls = failAt = 0; peak = 0; budget = SIZE_MAX; throwing = false;
}
static void reference(std::vector<BVItem> items, std::vector<dtBVNode>& nodes)
{
    dtBVNode node{};
    for (int d = 0; d < 3; ++d)
    {
        node.bmin[d] = items[0].bmin[d]; node.bmax[d] = items[0].bmax[d];
        for (const BVItem& item : items)
        {
            node.bmin[d] = std::min(node.bmin[d], item.bmin[d]);
            node.bmax[d] = std::max(node.bmax[d], item.bmax[d]);
        }
    }
    node.i = items.size() == 1 ? items[0].i : -int(items.size() * 2 - 1);
    nodes.push_back(node);
    if (items.size() == 1) return;
    int axis = 0;
    for (int d = 1; d < 3; ++d)
        if (node.bmax[d] - node.bmin[d] > node.bmax[axis] - node.bmin[axis]) axis = d;
    // Test oracle only: the production implementation cannot use this allocator.
    std::stable_sort(items.begin(), items.end(), [axis](const BVItem& a, const BVItem& b) {
        return a.bmin[axis] < b.bmin[axis];
    });
    const auto split = items.begin() + items.size() / 2;
    reference(std::vector<BVItem>(items.begin(), split), nodes);
    reference(std::vector<BVItem>(split, items.end()), nodes);
}
static void checkTree(std::vector<BVItem> items)
{
    std::vector<dtBVNode> expected;
    reference(items, expected);
    std::vector<BVItem> scratch(items.size());
    std::vector<dtBVNode> actual(items.size() * 2);
    int used = 0;
    subdivide(items.data(), scratch.data(), int(items.size()), 0, int(items.size()), used, actual.data());
    assert(size_t(used) == expected.size());
    assert(memcmp(actual.data(), expected.data(), expected.size() * sizeof(dtBVNode)) == 0);
}
static void ordering()
{
    for (int count : {1, 2, 3, 7, 32, 63, 64, 65, 257})
    {
        std::vector<BVItem> items(count), scratch(count);
        for (int i = 0; i < count; ++i) items[i].i = count - i - 1;
        const auto before = items;
        for (int axis = 0; axis < 3; ++axis)
        {
            stableSortItems(items.data(), scratch.data(), count, axis);
            assert(memcmp(items.data(), before.data(), items.size() * sizeof(BVItem)) == 0);
        }
        checkTree(items); // Equal keys preserve incoming order, even descending IDs.
        for (int i = 0; i < count; ++i)
            for (int d = 0; d < 3; ++d)
                items[i].bmin[d] = items[i].bmax[d] = (unsigned short)((count - i) * (d + 1));
        checkTree(items); // No-tie control.
        for (int seed = 0; seed < 20; ++seed)
        {
            for (int i = 0; i < count; ++i)
                for (int d = 0; d < 3; ++d)
                {
                    items[i].bmin[d] = (unsigned short)((i * (7 + d * 6) + seed) % (3 + d * 5));
                    items[i].bmax[d] = items[i].bmin[d] + (unsigned short)((i + seed + d) % 4);
                }
            checkTree(items); // Mixed ties and changing axes at deeper splits.
        }
    }
}
static void allocationFailures()
{
    unsigned short vertices[] = {0,0,0, 1,0,0, 0,0,1, 2,0,0, 3,0,0, 2,0,1};
    unsigned short polys[] = {0,1,2, 0xffff,0xffff,0xffff, 3,4,5, 0xffff,0xffff,0xffff};
    unsigned short flags[] = {1,1}; unsigned char areas[] = {11,11};
    float connection[] = {0,0,0, 2,0,0}, radius[] = {0.1f};
    unsigned short connectionFlags[] = {1}; unsigned char directions[] = {1}, connectionAreas[] = {11};
    dtNavMeshCreateParams params{};
    params.verts = vertices; params.vertCount = 6; params.polys = polys; params.polyCount = 2;
    params.polyFlags = flags; params.polyAreas = areas; params.nvp = 3;
    params.cs = params.ch = 1; params.bmax[0] = params.bmax[2] = 4; params.buildBvTree = true;
    params.offMeshConCount = 1; params.offMeshConVerts = connection; params.offMeshConRad = radius;
    params.offMeshConFlags = connectionFlags; params.offMeshConDir = directions; params.offMeshConAreas = connectionAreas;
    dtAllocSetCustom(allocate, release);
    unsigned char* output = nullptr; int bytes = 0;
    reset();
    assert(dtCreateNavMeshData(&params, &output, &bytes));
    assert(calls == 4); // Off-mesh classification, output, BV items, BV scratch.
    const size_t exactPeak = peak;
    std::vector<unsigned char> expected(output, output + bytes);
    dtFree(output);
    for (bool throws : {false, true})
        for (int failure = 1; failure <= 5; ++failure)
        {
            reset(); throwing = throws;
            if (failure < 5) failAt = failure;
            else budget = exactPeak - 1; // Reject the scratch allocation itself.
            output = nullptr; bytes = -7;
            try { assert(!dtCreateNavMeshData(&params, &output, &bytes)); assert(!throws); }
            catch (const std::bad_alloc&) { assert(throws); }
            assert(output == nullptr && bytes == -7); // No partial publication.
            if (!throws) assert(!head && live == 0);
            // The production Hooks registry also reclaims source-local data on throw.
            while (head) release(head + 1);
            reset(); budget = exactPeak;
            assert(dtCreateNavMeshData(&params, &output, &bytes));
            assert(peak == exactPeak && size_t(bytes) == expected.size());
            assert(memcmp(output, expected.data(), expected.size()) == 0);
            dtFree(output);
        }
    reset();
    for (int invalid : {-1, 0, INT_MAX})
    {
        params.polyCount = invalid;
        output = nullptr; bytes = -7;
        assert(!dtCreateNavMeshData(&params, &output, &bytes));
        assert(createBVTree(&params, nullptr, 0) == 0);
        assert(calls == 0 && !output && bytes == -7);
    }
    dtAllocSetCustom(nullptr, nullptr);
}
static void fixture(const char* path)
{
    std::ifstream stream(path, std::ios::binary);
    assert(stream);
    const std::vector<unsigned char> bytes{std::istreambuf_iterator<char>(stream), {}};
    assert(bytes.size() >= 120);
    dtMeshHeader header{}; memcpy(&header, bytes.data() + 20, sizeof(header));
    assert(header.magic == DT_NAVMESH_MAGIC && header.version == DT_NAVMESH_VERSION);
    assert(header.polyCount > 0 && header.bvNodeCount == header.polyCount * 2);
    const size_t offset = 20 + sizeof(header) + header.vertCount * 12 + header.polyCount * sizeof(dtPoly) +
        header.maxLinkCount * sizeof(dtLink) + header.detailMeshCount * sizeof(dtPolyDetail) +
        header.detailVertCount * 12 + header.detailTriCount * 4;
    assert(offset + header.bvNodeCount * sizeof(dtBVNode) == bytes.size());
    std::vector<BVItem> items(header.polyCount), scratch(header.polyCount);
    std::vector<bool> seen(header.polyCount);
    for (int i = 0; i < header.bvNodeCount - 1; ++i)
    {
        dtBVNode node{}; memcpy(&node, bytes.data() + offset + i * sizeof(node), sizeof(node));
        if (node.i < 0) continue;
        assert(node.i < header.polyCount && !seen[node.i]); seen[node.i] = true;
        memcpy(items[node.i].bmin, node.bmin, sizeof(node.bmin));
        memcpy(items[node.i].bmax, node.bmax, sizeof(node.bmax)); items[node.i].i = node.i;
    }
    assert(std::all_of(seen.begin(), seen.end(), [](bool value) { return value; }));
    std::vector<dtBVNode> rebuilt(header.bvNodeCount);
    int used = 0;
    subdivide(items.data(), scratch.data(), header.polyCount, 0, header.polyCount, used, rebuilt.data());
    assert(used == header.bvNodeCount - 1);
    assert(memcmp(rebuilt.data(), bytes.data() + offset, rebuilt.size() * sizeof(dtBVNode)) == 0);
}
int main(int argc, char** argv)
{
    static_assert(sizeof(BVItem) == 16 && sizeof(dtBVNode) == 16 && sizeof(dtLink) == 16);
    ordering(); allocationFailures();
    for (int i = 1; i < argc; ++i) fixture(argv[i]);
    puts("stable BV policy v1: ordering, exact fixture trees, allocation failure/retry passed");
}
