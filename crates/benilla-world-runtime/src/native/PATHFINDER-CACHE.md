# Retained PathFinder state boundary

Pinned reference: CMaNGOS `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`,
`PathFinder.cpp:306–383,410–789`, `PathFinder.h::clear`, and the Chase/Home
movement-generator call sites. Existing source/license notices remain applicable.

Native format **50**, policy `benilla-native-home-wander-v50`, retains the
optional retained cache to Chase snapshots, requests and completed calculation
receipts. Older native checkpoints/records are incompatible; character/profile
save formats are unchanged. An incompatible open preserves the existing files.

## Runtime ownership and transactions

`PathFinderCache` carries the complete polygon backing vector, active prefix,
point limit, terminal path type, last requested endpoint and filter. Inactive
slots and appended tail remain significant: source clear does not clear storage,
and smoothing uses separate scratch refs. The current domain is ordinary fixed
74-point, with source-owned dry filter `1/0` for habitat 1 or `13/0` for habitat 3.
The latter retains `CanSwim=true` while all loaded polygons remain validated ground.
Dungeon growth, wet movement and other filters are unsupported.
Operational storage is bounded at 4096 refs without truncation.

Each cache binds creature/map/content/geometry, NPC generation and loaded
inventory/source epoch/navigation policy. The expected filter is derived from the executing capability
owner, including on restore; a well-formed filter 1 cache cannot replace a habitat-3
filter 13 cache. A checked revision increments once per
accepted completed calculation. A reply must echo the exact prior cache and
requested endpoint; changing ownership, provenance, filter or point limit,
shrinking storage or overflowing the revision fails. Native generation validation
runs before an external Route query and again on its reply.

The mutable cache belongs to the staged Chase generator. A valid completed
NoPath calculation can update it even though no spline launches. Same-generator
retargeting and interruption retain it; Home uses a fresh temporary cache and
never installs it on Chase. Death or generator replacement drops it. Last
requested endpoint survives a failed calculation for speed-only replanning.
Validation errors, missing authority and a later frame failure discard the whole
staged update. Controller/store publication follows the existing atomic boundary.

The first Chase `OwnerPathFinder` calculation is admitted only with the explicit first
`PathfinderDispatch`, exact request/constructor identity, revision one and a
Complete NORMAL terminal cache with a nonempty active prefix. Missing cache state
alone never establishes construction: a retained dispatch with no prior remains
invalid, and a first dispatch cannot claim a retained prior. Every Complete result
retains the existing finite-point, exact origin/goal, point-limit and one-segment-
witness-per-edge validation. All five coverage/LOS/body/support/dry predicates must
pass. Nonterminal initial Chase source calculations, including NoPath, stay
unavailable. Wander has a separate initialized-owner contract and retains exact
failed requests, including a cold MinimumDistance calculation; see
[WANDER.md](../../WANDER.md). Retained source calculations require the non-first event, full exact prior echo,
unchanged binding and a single revision increment; retained NoPath remains valid.

The native adapter preserves its actual request-side runtime-minted dispatch and
requires it on source-owner replies. Missing selected configuration/live event
cannot be replaced by reply-supplied metadata. These checks validate typed trusted-
authority claims, not execution of native PathFinder or geometry. The app provider
must independently bind the actual loaded constructor/query, source configuration,
cache inventory/epoch/policy and real complete-segment evidence. The simulation's
publicly serializable dispatch and cache hashes supply no loaded authority.

Motion structural restore accepts a first source spline only with its matching
first constructor and current source cache; revision-one terminal evidence is
revalidated in full. A later retained NoPath or Complete calculation whose Chase
endpoint is unreachable may leave that older first spline with a newer current cache. Exact recorded facts can replay after runtime dispatch
is regenerated, without executing or rehydrating loaded queries. `NpcMotion::restore`
still rejects source-owner caches through its generic restore path. The separate
synthetic-only pending Wander owner supports one current NORMAL or MinimumDistance
NoPath head only after an exact challenged native replay and app-side loaded
product binding; see [WANDER.md](../../WANDER.md). Contract tests deliberately supply synthetic typed claims and
malformed variants; they establish no native/source/physics execution. Existing
cache-free bounded/synthetic fixtures remain usable, but cannot erase a cache.

