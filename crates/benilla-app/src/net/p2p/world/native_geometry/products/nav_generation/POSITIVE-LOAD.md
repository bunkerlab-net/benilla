# Positive single-tile source loading

`PositiveTileLoad` owns one fresh, isolated, successful load stage over genuine
immutable extraction and generation inputs. It is a prerequisite for future map
loading composition, not `TerrainInfo` construction, NGrid creation, object-data
loading, registration, selection, movement authority or production admission.

## Pinned source contract

CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`,
[`GridMap.cpp`](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Maps/GridMap.cpp)
(SHA256 `ecb78e94e3847c06f7f8a4ac339b3aa6124bfbca37cc0ed0e11c0934dca12d83`):

- `TerrainInfo::Load` increments the tile reference before inspecting the GridMap
  pointer. A present GridMap returns immediately without retrying VMap/MMap.
- With no GridMap, `LoadMapAndVMap` creates and loads it, then attempts VMap before
  MMap. The helper marks the GridMap fully loaded; the caller records the attempt.
- Source failures can retain partial state. A missing map file can even report a
  successful default/sentinel GridMap. Neither loaded flag proves useful terrain.
- `GetGrid` has different retry rules. Unload, lazy cleanup and construction's
  randomized cleanup timer are separate operations.

The selected VMap configuration is required, explicit and retained. Source map
loading is enabled when LOS **or** height calculation is enabled; this deliberately
narrow consumer supports only both enabled, matching its downstream resident
height and LOS queries. No default is chosen. Existing profile-bound MMap settings
remain mandatory in the native query loader.

## Actual allocation boundary

The constructor inspects the real terrain inventory and requires exactly one tile
matching the actual generated input/output. The generation result must be positive;
empty/omitted tiles and mixed sets are not admitted. Compatible source receipts and
freshness checks remain those of the existing consumers.

It stages a reference, executes the existing terrain decoder followed by the
selected VMap decoder, verifies their exact tile selections, and only then loads
the genuine generated MMap into the native query allocation. The existing adapter
also allocates its map-instance query; this is not a claim to reproduce the entire
CMaNGOS manager or query-construction timeline. Allocations made earlier to generate
or validate immutable inputs are not this owner's load transitions.

Only a fully successful operation returns an owner containing the actual height,
collision and navigation objects. Cancellation, stale input, incompatible data or
loader failure drops staged allocations and returns no usable owner. That atomic
host refusal is outside the supported positive source path; it does not model the
source's partially retained failure state or provide cached failure results.

Repeated `acquire` checks the same immutable source/configuration, then increments
the reference without decoding or allocating again. The positive signed 16-bit
source reference domain is bounded before increment. No unload or `GetGrid` API is
provided. Consuming `into_parts` transfers the exact allocations to existing
consumers; it neither reloads nor creates registration authority. That transfer
ends this lifecycle owner and conveys no retained load/reference/configuration
certificate. The current downstream consumer independently selects its fixed
both-enabled policy. A future map lifecycle must retain this owner or add an
inseparable certificate rather than infer load history from the transferred parts.

## Connected coverage and limits

The existing genuine generated single-tile cold-resident test consumes these
objects in its actual `NativeWorld` first-Chase dispatch, constructor/filter,
freshness rejection, replacement and retry path. Its source install is an authored
client-shaped fixture; this is component coverage, not player-verified gameplay.
Warm tests check the actual allocation identity and decoder preparation counts.

Multi-tile guard inventories with only one generated navigation tile cannot be
relabelled as fully loaded tile sets. Shared/global managers, partial/map-only
loads, failed-load caching, cleanup and registration remain unsupported. No held
compiler/consumer, native snapshot format, wire GUID or selection path changes.
