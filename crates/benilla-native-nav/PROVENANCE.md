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
- Ground area 11/flag 1 and source steep area 10/flag 2; no off-mesh links or water/magma polygons,
  extra layers or unknown flags. The generator's usesLiquids boolean alone is not an
  actual liquid polygon; both boolean values are permitted. Query filters remain ground-only
- Clockwise strictly convex 3–6-vertex polygons; finite nondegenerate detail triangles;
  exact reciprocal internal neighbors and reversed shared edges; genuine, complete
  detail boundary edges with reversed internal edges and no duplicate directed edges
- Serialized link heads/cache bytes are ignored because source addTile rebuilds every
  reachable link and head before use; exact source link capacity, valid contiguous detail
  ranges, complete BV subtree structure/leaf coverage/enclosing quantized bounds, and
  depth <=32. The source writer's final all-zero BV padding node is preserved; native
  queries consequently retain its polygon-zero callback behavior
- External portals only on exact cardinal tile faces or the pinned positive quantized
  plane min + 2000 * 0.2666666 (with the matching source quantization factor), with nonzero projected spans.
  Loaded cross-tile matches use the original ordered binary32 slab comparison,
  including its 0.01 plane/shrink constants and directional target-tile climb. Each
  directed portal may match at most one target edge; split or ambiguous matches reject,
  avoiding the source's four-match truncation and dropped links. All intermediate
  arithmetic and compression ratios must remain finite; geometry is never snapped
- A preflight upper bound of 1,048,576 native construction edge examinations, including
  each portal times every neighboring target edge, plus source-side scans. Cross-tile
  validation separately caps comparisons at 1,048,576. Oversized closures reject

This is intentionally a restricted source-compatible subset; it may reject lawful
source-generated data with narrow/degenerate triangles, ambiguous portals or larger
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
with an explicitly copied `SourceOwnerFilter`. Supported source-owned dry filters are
include=1/exclude=0 for habitat 1 and include=13/exclude=0 for habitat 3, with the native
default unit area costs. The mesh still accepts only ground flags/area; filter 13
does not admit wet polygons. Other masks reject;
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

## Warm smooth-calculation terminal dependency

`Mesh::loaded_source_warm_point_path` (policy 1) consumes an immutable, opaque
`LoadedSourceWarmCache` proposal on its exact original `Mesh` instance. It does not consume
public raw point lists or ref snapshots. Mesh lineage is checked before any smoothing work;
a new mesh loaded from identical bytes is not the same authority. The warm input is reusable
and unchanged on success, rejection, deadline expiry or budget exhaustion. The returned
`LoadedSourceWarmPointPath` owns fallibly copied warm diagnostics and a separate terminal
cache proposal. Neither this object nor its raw `path_type` is a Route fact, owner capability,
movement admission, persisted owner path or proof of complete `PathFinder` dispatch.

The explicit `SourcePointPathConfig` carries optimize-path, normalize-Z,
ignore-normalization, force-destination and the previous source path-type bits. Unknown
path-type bits reject. `normalize_z && !ignore_normalization` rejects before work: owner
height correction, liquids, transport coordinate transforms and world callbacks are absent.
The API represents only non-playerbots, non-dungeon, non-transport smooth calculations with
the exact source-owned dry filter (1/0 or 13/0) and source limit 74. Straight-line/raycast and straight-funnel
calculations are not represented by this API. In particular, source Chase first tries a
separate straight-line/raycast calculation when its distance, elevation, line-of-sight and
dry-state conditions allow (`TargetedMovementGenerator.cpp` 501-540). This terminal smooth
calculation is not that dispatch decision. Source Home creates a fresh PathFinder; the
force-destination arithmetic below is a raw supported calculation dependency, not proof of a
warm Home producer. The app separately restricts owner contexts and binds their configuration.