## Loaded warm polygon dependency

`native_geometry/source_route_warm_cache.rs` composes the two ordered loaded
liquid/filter observations with the actual retained-cache lookup/cut/replacement
query. It freezes the complete authority request, including prior cache revision,
and binds the exact owner generation, geometry, inventory, source epoch and
navigation policy. The source policy
`cmangos-8ec338a1-ground-or-water-capable-dry-route-filter-v2` preserves the exact
initial include flags across dry updates: ordered mask-15 then mask-10 `NO_WATER`
observations do not turn 13 into 1. Any nonzero status rejects before navigation.
Ticket allocation is checked before work; one absolute deadline
covers the entire dependency. Missing/changed data or an unsupported branch
publishes nothing.

The native cache handle is opaque and tied to one loaded immutable mesh instance.
Equal bytes reloaded into a different mesh do not rehydrate it. The bounded direct
corridor can seed a dependency handle; this is explicitly not an original owner
cold calculation. Tests separately author the initial synthetic runtime receipt,
then exercise loaded dependencies using the runtime-owned prior state.

Warm lookup preserves ordered active/tail scans, source squared-distance thresholds,
first-equal winner, early exit, overwritten comparison distance, 5/10 nearest
fallback and sequential start/end tail appends. Same-polygon/cut/replacement
proposals preserve all unaffected storage. Replacement uses the selected cached
refs and original direct 1024-node search. Cold uninitialized-distance cases,
endpoint distance greater than seven, non-exact direct statuses and later owner
processing remain explicitly unsupported. See native-nav `PROVENANCE.md` and the
`tools/warm_cache_README.md` oracle documentation for exact source fragments and test scope.

A polygon proposal stops before BuildPointPath. It is not a terminal cache receipt
or complete Route fact. OptimizePath, NormalizeZ, source configuration/endpoint
lifecycle and a complete supported owner calculation remain necessary. The
existing conservative route-evidence contract must also be reconciled explicitly
with source semantics. Production admission and participant requirements remain
unchanged; there is no populated-world feed or game activation in this checkpoint.

## Terminal smooth-calculation dependency

`native_geometry/source_route_point_path.rs` continues the same loaded warm
proposal through its selected active polygon prefix, without another nearest or
path search. Source SamePolygon takes its own shortcut path; Cut/Replacement
use the original smoother and source-ordered point conversion, optional
optimization, failure/limit classification and pre-normalization actual endpoint.
Successful smoothing never substitutes scratch polygon refs for the retained
cache. A terminal shortcut clears active length while preserving backing storage.

Optimization and normalization settings are explicit query inputs bound into
geometry policy, not assumed server defaults. The ordinary Chase dependency
admits NormalizeZ=false and ignoreNormalization=false. Active normalization
needs actual per-point allowed-Z/transport observations. Prior terminal path type
comes from the frozen runtime cache and preserves the source INCOMPLETE bit where
required. Original all-zero points in an optimized path are rejected: source's
zero sentinel can preserve old point-vector slots on a cut, and that defined
retained-point dependency is not represented here.

This is a terminal **smooth-calculation** proposal, not complete Chase dispatch.
Pinned Chase first attempts a straight-line/raycast calculation under its own
distance, vertical, LOS and liquid conditions before conditional smooth fallback.
Home creates a fresh temporary PathFinder and is rejected by this warm provider.
The native primitive separately tests force-destination arithmetic but does not
provide a Home producer. The separate [Home continuation](../../HOME.md)
uses a fresh constructor and cold same-polygon NORMAL branch. A source NORMAL result may end early after NoSteerTarget;
it does not satisfy or replace the existing CompleteRoute segment/endpoint proof.
Source initial cold policy, full dispatch, normalized points, loaded checkpoint
rehydration and production gameplay admission remain separate dependencies.

The [constructor dispatch](PATHFINDER-DISPATCH.md) lifecycle is retained separately
from this cache. A warm cache is never evidence that a constructor ran, and a
cache-free NoPath may still retain a constructed Chase identity.
