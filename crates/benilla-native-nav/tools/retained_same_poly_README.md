# Retained same-polygon continuation oracle

This independent observer pins CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. It reuses the guarded original
[point-path source package](point_path_source_fragments.test-fixture), original
G3D headers and Detour translation units, and authored precision geometry. It
never links the production navigation bridge.

## Initial compatibility versus original retained behavior

The sequence starts with the exact cache-only constructor projection and the
selected `cmangos-buildpolypath-zero-distance-v1` cold entry from the
[cold same-polygon observer](cold_same_poly_README.md). Only that entry initializes
BuildPolyPath's two caller-local distances to positive `0.0f`. The original
undefined cold entry is never invoked; there is no original-cold parity claim.
The existing cold source package and repair are reused without changes.

Every subsequent entry calls a separate original retained prefix, with the
original `float distToStartPoly, distToEndPoly;` declaration unchanged. The
harness requires a nonempty active cache of valid references before entry. The
complete original active scan therefore writes each distance before it is
read, including when that scan finds no reference within its squared-distance
threshold. No warm distance initialization or other semantic repair is added.
The only instrumentation inside the prefix copies the retained vector after
the first endpoint lookup; it does not change source lookup inputs or answers.

Each observed sequence uses one live PathFinder object and one loaded original
Detour query throughout. It neither reconstructs the object within a sequence nor
seeds later caches from expected answers. Each row's complete before-storage
must equal the previous row's complete terminal storage. The query's measured
node-pool capacity remains 1024, matching the guarded MoveMap allocation.

Both valid endpoint references must match and both distances must satisfy the
original near predicate before the exact original same-polygon branch runs.
That branch calls unchanged BuildShortcut and NormalizePath, then assigns slot
zero, active length one, and PATHFIND_NORMAL in source order. The complete
unchanged NormalizePath method executes its disabled-normalization early
return: normalize_z=false and ignore_normalization=false are explicit inputs,
with a nonnull diagnostic owner. Any reached owner/world callback aborts.
These settings are not claims about original configuration defaults.
Straight-line mode is explicitly set to true once and asserted before and
after every entry. This is a scalar input to the selected same-polygon branch,
which does not consult it; it is not execution of the complete calculate() path.

The original `dtMeshTile` declaration remains unchanged. Its raw-storage
object-lifetime caveat remains: matching normal and UBSan results are empirical
evidence, not a standards-defined lifetime proof. No tile-lifetime,
default-query-pointer, or unrelated BuildPolyPath-local repair is applied.

## Cases

The original eleven-step chain observes:

1. Repaired cold A lookup appends A, followed by the original same-polygon terminal
2. Original retained active lookup, preserving a nonzero destination height
3. Active squared distance 4 exceeds threshold 3, while retained A satisfies tail threshold 7
4. Both scans fail at squared distance 9; equal nearest distance does not append
5. Nearest lookup appends B; the second endpoint reuses that newly appended B tail
6. A later retained entry reuses B's tail after B became the active polygon
7. Returning to prior A reuses the older A tail and preserves B's tail
8. Identical endpoints remain two exact XYZ points
9. Active squared distance exactly 3 is accepted
10. Tail squared distance exactly 7 is accepted
11. The larger nearest search returns A at distance 6 without an equal-distance append

All entries finish successfully in the same-polygon terminal. Full storage is
recorded before lookup, after the first lookup, after both lookups, and after
the terminal, including every inactive zero and appended reference. Endpoint
distances, actual end and both output points use exact binary32 words. The
terminal preserves unprojected endpoint heights and retains prior tail slots.
The schema distinguishes the initial `compatibility-zero-v1` row from later
`original-retained-defined` rows.

## Fixed-cache differential and active-state counterexample

[The comparison fixture](fixtures/retained-same-poly/comparisons.test-fixture)
records separate simultaneously live source1024 queries for each complete and
reduced sequence, for dry include/exclude filters 1/0 and 13/0. Both queries
start with the original constructor cache projection, independently perform the
selected repaired cold entry, and use only original defined retained entries
afterward. No cache is copied from one query to the other or seeded from an
expected result. The unchanged source packages and cold compatibility policy
are shared with the original eleven-step observer.