SamePolygon follows source `BuildPolyPath` 506-522: `BuildShortcut` yields the two original
world endpoints, then slot zero and active polygon length one are reinstalled and type is
NORMAL in the admitted endpoint-distance-at-most-seven domain. It does not invoke the
smoother, optimizer or force-destination branch, even if the incoming type has INCOMPLETE.
For Cut and Replacement, source 786-789 first classifies NORMAL only when the retained last
polygon equals the selected end ref and the prior type lacks INCOMPLETE; otherwise it
classifies INCOMPLETE. The selected active prefix and original positions then feed the same
private raw smoothing engine used by the fresh-corridor dependency. A private input/result
factorization keeps those wrappers separate: no nearest query, direct re-search, fabricated
`Nearest` metadata or public raw-ref constructor is used. The smoother's repaired local
corridor is never substituted for retained `m_pathPolyRefs` storage.

`native/source_point_path.cpp` is a marked GPL-3.0-or-later adaptation retaining the source
CMaNGOS GPL-2.0-or-later notice. It implements the supported smooth branch of
`PathFinder::BuildPointPath` (795-986), `BuildShortcut` (1005-1021), `inRange` and `dist3DSqr`
(1446-1455), from the same pinned PathFinder files and SHA256s listed above. Its binary32
expressions retain original operation order and strict comparisons, under the existing
no-fast-math/no-FMA build flags. The source G3D `Vector3` component subtraction,
`squaredMagnitude` and zero-initialized default constructor are in
`dep/g3dlite/G3D/Vector3.h` (SHA256
`75b8fa263ade1337c4a06d5e6918aa0a7d28bcb107ed405487e62235838125e2`); no G3D runtime is added to the production build.

The helper converts Detour YZX values to owner XYZ before optimization. OptimizePath uses
LINE_FAULT 0.5f and SKIP_POINT_LIMIT 6, marking a removed point as zero immediately. Thus the
next iteration reads the **mutated** previous point, rather than the last retained point or
an unchanged original neighbor. Original all-zero input points (including signed zeros)
when optimization executes with more than two points are explicitly unsupported. Source's
zero sentinel can skip those points while `m_pathPoints.resize` retains prior vector slots
on Cut; the warm cache does not retain that previous point vector. This is a defined
unsupported state, not an undefined-behavior claim, and it is rejected rather than repaired
with invented zero history. When optimization is off or count is at most two, zero points
remain representable.

Optimization precedes source failure classification. A raw failure or optimized count below
two invokes BuildShortcut, clears only the active polygon length and sets NOPATH. Raw failure
at the source point limit is not rescued by an optimizer reducing the count. Only after that
check, optimized count exactly equal to limit invokes BuildShortcut and sets SHORT. All
backing polygon slots, inactive refs and searchable tail survive terminal clear. On the
remaining branch, actual-end is assigned from the final optimized point before normalization.
Force-destination retains source's strict horizontal/elevation one-unit tests and strict
`dist3DSqr(actual,end) < 0.3f * dist3DSqr(start,end)` decision. The near branch replaces only
the final point/actual-end and preserves active cache; the other branch clears active cache
through BuildShortcut. Both set NORMAL|NOT_USING_PATH. NoSteerTarget source success with at
least two points can retain NORMAL even when actual end differs from requested destination;
this raw source result is not upgraded to a complete route.

The new result exposes original warm selection, full terminal backing storage/active count,
XYZ point bits, pre-normalization actual-end, terminal type, raw smoother points/status/stop
and operational diagnostics. SamePolygon reports no smoother invocation. Smoothing remains
bounded by at most 256 native primitives, checked individually against the same absolute
caller deadline. Postprocessing charges at most 221 work units: 74 conversions, 72 optimizer
decisions, 74 compaction slots and one terminal decision (SamePolygon costs one). The native
helper performs those bounded loops in one non-preemptible call, with deadline checks before
and after, and uses only fixed stack arrays. Rust copies use fallible reservations; all
publication paths recheck the deadline. Outputs must remain finite and within the existing
100000-unit coordinate bound. This is finite-work control, not a hard scheduler wall-clock
guarantee. The native-boundary sanitizer runner includes the new arithmetic helper.

## Bounded warm straight-calculation dependency

`Mesh::loaded_source_warm_straight_path` (policy 1) accepts an opaque `WarmCache`
on its exact originating loaded mesh, explicit XYZ endpoints, full collision-width
bits, `SourcePointPathConfig`, bounded work budgets and one absolute deadline.
The caller supplies the nonnegative finite full-width dependency; this API does not
compute owner width or prove that an owner may move. An identical-byte replacement
mesh cannot consume the handle. Input storage is immutable; every successful result
owns a separate terminal proposal and failures publish no candidate. Public snapshots,
serialized data and reference lists cannot mint this loaded result.

