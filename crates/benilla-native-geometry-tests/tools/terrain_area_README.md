# Loaded-byte TerrainInfo area-selection source oracle

This is one bounded prerequisite for composite liquid selection. It closes the
loaded-static-area chain from raw VMap area metadata through terrain occlusion,
WMO/AREA flag selection, and final AreaTable/Map resolution. It does **not**
establish wet `TerrainInfo::getLiquidStatus`, indoor/outdoor classification,
`GetAreaName`, native gameplay admission, installation authenticity, or playable
world integration. All numeric geometry, tables and terrain are authored; no
installation content or held native compiler is used.

Run offline against CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

```sh
python3 crates/benilla-native-geometry-tests/tools/terrain_area_generate.py /path/to/mangos-classic
python3 crates/benilla-native-geometry-tests/tools/terrain_area_generate.py /path/to/mangos-classic --check
python3 crates/benilla-native-geometry-tests/tools/terrain_area_generate.py /path/to/mangos-classic --check --sanitize
```

`--check` reconstructs and compares without rewriting fixtures. `--sanitize`
adds `float-cast-overflow` with recovery disabled; it is not a complete memory or
undefined-behavior sanitizer suite. Generation and both checks passed on Linux
x86-64 with C++17, little-endian IEEE binary32, round-to-nearest, no fast math and
no contraction. This standalone oracle invokes no Cargo build.

## Source execution and byte provenance

[terrain_area_generate.py](terrain_area_generate.py) retains the existing whole-
file VMap/G3D and location source guards and adds
[terrain_area_source_hashes.sha256](terrain_area_source_hashes.sha256) for the
original terrain and DBC implementations. Original method bodies and declarations
are extracted verbatim; entire original `BIH.cpp`, `WorldModel.cpp`,
`ModelInstance.cpp`, G3D translation units and `DBCFileLoader.cpp` are compiled.

Every result row executes:

1. `VMapManager2::getAreaInfo`, `StaticMapTree::getAreaInfo`, original BIH point
   traversal, `ModelInstance::intersectPoint` and original group/model intersection
2. `TerrainInfo::GetAreaInfo`, with original `GetGrid` and original terrain-height
   dispatch/interpolation/hole handling
3. `TerrainInfo::GetAreaFlag`, with the original exact-triple WMO index and
   original `GetAreaEntryByAreaID`
4. `GetAreaEntryByAreaFlagAndMap`, including original AreaTable numeric-ID search
   and Map linked-zone fallback
5. For a nonzero resolved parent ID, original `GetAreaEntryByAreaID` once; this is
   a parent-row projection, not a recursive zone resolution or liquid override

The source oracle rereads all authored bytes used by those calls:

- Original `StaticMapTree::InitMap` and `LoadMapTile` read the authored native tree
  and placement tile. Original `BIH::readFromFile`, `ModelSpawn::readFromFile`,
  `WorldModel::readFile`, group readers and `readChunk` run unchanged
- Original `GridMap::loadData`, section readers, constructor and destructor read
  each emitted `MAPSz1.4` file. Each successful load is marked fully loaded,
  matching the surrounding loader's lifecycle; original `GetGrid` then returns
  the real loaded object
- Original `DBCFileLoader` and unchanged `DBCStorage` load every complete WDBC
  file. The exact original AreaTable and WMO indexing loops from `LoadDBCStores`
  build the secondary indices. Source packed structs and format declarations are
  used, with their in-memory sizes checked against the real loader

The minimal manager adapter substitutes ownership/cache infrastructure only:
`acquireModelInstance` caches by model name and invokes original
`WorldModel::readFile`. Original map/tile loading sets placement/model flags.
The minimal `TerrainInfo` declaration owns already-loaded grid pointers;
unexpected lazy loading throws instead of returning a scripted grid/result.
The types-only platform stub declares integer aliases and the verified little-
endian profile; original byte-conversion code executes. Core logging is a bounded
adapter. No metadata, terrain height, area row, flag, or selection reply is
injected by a callback.

[scopedarea/forward](fixtures/vmap_source/scopedarea/forward) and
[scopedarea/reverse](fixtures/vmap_source/scopedarea/reverse) are reused unchanged.
The generator reconstructs and compares **54 existing products**, including the
prior raw-area, dry-location and wet-VMap products. They are never rewritten.
Every new query also compares the reread native VMap result bit-for-bit with the
independent original writer-side constructed geometry. Only the loaded-byte
result feeds the new closure expectation. The inherited liquid reconstruction is
an immutability check, not evidence for new wet TerrainInfo status behavior.

## New products and table variants

[terrain_area.tsv](fixtures/terrain_area.tsv) contains **28,464** result rows.
[terrain-area](fixtures/terrain-area) contains 20 MAPS files and six sets of three
WDBC files, all with `.test-fixture` suffixes. AreaTable has all **25** raw fields,
Map all **42**, and WMOAreaTable all **20**, including valid string tables and
ignored nonzero fields. Physical rows are deliberately unsorted; the original
loader's numeric ID index and original secondary-index loops determine order.

Each `store0` through `store5` directory contains all three DBCs:

- 0: AreaTable ID 0 absent; Map 0 links to AreaTable 500
- 1: AreaTable ID 0 present on map 0; Map 0 links to 500
- 2: AreaTable ID 0 absent; Map 0 absent, while other map rows remain present
- 3: AreaTable ID 0 absent; Map 0 links to missing AreaTable 999
- 4: AreaTable ID 0 present on map 0; Map 0 links to 0
- 5: AreaTable ID 0 absent; Map 0 links to 0

