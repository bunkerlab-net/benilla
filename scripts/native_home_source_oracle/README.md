# Pinned Home source oracle

This independent C++ source oracle executes the selected Home lifecycle and its
MotionMaster interaction with a retained default Wander generator. It does not
link Rust, a world runtime, a production exporter, or game data. Geometry is one
authored numeric quad built by the pinned original `dtCreateNavMeshData`, queried
by original Detour with 1,024 nodes. It proves component behavior, not loaded
world membership or playable Classic integration. Production admission remains
closed.

## Source and compatibility boundary

CMaNGOS Classic revision is
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. The project ClassicDB pin is
`ec4f596146be6467ea93c57397858e329e2db852`; no database rows are read or bundled.
`source_hashes.sha256` pins every copied Detour/G3D compilation dependency and
the selected CMaNGOS source/context. Its own digest is guarded in the runner.
`source_intervals.json` records exact normalized-LF byte/line intervals, SHA-256
hashes, and whether an excerpt is compiled. `source_excerpts.test-fixture`
retains the excerpts and the original upstream license notice.

The sole semantic compatibility patch is the separately named
`cmangos-buildpolypath-zero-distance-v1`:

- `BuildPolyPath` changes `float distToStartPoly, distToEndPoly;` to
  `float distToStartPoly = 0.0f, distToEndPoly = 0.0f;`
- `cold-distance-zero.patch` records exactly those two initializers
- The original prefix is retained but never executed
- The selected prefix contains the original same-polygon terminal and throws
  if execution reaches the excluded cross-polygon continuation

This is deterministic compatibility behavior, not original undefined cold
execution parity. The distinct Wander endpoint policy,
`cmangos-computepathtorandompoint-zero-distance-v1`, is **not applied** here;
this oracle does not execute random endpoint sampling.

Home calls `calculate(start, destination, true)` with the default straight flag
false. The exact source constructor and calculation bodies execute. An actual
successful nearest-poly lookup appends a ref after the initial 74-slot storage;
the endpoint lookup reuses that retained ref. The selected same-polygon terminal
returns `PATHFIND_NORMAL` with two points before `BuildPointPath` can perform its
later force-destination rewrite. The full original `BuildPolyPath` and
`BuildPointPath` bodies remain review-only context. Force=true does not justify
turning this successful terminal into `PATHFIND_NOT_USING_PATH`.

## Executed lifecycle

The unchanged extracted bodies include Home Initialize, Reset, target selection,
Update and Finalize; MotionMaster Clear/Expire dispatch, Direct/DelayedClean,
Direct/DelayedExpire, Mutate, MoveTargetedHome and UpdateMotion; the Wander
constructor, Interrupt and Finalize; AbstractRandom Initialize, Reset, Interrupt
and Finalize; Unit::InterruptMoving; the combat-start capture statements; and the
Launch prefix that corrects its first vertex. Source declarations, Position's
zero-XYZ sentinel and ShortTimeTracker are included unchanged as well.

The executable asserts 15 separately named source observations:

- First combat-start capture survives a later capture attempt; nonempty XYZ
  selects that origin, while all-zero XYZ selects respawn even with nonzero facing
- Home's run/walk and facing values follow the selected origin and `runHome`
- An Idle default leaves a stale query origin that Launch corrects; a retained
  Wander default instead interrupts the running spline before Home queries
- Clear removes Chase, Mutate interrupts retained Wander a second time, and
  Unit::InterruptMoving forces relocation before Home Initialize and distance
  evaluation
- Stale distance 149 relocating to 151 takes the source far branch; the inverse
  151-to-149 control reaches calculation. These source-boundary controls use
  excluded missing-tile/long-distance behavior, not an admitted normal route
- Moving Home does not finalize. Completed spline state triggers addon reload,
  Home events, inactive restoration and combat-start clear before default Reset
- Reset retains the Wander instance and move count, zeros its timer, replaces
  its PathFinder and does not sample RNG or launch another spline
- Interrupted Home clears evade without arrival callbacks or combat-start clear
- An already-active owner remains active on arrival; temporary faction restoration
  precedes addon reload