The implementation represents the dry, non-playerbots, fixed-limit-74 straight branch
of the pinned source. Sequential original warm selection is reused, with all retained
backing slots and append ordering preserved. This particular boundary additionally
requires nonzero active refs, and retains the existing valid-ref, at-most-seven endpoint
distance and undefined-cold-distance exclusions. SamePolygon executes the original
shortcut/type/cache order and does not sample or raycast. Cut performs the original
active-prefix cut and samples directly, without introducing a ray test. Replacement
clears only active length before raycast; only the visited prefix overwrites storage.

`native/source_straight_path.cpp` is an explicitly marked GPL-3.0-or-later adaptation
retaining original CMaNGOS and Detour notices. It implements the pinned PathFinder.cpp
673-850 and 942-986 straight/hit/terminal expressions and G3D Vector3 arithmetic. Its
ray traversal adapts original `dtNavMeshQuery::raycast` with options zero, separated
only at polygon-iteration boundaries for budgeting. The exact default filter expression
is copied through public include/exclude getters, as in the existing direct boundary;
virtual filters are excluded. No production G3D runtime or owner callback is added.

A ray continues traversing after its 74-ref output buffer fills. Its raw
SUCCESS|BUFFER_TOO_SMALL status, active prefix, hit fraction and normal are retained;
truncation does not turn the ray into a failed calculation or terminate traversal early.
A FLT_MAX no-hit does not consult width. On a finite hit the source subtracts the full
width directly from the fraction, rejects strictly below 0.1f, and interpolates the hit
using the adjusted value. Original warm lookup may append a retained tail during hit
selection. getPolyHeight status is observed but ignored when its input height is already
initialized; failure retains those exact bits. The source comparison remains squared
3D distance strictly greater than the double literal 0.01. A hit branch finally assigns
INCOMPLETE after BuildPointPath, even when its internal force handling assigned another
type. Requested end, clipped end and actual end are separately observable.

Straight sampling retains source path length greater than 0.1f, four-unit steps,
integer truncation and the less-than-0.3f remainder merge. Every next sample advances
from the previous height-mutated vector. Original warm lookup and height queries happen
on that same candidate, so sampling can grow the retained tail. The optimizer flag is
bound in the configuration but is ignored on this branch. Active NormalizeZ remains
unsupported; disabled/ignored normalization performs no owner or world query.

The native plan validates finite arithmetic and length at most 4096 before converting
float step count to an unsigned integer. Its max-points argument is explicitly bounded
by 1026; capacity is checked against the actual post-merge generated count. These are
operational limits, separate from the source terminal test: only count exactly equal
to 74 invokes the SHORT shortcut. Counts above 74 are not silently truncated or declared
SHORT. BuildShortcut clears only active length, preserving inactive and tail storage.
Prior INCOMPLETE, source force-destination decisions and final type ordering remain
observable, without upgrading the result to a complete Route.

Ray polygons, lookup primitives, sampled points and native primitives have separately
bounded budgets with deadline checks around native calls, allocation and publication.
Each ray state is privately RAII-owned and freed on success or any exit; candidate point
and diagnostic allocations are fallible. Cancellation leaves the prior available for
reuse. The maximum ray traversal is 4096 polygons, straight native primitives 4096,
retained cache slots 4096 and generated point storage 1026. The limits do not promise
preemption inside a native primitive or hard scheduler wall-clock deadlines.

The returned terminal cache can be explicitly passed into the existing warm smooth
calculation dependency with its prior path type. This preserves the same-object source
cache/type dependence needed for a later smooth calculation. It does not itself perform
Chase's distance, elevation, line-of-sight, owner-state or fallback dispatch. No cold
producer, owner normalization, swimming/flying, transport, dungeon limit, gameplay
movement admission, persisted cache authority or complete PathFinder is established.

