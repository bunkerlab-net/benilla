# Source-backed ordinary NPC motion

`creature_motion` translates the source linear spline and the ordinary
`UnitAI` melee chase configuration (`offset=angle=0`, `moveFurther=false`),
short-distance Home movement, wounded chase speed, and the AI-notification
clock. It is a bounded intermediate toward full Classic. It does not open a
native admission capability, supply native database rows, implement an owner
PathFinder, or establish playable-world parity.

## Source and numeric contract

Pinned CMaNGOS Classic revision:
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, GPL-2.0-or-later.
The Rust translation and authored probes are GPL-3.0-or-later; existing
[authorship and license notices](PROVENANCE.md) apply.

Source entry points:

- `Movement/MoveSpline.cpp:31–59,92–157,210–280,304–308` and
  `Movement/spline.cpp:117–120,155–159`: linear interpolation, integer spline
  timestamps, segment progression and completed arrival
- `Movement/MoveSplineInit.cpp:55–105`: running-spline origin correction,
  source walk/run flags and velocity selection
- `Entities/Unit.cpp:490–531,10923–11095`: event → spline → motion generator →
  AI → combat-manager ordering; 400 ms map relocations; facing from the old map
  position; lastMoveTime; notification scheduling and deduplication
- `MotionGenerators/TargetedMovementGenerator.cpp:132–178,205–378,393–425,
  477–609,616–705`: initial chase, 250 ms rechecks, interrupted approach,
  source facing, 2000 ms backpedal timer, dispatch and reachability
- `MotionGenerators/HomeMovementGenerator.cpp:40–110`: destination selection,
  forced path request, explicit final orientation and arrival lifecycle
- `Entities/Creature.cpp:2983–2990`, `Entities/Unit.h:1331` and targeted movement
  lines 572–573: wounded slowdown from current health and static flags
- `Entities/Unit.cpp:7984–7988,8556–8559,9389–9395`: combat clears initial aggro
  inhibition; both ordinary initial-add and respawn use the configured delay
- `Entities/Object.cpp:2297–2412,3027–3035`: near-point and bearing operations
  retained as explicitly unresolved spatial queries

Spline distance squares, correctly rounded sqrtf, interpolation, integer→float
conversions, per-segment integer timestamp truncation and source mixed-precision
wounded speed use existing software binary32/binary64 APFloat operations, RNE.
No platform trig, sqrt, float clock or renderer math determines these outputs.
The first CommonInitializer accumulator starts at **1 ms**. Each segment rounds
the previous integer accumulator to binary32, adds its binary32 duration, and
truncates the result. Arrival discards the remaining update time; that time is
never applied to a replan launched later in the same update.

Wounded slowdown evaluates `health * 100.f / maximum` in binary32. Below 30%,
unless NO_WOUNDED_SLOWDOWN, running chase multiplies its launch velocity by
`1.f - ((30.f - min(percent,30.f)) * 1.67) / 100`, with the unsuffixed `1.67`
operations in binary64 and the source float assignment before multiplying speed.
Walking and Home do not apply this branch. Health, health maximum and effective
walk/run speed must be current staged state. A health change alone is not a
source `UnitSpeedChanged` notification; a new launch re-reads health.

Bearing and GetNearPoint trig are separate typed authority requests. Requests
bind exact source positions, clock, owner/target, radii, distance, intended angle,
map and content/geometry revision. Replies carry canonical finite binary32
values and are replayed exactly. They must come from source-compatible spatial
services. The module does not prove independent cross-platform recomputation of
libc atan2/sin/cos. Synthetic tests deliberately label their host-trig adapter;
it is not a production provider. A deterministic source-checked trig provider is
still required before peers can independently regenerate these replies.

The numeric domain rejects NaN, infinities, subnormals, overflow, invalid world
coordinates, nonpositive speeds, zero/dead health, and durations above int32 max.
Non-Playerbots source PathFinder caps the point path at 74; longer or truncated
paths are rejected rather than silently cut.

## State, queries and ordering

`MotionState` retains both a spline-computed position and the less frequently
relocated map pose. `advance` progresses an already active spline first, then
runs the movement generator, then emits complete source events. The first
position timer starts at zero; subsequent relocations occur at 400 ms or
arrival, reset to 400 rather than preserving excess elapsed time. A chase replan
forces a current map relocation before its path query. Home queries from the
old map pose, but Launch corrects its first vertex to the current spline pose;
the route request includes both origins, and spatial evidence must cover the
corrected path. InterruptMoving's relocate-only path is distinguished from map
relocation, since it does not invoke OnRelocated.

