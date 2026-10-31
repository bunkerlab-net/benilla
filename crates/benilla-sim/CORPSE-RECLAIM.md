# Ordinary PvE corpse-reclaim call plan

`corpse_reclaim` ports the guards and delegated callback order in pinned CMaNGOS
Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`
`WorldSession::HandleReclaimCorpseOpcode` (Entities/MiscHandler.cpp), using
Player's reclaim-delay helpers and WorldObject's map/distance helpers.

**This is an unconnected pure component, not playable recovery.** It does not
release a body, apply a ghost aura, create/register/place a corpse, select a
graveyard, teleport, mutate player health, remove a corpse, publish packets or
save state. Native manual/automatic release remains explicitly unavailable.
Native entry and all private compiler/admission holds are unchanged.

## Inputs and source order

A checked policy requires explicit non-battleground and PvE-delay configuration
facts. Missing facts are not false. The caller supplies a player observation and
its own registered resurrectable PvE corpse, if any. No packet corpse GUID is
accepted: upstream reads but ignores it and looks up the player's registered
corpse. Object/owner and actual map-instance identities must be host-owned;
numeric equality does not authenticate a registry or a fresh observation.
Bones and PvP corpses remain unsupported under this ordinary-world policy.

The source guards are alive, not ghost, missing corpse, reclaim deadline, then
map/distance. Unreached later clocks/geometry are not inspected. Reached malformed
identity, unsupported corpse kind or invalid arithmetic produces a component
error, not an invented upstream rejection. `Location.map_instance` names the
actual map instance, not just its numeric map ID. Both objects must be in world.

There are two independent clock operands: the sample inside
`GetCorpseReclaimDelay`, and the sample used for the handler's deadline comparison.
No monotonic relation is imposed between them. The C++ expression alone does not
promise evaluation order; the oracle asserts its actual adapter sampling order,
while the Rust API accepts either ordering of observed values. Supported times
are nonnegative signed-64-bit seconds; signed overflow/unsupported time domains
fail closed. The delay calculation preserves source conversion:

`u32(expiration - min(delay_now, expiration)) / 300`, then index capped at 2 in
`[30, 60, 120]`. Disabling the PvE delay configuration selects 30, not zero.
The deadline starts at corpse ghost/release time, not body death time. Equality
at the deadline permits reclaim; delay is recomputed, not frozen from a prior
notification. Wrap before division is retained for representable observations.

Distance uses software binary32 round-to-nearest-ties-even through the existing
`creature_ai::Point`/`Real` domain. Source grouping is preserved: `dx*dx + dy*dy`,
then `+ dz*dz`, strictly less than `(39 + (0 + player_combat_reach))²`.
The corpse inherits zero combat reach; neither bounding radii nor LOS belong in
this test. Existing world-coordinate bounds (±32760), normal/zero binary32 and
bounded/exception-checked intermediate arithmetic apply. Unsupported subnormal,
nonfinite or overflowing operands do not become successful range checks.

## What a successful plan means

Guard success returns exactly these **unexecuted** calls, in order:

1. `ResurrectPlayer(0.5f, false)`.
2. `SpawnCorpseBones()`.

The plan contains player/corpse identities and is inspectable, not deserializable
as a live capability. Repeating the same observation repeats the same plan; it
has no exactly-once, history-authentication, ownership or persistence authority.
After actual host recovery, an observed alive player is rejected normally.

The whole resurrection callback is required. Its source body removes ghost/wisp
auras, unroots, restores resources, forces zone/aura callbacks, resets the timer,
updates visibility and invokes applicable instance hooks. Resource maxima are
read after ghost removal, and zone updates can change final resources: this
module deliberately does not promise final HP or replace callbacks with integer
halving. Ordinary reclaim's default sickness argument is false.
`SpawnCorpseBones` retires the registered corpse; visual bones and its player-save
callback are conditional. The returned call must not be replaced by merely
showing bones or assuming a successful save.

## Verification and next integration prerequisites

`tools/generate_corpse_reclaim_fixture.py PINNED_SOURCE_DIRECTORY` verifies exact
source SHA256 values and extracts unchanged handler, delay, distance and map
bodies. A synthetic C++ harness compiles with warnings denied and UBSan; callback
adapters assert half/no-sickness resurrection followed by bones. The 355-row
original TSV contains no game data. It covers source guard combinations, clock
samples/delay boundaries/uint32 wrap, strict float neighbors, vertical/diagonal
range, instance mismatch and out-of-world observations. Rust tests compare every
result and call order and exercise malformed/unknown operands and lazy guard
precedence. This is differential component evidence, not a full-server run.

Before a native caller can use the plan, implement and independently validate:

- Actual release ordering (`JUST_DIED` manual release can invoke `KillPlayer`
  immediately), full ghost spell/aura/movement effects and stay-alive quest failure.
- Generation-owned corpse creation/registration, placement, persistence and
  retirement, including failed creation and conditional bones.
- Source-owned map/position/reach and graveyard/teleport dependencies.
- Whole resurrection aura/resource/zone/visibility callbacks and their ordering.
- Repeated-life native restore/replay and atomic exactly-once controller/store
  publication; never weaken the current first-life proof to manufacture a ghost.

Only then can normal UI release → ghost traversal → reclaim → continued play be
verified. None of those integration milestones is established by this component.