[The independent straight source observer](tools/straight_path_README.md) documents the
exact unchanged-source boundary, helper-only controls and original object-lifetime
caveat. Its four independent lanes use original versus default-constructor-only-repaired
dtMeshTile and normal versus UBSan/float-cast-overflow compilation, all with actual
original G3D and Detour and without the production bridge. The corpus has 35 straight
cases, 5 same-object smooth continuations, 1568 primitive traces and 10 non-invoked
rejected domains. Every fixture byte is independently regenerated; original unused
height-status code is preserved under an exact single-diagnostic warning guard.
Source hash drift, schema/state/trace drift and actual CRLF checkout round trips are
covered by [generator guard tests](tools/straight_path_test_generate.py).

The production standalone sanitizer runner includes the new ray/scalar dependency and
bounded allocation/cancellation cases, jointly instrumented with ASan, UBSan and
float-cast-overflow. The independent original-source lanes above use UBSan plus
float-cast-overflow without ASan. LeakSanitizer cannot execute under the sandbox ptrace
environment, so it is disabled there; explicit
release/allocation checks are narrower evidence, not a claim of a LeakSanitizer pass.
These standalone/source checks do not establish Rust consumer tests, full workspace
gates, other build targets or live game behavior.

## Loaded nontransport endpoint tile presence

`Mesh::source_tile_presence_pair` calls the actual loaded mesh's `calcTileLoc` and
`getTileAt(tx,ty,0)`, with original world `{x,y,z}` mapped to `{y,z,x}`. The predicate
is the nontransport branch of pinned `PathFinder.cpp:1122` `HaveTile`: exactly `-1`
on either coordinate rejects; other negative coordinates are looked up. No nearest
or cached polygon stands in for a loaded endpoint tile. The start observation is
first; a false start leaves the destination unobserved. Its opaque read-only result
retains original endpoint bits and the never-reused loaded `Mesh` identity.

The separately exposed bit-based `source_map_coord_pair` implements pinned
`GridDefines.h:177–190` and calculate's destination-first order: finite horizontal
coordinates have magnitude at most `17066.166015625` (bits `0x46855455`); height only
has to be finite. Source-invalid coordinates are `Format`. Source-valid heights
outside the native validator's `+/-100000` bound are `Unsupported`, never mislabeled
source-invalid. Deadlines are checked before and between observations and before
return. The native call refuses non-nearest rounding and x86 FTZ/DAZ (or aarch64
FPCR flush/alternate-handling modes) without changing the caller's environment.
`FLT_EVAL_METHOD == 0` is required at compile time; excess-precision profiles are
rejected, including an explicitly tested Linux x87 `-mfpmath=387` build.
At most two read-only hash lookups inspect at most 64 loaded tiles; query filters,
retained cache and native query state are unchanged. `WarmCache::check_mesh` permits
consumers to verify the exact loaded dependency before any native observation.

These are bounded loaded-mesh prerequisites, not complete source calculate or owner
proof. Source nav/query selection and availability, transport/ignore-pathfinding
state, global/disabled-map policy, dungeon classification, shortcut/path-type
publication and gameplay admission remain outside this primitive. The cold
uninitialized-distance path is unchanged. Authored Rust tests establish actual
loaded tile versus nearby/cached polygon behavior, binary32 boundaries and
nextafter values, exact `-1` versus loaded `-2`, layer-one loader refusal, coordinate
validation, expired deadlines, retained cache and fresh-instance rejection.

The [independent source observer](tools/tile_presence_README.md) compiles complete
unchanged `HaveTile` and scalar/two-/three-axis `IsValidMapCoord` methods against
original pinned Detour, never the production bridge. Its 91 rows include actual
native coordinates, layer-zero presence, source predicate results, unobserved
endpoints, and a successful real nearest query beside an absent endpoint tile.
A clearly authored host with a null owner executes only the nontransport branch;
complete calculate is retained as review context and is not compiled. Exact source
and authored-input hashes, original notices/licenses, symbol checks and fail-closed
mutation tests guard reproduction. Both the unchanged-original empirical lane
(with its known tile raw-storage lifetime caveat) and a separate default-constructor-
only repaired lane preserve all layout offsets and compare identical observations,
under normal and UBSan/float-cast-overflow Linux x86-64 GCC 14.2 profiles. The Rust
consumer compares all source coordinate observations and 60 admissible loaded
mesh rows; 27 invalid-coordinate rows fail before querying, while empty/layer-one
meshes and two out-of-native-domain heights remain explicitly rejected. This is
platform-specific source evidence, not untested target or full-gameplay parity.

