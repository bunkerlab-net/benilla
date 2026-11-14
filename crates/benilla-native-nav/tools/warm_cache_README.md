# Independent loaded warm-cache source oracle

This GPL-covered test observer is pinned to CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. It compiles both complete, unchanged
`PathFinder::getPathPolyByPosition` and `PathFinder::getPolyByLocation` bodies
against the actual original Detour translation units and genuinely loaded,
authored numeric meshes. It never links the production navigation bridge.

## Evidence boundary

The fixture is a polygon-cache dependency observation, **not full BuildPolyPath,
a Route, a complete chase calculation, world data, or gameplay admission**.
The source package includes complete BuildPolyPath and BuildShortcut for review;
neither complete method is compiled into this observer.

After sequential start/end lookup, selected-reference transitions compile these
unchanged source fragments from BuildPolyPath:

- The strict far-from-polygon predicate `distance > 7`, separately from the
  squared-distance lookup thresholds
- Active scanning for the first start reference and last end reference after it,
  including the original scan/cut block and `memmove`
- The original direct `findPath` call with output capacity 74, after original
  `clear()`; the map query uses the original MoveMap pool of 1024 nodes
- The two same-polygon storage assignments, after `clear()`

The last item is explicitly BuildShortcut's **polygon-storage projection**:
BuildShortcut starts by calling `clear()`, which clears active length and point
storage, leaving polygon storage intact. The observer does not execute its
point construction or NormalizePath. Replacement observes raw source status,
active count, and the complete retained vector immediately after findPath.
No BuildPointPath, owner terrain/capability decision, NormalizePath, smoothing,
shortcut geometry, or complete path-type interpretation is claimed.

All observed active references are valid loaded references. The original scan's
invalid-active diagnostic branch is retained verbatim but cannot be reached;
its diagnostic-only placeholders abort if called and are not scripted owner
or world callbacks. Primitive instrumentation is transparent forwarding:
closestPointOnPoly, findNearestPoly, and findPath return results from actual
original Detour. It supplies no scripted geometry or query answers.

## Warm-state and storage semantics

Primary runtime rows fix the point limit at 74 and retain at most 4096 polygon
slots. The latter is an operational observer/port cap, not an original source
vector bound. Rows record the full vector, not merely the active prefix.
Inactive slots between the active prefix and slot 74 are not searched; tail
search starts at slot 74 only when the original size predicate permits it.
Zeros in eligible inactive tail storage exercise actual Detour invalid-ref
returns and are skipped by the unchanged source helper. Active zeros are rejected.

The complete lookup methods show squared thresholds 3 and 7, a strict squared
`< 1` early exit, first-reference tie ordering, and square-root returned distance.
Exact adjacent binary32 squared words around 1, 3, and 7 are obtained from actual
closest-point calculations on the authored precision mesh. The final linear
far cutoff has separate adjacent distance observations around 7.

A missed tail scan overwrites the comparison distance from the active scan.
This can prevent an append or cause a duplicate append. A nearer findNearestPoly
result genuinely appends to warm retained storage. Start append is visible to the
subsequent end lookup, which can find that same reference in the tail. Both
nearest extents, their actual miss/success sequence, and a complete miss are
observed. Horizontal off-tile positions are used for nearest10 witnesses;
original BV vertical clamping can return a nearest5 result below a tile's floor.

The source `!*distance` append disjunct has **no finite warm-miss true witness**:
a failed nonempty active scan wrote sqrt(minimum) with squared minimum above 3,
and a failed eligible tail wrote a minimum above 7. Each is positive (including
sqrt(FLT_MAX) for an all-invalid scan). Zero distance would already have returned
a cached polygon. The fixtures exercise the strictly-smaller disjunct instead.

Cold active=0 with no eligible tail is explicitly excluded **before either
original lookup method runs**. It is never made deterministic by initializing
the original distance local. A helper-only active=0 row with an eligible tail
is safe because the nonempty tail writes distance before nearest can read it.
A separate helper-only lower-limit row exposes inactive zeros and a retained ref;
it is not evidence that an initial Chase producer changes its fixed limit.
The runtime rows and transition rows use 74 throughout.

`raw` rows preserve original partial, buffer-too-small, and out-of-nodes detail
statuses. Those statuses are not cancellation, a deadline, or a work-budget
result. `excluded` cases do not run the selected-reference transition, because
lookup is unsafe, missing, or outside the bounded far-distance slice.
Original source behavior beyond that boundary is not inferred.

