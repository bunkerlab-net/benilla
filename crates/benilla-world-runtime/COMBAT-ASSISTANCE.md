# Selected combat-entry assistance

This component owns a bounded `Unit::SetInCombatState` suffix from CMaNGOS
Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. It composes with selected
ordinary visitors and EventAI; it is not a receiver-dispatch implementation or a
loaded entry-6 scene. Production native admission remains closed.

## Source order

`Unit.cpp:7972–8013` invokes the creature AI's EnterCombat callback before it
sets CanCheckForHelp false. It schedules a UnitLambdaEvent to re-enable that
flag at the existing Unit EventProcessor's current time plus the configured
aggro help delay, then calls `TriggerAggroLinkingEvent`.

`Unit.cpp:559–582` calls `Creature::MarkCallAssistanceOnPull` synchronously,
then schedules another UnitLambdaEvent at the same delay. The first event is
inserted before the second. `Creature.cpp:2178–2197` saves the previous
AlreadyCallAssistance value, sets the latch true, and collects assistance
receivers when CanCallForAssistance allows it. Positive template CallForHelp
overrides the family-assistance radius. Ordinary sight visitation is not this
query: the callback, radius, state and predicates differ.

The captured boolean is literally the previous latch value. The constructor
initializes it false; CombatStop resets it false. Thus an ordinary first entry
still performs the receiver traversal and schedules both events, while the
captured false suppresses subsequent assistance receiver dispatch. This
source behavior must not be replaced with its negation.

At execution the second event resolves the captured enemy GUID and checks the
owner's current combat state before the linking/group/assistance suffix. The
captured enemy and original entry occurrence remain distinct from a later
creature generation or a new combat entry.

## Selected boundary

The assistance v2 selection binds each spawn/entry to its generation source,
positive raw CallForHelp, aggro delay and complete exclusions for unowned
linking/group hooks. ExtraFlags is the existing bound source field, not a
caller-supplied CanCallForAssistance boolean. Unsupported radius fallbacks,
hooks and receiver closures reject rather than acquire defaults. The existing
profile flag mask still excludes NO_CALL_ASSIST; its source-oracle/scalar controls
do not widen profile admission.

A provider supplies explicitly authored complete ordered grid and world creature
lists plus explicit outside-coverage IDs. These lists partition the request's
exact current-generation scene inventory. Missing, duplicate, foreign, omitted
or stale identities reject; the self-only legacy variant is valid only for a
one-owner inventory. Returned grid order precedes returned world order, and
neither list is sorted by spawn key. Real source cell membership and linked-list
order are not inferred from a profile, position or update permutation.

For each visited candidate the runtime owns the first rejecting predicate:
self identity, outdoor in-combat state, owner UNINTERACTIBLE, candidate-to-owner
friendship, then candidate-to-player CanAttackOnSight. These use retained source
fields and the exact current scene, not provider-supplied eligibility booleans.
Same entry is not self. Defensive AI/NO_AGGRO_ON_SIGHT is not an assistance
predicate. A neutral candidate can pass generic CanAttack due to the player's
reverse hostility yet fail IsEnemy, denying assistance before range or LOS.
A hostile Defensive candidate continues to the strict source range check.
A range rejection needs the current caller/candidate poses and combat reaches;
a passing range still requires the separately supported LOS/receiver continuation. No alive or CanInitiateAttack guard is invented for this path.

The request retains raw help radius separately from Cell visitation radius:
`CellImpl.h:54–77` adds owner bounding radius and caps at
MAX_VISIBILITY_DISTANCE (`ObjectDefines.h:32`), which is SIZE_OF_GRIDS
(`GridDefines.h:38`, 533.33333f). This numeric bound is not map traversal proof.
Runtime-derived ordered denial receipts are retained alongside the authored
membership evidence. History validation binds those entry-time receipts without
re-evaluating them against a later creature generation or combat state. Invalid
evidence rolls back preceding EventAI actions, RNG draws and queue allocation.