## Selected cold-distance compatibility policy

`Mesh::loaded_source_cold_distance_compatibility` is a separate, explicitly
versioned loaded calculation dependency under
`cmangos-buildpolypath-zero-distance-v1`. It initializes only the two
`BuildPolyPath` endpoint-distance locals to positive binary32 zero. This is a
selected repair of an undefined original cold read, not original cold parity.
The existing warm APIs and their cold refusal remain unchanged.

The opaque result retains actual mesh identity and constructor-equivalent
74-zero-slot/active-zero storage. It exposes observations, never a warm seed,
owner receipt, configuration authority, point path, or gameplay Route. The
bounded loader's 512-node lookup query is not source manager construction;
replacement retains the separate 1024-node direct-corridor dependency.
`m_defaultNavMeshQuery` and all unrelated source defects are unchanged.

The [compatibility review](../../docs/CMANGOS-COLD-DISTANCE-COMPATIBILITY.md)
records the exact source pin/lines, two-line initialization diff, policy binding,
original-versus-repaired oracle labels, bounds, exclusions, and reproduction
commands. The independent repaired observer uses real loaded Detour and checks
only the declared caller prefix and existing selected storage fragments.

## Retained source map-instance query allocation

`SourceMapQuery` is a distinct loader for the original 1024-node map-instance
query. `Mesh::load` and its existing bounded/warm APIs still allocate 512 nodes.
The two C ABI constructors dispatch to explicit capacities; Rust records the
chosen allocation profile and checks the native attached query's actual node-pool
capacity before publishing the owner. A policy hash cannot choose or relabel the
allocation, and there is no bounded-Mesh conversion or raw query replacement API.
All serialized tile validation, memory/alignment ownership and loaded inventory
limits are shared unchanged with the bounded loader.

Pinned `Map.cpp:203–210` calls `loadMapInstance` while creating the map, before
loading active entities. `MoveMap.cpp:356–380` creates a query keyed by actual
`(mapId, instanceId)` and initializes it with 1024 nodes; `GetNavMeshQuery` at
499–511 only looks up this existing slot. The explicit `SourceMapQuerySlot` is
this source key, distinct from the private never-reused local Mesh identity.
The low-level reader can represent other explicit instance slots for isolation
checks; the application constructor domain separately restricts actual selected
configuration to outdoor map type zero, source slot zero, enabled at construction,
nontransport owners and immutable inventory. Loading a query does not establish
these application facts by itself.

`RetainedSourceQuery` keeps the same actual allocation alive with private shared
ownership. It begins with no selected current query. The query-only
`select_for_calculate` transition checks destination and start coordinates before
selecting the retained default, matching `PathFinder.cpp:120–160`; invalid
coordinates preserve the prior selection. A source-valid height outside the
narrower native budget still selects first, then bounded HaveTile may refuse.
Map/instance changes are unsupported. Selected HaveTile observations retain exact
native allocation and endpoint identity. There is no unload, replacement,
serialization, byte/hash restoration, warm-seed conversion, or mutable path-cache
operation on this interface.

This is a genuine query allocation/retention dependency, **not by itself an owner
constructor receipt**. Chase lazily creates its `i_path` in
`TargetedMovementGenerator.cpp:498–499`, after forced spline relocation and before
its straight-line gate; an initially nearby Chase may not construct until a later
dispatch. `PathFinder.cpp:35–54` retains the default query only when enabled and
leaves the current query null. An application must pair its preloaded query with
that actual owner/generator dispatch event, complete selected configuration and
constructor filter state before claiming the constructor. A warm seed, a Route
request without this event, or serialized after-cache cannot establish it.
Disabled-at-construction then enabled on the same map reads the source's
uninitialized default-query field and is excluded, not repaired here.

