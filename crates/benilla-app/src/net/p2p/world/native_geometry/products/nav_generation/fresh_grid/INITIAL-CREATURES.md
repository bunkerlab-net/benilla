# Constructor-stage ordinary creature cells

The synthetic-component constructor adapter connects actual source generation to
an initial `Map::AddToGrid(Creature*)`/`SetCurrentCell` operation inside the retained
single-grid owner. This is **not completed Map::Add, AddToWorld, live membership,
GetUnit lookup, selection, production native admission or persistence**.

The connected tests use invented complete creature rows and authored client-shaped
geometry. They execute the real source-generation mechanism and ownership path;
they do not import a database roster or verify player-visible population.

## Actual constructor boundary

Pinned CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

- `Object.cpp::WorldObject` starts inactive, and `Camera.h::ViewPoint` starts with
  an empty camera list/null grid. `isActiveObject` is active OR has-viewers.
- `Map.cpp::Add` establishes the map and ensures the appropriate grid before
  AddToGrid, AddToWorld, active, ViewPoint and visibility operations.
- `Map.cpp::AddToGrid(Creature*)` inserts an ordinary non-pet into the grid-object
  store, then assigns CurrentCell. The grid reference inserts at the front.
- `Creature.cpp::AddToWorld` later registers GUID/database-GUID lookup and may
  activate the object from force-load-map configuration or the ACTIVE flag. It
  also handles spawn counting. `Unit::AddToWorld` schedules AI notification.

The runtime adapter privately mints a non-cloneable, non-deserializable fresh
object after the actual generation-zero result/projection and before
`NpcEvents::new`. It reuses the existing complete ordinary static identity/source
qualification, including its external-feature closure; a caller GUID, completed
pose index or profile row cannot mint a token. The token retains a clone of the
actual generation receipt and its world/profile/source/NpcId identity. This is
constructor-stage object substate, not pointer identity with the final State NPC.
Its empty ViewPoint and inactive state are newly owned constructor facts, not
assertions about a previously registered creature. Later respawns do not mint new
cold-constructor tokens.

## Real cell transaction

`with_initial_creatures` consumes the grid, invokes the exact runtime adapter and
binds every token to the original grid profile, world, source map/instance and
position-derived cell. The host checks source freshness/cancellation/deadline and
rejects duplicate NpcId/full source GUID. It inserts a stable boxed entry into the
actual cell's ordinary-creature container **before** assigning CurrentCell to the
one-shot object. Its ViewPoint stays null and the grid's object-data-loaded flag
stays false.

Any constructor error, callback refusal or unwind drops the entire consumed host
grid transaction, including prior entries and geometry. No partially populated
grid is returned. This rollback is host behavior, not source removal/unload/GC.
Only after successful construction can the consumer callback borrow the grid,
initial entries and exact query allocation. The query/height/grid lifetime remains
that of the enclosing retained callback.

The low-level runtime hook alone cannot roll back arbitrary caller storage; the
app transaction supplies that ownership. The token's consuming cell-assignment
primitive checks arithmetic and identity only: without insertion by this grid
bridge it proves neither storage nor registration.

## Limits after construction

The ordinary constructor and saved-state format are unchanged. The sidecar is not
serialized, replayed or reconstructed by restore. Returning a NativeWorld or query
from the callback gives no grid certificate. Native steps, relocation, removal,
respawn or visibility do not maintain these constructor-stage cell entries; never
consult them as current membership after simulation advances.

Complete object loading still needs source roster/persistent respawn/spawn/pool
owners and callback ordering. Completed registration still needs AddToWorld's
lookup, flags/event/string state, notification scheduling, active-map ownership,
ViewPoint and visibility effects. A fresh empty category does not discharge any
of those dependencies. The held compiler/consumer and selection remain closed.
