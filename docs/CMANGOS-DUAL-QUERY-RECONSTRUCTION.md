# Shared-query Wander and Chase reconstruction

This native composition reexecutes two independent constructor recipes on one
fresh source map-query allocation. It establishes query identity and calculation
semantics, not creature ownership, checkpoint admission or populated startup.

## Source ownership

Pinned CMaNGOS Classic revision
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f` keeps one 1,024-node
`dtNavMeshQuery` per map/instance (`MoveMap.cpp:356–379`). Each `PathFinder`
retains that shared query while constructing its own 74-entry zeroed polygon
backing and active length zero (`PathFinder.cpp:35–54`). The query allocation
is shared; the constructor's retained path state is not.

The selected default Wander creates its finder during initialization
(`RandomMovementGenerator.cpp:25–49`), before the later lazy Chase finder
(`TargetedMovementGenerator.cpp:487–504`). Interrupting Wander preserves its
finder. Home retirement deletes Chase, retains the suspended default, and later
resets Wander with a new finder. These are separate owner-lifetime obligations;
reconstructing calculations does not replay those gameplay transitions.

## Bounded composition

The factory accepts immutable native geometry inputs, a
[Wander reconstruction recipe](CMANGOS-WANDER-ENDPOINT-COMPATIBILITY.md), and a
[fixed-cache NORMAL Chase recipe](CMANGOS-CHASE-FIXED-CACHE-RECONSTRUCTION.md).
It allocates one actual source query, reconstructs Wander, then reconstructs
Chase on that same allocation. Each reconstruction starts its own constructor
state and retains its own typed challenge, terminal and resumed capture witness.
No serialized polygon reference is an input.

This selected order relies on the existing same-polygon primitives reading the
immutable mesh with local scratch. It is not an equivalence claim for arbitrary
interleavings of Detour node-pool, open-list or sliced-query operations.

The output owns the loaded query and both opaque certificates. Each certificate
must check against that exact allocation. A separately loaded query with identical
geometry, slot, capacity and output bytes is still a different query. Distinct
challenge types prevent substituting a Wander certificate for a Chase certificate;
challenge identity distinguishes attempts within each type. Native query leases
do not themselves prove a creature's constructor identity.

Both recipes keep their existing selected domains and operation budgets. A single
caller deadline covers the complete composition. Read-only observers can reject
at reached stages; successful callbacks cannot supply native state or mint a
certificate. A failed second reconstruction publishes no composed result.

## Future owner bridge requirements

The existing app/runtime compact restore path remains unchanged. A future bridge
must bind both certificates to the same unpublished provider, the immutable
inventory and configuration, each exact creature/construction identity, original
cache revision and accepted calculation head. It must compare complete native
terminal semantics and publish the provider, runtime and durable store together.
The source Chase straight-selection gate precedes current-query selection on
every calculation. Owner replay therefore needs a pre-selection validation point
for each retained operation; observing an already-selected query is too late.

The geometry loader must build its source-query snapshot around the allocation
returned by this factory after checking exact inventoried bytes and metadata.
Creating another provider and replacing its query afterward does not establish
that identity. A private unpublished loader can stage verified immutable products
for observers, then return the projection and both certificates together.

The successful Wander head is generator-owned and survives suspension. A Chase
NORMAL calculation may update its cache before a reachability failure prevents
launching a spline. Therefore a global spline alone is not proof of the latest
Chase cache. An initial bridge can admit only an exact latest-cache/current-Chase-
spline match and reject the unlaunched newer-head shape. It must preserve original
Wander and Chase revisions, clocks, RNG state, timers and spline progress rather
than replaying game ticks or issuing new random draws.

A concrete acceptance loop must save during an actual Chase over suspended
Wander, reopen on a fresh query with both original constructors, continue Chase,
return Home, retire Chase, reset Wander at the source-defined point, and save and
reopen again. Wrong geometry, allocation, constructor, head, revision, challenge,
operation order and publication faults must reject atomically. This document does
not claim that loop is implemented by the native composition alone.

## Executed native acceptance

An authored precision inventory executes independent original Wander and Chase
histories exceeding 2,048 calculations. The reconstructed pair uses the same new
allocation for both certificates while the old allocation stays alive. Tests
compare complete terminal state and original capture counts, then interleave
further calculations and compare each owner's exact continuation independently.
Different filters and backing states ensure one constructor cannot silently seed
the other. These are native calculation counts, not elapsed gameplay ticks.

The negative controls check foreign equal-byte allocations, old allocations,
wrong endpoints, independent challenges, malformed recipes and actual unloaded
tiles. All 19 Wander and 7 Chase observer stages reject independently and retry
cleanly; deadline injection covers each stage and never returns a partial pair.
Compile-fail cases guard pair fabrication, typed role/challenge swaps and access
to the private loaded-query Chase replay helper. Original source-oracle fixtures
and operation implementations are unchanged.
