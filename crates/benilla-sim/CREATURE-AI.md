# Bounded Classic solo creature control

`creature_ai::SoloCreatureAi` is a GPL-covered, engine-independent control kernel
for **one ordinary outdoor creature and one authoritative player threat reference**.
It does not activate the native-world gate, replace the existing original fixture
runtime, execute melee damage, or establish a playable starter zone. There are no
Blizzard rows, client assets, database dumps, fixed-damage attacks or teleports here.
The compatibility identity is `cmangos-8ec338a1-outdoor-solo-ai-binary32-rne-v1`.

## Actual upstream selection and accepted domain

Pinned source is CMaNGOS Classic commit
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, GPL-2.0-or-later; this Rust translation
uses GPL-3.0-or-later. Source identities and notices are in [PROVENANCE.md](PROVENANCE.md).

`FactorySelector::selectAI` consults scripts, pet/charm/totem/guard/AIName overrides,
then registered permit scores. Ordinary creatures select **CreatureEventAI**:
its `Permissible` returns reactive/proactive scores, above NullAI's idle score.
CreatureEventAI inherits CreatureAI and UnitAI; with no applicable events it uses
the common combat path translated here. **Empty AIName/ScriptName alone is not
proof of empty events.** NullAI overrides combat methods with no-ops and must
never be substituted for this profile.

A compiler must establish all of the following before constructing the profile:

- Ordinary uncharmed, unsummoned, ungrouped outdoor creature; one ordinary player
- Default EventAI with proven empty applicable events, no ScriptDevAI override
- No creature spells, addon auras, combat scripts, assistance/call-for-help, linking,
  spawn groups/pools/game events, temporary faction changes, pet/owner hooks
- Ground/dry movement, no transport, flight, swimming, stealth alert, CC, fear,
  root, fleeing/backpedaling/fanning/special movement, ranged attacks or combat AI
- One main hand, no offhand, extra attacks, next-swing spell or parry haste
- Idle default movement; native random wandering/waypoints remain unsupported
- Resolved normal native level, reach AND bounding radius, detection/rate, reaction,
  neutral-to-all status, run/home speeds, effective attack interval, pursuit/leash,
  corpse duration, random-selected respawn seconds and a shared clock origin
- Complete validated relationships, visibility, attack eligibility and geometry

`Profile::unsupported` carries a failed audit, not a capability override. Its
empty value is **not evidence** that absent metadata is safe. The native compiler
and runtime gate must independently enforce these prerequisites. `Unsupported`
variants and errors never guess defaults or silently drop unsupported behavior.
Passive sight/retaliation suppression is supported; damage-driven passive combat
is explicitly rejected. Periodic threat requires existing combat. Multi-reference
threat, offline-list recovery, taunt, fixation and suppression are not implemented.

## What is translated

- `UnitAI::MoveInLineOfSight`/`DetectOrAttack`: aggressive-only acquisition after
  the accepted visibility/relation/accessibility/LOS predicates; neutral-to-all
  suppression; inclusive 3D raw-center aggro radius
- `Unit::GetAttackDistance`: the exact level-difference clamp, zero handling,
  five-yard floor and rate multiplication; no detection auras or player-pet factor
- `UnitAI::AttackedBy`, `CreatureAI::AttackStart`: defensive/aggressive retaliation,
  combat-start/pursuit setup and a genuine chase command
- `ThreatContainer::selectNextVictim`: separately callable ordinary-player
  110% melee / 130% ranged threshold branch; the state machine is solo-only
- Source float threat accumulation from already-resolved positive threat receipts;
  periodic damage does not refresh pursuit; direct incoming damage does
- `Unit::Update` timer → validated movement → AI → combat-manager ordering
- `Unit::CanReachWithMeleeAttack`: strict **2D** range for this uncharmed NPC domain,
  combined native reach + 1.33f, minimum five yards, supplied native running leeway
- `Unit::UpdateMeleeAttackingState`: single main-hand readiness, cast/initiate gates,
  range/facing/alive/attack branches, one swing per update, no catch-up burst
