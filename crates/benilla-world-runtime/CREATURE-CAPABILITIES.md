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
coverage is required for unowned addon effects, all pets, summons,
control/possession/client control, initial auras/modifiers, scripts and replacement
AI, map creation hooks, forced/random/event/pool/group entry selection, and their
respawn re-evaluation. Spawn relay and string IDs are retained and must be zero.
Other source fields/lookups must be classified by that capture closure: checking
these few raw columns does not certify source completeness. Main catalog length
is bounded before cloning; nested row cardinalities and empty exclusions are
checked before source serialization or hashing.

The required `CreatureCapabilityAddonClosure` is either `Absent` or
`GenerationSource { source_identity }`. `Absent` excludes both spawn/template
addon rows; if a generation catalog is selected, even an all-zero or overridden
retained row contradicts absence. `GenerationSource` resolves the exact selected
immutable generation source, including stable spawn, effective entry, full source
identity and matching spawn-template/exclusion rows. The generation validator owns
complete addon queries and the sheath-only supported subset. Missing owners,
wrong identities and contradictory sources reject rather than supplying a fallback.
The capability source policy and source-binding domain are v3.

`Profile::initialize_creature_capabilities` atomically selects this host and
requires existing source motion, exact catalog coverage of runtime NPCs, matching
effective entries, and spawn flags containing only the supported RUN bit (0 or 1).
The required initialization closure selects `Ground` for actual habitat 1 or
`DryWater` for actual habitat 3. The latter requires generation/model/height
ownership and the [source-ordered dry initialization contract](CREATURE-DRY-HABITAT.md).
Other valid habitat values remain unavailable to this executable domain. Actual
entry 6 is not relabeled as habitat 1; its DBC, movement, AI and full content
admission remain separate from these bounded habitat-3 mechanisms.

## Current state and lifecycle

The private `CreatureCapabilityState` owns the bits read by `CanFly`:
LEVITATING `0x400`, CAN_FLY `0x800000`, and HOVER `0x40000000`, plus a separately
retained SWIMMING projection and generation-owned initialization facts. It is not
full `MovementInfo`. Constructor `MovementInfo` begins at zero, while Creature's
`SetWalk(true, true)` adds WALK_MODE; InitEntry's RUN flag can clear WALK_MODE.
Neither changes this projection. Supported non-AIR habitats cause
`SetLevitate(false)`; habitat 1 short-circuits WATER/IsSwimmable, while habitat 3
requires the selected-model dry observation before level selection. No HOVER/
gravity flags or excluded control/script paths can set the three flight bits. A generation-owned sheath-only
addon writes misc bytes, not these flags; its captured `move_flags` are unused by
the pinned loader. The resulting owned initial projection is therefore zero.
There is no public setter or independently
writable CanWalk/CanSwim/CanFly boolean.

Ordinary supported movement and damage/periodic-only NPC effects cannot mutate
the projection. Death, corpse removal and despawn retain it. On respawn the
source closure still excludes ResetEntry substitution and unowned LoadCreatureAddon
effects. The retained supported addon selection is reapplied without changing
the owned bits; SetWalk remains irrelevant to them. The state is validated,
then rebound from the same captured source after the runtime increments the NPC
generation. Habitat 3 repeats the ordered initialization query using that life's
selected model; every retained life must prove zero SWIMMING. The source only
ORs this bit, so the dry domain never clears an earlier wet state. Source binding,
raw habitat, projected flags, initialization facts and generation persist
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
then also queries dynamic height. Habitat 3 instead selects the non-flying
swimming-capable branch: a 50-yard static search, dynamic height, then the
all-liquid GetWaterOrGroundLevel query at groundZ. The separate model threshold
and default liquid classification height remain bound, even on NO_WATER.
Neither habitat proves a dry location. The separately bound application provider
composes complete-dry loaded products and positively proven empty dynamics under
explicit enabled configuration.
`CreatureCapabilityWitness::validate_for` checks that request against the immutable
selected source catalog and exact outer NPC generation. It is not authentication or
live-state reconstruction: the current dry lifecycle and generation history leave
the owned projections zero. Any future mutable or wet capability domain must
replace this invariant. The witness alone supplies no GetNearPoint configuration,
candidate arithmetic, selection/fallback, LOS or complete provider; the separately
selected app providers must establish their own bounded evidence.

