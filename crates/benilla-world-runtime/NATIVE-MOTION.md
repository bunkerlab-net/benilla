# Transactional source NPC motion

This optional native-format-10 component connects
[`creature_motion`](../benilla-sim/CREATURE-MOTION.md) to the runtime, durable
journal/reopening, and client/controller projection. It is a bounded intermediate
toward full Classic, not a completed ordinary combat loop or production admission.
The existing authored endpoint/segment policy remains a separate `source_motion=None`
branch. Neither mode constructs `NativeCapability`.

## Current represented state

`MotionGenesis` explicitly captures source initial run/walk rates, static movement
settings, control/script/transport exclusions, initial active state and orientation,
Home callback exclusions, detection range and AI-notify configuration. These supplied
fields are inputs, not a claim that raw database rows were inspected. Unknown evidence
is rejected. The current slice contains one NPC and one player; additional participants
and unknown visitor coverage fail closed, rather than becoming a proved empty set.

Private staged health, maximum health, actual damage-aura holders, current target,
combat/death phase, active state, pose and accepted player movement flags determine the
current update. A spatial provider cannot override those values. The admitted spell
component represents direct/periodic damage only, not roots, slows, NPC casts or scripts;
unsupported future mutations require an explicit extension. Captured initial player
auras with any nonzero raw AuraInterruptFlags are rejected. `Player::SetPosition`,
`Unit::DealDamage` and `RemoveAurasOnCast` remove even passive holders for movement,
turns, DAMAGE/NON_PERIODIC_DAMAGE and ACTION/ACTION_LATE; those removal callbacks are
not yet implemented. This conservative restriction belongs to this adapter, not
Classic. The current damage-only player actions cannot add player holders; any
future holder-producing operation must apply the same guard until removal is ported. No raw-input compiler changes are included.

`Npc.ai` is the sole authoritative victim/combat/death lifecycle. `Npc.source_motion`
owns map pose/orientation, spline, lastMoveTime, active state, combat-start orientation
and notify clock. Every current movement command is bound to the working AI. Opaque
kernel tokens commit completed source map relocations to AI; external authority cannot
serialize or submit one. Checkpoints reconstruct tokens from validated source state
and replies, and retain full path evidence and corrected launch origin.

`Command::MovementPose` atomically validates pose and source ground flags. Turning,
walking, running intent without displacement and stop remain distinct. Source mode
rejects old `Move`/`Pose` commands; the controller never infers IsMoving from traveled
distance. Legacy movement input behavior remains isolated. Existing cast-displacement
policy has not been replaced by movement flags: source Spell::update compares the
prepare position with current XYZ; generic movement processing does not immediately
cancel every cast. Source movement/turn aura removal is a separate missing branch.

## Ordered frame and lifecycle

