# Independent original navigation oracle

This directory builds CMaNGOS Classic commit
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f` directly. It does not import or execute
Benilla's navigation collector, generator helper or validator to make expected
results. All terrain, collision and placements are authored fixtures; no install
assets, held compiler patch or quarantined listfile oracle are used.

## Reproduce

From the repository root, with the approved pinned checkout next to it:

```
python3 tools/native-navigation/run_oracle.py ../cmangos-source --check
python3 tools/native-navigation/test_oracle.py
python3 tools/native-navigation/capture_scalar_nearest.py ../cmangos-source --check
python3 tools/native-navigation/retain_source.py ../cmangos-source --check
BENILLA_ORACLE_SANITIZE_ALIGNMENT=1 python3 tools/native-navigation/run_oracle.py \
  ../cmangos-source --work tools/native-navigation/.work/alignment --check
```

The audited capture profile is Linux x86-64, little endian, GCC 14.2.0. The build
requires a C++17/20 compiler, Python 3 and zlib development/link support. A clean
build uses three compiler workers. Generation uses one original worker and a
120-second subprocess timeout per case. These are authored-fixture timings,
not real-install performance claims or cross-platform byte-parity claims.

`--check` recompiles original source, reauthors the ridge ADTs, runs unchanged
original `ConvertADT` (also replaying the retained full-types liquid ADT and
requiring its exact original `.map` bytes), reassembles the existing captured WMO using unchanged
original `TileAssembler`, executes original `TerrainBuilder`/`MapBuilder`, loads
the actual emitted bytes with original Detour, and compares every deterministic
file byte. Existing terrain fixtures are read-only and verified against their
original writer manifest. `--skip-input-regeneration` skips only the optional
existing-fixture hash/reassembly audit; it never substitutes production output.

## Original source and harness boundary

`build_original.py` compiles complete unchanged original translation units:

- mmap: `MapBuilder`, `TerrainBuilder`, `IntermediateValues`, `VMapExtensions`
- VMap: `TileAssembler`, `WorldModel`, `BIH`, `ModelInstance`, `VMapManager2`, `MapTree`
- all original Recast and Detour source files
- the necessary original G3D math, memory, file and runtime translation units

There is one integer-alias-only `Platform/Define.h` shim. `NO_CORE_FUNCS`,
`MMAP_GENERATOR` and `DT_POLYREF64` match the extractor's standalone shape.
`-fno-access-control` allows the driver to inspect original config, bounds and
tile-union methods without editing their bodies. `-fno-fast-math`,
`-ffp-contract=off`, `-fno-strict-aliasing` and `FE_TONEAREST` define the floating
profile. Link wrappers observe original `fopen` success/order and the final
original `rcMergePolyMeshes` polygon count; they call the originals unchanged.
Neither wrapper replaces source behavior.

Query admission makes an owned raw-tile copy with a 0- or 4-byte base adjustment:
original Detour uses `dtAlign4` for its 8-byte-aligned `dtLink` array. The driver
checks both vertex-count parities and the actual loaded link pointer. Query and
mesh owners are destroyed before the copied bytes, including exception exits.
Failed original mesh initialization is process-bounded: its partial owner is not
destructed because pinned `init` can fail before initializing the tile array that
its destructor traverses. The failed capture exits without publishing a result.
This allocation/alignment-only harness adaptation leaves every serialized byte
and original algorithm unchanged. Both wide-height and scalar query captures are
replayed under the optional alignment sanitizer above.

The writer helper reuses the existing guarded original `ConvertADT` excerpt
harness and its byte transport shim. `authored_ridge.py` changes only authored
MCVT values. `source/` retains complete navigation source/config/offmesh excerpts
verbatim, with byte intervals and 282 pinned source/header hashes. Original
notices are retained beside those excerpts and under `notices/`.

## Fixture shape

`fixtures/manifest.json` describes 15 terrain/combined cases. Per-case
`expected.json` records source config, exact binary32 bounds, actual terrain
file opens, discovered tile union, input counts and query expectations.

Each compressed array fixture is a zlib stream of this little-endian format:

1. solid vertex count u32, solid triangle count u32
2. solid f32 vertices, xyz triplets, then u32 triangle-index triplets
3. liquid vertex count u32, liquid triangle count u32
4. liquid f32 vertices, then u32 indices, then one u8 type per triangle

`source-terrain.bin.zlib` is exactly public `TerrainBuilder::loadMap`, before
cleanup. `source-combined.bin.zlib` adds original `loadVMap` before cleanup.
`source-mesh.bin.zlib` is after original `cleanVertices`. The manifest includes
whole-blob and individual-array SHA256 values, allowing Rust tests to compare
full arrays without introducing a decompression dependency.

`fixtures/authored/` contains the exact authored ridge ADT inputs and original
`.map` outputs. The selected original generator tile is map 0, x 30, y 31; its map
filename is `0003130.map`. Its four neighbors are +x `0003131.map`, -x
`0003129.map`, +y `0003230.map`, -y `0003030.map`. Ridge neighbor inputs extend the
same continuous authored height function across the tile edges.

The existing captured WMO has unique ID 1, ADT 11, position (16600,16100,4),
rotation (0,0,0), scale 1, flags 4 and name `Hull.wmo`. Its original extractor
and assembler bytes are reused from the `captured_wmo` map-products case.

`fixtures/models/` adds 14 model-only original `loadVMap` captures. Inputs include
originally assembled products and authored raw records. They cover nonzero
XYZ Euler rotations, scale, signed zero, two groups, seven global spawn slots,
shared/mutated duplicate ID records, case-sensitive `.m2`/`.M2` substring winding,
filename/flag disagreement, rectangular 2x3 liquids, hidden flags and liquid type
masking. Their combined pre-clean arrays retain unused liquid/model vertices.

`fixtures/quantized/` adds two original `ConvertADT -f1` outputs (uint8 and
uint16) and complete original terrain-collection arrays. The height function is
scaled before authoring for uint8, rather than quantizing an expected map by hand.

`fixtures/configs/` records direct original default/map/tile merge behavior,
float-to-int narrowing and squared region sizes. An authored config demonstrates
that tile 1,23 and 12,3 share the unpadded key `123`; object-valued entries are
removed only after tile merge.

## Positive generation and source rejection

Both `ridge_wmo` and `ridge_neighbors_wmo` genuinely emit 634 polygons:
632 area 11/ground and 2 area 10/steep. Each emits a 28-byte `.mmap`; the no-neighbor `.mmtile` is 106,252 bytes
and the four-neighbor `.mmtile` is 109,580 bytes. The no-neighbor MMTile SHA256 is
`d9032754b3a2bdf0e0c2c3891635256ad00915624b0d9bbe9ae76c5206eb4971`.
The records include 25 nearest-poly/height queries and 24 successful complete
corridors with straight-path coordinates, all measured on the emitted bytes by
original Detour, using ground-only filter flags.

`ridge_only` merges exactly 625 polygons and the original intentionally emits no
MMTile. This is observed at the original merge call, not inferred from absence.
The existing collapsed `flat.map` contains `MAP_HEIGHT_NO_HEIGHT`, so original
terrain collection returns before trying neighbors. Those are distinct source
behaviors.

The source `.mmap` stores `maxPolys` as bits `80000000` (signed INT_MIN) and
origin Y as bits `00800000` (FLT_MIN). The selected-only tile union has maxTiles 1
and origin (533.3333,FLT_MIN,0); the five-tile union has maxTiles 5 and
origin (0,FLT_MIN,-533.3333). Production must not reinterpret these source fields
as a convenient positive limit or reconstruct origin only from the selected tile.

`helper-request.bin.zlib` encodes original captured arrays/config/parameters in
the production helper's documented wire format. This is a test-input adapter;
expected output files and queries still come exclusively from the original.

## Generated Classic liquid evidence

`liquid_full_types_wmo` reuses the unchanged retained
`crates/benilla-native-terrain/tools/fixtures/liquid_full_types.adt.test-fixture`
and its original `.map`, plus exactly the captured Hull WMO used by `ridge_wmo`.
The selected original tile remains map 0, x 30, y 31; production `TileId` is
map 0, x 31, y 30. Its manifest entry binds the ADT/map, original writer/source
provenance and individual VMap product hashes. No neighboring maps are added.

Original collection produces 16,641 liquid vertices and 32,768 liquid triangles:
28,032 area 9 (water) and 4,736 area 8 (magma/slime). The collapsed solid terrain
contributes no vertices or triangles; Hull adds three solid vertices and one
triangle. Unchanged original `MapBuilder` emits 998 polygons: 770 area 9/flag 4
and 228 area 8/flag 8, with no ground polygons. This genuinely generated output
is retained with its helper request and exact MMap/MMTile bytes.

Original Detour successfully initializes and loads the output. All 25 nearest
queries use include flags 1 and exclude flags 0 and return `DT_SUCCESS` with
reference 0; no height or corridor query is fabricated for those legitimate
ground-only misses. Detailed flags, statuses and exact query bits are recorded
only for this new case, leaving every existing per-case fixture unchanged.
The old ridge cases still require successful complete ground corridors.

`check_original.py` in the generator crate compares this new case's actual helper
MMap/MMTile bytes against the independently generated original. Generation does
not establish reader acceptance or swimming support. This exact output first
fails the production reader's strict base-vertex envelope check (`Format`): its
quantized maximum base-vertex height exceeds its original header maximum. Wet
flags remain unsupported independently; no admission rule is relaxed here.

## Measurements and limits

`measurements-linux-x86_64.json` records one complete original-process run per
case, including collection, generation and original Detour queries. Positive
single-tile runs took approximately 2.2–2.5 seconds with a measured 13.1 MiB child
peak RSS. The Python process launcher contributes to the measured high-water
mark. The selected tile has 33,025 terrain vertices and 65,536 terrain triangles;
four cardinal strips increase pre-clean terrain vertices to 165,125 and triangles
to 67,584. The WMO adds three vertices and one triangle.

These fixtures prove only bounded source-compatible generation and queries.
They do not establish real-install resource sizing, all liquid/off-mesh movement
policy, allocation-failure behavior, cross-platform parity, game startup or NPC
body support. Those belong to their production acceptance tests and consumers.

## Scalar nearest-query replay

`fixtures/scalar-nearest/` is additional original-query evidence. It leaves every
prior fixture unchanged. The input to each query is the exact on-surface point
previously returned by original Detour, and the new call uses half-extents
(5,5,5), include flags 1 and exclude flags 0. The original `findNearestPoly` and
`getPolyHeight` calls are executed again on the actual original MMap/MMTile bytes.
The result records exact input/output bits, reference/status and height at both
the input and returned point. These are independently measured expectations,
not assertions that a production reprojection happened to be stable.

Its manifest binds both positive fixtures by their original input file hashes.
It also lists the exact five neighbor ADT/map filenames and SHA256 values, plus
the full source tile union. The MPQ basename template is `{directory}_X_Y.adt`;
the source map filename places Y before X. Only the selected ADT needs the
existing captured WMO placement; neighboring ADTs contain authored terrain only.
