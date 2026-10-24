# Exact-source authored dry WMO extraction and assembly oracle

This offline oracle supplies authored numeric WMO root/group bytes to the pinned
CMaNGOS Classic extractor, runs the original complete raw assembler, reloads its
products, and records original ray, height and location queries. It establishes
bounded compatibility for these authored dry cases only. It does not establish
installation compatibility, WMO liquid production, doodad extraction, a complete
world compiler, gameplay admission, body support or pathfinding. No installation
assets, downloaded executables, world databases or renderer geometry are used.

## Reproduce

Use local CMaNGOS Classic source at
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

```sh
python3 crates/benilla-native-geometry-tests/tools/generate_wmo_vmap_fixture.py /path/to/mangos-classic
python3 crates/benilla-native-geometry-tests/tools/generate_wmo_vmap_fixture.py /path/to/mangos-classic --check
CMANGOS_SOURCE=/path/to/mangos-classic python3 -m unittest discover -s crates/benilla-native-geometry-tests/tools -p 'test_generate_wmo_vmap_fixture.py'
```

The existing [VMap source guard](vmap_source_hashes.sha256) protects the assembler,
reader, query and G3D sources. The additional
[WMO source guard](wmo_vmap_source_hashes.sha256) protects the complete WMO and
export implementations, their included extractor headers, the filename helpers,
and the included libmpq declarations. A mismatch fails before compilation.
`--check` compiles and regenerates into a temporary directory, then checks the
complete fixture filename set and every byte without rewriting checked fixtures.
The existing M2 and VMap fixtures and guards are unchanged.

Compilation uses Linux x86-64 C++17, little-endian IEEE binary32, round-to-nearest,
`-O0`, no fast math, no floating-point contraction and no strict aliasing.
Cross-platform verification is separate.

## Original code and the bounded seams

The generated [source fragment](fixtures/wmo_vmap_source/source-fragments.cpp.test-fixture)
contains unchanged extracted definitions from the guarded source:

- `WMORoot` construction, open, raw header conversion and destruction
- `WMOGroup` construction, open, conversion, `ShouldSkip` and destruction
- The complete `ExtractSingleWmo` orchestration and `FileExists`, original raw
  magic, small-mode selection, work-path and doodad-map declarations
- Original mutable `GetPlainName`, `fixnamen` and `fixname2` helpers
- Original map ray/area/location callbacks, map construction/destruction,
  `getIntersectionTime`, `getHeight`, `getAreaInfo` and `GetLocationInfo`
- Original `VMapManager2::convertPositionToInternalRep` and the same two G3D
  AABox collision methods used by the existing oracle

`TileAssembler.cpp`, `WorldModel.cpp`, `BIH.cpp`, `ModelInstance.cpp`, and G3D
`Ray.cpp`, `Vector3.cpp`, `AABox.cpp`, `Matrix3.cpp` and `g3dmath.cpp` are compiled
unchanged as complete translation units. Original `wmo.h`, `adtfile.h`, model,
vector, MPQ and export declarations are included unchanged. Only
`Platform/Define.h` and `loadlib/loadlib.h` are types-only stubs.

The authored `MPQFile` implementations substitute acquisition with a closed map
of named authored files, retaining the original class declaration and original
`flipcc`. They do not read an installation or simulate archive decompression.
The fixture begins each root/group with `MVER 17`; the source's pre-termination
`strcmp` in group parsing is not patched. Every group has a 68-byte MOGP header
with nested source chunks. The source's own header-size override reads them.

The root declares no doodads and contains no MODN, MODS or MODD. A guarded
`ExtractSingleModel` seam throws if called: it never falsely reports a doodad
model as extracted. This oracle therefore does not prove the MODN extraction
side effects of `WMORoot::open`. Liquid is similarly outside the proof: no MLIQ
is authored, and there is no liquid expected output. The production boundary's
explicit liquid rejection is tested separately.

The exact `ExtractSingleWmo` opens all six declared groups, calls the original
skip predicates, adds each retained group's returned selected-triangle count,
and seeks back to rewrite both root counts. The fixture separately removes the
named member `Authored_004.wmo` (which normally skips) from the acquisition map and
verifies the original orchestration deletes its partial output. Original code
returns true despite that missing-member case; existence is checked separately.
No truncated buffer is passed to the original unsafe parser.

Original `TileAssembler::convertWorld2` reads authored `dir_bin` records produced
by original `ModelSpawn::writeToFile`, reads the raw WMO using
`WorldModel_Raw::Read`/`GroupModel_Raw::Read`, builds all mesh/group/map BIHs and
writes the tree, tile and model. WMO placement bounds are supplied in the
placement records, as the original assembler requires. Their corners are
transformed using original `ModelPosition` arithmetic. The WMO model's group
bounds remain the exact input MOGP values; they are not recomputed from vertices.