The authored long600 regression exercises the actual stored native query: the
legacy pool exhausts at 512 while the distinct source pool completes all 600
expansions (the output remains explicitly truncated). Other tests pin actual
capacity readback, unchanged bounded APIs, same-byte fresh-allocation rejection,
source-slot separation, retained allocation lifetime, destination-first invalid
coordinates, unchanged selection on invalid/map/instance inputs, exact loaded
HaveTile identity and pre-native malformed-input refusal. These are compiled
native/Rust boundary tests, not a new independent original-constructor oracle or
complete PathFinder/Route/gameplay evidence. No cold-distance or other source UB
repair is part of this query-allocation change.

## Selected source-query same-polygon lineage

`SelectedSourceQuery::loaded_cold_same_polygon` composes only the reviewed
`cmangos-buildpolypath-zero-distance-v1` lookup with the same-polygon terminal
at `PathFinder.cpp:508–519,988–1020`. It exclusively borrows the genuine retained
source1024 query for each ordered native nearest/closest-point call. It never
uses a legacy512 lookup, creates a separate direct query, or invokes raycast,
sampled straight BuildPointPath or smoothing. Different polygon references,
missing polygons and distances greater than seven publish no candidate.

`loaded_cold_same_polygon_with_filter` additionally takes the exact copied
`SourceOwnerFilter`; the legacy entry point supplies `1/0`. The explicit path
retains `1/0` or source-owned dry `13/0` through the same loaded query and cache.
The existing ground-polygon validator is unchanged. A habitat-3 filter does not
admit wet polygons or recreate an owner initialization observation.

`SourceSamePolygonConfig` requires explicit `normalize_z=false` and
`ignore_normalization=false`; neither a server default nor a fabricated callback
is used. The source NormalizePath condition returns immediately in this domain.
The shared private same-polygon terminal preserves exact XYZ start/actual-end
bits and NORMAL=1. Existing warm straight/smooth callers use the same helper
without changing their original behavior. This terminal precedes both dispatch
branches; reusing it does not imply that either BuildPointPath branch ran.

The opaque result retains the actual query allocation, the named cold policy,
original constructor-equivalent 74-zero-slot/active-zero storage, full native
lookup observations, final active-one polygon cache including every inactive and
tail slot, explicit normalization selection, and two terminal points. Read-only
access cannot export a WarmCache, original-warm terminal or serialized restore.
Query checks compare the allocation and original calculate endpoints; identical
mesh bytes or source slot alone do not match. The result keeps its allocation
alive after loader/lease handles are dropped.

At most five lookup primitives (two start nearest attempts, one end tail scan and
two end nearest attempts) and one terminal primitive are allowed. These operational
budgets are separate from the source point limit; each bounded primitive checks
the deadline before and after its work. Failed or
expired calls discard their private candidate, and retries start from fresh logical
constructor storage on the same selected allocation. Native lookup scratch is not
a retained path-cache mutation. There is no accepted-cache commit or public warm
seed. `LoadedSourceSamePolygon` is the common initial/retained lineage type; old
cold-only names are type aliases, which preserve all the same policy observations.

`SelectedSourceQuery::loaded_retained_same_polygon` takes that actual prior opaque
result by shared reference. It checks the same query allocation and exact configuration,
then performs ordered original warm lookups on a private copy of the complete current
cache. The nonempty, valid active prefix initializes each source distance before its
read; no cold initializer or new undefined-behavior repair runs. New endpoints are
allowed. A same-byte query reload, changed configuration, malformed/unsupported input,
missing/different polygons, distance greater than seven, exhausted work or deadline
publishes no candidate. Multi-polygon selection may inspect the existing prefix, but
never invokes replacement, raycast or smoothing before rejecting it.

Each retained result preserves the original zero constructor storage and compatibility
ID/version/source pin, the exact latest before-storage/active length, and current full
after-storage. `compatibility().is_retained()` distinguishes that observation from the
initial repaired calculation. Both APIs return the same policy-bearing type, allowing
repeated replans without a chain of proof wrappers or a serialized restore. Memory is
bounded independently of replan count: original 74 slots plus latest before and after
vectors of at most 4096 slots each. At most 8050 retained lookup primitives and one
terminal primitive are allowed; the common default of five can be raised explicitly
for longer tails. Every success preserves the whole previous proposal, too. Retrying
against an earlier proposal starts from that proposal's own exact cache, not from an
implicitly committed latest result. Native query scratch can change, but accepted
owner/cache state remains the future application's transaction responsibility.

