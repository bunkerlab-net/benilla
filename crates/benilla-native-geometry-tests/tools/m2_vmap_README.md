# Exact-source authored M2 extraction and VMap assembly oracle

This bounded oracle supplies numeric input to the original M2 extractor and
assembler, then queries the resulting original native products. It establishes
raw byte, serialized model and static point-query compatibility for the authored
cases only. It does not establish real-installation compatibility, gameplay
admission, world compilation, body support, or pathfinding. No installation
assets, world databases, downloaded programs or client executables are used.

## Reproduce offline

Use the existing local CMaNGOS Classic source at
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

```sh
python3 crates/benilla-native-geometry-tests/tools/generate_m2_vmap_fixture.py /path/to/mangos-classic
python3 crates/benilla-native-geometry-tests/tools/generate_m2_vmap_fixture.py /path/to/mangos-classic --check
CMANGOS_SOURCE=/path/to/mangos-classic python3 -m unittest discover -s crates/benilla-native-geometry-tests/tools -p 'test_generate_m2_vmap_fixture.py'
```

The generator guards complete source files with the existing
[vmap_source_hashes.sha256](vmap_source_hashes.sha256), plus all six extractor
implementation/header files in
[m2_vmap_source_hashes.sha256](m2_vmap_source_hashes.sha256). It rejects a changed
source before compilation. `--check` regenerates into a temporary directory and
compares every generated product byte-for-byte without rewriting any fixture.
Existing VMap fixtures and their guards are not regenerated or changed.

Compilation uses the original `TileAssembler.cpp`, `WorldModel.cpp`, `BIH.cpp`,
`ModelInstance.cpp`, and G3D `Ray.cpp`, `Vector3.cpp`, `AABox.cpp`, `Matrix3.cpp`
and `g3dmath.cpp` translation units unchanged. Original extractor `model.h`,
`modelheaders.h`, `vec3d.h` and `vmapexport.h` declarations are included unchanged.
The source-exact extracted methods are checked into
[the generated source fragment](fixtures/m2_vmap_source/source-fragments.cpp.test-fixture):

- `Model::Model`, `Model::open`, `Model::ConvertToVMAPModel`, `fixCoordSystem`
- The original raw-magic declaration from `vmapexport.cpp`
- `MapRayCallback`, `StaticMapTree` constructor/destructor, `getIntersectionTime`
  and `getHeight`
- `VMapManager2::convertPositionToInternalRep`
- The two G3D AABox collision functions used by the existing oracle

The only extractor substitute is `MPQFile` input acquisition: an authored file
reader returns the exact generated input bytes. No archive behavior is simulated.
The `loadlib.h` and `Platform/Define.h` substitutes provide fixed-width integer
aliases only. Original `Model::open` reads the native header and collision
arrays; original conversion performs the coordinate transforms and index swap.

Original `TileAssembler::convertWorld2` reads an authored `dir_bin` containing
original `ModelSpawn::writeToFile` records, computes placement bounds from the
raw vertices, builds the map BIH, and writes the tree, tile and models. Original
`GroupModel_Raw::Read`, `WorldModel_Raw::Read`, `convertRawFile`, group/model BIH
builders and writers all run. There is no authored replacement for any of them.

The query harness reads the generated BIH, spawn records and `.vmo` models using
the original `BIH::readFromFile`, `ModelSpawn::readFromFile` and
`WorldModel::readFile`. It substitutes manager ownership with direct binding of
the validated authored records and sets model flags from each spawn as the
original map loader does. It does not invoke the original MPQ implementation,
VMapManager ownership/loading, terrain, or world compiler. Protected/private
access changes enable fixture binding and bounds inspection only; arithmetic,
query methods and data layout are unchanged.

The checked numeric profile is Linux x86-64, C++17, little-endian IEEE binary32,
round-to-nearest, `-O0`, no fast math, no contraction and no strict aliasing.
Cross-platform and real-asset verification remain separate.

## Numeric fixtures

All files are under [fixtures/m2_vmap_source](fixtures/m2_vmap_source).

- `authored-m2.m2.test-fixture`: authored MD20 version 256, 324-byte native
  header, 25 vertices and 32 triangles on the plane `z = 4`, with `x` in
  `[8,16]` and `y` in `[12,20]`
