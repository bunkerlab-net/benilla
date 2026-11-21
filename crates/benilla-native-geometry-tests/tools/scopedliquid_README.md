# Exact-source bounded wet VMap liquid oracle

This authored oracle covers the loaded static `VMapManager2::GetLiquidLevel`
chain. It does not establish composite `TerrainInfo` liquid/area selection,
liquid status, body support, actual installation geometry, source authenticity,
or production-session authority. No installation assets or native input parser
are used.

Regenerate offline against CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

```sh
python3 crates/benilla-native-geometry-tests/tools/scopedliquid_generate.py /path/to/mangos-classic
python3 crates/benilla-native-geometry-tests/tools/scopedliquid_generate.py /path/to/mangos-classic --check
python3 crates/benilla-native-geometry-tests/tools/scopedliquid_generate.py /path/to/mangos-classic --check --sanitize
```

## Source execution and native bytes

The generator uses the same whole-file SHA-256 guards, source-exact extracted
manager/tree bodies, original G3D/VMap translation units, and types-only
platform stub as the [dry location oracle](scopedlocation_README.md). It compiles
the unchanged original `WmoLiquid::GetLiquidHeight`, `GroupModel::GetLiquidLevel`,
`ModelInstance::GetLiquidLevel`, geometry selection, and native serializers.
`GetLiquidMask` and the manager body are extracted byte-for-byte from the guarded
source. No interpolation, selection, transformation, or serialization algorithm
is reimplemented in this oracle.

The harness constructs numeric `WmoLiquid` arrays using the original constructor,
attaches them with `GroupModel::setLiquidData`, and serializes their owning
`WorldModel` objects. Direct queries use those same liquid/group objects. The
original assembler's transformed-bound machinery creates the placement bounds.
The original point/ray/BIH chain selects the actual instance and group before
manager liquid queries. No selected-group or floor stub supplies oracle answers.

Every run also reconstructs all eleven existing base/dry-location native products
and `scopedlocation.tsv`, requiring byte identity and never rewriting them. The
new [scopedliquid directory](fixtures/vmap_source/scopedliquid) contains 21 native
products: one tree, one tile, and nineteen models, with 25 placements. These
include a 3-by-2 liquid grid, dry and wet-hole occluders, a liquid-bearing M2,
all source-mapped type IDs, unknown and zero types, zero-cell grids, and a
zero-corner liquid. Metadata includes high-bit root/group/ADT IDs. Placements
include yaw, translation, half scale, and simultaneous pitch/yaw/roll at scale
1.75.

## Recorded observations

[scopedliquid.tsv](fixtures/vmap_source/scopedliquid.tsv) contains 5,511 manager
queries, 4,566 selected locations, and 1,662 wet results. Its first twenty
columns match `scopedlocation.tsv`: dataset, case, world XYZ, actual converted
internal XYZ, location hit, ground, spawn/root/group IDs, group flags, ADT ID,
requested mask, liquid result, resulting level/floor/type. All float columns are
eight hexadecimal binary32 digits. IDs, masks, flags and booleans are decimal;
`root_id` is the source signed `int32` preserving its raw bits.

Three appended columns record the separately executed selected-instance liquid
result, its resulting level, and whether that call was executed. It is omitted
for missing locations and three explicitly named mask-rejected unsafe-coordinate
cases. Those manager calls return before liquid interpolation and demonstrate
that mask rejection precedes the source casts; a direct instance query at the
same points would be outside the source-defined cast domain.

The level, floor, and type inputs are respectively `123.25`, `-100000`, and
`0x12345678`. The harness independently asserts that every false manager/instance
result preserves level, every selected location overwrites floor/type, and every
missing location preserves all three outputs. Concrete original-source results:

- A nearest dry group or dry instance occludes a lower wet candidate; no fallback
- A nearest group's disabled liquid tile also occludes a lower wet candidate
- A level below the selected floor can still return true: named
  `wet_level_below_floor_still_true` yields level bits `bf60b988`, floor zero
- An unknown type, including zero, succeeds with mask zero but rejects nonzero
  masks after writing floor/type
- All 256 requested masks are tested for raw types
  `0, 1, 2, 3, 4, 21, 41, 61, 999, 0x12345678`
- The pinned source maps water types `1/41/61` to mask 8, ocean `2` to 2,
  magma `3` to 1, slime `4/21` to 4, and unknown types to zero
- A liquid-bearing M2 alone is excluded; it also does not occlude overlapping WMO
  liquid geometry
- Tilted instance results retain the original approximation
  `(local_level - local_query_z) * scale + internal_query_z`

[scopedliquid_direct.tsv](fixtures/vmap_source/scopedliquid_direct.tsv) contains
1,372 original `WmoLiquid` and `GroupModel` query pairs, including 962 wet hits.
Columns identify the model and serialized group index/ID, case, local XYZ,
liquid result/level, group result/level, and raw type. The harness asserts equal
liquid/group outcomes. Direct inputs avoid world-conversion quantization and
cover both asymmetric interpolation triangles, diagonal equality, exact and
adjacent grid edges, negative fractional offsets, upper/outside bounds, signed
zero, and query-Z independence. The numeric grid uses flag bytes
`00, 0f, 10, ff, 08, f0`; only low-nibble `f` disables a cell. Liquid corner Z
is deliberately unrelated to the stored absolute heights. Zero-cell grids
retain a liquid/type payload but return false.

The exact tile-size macro is `533.333f / 128.f`, not the terrain grid constant.
The original code casts the X coordinate before its negative check, and casts Y
only after X succeeds. Direct negatives stay in `(-1,0)` tile units, which
truncate to representable zero. Deliberately unsafe Y values occur only behind
a safe early X rejection. No query executes an out-of-domain original
float-to-unsigned cast. `--sanitize` instruments original translation units with
`-fsanitize=float-cast-overflow` and traps on any violation; its fixture comparison
is byte-for-byte identical.

## Native LIQU size discrepancy

[scopedliquid_sizes.tsv](fixtures/vmap_source/scopedliquid_sizes.tsv) records each
original `GetFileSize` result, actual bytes emitted by `WmoLiquid::writeToFile`,
serialized group byte count, and the group's `LIQU` header offset, along with
dimensions, corner, type and original type-mask mapping.

The harness measures the original output streams with `ftell`, checks the
group's declared `LIQU` length, and compares the entire group's liquid suffix
with the independently serialized original payload. Every wet payload emits
four more bytes than `GetFileSize` reports: the helper omits `iType`, while the
writer emits it. Examples are 74 declared/78 emitted for 3-by-2, 37/41 for
1-by-1, 32/36 for 0-by-2 or 2-by-0, and 24/28 for 0-by-0. A dry group's zero
length remains distinct from every zero-cell wet payload. A reader cannot
treat the native declared length as the complete wet payload extent.

Generation and both normal and cast-sanitized byte comparisons were verified on
Linux x86-64 with C++17, round-to-nearest, no fast math, and no contraction.
Cross-platform and real-asset validation remain separate. Original CMaNGOS and
G3D notices remain in the [fixture licensing record](fixtures/vmap_source/README.md).
This authored harness and generator are GPL-3.0-or-later.
