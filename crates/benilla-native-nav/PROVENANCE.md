# Pinned dry MMap query and source-smoothing boundary

This private crate loads actual serialized CMaNGOS MMap/Detour products and runs the
pinned native nearest-poly, sliced corridor and straight-path primitives, plus a separate
bounded original direct-corridor and pre-optimization source-smoothing dependency. It is not
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
swim/fly state, slope admission, near-point choice or complete PathFinder. The existing
`Mesh::route` remains unsmoothed; the separate API below does not change its semantics.
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

## Separate bounded loaded source-smoothing dependency

`Mesh::loaded_source_corridor` prepares a fresh corridor on the validated immutable mesh,
with an explicitly copied `SourceOwnerFilter`. The initial supported filter is exactly
include=1/exclude=0, with the native default unit area costs. Unsupported masks reject;
they are never silently replaced. The handle retains an exclusive mesh borrow, original
world endpoint bits, copied filter, source point limit, operational budgets and one absolute
caller deadline. Its fields are private, and no API accepts caller-supplied polygon references.
Read-only polygon outputs cannot be passed back as authority. `LoadedSourceCorridor::smooth`
consumes the handle; `Mesh::loaded_source_smooth` composes these two stages.

Preparation performs bounded direct nearest-poly 5/10-box queries only to obtain references.
Both the fresh direct findPath and smoother receive the **original unsnapped positions**.
This is not `getPolyByLocation`: the pinned owner helper's successful cold lookup reads an
uninitialized caller distance in its cache-append predicate. This boundary neither executes
nor repairs that helper and makes no cold-owner-lookup compatibility claim. Cached owner
corridor reuse, owner state, transports, playerbots and wet polygons are outside this stage.

Source smoothing policy 2 uses a separate private direct-search state owning the original
`dtNodePool(1024, 256)` and `dtNodeQueue(1024)`, matching MoveMap.cpp's 1024-node query
initialization and Detour's `dtNextPow2(maxNodes/4)` hash size. It copies the original
`findPath` outer expansion rather than using `initSlicedFindPath`/`updateSlicedFindPath`.
The original link order, `(polyRef, crossSide)` node keys, first-visit portal midpoint,
heap tie order, cost/heuristic expressions, `>=` rejection, closed-node reopening,
strict-`<` last-best update, goal-pop stop and accumulated out-of-nodes flag are retained.
The exact `getPathToNode` parent walk is preserved, with a defensive 1024-parent ceiling
and index check before native access. No parent reversal or sliced finalization occurs.

Each Rust step surrounds one original outer expansion with deadline and operational-budget
checks. The operational limit remains 4096 expansions: reopening nodes means 1024 allocated
nodes is not itself an expansion limit. Same-polygon source success requires no expansions.
A step completes its bounded neighbor list before returning: validated ground polygons have
at most six edges, at most one loaded link per edge, and node-pool/heap operations are bounded
by 1024 nodes. Final reconstruction visits at most 1024 parents; the public dependency
writes at most 74 refs.
Allocation, nearest lookup, one expansion and finalization cannot be preempted mid-call;
these are finite-work bounds, not a hard millisecond wall-clock guarantee.

The source non-playerbots point limit remains 2..74 and also limits the admitted corridor
length. Direct finalization uses that exact capacity: an exactly full path is SUCCESS;
actual truncation is SUCCESS|BUFFER_TOO_SMALL. The private native finalizer additionally
admits capacities 1..1024 solely for bounded source differential tests; each test supplies
a matching output buffer. This does not widen the public 2..74 point/corridor domain.
Partial and out-of-nodes detail bits are independent and retained, including node exhaustion
when the goal is reached. Any non-exact
SUCCESS final status rejects without a public corridor or smoothing result. Operational
budget/deadline failures likewise discard the private state without publication. This does
not change `Mesh::route`'s 512-node sliced/funnel path or its cleanup semantics.