Area 500 intentionally belongs to another map: linked-zone resolution is an ID
lookup and does not apply another map filter. Parent rows include a missing ID,
a cycle, a different-map row and a parent with its own parent; projection does
not recurse.

All query `map_id` values are 0. The VMap fixture tile filename remains
`000_32_32.vmtile`, as authored by the existing fixture. The actual terrain grid
coordinates are derived from world positions and appear in the final TSV
columns; they are normally `(0,0)`. A consumer places the MAPS profile at those
terrain coordinates rather than inferring its grid from the VMap tile filename.

## Terrain profiles and supported subset

The `profile` column selects `<profile>.map.test-fixture`:

- 0: flat 0, AREA flag 1000
- 1–3: flat 6, its previous float, and its next float
- 4–6: flat 9, its previous float, and its next float
- 7: flat source `INVALID_HEIGHT_VALUE`
- 8: float height 7 with every hole bit set
- 9: float height 7 without holes
- 10: flat height 7 with every hole bit set; source flat dispatch ignores holes
- 11–14: flat 0 with AREA flags 0, 1001, unmatched 1002, and 65535
- 15: all 256 AREA cells cycle through 1000, 1001, 1002, 0, 65535, 9100, 4464, 9002
- 16: source-only absent MHGT section, preserving constructor invalid height
- 17–18: source-only uint8/uint16 height 7 with every hole bit set
- 19: nonconstant float-height center-fan interpolation and the AREA cell table

Profiles **16, 17 and 18** are intentionally outside the private production
TerrainTile format profile. Each contributes 1,008 source observations. They
must remain explicit unsupported/rejection evidence; do not silently count them
as supported Rust parity or relax existing byte-reader policy. The remaining
**25,440** rows are in the existing terrain profile. Every recorded query has an
actual loaded MAPS grid. **No source-nullgrid case is recorded**, and an absent
inventory tile is never substituted with the source's null-grid fallback.

## Selection distinctions exercised

- The raw overlapping query combines highest-floor/ADT from one instance with
  last-successful root/group/flags from another. Reversing map traversal changes
  metadata independently of the highest floor
- Duplicate exact WMO triples select the **last eligible same-map row in numeric
  WMO ID order**, independently of physical WDBC record order
- A wrong-map WMO area, missing area, or absent area ID 0 does not invoke a
  `groupId == -1` fallback. Matching `-1` entries are deliberately present
- WMO area ID 0 still executes AreaTable lookup. When the same-map zero row is
  present, it can become the selected area; an absent zero row leaves a prior
  valid area intact
- Selected `exploreFlag` 70000 narrows to 4464 and resolves a different AreaTable
  row. Selected flag 65536 narrows to zero; zero skips all flag matches and uses
  Map linked-zone resolution
- Final flag resolution chooses the **first same-map row in numeric AreaTable
  ID order**, or the **last wrong-map row** if no same-map row exists. Only a
  complete miss, including flag zero, consults Map linked zone
- Map 0 missing, its linked row missing, and linked ID 0 present/absent remain
  distinct outcomes
- Original terrain rejection uses both strict comparisons:
  `z + 2.0f > terrain_height && terrain_height > raw_vmap_floor`. Exact and adjacent
  binary32 boundaries, flat/float holes, invalid height, and no-hit paths run
- M2-only points, gaps, transformed/scaled WMOs, signed/high-bit IDs, negative
  zero, every AREA cell and neighboring world floats around AREA cell edges run

The full source table has 10,152 raw hits, 7,116 accepted area-info hits,
3,036 terrain-occluded raw hits, 18,312 raw misses and 2,097 unresolved final
areas. These full-table counts include the clearly identified unsupported
terrain profiles.

## TSV contract

The header names all 32 columns. `dataset` selects the unchanged native fixture;
`store` selects a DBC directory and `profile` the MAPS file. Coordinate/floor/
terrain-height columns are eight hexadecimal binary32 digits. Other numeric
columns are decimal; signed native root/group patterns are widened as uint32.
Presence columns distinguish a genuine resolved row ID 0 from absence. A missing
resolved row writes zero filler fields, which have meaning only when presence is
true. Parent presence means one existing row at the resolved area's nonzero
`zone` ID, not a recursively determined zone.

Raw misses retain query Z and caller metadata sentinels. A terrain rejection
retains the raw metadata output arguments in the original source, but its boolean
is false; consumers must not treat rejected metadata as a selected WMO area.
The optional original `isOutdoors` and `wmoGroupId` outputs are not requested.
No source-loop overflow case is executed: AreaTable IDs that would wrap the
original inclusive uint32 scan are outside this bounded fixture domain.

Original CMaNGOS/G3D notices remain in the
[existing fixture licensing record](fixtures/vmap_source/README.md), and the
original extracted/compiled source retains its notices. This authored generator
and harness are GPL-3.0-or-later.

The same hash-guarded source harness also executes the original
`TerrainInfo::GetZoneAndAreaId` and `TerrainManager::GetZoneAndAreaIdByAreaFlag`
(`GridMap.cpp:1049–1052,1415–1421`) for every recorded row. It verifies direct selected-row
zone/ID projection without a parent-row dependency; existing fixture bytes and TSV
columns are unchanged. The Rust loaded-byte comparison independently checks the new
location path against those same area rows, including missing entries and real ID zero.