The [independent cold same-polygon observer](tools/cold_same_poly_README.md)
executes the selected two-local repair, unchanged lookup bodies, exact same-poly
branch and complete BuildShortcut/NormalizePath methods on original source1024
Detour. Four authored cases compare every terminal point bit and retained slot,
including a nonzero vertical endpoint and nearest10 start. This observer leaves
the original tile raw-storage lifetime caveat explicit and applies no new UB
repair. Rust tests additionally reject configuration/policy mismatches, different
polygons, equal-byte foreign allocations, endpoint mismatch, work/deadline failures
and exclusive-borrow conflicts, and establish retry and allocation lifetime.
Compile-fail tests forbid public original-warm relabel and restoration. The
[independent retained observer](tools/retained_same_poly_README.md) follows one actual
query and cache from repaired cold origin through unmodified warm caller prefixes.
Retained native tests compare exact source cache/point observations, append/reuse of
tails, repeated continuation, foreign allocations, configuration and budget/deadline
rejection, and successful retry after a partially completed lookup.

Constructor event/filter/configuration provenance, runtime dispatch, HaveTile and
fresh filter-update composition, actual segment geometry, authority/cache commit,
Route evidence and gameplay admission remain separate application prerequisites.
Source NORMAL does not prove dry support, endpoint support or body clearance.
This primitive grants no application/runtime admission. Disabled-constructor
`m_defaultNavMeshQuery` and all other excluded undefined branches remain untouched.

## Wander random-point endpoint and same-polygon lineage

`source_wander_same_polygon.rs` implements the distinct bounded
ComputePathToRandomPoint dependency from pinned PathFinder.cpp:1351–1440.
Its [separately approved endpoint-local initialization](../../docs/CMANGOS-WANDER-ENDPOINT-COMPATIBILITY.md)
is `cmangos-computepathtorandompoint-zero-distance-v1`. It does not invoke the
BuildPolyPath cold-distance repair. A private endpoint lookup mutates the cleared
retained cache before the real loaded getPolyHeight and original warm
BuildPolyPath lookups. This order makes the newly appended tail available to the
latter source distance locals. The original same-polygon terminal is shared only
after genuine same-polygon selection; no completed Chase result is relabeled.

The opaque pre-draw prefix clears active length while preserving full storage.
Its distinct terminal payload also represents the defined strict near-zero
NOPATH return, with no endpoint/height/tail calls and the previous end preserved.
Loaded endpoint polygon misses and nonexact-success height results have distinct
NoPath payloads that preserve endpoint/ref mutations and require the source dry
suffix. Every defined NoPath payload remains eligible as genuine prior state for
a later successful leg.
Selection occurs separately on its exact query allocation; later continuation
cannot select again. Initial and retained endpoint queries preserve source-owned
filter 1/0 or 13/0. Operational or unsupported failures leave the previous immutable terminal
unchanged and publish no candidate. Defined NoPath results publish a distinct
source terminal for the owner to commit. Owner constructor/filter timing, source water/above-ground tail and
concrete dry segment geometry remain application prerequisites. Native outputs
alone do not authorize a Route, runtime cache commit or movement.

## Source-generated detail representation

Original MapBuilder serializes after addTile, so link caches need not be zero.
Captured original ridge/WMO outputs exercise this path and an all-0xff cache test
checks unchanged query results after reconstruction. Structural sections remain checked.

Detail Y uses the source raster span rounding and the extra MeshDetail cell offset:
its upper bound is (floor((maxY-minY)*q)+1)*(1/q) + (minY+1/q).
Unset-height samples remain unsupported. On axis-aligned polygon edges, only a
single adjacent binary32 normal-coordinate representation is accepted, and the
along-edge coordinate must remain inside the closed segment. Stored coordinates
are never snapped or changed.

The source BV builder sees full detail vertices, while serialization replaces its
first polygon-vertex entries with merged polygon coordinates. Exact BV containment
is required for every serialized extra detail vertex. For replaced base vertices,
a narrow fallback admits the captured +cell Y and adjacent-float quantization
interval. Recast can merge vertices up to two Y cells apart; this is not a general
reconstruction proof for those cases, which remain outside the supported envelope.
Both original ridge captures establish the current subset; malformed bounds still
reject rather than being reinterpreted as clear or walkable space.

