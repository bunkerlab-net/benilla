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
requires a map liquid level above its effective ground, including any location-floor mutation. A complete dry VMap
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
TerrainInfo-specific map-aware WMO area resolution and wet VMap liquid selection,
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
place. The separate dry-VMap composite query below combines these raw results
with source ground selection. Wet VMap water and cached player environment state
remain prerequisites to broader water coverage and a genuine accessibility answer.

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

## Loaded static TerrainInfo height prerequisite

`load_static_height` builds an immutable `StaticHeightSnapshot` from actual terrain
and a completely resolved dry VMap. It implements the pinned
`TerrainInfo::GetHeightStatic` branch with both `useVmaps` and VMap height calculation
enabled. That explicit profile is included in the policy digest; it is not evidence
that a running source configuration enabled height calculations. The source manager
constructor, world configuration fallback and distributed config have different
defaults, so production configuration admission remains independent.

Each query binds map, content, source epoch, full inventory/policy, ticket, exact
position and signed search-distance bits. Raw GridMap height is read from the
covered tile; a real float hole translates to source -200000, while a missing tile
returns an error. Source valid-height comparisons use the separate -100000 threshold.

VMap height uses the exact source world/internal coordinate conversion and a unit
vertical ray, downward for nonnegative search (including negative zero), upward for
negative search. It uses nearest intersection with M2 included, source strict
triangle endpoints and source miss/sentinel conversion. It never reconstructs a
height from the normalized segment API or its 4096-unit cap. One bounded work budget
and absolute deadline cover the entire composite selection.

The exact selector first searches from z+2 and extends the requested range to
z+2-mapHeight+1 when source requires. On invalid height it tries 10000 downward,
then conditionally searches upward, then conditionally searches from mapHeight+2
with the source default range 10. Final selection uses the original z, not z+2,
and source strict comparisons. The raw selected value is not a support certificate.

The admitted ray profile requires finite poses within ±100000 and absolute search
range ≤10000, including every source-derived probe. Consequently some bounded
external inputs still return Unsupported: for example z=20000 over map height7
requires an initial range above10000, and z=99999 produces a ray origin100001.
These probes are rejected without clamping, splitting, returning a no-height
sentinel or publishing a partial result. The source selector itself has no such
clamp; its arithmetic oracle also covers larger derived ranges. Unknown VMap slots,
unsupported WMO liquid products, expired budgets and malformed geometry remain
errors. No native capability, participant, movement or accessibility gate changes.

This is deliberately not `TerrainInfo::getLiquidStatus`. Even a dry VMap can change
its reference ground argument: `VMapManager2::GetLiquidLevel` writes the successful
`GetLocationInfo` floor before a dry group returns false. That location floor can
differ from GetHeightStatic's floor. The separate location query below supplies that source-selected dry floor.
The separate dry-VMap composite query below combines it with this static height
and the existing GridMap liquid result in the original branch order; actual VMap
LIQU handling remains unsupported. Surviving map liquid
retains raw GridMap depth data; it must not be silently replaced with a static
or location floor. Cached player water state and creature capabilities remain
separate prerequisites after those spatial operations.

Two distinct source comparisons cover this prerequisite. The
[loaded VMap height oracle](../../../../../../../benilla-native-geometry-tests/tools/scopedheight_README.md)
executes original VMapManager2/StaticMapTree height methods over authored native
geometry, verifies 1,018 exact height results and reproduces the existing native
product bytes unchanged. The
[static selection oracle](../../../../../../../benilla-native-geometry-tests/tools/generate_static_height_fixture.py)
executes the full original GetHeightStatic method and flat GridMap dispatch, with
242 explicitly scripted VMap-response vectors checking every ordered probe and
returned height bit. Those scripts prove selector arithmetic/order only, not real
geometry. Separate integration tests combine the actual decoded GridMap/VMap bytes
through the production snapshot for initial-down, 10000 fallback, upward and
near-map searches, map/VMap selection and a verified raw terrain hole. Failure
injection proves callback errors never become a selected height or later probe.

## Loaded dry VMap location floor prerequisite

