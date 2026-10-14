# Pinned dry MMap query boundary

This private crate loads actual serialized CMaNGOS MMap/Detour products and runs the
pinned native nearest-poly, sliced corridor and straight-path primitives. It is not
CMaNGOS owner-dependent PathFinder, a body/slope proof, or native gameplay admission.
No game assets or third-party database rows are bundled. The fixtures are authored
numeric polygons serialized by the original source writer.

## Source, license and exact deviations

Source: [CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f](https://github.com/cmangos/mangos-classic/tree/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f).
Its dep/recastnavigation tree is 9694f7477cd03d3838322160833b2c995f6e5686.
The vendored Detour files retain Mikko Mononen's original notices and
[license](native/DETOUR-LICENSE.txt); [upstream hashes](native/UPSTREAM-SHA256SUMS)
identify their exact originals. The new validator, bridge, product/provider integration
and authored harness are 2026 Benilla fork contributors, GPL-3.0-or-later. CMaNGOS
coordinate order, native MMap layout, ground flags and near/far search bounds derive
from its MoveMapSharedDefines.h, MapBuilder.cpp and PathFinder.cpp. Its original
[notice](../benilla-app/src/net/p2p/world/native_geometry/products/CMANGOS-NOTICE.txt)
and [GPL license](../benilla-sim/licenses/GPL-3.0.txt) remain available.

The native query algorithms are unchanged. The following marked safety alterations
are necessary before using the old source through a safe Rust interface:

- DetourNavMesh.cpp resets m_maxTiles on either initialization allocation failure, so
  destruction does not traverse null or uninitialized tile storage
- DetourNavMeshQuery.cpp checks allocation before placement-new and detects/cleans up
  partially allocated node pools and queues; small node-count hash tables have at least
  one entry
- DetourNode.cpp/.h avoid null memset and expose allocation-validity predicates
- DetourNavMesh.h gives dtMeshTile an explicitly defaulted trivial default constructor,
  establishing C++20 implicit-lifetime allocation while retaining noncopyability,
  size, alignment and every field offset

The wrapper uses only the default malloc-backed Detour allocator. No custom allocator
or global allocator-setting API is exposed to Rust. The separate fault-injection test
process temporarily installs its own malloc-backed allocator and restores it.

## ABI, ownership and alignment

Only MMap magic 0x4d4d4150/version 8 and Detour version 7, little-endian IEEE binary32,
DT_POLYREF64 (64-bit refs), 100-byte header, 32-byte polygon, 16-byte link,
12-byte detail and 16-byte BV node are admitted. C++ static assertions pin sizes,
alignment and selected offsets; the lifetime test pins the complete tile layout.
Parameters preserve source maxPolys raw bits 0x80000000, not a signed allocation count.

Build targets are explicitly x86_64 GNU Linux, aarch64 GNU Linux, x86_64 MSVC Windows,
and aarch64 macOS. One build-script cfg selects the compiled bridge and Rust FFI
consistently. Other targets return Unsupported without a native link. Build requires
C++20; GCC/Clang disable fast-math and FP contraction, MSVC uses /fp:strict. Linux
x86-64 is locally tested. The other three target results require their native CI;
accepting their ABI at compile time does not claim tested cross-platform parity.

All inputs are validated before native init or addTile. Native addTile does not validate
its dataSize, so no merely envelope-checked bytes reach it. The bridge copies each tile
into separately owned malloc storage and passes flags=0. Detour's serialized sections
use dtAlign4, although dtLink is eight-aligned in this profile. The wrapper selects a
base offset of zero or four bytes so the links are eight-aligned; every other section
is four-aligned. No serialized offset, byte, polygon reference or numeric data changes.

GCC can warn that `findConnectingPolys` slab bounds may be uninitialized: the
upstream helper initializes only cardinal sides. The Rust validator admits only
external neighbor values 0x8000/0x8002/0x8004/0x8006. The sole native caller derives
`dir` from those immutable validated values and passes `dtOppositeTile(dir)`, so
both slab endpoints are initialized on every admitted call. Diagonal neighbor
passes skip these portals. This warning is not suppressed; the unsupported generic
upstream input path must remain unreachable through the wrapper.
The original allocation pointer is retained, queries are destroyed first, then the
mesh, then each original buffer exactly once. Partial construction has the same RAII
cleanup. Mesh is neither Send nor Sync, and query methods require exclusive access.
The app snapshot uses a non-reentrant RefCell in the existing NonSend service.

## Admitted data and explicit limits

- At most 64 uniquely keyed layer-zero tiles, 4096 total polygons, 16 MiB native input
  and 4096 allocated native tile slots; inventory separately caps total input at 64 MiB
- At most 65,534 vertices per tile, 73,728 links per tile, 65,536 total extra detail
  vertices plus detail triangles, and 2*polyCount BV slots per tile
- Exact framing, count/width arithmetic, map filename grid versus native origin-relative
  tile coordinates, bounded finite coordinates (absolute value <=100,000), source
  grid width 533.3333, bounded dimensions, and safe quantization ranges
- Ground type/area 11/flag 1 only; no off-mesh links, water/magma polygons, steep flags,
  extra layers or unknown flags. The generator's usesLiquids boolean alone is not an
  actual liquid polygon; both boolean values are permitted with the ground checks
- Clockwise strictly convex 3–6-vertex polygons; finite nondegenerate detail triangles;
  exact reciprocal internal neighbors and reversed shared edges; genuine, complete
  detail boundary edges with reversed internal edges and no duplicate directed edges
- Canonical zero fresh link slots, exact source link capacity, valid contiguous detail
  ranges, complete BV subtree structure/leaf coverage/enclosing quantized bounds, and
  depth <=32. The source writer's final all-zero BV padding node is preserved; native
  queries consequently retain its polygon-zero callback behavior
- External portals only on exact cardinal tile faces, with nonzero projected spans.
  Loaded cross-tile matches must be exact one-to-one reversed endpoints. Partial or
  split portals reject, avoiding the source's four-match truncation and dropped links
- A preflight upper bound of 1,048,576 native construction edge examinations, including
  each portal times every neighboring target edge, plus source-side scans. Cross-tile
  validation separately caps comparisons at 1,048,576. Oversized closures reject

This is intentionally a restricted source-compatible subset; it may reject lawful
source-generated data with narrow/degenerate triangles, partial portals or larger
closures. It does not reinterpret rejection as empty or traversable geometry. Bounds
cover CPU loops and allocation sizes, not hard operating-system scheduling. Peak memory
includes the inventory, validation scratch, native copies and fixed query state; the
16 MiB raw cap is not a 16 MiB resident-memory claim.

## Queries and provider semantics

Inputs/outputs preserve binary32 bits and PathFinder's world-to-Detour permutation
{y,z,x}. Ground-only nearest uses the source 5-unit box, with 10-unit fallback for
fresh route lookup. Results do not implement owner path reuse, owner masks, liquid or
swim/fly state, smoothing, slope admission, near-point choice or complete PathFinder.
Each nearest query has a maximum 16-cell search rectangle; safe finite grid casts and
coordinates are checked independently of loaded tile count.

A route uses 512 native nodes, at most 4096 single-expansion sliced calls, and 256
polygon/corner outputs. ANY_ANGLE and portal-crossing options stay zero. Partial,
out-of-nodes and buffer-too-small statuses are errors even when SUCCESS is also set.
No-path or missing tile coverage yields no complete result. Early failures finalize
and discard sliced state before reuse. Deadlines are checked before/after native calls
and before every expansion, plus a final check before result publication. A nearest,
finalization or funnel call cannot be preempted midway; validated counts bound its work.
Hard wall-clock preemption would require process isolation.

The app loader consumes the existing immutable InventoriedProducts and binds map,
content, source epoch, full inventory revision, source pin and query policy. Each reply
echoes request endpoints, ticket and snapshot binding. These are consistency bindings,
not proof of an authentic extractor or live immutable source lease.

SourceNavigationProvider composes the current raw support/body/participant corridor and
existing SourceVmapProvider first. Navigation is only an additional veto: projected
endpoints, detoured funnel routes, unresolved body size, deadline expiration or incomplete
paths reject. A successful route cannot provide missing support, clearance, slope,
participants, source epoch, NativeCapability or production session entry. Replacing the
service still uses the existing mailbox cancellation. Source-product loading adds no
production consumer and does not touch the held offline compiler.

## Evidence and reproducibility

[generate.py](tools/generate.py) verifies upstream SHA256s, compiles the original
unmodified source with the authored fixture, and produces real .mmap/.mmtile bytes plus
273 direct native oracle rows (165 nearest hits, 128 full routes, 37 partial routes). The fixture includes adjacent polygons/tiles, a sloped
neighbor tile, stacked floors and a disconnected island. The test compares exact nearest
points/refs and full path/funnel outputs against direct findPath; partial source results
must remain unavailable. Odd/even vertex-count products test both link alignment cases. Source-written 300- and
600-polygon corridors exercise actual buffer truncation and node exhaustion.

Run from the repository root with the canonical Rust toolchain:

    cargo test -p benilla-native-nav -p benilla-native-geometry-tests
    cargo clippy -p benilla-native-nav -p benilla-native-geometry-tests --all-targets -- -D warnings
    python3 crates/benilla-native-nav/tools/generate.py --source /path/to/pinned/mangos-classic --check
    bash crates/benilla-native-nav/tools/check-sanitizers.sh
    ASAN_OPTIONS=detect_leaks=0 bash crates/benilla-native-nav/native/Detour/Tests/run-allocation-failure.sh

The native-boundary sanitizer script instruments the C++ implementation and bridge,
not Rust. It runs truncation/reference/topology/deadline/budget tests and deterministic
single-byte mutations through validation and actual queries. Allocation tests cover
both mesh-init allocation failures, all 10 fresh query-init allocations, six resize
allocations, retry/destruction and zero outstanding allocation counters. Object-lifetime
traits and all tile layout offsets have separate checks. LeakSanitizer is disabled in
the ptrace execution environment; ASan and UBSan are independent checks. Full Bevy
linking, real lawful local Data, live navigation and other target execution are not
established by these headless tests.