- `signed-zero-m2.m2.test-fixture`: authored MD20 version 257 with the same
  counts, `x` and `y` in `[0,8]`, negative-zero values on the zero axes, and
  negative-zero `z` on every vertex
- `authored-m2.raw.vmap`, `signed-zero-m2.raw.vmap`: original extractor outputs,
  each 588 bytes; all vertex bit patterns remain unchanged after the two source
  coordinate transforms, and each triangle exchanges its second/third indices
- `products/authored.m2.vmo`, `products/signed-zero.m2.vmo`: original assembler
  outputs, each 1,364 bytes with 32-triangle multilevel mesh BIHs
- `products/000.vmtree`, `products/000_32_32.vmtile`: original map tree and tile
  containing M2 spawn 101 at the identity transform and M2 spawn 102 translated
  to `(40,0,2)` with scale `0.5`; both rotations are zero. Spawn 103 reuses
  `authored.m2`, translated to `(70,20,4)`, Euler rotation `(13,37,-9)` degrees,
  and scale `1.75`

The input filenames intentionally end in `.test-fixture`; the distribution
asset guard remains unchanged. Temporary raw `.m2` names are used only inside
the generator workspace to match the original assembler's model naming.

## Bound and query distinction

Original extraction writes six positive-zero group-bound floats even though
the vertices are nonzero. Original assembly preserves those zero group bounds
and builds a zero-root group BIH. It independently builds mesh BIHs around the
actual triangles and placement bounds around transformed actual vertices.

The source `WorldModel::IntersectRay` single-group fast path bypasses the group
BIH and still hits the off-origin plane. Model and instance rays therefore
remain useful with the serialized zero bounds; `ignoreM2Model=true` misses.
The source point/location paths retain their group-bound tests. They miss at
`(10,14,4)` on the off-origin plane and `(2,2,0)` on the signed-zero plane. The
signed-zero model can return a point/location hit at positive/negative zero
origin because it is inside its zero box. Both M2 instances always decline
area/location queries before model-local traversal. Replacing the group bound
with mesh-derived geometry would change these point results.

`rays.tsv` contains 1,530 rows (492 hits, 1,038 misses), with interior/edge/corner,
outside-box, both ray directions, signed-zero, strict endpoint and on-surface
cases. Scope `authored`, `signed-zero` or `rotated` calls original model-local intersection;
`map` calls original map-tree intersection using the serialized placements.
Each runs with both values of `ignore_m2` and nearest-hit mode. All floats use
eight hexadecimal binary32 digits, boolean columns use decimal zero/one.

`heights.tsv` contains 366 original map-tree height calls (221 finite hits),
including negative maximum distance for upward searches. The source miss is
positive infinity (`7f800000`), not a portable consumer sentinel. The rotated
query positions are transformed by original `ModelPosition::transform`. Height
inputs are then roundtripped through original
`VMapManager2::convertPositionToInternalRep` to obtain the actual binary32
internal query reachable through public world coordinates. This avoids assuming
a lossless world/internal conversion for non-axis-aligned positions. Rotated map
rays use two source-transformed endpoints and the source-style normalized
endpoint difference, with all resulting input bits stored in the TSV.

`points.tsv` contains 18 rows of original model area/location, direct group,
and placed instance area/location results. Columns are declared in the header.
Miss distance sentinels are 99 for model area, 88 for model location, and 77 for
group. Area metadata on a miss remains root 123, group 456, flags 789; location
root remains 321. On the two origin hits all IDs/flags and distances become
zero. `world_px/world_py/world_pz` are the exact transformed instance query
inputs; both instance result fields are false for every row. Scope `rotated`
uses the authored model with spawn 103; the other scopes use spawns 101 and 102.

## Provenance and licenses

CMaNGOS's GPL-2.0-or-later notices are retained in the generated source fragment
and in the existing
[original source notice](../../benilla-app/src/net/p2p/world/native_geometry/products/CMANGOS-NOTICE.txt).
The authored harness and generator are GPL-3.0-or-later. G3D's Modified BSD grant
is retained in [vmap_G3D_LICENSE.cpp.txt](vmap_G3D_LICENSE.cpp.txt); the pinned BIH
header retains its Sunflow-derived MIT/X11 attribution. No external
implementation is vendored or patched by this generator.