The assistance events live in the same NpcEvents EventProcessor as ordinary
notify and spell events. There is no parallel clock or queue. Combat entry reached inside that NPC's active notify callback schedules on the
actual borrowed EventTransaction. Its clock and insertion allocator remain the
Unit's owners; the temporary empty queue is never a scheduling authority.
The entered NPC's own queue is used when it is the visited peer instead.

## Visible-peer help prefix

`UnitAI.cpp:68–101` invokes CheckForHelp for a creature currently in combat,
after the observer's outdoor active-victim and raw neutrality returns, and before
the Defensive reaction return. Defensive does not itself suppress help.

`UnitAI.cpp:495–525` first calls the combat peer's getAttackerForHelper.
`Unit.h:1239–1248` selects GetVictim before the attacker set. The selected runtime
supports the validated active SOLO_UNIT attack victim, including the interval
before ThreatManager chooses its cached victim. A retained target GUID after
victim detachment is insufficient. General attacker-set fallback remains
unsupported even if a later predicate could reject the help attempt.

The runtime reads the current observer and combat-peer owners in source order:
observer combat, observer/peer CanCallForAssistance, observer/peer CanCheckForHelp,
peer retreat/confused/stunned, observer CanInitiateAttack, then observer
CanAttackOnSight against the resolved player victim. Missing selected assistance
state is an error. CanCheckForHelp is the actual retained flag changed by Unit
queue callbacks; it is never recomputed from world time or the nominal delay.
The ordinary source control closure continues to exclude external control writers.

Runtime-derived denial and positive-assistance receipt options accompany the
peer-sight disposition. A denial occupies the help option; a successful attempt
occupies the assistance option. Both are null only for pre-help returns.
Structural receipt checks
bind owner IDs, selected source and player victim; replay regenerates the actual
callback and rejects a false null, substituted denial or changed live flags. This
is not standalone authentication of an arbitrarily rewritten checkpoint.
A denied help attempt resumes MoveInLineOfSight: Defensive returns NonAggressive,
while Aggressive still runs its ordinary initiation and NPC/NPC relation checks.
The selected positive continuation resolves accessibility from the player's
cached IsInWater owner and the helper's current CanSwim/CanWalk/CanFly owners.
It then asks for the exact victim-visibility, helper-to-peer map membership and
ignore-M2 LOS predicates in source order. Range arithmetic is runtime-derived
from current poses and both combat reaches, with a strict binary32 comparison.
The player visibility and map-membership facts remain explicitly authored in
component tests; loaded geometry does not prove either fact.

After CanAssistInCombatAgainst succeeds, the engine executes source AttackStart
against the resolved player, including EventAI combat entry and Unit assistance
queue effects, before the next visitor. The resulting proof retains the exact
callback, current generation/model owners and evaluated predicate answers.

The positive source chain tests helper-victim accessibility and visibility,
then observer-to-combat-peer distance and LOS (ignoreM2Model=true), then
CanAssistInCombatAgainst. This differs from pull-assistance candidate traversal,
where CanAssistInCombatAgainst precedes distance and LOS. Its distance parameter
is CONFIG_FLOAT_CREATURE_CHECK_FOR_HELP_RADIUS (default 5), not the creature's
CallForHelp or family-assistance radius. A negative prefix does not consume or
invent evidence for any unreached geometric predicate.

## Physical Unit lifetime

CombatStop resets the assistance latch; it does not cancel pending help events
or reset CanCheckForHelp. `Creature::Update` advances Unit events during CORPSE
and freezes them during DEAD, including the respawn branch's tick. Respawn
retains the physical Unit processor and previously captured payloads. A new
public generation handle does not authorize deleting old-life events. Unit
teardown destroys pending lambda events without executing their functions.

NPC event snapshot policy v4 requires the nested traversal receipt option.
Selected state, payload identity, queue order and retained history must survive
checkpoint restoration and replay together. An independent required NPC anchor
counts and chains assistance transitions, so deleting completed history cannot
make a used owner look freshly initialized after its queue drains. These
redundant consistency checks do not authenticate a wholly fabricated checkpoint.
Native format 51 and policy
`benilla-native-home-wander-v51` bind the changed state; native50 and older
experimental identities reject without migration.