The harness reads the serialized BIH, spawns and model using original readers.
It directly binds model objects instead of invoking the VMapManager ownership
loader, and sets model flags from each original spawn record. Private/protected
access changes permit binding and inspection only. All recorded intersection,
height and point/location arithmetic remains original source.

## Authored geometry

All 15 files are under [fixtures/wmo_vmap_source](fixtures/wmo_vmap_source).
Input filenames end in `.test-fixture`; the only raw temporary `.wmo` exists
inside the generator workspace. The source's `fixnamen` produces the model
name `Authored.wmo`.

- Root ID 4242 declares six groups and has no doodads
- Group 701 contains all 256 possible MOPY flag bytes, one disjoint triangle per
  mask. Its material byte is always 127. Vertices occupy 16 by 16 cells on
  `z=4`, with cells starting at `(8,12)` and spacing four. Each triangle uses
  `(x,y)`, `(x+2,y)`, `(x,y+2)` in descending source-index order. Original small
  mode selects 160 triangles: bit 8, or bit 32 without bit 4. Thus `0x24` is
  excluded and `0x0c` retained. Two additional unreferenced vertices are dropped
- Group 702 is a 32-triangle, 25-vertex plane at `z=10`, covering `[12,20]` by
  `[16,24]`; it uses alternating `0x0c` and `0x20` flags and overlaps group 701
- Group 703 has a source triangle with mask `0x24`, yielding zero selected
  triangles and vertices. Its group record and nonzero MOGP bounds are retained
- Groups 704, 705 and 706 each have valid collision geometry but are skipped,
  respectively, by unreachable bit `0x80`, antiportal bit `0x4000000`, and the
  MOGN name `antiportal`

The raw header has selected-triangle count 192, retained-group count 3 and root
ID 4242. This differs from the 505 selected vertices. Original ascending source
vertex-index compaction produces first output indices `2,1,0`, distinguishing
it from first-use renumbering. No index winding or coordinate conversion occurs
in this WMO path. The raw output is 7,472 bytes.

Each raw GRP branch is the source MOBA word at offset 16 in a 24-byte record,
expanded to uint32. Group 701 branches are `12,345,65535`, group 702 branches are
`96,17`, and group 703's branch is `3`. These deliberately differ from selected
triangle counts and are not regenerated from collision output.

The original assembled model is 11,840 bytes. Spawn 201 uses identity placement
with ADT ID 13. Spawn 202 uses translation `(180,20,4)`, original Euler rotation
`(13,37,-9)` degrees and scale `1.75`, with ADT ID 29. Both have original
`MOD_HAS_BOUND` flags and reference the same model.

## Query corpus

All floats are eight hexadecimal binary32 digits; IDs and booleans are decimal.

- `rays.tsv`: 2,208 original nearest-hit calls, 1,350 hits. Columns match the M2
  corpus: scope, case, origin xyz, direction xyz, maximum, ignore-M2, hit,
  resulting distance. Scopes `authored` and `rotated` invoke model-local rays;
  `map` invokes original map-tree rays. Every case runs with both ignore-M2
  settings, which do not change WMO results
- `heights.tsv`: 540 original map height calls, 171 finite hits. Columns are
  case, internal xyz, maximum, height. Positive infinity is the source tree's
  miss sentinel; the consumer applies its existing public manager sentinel
- `locations.tsv`: 540 original map location calls, 170 hits. Columns are case,
  public world xyz, exact internal xyz, hit, floor, spawn ID, root ID, group ID,
  group flags, ADT ID. Each hit is cross-checked against original `getAreaInfo`.
  Miss floor is negative infinity and miss metadata is zero

Cases include every MOPY mask, overlapping floors, retained-empty and skipped
geometry, outside bounds, signed-zero query coordinates/directions, upward
queries, and strict ray endpoints. Map query positions are roundtripped through
original world/internal conversion before height/location calls, recording the
actual binary32 inputs reachable through the public coordinate boundary.
Rotated map rays use two source-transformed endpoints and the normalized
endpoint difference. No renderer or alternate intersection implementation
supplies expected values.

## Notices

Original CMaNGOS GPL-2.0-or-later notices are retained in the generated source
fragment and the existing [CMaNGOS notice](../../benilla-app/src/net/p2p/world/native_geometry/products/CMANGOS-NOTICE.txt).
The authored harness, generator and regression tests are GPL-3.0-or-later. G3D's
Modified BSD grant remains in [vmap_G3D_LICENSE.cpp.txt](vmap_G3D_LICENSE.cpp.txt),
and the guarded original BIH header retains its Sunflow MIT/X11 attribution.
No external implementation is downloaded, patched or vendored by this generator.
