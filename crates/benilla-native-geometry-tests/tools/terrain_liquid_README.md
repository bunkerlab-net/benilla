# Exact-source TerrainInfo liquid with dry loaded VMaps

This authored oracle covers the bounded composite
`TerrainInfo::getLiquidStatus` query with dry static VMap models and supported
raw terrain/DBC products. It does not establish wet WMO liquid, moving models,
body support, actual installation coverage, source authenticity, startup
admission, playable-world parity, or production-session authority.

Regenerate offline against CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

```sh
python3 crates/benilla-native-geometry-tests/tools/generate_terrain_liquid_fixture.py /path/to/mangos-classic
python3 crates/benilla-native-geometry-tests/tools/generate_terrain_liquid_fixture.py /path/to/mangos-classic --check
```

## Original source execution

The generator guards complete upstream files with the existing
[VMap source hashes](vmap_source_hashes.sha256),
[location source hashes](scopedlocation_source_hashes.sha256), and
[GridMap/DBC source hashes](generate_area_liquid_fixture.py). It extracts these
complete original method bodies byte-for-byte:

- `TerrainInfo::getLiquidStatus`, `GetHeightStatic`, and `GetGrid`
- `GridMap::getLiquidStatus`, `getArea`, `getHeightFromFlat`,
  `getHeightFromFloat`, and `isHole`
- `GetAreaEntryByAreaID`
- The [location oracle's](scopedlocation_README.md) original manager/tree
  height, location, liquid, and coordinate-conversion methods

The complete original `BIH.cpp`, `WorldModel.cpp`, `ModelInstance.cpp`, and G3D
translation units execute the authored geometry. No height, location, floor,
liquid, ray, or point result comes from a scripted callback. The source's
`LocationInfoCallback`, transformed group/instance selection, `GetLiquidLevel`
floor side effect, and manager height retries execute unchanged. All existing
base and scopedlocation products and the scopedlocation TSV are reconstructed
in a temporary directory and compared byte-for-byte; they are never rewritten.

The source's `GridMap` data fields, height dispatch, hole masks, grid constants,
`GridMapDefines.h`, DBC record structures, and DBC format declarations are taken
from guarded source. The harness supplies already-loaded pointer storage,
map ID zero, and complete authored DBC stores keyed by ID. Storage lookup is a
linear search of those complete rows, with genuinely missing IDs returning
null; area/type/status decisions run in original methods. The harness emits
raw terrain and WDBC bytes from the same numeric profiles used to populate
source storage. Rust consumes the emitted bytes through its actual decoders;
this oracle does not claim to test the original source's input parsers.

All queried terrain belongs to map zero, tile `(0,0)`, and is preloaded.
`GetGrid` executes its original tile calculation and selection. An attempted
load throws. No absent-grid source result is equated with consumer `NoWater`:
unloaded/missing inventory coverage is a separate consumer error contract.
Native VMap placement coordinates are unchanged; the test inventory declares
coverage for the matching terrain tile rather than inferring coverage from
the synthetic VMap tile filename.

The full original wet-WMO branch remains in the extracted composite method.
All geometry is dry. Its unimplemented wet-only `GetAreaFlag` and
`GetAreaEntryByAreaFlagAndMap` dependencies throw if reached, and the harness
asserts that there were zero such calls. Their presence is not wet-branch
parity evidence.

## Corpus and raw products

[terrain-liquid.tsv](fixtures/terrain-liquid.tsv) contains 5,048 source queries:
1,725 accepted results, 857 nonzero raw GridMap results suppressed by the
composite floor comparison, 2,998 actual dry-floor overwrites of a different
static height, and 1,463 location misses. Every source status occurs. These are
observed source counts, not proof of broader coverage.

[terrain-liquid/](fixtures/terrain-liquid) contains 29 authored native MAPS
products, `0.map` through `28.map`, and complete `LiquidType.dbc` and
`AreaTable.dbc` files. They contain numeric test data only, not installation
assets or extracted game rows. A profile ID selects the corresponding map.

- Profiles 0–3 distinguish water level 5, level 4 equal to the location floor,
  and the adjacent binary32 levels below and above 4
- Profiles 4–8 vary raw ground independently of water, location floor, and
  selected static height, including the raw `z == ground - 2` boundary
- Profiles 9–13 distinguish early dry flags, absent LiquidType entries,
  direct AREA override, one-parent fallback, and a real ID-zero liquid row
- Profile 14 uses a nonsquare liquid rectangle with unequal X/Y offsets
- Profile 15 uses original float-height dispatch and all-set terrain holes;
  profile 16 uses the same float grid without holes; profile 17 proves that
  original flat-height dispatch ignores those same holes
- Profiles 18–19 distinguish equal zero surface/ground suppression and the
  original `INVALID_HEIGHT_VALUE` raw ground
- Profiles 20–24 cover entry zero, missing AREA, missing parent, missing
  nonzero override, one-parent-only lookup, and a cyclic AREA relationship
- Profile 25 has no MLIQ section; profile 26 has a nonsquare liquid height
  array; profile 27 has per-cell AREA, MLIQ entry, and MLIQ flag tables
- Profile 28 provides level 8 above the location-miss static surface 7

Query families cover requested masks `0,1,2,4,8,15,16,32,63`; collision heights
0, 2, and 2.03128; adjacent binary32 status boundaries; null output; transformed
and scaled placements; gaps and bounds; location above/below-floor behavior;
and actual no-location fallback to selected static height. Every query stays
inside the authored loaded terrain tile.

The first profile's `floor4_static6_water5` row is independently asserted by
the C++ harness: world `(0x46855555,0x46855555,5)` converts to internal `(0,0,5)`,
original static height is 6, original dry location floor is 4, raw water level
5 survives, status is `WATER_WALK`, and the returned raw depth remains 0.
The composite comparison is strictly `map_data.level > ground_level`; it does
not replace the returned raw terrain depth with either VMap floor.

## TSV contract

The header declares 25 tab-separated columns:

```text
profile case x y z collision mask wants_data status entry type_flags level depth
location_hit location_floor static_height effective_floor raw_status raw_entry
raw_flags raw_level raw_depth raw_height tile_x tile_y
```

Float columns are eight hexadecimal binary32 digits. Other numbers are decimal.
`effective_floor` is selected static height after the original dry
`GetLiquidLevel` call. `raw_*` values are from a separate original GridMap
query over the same data. A location miss retains the original location
object's negative-infinity diagnostic sentinel; it is not a returned floor.

Output starts with entry `0xdeadbeef`, flags `0xdecafbad`, level bits
`0x4a123456`, and depth bits `0xca654321`. Source `NO_WATER` preserves those
sentinels; `wants_data=0` preserves them even for a nonzero status. The consumer
may represent absent output as `None`; null-output rows compare status only.
The harness asserts every nonzero non-null composite result preserves the
entire raw GridMap data object and every other output preserves all sentinels.

The compiler profile is Linux x86-64, C++17, `-O0`, no fast math, no contraction,
no strict aliasing, IEEE binary32, little endian, and round-to-nearest.
Cross-platform execution and actual installation validation remain separate.
Original CMaNGOS and G3D grants remain in the
[fixture licensing record](fixtures/vmap_source/README.md). Authored harness
and generator are GPL-3.0-or-later.
