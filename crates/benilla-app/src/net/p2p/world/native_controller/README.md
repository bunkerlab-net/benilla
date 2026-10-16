# Native app controller boundary

The selected-character app route uses the same `Driven` interface and real blocking
command/event driver for authored `World` and native `NativeWorld`. Native controller
composition is exercised only by private `cfg(test)` component entry. Production
`Request::native` requires the uninhabited `NativeCapability`; preparation reports,
serialized profiles, synthetic dependency features and existing synthetic snapshots
cannot satisfy it. The character browser still passes no gameplay request. This is
not evidence that a native Classic world can presently be played.

The native controller owns the exact authenticated profile selection, `NativeWorld`,
`NativeProjection`, exclusively locked `NativeStore`, and driver-local authority.
The app overlays world fields and raw binary32 world position before creating the
selected avatar. It never converts native position to the authored runtime's integer
coordinates, writes native state to the exploration profile, or starts peer transport.
The test entry distinguishes Create from Open and checks catalog/owner before a new
save is created. It contains no original game assets or permissive catalog fallback.

## Clock, durable publication and failures

Wall-clock elapsed time contributes debt for the admitted fixed tick interval. Each
pump executes at most 32 ticks and retains catch-up debt. Each tick drains up to 64
accepted local inputs in their arrival order before one simulation update. Inputs
cannot advance time. Movement shares this bounded FIFO; it is not reordered or
coalesced across casts and cancellation. Native movement remains decided by native
source authority. Authored speed clamping and its separate cast-movement prohibition
are never reused.

Player-movement requests carry the immutable frame-start origin, every previously
accepted endpoint in order, the candidate endpoint, and one elapsed frame budget.
There are at most 64 endpoints per frame. The authority must validate the cumulative
path length and each segment against that one budget; elapsed time is never renewed
per input, and backtracking consumes travel. Rejected candidates do not enter the
accepted path. The full path is part of the echoed authority binding and replay.
The production player-movement authority is still unavailable. Tests use an explicitly
synthetic path-budget oracle and do not establish native speed or collision parity.

The flat batch permits no nested commands and is bounded by 64 inputs and 64 KiB of
canonical input bytes. World, owner, selected character, tick, contiguous sequence,
and backward cast references are preflighted across the whole batch. Invalid
envelopes reject the batch before authority calls; valid gameplay rejections consume
their own sequence and do not discard later inputs. Empty ticks consume no sequence.
Authority, mechanics, capacity, or projection failures cannot partially acknowledge
inputs. A runtime failure leaves every component, RNG draw, sequence and journal
unchanged. Projection failure retains the committed batch without reapplying it.

The runtime versions its native snapshot/journal schema, policy and digest domains.
Tick records contain `inputs: Vec<Input>` and source-ordered per-input outcomes.
Older native saves are rejected, never migrated or relabeled. Authored World v2 is unchanged; the store's
outer native envelope delegates the schema check to the runtime. Format 23 includes
runtime-owned route owner/position/capability requests and creature capability source bindings alongside source spell range
receipts, backpack movement receipts and per-NPC event queues through durable
publication, retry and reopen. Tests verify a
due NPC notification is not dispatched again after a checkpoint failure.

Checkpoints run every ten simulated seconds or before native journal headroom is
exhausted. `NativeError::JournalFull` means only uncommitted journal-capacity refusal;
it permits one checkpoint and retry of the exact staged tick/batch. Other `Limit`
and single-frame `TooLarge` errors never enter this path. The store's checked receipt
acknowledges and clears the journal only after durable publication and replay checking.
Projection runs before acknowledgment, and committed inputs are retired before
projection. A projection failure cannot reapply a committed command on retry/logout.

Unavailable authority pauses with the exact uncommitted batch, tick, sequences and
journal intact. Only explicit `RetryTick` allows another attempt. Paused time adds
no debt. A definite pre-publication checkpoint failure permits explicit
`RetryCheckpoint`; uncertain/external modification remains blocked for explicit
recovery. Logout cancels only unadmitted intentions and saves the current boundary.
Failed ordinary logout keeps the world, journal and lock until explicit retry; it
never claims success or opens a replacement save. Process stop/output loss attempts
the current checkpoint but cannot retain memory after termination.

