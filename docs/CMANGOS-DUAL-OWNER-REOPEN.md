# Bounded Chase and suspended Wander reopen

The synthetic component harness can reconstruct one dry ordinary NORMAL Chase and
its suspended NORMAL Wander on one freshly loaded, inventoried map-instance
query. This is a loaded-component persistence boundary. It does not enable
native content admission or populated player startup.

## Supported saved state

The active NPC must have an ordinary Chase constructor, a current successful
NORMAL Chase spline whose calculation exactly matches its latest cache, and a
separate suspended Wander constructor with its own durable NORMAL route head.
Other NPCs must be cache-free Idle. Both constructors belong to the same NPC
life and retain their original order, revisions, counts and accepted inputs.

The checkpoint, movement timers, target, spline progress, RNG and clocks are
restored unchanged. Reconstruction performs no gameplay tick or random draw.
The replay journal starts empty. The current Chase spline is never used as the
suspended Wander head.

A newer cache beside an older spline is rejected. This matters because pinned
CMaNGOS `TargetedMovementGenerator.cpp` calculates before its reachability check;
a calculation can update the cache without launching a replacement spline.
Chase NoPath/fallback and changed-active-polygon lineages are outside the bounded
fixed-cache proof. Suspended failed Wander heads and other generator shapes are
also rejected here. The existing single-Wander reopen has its own separate
NORMAL/pause and minimum-distance NoPath support.

## Fresh ownership and publication

The application recipe has a distinct kind and version 1, embeds the existing
Wander recipe, and retains exact accepted Chase cold/final requests. It binds the
checkpoint, world, profile, geometry, inventory, source epoch, configuration and
both constructor identities. Canonical decoding is bounded to 8 MiB and grants
no authority by itself. The checkpoint format is unchanged.

The private loader stages the exact inventory parameters, tiles, dry proof and
VMap before allocating a query. Historical body, target, terrain, collision,
filter and corridor checks run around the native operations. Chase validation
runs before selection on every retained input, including the final calculation.
The factory loads one fresh query and executes both independent constructor
recipes against that allocation.

The runtime pending value retains the complete unpublished world and consumes
an opaque dual certificate. A sealed installation plan preserves the exact
query allocation across that operation. The loader checks allocation identity
again before wrapping the query in its inventory projection. Two typed
certificates and their original semantics must match before the pristine
registry reserves and installs both entries. Historical dispatch metadata stays
inert; reopened handles have new live identities.

The world, composed providers, restored accepted-input captures and route
boundary publish only after all checks succeed. The store wrapper requires the
exact checkpoint/recipe lease and a pending reopen. Failed attempts discard the
unpublished values and leave saved bytes untouched. Generic cached checkpoint
restore remains closed.

## Source and acceptance

Source remains CMaNGOS `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:
`MoveMap.cpp` supplies the shared 1024-node map query; each `PathFinder` keeps its
own backing storage. `RandomMovementGenerator` interruption preserves its
finder, while Reset resets its timer and initializes a new finder.
`HomeMovementGenerator` uses a local finder. No constructor is synthesized from
serialized references.

The controlled loaded-geometry tests in
`crates/benilla-native-corridor-tests/src/dual_reopen_component_tests.rs` compare
saved state and continued Chase, then ordinary Home retirement, Wander Reset,
fresh Wander launch and a subsequent single-owner save/reopen. They also exercise
recipe mutations, distinct equal-byte allocations, swapped constructor proofs
and rejection between replay, installation and publication stages.

See [the shared-query proof](CMANGOS-DUAL-QUERY-RECONSTRUCTION.md) and
[the fixed-cache Chase proof](CMANGOS-CHASE-FIXED-CACHE-RECONSTRUCTION.md) for the
selected native recipe domain and resource bounds. These do not establish
arbitrary multi-polygon routes, arbitrary query interleaving or general loaded
cache recovery.