`VmapSnapshot::query_location` follows the pinned `StaticMapTree::GetLocationInfo`
path over actual decoded native bytes. Its separate policy-bound request includes
map, content, source epoch, complete inventory, ticket and exact world-position
bits. The admitted external position is finite and within ±100000 on each axis;
outside that profile returns Unsupported without clamping. Every map primitive
must be resolved and every loaded model dry before this query can publish a result. A source-proven location miss is distinct from missing
geometry, malformed bytes, an unsupported profile or an exhausted deadline/budget.

The source BIH point traversal preserves inclusive bounds, strict gap tests and
left-before-right traversal when clips overlap. It excludes M2 instances and checks
instance and group containment before the group ray. `GroupModel::IsInsideObject`
starts that ray at `pos - 0.1f * down`, uses the original unbounded positive-infinity
search distance, and reports `distance - 0.1f`. The work bound and deadline, rather
than an invented distance clamp, bound this internal query. Public segment and
height limits are unaffected. Local groups compete by strictly smaller distance;
instances compete by strictly greater transformed world Z. Equal results retain
the first source traversal selection. Source matrix/vector expression order is
preserved when converting the selected point back to world Z.

The result identifies the selected spawn, ADT, root and group, retaining the actual
serialized ID/flag bits. These identities are not AreaTable resolution or an area
membership certificate. Nor does a location floor establish support, accessibility
or liquid. In particular, the source manager writes a successful location floor
before a dry group's liquid lookup returns false. Composite TerrainInfo liquid
must account for that mutation: using static height alone can suppress legitimate
raw GridMap water below a nearby ceiling. Surviving map liquid must retain its raw
GridMap depth data. The dry-VMap composite selector below accounts for those
facts. Wet VMap LIQU data, map-aware WMO area resolution, runtime player water state
and creature movement capabilities remain separate dependencies; native entry and participant admission stay closed.

The [original-source location oracle](../../../../../../../benilla-native-geometry-tests/tools/scopedlocation_README.md)
compiles hash-guarded original BIH, WorldModel, ModelInstance and G3D translation
units, with the complete original map/manager methods. Its 2,032 location queries
include 1,040 selected floors over both existing and newly authored native products;
492 point-traversal traces check ordered callbacks independently. The original dry
manager's false return, unchanged liquid level, and hit-only floor/type mutations
are verified alongside each location. The fixtures cover overlapping bounds,
nearest groups and equal-height traversal ties, rotated/nonunit-scaled instances,
M2 exclusion, empty groups, gaps, exact boundaries and the 0.1 local-unit offset.
Full-width metadata bits are preserved. Source binary32 rounding is intentional:
a nominal floor can differ by one ULP after offset subtraction and back-transform.

A separate product integration test reads the same actual thick-WMO bytes together
with flat GridMap terrain: at the authored point z=5, static z+2 height is 6 while
the selected location floor is 4. This proves the two real spatial facts needed by
the separate composite selection below; this particular test asserts only the two
spatial facts rather than a water result.
Missing native tile slots, nonempty LIQU, stale identities and all work-budget cuts
remain errors rather than partial floor or absence publications.


## Bounded TerrainInfo liquid with complete dry VMap

`load_terrain_liquid` and `load_terrain_liquid_with_areas` build a separate immutable
`TerrainLiquidSnapshot` from actual inventoried terrain/VMap products and complete
LiquidType/optional AreaTable WDBC bytes. Its query is the pinned
`TerrainInfo::getLiquidStatus` under an explicit enabled-height, completely resolved
dry-VMap profile. Wet terrain is supported in that profile; nonempty WMO LIQU,
worldspawn, incomplete geometry and unsupported inventory domains remain rejected.
It is not a claim that ordinary continent products satisfy these bounds.

The query preserves the original three stages:

1. Compute `GetHeightStatic` with `useVmaps=true` and `DEFAULT_WATER_SEARCH=50`,
   retaining the original derived probes and raw invalid-height semantics.
2. Query actual WMO location. A selected floor replaces the reference ground even
   though dry `GetLiquidLevel` returns false; a location miss leaves static ground
   unchanged. No liquid level is fabricated from a floor.
3. Evaluate the raw GridMap liquid query with the exact requested mask, collision
   height and complete DBC stores. Keep its result only when it is not NoWater and
   its liquid level is strictly greater than the effective reference ground.