## What this does not establish

A component scene can combine byte-backed dry habitat-3 initialization, its
CanSwim/filter ownership, generation-owned melee sheath, Defensive sight,
hostile-spell EventAI and these selected assistance events. Its model data,
combat inputs, text/cameras and complete ordered candidate traversal remain
explicitly authored. Wet movement, route authority and historical SWIMMING
continuity retain their independent restrictions.

The selected catalogs support shared template rows and independent per-spawn
owners up to the existing MAX_NPCS resource bound. Shared entry EventAI rows
produce separate holders and occurrence counters; GUID rows stay per spawn.
Current entry-keyed spell and interaction closures reject differing effective
spawn overrides rather than silently merging them.
Nonempty delayed assistance dispatch, general attacker-set fallback, group/linking behavior and the
24 base random-moving entry-6 spawns are outside this component. Actual client
models/terrain, complete source/loader-order capture and the closed production
admission boundary are still required before normal solo startup can use real
NPCs. A pinned C++ oracle with authored traversal controls proves the source
mechanism, not actual map membership.

## Executed component acceptance

The dry byte-backed fixture constructs seven independent same-entry owners with
separate cold-entry, update and authored grid/world traversal orders. It enters
two owners into combat through hostile projectiles, retains the first owner's
entry-time neighbor denials after the second owner's combat transition, and
checks checkpoint restoration and journal replay. Eight rehashed multi-neighbor
payload mutations reject; missing, duplicate, omitted and stale traversal
answers roll back the whole entry frame, including text/RNG/queues. A failed
seventh dry initialization retries with the same successful draw sequence.

These authored positions stay inside the actually loaded fixture tile. The
fixture does not supply Northshire terrain, imported spawn membership, actual
client models or a real-time performance benchmark. Unsupported helper-victim
fallback still rejects explicitly. Neutral entry-6 neighbors keep their source
denial against the player; positive help requires a genuinely hostile template.

The visible-neighbor acceptance uses actual initial AiNotify events scheduled
before, at and after the combat owner's actual queued help re-enable deadline.
At the equal deadline, the first idle owner reads the still-disabled combat peer;
later owners read its re-enabled flag after that peer's consecutive Unit update.
Both directional callbacks execute. The six idle owners retain independent
Defensive state and deny help through the source prefix rather than suppressing
visibility. A late last-owner receipt failure rolls back earlier callbacks and
re-enable/linking changes; retry matches clean execution. Ten forged receipt
variants fail replay, including wrong null help and a substituted helper victim.
Complete replay and checkpoint compaction/continuation preserve the result.

## Source oracle

The [independent C++ oracle](../../scripts/native_assistance_source_oracle/README.md)
extracts and compiles pinned source blocks with declared scalar/receiver shims.
Its positive and negative guards distinguish source event/lifetime behavior
from authored geometry and receiver membership. Runtime scalar comparisons and
the dry-scene component loop exercise separate acceptance surfaces.

The positive-help component uses a separate authored hostile Defensive helper
(entry 990006) beside six unchanged neutral entry-6 owners. Its full initial-pull
traversal rejects that helper at the smaller pull radius. After the caller's real
help re-enable event, the helper's own initial notify reaches CheckForHelp and
joins the player fight. Its independent EventAI holder emits speech and consumes
its two source RNG draws; the assistance history binds the exact PeerHelp
callback digest. Subsequent ordinary updates perform melee, then a targeted
lethal spell drives corpse and fresh dry respawn. Checkpoint restoration and
complete journal replay retain that lifecycle. A late final-owner failure rolls
back the join, text, draws and queue changes, while ten forged positive receipts
and three negative dependency controls reject as specified.

This positive scene executes OwnerToVisited. A genuine later-relocation scene
for positive VisitedToOwner remains unexecuted. Separate real own-notify combat
and low-level source callback-entry tests prove borrowed queue scheduling,
including zero-delay insertion/execution order; they do not establish that
missing reverse-direction scene. The component's visibility and same-map answers
remain authored; helper-to-peer ignore-M2 LOS uses the loaded fixture geometry.