- Lost-control refusal precedes mutation; the separate not-move target guard
  permits Initialize's active/evade effects without constructing a path
- The far comparison remains literally `GetDistance(...) > 150.f * 150.f`.
  Exact equality and the adjacent higher binary32 endpoint distinguish `>`
  from `>=`. The no-transport `DIST_CALC_NONE` leaf returns squared distance;
  the complete pinned WorldObject overload is retained as review-only context
- Ignore-pathfinding and missing-tile shortcuts have type 17 without a 400 speed
  override; a real nearest-poly miss has `PATHFIND_NOPATH` and the source's 400
  override. These are explicit negatives for a normal same-polygon consumer

Snapshots retain full path storage, binary32 query/goal/launched-point bits,
distance/facing/velocity bits, owner state, constructor count, retained Wander
count/timer where present, and ordered leaf/query/callback traces. Constructor
counts include the initial retained Wander before the trace is cleared for the
second-interrupt cases. A zero captured path in a refusal is an observation
default, not a calculated route.

## Explicit shell limits

Owner flags, positions, squared-distance calculation, activity/world membership,
enabled pathfinding, a dry owner filter, callbacks, and spline Finalized,
ComputePosition and Launch duration are authored external leaves. An observation
of ComputePosition supplies a chosen pose; it is not full spline interpolation.
The source Launch prefix corrects the first vertex, but its duration and packet
output are not executed. Addon/Home callbacks record order, not their full world
implementations. The authored Chase records Finalize and does not simulate combat.

The exercised MotionMaster path is direct cleanup/expiration. Delayed variants
are compiled unchanged as dependencies but no reentrant callback/mutation proof
is claimed. Transport, controlled owners, summoner callbacks, normalization,
cross-polygon routing, full owner construction, world membership, and subsequent
random Wander legs remain outside this oracle. The separate Wander oracle covers
its own endpoint and update loop. No fallback observation broadens admission.

## Run and verify

From the repository root:

```
python3 scripts/native_home_source_oracle/run_oracle.py \
  --core /path/to/pinned/cmangos-source \
  --output /path/to/scratch/home-runs --check-bundled
python3 scripts/native_home_source_oracle/test_oracle.py \
  --core /path/to/pinned/cmangos-source
```

`--write-bundled` explicitly regenerates vectors, intervals, excerpts and the
two-initializer patch. It is mutually exclusive with `--check-bundled`. Source
bundle checks run before compilation; output equality is checked after execution.
The Python gate alone performs no compilation: 17 bundled contract tests plus
three checkout tests with `--core`. Without `--core`, only the 17 package tests
run. Negative guards cover each pinned whole file, rewritten/rehashed bundled
excerpts, callback/mutation ordering, the literal far comparison, the two-scalar
patch and the same-polygon return.

Each execution uses a fresh numbered output directory containing extracted
inputs, compiler commands, symbol checks, stdout/stderr, return codes, source
vectors and a file-hash manifest. GCC 14.2.0 on `x86_64-linux-gnu` is required,
never silently skipped. Compilation uses C++20, strict non-fused binary32
operations, no fast math, and undefined/float-cast-overflow sanitizers. Required
Home, MotionMaster, PathFinder and original Detour symbols are checked with
`nm -C`. Rounding is nearest, and flush-to-zero/denormals-are-zero are disabled.

Each run exports `params.test-fixture` and framed `quad.test-fixture`; their
digests are retained in `source_intervals.json` and checked by bundled metadata
equality. The tile is `(0,0)`, origin `(0,0,0)`, width/height 64, with one flat
Detour X/Z 0–40 quad, area 11 and flags 1. The tile buffer is borrowed at the
required link alignment and removed before its allocation is freed. Original
Detour's `dtMeshTile` raw-storage lifetime caveat remains unchanged: these are
empirical results under the documented compiler profile, not a lifetime proof.

The upstream source retains its GPL-2.0-or-later notice. Authored adapters and
scripts are GPL-3.0-or-later; the combined oracle is GPL-3.0-or-later. No game
assets or database rows are bundled.