A surviving result is copied unchanged, including its raw GridMap depth, entry,
flags, level and status. Equal liquid/floor heights are suppressed by the outer
strict comparison even where GridMap's own non-strict ground comparison accepted
them. A suppressed result carries NoWater with no data, representing the source
method's untouched output rather than leaking its temporary GridMap data.

The raw GridMap call precedes the outer level filter. Missing AreaTable dependencies
cannot be hidden by predicting that the later ground comparison will suppress
water. Likewise, absent liquid does not bypass static-height computation or its
bounded derived-probe checks. Missing terrain is unavailable, never an invented
source no-grid NoWater result. Real terrain holes retain the raw source sentinel
and do not become missing coverage or support proof.

One absolute deadline and cumulative work allowance cover the entire query. The
request binds map, content, source epoch, complete inventory, the enabled-height
and location policies, complete DBC digests, ticket, exact position bits, requested
mask and collision-height bits. Replacing files or DBC bytes cannot relabel a prior
reply. An exhausted intermediate phase cannot publish a selected floor, partial
liquid answer or no-water shortcut.

This closes the dry-VMap composite spatial dependency only. Wet VMap LIQU and its
map-aware WMO area overrides, source-authenticated extraction/epoch lifecycle,
player cached water transitions/timers/restoration, creature movement capabilities,
Detour path/support and independent content/participant admission remain separate.
No composite reply is routed into runtime accessibility or character entry.

The [composite source oracle](../../../../../../../benilla-native-geometry-tests/tools/terrain_liquid_README.md)
executes complete original `TerrainInfo::getLiquidStatus`, `GetHeightStatic` and
`GetGrid` methods with original VMap geometry queries and GridMap liquid, AREA,
flat/float height and hole methods. Its 29 authored MAPS profiles and complete tiny
WDBC files are decoded as native bytes by the Rust comparison, alongside the exact
previously verified scopedlocation VMap products. No spatial query replies are
scripted. Wet-only WMO area helpers and unexpected grid loading throw if reached;
the oracle verifies zero wet-only calls and all queried grids already resident.

The 5,048 source cases include 1,725 surviving liquid results, 857 raw GridMap
results suppressed by the outer floor filter, 2,998 actual dry location-floor
overwrites, and 1,463 location misses. They span all five liquid statuses,
strict threshold neighbors, masks, area overrides, sampled liquid rectangles,
float terrain/holes, transformed WMO placements and source calls with a null
output pointer. The source asserts that suppressed/null-output calls leave its
output sentinels unchanged and surviving data copies raw GridMap data exactly.
The original floor4/static6/water5 case yields WaterWalk with raw depth0, directly
proving why substituting static ground or replacing the returned depth is wrong.


## Separate raw VMap liquid-level prerequisite

`VmapLiquidProducts::load` is an explicitly separate inventory reader for native
WMO LIQU bytes. It has its own inventory policy and exposes only
`load_vmap_liquid`, producing a privately owned `vmap::liquid::LiquidSnapshot`.
There is no conversion into `InventoriedProducts` and no public access to its
internal geometry. Existing inventory, static ray/height/location, dry liquid and TerrainInfo
dry-VMap composite readers retain their nonempty-LIQU rejection. Navigation
continues to rely on conservative inventory; an explicit private profile check
also prevents the opt-in inventory from feeding that reader internally. The
new wrapper exposes no navigation route and relaxes no admission gate.

The reader handles the pinned writer's real serialization quirk. A nonempty LIQU
header declares `20 + 4*(width+1)*(height+1) + width*height` bytes, but its payload
contains four additional bytes for the uint32 liquid type. The original
`WmoLiquid::GetFileSize` omits that word while `writeToFile` emits it. Both the
declared size and actual consumed framing are checked; neither the header length
nor an inferred empty liquid substitutes for validating all arrays. Checked
arithmetic, bounded element counts, finite bounded coordinates/heights and total
inventory/VMap byte budgets precede allocation and indexing. Raw liquid types
retain all uint32 bits; they are not narrowed to the separate LiquidType DBC profile.