`start_chase` and `start_home` consume no elapsed time. New commands from the
later AI phase are launched after that frame's movement update. By contrast,
`advance_for_event_command` advances the clock once, launches an event-phase
AttackStart at that clock, and then gives its spline the current Unit diff.
`begin_map_frame` also binds a synchronous player/event callback to the current
map time while `processed_map_clock_ms` retains the prior completed Unit frame. The
later Unit update consumes that diff exactly once. StopMoving(true) uses computed
spline XYZ and a new stop spline ID without relocating the map pose or lastMoveTime;
SetInFront changes only orientation.
The paired regression preserves that distinction through snapshot reopening and
checks exact query-clock bindings. Chase arrival and Home arrival
come from completed spline state, never from proximity or renderer interpolation.
Final facing and Home's final angle are retained in snapshots. Source SetFacing
wraps the upper 2*pi boundary to zero, and saved spline-facing angles enforce
that half-open interval. Old target-facing objects are resolved independently
when a chase retargets without immediately replacing its old spline. lastMoveTime
updates only at source position-update sites, including forced relocations.
The stationary-melee refresh helper uses the strict `lastMoveTime + 3000 < now`
comparison and sequenced target movement flags. GroundMovementFlags represents
the supported source XY/turn/walk subset: turning alone is not IsMoving, walking
is moving but not running, and unsupported/authority-owned bits fail closed.

The transactional runtime adapter, sequenced player movement ingress and exact
client spline projection are documented in
[`NATIVE-MOTION.md`](../benilla-world-runtime/NATIVE-MOTION.md). Its current boundary
explicitly rejects co-due player notify/SpellEvent updates until their shared source
event scheduler is integrated.

AI consumes an opaque kernel-issued SourceMotion token bound to command, request,
origin, elapsed time, source end-clock and internal movement predicates. It has
no serializer/deserializer and the external segment authority rejects it. Legacy
endpoint speed validation is unchanged for external segments. A whole-world
adapter must stage this token, motion state and AI together before committing.

Near-point, complete-route, bearing and nearby-collider replies have distinct
request/response types and exact request echo validation. A complete route
contains its producer class, stable path identity, all bounded points and one
spatial witness per corrected path segment. Full source owner PathFinder,
bounded dry Detour and synthetic fixtures are explicitly different origins.
None is relabeled as another. Incomplete, truncated, missing and shortcut results
remain distinct; unknown checks cannot become successful movement.

Current conservative spatial witnesses require loaded coverage, segment LOS,
body sweep, continuous support and dry ground. These are **provider safety
restrictions**, not a claim that CMaNGOS itself demands each of them for every
path segment. Source PathFinder can intentionally shortcut, tolerate incomplete
paths or move very quickly home; these branches are not admitted by this first
provider contract. A known chase NoPath is distinct from missing geometry.

The motion state and its query counter are staged transactionally. A later
route, bearing or collision failure rolls back prior spline/time/pose changes.
Authorities must be read-only proposals: external cache mutations cannot be
undone by this clone-and-commit boundary. Snapshots retain the complete route request, producer identity, points and
per-segment evidence as well as the corrected spline. Restore revalidates
those bindings, spline timestamps, progress and Home endpoint/facing agreement; a serialized success boolean is not a motion
completion token.

## AI notification is a lifecycle, not a boolean

`AggroNotify` starts CanAggro=false. Ordinary initial AddToWorld schedules the
configured CreatureRespawnAggroDelay; JUST_ALIVED schedules it forcibly and
respawn clears CanAggro. Visibility changes can schedule zero delay;
OnRelocated schedules the configured relocation delay. A pending event is not
postponed/replaced unless forced. Unit's event phase executes due notifications
before spline/motion/AI. Execution sets CanAggro to actual alive state and emits
a requirement to run the nearby-object visitor. EnterCombat while inhibited sets
CanAggro=true and aborts that event. No visitor members, LOS, accessibility,
visibility, faction reaction or successful aggro are inferred by this clock.

## Exact missing inputs and source exclusions

A native adapter must establish, from actual current source state:

- Spawn/combat-start/reset pose including orientation, active-object state and
  reset-position availability; transport-local transforms where applicable
- Effective current run and walk speeds, health/max health, combat reach,
  bounding radius, collision height, NO_WOUNDED_SLOWDOWN,
  DONT_REPOSITION_IF_MELEE_TARGET_IS_TOO_CLOSE and world-boss fanning settings
