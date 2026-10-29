# Fresh position-driven single-grid creation

`FreshSingleGrid` implements a bounded positive `EnsureGridCreated` stage on
actual position-derived source cells, with a retained geometry lifetime for its
consumer callback. It is not full `Map::Initialize`, object-data loading,
registration, movement authority or normal-gameplay admission.

## Source contract

CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

- [`Map.cpp`](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Maps/Map.cpp)
  `EnsureGridCreated` stores a newly constructed grid, links it to the map, sets
  IDLE, then loads terrain at `(63-gridX,63-gridY)`. An existing grid returns
  without a further terrain load/reference. `Map::LoadMapAndVMap` records success
  only when `TerrainInfo::Load` returns a GridMap.
- [`NGrid.h`](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/framework/GameSystem/NGrid.h)
  initializes ID `x*64+y`, INVALID state, object-data-loaded false, 16×16 cells,
  expiry timer, zero active locks and explicit unload lock `!unload_enabled`.
- [`Grid.h`](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/framework/GameSystem/Grid.h)
  separates typed world/grid stores and active-grid objects.
- [`GridReference.h`](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/framework/GameSystem/GridReference.h)
  links by insertion at the manager's front and incrementing its size.

The Rust owner has an actual owned grid node, grid-keyed slot collection and
insertion-ordered linkage resolving to that node. Its separate typed containers
represent only the initially empty construction stage. Their uninhabited private
element types prevent manufacturing object IDs or insertion authority. They are
not implemented registration stores, and their emptiness is never evidence that
the source world roster is empty. Registration needs real qualified references.

Expiry and unload configuration are explicit local effective choices, not
inferred server defaults. The bounded `u32` expiry is a direct constructor
operand; it does not prove configuration parsing or `Map::SetTimer` minimum-delay
normalization. No timer is executed here. Map and instance identity come from the existing
profile-bound query configuration, checked against the actual loaded query.
The existing source cell calculation derives the requested grid from binary32
position operands; the reversed tile must match actual generation inputs.

## Real resource lifetime

After slot insertion, linkage and IDLE, the owner invokes the actual
[positive terrain loader](POSITIVE-LOAD.md). It retains its original decoded
height/VMap, configuration, reference and attempt counts, and an Rc lease on the
exact native query allocation. No preloaded object can be passed as a new load.
Repeated same-grid `ensure` checks freshness without reloading or incrementing
terrain references. Another grid is unsupported, not implicitly loaded.

`with_query` consumes the owner, transfers its original query snapshot into a
callback, and borrows the still-live grid owner to that callback. Exact native
allocation identity is checked before transfer. The cold-controller component
fixture runs its real `NativeWorld` first-Chase/freshness/replacement assertions
inside this scope using the retained height. The owner, linked node and native
allocation lease remain alive throughout this callback. Returning or moving an
independent snapshot out of it conveys no grid certificate or extended grid
lifetime. Dropping the host scope is not source unload, cleanup or manager GC.

## Deliberate limits

This is fresh positive single-grid host composition. Failed or cancelled stages
are dropped transactionally, unlike source retained partial failure state. No
source tick, grid expiry transition, unload, shared-manager, preloaded/mixed tile,
object-load recursion, active-object, ViewPoint, visibility or registration API
is implemented. `object_data_loaded` remains false. Full `Map::Initialize` also
requires persistent-state/pool/spawn/transport and other owners not supplied by
this stage. No held compiler/consumer, native format or selection path changes.