## Inputs, products, and schema

`warm_cache_source_hashes.sha256` covers every original compiled Detour source
and header plus PathFinder, MoveMap, and both license files. Source input CRLF
is normalized to LF before hash verification; source content drift is rejected.
`warm_cache_input_hashes.sha256` binds the existing authored two-tile mesh and
existing direct-oracle four-tile/600-/1100-polygon products. No install assets,
world database, importer, download, or network is used.

`fixtures/warm-cache/precision.test-fixture` is generated by original
`dtCreateNavMeshData` from four small integer-coordinate quads: adjacent ground
quads, a floor one unit above one quad, and a disconnected ground island. It uses
`fixtures/direct-corridor/params.test-fixture`, tile coordinates (1,1), and load
grid [31,31]. All generated filenames have the non-admitted `.test-fixture`
suffix; the binary product cannot be confused with a runtime `.mmtile` file.

Text products have one tab-separated header beginning `# label`:

- `scans.test-fixture`: point, squared max-distance argument, ordered references,
  selected reference, returned distance, and actual per-reference squared distances
- `lookups.test-fixture`: point, scope/limit, before-active/full storage, selected
  reference/distance/origin/append, independent observations of active/tail and
  both nearest stages, and after-full storage. Unconsulted fields are zero
- `cases.test-fixture`: from/to, before-active/full storage, both sequential lookup
  results, selected transition phase, optional raw direct status and pool-node
  count, then after-active/full storage
- `traces.test-fixture`: case label, endpoint (0 start/1 end), zero-based ordinal,
  primitive, polygon, raw status, and distance bits for each actual lookup call

Coordinates are world XYZ binary32 words, comma-separated, eight hex digits per
component; the harness converts from Detour YZX. Distances/statuses are eight hex
digits; references/storage are decimal comma lists; `-` means absent direct
status. Scan trace distances are the square root of the running source minimum;
nearest-miss trace distance is zero. Traces omit cut/direct instrumentation.

## Compiler and object-lifetime lanes

The audited observation profile requires GCC 14.2.0, x86_64-linux-gnu, C++20,
64-bit polygon references, IEEE binary32, nearest rounding, and no FTZ/DAZ,
fast-math, contraction, or FMA. A missing or different compiler fails closed.
Symbols must include both original lookup methods and actual Detour; any
production bridge symbol fails the guard.

The unchanged original Detour `dtMeshTile` declaration is not default
constructible or aggregate in C++20, while original allocation uses raw storage.
Thus the original lane retains the known object-lifetime caveat: reproducible
output and clean UBSan are empirical evidence, not a proof of standards-defined
lifetime. The explicit `--repair-tile-lifetime` lane inserts only
`dtMeshTile() = default;`. It verifies identical layout, alignment, every member
offset, and noncopyability, and must reproduce all fixture bytes. This narrowly
repaired lane does not change lookup, nearest, path-search, or storage logic.
Neither UBSan nor the repair establishes complete world or gameplay correctness.

## Reproduce and verify

From the repository root, with an existing local pinned source checkout:

```sh
SOURCE=/path/to/mangos-classic-8ec338a1704e7dcb1c0213eb7ed58f9231ade40f
TOOLS=crates/benilla-native-nav/tools
python3 "$TOOLS/warm_cache_generate.py" "$SOURCE" --check --receipt-dir /tmp/warm-original-normal
python3 "$TOOLS/warm_cache_generate.py" "$SOURCE" --check --sanitize --receipt-dir /tmp/warm-original-ubsan
python3 "$TOOLS/warm_cache_generate.py" "$SOURCE" --check --repair-tile-lifetime --receipt-dir /tmp/warm-repaired-normal
python3 "$TOOLS/warm_cache_generate.py" "$SOURCE" --check --repair-tile-lifetime --sanitize --receipt-dir /tmp/warm-repaired-ubsan
python3 "$TOOLS/warm_cache_test_generate.py" "$SOURCE"
```

Omit `--check` only to regenerate the package and fixtures from verified source.
Receipts preserve generated harness, symbols, observed tile header, compiler
command/profile, source/input hashes, and output hashes. Guards exercise
malformed manifests, numeric/cache bounds, unsafe names, source drift, missing
compiler/symbols, bridge exclusion, output drift without rewrite, and an actual
`core.autocrlf=true` Git add/checkout-index round trip. No commit is made.