Pinned source is CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`.
Source entry points supplement the kernel provenance:

- `Map.cpp:743–751,870,995–998`: sessions, player update, then collected creatures
- `Unit.cpp:490–531,10923–10954`: event processor, spells/auras, spline, motion, AI,
  combat manager; notification visitor
- `Player.cpp:6031–6039`: translated or orientation-only relocation notification
- `Unit.cpp:8534–8559,9908–9935`, `MoveSplineInit.cpp:122–170`: death stops,
  separate Stop versus InterruptMoving, new stop spline ID without map relocation
- `Creature.cpp:705–777`: corpse updates versus DEAD/respawn; the branch is selected
  before Unit::Update, so a player-phase death differs from an in-Unit DOT death
- `Unit.cpp:4428–4431,8745–8754`: SetInFront before melee, orientation only
- `SpellDefines.h:365–366`: movement/turn aura interruption flags

Player initial AddToWorld schedules a zero-delay event. NPC initial add uses configured
creature aggro delay. A player visitor never enables NPC CanAggro. NPC due events set
CanAggro from current alive state, then request the actual visitor. Radius is computed
in software binary32 as `max(uint32 detection,45) * aggroRate`; player detection is 18.
This is not map visibility distance. Relocations schedule their respective Unit's
notification, including player turns. Pending events are not postponed; entering combat
clears initial inhibition and aborts its pending event as source does.

Source player-phase combat callbacks apply synchronously while each health/pose witness
is current. CasterHitTargetWithSpell runs before health damage, including the supported
resisted-hit branch: first chase launch uses pre-damage health, then later replans use
current wounded health. An explicit combat-timer receipt refreshes pursuit even on
zero-damage/resisted reattacks. Raw per-spell attribute/implicit-target/aura flags gate
this branch; the internally resolved directional CanAttack uses current tracked
reputation when an interaction catalog is present. Directional caster detectability
remains its own exact query and is never replaced by generic LOS. NO_THREAT and
NO_INITIAL_THREAT/THREAT_ONLY_ON_MISS are evaluated; a damage-only transition needing
later AI victim selection remains explicitly unsupported. Player/NPC CanEnterCombat
uses captured raw permission and current Home state. Stealth/PvP, interrupted holders,
combat without an AI victim and a detected dead-caster callback are rejected.
Source release EVADE and direct-impact evade absorption remain unsupported and are
rejected from private AI state, regardless of an external allowed=true spell reply. NPC's own notification runs before its periodic aura update. The source
spline then consumes exactly one Unit diff, followed by motion generator and AI.
A player/event-phase launch binds the current map clock without consuming the creature's
later diff twice; a post-AI launch consumes zero elapsed time until the next update.
SetInFront's bearing is resolved and committed before a possible mainhand callback,
including AI ticks without a swing. It does not relocate, update lastMoveTime or schedule
notify. Native melee queries bind this ordered source orientation when available.

Death uses StopMoving(true): emit a newly identified stop at computed spline XYZ but
retain the less frequent authoritative map pose and lastMoveTime. Pending notify remains
and corpse Unit updates can execute it with CanAggro=false. Corpse removal relocates to
spawn and schedules notify without changing lastMoveTime. DEAD skips Unit event time;
respawn force-replaces the pending notification and does not advance that Unit clock on
the respawn tick. The shared map clock continues throughout. Source Home completion
restores active state and clears captured combat-start pose; unsupported addon, faction,
script and summon callbacks are explicit exclusions, not silently invoked successes.

The entire world, hooks, RNG, source queries, input sequence and event stream are one
clone-and-commit transaction. A late missing route, unknown visitor or bad bearing leaves
all of them unchanged. Strict restore checks bind motion identity/clock/map pose, current
AI command/target, combat-start position and Home active-state invariants. AI checkpoint
version 4 records whether death occurred inside the selected ALIVE Unit branch; the
legacy adapter rejects the new pre-Unit death flag rather than adopting source timing
through an edited checkpoint. Rehashed malformed clock, flags, notify and phase claims
are rejected.

## Player event ordering

Player notifications and SpellEvent now share the source EventProcessor queue,
including equal-key insertion order, delayed callbacks, cancellation, current-slot
references and retained finished spell objects. See [the player event contract](PLAYER-EVENTS.md)
for the bounded host phases, durable owner/cast binding and explicit teardown.
NPC notifications remain within the reviewed creature motion lifecycle; their local
clock freezes in DEAD and survives respawn independently of visible generation IDs.

The explicitly selected [combat semantic host](COMBAT-AUTHORITY.md) supplies
source-derived NPC and ordinary melee relationships/lifecycle/movement facts.
Unselected synthetic profiles retain the older authored semantic replies.

## Geometry and projection limits

Routes, near points, canonical bearings and complete visitor membership remain explicit
source-compatible services. The bounded dry Detour provider is not an owner PathFinder,
liquid accessibility or LOS implementation. No slope restrictions have been relaxed;
see the kernel's distinction between Classic rules and temporary provider validation gaps.
Test geometry/trig is labeled synthetic. Cross-platform independent trig parity is not
claimed from Linux-only tests.

The authoritative event retains the full source path, spline ID, duration, facing
and walk/run state. Client launch presentation reproduces source WriteLinearPath:
short endpoint offsets are filtered, remaining offsets are truncated and packed
to quarter-yard 11/11/10-bit fields, then decoded. Monster-move Runmode is always
set as the source packet does; creation retains the raw walk/run flag. Stop records
retain the newly allocated source ID. Periodic map relocation does not generate
a fake two-point one-tick movement. Creation/reopening restores the active full path and elapsed
time from the validated snapshot. Source callbacks are projected before deferred
NPC value fields; launch/stop before same-frame corpse removal is preserved using
chronological incarnation ownership, and respawn requires a fresh GUID. SetInFront does not manufacture a packet that source
would not send. The existing renderer's create-spline API lacks exact per-segment timing
and final-facing playback; it remains presentation only and cannot provide authoritative
movement facts. Source event/state/replay bits remain independent of renderer interpolation.