## Generated liquid evidence preserves the dry admission boundary

The `liquid_full_types_wmo` original-source capture combines the existing authored
full-type liquid terrain fixture with the captured WMO. Its expected result records
original generation, polygon area counts and ground-only query misses. The genuine
MPQ integration compares the emitted terrain product, complete helper request
identity (normalizing only the oracle's larger allocation allowance), and complete
map/tile bytes before separately exercising ordinary generation and reader refusal.
The raw comparison helper exists only under `cfg(test)` and never creates an
admitted `GeneratedNavigation` value.

For this exact original tile the first refusal is `Format`: its base vertices are
at Y=13.26666355 (minimum 1 plus 46 source cells), just above the input header's
maximum 13.25. The current base-vertex envelope rejects this quantized surface
before the area whitelist. The integration preserves that precise refusal; it
neither expands height bounds nor treats it as corrupted source data.

The production reader continues to admit only exact ground `(flags=1, area=11)`
and steep `(flags=2, area=10)` polygon pairs. Water `(4,9)` and magma/slime `(8,8)`
remain unsupported. Merely extending that pair list would be insufficient:
`SourceOwnerFilter` currently permits source habitat mask `13/0` only within an
already dry loaded mesh, and that mask contains the liquid flags. Wet traversal
therefore requires a separate capability/filter and ownership design. Neither the
fixture nor helper generation grants swimming, magma traversal or world admission.

## Two emitted tile seam evidence

The authored `two-tile-seam` fixture adds one interior hole to the neighboring ridge
ADT, preserving the shared edge. The pinned extractor and generator emit both tiles;
the corresponding neighbor without the hole retains its original 625-polygon skipped
result. Genuine MPQ capture, generation requests and both products are compared with
the original bytes. Six original forward/reverse crossings check nearest points and
references, complete corridors and corner bits/flags/references using both tile slots.
The seam uses the source slab rule rather than exact endpoint equality. An independent
original-function corpus covers 25 bounded finite cases; three zero-span/overflow
cases explicitly remain outside the supported arithmetic envelope. This evidence
concerns generated geometry queries, not normal game-session movement admission.

## Cold-origin retained straight composition

`SelectedSourceQuery::loaded_retained_straight_from_same_polygon` borrows an actual
`LoadedSourceSamePolygon`; `loaded_retained_straight` borrows the actual preceding
`LoadedSourceRetainedStraight`. Neither entry point accepts serialized polygon
references or exports an original-warm cache. The new opaque proposal keeps the
source1024 allocation, original zero-distance compatibility version and constructor
storage, complete immediate before-cache, and the unchanged loaded straight
algorithm's terminal result. Immediate predecessor identity tokens do not retain
prior result objects. Public observations are not owner, dispatch or Route evidence.

Both entry points use original defined warm lookup semantics, never another cold
initialization. Lookups and height queries use the selected allocation's query;
ray traversal borrows that same allocation's immutable mesh without creating a
replacement query. Replacement runs the existing source raycast and sampled straight
BuildPointPath, not direct findPath or smoothing. SamePolygon and Cut retain their
source bypasses. Full inactive storage and appended tail references remain intact.

The previous path type is taken from the actual opaque predecessor. The caller
explicitly supplies optimization and full collision-width bits; both are fixed for
subsequent straight proposals. Force destination is false, and normalization must
match the supported false/false cold origin. These scalar bindings establish no
creature model or owner-width provenance. The source result may be NORMAL, NOPATH,
INCOMPLETE or another supported raw terminal type; no automatic Chase fallback or
promotion to a complete route occurs. Existing same-polygon entry points continue
to reject non-same selections.

The unchanged lookup, ray-polygon, sampled-point and native-primitive budgets are
separate finite work limits. Failure or deadline expiry publishes no proposal and
leaves prior storage/type unchanged; native query scratch is not rolled back.
Composition tests cover genuine loaded origin/successor ownership and exact budget
boundaries. They are distinct from an independent original-source cold-to-straight
sequence oracle, and neither establishes application polyline clearance, transaction
acceptance, persisted owner reopening or production gameplay admission.
