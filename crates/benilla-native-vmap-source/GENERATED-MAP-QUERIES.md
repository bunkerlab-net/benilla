# Generated map queries

This test connects the Rust map and model assemblers to Benilla's existing
inventory and static collision query implementation. It checks authored WMO
content against the pinned CMaNGOS Classic implementation. It does not enable
world startup or establish that a game session is playable.

## Data flow

1. The original-source generator creates two ordinary tiled maps from authored
   `dir_bin` records and raw WMO triangles. One logical model name is plain;
   the other retains a terminal NUL. Both have valid transformed bounds.
2. The generator records complete original map, tile and model bytes, plus
   public `VMapManager2` query results. It requires a loaded tile and a positive
   geometric hit; an apparently successful load alone is insufficient.
3. A test-only adapter feeds the same authored records through the Rust
   first-wins spawn index, supplied WMO bounds, map BIH and map/tile writer.
   The raw model goes through the Rust model writer. Every output is compared
   with the original bytes before loading.
4. The generated products enter normal inventory validation with exact typed
   source model identities and digest-based filenames. The unchanged production
   loader and query implementation validate and query those products.

The authored tile coordinates are asymmetric: the extractor record uses
`(30, 31)`, and the original manager loads `(31, 30)`. This checks the original
filename transposition rather than hiding it with equal coordinates.

## Test adapter boundary

`assemble_authored_wmo_map` exists only with the `synthetic-map-tests` Cargo
feature. That feature is enabled by test-only dependencies in the app and
geometry harness. The workspace uses Cargo resolver 2. The normal/build
feature graph of the `benilla` player does not include this adapter or the
source discovery crate.

The adapter accepts only pre-bounded WMO records. It uses the production record
parser, count and byte limits, first-wins indexing, numeric checks, BIH builder,
map writer and cancellation checks. Its synthetic binding is not a source
receipt or an authority token. No source lease, filesystem publication, runtime
admission or bypass of consumer checks is provided.

Real-source discovery, retained receipt checks, selected-model conversion and
capture-to-bundle tests remain separate. These authored tests do not prove that
an arbitrary client archive can be captured successfully, that global maps are
supported, or that NPC simulation is connected to normal gameplay.

## Original-source expectations

The companion query fixture records binary32 request and result bits from the
unchanged pinned original implementation. World-to-internal coordinate
roundtrips are recorded explicitly. Height misses use the public manager's
sentinel. LOS and hit-position checks retain original endpoint and search
behavior. Expected nearest distances are not inferred by subtracting rounded
hit positions.

A terminal-NUL WMO name is an authored identity test. It is not a claim that the
original WMO extractor naturally emits that name. Both source naming variants
must keep distinct typed identities through validation and lookup.
