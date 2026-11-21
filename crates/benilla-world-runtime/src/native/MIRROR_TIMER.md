# Owned mirror timers

`native::environment::mirror` independently owns the four source `MirrorTimer`
objects: FATIGUE, BREATH, FEIGNDEATH and ENVIRONMENTAL. This GPL-3.0-or-later
port is based on CMaNGOS Classic revision
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, `Player.cpp:154–286`,
`Player.h:410–474`, and `shared/Util/Timer.h:105–128` (`ShortIntervalTimer`).
Upstream is GPL-2.0-or-later, copyright CMaNGOS contributors.

## Boundary and source semantics

The owned `MirrorTimer` implements the constructor, both Start overloads,
Stop, FetchStatus, SetRemaining, SetDuration, SetFrozen, SetScale and Update,
including both tracker and pulse state. Getters are read-only; an explicit
`status()` inspection does not consume status, while `fetch_status()` does.

- Source status is last-write-wins, not a full/status-update priority queue
- Scale zero requests freezing without changing the stored nonzero scale
- Regeneration overrides the stored frozen bit; SetFrozen compares effective
  IsFrozen, which can differ from that stored bit
- Start(interval, spell) retains freezing; Start(current, max, spell) sets
  elapsed to max minus current and requests unfreezing, possibly overwriting
  the full status update with a status update
- Stop only resets active timers, retaining interval, spell, scale and freeze
- Depletion's first expiry returns false immediately; subsequent expired ticks
  accumulate into a 2,000 ms pulse tracker. Reset subtracts one interval, so a
  large pulse retains its backlog instead of reducing it modulo 2,000
- First-expiry excess is not credited to that pulse tracker. Starting already
  expired or at zero duration follows the source equality branch, without an
  invented instant pulse
- Regeneration resets pulse progress while remaining active and stops when the
  scaled difference reaches or exceeds the elapsed tracker value

All source uint32 subtraction, addition and scale multiplication use explicit
32-bit wrapping arithmetic, including remaining greater than maximum,
SetDuration below elapsed, tracker wrap, and product wrap. The one undefined
source operation is `std::abs(INT32_MIN)` on an active, effectively unfrozen
Update. That exact path returns `UndefinedScaleMagnitude` without mutation,
even for diff zero. Setting INT32_MIN, inactive Update, and frozen Update are
well-defined and remain supported. The rejection is a Rust safety bound, not a
CMaNGOS result or a guessed magnitude.

`Update` exposes the source boolean only. False does not claim that damage,
drowning, fatigue, resurrection, aura expiry, spell effects or packets executed.
There is no Player::UpdateMirrorTimers driver, breathing interval/configuration
implementation, aura multiplier evaluation, packet producer, NativeWorld save
format change, Engine hookup, startup activation or native admission capability.
Direct timer ownership and environment scale ownership do not supply those
missing dependencies.

## Standalone persistence and replay

`MirrorTimerMachine::new(world, player)` owns all four timers under one explicit
world/player identity. Its `apply(timer_type, operation)` journals each accepted
operation, including no-op calls and status consumption, with the exact result,
sequence, previous-record digest and before/after state digests. An undefined
operation, full journal or exhausted sequence commits nothing.

Checkpoints retain every timer field: type, scale, spell, status, active flag,
stored frozen flag, tracker interval/current and pulse interval/current. The
format has a separate version/policy and does not alter native world saves.
Restore requires independently selected nonzero world/player identities,
bounded canonical bytes, exact field schema, compatible version/policy,
checksum/state digest, and structural source-state invariants. Sequence zero
must be the actual constructor state. Crate-private snapshots use the same
checked timer restore before becoming mutable source state.

Structural validation is not a reconstruction of the entire execution history.
Checksums/hashes detect corruption; an adversary able to rewrite and re-hash
a complete checkpoint can manufacture compatible state. Trusted storage or an
independently pinned state digest is required. No provenance, authorization,
source-content certification or effect-execution receipt is implied.

Replay re-executes each operation and compares its complete record, including
return value and chain/state digests. The full batch is staged; a malformed or
changed tail, unsupported operation, identity mismatch or capacity error rolls
back the entire batch, including earlier status consumption. Journal capacity
is 2,048 records and 4 MiB canonical JSON; checkpoints and replay inputs share
the 4 MiB input bound. Durable storage is the caller's responsibility: retain
the exact current checkpoint first, then pass it to `discard_records`. The
component has no filesystem writer and cannot verify that retention succeeded.

## Evidence

The [source oracle](../../tools/mirror_timer_README.md) executes exact extracted
pinned methods and the original ShortIntervalTimer class over authored command
sequences. The Rust fixture consumer compares every before/after field and
return, and round-trips checked snapshots for every row. Undefined-abs sentinel
rows verify the explicit unsupported boundary, not source execution parity.
Hand-authored tests additionally cover status overwrite/consumption, pulse
backlog, both freeze states, zero starts, wrapping arithmetic, all four timer
identities, persistence, suffix replay, corruption, canonical-schema rejection,
capacity/sequence limits and whole-batch rollback. These are standalone timer
and integrity tests, not evidence of live world gameplay.
