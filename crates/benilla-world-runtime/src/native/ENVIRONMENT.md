# Cached player environment component

`native::environment` is a shared, opt-in GPL-3.0-or-later port of the
CMaNGOS Classic revision
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f` player environment reducer.
Upstream is GPL-2.0-or-later, copyright CMaNGOS contributors. Source anchors:
`Player.cpp` SetEnvironmentFlags:1105–1140, Update:1483–1489,
SetPosition:6069–6075, UpdateTerainEnvironmentFlags:19113–19175;
`Player.h` EnvironmentFlags; `GridMapDefines.h` liquid bits;
`DBCStructure.h` LiquidTypeEntry and `DBCfmt.h` niii.
No client assets, genuine game rows or world database contents ship.

## Exact boundary

The component runs the environment phase **after an accepted relocation**.
It does not implement SetPosition coordinate/orientation acceptance, movement
interrupts, map relocation, group/trade/zone updates, or the subsequent
CheckAreaExploreAndOutdoor call. An `EnvironmentPose` is an already accepted
source-world pose/collision-height bit pattern; finite-value checks protect this
component's arithmetic and do not certify full SetPosition validity.

The immutable `EnvironmentQuery` binds the existing published TerrainInfo
snapshot (map, content, source epoch, product inventory and policy), the SHA-256
of complete LiquidType WDBC bytes, an exact sequence ticket, source binary32
position, all-liquid mask and collision height. `BoundLiquidResult` retains
status and full uint32 entry/type flags plus unchanged level/depth bits. Public
DTOs are inert observations: consistency checks, not authenticated producer
identity, provenance, a native-content compiler, or gameplay admission.

`LiquidCatalog` and the geometry reader share one bounded WDBC parser. All four
niii columns and the complete-file digest survive; an absent row is distinct
from row zero and a present zero-spell row. No selected-row constructor,
default SpellId or Deserialize catalog exists. The separate explicitly selected
[creature capability host](../../CREATURE-CAPABILITIES.md) retains raw InhabitType
and current capability bits for bounded ground-creature NearPoint requests; it
does not provide this component's player environment or accessibility feed.

## Source behavior

The source starts with zero flags, zero timer and a null last-liquid pointer.
Update only saturating-decrements the timer. Expiration while stationary never
queries or changes cached environment flags. Every accepted post-relocation
call checks the gate, including identical pose and rotation-only calls. At zero
it resets to 100 ms and consumes one exactly bound query; calls during the
cached window consume none. The cache retains its original query separately
from the latest post-relocation pose.

Classic liquid type bits are magma 0x01, ocean 0x02, slime 0x04, water 0x08 and
deep water 0x10. The corresponding environment bits are a different enum.
Only matching liquid kinds update WATER/MAGMA/SLIME/UNDERWATER flags, so a
cross-kind transition intentionally retains unrelated prior bits. Entry zero
bypasses lookup even when row zero exists, clearing a prior spell-bearing row
but retaining a remembered zero-spell row. Missing nonzero IDs are real absence.
A differing old row's spell removal precedes the new HasAura query, including
when both rows share the same spell ID.

SetEnvironmentFlags mutates flags before its callbacks. Its no-op guard tests
whether **any requested bit** already agrees; callback dispatch uses the
requested mask, not a changed-bit mask. The dry combined clear therefore calls
threat, interrupt removal, fatigue, breath and environmental scaling in that
order even when only the LIQUID bit was previously set. Water-breathing interval
is queried only on applying a changed UNDERWATER flag. High-liquid depth uses
binary64 multiply/add/comparison because the source's 0.75 literal is double;
there is no intervening binary32 rounding.

## Effects and atomic failure

The exact reducer represents ordered threat updates, interrupt/spell aura
removals, mirror scaling, HasAura/interval reads and self-triggered spell casts.
The standalone production callback host executes `SetMirrorScale` against actual
owned source MirrorTimer states. Every other operation returns its typed
`CallbacksUnavailable` intent. A staged transition that encounters an unavailable callback is discarded without changing flags,
refresh timer, last liquid, cache, mirror states, sequence, history tip or journal.
Merely recording a callback never applies its effect.
The private callback interface cannot be implemented by external callers; no
generic clone-based host promises rollback for shared external state.

Initial dry and above-water zero-spell observations, cached calls and refresh
timer phases do commit. Above-water deep-water transitions also execute the
source fatigue scale mutation, including restoring scale 10 when HIGH_SEA clears.
The loaded-product test obtains both observations from actual authored GridMap
cell arrays through the existing TerrainInfo query. It does not fabricate a
water predicate. The mirror states remain inactive until a future player timer
lifecycle starts them; scale mutation alone does not start fatigue or cause damage.
A dry exit after LIQUID was set requires the combined callback sequence and refuses.
This intentionally bounded production domain cannot import arbitrary wet state,
spell-bearing remembered rows, or active mirror states through an environment
checkpoint. A private `cfg(test)` fully owned synthetic host exercises the wider
reducer and staged host rollback; it is not the game's effect implementation.

The standalone component does not supply a live world-feed producer, NPC
accessibility admission, spell/threat authority, damage/drowning/fatigue
implementation, or playable-world claim. The explicit native owner below selects
its callback execution domain separately.
The original callback-free domain is unchanged; the native surface executor adds
a crate-private seam for the actual staged threat/aura owners. Remaining lifecycles
are follow-on engineering work, not implicitly denied permission. Concrete seams:

- Threat updates need the actual hostile-reference container lifecycle:
  HostileRefManager::updateThreatTables calls HostileReference::updateOnlineStatus,
  which consults Unit::IsOfflineTarget (successful feign death). Creature
  Unit::isInAccessablePlaceFor is a separate water/land accessibility path
- Aura interrupts/removal/HasAura need the real ordered holder lifecycle and
  complete interrupt/spell metadata; removal must affect later same-spell queries
- Mirror scaling now mutates owned persistent source timer state. Starting/ticking
  those timers in the player lifecycle still needs actual breathing configuration,
  multiplier/aura lifecycle, timer ownership and expiration-effect handling
- Triggered self-casts need genuine spell catalog and the source spell/event
  lifecycle; an emitted cast intent cannot substitute for execution

## Native owned cache

`Profile::initialize_player_environment` explicitly selects the source snapshot
and complete liquid catalog only after genuine loaded player model-height
ownership. The optional field has no dry fallback. `NativeWorld` owns private
reducer state, a monotone phase sequence, the last countdown tick, and the latest accepted full pose;
it does not embed `EnvironmentMachine` or accumulate a second journal.

`Frame::pose` executes the phase after accepted relocation and orientation,
including unchanged and turn-only inputs, preserving every pose in batch order.
Rejected or dead movement has no position phase. After commands and player
reactives, the frame decrements once before NPC processing, including empty
batches and dead-player updates. Expiry only zeros the timer. This bounded
ordering does not implement complete Map/Player::Update or SetPosition parity.

A due `Query::PlayerEnvironment` binds world, player, current profile and geometry,
source snapshot/epoch/inventory/policy/catalog, owned model-height witness,
phase sequence, before-cache digest, and exact current position/orientation.
`Facts::PlayerEnvironment` must echo the entire query and raw liquid result.
Native events retain each position phase's input sequence and one countdown
receipt. Existing staged native replay covers requests, facts, ordered receipts
and full state; later NPC/RNG/checkpoint failures discard the entire staged frame.

The original native callback-free domain accepts initial/repeated NoWater and
nonhazardous AboveWater/WaterWalk observations; its lifecycle guard and restore
remain unchanged. The separately selected native surface executor additionally
accepts ordinary InWater and dry exits when the actual staged NativeWorld adapter
owns threat and aura-removal execution. Its crate-private effects interface has
no public caller-provided successful host and no Clone-based rollback contract.
The environment reducer stages its own state only; callback mutations belong to
the outer native frame transaction. A typed CallbackFailed intent preserves the
failure boundary while the adapter retains its original native error.

Surface state permits only IN_WATER, HIGH_LIQUID and LIQUID flags. UnderWater,
magma/slime type bits, deep-water/high-sea, nonzero-spell rows, active mirrors and
spell/interval reads remain closed. The owned wrapper executes mirror scale
changes itself. A NoWater combined clear runs the unchanged source sequence,
including fatigue, breath and environmental scale 10 even from LIQUID alone.
Restore preserves those exact inactive timer mutations and the constructor
FeignDeath timer, requiring a phase sequence capable of the clear and any later
re-entry. The refresh countdown never starts or ticks a mirror: source
UpdateMirrorTimers at Player.cpp:1409 is a separate, earlier lifecycle. Test-only
callback probes establish reducer ordering, not production threat/aura effects.
No placeholder predicate or callback receipt substitutes for execution.

Chase route requests carry a frozen `PlayerEnvironmentTarget` dependency for the
last `!i_target->IsInWater()` clause of TargetedMovementGenerator.cpp:502. It uses
the cached IN_WATER environment bit, not a new sample at the current target pose.
The latest accepted full pose is checked independently of the historical cached
query. The sampling tick must match the route clock, with bounded sequence and
phase-specific timer state; an older same-pose cache cannot be transplanted into
a later route. Synchronous player AINotify/combat callbacks may start Chase before
the countdown. Witnesses bind BeforeCountdown/AfterCountdown against the owned
last countdown tick rather than moving that source callback or timer phase. Distance, vertical, LOS, owner-water and complete source dispatch remain
separate dependencies; this witness does not select straightPath or certify that
the complete Chase gate ran. Home and Backpedal carry explicit absence.

Native format 31 and `benilla-native-first-owned-pathfinder-cache-v31` include the
optional owned cache, required optional latest-pose and route-witness fields,
and v31 profile/state/record/checkpoint/melee domains. Missing fields and older
native checkpoints reject rather than fabricate initial state. Independent
component v2 encoding, exploration character data and raw native admission stay
unchanged. Hashes establish consistency, never historical authenticity; complete
journal replay is required to verify the supplied history.

## Owned surface callbacks

`initialize_player_environment_surface_callbacks` separately selects the bounded
surface executor after the cache source has been initialized. It requires the
complete source interaction/combat catalogs and actual player-slow host/catalog,
even when the live holder registry is empty. Source selection is part of the
profile digest and environment source policy v2. Native format29 rejects older
experimental saves; there is no inferred hostile-reference or aura migration.

The actual frame borrows its staged NPC references, player holders and events.
UpdateThreatTables updates every owned hostile reference against the source
IsOfflineTarget predicate (successful feign death and the NPC ignore-feign flag),
not water accessibility. The selected source closure excludes feign-changing
scripts; restore rejects unresolved or source-inconsistent native membership.
Manager current victim is separate from the AI/Unit target. Independent reference
updates commute in this single-player closed callback domain: they do not mutate
shared threat, select a target, run hooks or advance clocks. BTree iteration does
not claim general HostileRefManager front-insertion ordering. That ordering must
be owned before admitting callbacks with cross-reference observable effects.

HIGH_LIQUID entry and exit execute real default removal with masks0x80/0x100.
The holder multimap is scanned in spell order and restarted after each removal;
actual RUN/RUN_BACK/SWIM rates and persistent slot levels change before the
following environment receipt. Immutable initial passives must prove zero
interrupt masks. Tests use authored61004/61005 rows, not a claim about stock1604.
If any callback or later phase fails, the enclosing frame discards NPC membership,
holders, rates, slots, environment state, RNG, events and journal changes together.
No callback receipt or successful external predicate substitutes for this state.

## Independent persistence and evidence

The crate-private state reducers stage one phase and return owned state plus
a checked monotone sequence; they own no journal. The surface reducer borrows
only the selected native frame's effect adapter and cannot independently roll
back its effects. Initialization, query construction and validated snapshots
share the source reducer. The standalone `EnvironmentMachine` retains its
existing callback boundary, atomic journal limits and v2 encoding. Separate
surface restore cannot widen standalone restore: importing native wet state or
dry-clear mirror mutations into component v2 fails. Existing optional snapshot
fields must be present (explicit null is valid); omission cannot silently become
constructor state. Valid component v2 serialized bytes remain unchanged.

Component checkpoint/journal formats have their own version/policy and do not
change native world saves. Environment component v2 deliberately rejects v1
component checkpoints/journals rather than inventing missing mirror states.
Checkpoints retain exact owned mirror states, the refresh timer and cached query,
last observed pose, last-liquid ID resolved against the selected complete
catalog, selected world/player/snapshot identity, sequence and history tip.
Restore checks bounded canonical bytes, compatible identity, structurally
supported state, state digest and checksum. These checks do not prove that every
historical transition occurred. The immediate query/sequence correlation and
permitted mirror-state combinations are validated; full history still requires
trusted storage and replay. An attacker who can rewrite and re-hash an entire
checkpoint can invent self-consistent state. These are integrity checks, not
history authentication or proof of source data provenance.

Create a checkpoint, durably retain it through the caller's storage, then call
`discard_records` with that exact checkpoint. The component provides no storage
writer and cannot certify durable I/O. Replay checks the chain and re-executes
all phases through the same fail-closed production boundary, comparing complete
records. The whole replay batch is staged, so a changed tail cannot partially
commit. Local limits are 2,048 records and 4 MiB canonical journal bytes.

The [source oracle](../../tools/environment_state_README.md) extracts original
methods and timer/gate fragments from hash-guarded pinned files. Its 1,324 rows
cover cached gates, 99/100 ms, same pose, stationary expiry, selective retention,
row zero/missing/full-uint32 IDs, crossing liquids and spells, combined clears,
callback order and binary32 neighbors around the binary64 depth threshold.
Twenty lookup observations come from the original WDBC reader over complete
**authored** stores. Harness geometry facts and HasAura/interval answers are
explicitly authored dependencies; callback traces prove source invocation
order, not actual aura/threat/mirror/spell effects. The private owned-host test
separately exercises same-spell removal feedback and transactional failure.
Raw source-only cases include arbitrary uint8 flags and uint32 timers that
public production checkpoint admission intentionally cannot restore.

## Standalone mirror state machine

The independent [mirror component](MIRROR_TIMER.md) implements actual
source MirrorTimer state mutation, including start/stop, remaining/duration,
freeze/scale, pulse/regeneration and fetched-status reset. Its checkpoint/journal
is separate from the environment component. A returned expiration/pulse does not
execute damage, resurrect, cancel an aura, or send a packet. No Engine routing or
production admission changes accompany either component.
