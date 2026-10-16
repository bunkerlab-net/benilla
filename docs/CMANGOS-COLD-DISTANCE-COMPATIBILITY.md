# Selected cold PathFinder distance compatibility

This is a deliberate, narrowly selected compatibility repair, **not a claim of
historical CMaNGOS cold-path parity**. Its stable policy name is
`cmangos-buildpolypath-zero-distance-v1`, version `1`. It is implemented separately
from the original warm-cache policy, and can be reviewed or reverted independently.

## Pinned evidence and exact decision

The source is CMaNGOS Classic revision
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, non-PLAYERBOTS configuration.

- `src/game/MotionGenerators/PathFinder.cpp:36–55`: the owner constructor creates
  `m_pointPathLimit` polygon slots and sets `m_polyLength` to zero
- `src/game/MotionGenerators/PathFinder.h:37–45`: the non-PLAYERBOTS maximum is 74
- `PathFinder.cpp:306–334`: `getPathPolyByPosition` returns immediately at lines
  308–309 for an empty active path, without writing the distance output
- `PathFinder.cpp:337–383`: `getPolyByLocation` consults active storage, then an
  eligible tail, then the near and far nearest-poly searches, in that order
- `PathFinder.cpp:410–415`: `BuildPolyPath` declares both endpoint distances without
  initializing them and calls `getPolyByLocation` first for start, then for end
- `PathFinder.cpp:362,377`: a successful nearest-poly result tests `!*distance`
  before the assignment at lines 364 or 379

For constructor-equivalent cold storage (74 zero slots, active length zero, no
searchable tail), neither scan writes the first endpoint's distance. A successful
nearest query therefore reads an indeterminate floating value in the append
predicate. The second local is independently uninitialized; a first-endpoint
miss can leave it on the same undefined branch. There is no stable original
cold output to reproduce. Do not execute the original cold branch and interpret
an accidental output as a reference result.

The complete selected semantic amendment is this two-line diff:

```diff
-    float distToStartPoly, distToEndPoly;
+    float distToStartPoly = 0.0f, distToEndPoly = 0.0f;
```

The exact value is positive binary32 zero (`0x00000000`) for **both** locals,
not an epsilon, infinity, a maximum distance, or an arbitrary nearest result.
Zero gives the existing source `!*distance` condition its defined true value
when neither scan has supplied a comparison distance. A nonempty active or tail
scan still overwrites the local, including the source's tail-overwrites-active
behavior. Both helpers, scan thresholds, query order, append condition, start
append visibility to the end lookup, fixed point limit, and replacement corridor
calculation remain unchanged.

The Rust port selects this initialization only through the named compatibility
entry point. The existing original-policy warm entry point still returns
`ColdDistanceUndefined` for an empty active path without a tail. This repair does
not change every caller of `getPolyByLocation` or assign a default inside that
source helper.

## API, identity, and admission limits

`Mesh::loaded_source_cold_distance_compatibility` requires the exact supported
compatibility version. Unknown versions are refused. Its opaque
`LoadedSourceColdDistanceCompatibility` result retains:

- The actual immutable loaded mesh instance identity, checked by `check_mesh`
- The selected policy name, version, and source revision
- The actual initial 74-slot zero-filled storage, active length zero, and fixed
  ground-only filter (`include=1`, `exclude=0`)
- The actual ordered lookup observations, distance words, polygon-storage
  result, direct status, and primitive trace

This is a loaded calculation dependency with constructor-equivalent **storage
shape**, not proof that any game object's constructor ran. It does not expose a
conversion to a warm-cache seed or permit restoration from serialized reference
numbers. Equal bytes loaded into a second mesh do not satisfy its identity check.
Operational work/deadline budgets remain separate from the source point limit.
A failed calculation publishes no candidate and a later independent call can retry.

The bounded loader's existing query has **512 nodes**, whereas the pinned source
manager allocates **1024** (`MoveMap.cpp:369`). Nearest/closest-point lookup is
compared with a real 1024-node source query; this does not claim source manager
allocation or ownership. Replacement uses the existing separately bounded
1024-node source direct-corridor calculation. Source query construction and its
owner lifetime remain a separate prerequisite for full PathFinder composition.

