# Complete wet TerrainInfo liquid source oracle

`terrain_wet_liquid_generate.py` compiles the pinned CMaNGOS Classic revision
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f` and its original G3D dependencies. Run:

```sh
python3 crates/benilla-native-geometry-tests/tools/terrain_wet_liquid_generate.py /path/to/mangos-classic
python3 crates/benilla-native-geometry-tests/tools/terrain_wet_liquid_generate.py /path/to/mangos-classic --check
python3 crates/benilla-native-geometry-tests/tools/terrain_wet_liquid_generate.py /path/to/mangos-classic --check --sanitize
```

The existing `vmap_source_hashes.sha256`,
`scopedlocation_source_hashes.sha256`, and `terrain_area_source_hashes.sha256`
guard every compiled or extracted source file. The generator reuses the
hash-checked area generator's original readers, declarations and index-building
loops, then adds the complete, unchanged original methods:

- `TerrainInfo::getLiquidStatus`, `GetHeightStatic`, `GetGrid`, `GetAreaInfo`,
  `GetAreaFlag`, and their DBC helper lookups
- `GridMap::getLiquidStatus`, MAPS loading, flat/float heights, holes and AREA
- `VMapManager2::GetLiquidLevel`, `getHeight`, `getAreaInfo`, and the actual
  tree/group/model/instance traversal and geometry methods
- `DBCStorage` / `DBCFileLoader` loading of all four complete WDBC stores
- Native `StaticMapTree::InitMap`, `LoadMapTile`, and model file readers

Only logging and unrelated lazy-load/cache ownership infrastructure are
substituted. Every queried grid is loaded first, and a lazy-load attempt throws.
No selected floor, status, area, DBC lookup, liquid result, or query callback is
stubbed. New geometry is serialized by original writers and then reread by
original readers before it contributes expectations. The old geometry is
regenerated using its existing numeric construction. All **93** previously
checked-in VMap and TerrainInfo-area products are compared byte-for-byte and
never rewritten by this generator.

## Products and primary contract

[terrain-wet-liquid.tsv](fixtures/terrain-wet-liquid.tsv) has **27,566** rows:

- **27,306** data-present source calls; **25,836** are within the private
  terrain coordinate domain and compared for full status and output data
- **260** source-null-data calls; **235** are within that domain and compared
  for status only; the private Rust API
  intentionally represents data-present calls, not the source nullptr API

The remaining **1,495** source observations (**1,470** data-present and **25**
nullptr) lie just outside the terrain world's positive outer edge. Original
GetGrid truncates their negative fractional grid coordinate to grid 0; the
existing private `world_tile`/TerrainTile coordinate profile rejects them.
The Rust test explicitly asserts `OutsideTile` for every one. These are defined
source-only observations, not supported Rust parity, and no bound is relaxed.

The generator writes **111** new products: the TSV; **26** MAPS profiles;
**five sets of four WDBC stores**; and **64** native VMap tree/tile/model files.
MAPS and WDBC filenames end in `.test-fixture`. Native files retain the existing
approved `.vmtree`, `.vmtile`, and `.vmo` fixture suffixes. All are authored
numeric fixtures, without game install assets or native compiler products.

`dataset` is one of:

- `terrain-wet-vmap` or `terrain-wet-vmap/reverse`, under `fixtures/`
- `scopedliquid`, `scopedarea/forward`, or `scopedarea/reverse`, under the unchanged
  `fixtures/vmap_source/`

The source VMap tile file is `000_32_32.vmtile`. World-position-derived terrain
coordinates appear in the final `tile_x`/`tile_y` columns and include `(0,0)`,
`(1,0)`, `(3,0)`, and `(7,0)`. Rust test inventory identities place the actual
MAPS profile at those coordinates and materialize the complete original VMap
bytes through the existing private, hash-bound test loader.

## Stores and MAPS profiles

Each `terrain-wet-liquid/storeN/` contains complete `LiquidType`, `AreaTable`,
`Map`, and `WMOAreaTable` WDBC files, with original field counts, valid string
tables, nonzero ignored fields, and deliberately unsorted physical row order.
The original numeric ID index and secondary index-building loops decide lookup
order. LiquidType ID **65536** is present, proving lookup beyond a uint16 ID;
raw type **0x12345678** is absent. The original indexed DBC reader therefore
allocates only a bounded 65,537-entry index rather than a multi-gigabyte table.

Store variants:

- 0: LiquidType ID 0 and ID 1 present; ID 2 absent
- 1: ID 0 absent; ID 1 present; ID 2 absent
- 2: ID 0 present; ID 1 and ID 2 absent
- 3: ID 0, ID 1 and ID 2 absent; Map 0 linked-area target also absent
- 4: ID 1 has Type 32, but source queries execute it only where a valid area or
  parent override replaces Type 32 before any shift; other store contents match 0

Area rows include direct overrides, one-parent overrides, a missing parent,
a parent whose own parent must not be followed, a cycle, zero overrides,
nonzero missing overrides, wide Type 31, high ID 65536, and Type 30/Type 7. A parent
on another map is still selected by ID. WMO entries include numeric-order
selection, wrong-map rows, exact mixed tuples, and an exploreFlag that narrows
from 70000 to 4464 before final area resolution.

MAPS profiles 0–8 vary flat/float height, invalid height and hole behavior.
Profiles 9–17 choose AREA flags 1001–1009. Profiles 18/19 use zero/unmatched flags;
20 has no MLIQ section. Profiles 21/22 select raw GridMap ID 1 and its direct
or Type 31 override; 23/24 select Type 30/Type 7 wet overrides. Profile 25 selects
raw GridMap ID 1 with the high-ID 65536 override, preserving that uint32 entry
while narrowing Type 31 into uint8 flags with deep-water bit 16 retained.
Every byte profile stays within the existing supported flat/float MAPS domain;
the separately identified outer-edge query coordinates remain unsupported.

## Source ordering and edge witnesses

The complete source table, including those source-only outer-edge rows, contains:

- **12,107** successful raw VMap liquid queries
- **705** successful raw VMap queries rejected by the later level/floor or
  strict Z guard, suppressing raw GridMap fallback
- **9,557** raw GridMap fallbacks, preserving its original depth and type
- **54** otherwise-valid raw GridMap results filtered by the effective floor
- **2,939** raw VMap floor writes differing from initial static height
- **1,257** raw area hits rejected by original terrain occlusion
- **600** rows with independent location versus raw-area metadata selection
- **6,841** unchanged caller-data sentinel observations

The supported-domain Rust comparison includes **555** wet rejections and
**9,288** fallbacks. The following counts describe all source rows.

All five statuses are present: NoWater 6607, AboveWater 1533, WaterWalk 2235,
InWater 7461, UnderWater 9730. Exact and neighboring binary32 values around
`level - collisionHeight`, `level`, `level + 1`, and `floor - 2` are queried.
Collision heights 0, 2 and 2.03128 run. A real scale 32 WMO expands the original
0.1 model-space ray bias to 3.2 world units; this produces a successful raw wet
query at world Z exactly `floor - 2`, proving the later strict Z guard without
inventing a floor or bypassing geometry selection.

All **256** request masks run for raw IDs 0–21, 65536 and 0x12345678, and again for
selected missing-base/direct/parent/high-flag override combinations. The source
raw mask gate precedes area override, and no second mask test occurs after it.
Missing base LiquidType rows still permit wet ID 1–20 overrides. A surviving
missing/unknown base defaults to flag 1. Raw ID 0 and IDs>=21 skip wet override.
Liquid height equal/below/above the actual floor, dry groups, hole groups, dry
instances, M2 exclusion, transformed WMOs, and gaps run through real native data.
Forward/reversed original BIH leaf order changes the independent raw-area
metadata without replacing the selected highest liquid floor.

## TSV and sanitizer boundary

The header names all **54** columns. All coordinates, heights and collision
values are eight hexadecimal binary32 digits; IDs, statuses and flags are
unsigned decimal except `location_root`, which retains the original signed
int32 decimal representation and is compared as its uint32 bit pattern. Source data outputs begin as entry `deadbeef`, flags `decafbad`,
level `4a123456`, depth `ca654321`. Failure and nullptr calls leave those outputs
unchanged. The file includes independently executed initial static height,
raw VMap liquid, raw GridMap liquid, raw area, terrain-accepted area, resolved
AreaTable/parent rows and location metadata, so the Rust test compares both the
full result and each actual-byte component.

`--sanitize` enables `shift,float-cast-overflow` with nonrecovering failures.
Type 31 is defined in this C++17 source and yields uint32 `0x80000000` for the wet
path; GridMap subsequently narrows its type flags to uint8. A selected final
Type >=32 would be an undefined source shift and is never executed by this
oracle. The safe Type 32-replaced-before-shift cases do execute. Out-of-range or
nonfinite original float-to-integer casts are excluded. Private Rust rejection
tests cover invalid final shifts and nonfinite coordinates/collision heights;
existing raw VMap tests cover executed out-of-domain liquid casts. No undefined
source execution is offered as parity evidence.

The CMaNGOS/G3D notices remain with the extracted source and the existing
[fixture licensing record](fixtures/vmap_source/README.md). The authored harness
and generator are GPL-3.0-or-later. This is bounded, synthetic, data-present
source parity only: it enables no native compiler, product installation,
readiness, coverage, movement, gameplay, or external product admission.