`native/source_direct.cpp` is a marked extraction outside the vendored Detour tree. It
retains the original Mikko Mononen notice and adds the GPL-3.0-or-later adaptation notice.
Its direct loop, reconstruction, default filter expressions and narrow ground-only portal
helpers come from the pinned original
`dep/recastnavigation/Detour/Source/DetourNavMeshQuery.cpp`, SHA256
`fb3f040b35633ad01dfec2a18e1dd1433b10ede69780dddf65bd0a016af0183b`.
The original default filter methods are inline in that source translation unit; the extraction
uses the same expressions through public filter getters, with unchanged unit area costs.
The portal helper preserves first matching link, clamp and midpoint binary32 arithmetic;
unsupported off-mesh branches are excluded. If a validated adjacency invariant were violated,
a missing portal rejects before reading an unwritten midpoint. Node allocation/heap operations
use the existing pinned `DetourNode` implementation and its allocation-validity guards.
The separate state's partial construction, cancellation and successful completion all free
its owned pool and queue before the mesh can be reused; it retains no caller-owned pointers.
The 5/10-box nearest lookup remains a bounded dependency, not OwnerPathFinder or cold-cache parity.

`native/source_smoothing.cpp` adapts these original PathFinder methods, preserving their
arithmetic and defined quirks while splitting at native primitive boundaries:

- `fixupCorridor`, including the lowest visited duplicate match, no-intersection return,
  maxPath truncation and `req >= maxPath` unchanged-path behavior
- `getSteerTarget`'s three-point funnel, accepting SUCCESS|BUFFER_TOO_SMALL, strict inRange
  comparisons, elevation flattening and false/no-steer early return
- Ground-only `findSmoothPath`, including 4.0f steps, 0.3f slop, `sqrtf`, source binary32
  expression order, 16 visited polygons, corridor repair, sampled height +0.5f and copying
  the final boundary target without that offset
- `inRangeYZX`'s strict horizontal squared-radius and absolute-elevation tests

The exact source files at Classic commit `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f` are:

- `src/game/MotionGenerators/PathFinder.cpp` SHA256
  `0fc2684275621af341e104d7593c7f66e5baa6251e088548deb5c547625294ef`
- `src/game/MotionGenerators/PathFinder.h` SHA256
  `5c98dfd9a7d28a5faf3a86866baf056cd6df40c06f4816f64070a92d4a89773b`

The adapted file retains the original GPL-2.0-or-later notice; the combined adaptation is
GPL-3.0-or-later. Existing CMaNGOS notices and full GPL text linked above apply. No original
server source, game install data or database rows are required in a shipped build.

A private native state copies the corridor, endpoints and filter into fixed storage. Boundary
start, boundary end, steer, moveAlongSurface and getPolyHeight each consume one separately
bounded primitive call. The maximum operational budget is 256 primitives; it is independent
of the semantic source point limit. Deadline checks surround each primitive and iteration,
initialization, snapshot copy and publication. Cancellation publishes no partial dependency.
The separate direct-search state is discarded on errors; private direct and smoothing state
is freed on every exit before mesh reuse. Native primitives cannot be preempted midway; validated
mesh limits and fixed query pools bound their work rather than promise OS wall-clock control.

The source ignores moveAlongSurface/getPolyHeight failure details. This safe boundary records
last primitive statuses plus truncation/failure counters. moveAlongSurface SUCCESS and
SUCCESS|BUFFER_TOO_SMALL both copy an initialized result and bounded visited list, so both
are retained, including the source's 16-visited repair behavior. An actual move failure can
leave its output uninitialized and is rejected without fabricating a result. For validated
ground polygons, getPolyHeight's FAILURE|INVALID_PARAM returns before writing; the preceding
successful move already initialized all result components. That defined existing-height +0.5
fallback is preserved exactly, with a diagnostic failure count, not silently repaired or
excluded. Other unsupported statuses and nonfinite outputs reject. Off-mesh branches cannot
occur in admitted validated products and are not implemented.