There is no new owner receipt, selected-world configuration, `SetCurrentNavMesh`,
`HaveTile`, `createFilter`/`updateFilter`, `BuildPointPath`, normalization,
`path_type`, Chase dispatch, movement, or Route admission here. In particular:

- Same-polygon results project only the source polygon-storage assignment;
  they do not execute or claim `BuildShortcut`/normalization
- Missing polygons, distances greater than seven, partial paths, truncation,
  and out-of-nodes remain refused; they are not turned into invented routes. The
  observer records both endpoint calls in missing-polygon cases; the production
  boundary may stop at the first failed lookup and publishes no partial result
- The original cold refusal elsewhere is not lifted by relabeling a warm seed
- Dungeon-dependent point-limit expansion, transport, water/flying policy, and
  arbitrary creature/owner provenance are outside this API
- `m_defaultNavMeshQuery` is **not initialized or otherwise repaired**. The
  disabled-construction to enabled-same-map case is still excluded

## Independent oracle labels and reproducible checks

Files are in `crates/benilla-native-nav/tools/`:

- `cold_distance_source_fragments.test-fixture` records the exact original and
  repaired caller prefix, source pin, and stable policy name
- `warm_cache_source_hashes.sha256` binds all original source/header/license
  inputs; `warm_cache_input_hashes.sha256` and
  `cold_distance_input_hashes.sha256` bind every reused authored mesh input
- `warm_cache_source_fragments.test-fixture` retains the original complete
  lookup methods, constructor, and full `BuildPolyPath` review context
- `cold_distance_generate.py` hash-checks the pinned source, guards the exact
  single initializer replacement, and compiles a separate C++ source observer
- `cold_distance_oracle.cpp` invokes the repaired caller prefix, unchanged
  complete lookup bodies and bounded original polygon-storage fragments, with
  real separately compiled Detour and loaded authored tiles
- `fixtures/cold-distance/` contains only `compatibility-zero-v1` observations
- `cold_distance_test_generate.py` rejects initializer drift, ambiguous labels,
  malformed rows, or generated-output drift

Labels have different meanings and must not be merged:

- Original warm oracle: unchanged-source defined warm observations
- Original cold: `undefined-not-invoked-no-parity-claim`; the existing
  `cold_without_tail_excluded` fixture remains excluded
- Repaired cold oracle: `compatibility-zero-v1`, selected compatibility evidence
  against the exact declared amendment

The repaired oracle uses the existing, separately documented
`dtMeshTile() = default` lifetime profile already used by production; this commit
adds no other UB repair. Original source hashes are checked **before** either
known amendment. Receipt metadata names both the distance compatibility profile
and this existing tile-lifetime profile. The compiler profile is GCC 14.2.0,
x86_64 Linux, C++20, binary32, no fast math or FMA contraction. Other supported
ABIs are not certified by this Linux differential run.

Run from the repository root, supplying the pinned checkout as `SOURCE`:

```sh
python3 crates/benilla-native-nav/tools/cold_distance_generate.py "$SOURCE" --check --receipt-dir "$RECEIPTS"
python3 crates/benilla-native-nav/tools/cold_distance_generate.py "$SOURCE" --check --sanitize
python3 crates/benilla-native-nav/tools/cold_distance_test_generate.py "$SOURCE"
cargo test -p benilla-native-nav
cargo clippy -p benilla-native-nav --all-targets -- -D warnings
```

Coverage includes exact +0 words, near/far nearest lookup, first append then
second tail lookup, and (in the C++ oracle only) independent second-local
initialization after a first miss. Rust verifies refusal for that missing-polygon
case rather than claiming execution of its second lookup. Further coverage includes
full retained storage, actual direct order across tiles, exact 74 capacity and
75 truncation, partial/out-of-nodes refusal, version mismatch, instance identity,
invalid coordinates, deadline/work-budget failure, and retry. Every existing
warm fixture and test remains in place; additional differential tests compare
original and repaired initialization on all bounded warm cases, including their
existing rejections. Compile-fail coverage prevents erasing the repair policy by
exporting its internal cache as an original warm dependency.

These checks establish only the stated loaded dependency and selected repair.
They do not establish playable creature routing or a first gameplay Route.