- `CombatManager`: post-AI timer decrement, its initial/once-per-update 1200ms tick,
  refreshed-position pursuit leash, template hard leash, ten-second unreachable
  timer and cancellation when the authority proves reachability again
- `UnitAI::EnterEvadeMode`/home completion: clear combat/threat/loot/aura effects,
  validated travel to the saved combat-start position, no instant heal or home warp;
  the >150-yard outdoor forced-despawn home branch explicitly returns unsupported
- `Creature::SetDeathState`, `Update`, `RemoveCorpse`: corpse → invisible/despawned →
  respawn ordering, separate corpse and respawn deadlines established at death,
  invisible relocation only after corpse removal, next-update respawn

Pinned quirk: `SWING_ERROR_NOT_IN_RANGE` is enum value **zero**. The source checks
`if (swingError)` before resetting to 100ms, so out-of-range attempts retain their
ready timer. Other swing errors retry in 100ms. The port preserves this rather
than quietly fixing it. `melee_decision` returns an explicit decision, not the
upstream boolean whose meaning is "nonzero swing error".

Leashes use the source's **default bounding-radius** `GetDistance2d` overload,
not center distance. It computes binary32 dx²+dy², global binary64 `sqrt`, subtracts
this object's radius in binary64, converts to binary32 and clips at zero. The
integer/software implementation preserves these operations. It does not replace
that sequence with squared-distance algebra.

The logical map clock starts at `Profile::clock_origin_ms`, shared by the world.
Corpse expiry adds milliseconds; source `time(nullptr)` respawn arithmetic first
truncates the shared clock to a whole second and then adds resolved seconds.
Persisted wallclock jumps, unloaded grids and restart reconciliation are outside
this kernel. It cannot independently advance an offline world.

## Effects, observations and authority boundary

`CombatAuthority::observe` supplies sequenced `Observation` values and monotonically
increasing authenticated combat receipt IDs. Types deliberately have no wire
Deserialize implementation. The adapter must establish ownership, target identity,
movement, metadata and combat eligibility from its own state. **A PSK-authenticated
peer claiming damage, LOS, position or a successful hit is not that evidence.**
Input failure leaves the entire AI state unchanged. World effects must likewise
be committed transactionally; observation and movement-proposal adapters must not
mutate the world before acceptance. Determinism assumes identical authoritative
observations and step boundaries, not arbitrary reordering or variable frame rates.

A `MainhandSwing` is a request for native hit/damage/proc resolution, **not damage**.
That executor is still required; source callback ordering must be preserved when
integrating it. Outgoing pursuit refresh uses an authority-validated post-movement stationary
creature/victim predicate. It is applied inside the swing branch, before combat
manager decrement, exactly where `AttackerStateUpdate` calls its reset helper.
It is not a next-tick receipt or a blanket "every outgoing damage refreshes" rule. The damage executor must use `is_evading`
to honor immunity, and reject damage while returning/unreachable; this state machine
rejects damage receipts during those states. `is_evade_regeneration` exposes the
source flag. The native runtime applies the separate [ordinary creature health](CREATURE-REGENERATION.md) kernel after this update; reset-entry effects remain external.

## Movement adapter contract

`MotionCommand` identifies a persistent chase or home intent. Movement generators,
source near-point selection, spline timing/250ms replan cadence, native speed
resolution and geometry are separate responsibilities. `Profile.run_speed` and the
request speed are the unmodified native speed ceiling for this profile, not a
claim that actual progress always runs at that speed. The provider must apply
Creature::IsSlowedInCombat / the spline wounded-speed multiplier below 30% health
(unless the native NO_WOUNDED_SLOWDOWN flag excludes it), or return unsupported.
A displacement budget alone does not certify correct health-adjusted movement. No interpolation algorithm
or substitute navmesh is claimed by this module.

The adapter creates a `MovementRequest` with:

- Unique monotonic request ID and the exact active command/creature IDs
- Native map ID, native WoW-axis origin and a source-resolved goal
- Native yards/second speed and exact elapsed simulation milliseconds
- Content and geometry revision digests, chase target or home purpose