`LoadedSourceSmooth` retains original and final polygon lists, world point bits, raw source
status, termination reason and primitive diagnostics. DT_SUCCESS with one point/no steer is
retained as such. A result whose count reaches the source point limit retains DT_FAILURE,
even if its final iteration reached the target. `TargetReached` denotes that actual branch;
it does not override raw failure. Boundary failures preserve the source's zero-point failure.
These outputs precede source OptimizePath (default true) and NormalizeZ (default false);
neither later stage is included. They are dependencies only, never owner PathFinder,
Route facts, slope/body support, gameplay admission or a production session proof.

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
not Rust. Before the Rust suite, a separate authored native executable injects each of
the direct state's four backing allocation failures and its outer nothrow-new failure.
It uses the production bridge and authored two-tile mesh, checks balanced partial cleanup,
cancellation after zero/one/two expansions, successful same-mesh reuse after each failure
or cancellation, and zero outstanding Detour allocations after destruction. Its allocator
overrides are process-local and never run in the parallel Rust tests.
The script also runs truncation/reference/topology/deadline/budget tests and deterministic
single-byte mutations through validation and actual queries. Allocation tests cover
both mesh-init allocation failures, all 10 fresh query-init allocations, six resize
allocations, retry/destruction and zero outstanding allocation counters. Object-lifetime
traits and all tile layout offsets have separate checks. LeakSanitizer is disabled in
the ptrace execution environment; ASan and UBSan are independent checks. Full Bevy
linking, real lawful local Data, live navigation and other target execution are not
established by these headless tests.

## Bounded loaded warm polygon-cache dependency

`LoadedSourceCorridor::warm_cache_seed` mints an opaque `WarmCache` from that handle's
private original, unsmoothed corridor, at the fixed non-dungeon source point limit 74.
It copies the original refs into 74 initialized backing slots. This is an explicit bounded
loaded dependency seed, **not** evidence that the original owner's initial cold calculation
produced it. No public constructor accepts references, a supplied identity/hash or serialized
cache state. The private authored-state constructor exists only in native unit tests and
checks every nonzero ref against the loaded immutable mesh. Cloning a handle retains its
lineage; it cannot authorize another mesh. Each successful `Mesh::load` receives a private
monotonic instance ID, with checked exhaustion, rather than relying on a reusable allocation
address. Handles may outlive the mesh but can only be queried against that exact instance.

`Mesh::loaded_source_warm_cache` uses source policy 1, an immutable prior handle and a
separate fallibly allocated working copy. Start lookup runs before end lookup on that same
copy, so a start append can be observed in the end tail scan. The new opaque after handle,
original query bits, selected refs/distances, polygon-phase branch and read-only trace form
an uncommitted proposal. The caller remains responsible for its own transaction and identity
bindings; the native mesh stores no authoritative current cache. Every failure discards the
candidate, leaving the prior unchanged. No replay, checkpoint or serialized projection can
reconstitute this loaded authority.

`native/source_warm_cache.cpp` preserves PathFinder.cpp lines 306–383's native distance
arithmetic against actual loaded Detour. Rust splits the ordered source scans at each
`closestPointOnPoly` call, preserving strict smaller-distance selection, first-equal winner,
early break at squared minimum `< 1.0f`, active acceptance `<= 3.0f` and tail acceptance
`<= 7.0f`. Returned distances retain the source `dtMathSqrtf` bits. Failed ref probes do not
change the minimum; an all-failed nonempty scan retains `sqrt(FLT_MAX)`. Tail eligibility is
exactly storage length greater than max(active length, point limit), and its first slot is
always 74. Tail scanning overwrites the active comparison distance. Ordered nearest queries
use the source 5-unit then 10-unit box; a successful result is selected whether or not it
appends. Appending retains the exact `!distance || newDistance < distance` predicate, including
duplicate refs. The zero-distance disjunct has no finite admitted warm-miss witness; the
strictly-closer append branch is exercised. There is no deduplication, compaction or eviction.

The source caller's uninitialized-distance cold domain is explicitly excluded. If active
length is zero and no eligible nonempty tail scan exists, the boundary returns
`ColdDistanceUndefined` before a nearest lookup can consume an unwritten distance. It does
not initialize the distance to zero, reduce the semantic point limit or claim a source cold
repair. This conservative rejection also excludes the no-nearest-hit cold subcase. Dungeon
limit growth, playerbots, straight-line raycast, owner wet/swim/fly state and transport state
are absent. Any selected endpoint distance greater than seven units returns
`FarFromPolygonUnsupported` before proposing a polygon transition: source water/flying
classification and end projection must not be silently skipped.