The existing local geometry provider remains independently gated by exact terrain
coverage and a private participants-domain proof. This controller adds no proof,
geometry fallback, renderer-thread blocking call, or synthetic production authority.
The provider handles bounded source-resolved NPC/player movement and explicitly
selected static source LOS. The selected source spell host derives range from the
raw range row, both combat reaches, current staged poses and executing cast phase.
NPC visibility/accessibility, spell impact visibility, melee and other spatial
facts remain unavailable. Service installation and an admitted source compiler
remain required before production entry can open.

## Creature capability requests

An explicitly selected source capability catalog binds each spawn to its raw
effective creature entry, actual `InhabitType`, requested/effective spawn-template
row and complete source exclusions. Missing rows or exclusions cannot become
empty/default lookups. The bounded runtime accepts actual habitat `1` and the
supported RUN spawn flag, with no addon, script, control, entry-substitution or
other capability mutation. Habitat `3` still has source `CanWalk`, but also permits
swimming and is rejected by this ground-only execution boundary. Habitat `1` does
not establish dry terrain at the creature's current position.

The runtime owns only the capability-relevant current movement bits and derives
`CanWalk`, `CanSwim`, `CanFly` and the allowed-position-Z branch from those bits
and the retained raw habitat. Callers cannot set these predicates independently.
An actual source chase and moving-target replan wrap the original NearPoint query
with the current spawn generation and source-bound capability witness. The outer
authority receipt retains the profile, world, tick and geometry bindings; the
inner spatial receipt must still echo the original query exactly. Unselected
component fixtures keep their previous request path.

Capability-specific tests answer these requests with an explicitly authored
spatial authority. Separate primary NearPoint tests use actual loaded authored
height/VMap products and the bounded provider through a crossed-thread local
mailbox, then exercise controller projection, store publication and receipt replay.
Other authority queries in that fixture remain synthetic. The tests do not exercise
the Bevy service scheduler or establish actual installed-world content, complete
NearPoint fallback, OwnerPathFinder routes or native content admission.

Tests cover exact-request replay, movement/replan, failed-authority rollback,
non-retryable mismatched echoes, failed-save publication followed by one retry,
and reopen. Missing/mismatched catalogs or water-capable rows cannot create a
world; changed or removed selected catalogs cannot reopen or rewrite its durable
save. Source spell death, corpse removal and respawn retain the catalog binding
while rebinding the runtime witness to the new creature generation, including
corpse and respawn reopen boundaries.

## Initial creature collision height

The explicit initial-height option derives only from the retained, approved
initial-model width source. The original Plan receipt selects the native display
and object scale; the same immutable MPQ-loaded `CreatureCatalog` supplies the
actual `CreatureModelData` field 15. Height is that positive object scale times
raw field 15, with the source's exact-zero fallback `2.03128`. Radius, width,
display scale and model/render scale cannot substitute for height. Initialization
atomically installs the height catalog and derives the NPC motion-policy scalar;
a later independent scalar change invalidates the selected profile.

Generation-zero NPC state owns both source-bound width and height witnesses. The
real controller Chase and moving-target replan freeze both in the typed Route
request; height validation checks the shared model source, profile, generation
and exact `owner_collision_height`. Native format 33 retains the witness through
checkpoint, journal replay and reopen. A missing schema field fails to decode;
explicit null denotes an unavailable dependency. Initial receipts survive corpse
reopen but are cleared at respawn, and relabeling one cannot authorize a later
creature generation.

Authored controller tests resolve the real Plan receipt and MPQ/DBC chain, then
exercise store publication, exact staged-batch replay, failed-authority retry,
failed-save retry and reopen. Forged scalar, model, source, profile, generation or
missing dependency echoes cannot mutate, persist or publish a tick. Changed or
stale catalogs cannot reopen or rewrite the saved world. The successful spatial
answers remain explicitly `SyntheticFixture`, including player movement and
routes. These tests establish initial NPC model-height ownership; complete source
Route production, actual geometry and native production admission remain unproven.

## Initial player collision height

