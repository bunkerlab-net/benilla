# Selected cold same-polygon terminal oracle

This independent observer pins CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f` and reuses the guarded original
[point-path source package](point_path_source_fragments.test-fixture), original
G3D vector headers, Detour translation units, and authored precision geometry.
It never links the production navigation bridge.

The selected `cmangos-buildpolypath-zero-distance-v1` policy initializes only
BuildPolyPath's two caller-local distances to positive `0.0f`. The original
uninitialized cold branch is never invoked, and these results do not claim
original-cold parity. All other compiled source remains unchanged, including
the original `dtMeshTile` declaration. Its existing raw-storage object-lifetime
caveat remains: matching normal/UBSan outputs are empirical evidence, not a
standards-defined lifetime proof. No tile-lifetime or default-query-pointer
repair is applied by this observer.

## Observed boundary

The original constructor's cache initializer span is compiled in a cache-only
projection with the original member initialization order. It produces point
limit 74, active length zero, and 74 zero-valued retained polygon slots. That
cache is supplied to the diagnostic harness. The complete owner constructor,
`m_defaultNavMeshQuery`, owner/configuration lifecycle and SetCurrentNavMesh
are not executed or repaired. The complete constructor is preserved only as
review context in [the source fragments](cold_same_poly_source_fragments.test-fixture).

A genuinely loaded original Detour query uses 1024 nodes, as guarded in the
original MoveMap allocation. Each row records the actual node-pool capacity.
The repaired caller prefix then executes the complete unchanged lookup
methods sequentially. Full retained storage is captured after the first
lookup and after the second lookup, including the appended tail reference.
The source distance predicate must yield false, and both references must be
valid and identical before entering the exact original same-polygon branch.

That complete branch calls the unchanged BuildShortcut, including its clear,
two XYZ points and NormalizePath call, and then assigns polygon slot zero,
active length one and PATHFIND_NORMAL in the original order. NormalizePath is
compiled in full. Every case explicitly supplies normalize_z=false and
ignore_normalization=false, with a nonnull diagnostic owner. Thus the original
disabled-normalization early return is exercised; every world/transport
callback aborts if reached. These inputs are not claims about source defaults.
Optimization and force are explicitly false, and prior type is BLANK.

Four generated cases cover an on-polygon endpoint, a nonzero vertical endpoint,
identical endpoints and a start selected by the larger nearest search. The
terminal points and actual end are recorded as exact binary32 XYZ values,
including the unprojected destination height. All retained cache slots are
recorded, including inactive zeros and the appended tail after the shortcut.

The harness retains the independent warm point-path functions for shared
source/symbol validation, but the cold entry point invokes neither those warm
cases nor BuildPointPath/smoothing. There is no complete BuildPolyPath, broad
world pathfinding, owner event integration, admitted Route or gameplay claim.
Other cold branches and normalization settings remain outside this product.

## Schema and reproducibility

[The cases](fixtures/cold-same-poly/cases.test-fixture) use one tab-separated
header. Points are world XYZ hexadecimal binary32 words; point sequences are
semicolon-separated. References and counts are decimal. `FIELDS` in the
generator is the exact schema. Constructor cache, lookup cache, terminal cache,
query capacity, policy label and both normalization inputs are explicit.

Existing point-path source/input and cold-distance authored-input manifests
are checked without modification. The compiler profile remains GCC 14.2.0,
x86_64-linux-gnu, C++20, 64-bit references, nearest rounding, no FTZ/DAZ,
fast-math, FMA or contraction. Receipts preserve the compiled harness, symbols,
unchanged observed tile header, compiler command, manifest hashes and results.
No install assets, database, downloader or network access is used.

From the repository root with the existing local pinned source checkout:

```sh
SOURCE=/path/to/mangos-classic-8ec338a1704e7dcb1c0213eb7ed58f9231ade40f
TOOLS=crates/benilla-native-nav/tools
python3 "$TOOLS/cold_same_poly_test_generate.py" "$SOURCE"
python3 "$TOOLS/cold_same_poly_generate.py" "$SOURCE" --check --receipt-dir /tmp/cold-same-normal
python3 "$TOOLS/cold_same_poly_generate.py" "$SOURCE" --check --sanitize --receipt-dir /tmp/cold-same-ubsan
```

The unit guards do not compile. The two generator checks independently compile
and execute the selected repaired-source observation and compare every output
byte. Omit `--check` only to regenerate from the hash-verified sources.