The bound query implements `VMapManager2::GetLiquidLevel`, not TerrainInfo liquid
status. Exact loaded location selection chooses one spawn/group. The result
separates no location (source leaves all output references unchanged) from selected
location with floor/type and an optional level. Mask rejection, a dry selected
group, an outside rectangle or a disabled cell retain the selected floor/type but
no level. There is no fallback search for another wet group. The source mask table
uses Classic's literal bits: 1/41/61 water (0x08), 2 ocean (0x02), 3 magma
(0x01), 4/21 slime (0x04), all other types zero; an
unfiltered mask can still return a height for an unknown raw liquid type.

Local liquid coordinates use `533.333f/128.f`, deliberately distinct from the
GridMap grid-size constant. The low nibble equal to 0xF disables a cell; other flag
bits do not. Original binary32 arithmetic chooses the first triangle only when
`dx > dy`; equality uses the second. Corner Z does not affect local interpolation.
Instance conversion retains the original `(localLevel-localPose.z)*scale+worldZ`
formula even for tilted WMOs, rather than replacing it with a geometric rotation.
A returned raw level may be below the selected floor; TerrainInfo's wet checks are
not implemented by this result.

The source casts each floating tile coordinate to uint32 before its negative and
rectangle checks. The admitted query profile therefore excludes undefined C++
conversions: each reached coordinate must be finite, greater than -1 and less than
2^32. Out-of-domain values return Unsupported, never a fabricated absent level.
Safely fractional negative values truncate to zero and follow the original negative
exit. X exits precede evaluation of Y, and mask/dry exits precede interpolation;
an unexecuted conversion cannot create an error. External pose bounds remain finite
±100000, with complete map-slot resolution and one cumulative work/deadline budget.

Request/reply identity includes map, content, source epoch, the complete inventory
and query policies, ticket, exact world-position bits and requested mask. File
replacement cannot relabel an existing immutable reply. Missing geometry or any
budget/profile failure publishes neither partial floor/type nor a liquid result.

This provides raw static WMO liquid levels only. Existing dry readers still refuse
wet geometry, and the separate TerrainInfo composite remains dry-VMap-only. Actual
wet TerrainInfo status additionally needs map-aware WMO area selection and overrides;
source-authenticated extraction, player water-state transitions and creature movement
capabilities, path/support and participant/content admission remain unresolved.

The [wet VMap source oracle](../../../../../../../benilla-native-geometry-tests/tools/scopedliquid_README.md)
executes unchanged, hash-guarded pinned C++ over authored numeric geometry and
original serializers: 5,511 manager queries, 1,372 direct liquid/group pairs and
19 serialized-size/type-mask cases. Coverage includes all 256 masks, asymmetric
cells, both triangles, disabled cells, zero-cell grids, dry/wet occlusion, M2
exclusion, transformed/scaled instances and raw level/floor side effects. Normal
and float-cast-overflow-sanitized regeneration are byte-identical; deliberately
undefined conversions are tested only as Rust profile rejections, never oracle
answers. These fixtures contain no client installation assets.


## Separate raw VMap area-information prerequisite

The opt-in `VmapLiquidProducts::load_vmap_area` reader supplies bound raw
`VMapManager2::getAreaInfo` results from the same complete owned native VMap bytes.
It accepts validated dry or wet groups without exposing the conservative geometry
snapshot. This is a separate source query, not a conversion of the liquid/location
winner and not `TerrainInfo::GetAreaFlag`.

The pinned `WorldModel::IntersectPoint` selects the nearest intersecting group
within each model and immediately overwrites result/root/group/flags. Its caller,
`ModelInstance::intersectPoint`, then replaces ground and ADT only when the new
world-space ground is strictly higher. A later successful lower or equal-floor
instance can therefore supply root/group/flags while ground/ADT remain from an
earlier instance. The reader preserves that behavior and makes no coherent
winning-spawn assertion. Traversal order, group-distance ties, transformed local
rays, M2 exclusion and exact binary32 world-floor arithmetic remain source-defined.

An absent raw area result leaves the source output references unchanged. Complete
map-slot validation, one cumulative work/deadline budget and immutable geometry
ownership precede reply publication. Its distinct policy binds constituent geometry
and selection policies; map/content/source epoch, inventory bytes, ticket and exact
pose bits are carried by the query/reply identity. Errors publish no partial area
metadata and cannot mutate a previously owned snapshot.