The optional initial-player source is selected from the identity retained by the
real successful `StarterInput` initializer, strict MPQ-loaded `ChrRaces` selection,
and the actual loaded CDI/CMD rows. It retains the raw scale operands and derives
stored object scale, the exact-zero getter repair, and collision height separately.
Callers cannot supply a final height or substitute NPC width, player radius or a
renderer model. Selecting the source atomically derives the player motion scalar;
edited scalars, changed source catalogs and a different starter identity fail closed.

The actual controller freezes the selected player's world/character identity,
source binding and exact sampled target pose, flags and dimensions into typed
NearPoint and Route requests. Native format 33 retains ownership through journal
replay, checkpoint and reopen. Tests exercise chase and moving-target replan,
source/identity/pose/height/width echo forgery, atomic controller/store/projection
failure, exact authority retry, single publication after a store retry, and both
signed-zero scale-getter and raw-height fallback cases. Recomputed checkpoint
hashes cannot authorize a changed retained-spline target height, identity or body
bounds; a valid historical target pose may differ from the player's current pose.

All fixture DBC/MPQ bytes are authored, successful spatial responses remain
`SyntheticFixture`, and only initial unshapeshifted player ownership is covered.
These tests supply neither a production reader/admission proof nor real geometry,
server-row completeness, transforms, shapeshift, mounts or later model mutation.

## Commands and movement orders

Supported component-test inputs are exact-ID NPC casts, cancellation of the captured
preparing cast, basic untransported movement, and explicit spirit-release refusal
through the native runtime, source-bound quest panels/accept/reward, ordinary corpse
loot, item queries and backpack-only `SwapInvItem`. Player melee and other unimplemented forms remain explicit
unsupported requests. The app arms its PendingCast at send, so a
cancel can arrive before the first SpellStart. It captures either an existing runtime
instance or the matching earlier queued cast request. In the durable batch that
request becomes an exact preceding input sequence. A rejected request cannot redirect
cancellation to another active instance; stale/mismatched cancellation is inert.
Cast then cancel still executes prepare and its GCD, followed by source cancellation
and GCD reset, before the simulation update. It never deletes both intents or lets a
short cast advance first. Per-input projection outcomes keep multiple starts,
rejections and cancellations in order.

