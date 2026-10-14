# Generated-product inventory and dry GridMap point reader

This is a source-bound prerequisite, not native geometry admission. `InventoriedProducts`
owns a bounded exact declared file set, validates hashes and serialization framing, and
contains decoded dry terrain. It cannot construct `Session`, `NonRenderParticipants`, a
path, an LOS result, or a body-support certificate. No existing geometry query consumes it.

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

- `src/game/Maps/GridMap.cpp`: `52c6d22fbe9f275196a44bf4f9b39da527071e3c81f7460a4ac03a3c9107b85d`
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
implementation or generic replacement library is linked here.

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
adds at most 64 × 132,100 bytes plus fixed hole arrays. Filesystem reads stop at the
expected length plus one byte; no streaming native engine or lazy input handle remains.
The inventory always requires terrain, VMap tile and MMap tile products for every declared
tile plus the VMap tree and MMap parameters. Direct VMap spawn model names must match the
declared model dependencies exactly. Generation-neighbor edges remain caller evidence;
this list does not prove all polygons of any route have been covered.

## Exact implemented subset

- `MAPS` / `z1.4`: exact 40-byte header and contiguous AREA/MHGT/hole sections from the
  pinned `ad -f 0` writer; flat or float heights only; finite heights bounded to ±1,000,000;
  unknown flags, packed heights, liquids, offset aliases, gaps and extra bytes rejected
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

The next concrete step is a separately bounded isolated native loader over the exact
vendored Detour tree and VMap implementation. It must validate all opaque counts,
indices/references and allocation/work budgets before calling native loaders, then
check actual per-product load results and complete model/navmesh closure before exposing
any loaded-core handle. Only then can an owner-dependent PathFinder adapter and full-route
consumer be developed. Stacked-floor/narrow-clearance/liquid/body/LOS and complete-route
oracles, live immutable source binding, participant admission and real local Data remain
mandatory and independent. Point interpolation opens no existing slope or movement gate.
