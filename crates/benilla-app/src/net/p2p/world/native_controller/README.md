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
substitution. Native format 24 retains that witness in the explicitly synthetic
initial route's replay and reopen; the real loaded dependencies never enter the
accepted authority journal.

Loaded straight rejection and clipping feed a typed continuation using the first
terminal cache, terminal path type and original requested goal, with fresh ordered
owner-liquid observations. Tests cover unchanged repeated requests, SHORT without
fallback, deadline/work-budget refusal and forged width, generation, content or
cache bindings. A successful dependency still pauses without committing or
publishing changes to the world, checkpoint, journal, durable store, projection,
delivered pose or pending input. Initial route/cache and surrounding NearPoint/player movement remain
explicitly synthetic. Complete source Route production, cold initial-owner policy,
checkpoint cache rehydration and production entry remain unavailable.