Pinned CMaNGOS
[WorldSession.cpp:517–535](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Server/WorldSession.cpp#L517)
drains received map packets in FIFO order. The local 64-input/64-KiB budget is an
explicit resource bound, not a claim about the original server's packet cap.
Pinned CMaNGOS [SpellHandler.cpp:361–377](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/SpellHandler.cpp#L361)
and [Unit.cpp:4384–4396](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp#L4384)
match the requested ID and treat zero as a wildcard. The stronger captured-request
binding is deliberate: local queued cancellation cannot fall through from a rejected
new request to an older active instance. The creation-tick skip remains the source
Spell.cpp:3519–3527 behavior; batching does not change spell timer arithmetic.

One actual session owns `NativeMovementOrders`, initially counter zero independently
of simulation tick. Root currently is the only forced-order producer; unsupported
speed/mode operations cannot allocate another namespace. Death projection records
the emitted guid/counter/mode/apply. The actual `ClientCommand::MoveModeAck` must echo
that order and the current app's root flag without moving/falling flags. Turning,
walking and other stationary echoed flags are not rejected merely for being present.
Matching duplicate acknowledgment is idempotent. Unknown/stale counters do not clear
a current order. ACK position and orientation are validated but never committed as
world movement. Dead/rooted movement receives a current self-mover correction
without changing the saved or projected orientation.
A restored corpse starts a fresh session, emits its own root, and cannot reuse the
prior session's acknowledgment state.

This proves the existing client-internal typed order/ACK contract, not raw protocol
mask parity: the pinned CMaNGOS `Object.h` declares `MOVEFLAG_ROOT = 0x08000000`, while
this client's `MoveMode::Root.flag()` and `creature_anim::move_flags::ROOT` use
`0x00001000`. The discrepancy requires separate reference resolution; this controller
does not change those constants or use the upstream bit to invalidate real app ACKs.

## Transactional player pose

Selected source spell tests reject every external range request. Prepare checks
strict bounds; release checks its current poses with the source non-strict
tolerance. Equal and adjacent binary32 endpoints exercise both capped and
proportional tolerance, with LOS before range and range before facing/power.
Authored raw variants explicitly disable movement interruption when moving the
caster between phases; they establish neither stock spell content nor movement
authority. Failed derived receipt observation rolls back the exact tick/input,
and failed store publication retains the range failure and pose until one retry.
Publication retry and reopen preserve the bound pose without reapplying the input,
dice or cost; journal replay re-derives the same source range receipts.
The client projection preserves source range errors `0x59`/`0x76` in one failed
`CastResult`. Prepare failure emits only that result, matching pinned CMaNGOS
[Spell.cpp:3011–3016](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/Spell.cpp#L3011).
Release failure emits the result before `SpellFailedOther`, following
[StopCast](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/Spell.cpp#L7857)
and [SendInterrupted](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/Spell.cpp#L4130).
There is no duplicate generic result or rejection notification for that failure.
Same-tick casts of one spell retain distinct reasons and a later valid start across
failed publication, retry and replay; reopen does not repeat delivered failures.
The existing successful input-acceptance acknowledgement timing is unchanged;
this covers failure messages only. These tests establish typed client events, not
live stock-UI rendering.

Position and orientation enter as one sequenced `Pose`, with initial orientation
explicitly bound by the native profile. Neither queued input nor blocked/dead
movement changes the selected character or the delivered player facing. Pure turns
have no geometry/path-distance request and do not renew elapsed movement time.
Ordered pose events retain each input's orientation, including turn/cast/turn within
one tick. Spell authority requests witness the current orientation without claiming
implemented facing predicates or rear-hit/daze support.

Pose-bearing ticks save before publishing, even when progression is disabled.
The last delivered pose is projection state: if checkpointing fails, the computed
world and ordered outbox are retained for exact retry, while player getters and
corrections still report the previous delivered pose. A save receipt alone does not
publish it. Reopen uses the durable native pose rather than exploration-character
orientation. See [the runtime contract](../../../../../../benilla-world-runtime/src/native/ORIENTATION.md).

## Inventory, corpse panels and ordinary quest rewards

Inventory is projected from durable native item instances with stable per-instance
GUIDs, owner/container fields, backpack pointers and exact stack changes. Item
queries require audited gameplay metadata and exact client display bindings.
Equipment copies retain separate identities. Pickup/quest bonding also follows the
source `_StoreItem` path on whole-stack movement, swaps and complete merges; a
partial merge changes counts without rebinding. Backpack movement never equip-binds.

`SwapInvItem` accepts only backpack positions 23..38. Each queued positional input
resolves after earlier loot, quest and movement inputs in the same bounded FIFO.
Source receipts witness the exact before/after inventory; projection validates the
receipt before emitting stable-GUID slot pointers, stack changes and consumed-source
destruction. Moves, swaps and merges emit no item-acquisition push or quest credit.
Same-slot and empty-source inputs are silent; dead-player refusal emits one
`InventoryFailure` naming both callback-time item GUIDs. Equipment, equipped-bag,
bag-content, bank, split-stack and item-use commands remain outside this route.

GUID-less loot item/money commands capture the immutable corpse generation from the
last successfully published `LootResponse`. A queued or retained pending open cannot
authorize collection. Missing-slot and explicit release remove the source looter
without a `LootReleaseResponse`; item-last automatic exhaustion sends one, while
money-last exhaustion follows ordinary release. Published rights are retired from
committed state as well as response/destruction packets. Restore materializes a saved
target only with initial delivery: the fresh stock UI rejects its unsolicited response
and releases the target before the next ordinary right-click opens a visible panel.

Reward choice indices are bounded before giver lookup. Native choices 0..5 are
admitted, including empty slots. Source storage refusal emits `InventoryFailure`
then the callback-time offer; incomplete/already-rewarded/money refusals offer again
without inventing an inventory error. Earlier refusal remains visible if a later
same-batch choice succeeds. Required-stack destruction precedes reward creation and XP. Reward
push flags are received/from-NPC, not created, hidden from acquisition chat, bag 255;
slot selection compares the last stack count against the full grant. Completion
reports only fixed template rewards, including a legitimately skipped later grant.
Hidden rewards are suppressed in detail/template panels, but shown by the offer.

Every enabled progression tick saves before publishing. Failed checkpoints retain
already-computed item changes and the entire ordered outbox; retry saves and flushes
that boundary without reapplying grants. Restored inventory sends descriptors, not
reward notifications. These authored tests do not establish real client rendering
or production native-content admission.

## Verification boundary

`cargo test -p benilla-world-controller-tests` compiles these exact app modules without
Bevy. Its build script extracts real `ClientCommand` declarations and the real app
movement flag module, rather than keeping a second manually authored enum/mask.
Authored placeholder catalogs and source-shaped synthetic runtime fixtures test
identity, clock debt, flat-batch pressure, pre-pump/short-cast cancellation, per-input
projection, native movement interruption, real ACK handling,
dead-state reload, checkpoint replay, fault/retry and final-save lock retention.
Backpack tests also run the real Lua cursor verbs, verify the pending item lock
through failed save, and apply projected descriptors to an item cache across retry
and reopen. They verify the slot/count changes consumed by the app's lock watcher;
this no-Bevy harness does not execute that watcher or establish live UI rendering.
Full app compilation and live user-local asset/rendering checks remain separate gates.

## Empty-selector source NearPoint receipts

The separately selected loaded empty-selector provider preserves the source's
ordered primary/free search and exhausted first-XY/last-Z return. Its mailbox
policy and geometry revision cannot be confused with the primary-only provider.
The resulting source point is not a route-validity certificate: remaining source
path and combat authorities in these component tests are explicitly synthetic.
No character-entry or production admission path is enabled by this composition.

The route-owner/filter component tests reach the actual typed Route request after
NearPoint and read actual authored dry products at the current owner map position.
A successful initial filter still pauses as unavailable route production; no
staged world, durable store or projection changes escape. Binding faults remain
terminal simulation failures, while coverage/deadline failures retain exact retry.
Separate successful-route replay/reopen checks use explicitly synthetic routes.

The warm-cache component tests distinguish an explicitly SyntheticFixture initial
calculation from later actual loaded polygon dependencies. Retained cache state
travels in the real runtime Route request and durable replay, while a loaded warm
proposal still cannot publish a complete route. Exact retries and failed bindings
preserve the staged world, input, store and projection. Source-owned checkpoint
rehydration and initial cold calculation remain unavailable; see
`benilla-world-runtime/src/native/PATHFINDER-CACHE.md` for the format-23 boundary.

The terminal smooth-calculation tests extend that owner-thread path through the
actual selected warm polygon prefix, source smoother and point postprocessing.
They retain the explicit synthetic initial state and refuse a full Route after a
successful loaded dependency. Source straight-line-first Chase dispatch and fresh
Home creation are separate boundaries; supported point output does not bypass
endpoint/segment evidence or production admission.

The warm straight-calculation tests initialize collision width through Plan's
opaque initial-model receipt and authored MPQ/DBC loading, then consume the exact
runtime Route witness on the loaded navigation owner thread. Width is the source
object-scale times raw field14, with no radius, display-scale or render-scale
substitution. Native format 33 retains that witness in the explicitly synthetic
initial route's replay and reopen; the real loaded dependencies never enter the
accepted authority journal.

Loaded straight rejection and clipping feed a typed continuation using the first
terminal cache, terminal path type and original requested goal. Each calculation
first validates source map coordinates, then validates the runtime-issued
`OwnerIgnorePathfindingReceipt`, then observes source HaveTile on the same
loaded mesh before its fresh ordered owner-liquid observations. First and retry
tile replies retain the original unsnapped endpoints and distinct source-ordered
tickets. Invalid coordinates or an absent endpoint tile leave the controller
boundary and pending input unchanged, even where nearby cached polygon lookup can
succeed. A missing start tile skips the destination observation and remains an
unavailable shortcut, never a fabricated NOPATH. Tests cover unchanged repeated
requests, SHORT without fallback, deadline/work-budget refusal and forged width,
generation, content or cache bindings. A successful dependency still pauses without
committing or publishing changes to the world, checkpoint, journal, durable store,
projection, delivered pose or pending input. Initial route/cache and surrounding
NearPoint/player movement remain explicitly synthetic. Complete source Route
production, cold initial-owner policy, checkpoint cache rehydration and production
entry remain unavailable. These warm-route tests do not consume the separate
selected MMap configuration or retained source1024 constructor-query dependency.
Their full constructor/query
composition remains unproven; see `../native_geometry/README.md`.
The owned receipt proves only the clear `0x10000000` UnitState bit from explicit
interaction seeds and the bounded lifecycle closure. Both calculation dependencies
retain it; missing or edited receipt/profile/source/generation bindings stop before
tile/filter work. Journal roundtrip, replay rejection and retry rollback retain this
narrow provenance without authenticating the synthetic initial warm route.

## Owned cached player environment

An explicitly selected player environment source uses the source-owned initial
player height and the actual immutable TerrainInfo/LiquidType products. Due
post-relocation observations pass through the typed geometry mailbox; missing
products pause the same pending input for retry. Every accepted pose is reduced
in command order, including unchanged-position and rotation-only calls. The
100 ms gate counts down after those commands; stationary expiry never queries
terrain. The last accepted pose and the earlier cached-query pose remain distinct.

Native format 33 stores the cache and monotone phase sequence in the native
checkpoint, without a nested standalone environment journal. The controller saves
every enabled environment tick before publishing, including countdown-only ticks.
A failed save retains the committed tick and withholds projection until retry;
a geometry failure never commits the staged pose, cache, receipt or input sequence.

The explicitly selected surface callback domain supports ordinary nonhazard
InWater, AboveWater, WaterWalk and NoWater transitions with actual owned slow-aura
removal and hostile-reference callbacks. Underwater breathing, high-sea, hazards,
liquid self-casts and active mirror timers remain unavailable. Its cached target
water bit does not itself certify navigation or playable wet movement.

## Loaded dry Chase selection

The Chase tests compose exact source binary32 distance and vertical predicates,
real static VMap rays (including M2), an actual owner dry-liquid query and the
runtime's owned cached player-water witness. LOS uses the requested route goal,
not the current target pose, and independently authenticated owner/target heights.
A single immutable inventory supplies environment, dry navigation and VMap;
loading a dry projection cannot relabel another inventory or admit wet navigation.

Gate success reaches the loaded warm straight/retry dependencies, with fresh
calculate-time liquid observations. Gate rejection selects unavailable normal
routing, never fabricated NOPATH. Both paths still withhold complete Route facts.
The controller tests keep the initial warm seed and surrounding movement/combat
explicitly synthetic, then require complete world, store, cache, journal and
projection rollback on the genuine dependency boundary. Neither cold owner
initialization nor production entry is opened.

## Complete authored two-owner visitors

The injected-authority component path accepts native33's explicitly selected
complete one-player/two-NPC fixture. Its independently owned hostile A and raw
neutral-to-all B both remain ordinary Aggressive, combat-capable empty EventAI.
An authored owner permutation is independent of complete grid/world membership
order; it does not claim a universal source update schedule. Directional AI
visibility precedes the runtime-derived read-only peer sight prefix, and any
reached unrepresented assistance or peer-attack continuation rejects the frame.

The exact controller tests exercise two NPC GUIDs, A's motion/death/respawn without
B identity churn, stale A handles, late-owner authority rollback, checkpoint
publication failure/retry without redispatch, and synthetic replay/reopen. Loaded
app body/geometry policies cover both selected owners. There is no loaded visitor
producer, graphical create-to-reopen evidence, production startup admission, or
permission to reopen a retained native PathFinder cache.


## Distinct quest loop with loaded geometry

A separate app-only fixture composes the selected two-actor quest/combat/loot
catalogs with the installed loaded Environment/NearPoint/same-polygon Route
Service. The controller-selected identity is bound into the provider policies
before installation. Natural hostile sight obtains A's native cache before B's
quest is accepted; stock source-spell133, corpse collection and B reward commands
then traverse the normal projection/store path. B remains an independent full
neutral Aggressive owner, and semantic escapes to the synthetic spatial remainder
fail closed.

Shared controller assertions are also compiled by the headless controller harness,
but only the app fixture executes the actual Service/LocalCorridor and checks its
native candidate pointers. That fixture exercises late-owner rollback, pending
input preservation, checkpoint publication retry without authority reexecution,
and direct/post-journal native-cache reopening refusal. Cell membership,
directional visibility, spell visibility/LOS and player pose acceptance remain
authored. This is not a loaded visitor producer, supported native-cache reopening,
production entry or an installed-content playability claim.