`GeometryAuthority::resolve` returns a bound `MovementOutcome`: unchanged IDs/map/
revisions, progress position, total travelled path length and arrival, or an explicit
`BlockReason`. Successful progress requires known complete loaded coverage, clear
LOS, clear body sweep, continuous supported ground and dry-ground status.
`resolve_movement` produces a privately constructed `ValidatedMovement` only after
checking finite values, revision binding, path-length/speed budget, no stationary
progress on a nonempty segment, and arrival exactly at its goal. The AI also checks
current origin, frame duration, native speed, active intent and home destination.
No public token constructor or raw claimed-peer-state API exists. These checks
cannot prove a dishonest authority's geometry; that is explicitly its trust boundary.

Known obstacles produce unreachable observations and native evade timing. Unknown
coverage/navigation is an error, **not an unreachable-path success**. A required
chase with no feedback fails the next step; home travel never completes without
validated arrival. The >150-yard raw-3D home return enters a distinct upstream five-second respawn /
delayed forced-despawn path; this kernel returns `LongDistanceHomeDespawn` there.
No stationary fixed-melee approximation, teleport-to-victim,
teleport-home, 400-yard/sec return or blind path shortcut is implemented.

## Mmaps are optional upstream; complete movement is not

Pinned `PathFinder::calculate` explicitly handles disabled/missing navmesh/query/
tiles by `BuildShortcut`, a two-point path, optionally height-normalized with
`UpdateAllowedPositionZ`, labelled NORMAL | NOT_USING_PATH. `mmap.enabled` defaults
to 1 but can be disabled. `ChaseMovementGenerator::DispatchSplineToPosition` also
tries straight paths for nearby (<200-yard), low-height-difference (<5-yard), LOS,
non-underwater endpoints before other pathfinding. `HomeMovementGenerator` includes
shortcut/failed-path high-speed behavior. These facts do **not** validate our geometry.

A useful future bounded provider can use existing Benilla `WorldCollision::cast_body`,
`ray_body`, loaded ADT/WMO/M2 collision and ground/liquid information to support only
fully checked unobstructed outdoor segments at native speeds. It must validate the
whole swept corridor and ground support, not just an endpoint ray; honor relevant
source LOS masks/heights; identify loaded terrain/model/collider revisions; and reject
missing geometry, obstacles, water, drops or unsupported slopes. Home travel uses
the same real provider. This stricter fail-closed adapter is an explicit restriction
of upstream's permissive no-mmap route, **not mmap/vmap parity**. Body/camera collision
masks in the current renderer are not by themselves a proven server LOS equivalent.

That provider and native near-point/spline adapter are **not implemented here**.
No new fake map-data source or native-enable bypass is introduced. This is the
concrete remaining movement integration boundary, not a requirement to ship mmaps
before any bounded outdoor play can be possible.

## Numerical and verification contract

All authoritative arithmetic uses the already-pinned `rustc_apfloat` dependency.
`Real` accepts signed normal finite binary32 values (canonical zero), magnitude at
most 1e12; fields requiring nonnegative values reject negatives. Positions are
bounded to ±32760 native yards. Float overflow/subnormal exceptions are rejected,
not clipped. Levels are 1..255, steps 1..60000ms, ≤64 receipts/step, one target.
Software square-root rounding uses exact u128 significand comparisons, no host FPU.

`tools/generate_creature_ai_fixture.py` hash-checks source bytes, extracts entire
upstream aggro/range/reach/distance/melee/target-selection bodies and the actual swing
enum, then compiles independent C++ adapters with FE_TONEAREST, no fast math/FMA,
and undefined/float-cast-overflow sanitizers. The fixture has 52,394 original numeric
vectors. Tests cover those outputs and deterministic state traces, retaliation,
DOT/direct threat, chase requirements, dead targets, pursuit/bounding radius,
unreachable recovery, home arrival, corpse/respawn sequencing and transactional
replay/unsupported/malformed movement rejection. Synthetic geometry tests exercise
the contract, not an actual terrain provider. No live-native or visual parity is
established, and no user-local DB/install was available to select a real starter mob.