At the polygon phase, same-polygon handling changes slot zero and active length only. Cut
uses the first source start match and the last end match strictly after it, then overlapping
prefix movement, preserving all remaining slots and tail. Replacement clears only active
length and invokes the existing private original 1024-node direct engine with the **selected
cached refs** and original positions. It never calls `loaded_source_corridor` to redo nearest
selection. Only its returned prefix is overwritten; inactive storage and tail survive.
Raw non-exact direct statuses, including SUCCESS with PARTIAL_RESULT, BUFFER_TOO_SMALL or
OUT_OF_NODES, return `UnsupportedDirectStatus(raw)` without an after handle. They are not
silently stripped, upgraded or represented as a source-complete cache transition.

These are named polygon-phase outcomes only. No `BuildPointPath`, terminal `m_type`,
OptimizePath, NormalizeZ, owner capability, complete route or gameplay admission is produced.
The separate smoother's `final_polygons` are never committed as retained polygon storage.
Existing `Mesh::route`, source direct-corridor and source-smoothing outputs are unchanged.

Retained storage is operationally capped at 4096 slots; an append beyond that bound rejects
the whole proposal. Lookup primitives (including each examined cached ref and cut comparison,
nearest calls and direct create/finish) are capped at 16388; direct outer expansions have a
separate 4096 cap. Absolute deadlines surround each native primitive and source expansion,
allocation and final publication. Query rectangles retain the existing 16-cell bound.
No native warm-cache heap allocation is needed; Rust owns candidate/trace allocations with
fallible reservation. Private replacement state is RAII-owned before any post-call deadline
or allocation check, and owns/frees its 1024-node pools on every exit. Native calls cannot be
preempted mid-call; these are bounded-work checks, not hard scheduler wall-clock guarantees.
The existing sanitizer runner also compiles the warm arithmetic and bridge primitives.

[The warm source oracle](tools/warm_cache_generate.py) checks pinned source and authored
input hashes, packages the unchanged original lookup bodies and separately scoped source
storage-transition fragments, then runs them against actual source-written loaded Detour
meshes. Its [fixtures](tools/fixtures/warm-cache/cases.test-fixture) record original query
bits, full before/after storage, selected refs, distance bits, append decisions, phase and
raw direct status. Separate scan, lookup and primitive-trace fixtures establish first-equal
selection, early exit, adjacent binary32 squared 1/3/7 thresholds, active-before-tail priority,
tail-overwritten comparison, real 5-to-10 fallback and sequential start-append/end-tail-hit.
Neighboring unsquared distance-seven values separately establish the excluded far branch.
The raw partial/truncated/out-of-nodes source observations remain present, while the loaded
boundary tests explicitly require their rejection and unchanged prior storage.

Reproduce the original observation profile using:

    python3 crates/benilla-native-nav/tools/warm_cache_generate.py /path/to/pinned/mangos-classic --check

The generator's optional `--sanitize` profile and separately labeled
`--repair-tile-lifetime` profile retain distinct receipts. The exact original profile carries
the existing original `dtMeshTile` raw-storage lifetime caveat; a sanitizer pass does not
remove it. The repaired profile changes only that previously documented tile lifetime
construction, not warm lookup or polygon-transition arithmetic. Cold uninitialized-distance
calculations are explicitly excluded, never executed as an oracle.

`warm_cache_tests.rs` compares actual loaded query output and full retained storage with
these independent observations. Additional tests cover exact/one-short work bounds,
start-append followed by rejected end lookup, full-storage append refusal, original deadline,
Mesh drop and identical-byte replacement identity rejection, clone lineage, invalid fixture
refs, zero inactive slots, original-versus-smoothed corridor separation and legacy query
reuse after cancellation. The production interface has compile-fail checks against raw-ref
construction and the test-only authored-state constructor. These tests do not establish
initial owner-cache production, serialized rehydration, full PathFinder, final path_type or
complete Route authority.
