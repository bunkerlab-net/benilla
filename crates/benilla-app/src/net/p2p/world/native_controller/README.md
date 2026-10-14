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

Native snapshot/journal schema, policy and digest domains are v4. Tick records contain
`inputs: Vec<Input>` and source-ordered per-input outcomes. Version-3 native saves are
rejected, never migrated or relabeled. Authored World v2 is unchanged; the store's
outer native envelope delegates the schema check to the runtime.

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
The provider currently handles only source-resolved NPC movement queries; unresolved
player movement, spell, melee and NPC facts remain unavailable. Service installation
and an admitted source compiler remain required before production entry can open.

## Commands and movement orders

Supported component-test inputs are exact-ID NPC casts, cancellation of the captured
preparing cast, basic untransported movement, and explicit spirit-release refusal
through the native runtime. Quest/loot/player melee and other unimplemented forms
remain explicit unsupported requests. The app arms its PendingCast at send, so a
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
world movement. Dead/rooted movement receives a current self-mover correction.
A restored corpse starts a fresh session, emits its own root, and cannot reuse the
prior session's acknowledgment state.

This proves the existing client-internal typed order/ACK contract, not raw protocol
mask parity: the pinned CMaNGOS `Object.h` declares `MOVEFLAG_ROOT = 0x08000000`, while
this client's `MoveMode::Root.flag()` and `creature_anim::move_flags::ROOT` use
`0x00001000`. The discrepancy requires separate reference resolution; this controller
does not change those constants or use the upstream bit to invalidate real app ACKs.

## Verification boundary

`cargo test -p benilla-world-controller-tests` compiles these exact app modules without
Bevy. Its build script extracts real `ClientCommand` declarations and the real app
movement flag module, rather than keeping a second manually authored enum/mask.
Authored placeholder catalogs and source-shaped synthetic runtime fixtures test
identity, clock debt, flat-batch pressure, pre-pump/short-cast cancellation, per-input
projection, native movement interruption, real ACK handling,
dead-state reload, checkpoint replay, fault/retry and final-save lock retention.
Full app compilation and live user-local asset/rendering checks remain separate gates.
