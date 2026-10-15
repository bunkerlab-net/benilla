# Retained PathFinder state boundary

Pinned reference: CMaNGOS `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`,
`PathFinder.cpp:306–383,410–789`, `PathFinder.h::clear`, and the Chase/Home
movement-generator call sites. Existing source/license notices remain applicable.

Native format **23**, policy `benilla-native-warm-route-cache-v23`, adds the
optional retained cache to Chase snapshots, requests and completed calculation
receipts. Older native checkpoints/records are incompatible; character/profile
save formats are unchanged. An incompatible open preserves the existing files.

## Runtime ownership and transactions

`PathFinderCache` carries the complete polygon backing vector, active prefix,
point limit, terminal path type, last requested endpoint and filter. Inactive
slots and appended tail remain significant: source clear does not clear storage,
and smoothing uses separate scratch refs. The current domain is ordinary fixed
74-point, ground filter `1/0`; dungeon growth and other filters are unsupported.
Operational storage is bounded at 4096 refs without truncation.

Each cache binds creature/map/content/geometry, NPC generation and loaded
inventory/source epoch/navigation policy. A checked revision increments once per
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

Accepted persistence/replay tests explicitly use `SyntheticFixture` calculations.
A serialized vector is not loaded navigation authority. Source-owner cache
restoration is refused until an actual owner-thread lineage rehydration contract
exists. No initial source-owner cache producer is supplied here; a cache-free
source-owner calculation remains unavailable. Existing cache-free bounded/synthetic
fixtures remain usable, but cannot silently erase an existing cache.

## Loaded warm polygon dependency

`native_geometry/source_route_warm_cache.rs` composes the two ordered loaded
liquid/filter observations with the actual retained-cache lookup/cut/replacement
query. It freezes the complete authority request, including prior cache revision,
and binds the exact owner generation, geometry, inventory, source epoch and
navigation policy. Ticket allocation is checked before work; one absolute deadline
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
