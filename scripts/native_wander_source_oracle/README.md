# Pinned Wander source oracle

This is an independent C++ source oracle for the selected dry, same-polygon
Wander component and its `AbstractRandomMovementGenerator` loop. It does not
link the Rust runtime, native bridge, or any production exporter. The main
geometry is one authored numeric quad built by the pinned original
`dtCreateNavMeshData`. A separately labeled observer lane loads an unchanged
existing numeric precision fixture. Both use the original Detour implementation
and 1,024 query nodes. They are **component geometry**, not loaded world
membership or a Classic installation.

## Source and sole semantic compatibility patch

CMaNGOS Classic revision:
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`.
The ClassicDB project pin is `ec4f596146be6467ea93c57397858e329e2db852`;
this oracle reads no database rows. `source_hashes.sha256` checks every copied
Detour/G3D compilation dependency and the selected CMaNGOS source/context.
`source_intervals.json` records exact normalized-LF byte and line intervals
and SHA-256 hashes. `source_excerpts.test-fixture` retains the original bodies,
review-only context, the selected compiled prefix, and the patched endpoint
body with upstream license notice.

The explicitly authorized policy is separately named
`cmangos-computepathtorandompoint-zero-distance-v1`:

- Only `ComputePathToRandomPoint` changes `float distanceToPoly;` to
  `float distanceToPoly = 0.0f;`
- `endpoint-zero.patch` records exactly that one scalar initialization
- The original complete endpoint body is retained but never executed
- This selects deterministic compatibility behavior; it is not undefined C++
  bit parity, and sanitizers do not establish that claim

This policy is distinct from `cmangos-buildpolypath-zero-distance-v1`.
The latter policy is **not applied** here. The endpoint's real successful
nearest-poly lookup appends a ref beyond the 74-slot original cache shape.
The unchanged `BuildPolyPath` prefix then reads that retained tail through
`getPathPolyByPosition`, which writes both of its distance outputs before use.
The successful vectors record the real query order and complete retained
storage. This causal precondition makes the selected prefix's distance reads
defined without changing its two locals.

The compiled `ComputePathToRandomPoint` body is complete. The unchanged
`BuildPolyPath` source prefix includes its same-polygon terminal; reaching any
later branch raises an explicit out-of-scope error. The complete original
`BuildPolyPath` and PathFinder constructor are review-only context. The test's
PathFinder adapter supplies the original cache dimensions, not the full
server constructor or its default-query-pointer lifecycle.

## Executed scope

The exact source bodies cover the Wander creature constructor, Initialize,
Finalize, Interrupt, Reset, Update, `_setLocation`, AddToRandomPauseTime,
ShortTimeTracker, endpoint sampling, nearest/retained polygon lookup,
`HaveTile`, `BuildShortcut`, and disabled `NormalizePath` dispatch. Original
G3D arithmetic and actual Detour lookup/height methods are compiled.

The input boundaries are explicit: owner state and lifecycle leaves, normalized
and integer RNG transcript values, an owner filter snapshot, navmesh pointer
selection, normalization configuration false, and spline Launch duration /
Finalized observations. They are not measured world behavior. The RNG
transcript asserts each call's type, bounds (normalized values strictly below
one), ordering, and complete consumption;
it does not implement or prove the upstream RNG engine. Spline interpolation,
packet output, real elapsed-time arrival, and full owner construction remain
outside scope.

On the audited Linux include/compiler profile, unqualified `cos(float)` and
`sin(float)` resolve to float overloads through the pinned G3D headers. A
compile-time type assertion checks this fact. Endpoint expressions retain
`(draw * 2) * float(M_PI)` ordering and the original float sin/cos arithmetic.
Five nontrivial angle vectors retain exact binary32 endpoint bits.

The executable asserts 33 cases / 53 snapshots:

- Initialization takes no random draw and launches no new spline; prior AI,
  client-controlled, and no-AI spline interruption branches are distinct
- Endpoint angle then radius precede lookup, optional random walk/run,
  Launch, move-count draw, and pause draw
- Actual same-polygon success uses endpoint lookup, height, then the two
  retained-tail lookups, returning the source two-point shortcut terminal
- Moving freezes the pause timer; finalized countdown tests one millisecond
  before and exactly at expiry; positive remaining move count decrements
  without a new pause draw
- Real filter rejection and an endpoint outside its polygon exercise real
  Detour misses and height failure; failures retry after 500 ms or 100 ms for
  player-controlled owners, without success-delay draws
- Blocked/dead paths, zero-duration Launch, random walk threshold 14/15,
  slowed-combat factor, reset, and pause adjustment are separate cases
- Adjacent binary32 radii straddle the `< 0.01f` squared-distance check
- No mesh, no query, ignored pathfinding, missing start/end tile, and the water
  height-failure fallback record original source shortcut behavior

Those fallback vectors are source-boundary negatives for the bounded dry
same-polygon consumer, not permission to admit such routes. No synthetic
lookup result is described as actual geometry. Query outcomes come from the
original Detour executable; missing pointers, flags and world leaves are
explicit external controls.

## Run and verify

From the repository root:

```
python3 scripts/native_wander_source_oracle/run_oracle.py \
  --core /path/to/pinned/cmangos-source \
  --output /path/to/scratch/wander-runs --check-bundled
