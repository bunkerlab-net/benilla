# Source creature capability ownership

The optional native creature capability host retains raw source inputs, owns the
current capability-relevant movement flag projection, and binds the result into
reachable source-motion `NearPoint` and `Route` requests. This is not a height/LOS provider,
raw-content compiler, playable-world claim or native admission token. The
uninhabited `NativeCapability` and unselected component path remain unchanged.

## Source and capture contract

CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f` is the pinned source;
copyright CMaNGOS contributors, upstream GPL-2.0-or-later, Rust GPL-3.0-or-later.
No client assets or genuine database rows are bundled. The independent scalar
[predicate port and oracle](../benilla-sim/CREATURE-CAPABILITIES.md) cover
`Creature.h:643-646`, `Object.h` movement bits/`HasMovementFlag`, and
`ObjectMgr.cpp:520-524` loader repair. The runtime source policy also binds the
complete LF-normalized `Creature.cpp` and `Unit.cpp` hashes. These additional
guards pin reviewed constructor/lifecycle source; the scalar oracle does not
execute those callbacks.

`CreatureCapabilityCatalog::validate_sources` retains raw `InhabitType` values
1 through 7, rejecting the invalid inputs which ObjectMgr repairs to 7. Each
runtime spawn key has its raw spawn entry, exactly one effective template row,
its raw requested `SpawnDataId`, and exactly one effective spawn-template row.
Raw and effective entries must match; positive requested spawn-template IDs
cannot silently fall back to row zero. A genuine captured row zero is allowed.
Duplicate spawn keys and contradictory repeated template/spawn-template rows
reject. The stable spawn key equals `NpcId.spawn` / `ai.creature_id`; the typed
catalog does not authenticate an imported database GUID.

Every lookup is explicitly `Missing` or `Complete`. Empty template/spawn queries
are unavailable, not default rows. Complete-empty `creature_init::ExternalFeature`
coverage is required, including both spawn/template addons, all pets, summons,
control/possession/client control, initial auras/modifiers, scripts and replacement
AI, map creation hooks, forced/random/event/pool/group entry selection, and their
respawn re-evaluation. Spawn relay and string IDs are retained and must be zero.
Other source fields/lookups must be classified by that capture closure: checking
these few raw columns does not certify source completeness. Main catalog length
is bounded before cloning; nested row cardinalities and empty exclusions are
checked before source serialization or hashing.

`Profile::initialize_creature_capabilities` atomically selects this host and
requires existing source motion, exact catalog coverage of runtime NPCs, matching
effective entries, actual habitat `GROUND=1`, and spawn flags containing only the
supported RUN bit (0 or 1). Other valid habitat values can be retained in the
independent catalog but cannot enter this first executable runtime domain.

## Current state and lifecycle

The private `CreatureCapabilityState` owns only the bits read by `CanFly`:
LEVITATING `0x400`, CAN_FLY `0x800000`, and HOVER `0x40000000`. It is not full
`MovementInfo`. Constructor `MovementInfo` begins at zero, while Creature's
`SetWalk(true, true)` adds WALK_MODE; InitEntry's RUN flag can clear WALK_MODE.
Neither changes this projection. Actual GROUND causes `SetLevitate(false)` and
short-circuits the WATER/IsSwimmable branch; no HOVER/gravity flags or excluded
control/addon/script paths can set these three bits. The resulting owned initial
projection is therefore zero. There is no public setter or independently
writable CanWalk/CanSwim/CanFly boolean.

Ordinary supported movement and damage/periodic-only NPC effects cannot mutate
the projection. Death, corpse removal and despawn retain it. On respawn the
source closure still excludes ResetEntry substitution and LoadCreatureAddon
effects; SetWalk remains irrelevant to the owned bits. The state is validated,
then reset from the same captured source after the runtime increments the NPC
generation. Source binding, raw habitat, projected flags and generation persist
and are validated on restore. Rehashed contradictory values or missing state
reject rather than reconstructing omitted inputs.

## Reachable query boundary

The executing NPC is often temporarily removed from the runtime map and its
motion owner is taken into a local variable. `source_spatial` therefore receives
the real staged capability owner explicitly, validates it against the selected
catalog and generation, and carries its witness into the adapter. It never
looks up a stale map copy or substitutes a template default.

For selected source hosts, actual chase NearPoint calls become
`MotionQuery::NearPoint(CreatureNearPointRequest)` and route calls become
`MotionQuery::Route(CreatureRouteRequest)`. Both wrappers include the
unchanged original simulation request and the read-only current source witness.
Identity, map, content, geometry and clock bind to the executing frame; the outer
authority receipt additionally binds profile, world, tick and NPC generation.
Returned `motion::Reply.request` still must equal the original simulation query.
Replay compares the full wrapper and witness in the ordinary receipt stream.
Unselected source-motion fixtures retain `Spatial(NearPoint)` and `Spatial(Route)`
without invented source rows. Synthetic fixture geometry explicitly handles the wrappers and
remains authored host-trig test geometry.

`CreatureRouteRequest.owner_position` is the executing Unit's map position for
`PathFinder::updateFilter`, not a supplied liquid flag or the computed spline
launch position. The simulation's trusted `dispatch` constructs `RouteRequest.origin`
directly from its staged `map_pose.position` at the moment of the route query.
The wrapper binds that value explicitly as `owner_position`. A moving chase
dispatch first relocates to the computed spline position; Home does not force
that relocation and can retain a different `launch_origin`. A pre-call pose
copied from the runtime map would be stale during an in-call replan. The source
contract test guards this construction and an actual Home transition proves
different map and launch positions. Geometry must retain this distinction and
bind the full typed request, including the owner generation and capability
source, rather than accepting habitat as evidence of a physically dry location.
The wrapper supplies no route, liquid or path-result fact by itself.

The witness derives the exact source predicates and
`Unit::UpdateAllowedPositionZ` capability branch. GROUND with zero capability bits
is the non-flying, non-swimming branch. Its `Map::GetHeight` argument is
`swim=false`, not disabled VMAP: Map requests static VMAP (still subject to the
effective source height configuration), uses the ordinary 10-yard initial search,
then also queries dynamic height. No physical dry-location
claim follows from habitat. The separately bound application ground-height subquery now composes loaded static
products and positively proven empty dynamics under explicit enabled configuration.
`CreatureCapabilityWitness::validate_for` checks that request against the immutable
selected source catalog and exact outer NPC generation. It is not authentication or
live-state reconstruction: the current closed ground-only lifecycle leaves the owned
projection fixed, and any future mutable capability domain must replace this check.
The actual GetNearPoint collision configuration, source candidate arithmetic,
selection/fallback sequencing, LOS and complete provider remain unavailable; no
guessed config or dry fallback is supplied.

## Persistence and checks

Native format 26 (`benilla-native-initial-player-model-height-v26`) binds the typed route owner
transcript through profile, state, record, checkpoint and source-action digest
domains. Format 21 and earlier saves/records are rejected rather than rewriting
`Spatial(Route)` receipts or inventing an owner witness. The original fixture
format 2, equipment format and character/profile file schemas are unchanged. Source
policy IDs and all five hashes are retained in the catalog/profile and each source
witness binding. Hashes are integrity/version bindings, not source authenticity.

Focused runtime tests cover valid/invalid habitat, requested/effective-row linkage,
all existing exclusions, contradictory catalogs, real chase NearPoint-to-Route
request binding, Home map/spline origin distinction, current respawn generation,
unselected compatibility, canonical persistence/replay/reopen, rehashed
state/receipt forgery and whole-frame rollback/retry when the provider is missing,
returns an altered inner request, or returns an unsupported/partial route. Controller tests additionally cover
durable replay, append/publication failures and death/despawn/respawn generation.
The scalar oracle remains independent evidence for the predicates, not proof of
real-install geometry or constructor/lifecycle parity.
