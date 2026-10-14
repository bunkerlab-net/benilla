# Private native geometry export and admission contract

This is the concrete next exporter/consumer boundary, not a claim that the exporter,
VMap/MMap authority, or production gameplay exists. The current provider can inspect
strict dry ADT/M2/WMO sources and exact live client-static hulls. WMO roots require every
declared group and MODD M2 dependency; renderer-recovered failures do not certify coverage.
Sloped support, liquids, server masks, near-point selection and full navigation remain
unsupported; flat corridors can cross chunk seams and pass unrelated nonplanar cells; the strict WMO subset is detailed in [the geometry boundary](README.md).
The content compiler's non-render participant audit is an independent required input.

## Local source and output boundary

Only read a user's own compatible 1.12.1 installation. Keep the installation read-only;
put tools, scratch, logs and generated output outside it under private `benilla-config`
state. Do not commit or redistribute original members, extracted DBCs, generated
geometry or database captures. Authored synthetic format/collider fixtures belong in
repository tests; no game assets are needed for those tests.

Create fresh content-addressed staging for each source/configuration revision. Never
reuse an earlier output directory as proof of regeneration: the pinned MMap builder's
`shouldSkipTile` checks only format versions, not changed source/config/offmesh hashes.
The exporter must enumerate expected products and validate each one. A successful
process exit is insufficient because some tile failures are logged from void methods.

## Pinned extractor tools

