# Generated-product inventory, GridMap points/liquids and optional static VMap loading

This is a source-bound prerequisite, not native geometry admission. `InventoriedProducts`
owns a bounded exact declared file set, validates hashes and serialization framing, and
contains decoded terrain. It cannot construct `Session`, `NonRenderParticipants`, a
path, an LOS result, or a body-support certificate. The optional
[source-derived VMap loader](vmap/PROVENANCE.md) separately validates actual hierarchy and
model references and supplies static-ray queries to the current provider as an extra veto.

## Source and license

Pinned source: [CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f](https://github.com/cmangos/mangos-classic/tree/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f).
The original [source notice](CMANGOS-NOTICE.txt) is preserved.
CMaNGOS Project contributors license the referenced code under GPL version 2 or, at the
recipient's option, any later version. Rust translations and the authored oracle harness
are 2026 Benilla fork contributors, GPL-3.0-or-later. The existing upstream
[AUTHORS](../../../../../../../benilla-sim/licenses/AUTHORS.md),
[COPYRIGHT](../../../../../../../benilla-sim/licenses/COPYRIGHT.md),
[GPLv2 grant](../../../../../../../benilla-sim/licenses/LICENSE) and applicable
[GPLv3 text](../../../../../../../../LICENSE-GPL-3.0) are retained with this source.
The combined app and headless harness already declare GPL-3.0-or-later package licenses.
No installation assets, database rows or generated game products are included.

Full-file SHA-256 for the point-height port and generated format layout:

- `src/game/Maps/GridMap.h`: `accbf9ecc6f436b952d3d38ee2e506fe3d331a7367c048745334160705760c4d`
- `src/game/Maps/GridMap.cpp`: `52c6d22fbe9f275196a44bf4f9b39da527071e3c81f7460a4ac03a3c9107b85d`
- `src/game/Server/DBCStructure.h`: `354013c7508d7891f19a1856cd7bbafda19f18388bc20671f5692efa57ebdf50`
- `src/shared/Database/DBCFileLoader.cpp`: `13f8c7c07c4644a7ec3c0c0a1fd32fc5a1775a6416f0c0ba3b1118b6466ea2fd`
- `src/game/Server/DBCStores.cpp`: `5f1bef967c9ad0ecc8779f679e6e734f068994979917323d507eb95663895109`
- `src/game/Server/DBCfmt.h`: `8232159a33cd63fe473457f19ae9343aaf2c81a0e9a0c3ba0c3294e1637358e4`
- `src/game/Maps/GridMapDefines.h`: `3a901f22b6d65edae74a93b5cd2be793ebc5c3884eb2c0aa06a37265ef53e54e`
- `src/game/Maps/GridDefines.h`: `5f5413657f84c5f5964f2a69d342874d8d1f7f5ed2bab969e1d78b0650141d82`
- `src/game/MotionGenerators/MoveMapSharedDefines.h`: `7994c9cbb53c81bc30b92b71201e17081040e5988c0e54d9425e0746dec7fd16`
- `dep/recastnavigation/Detour/Include/DetourNavMesh.h`: `26c4268830b800659798a3e652b3a5f89b0c7eaffc141306dec4fec960bec481`

The format framing follows that pin's `contrib/extractor/System.cpp`,
`contrib/mmap/src/MapBuilder.cpp`, `src/game/MotionGenerators/MoveMap.cpp` and
`src/game/vmap/{MapTree,ModelInstance,WorldModel,BIH}.cpp`. The VMap manager appends `.vmo`
to spawn model names. TerrainInfo grid coordinates become `MMMXXYY.map` and
`MMMXXYY.mmtile`, but `StaticMapTree::getTileFileName` produces `MMM_YY_XX.vmtile`.
Detour tile indices are origin-relative and are not assumed equal to either filename pair.

Detour's format declarations are by Mikko Mononen, copyright 2009-2010, under its zlib
license. The preserved notice is [DETOUR-NOTICE.txt](DETOUR-NOTICE.txt). No Detour native
implementation or generic replacement library is linked here. The optional static VMap
port also preserves the G3D notice, as documented in its [provenance](vmap/PROVENANCE.md).

## Consistency is not provenance authorization

`InventorySpec` is untrusted caller input. Its exact pin, ABI, tool binary hashes, raw
source/patch-chain manifest hash, source epoch, static inventory, content, effective
configuration, offmesh input and build-configuration digests are bound into a policy-v1
revision. Matching those declarations establishes neither that the tools ran nor that
all consumers resolved the same patch-chain members. A digest cannot freeze a mutable
installation. Audited extraction, epoch lifecycle and the independent participant audit
still have to be implemented and connected; no production proof constructor is added.

The loader is read-only. A future exporter must give it a curated private staging root
outside the installation, containing exactly the expected `maps/`, `vmaps/` and `mmaps/`
products. It rejects extra files/subdirectories, missing files, duplicate/case-aliased
names, invalid paths/IDs, symlinks, changed lengths/hashes and incomplete declared edges.
It owns all bytes before returning; later file replacement or deletion cannot mutate a
published value. Regular-file checks are hygiene, not a hostile-filesystem security
boundary. No claiming that source provenance was authenticated follows from this load.

Limits are checked before reads/decoding: one map ID 0..999, 1..64 declared grid tiles,
4,096 products, 16,384 dependency edges, 16 MiB/file and 64 MiB total input. Decoded terrain
adds at most 64 × (132,100 height bytes + 67,332 liquid bytes) plus fixed hole/header fields and optional 512-byte AREA arrays. Filesystem reads stop at the
expected length plus one byte; no streaming native engine or lazy input handle remains.
The inventory always requires terrain, VMap tile and MMap tile products for every declared
tile plus the VMap tree and MMap parameters. Direct VMap spawn model names must match the
declared model dependencies exactly. Generation-neighbor edges remain caller evidence;
this list does not prove all polygons of any route have been covered.

## Exact implemented subset

- `MAPS` / `z1.4`: exact 40-byte header and contiguous AREA/MHGT/optional MLIQ/hole sections from the
  pinned `ad -f 0` writer; flat or float heights only; finite heights bounded to ±1,000,000;
  unknown flags, packed heights, offset aliases, gaps and extra bytes rejected
- Point height: pinned binary32 expression order, tile ownership and four center-fan
  triangles; float mode checks the source hole mask. Pinned flat dispatch bypasses holes
  and is deliberately preserved. `Height(-100000)` remains a raw height in flat mode,
  whereas float holes return the typed `Hole` result. Neither is a support certificate
- `VMAP_7.0`: tiled exterior tree framing, bounded BIH byte extents, spawn framing and
  direct model-name closure, bounded model/group/vertex/triangle framing and finite
  transforms. Worldspawn flags, unknown spawn flags and liquid models reject. MOGP flags,
  BIH child/object references and cross-file spawn-node identities remain opaque; no
  collision mask or native model validity is certified
- MMap magic `0x4d4d4150`, version 8, Detour version 7: exact 28-byte parameters, 20-byte
  tile header and sized Detour sections for little-endian 64-bit pointers/polyrefs.
  `maxPolys` is the pinned writer's raw `0x80000000` (`1 << DT_POLY_BITS`, 31) value.
  Unknown header booleans, layers and offmesh connections reject. `usesLiquids=1` records
  the generator setting and does not itself indicate water in the tile. Internal polygon,
  portal, detail and BV references remain opaque; no `dtNavMesh::addTile` is called

Versions alone never establish ABI compatibility. The compatibility envelope records the
specific relevant native sizes; only the declared pinned profile is accepted. Because
all its accepted fields are constants, the inventory policy version binds those values.
A future native wrapper must independently verify its compiled sizes/flags and source.

## Oracle and next loading step

[The generator](../../../../../../../benilla-native-geometry-tests/tools/generate_terrain_fixture.py)
hash-checks the complete pinned `GridMap.cpp`, extracts the original `isHole`,
`getHeightFromFloat` and `getHeightFromFlat` bodies verbatim and compiles them with g++
C++17, FE_TONEAREST, no fast math and no FP contraction. It emits 6,272 authored vectors
for flat, ramp, ridge, bowl, cliff, center-fan and hole profiles across four non-symmetric
and boundary tiles. Cargo compares exact float bits without C++, downloads or game data.
This oracle covers point-height selection only; it does not test stacks, clearance,
VMap composite height, native pathfinding, or gameplay. The fixture is original numeric
output. Linux generation is verified; other runtime platforms remain separate gates.

The optional [safe static VMap loader/query port](vmap/PROVENANCE.md) now implements
hierarchy/reference validation and nearest static rays over immutable inventoried bytes.
This inventory's own envelope pass still does not perform those checks. The optional
[bounded native navigation reader](../../../../../../../benilla-native-nav/PROVENANCE.md)
validates actual dry MMap sections, polygon/detail/BV references, topology and load/query
budgets before the pinned Detour loader. Its bound nearest/corridor/funnel queries compose
as an additional provider veto. This loaded subset does not establish complete navmesh
coverage, an owner-dependent PathFinder adapter or full-route movement authority. Stacked-floor/narrow-clearance/liquid/body/LOS and complete-route
oracles, live immutable source binding, participant admission and real local Data remain
mandatory and independent. Point interpolation opens no existing slope or movement gate.

## Exact dry native liquid prerequisite

`InventoriedProducts::load_dry_liquid` derives a separate immutable
`DryLiquidSnapshot` from the actual decoded native products. It first loads the
existing dry VMap reader and requires every map-tree primitive slot to be resolved.
A successful ordinary ray load can leave unrelated slots unknown and is therefore
insufficient. The retained terrain tile set comes only from fully decoded
`TerrainTile` products and explicitly requires both liquid-section offset and size to
be zero. The general terrain decoder may accept wet products; this stronger dry
proof never treats a decoded liquid section as absent water.
No caller-provided dry flag, successful path, missing file, or renderer observation
can construct this result.

For the pinned source, these facts imply the exact `LIQUID_MAP_NO_WATER` branch:

- `GridMap.cpp:43–68,120–125` leaves a new grid's liquid pointers/flags empty when
  no liquid section is present
- `GridMap::getLiquidStatus`, `GridMap.cpp:544–548`, returns NO_WATER before any
  height, hole, area/DBC override or collision-height computation in that state
- Every accepted VMap group has no liquid payload; the source empty-group branch
  also has no liquid. `WorldModel.cpp:393–404` therefore cannot supply a liquid level
- `VMapManager2.cpp:288–305` cannot return a liquid level for any selected group
  in a fully resolved dry map. No location/floor winner needs to be approximated
- `TerrainInfo::getLiquidStatus`, `GridMap.cpp:1054–1127`, receives false from
  VMap and NO_WATER from the exact GridMap tile and retains its initial NO_WATER.
  Its preliminary composite-height number is irrelevant to that bounded result

A query binds map, content, source epoch, complete inventory, policy, ticket and
exact world-position bits. Grid selection preserves `TerrainInfo::GetGrid`'s
binary32 division/subtraction and truncation (`GridMap.cpp:1224–1226`), with invalid
coordinates rejected before indexing. Only covered tiles can answer; a wholly dry
VMap never makes a missing terrain tile dry. Deadline, malformed-point, stale
binding, unresolved-slot, missing/corrupt-file and liquid-payload failures return
errors. Even a liquid payload outside the requested point is unsupported in this
strict profile. Holes and unsupported walking geometry cannot change NO_WATER,
but this query grants no support, accessibility or navigation fact.

The [offline oracle](../../../../../../../benilla-native-geometry-tests/tools/generate_liquid_fixture.py)
hash-checks complete pinned GridMap.cpp, GridMapDefines.h and GridDefines.h,
extracts the source grid constants, compiles the complete
original `GridMap::getLiquidStatus` method and exact source grid-selector lines,
and checks 1,338 authored center/boundary/adjacent-float/height vectors. DBC and
height dependencies abort if reached, so they cannot supply invented values to
an accidentally non-dry branch. The harness establishes only the dry branch and
grid arithmetic; it is not a composite TerrainInfo or wet-liquid implementation.
Original GPL source notices are retained with generated temporary translations;
only authored numbers and the harness are committed. Run with `--check` to require
an exact fixture match; a missing compiler is an error, never a silent skip.

This is a product-level prerequisite, not current player water state.
`Player::IsInWater` reads cached environment flags; `SetPosition` updates them only
when its 100-ms position-status timer permits. Liquid transition callbacks, prior
wet state and restoration history cannot be replaced by a fresh point query.
Native `NpcAccessibility` remains unavailable pending that state/lifecycle and
raw creature `InhabitType`/current movement-capability integration. Existing native
capability, participant and local extraction/source-epoch admission stay closed.
The immutable snapshot survives file changes under its old binding; it does not
establish a live source lease. Whole-map dry closure is deliberately restrictive
and may reject real continent products, including those exceeding existing caps.
No real client assets or playable-world claim accompanies this prerequisite.


## Bounded wet GridMap queries

`InventoriedProducts::load_grid_liquid` separately accepts complete `LiquidType.dbc`
WDBC bytes and constructs a borrowed immutable `GridLiquidSnapshot`. Full-file
DBC hashing joins the inventory/source epoch and policy binding. This proves only
consistency of the supplied bytes, not their extraction provenance or a live source
lease. There is no caller-authored liquid-row callback. The snapshot still requires
all static VMap slots to resolve and every model to be dry; unsupported WMO liquid,
worldspawn or incomplete static geometry cannot silently become a water answer.

This result is explicitly `GridMap::getLiquidStatus`, not the composite
`TerrainInfo::getLiquidStatus`. The latter separately selects VMap liquids and
requires a map liquid level above its composite static ground. A complete dry VMap
does not remove that ground comparison for wet GridMap results. The independent dry-navigation loader rejects wet terrain to preserve its previous
accepted domain. No snapshot here
is routed into player environment, accessibility, navigation or runtime admission.

The native reader decodes actual packed `MLIQ` headers, optional 16×16 entry/flag
arrays and optional bounded liquid-height rectangles, with source-swapped offset
axes and nearest height samples. Header lengths, flags, finite heights, rectangle
bounds and full section framing validate before allocation/indexing. Flat and float
terrain heights retain their existing exact source behavior. Missing liquid sections
are distinguished from noncovering rectangles and from missing terrain tiles.

`LiquidType.dbc` uses pinned `DBCfmt.h`'s `niii` layout and `DBCStructure.h`'s four
fields (ID, LiquidID, Type, SpellID). The accepted complete-file profile is bounded to
65,536 rows, IDs ≤65,535, Type 0..3 and a 1-MiB string block. Duplicate IDs and wrong
layout/framing reject. A genuinely absent row preserves the raw liquid entry and
flags exactly as source; a matched row preserves only DEEP_WATER and derives the
basic flag from Type. Matched IDs below 21 require `AreaTable` and possibly its parent zone. The
no-area loader still rejects this dependency; the separate complete-area loader
below resolves it from actual bytes. An omitted AreaTable is never interpreted
as an empty or no-override table.

The query preserves source order: DBC resolution, requested-mask filter, rectangle
coverage, liquid-versus-ground and ground-minus-two checks, then strict delta
thresholds `> collisionHeight`, `> 0`, and `> -1`. Above-water results still contain
liquid data; no-water results do not. Float terrain holes use the exact source
-200000 height sentinel for this raw query, while flat dispatch bypasses holes;
neither behavior establishes body support. Exact query pose, mask, collision-height bits,
ticket and snapshot binding are echoed; stale identities, invalid floats, missing
tiles, unsupported area dependencies and expired budgets return errors.

Remaining prerequisites include source-authenticated DBC extraction/epoch lifecycle,
TerrainInfo-specific map-aware area resolution, VMap liquid selection and composite static ground,
player environment caching/timers/transitions/restoration, creature movement
capabilities, and independent production/participant admission. This subset neither
loads a real continent nor establishes that current inventory caps cover one.

The [wet status oracle](../../../../../../../benilla-native-geometry-tests/tools/generate_wet_liquid_fixture.py)
hash-checks the original GridMap methods, declarations, constants and DBC layout,
then compiles the complete upstream status method and its real flat-height dispatch.
Its 2,953 authored numeric cases span ten explicitly described profiles, table/global
types, flat/sampled liquid heights, asymmetric rectangles, masks, absent DBC rows,
DEEP_WATER preservation, ground comparisons and adjacent-float status thresholds.
Area lookup aborts rather than supplying invented overrides. The Rust consumer
reconstructs actual MLIQ and complete WDBC bytes from the fixture profiles and
compares decoded query status/data bit-for-bit. Separate reader tests cover float
terrain holes and malformed inputs; this oracle's ground is explicitly flat.
Original notices are retained in temporary translations, and `--check` requires an
exact match with no silent missing-compiler skip. No upstream source or installation
products are generated into the repository.


## Exact GridMap AreaTable/zone overrides

`load_grid_liquid_with_areas` accepts complete actual `AreaTable.dbc` bytes alongside
`LiquidType.dbc`; this profile binds the entire additional file digest under a
separate policy domain. The original no-area loader retains its old binding and
unsupported behavior when a matched liquid ID below 21 needs AreaTable. Supplied
empty tables and omitted inputs are distinct; only a complete decoded table can
prove that a row is missing. Neither profile authenticates the supplied provenance.

The source `AreaTableEntryfmt` is `niiiixxxxxissssssssxixxxi`: 25 four-byte fields,
100-byte records. ID, parent zone and override are uint32 fields 0, 2 and 24.
The bounded parser preserves full-width keys/references, rejects duplicate IDs and
record ID UINT32_MAX (the source loader's max-ID increment overflows for that ID),
rejects wrong layouts, caps rows at 65,536 and strings at 1 MiB, and validates all eight
name offsets against a byte block with initial/final NULs without imposing UTF-8
or string-boundary assumptions. Parent/override references retain all uint32 values;
only record keys exclude the source-invalid maximum. All bytes, including unused fields and strings, affect the
policy digest. Duplicate rejection is narrower than the source's last-row-wins
index loader, not a change to the admitted source query semantics.

`TerrainTile` retains the actual AREA header value or 256 uint16 cell values.
Selection uses the pinned `GridMap::getArea` binary32 expression order at scale 16,
truncation and row-major masking. The liquid query then looks up that exact numeric
value as an AreaTable ID. It does not substitute exploreFlag matching, filter by map,
or reinterpret the extractor's AREA representation. This is the literal pinned
GridMap branch; TerrainInfo's VMap branch uses different area selection.

For an initial matched liquid ID below 21, an existing area supplies its override.
Only a zero override with nonzero parent requests one parent lookup. The resulting
override, including zero, is looked up in LiquidType. A found row replaces liquid
entry/type; a missing row keeps the initial selection. A missing initial area skips
override lookup, while a missing parent still permits lookup of override zero.
Nonzero but missing overrides suppress parent fallback. Parent cycles, grandparent
chains and overrides below 21 do not recurse. DEEP_WATER survives the initial type
resolution and any override, while other raw flags do not. Requested-mask and
rectangle/height/status checks retain their original ordering afterward.

This closes only the GridMap area dependency. Complete dry VMap loading, missing
slot rejection, immutable pose/map/content/epoch/inventory binding, deadlines, dry
navigation restrictions and all runtime/participant admission checks remain in
place. Composite ground/VMap water selection and cached player environment state
are still prerequisites to a genuine accessibility answer.

The [area override oracle](../../../../../../../benilla-native-geometry-tests/tools/generate_area_liquid_fixture.py)
extracts the complete pinned `getArea`, `getLiquidStatus` and `GetAreaEntryByAreaID`
bodies plus original flat-height dispatch, declarations, constants and DBC formats.
Seven full-file hashes guard the input. Its 1,931 authored cases cover global/cell
AREA selection, two tiles and adjacent-float cell boundaries, missing/present zero
rows, direct/one-parent overrides, full-width references, cycles/no-grandparent
semantics, masks, DEEP_WATER and status thresholds. Complete authored WDBC rows and
profile metadata are emitted beside the numbers; the Rust consumer reconstructs
actual native AREA/MLIQ and both DBC files before querying the reader. This tests
source branch parity rather than a copied Rust oracle, while retaining the same
explicit exclusion of composite TerrainInfo and cached player state.