python3 scripts/native_wander_source_oracle/test_oracle.py
```

`--write-bundled` regenerates the checked-in numeric vectors, source intervals,
source excerpts and one-scalar patch. Each run receives a new output directory,
retaining source inputs, compiler command, symbols, stdout/stderr, return codes,
input/output hashes and failure logs. The compiler is required, never skipped:
GCC 14.2.0, `x86_64-linux-gnu`, C++20, strict non-fused float expressions, and
undefined/float-cast-overflow sanitizers. The required real source and Detour
symbols are verified using `nm -C`.

The original Detour `dtMeshTile` raw-storage lifetime caveat remains unchanged;
results are empirical under that documented profile, not a lifetime proof.
The authored tile byte buffer is borrowed at the required link alignment and
removed before freeing its allocation. No Detour class, source body, or
compiler undefined-variable initialization option is silently repaired.

`fixtures/params.test-fixture` and `fixtures/quad.test-fixture` export the exact
existing original Detour parameter bytes and framed tile used by the executable.
The tile is `(0,0)`, origin `(0,0,0)`, width/height 64, with one flat ground quad
spanning Detour X/Z 0–40, area 11, flags 1. The runtime may compare against these
same bytes; no geometry was changed to accommodate runtime expectations.
`geometry_hashes.sha256` pins both files, and each oracle run regenerates them
from its original numeric inputs. `--check-bundled` checks their bytes too.

Additional inside-loaded-tile negative/retry vectors distinguish actual
nearest-poly misses (endpoint X60, beyond the 10-unit box from the quad edge)
from actual height failure (endpoint X42, nearest ref found but outside polygon).
They preserve the source's different cache effects: miss leaves 74 slots;
height failure leaves the appended 75th ref. Both consume two draws, execute the
dry suffix, wait 500 ms, and successfully retry from their retained state.

The Python tests check source excerpt hashes, the exact one-scalar patch,
retained-tail causality, actual Detour call sequences, and discriminating
loop/RNG boundary outputs. Full world admission and production data loading
remain closed and untested by this oracle.

## Unchanged precision-fixture observer lane

Three separately labeled rows also execute the complete patched source on the
existing, unchanged `crates/benilla-native-nav/tools/fixtures/direct-corridor/params.test-fixture`
and `warm-cache/precision.test-fixture`. Their original hashes are checked before
loading and recorded in `source_intervals.json`; no loader guard or geometry
was altered. Their `geometry` field is `unchanged-existing-precision-fixture`.
The original 30-case quad lane remains separately labeled `original-authored-quad64`.

The precision rows cover source same-polygon success, an endpoint in the loaded
tile but outside the selected polygon (height failure), and a loaded-tile
nearest-poly miss. The height-failure case uses world `(5,5,0)` to `(11,5,0)`.
A negative-coordinate endpoint such as `(5,-1,0)` would fail source `HaveTile`
and take its shortcut branch instead; invoking only the lookup dependency at
that endpoint does not prove a full-source height-failure route.