The fixed-cache sequence executes 4,097 calculations: cold A, 4,095 repetitions
of active, tail, near-nearest-no-append and far-nearest-no-append inputs, and a
final active input preserving the requested nonzero height. Every intermediate
calculation is checked before and after against the complete initial accepted
backing vector and active length; fixed point limit and filter are checked too.
Its final result equals a fresh query executing only the original cold and final
inputs. Comparison includes exact binary32 endpoints, actual end, path points,
path type, flags, complete backing, active length, limit, filter and unchanged
path scratch state. First-lookup and second-lookup observations also match.
The fixture records the cold entry, four representative intermediate entries,
and the final entry; the other calculations execute without emitting rows.

Both live queries then execute identical B append, A no-append return and shared
edge inputs. Their complete terminal states and exact lookup observations match
after each operation. The append leaves the saveable fixed-cache domain; this is
continuation evidence, not a claim that the longer mutated lineage remains
fixed-cache saveable.

The negative pair executes world XYZ A `(5,5,0) -> (5,6,0)`, then B
`(5,15,0) -> (5,16,0)`, then A again without appending, then the shared edge
`(5,10,0) -> (6,10,0)`. Its partner drops only the third operation. Original
active-first lookup selects A for the complete sequence and B for the reduced
one, at exactly zero endpoint distance. Both have the same ordered `[A,B]` tail
and identical inactive slots; only active slot zero differs. Equal path points
and equal tails therefore cannot justify retaining only appenders and the final
operation. This observer does not execute certificate capture or rejection.

`COMPARISON_FIELDS` adds `include_filter`, `exclude_filter` and `point_limit`
after `query_nodes` in the original schema. `step` is the actual local execution
ordinal: complete fixed-history head 4,096 versus reduced head 1. The parser
requires every comparison lane, complete/reduced exact final and continuation
equality, and the negative pair's differing active reference with equal tails.
It does not treat the sparse output rows as the original execution count.

## Scope and reproducibility

[The source fragment package](retained_same_poly_source_fragments.test-fixture)
records the original retained prefix and same-polygon branch. The existing
source packages preserve the complete constructor as review-only context,
unchanged lookup and terminal methods, G3D/Detour source and license notices.
The shared compile helper only builds the supplied harness and copies original
dependencies; it has no repair switch. Original methods retained from the
point-path harness remain subject to symbol verification, but the sequence
never runs BuildPointPath, smoothing, or other BuildPolyPath branches.

Complete owner construction, default query/configuration lifecycle, tile
presence checks, world callbacks, admitted Route production and gameplay are
outside this observer. There are no install assets, databases, downloaders or
network calls.

[The original fixture](fixtures/retained-same-poly/cases.test-fixture) is tab-separated.
`FIELDS` in the generator is its exact 23-column schema. `sequence` and `step`
identify the single contiguous live-object chain. Points are world XYZ
hexadecimal binary32 words, with semicolon-separated point sequences. Counts
and references are decimal. The schema fails closed on a reset cache, altered
policy, invalid terminal, mismatching points or missing continuation.

The existing point-path source/input and cold-distance input manifests are
checked unchanged. Compilation uses the audited GCC 14.2.0 x86_64-linux-gnu
C++20 profile, 64-bit references, nearest rounding, no FTZ/DAZ, no fast-math,
no FMA and no floating contraction. Receipts retain the compiled harness,
compiler command, symbol list, unchanged observed tile header, source/input
hashes and output hashes. The profile explicitly records straight_line=true.

From the repository root with the local pinned source checkout:

```sh
SOURCE=/path/to/mangos-classic-8ec338a1704e7dcb1c0213eb7ed58f9231ade40f
TOOLS=crates/benilla-native-nav/tools
python3 "$TOOLS/cold_same_poly_test_generate.py" "$SOURCE"
python3 "$TOOLS/retained_same_poly_test_generate.py" "$SOURCE"
python3 "$TOOLS/retained_same_poly_generate.py" "$SOURCE" --check --receipt-dir /tmp/retained-same-normal
python3 "$TOOLS/retained_same_poly_generate.py" "$SOURCE" --check --sanitize --receipt-dir /tmp/retained-same-ubsan
```

The unit guards do not compile. Each generator check compiles and executes the
independent source observer and compares every output byte. Omit `--check`
only to regenerate from hash-verified sources. The cold observer retains its
own unchanged fixtures and source package after compiler-helper extraction.