This removes only the raw area-traversal dependency of wet TerrainInfo. That later
path must still apply the source terrain-between-point-and-WMO-floor rejection,
complete WMOAreaTable triple lookup, AreaTable map/explore-flag ordering and Map DBC
linked-zone fallback before liquid override selection. Missing product inventories
remain errors, never invented source fallback states. Existing conservative readers,
wet TerrainInfo availability, accessibility and production admission are unchanged.

The [raw area source oracle](../../../../../../../benilla-native-geometry-tests/tools/scopedarea_README.md)
executes the original manager, area callbacks, model/instance queries and BIH/G3D
code over authored numeric native products. Its 14,795 queries include 8,382 hits,
6,413 unchanged-output misses and 499 differences from location-winner metadata.
Forward/reverse fixtures change only map traversal order: unequal-floor overlaps
retain highest floor/ADT while metadata changes, and equal-floor overlaps retain
the first ADT but the last successful metadata. Fourteen dry/wet query pairs prove
that raw area selection does not evaluate liquid cells. Previously recorded source
products and answers remain byte-identical. No fixtures contain installation assets.


## Bound terrain/WMO area resolution

The separate terrain-area profile resolves the area needed by wet TerrainInfo
from actual inventoried native terrain/VMap products and complete AreaTable,
WMOAreaTable and Map WDBC bytes. It does not accept an authored area flag or
caller-selected DBC row. Existing conservative inventories and liquid profiles
retain their existing policies and admission limits.

`TerrainInfo::GetAreaInfo` first runs raw VMap area selection, retaining its
independent metadata/floor writes, then rejects it only when the original strict
comparison `z + 2 > rawMapHeight && rawMapHeight > vmapFloor` holds. This is raw
GridMap height, including its hole sentinel, not composite static height.
`GetAreaFlag` searches exact signed root/ADT/group triples in ascending WMO row-ID
order. The last referenced AreaTable row whose map matches supplies its explore
flag; otherwise the actual GridMap AREA field supplies the flag. There is no
wildcard group fallback. Source area ID zero remains a real lookup key when such
a row exists. The selected flag is narrowed to uint16 before the next lookup.

`GetAreaEntryByAreaFlagAndMap` scans nonzero flags in ascending AreaTable ID order:
the first same-map match wins, otherwise the last wrong-map match wins. Only when
no flag matches (including flag zero) does the actual Map row's linked-zone field
select an AreaTable row. Zero linked-zone remains a real row-zero lookup. No
missing supplied file is converted to an empty store, null GridMap or authored
fallback. A genuinely absent row in complete validated bytes remains absent.

All three complete WDBCs are framed and bounded before allocation; retained locale
string offsets, duplicate indexed IDs and supported source domains are checked.
This profile additionally rejects AreaTable ID `UINT32_MAX-1`: the source
max-ID-plus-one count makes its inclusive uint32 scan wrap forever. This restriction
does not change the existing raw GridMap area-override reader. The new snapshot
binds full DBC digests, native inventory, map/content/source epoch and constituent
geometry/query policies. The exact request pose and ticket remain
in its reply, and geometry traversal plus store work share one cumulative deadline
and work budget. Failures publish no partial resolution. These results establish
area selection only; wet liquid-status composition, player environment state,
movement capabilities and production admission remain separate prerequisites.

The [terrain-area source oracle](../../../../../../../benilla-native-geometry-tests/tools/terrain_area_README.md)
uses original native VMap readers, GridMap loading, DBCFileLoader/DBCStorage and
indexed-store construction, followed by the pinned area query bodies. Its authored
fixtures record 28,464 queries across six complete DBC store sets and both native
traversal orders. Of these, 25,440 use the admitted terrain encoding; 3,024 cover
source-only absent-height/uint8/uint16 encodings that remain explicitly unsupported
by the existing terrain reader. No missing inventory is treated as a source-null
grid. Coverage includes strict terrain rejection, duplicate triples, sparse/reversed
record IDs, wrong-map matches, row-zero and linked-zone fallbacks, uint16 truncation
and independently sourced overlapping-WMO metadata. All 54 prior source products
are regenerated byte-identically; fixtures contain only authored numeric data.