## Single-bit owner pathfinding closure

`CreatureRouteRequest.owner_ignore_pathfinding` is a required-option transcript
field. It is `None` when the optional interaction catalog is absent; missing
schema fields reject. Capability/model catalogs alone do not mint this receipt.
When selected source data is present but invalid, it errors rather than being
silently treated as missing. `require_owner_ignore_pathfinding(profile)` validates
request owner identity and returns the typed `OwnerIgnorePathfindingReceipt`.

The private mint runs inside `Frame::source_spatial`, borrowing the actual staged
`CreatureCapabilityState` even when the NPC has been removed from the state map.
It uses the immutable interaction source row with explicit `control.unit_state=0`,
all interaction exclusions (including absent external mutation, controlled/summoned
units and NPC aura/script hooks), complete capability exclusions, validated NPC
motion/Home audits and the actual `State::abilities(profile)` executable set.
The frozen ability list includes learned/ranked changes, rather than relying only
on initial profile abilities. Available effects are closed to damage/periodic
semantics; unavailable abilities cannot execute. Invalid live effects reject.

Only `UNIT_STAT_IGNORE_PATHFINDING=0x10000000` is projected. This is not complete
UnitState and does not mean that a chasing or melee-attacking NPC has state zero.
Chase/ChaseMove and melee set other bits. Home clears `ALL_DYN_STATES`, whose source
mask explicitly preserves IGNORE_PATHFINDING; it does not reset that bit to zero.
The explicit initial zero and closed supported lifecycle preserve this one bit.
Respawn uses the rebound capability owner after the actual generation increment.
There is no second mutable UnitState owner or checkpoint field.

The receipt binds policy, profile, immutable source closure, capability source and
current `NpcId`, and validates request map/content/geometry/spawn identity.
`owner_position` remains `request.origin` (current Unit map position), independently
of Home's potentially different `launch_origin`. The invariant does not depend on
path goal or prior polygon cache; the outer authority context binds the exact query.
Deserialized receipts are transcript consistency evidence, not authentication or
live-state constructors. Replay regenerates them from the executing state.

The six complete LF-normalized source hashes in
`OWNER_IGNORE_PATHFINDING_SOURCE_SHA256` cover Unit.h, Unit.cpp, Creature.cpp,
HomeMovementGenerator.cpp, TargetedMovementGenerator.cpp and PathFinder.cpp from
the pinned source. `tools/verify_owner_ignore_pathfinding_source.py` verifies them
and the reviewed source expressions. This source guard and numeric mask tests do
not claim to execute the full C++ Unit lifecycle. Creature ExtraFlags 0x2000/0x4000,
`m_ignoreMMAP`, MMap configuration, tile presence and route admission are separate
dependencies; this receipt supplies none of them.

## Persistence and checks

Native format 48 (`benilla-native-wander-leg-v48`) binds the typed route owner
transcript, generation-bound dry initialization and explicit addon closures
through v48 profile, state, record, checkpoint and source-action digest domains. Native47 and earlier saves/records
are rejected rather than rewriting `Spatial(Route)` receipts or inventing an
owner witness. The original fixture
format 2, equipment format and character/profile file schemas are unchanged. Source
predicate/runtime policy IDs and source hashes, plus the initialization and
scalar-swimming policies/hashes, are retained in each source witness binding and
the selected catalog/profile. Hashes are integrity/version bindings, not source authenticity.

Focused runtime tests cover valid/invalid habitat, requested/effective-row linkage,
all existing exclusions, contradictory catalogs, real chase NearPoint-to-Route
request binding, Home map/spline origin distinction, current respawn generation,
unselected compatibility, canonical persistence/replay/reopen, rehashed
state/receipt forgery and whole-frame rollback/retry when the provider is missing,
returns an altered inner request, or returns an unsupported/partial route. Controller tests additionally cover
durable replay, append/publication failures and death/despawn/respawn generation.
The scalar oracle remains independent evidence for the predicates, not proof of
real-install geometry or constructor/lifecycle parity.
