# Exact-source bounded dry VMap location-floor oracle

This authored oracle covers loaded static `GetLocationInfo` and the dry
`GetLiquidLevel` floor/type side effects. It does not establish a wet-liquid
query, composite `TerrainInfo` selection, liquid status, body support, actual
installation geometry, source authenticity, or production-session authority.

Regenerate offline against CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

```sh
python3 crates/benilla-native-geometry-tests/tools/scopedlocation_generate.py /path/to/mangos-classic
python3 crates/benilla-native-geometry-tests/tools/scopedlocation_generate.py /path/to/mangos-classic --check
```

## Source execution and byte identity

The generator reuses the whole-file SHA-256 guards and native source compilation
from the [authored VMap fixture contract](fixtures/vmap_source/README.md), plus
[three additional whole-file guards](scopedlocation_source_hashes.sha256).
It compiles the original `BIH`, `WorldModel`, `ModelInstance` and G3D translation
units, including the source point traversal, group containment/ray operation,
nearest-group selection, and transformed highest-instance selection. It extracts
these additional original bodies byte-for-byte from the checked sources:

- `LocationInfoCallback` and `StaticMapTree::GetLocationInfo`
- `VMapManager2::GetLiquidLevel` and `GetLiquidMask`
- `StaticMapTree::getHeight`, `VMapManager2::GetMapTree`,
  `VMapManager2::getHeight`, and `IVMapManager::isHeightCalcEnabled`

The existing harness supplies exact source coordinate conversion and native
serialization. Its minimal manager declaration substitutes only unrelated
ownership/loading machinery with the original map-pointer storage shape,
populated by the harness, and an enabled height flag. No point, intersection,
transformation, height, liquid or serialization algorithm is reimplemented.
The liquid-mask constants come from the checked original `GridMapDefines.h`.

Every run reconstructs the six existing authored geometry products and requires
byte identity with the checked-in files; it never rewrites them. The additional
[scopedlocation directory](fixtures/vmap_source/scopedlocation) contains five
new native products: one tree, one tile, and three models. All geometry is
numeric authored input, serialized by original source. No installation assets
are used and no source native parser is invoked. Rust tests load these exact
serialized products through the checked native-product decoder.

The added products contain nine placements of three dry models. Four volumetric
floor groups include equal-height geometry with different group IDs/flags;
another serialized group has zero vertices and no triangles. The single-group
raised model has high-bit root/group IDs. Explicit unreferenced corner vertices
preserve thick bounds under the original transformed-bound assembler. Instances
include overlaps, equal-height ties, nonzero yaw/pitch/roll, nonunit scales, and
an M2 both alone and overlapping WMO geometry.

## Fixtures and observable distinctions

[scopedlocation.tsv](fixtures/vmap_source/scopedlocation.tsv) contains 2,032
queries, including 1,040 location hits. `base` uses the unchanged existing
products; `scopedlocation` uses the new isolated directory. Float columns are
eight hexadecimal binary32 digits; IDs, flags, masks and booleans are decimal.
`root_id` is the source signed `int32`, preserving its raw bits when interpreted
as `u32`. Column order is declared in the file header. Every row records:

- World input and the actual source-converted internal point
- Location result, floor bits, spawn/root/group IDs, group flags and ADT ID
- Dry manager result and resulting level/floor/type, including masks 0 and 15
- Original static `getHeight` at the same world XY and `z + 2`, search distance 50

The manager inputs are level 123.25, floor -100,000 and type `0x12345678`.
Every result is dry/false. On a location hit the original method nevertheless
writes the selected floor and type 0; on a miss it leaves all three sentinels
unchanged. The harness independently asserts these mutations before writing
any row.

The cases distinguish point containment from vertical height search, including
map/instance/group gaps, exact and adjacent bounds, world-coordinate rounding,
positive/negative zero, empty groups, M2 exclusion, and floors above the query
within the source 0.1 local-unit origin offset. Original group selection uses
strictly smaller ray distance; original instance selection uses strictly higher
reconstructed world Z. Equal candidates preserve serialized point traversal
order rather than numeric ID order.

Concrete source results on the new authored products:

- Internal `(0,0,5)`: location floor 4, static height at `z + 2` is 6
- Internal `(0,0,7)`: equal-height tie selects spawn 203/group 204, floor 6
- Internal `(0,0,8)`: location floor bits `0x40bfffff` (one binary32 ULP below
  6 after source offset arithmetic), while M2-including static height is 7
- Internal `(0,0,11)`: no location, while static height remains available

These distinctions do not by themselves determine a composite water/terrain
result.

[scopedlocation_bih.tsv](fixtures/vmap_source/scopedlocation_bih.tsv) contains
492 original `BIH::intersectPoint` callback traces. Authored structures cover
root boundaries, empty leaves, source omitted-child infinity sentinels, empty
children, gaps, touching/overlapping clips, nested overlap, leaf permutations,
BVH2/plane clips, all axes, signed zero and subnormals. The fixed root is
`[-2,2]^3`. Callback IDs and serialized words are hexadecimal; `-` denotes an empty
object or callback list. The harness reuses only the existing authored BIH data
holder and printers, not the ray algorithm.

Original CMaNGOS and G3D notices remain in the
[fixture licensing record](fixtures/vmap_source/README.md). The authored harness
and generator are GPL-3.0-or-later. Generation and byte-for-byte `--check` were
verified on Linux x86-64 with C++17, no fast math, no contraction and
round-to-nearest; cross-platform and real-asset validation remain separate.