- Actual movement/unit flags and current auras/casts/scripts; target movement
  flags, alive/in-world/map/transport identity, falling/flying and victim link
- Near-point collision configuration, collision candidates, allowed-position Z
  and ground/liquid queries, collision-height LOS at the exact source endpoints
- Per-owner PathFinder map/instance/transport mesh, pathfinding enablement,
  IGNORE_PATHFINDING, CanWalk/CanSwim/CanFly and water/underwater liquid type,
  owner filter, straight-line eligibility, Z-normalization configuration and
  source path status/corridor reuse
- Complete current nearby unit set for fanning; source random direction/angle
  draws when an actual eligible collider is present
- Initial-add/respawn and relocation AI notify configuration, current loaded
  cell membership, and source notification visitor phase
- Home's final addon/temporary-faction/summoner/script callbacks and active-state
  restoration, or explicit source-backed proof that those effects are absent

This first kernel does not implement nonzero-offset/angle/ranged chase,
moveFurther=true, distancing, multi-target fanning randomness, immobilization,
casting interruption, flying/falling/swimming/transport movement, smooth/cyclic
splines, dynamic mesh updates, owner PathFinder cache behavior, >150-yard outdoor
Home despawn or Home's 400-speed shortcut. Those are missing branches, not
permanent restrictions on the full Classic goal. Caller admission must reject
unrepresented current inputs instead of treating absent metadata as false.

Backpedaling for the ordinary no-script, ground-player case is implemented.
Fanning with a proven empty eligible nearby set does nothing as source does;
unknown set membership is an error, and a real collider reaches the explicitly
unsupported random fanning branch.

### Ordinary slopes are supported by Classic

The spline itself supports arbitrary bounded 3D segments, including dry slopes;
the fixture includes rising and descending paths. When the explicitly selected
source NormalizeZ configuration is enabled, source normalizes each path vertex
through `UpdateAllowedPositionZ`; the pinned default is disabled. Source uses
the owner navigation filter and terrain/VMAP height/liquid queries, and retries
straight-line generation when its source checks fail. Path optimization is a
separate configuration whose pinned default is enabled; neither default is proof
of the current world configuration. Ordinary sloped ground is not a universal rejection.

Our existing raw-terrain/body provider's flat-only and dry-only vetoes are
validation gaps. Supporting ordinary sloped dry routes needs source-compatible
height sampling/normalization, continuous support across slope/triangle/cell
boundaries, body collision and coverage for the complete corrected route, and
liquid classification separate from polygon ground flags. Supplying those facts
can admit a slope without changing the source spline. This module neither
silently relaxes current providers nor claims a NAV_GROUND polygon proves dry,
walkable, collision-free support or LOS. Native owner PathFinder is still a
separate implementation obligation.

## Verification scope

`tools/creature_motion_oracle.cpp` is an independent C++ source-expression probe
for the explicitly named branches, with synthetic values and strict no-fast-math,
no-contraction compilation. It is not a full server build or a terrain oracle.
The committed 1095 rows cover source mixed-precision speed thresholds, float
integer precision boundaries, linear timing, duplicate/zero-length segments,
3D slopes and source position bits. Rust tests also cover cadence, source-phase
origins, replan, completed Home, rollback, snapshot/replay and notify lifecycle.
Linux oracle agreement is numeric fixture evidence, not whole-world parity or
independent platform-trig agreement. No Blizzard assets or database rows appear.

Spline identifiers are deterministic per-creature bounded counters (u32 range)
for this adapter, not the source server's thread-local global allocator. Their
producer identity and all route points are retained in launch events so a client
can project the actual spline/duration/facing instead of constructing a fake
single-tick line segment. Counter wrap is outside the admitted numeric domain.

## Constructor dispatch identity

Motion policy `cmangos-8ec338a1-ground-linear-motion-constructor-dispatch-v2`
models TargetedMovementGenerator.cpp:498–499 at the actual dispatch boundary,
after forced relocation and before the authority's straight gate. The required
`pathfinder_dispatch` Route option identifies the first actual dispatch and each
retained use; Chase retains its first query binding separately from polygon cache.
A skipped initial Chase has no constructor until a later dispatch. Retarget and
NoPath retain it; generator replacement/death do not. Home's distinct local
constructor remains explicit null in this seam. This is modeled lifecycle
metadata, never loaded query ownership or a successful route. Required-option
snapshot/request fields reject older missing evidence rather than default it.
See the runtime [constructor contract](../benilla-world-runtime/src/native/PATHFINDER-DISPATCH.md)
for live receipt, replay and loaded-lease boundaries.