Use CMaNGOS Classic commit
[`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`](https://github.com/cmangos/mangos-classic/tree/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f),
not a moving branch. Its real targets are `ad`, `vmap_extractor`, `vmap_assembler` and
`MoveMapGen`; old README command names/options can be stale. Build with
`BUILD_EXTRACTORS=ON`; server targets may be off. This pin requires CMake >=3.16,
C++20, Boost >=1.70, the configured SQL client library and OpenSSL >=3 on Unix.
libmpq, zlib, G3D and Recast/Detour are integrated dependencies; BZip2 has a bundled
fallback. The top-level build excludes extractor targets when the processor matches
`^arm`; ARM extractor support is not established.

Proposed local sequence, not executed:

```sh
ad -i CLIENT_ROOT -o STAGE -e 3 -f 0
vmap_extractor -d CLIENT_ROOT/Data -o STAGE -s
vmap_assembler STAGE/Buildings STAGE/vmaps
MoveMapGen MAP_ID --workdir STAGE --configInputPath PINNED_CONFIG \
  --offMeshInput PINNED_OFFMESH --threads N --silent
```

Create required output directories first. Use absolute paths for pinned configuration
and offmesh input. `-f 0` is the explicit floating-height profile and `-s` the standard
small VMap profile; `-l` is a different, separately reviewed profile. A single tile
uses `--tile X,Y`, with all required neighboring input tiles present. Generator skip
options are bare flags. `--buildGameObjects` additionally generates a hard-coded
transport model list; it is not a complete dynamic-object audit.

Sources: [CMake configuration](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/CMakeLists.txt),
[generator options](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/contrib/mmap/src/generator.cpp),
[tile generation and reuse](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/contrib/mmap/src/MapBuilder.cpp).

## Manifest and compatibility envelope

Record raw SHA-256, canonical names, supplying archive, sizes and dependency edges for:

- Original archive inventory and exact patch precedence, plus every consumed
  WDT/ADT/M2/WMO/DBC member
- Extractor source pin, binary hashes, flags, effective map/tile configuration, and raw
  config/offmesh bytes
- All expected generated files and their map/tile identities, lengths and hashes
- Build platform/endianness, relevant structure sizes, compiler configuration and
  native wrapper version

Prove that `ad`, the VMap extractor and Benilla resolve the same effective patch-chain
members; their archive enumeration is not identical by construction. Hashing only
filtered renderer records or normalized JSON cannot establish that identity.

Expected private products include `maps/*.map`, `dbc/*.dbc`, `Buildings/dir_bin`, raw
model files, `temp_gameobject_models`, `vmaps/MMM.vmtree`, `MMM_XX_YY.vmtile`, `*.vmo`,
`mmaps/MMM.mmap`, `MMMXXYY.mmtile` and optional `goDDDD.mmtile`.

The exact format envelope is:

- Terrain `MAPS` / `z1.4`
- Raw VMap `VMAPs05`; assembled VMap `VMAP_7.0`
- MMap magic `0x4d4d4150`, version 8
- Detour `DT_NAVMESH_VERSION` 7 with `DT_POLYREF64` enabled
- Vendored `dep/recastnavigation` Git tree
  `9694f7477cd03d3838322160833b2c995f6e5686`

Do not substitute a current Detour release. Versions alone do not bind ABI or generator
behavior. `.mmap` contains raw `dtNavMeshParams`; `.mmtile` begins with five uint32
fields before the Detour payload. Consumers validate inventory, framing, bounds,
references, versions, lengths and actual load results before publishing a revision.

Sources: [MMap definitions](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/MotionGenerators/MoveMapSharedDefines.h),
[VMap definitions](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/vmap/VMapDefinitions.h),
[M2 collision extraction](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/contrib/vmap_extractor/vmapextract/model.cpp).

## Independent participant audit

Data extraction cannot prove absence of non-render collision participants.
`GameObjectDisplayInfo` describes models, not active placements or runtime state.
The compiler must emit a complete content/map-bound audit, with dependency closure,
of relevant database spawns/templates, events/pools/conditions, doors, transports,
linked objects, scripts, spell summons and runtime creation/relocation paths.
Unsupported rows and missing source tables are explicit failures, never empty sets.

A production constructor for the participant proof should consume this validated
compiler result and the controller's closed capability set, rather than caller booleans,
counts, digests, or an empty rendered query. For an absence-based subset, every audited
source must prove no collision participant can exist in the admitted region and all
creation paths must be disabled by the active controller. For a nonempty subset, the
complete stateful geometry registry must be implemented first. Content/state changes
invalidate the proof before another authority query.

Upstream LOS combines static VMap and a dynamic tree. Door state, spawn state,
server-only/type flags, relocation and display/model changes affect collision. Thus
this audit cannot be replaced with the ADT/M2/WMO proof.

Sources: [Map LOS](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Maps/Map.cpp#L2668-L2699),
[GameObject collision state](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/GameObject.cpp#L1317-L1334).

## Consumer and path semantics

Prefer a bounded wrapper over this exact native implementation, or a versioned port
with differential fixtures. Keep it client-local; bind every answer to the current
world/profile/content/geometry revision, map, request and runtime snapshot. Unloading
or replacing any input invalidates the relevant snapshot.

`PathFinder` is owner-dependent. It has destination-only and explicit start/destination
`calculate` overloads; transports change coordinate spaces/meshes, and Detour query
coordinates use `{y,z,x}`. A `true` result is not verified movement. Missing meshes or
tiles, ignore-pathfinding state and forced-destination fallback can return
`NORMAL|NOT_USING_PATH` shortcuts. Inspect every `PathType` bit and requested/actual
endpoints, and require independently verified tile coverage. Missing inputs mean
unavailable, never unreachable or permission for a direct segment.

`setPathLengthLimit` is a point-limit heuristic, not a traveled-distance budget.
Keep cumulative player polyline budgeting independently enforced. Playerbot compilation
also changes limits and traversal filters and must be bound in the profile.

Still separate unsupported capabilities until source-port/differential validation:
near-point angle/distance/normalization/height/alternate-LOS rules; MOD_M2/ignoreM2 masks;
liquid classification and allowed-Z; swim/fly/water-walk; path smoothing/straight modes,
partial routes/offmesh links/transports; dynamic-object lifecycle; source collision
height/scale and owner movement state. A successful import opens none of these by itself.

Source: [PathFinder interface](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/MotionGenerators/PathFinder.h),
[PathFinder implementation](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/MotionGenerators/PathFinder.cpp).

## Why height interpolation does not unlock slopes

The present authority proves a horizontal area and sweeps an upright enclosing box.
Its `LocalCorridor::certify` excludes the one source-verified terrain root from the
static-hull tests only after proving that entire supporting area is flat. Removing
that proof would ignore intervening terrain: an uphill ramp also intersects the box's
flat underside, so merely accepting a new endpoint Z would not describe a supported
body trajectory. `core::player_path` and the NPC solver independently retain equal-Z
requirements. The cumulative per-frame distance and work budgets are unchanged.

The existing client mover is a different mechanism: `player/mover.rs` classifies a
capsule contact, slides, rides/pops obstacles, then snaps under bounded reach. Its
`player/state.rs` constants distinguish the 50-degree walkable normal, a separate
step/fall ratio, skin/slack and player/creature rise budgets. A native `Body` only
carries source-resolved radius and height; it does not contain those mover states.
Copying one slope number into the geometry authority would not port that mechanism
or the source server's navigation.

Pinned `GridMap::getHeightFromFloat` selects one of the four center-fan triangles
and interpolates height, with a hole rejection. This is a point query over extracted
`.map` heights. `TerrainInfo::GetHeightStatic` selects between terrain and VMap height,
including vertical-search fallbacks; `Map::GetHeight` additionally incorporates the
dynamic tree. Raw ADT interpolation alone cannot reproduce that composite query.
[GridMap source](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Maps/GridMap.cpp),
[Map source](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Maps/Map.cpp).

`WorldObject::UpdateGroundPositionZ` adds 0.05 after a valid height; its allowed-Z
counterpart does not. Near-point selection normalizes XY, adjusts height and tests
vertical separation/LOS while considering candidate angles. Those endpoint rules
are not a proof of traversability over the interval.
[Object source](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Object.cpp).

The MMap builder uses effective map/tile configuration for slope, climb, clearance,
radius, rasterization and erosion. Its extra 50-degree steep-area classification
is separate from the configured walkability ceiling. The generated navigation and
owner-dependent filters cannot be reconstructed by checking ADT face normals alone.
[MapBuilder source](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/contrib/mmap/src/MapBuilder.cpp).

## Minimal faithful next adapter

The [generated-product prerequisite](products/PROVENANCE.md) implements bounded immutable
inventory, native serialization framing and an oracle-checked dry GridMap point reader.
The optional [safe static VMap port](products/vmap/PROVENANCE.md) now loads validated
hierarchies/models and supplies bounded source-derived nearest rays as an additional
provider veto. Detour internal references/loading remain unimplemented; caller-provided
source/tool hashes are consistency declarations, not extraction or participant proofs.
The rest of this adapter remains an implementation contract, not a product availability claim:

1. **Private native snapshot.** Materialize and validate the exact generated products
   and source manifest above. For the first dry exterior route, load the map parameters,
   every navigation tile touched by its polygon corridor, the terrain height tiles,
   and the VMap map tree/tile/model closure. A rectangular guard or two loaded endpoints
   cannot prove all intermediate polygon references. Keep the existing raw-source
   inventory and immutable source-epoch binding alongside these native products.
2. **Pinned native core.** Build a bounded GPL-covered wrapper or differential-tested
   port over the pinned Detour, VMap manager/model tree, `GridMap`/terrain height and
   `PathFinder`. Preserve upstream notices for reused code. A current generic Detour
   binding or standalone height-formula crate is insufficient. Record `DT_POLYREF64`,
   build flags and effective path-normalization/optimization configuration in the
   policy digest. Prefer an isolated native worker so failed loads never publish a
   partially admitted snapshot.
3. **Actual owner context.** Supply audited map/instance, position, type, collision
   width/height, walk/swim/fly capability, water/hover/levitation state, pathfinding
   eligibility and configuration. Initially reject transports, liquids and unsupported
   offmesh links rather than substituting default state. `ENABLE_PLAYERBOTS` changes
   filters/limits and enables ownerless overloads; do not enable it just to avoid the
   normal owner contract. Height normalization still requires the real allowed-Z path.
4. **Whole-route output.** Return typed status, path-type bits, polygon references,
   requested/actual endpoints and the complete point route. Reject shortcuts,
   `NOT_USING_PATH`, partial/incomplete/truncated routes and missing/invalid references
   as unavailable. Feed every accepted route segment through source-bound continuous
   coverage and collision checks; define the sloped body-support mechanism first.
   Keep swept-body/LOS and independent non-render participant proof mandatory.
5. **Current consumer connection.** Add the native snapshot to the existing geometry
   service, with request/map/content/revision binding and the existing absolute deadline,
   queue and render-frame budget. Replace neither `NonRenderParticipants` nor the full
   model inventory. A route must be budgeted as its whole polyline, not its endpoint
   chord; the current player endpoint-only request cannot attest an unreported curved
   trajectory. Extend that contract and its deterministic validator before accepting it.
6. **Oracle before admission.** Author synthetic ramp, ridge, bowl, cliff, hole, seam,
   stacked-floor and narrow-clearance fixtures. Run the pinned native implementation
   as a differential oracle for height selection, filters, full path/status and source
   replacement. Corrupt/missing intermediate tiles must fail even with valid endpoints.
   Then validate on the user's lawful local starter inputs without copying them into
   the repository. Full Bevy/runtime, real-data and live visual gates remain separate.

The specific blocker is the absence of this source-bound native snapshot and its
owner/body/path consumer, not a missing interpolation formula. No production admission
or VMap/MMap parity follows from the flat-cell coverage extension.
