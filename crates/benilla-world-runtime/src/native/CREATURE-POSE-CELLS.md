# Source-qualified creature pose-cell observations

Native format 80 optionally retains a `CreaturePoseCellIndex` for the exact
current NPC roster. This is a connected constructor/committed-frame observation
index, **not source CurrentCell, a grid-load receipt, object registration,
GetUnit lookup, visibility, movement permission, or native admission**. Existing
wire GUIDs and selection handling are unchanged.

## Source arithmetic and identity qualification

The GPL simulation primitive translates pinned CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`,
`src/game/Maps/GridDefines.h::ComputeCellPair` and Map::Add's cell bounds. Binary32
constants are rounded before promotion; coordinate arithmetic is binary64 and
conversion truncates toward zero before the unsigned bound check. Software
floating point preserves that grouping. Undefined/nonfinite conversion domains
fail closed; this is not a replacement for source IsValidMapCoord. The extracted,
hash-pinned C++ oracle covers 6,200 coordinate pairs, boundaries and invalid inputs.

The optional immutable owner is derived from an existing exact loaded common-map
classification, source motion policy and complete source-generation roster. Map
IDs 533/543 are refused because pinned Map::IsDynguidForced returns true for those
IDs. Common does not imply IsContinent or complete map content.

For each selected source, DB GUID and entry must be nonzero and fit 24 bits.
The retained template must not request dynamic GUIDs (ExtraFlags 0x00100000), and
the already-required complete external-feature contract must exclude event/group
substitution. Any retained EventAI database GUID/entry must agree with the same
source identity; conflicting source owners fail closed. These are the bounded ordinary GridLoader caller conditions for
newGuid=dbGuid. The source ObjectGuid expression is preserved without masking:
`0xf130000000000000 | (entry << 24) | dbGuid`.

`SourceStaticCreatureIdentity` records that qualified static identity candidate;
it does not assert that the source object was allocated, inserted or registered.
The source rows remain the existing capture contract, not new proof of database
completeness/authenticity. No held compiler/consumer or alternate capture is added.
Dynamic allocation is unsupported: source Map::Initialize seeds its allocator from
ObjectMgr's temporary-GUID high-water, not an invented counter of one.

## Live integration and restore

Actual NativeWorld construction captures the complete initialized roster. Each
successfully completed frame captures actual current owned NPC positions after
source motion and lifecycle execution, before commit. A pose may cross a cell or
grid boundary in this index without claiming any source grid-load permission.
Relocate-only spline updates are still pose observations; they are not mislabeled
as Map::CreatureCellRelocation callbacks. No speculative route destination is used.

Each entry contains its immutable static identity, current proof-generation NpcId
and computed cell. Cell buckets are exact, with no missing, duplicate or foreign
NPC membership. Hidden/dead creatures remain observations of retained runtime
objects; they are not declared visible or registered. Legacy respawn changes the
proof generation but not the qualified static identity.

The required checkpoint option cannot be omitted and interpreted as default.
Restore and checkpoint creation recompute the entire index from the current
validated owners; changed identities, cells, buckets, profile/world/player binding
or owner presence are rejected. This proves derived consistency, not independent
movement authority or historical trajectory. Exact TickRecord replay supplies the
latter. There is no new history vector; roster and existing checkpoint byte bounds
continue to apply. Out-of-domain observations abort the cloned frame transaction.
Invalid deserialized CellPair lookup inputs return no entries, avoiding key aliasing.

## Remaining genuine map ownership

Actual registration is still blocked. Source EnsureGridCreated constructs/linkages
an NGrid and attempts LoadMapAndVMap. Geometry-product/cache residency does not
prove that operation. The separate `loaded(grid)` predicate additionally requires
object-data loading through ObjectGridLoader, not merely terrain/VMap availability.
Source cross-grid relocation requires an active creature or that loaded predicate.
Map::Add subsequently owns AddToGrid, AddToWorld, active, ViewPoint and visibility
callbacks; their effects/noninterference remain to be composed. Dynamic queued
removal also performs spell/event/aura cleanup before erasing registration.

This index supplies exact reusable coordinate/identity observations for that work.
It does not complete the previously proposed resident-map/registry prerequisite,
open selection, create a corpse, release a ghost, or make production native entry
available. No previous checkpoint is migrated.
