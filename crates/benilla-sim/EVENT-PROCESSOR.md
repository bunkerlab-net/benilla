# Bounded source EventProcessor

`event_processor` is a per-Unit deterministic translation of the pinned CMaNGOS
Classic EventProcessor queue. It does not implement Spell::prepare, SpellEvent,
delayed spell offsets, spell cancellation side effects, AI notifications' nearby
visitor, or the ordering of the enclosing Unit phases. Those hosts must supply
actual source callbacks and stage their state with the queue. No runtime/app
adapter, native admission, database, credentials, game assets or network is used.

## Source and license

CMaNGOS Classic commit `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

- `src/framework/Utilities/EventProcessor.cpp`, SHA-256
  `164cda6f21f72d60e6c426ad04d39036565eda6e60e5e10fce390c43eb396e2f`
- `src/framework/Utilities/EventProcessor.h`, SHA-256
  `8e85449a46540a7f999f1ac3a3132c2d2b3256b9c29b41a7e2c29ae78cf812c9`

The exact unmodified files, including their GPL-2.0-or-later notices, are in
`tools/event_processor_source/`. Rust translation and authored test drivers are
GPL-3.0-or-later. [Existing authorship, copyright and license texts](PROVENANCE.md)
apply. The authored `Platform/Define.h` shim provides only uint32/uint64 names;
it is not a port of the upstream platform layer.

## Source semantics

- `begin_update(diff)` advances the processor's clock once. Each Execute gets
  that same current clock and **entire** uint32 Unit diff, not the scheduled
  deadline, lateness, remaining diff or a fabricated fixed interval
- Before each callback, Update erases the earliest queued event. Callbacks may
  add, re-add, modify, flag-abort or kill other events before `pop_due` chooses
  the next one. Re-added deadlines <=now execute again in the same update
- `std::multimap` equivalent keys retain insertion order. `modify_event_time`
  erases/reinserts with a fresh insertion ID, even if the deadline is unchanged
- `add_time_ms` corresponds to BasicEvent::m_addTime. Initial AddEvent sets it
  to now; re-add optionally preserves it. `exec_time_ms` is updated by every
  add or queued modification. Scheduled time and insertion identity are exposed
  separately on each callback token
- Execute(true) destroys the event. Execute(false) leaves the object alive;
  without re-add it becomes **detached**, stays counted against the live-object
  limit, and is persisted. There is no invented automatic rescheduling/deletion
- `set_aborted` changes `to_Abort`; it neither removes the event nor calls Abort
  immediately. Update invokes Abort at its deadline and unconditionally deletes
  it, ignoring IsDeletable. `kill_event` removes a queued event immediately
  **without Abort**. Missing/detached events, including the currently executing
  event before re-add, make KillEvent and ModifyEventTime no-ops
- `begin_kill_all_events(force)` sets the source aborting state, walks queue
  order, sets to_Abort and invokes Abort. `finish_abort(is_deletable)` supplies
  the virtual IsDeletable result **after** the host Abort has run. Force always
  deletes; non-force may retain flagged nondeletable events. A retained event
  receives Abort again on a later KillAll or due Update. Detached objects are
  not part of KillAll's queue
- The source's comment says `m_aborting` prevents additions, but AddEvent does
  not read it. Its persisted flag is observational: adding after KillAll works

IsDeletable is a virtual method, not a BasicEvent field. Its actual dependency
state belongs to the host callback snapshot. It must not be replaced by a stale
pre-Abort boolean in the runtime. Likewise event destructors may have host-side
effects; successful `finish_*`/`kill_event` deletion must be observed and staged
by that host. Rust Drop cannot execute host callbacks. The owning Unit's
teardown must explicitly drain `begin_kill_all_events(true)` before discarding
its queue if source Abort/destructor effects apply. Dropping the primitive alone
is not a translation of the C++ destructor.

## Owned-handle and admission boundaries

Every live BasicEvent has one nonzero stable ID and at most one queue entry.
`next_event_id()` exposes the persisted per-processor lifetime watermark;
add_event requires an ID at or above it and checked-increments that watermark.
EventKind names AiNotify or SpellUpdate{cast_id}, with a nonzero cast ID. The
outer runtime must validate that the queue is bound to the exact Unit identity,
owner/generation and corresponding cast/notify state. It must explicitly handle
respawn and old-generation cancellation. The primitive rejects duplicate live
IDs and reuse/regression after deletion, including across snapshot reopening.
A fresh replacement receives a fresh lifetime ID even while the old callback is
still detached. A runtime must additionally bind owner/generation when a Unit
or processor is replaced, and reject handles from the prior generation. No global ID allocator, clock, randomness or pointer is used.

The safe subset explicitly rejects multiple simultaneous entries for the same
pointer, re-add then return true (which leaves a dangling source queue pointer),
KillEvent(this) after re-add during its callback, first add with an uninitialized
m_addTime, and structural queue mutation during KillAll callbacks. Source
KillAll's iterator can be invalidated by peer removals; additions also interact
with its saved next iterator and forced final clear. These lifetimes/iteration
patterns are not approximated. Update Abort callbacks may still add, modify,
flag-abort or kill **other** events; their resulting ordering is source-tested.
Nested Update/KillAll callbacks and external raw-pointer/member mutation are not
exposed. The host must reject unrepresented behaviors rather than omit them.
Retained detached objects and delayed-deletion SpellEvent state are represented
at the queue level; this is not permission for an incomplete spell host to
admit them as a working spell scheduler.

## Transactions, bounded state and replay

All IDs, clocks and deadlines use integers. Clock/CalculateTime/event-ID/insertion-counter
overflow is rejected instead of reproducing source uint64 wrap. This explicit
admitted-domain restriction has policy ID
`cmangos-8ec338a1-event-processor-owned-v1`. There are at most 1024 live events
(queued **and** detached) and 4096 callbacks per transaction. Infinite same-now
re-add cannot hang the simulation; budget exhaustion fails the whole update.

A transaction owns a staged queue clone. Any operation error poisons it; drop,
an unfinished callback, remaining due events, overflow or budget exhaustion
prevents commit. Rejected direct operations also leave the processor unchanged.
The queue cannot roll back arbitrary caller effects: **stage the complete
Unit/world, spell/notify callback state, RNG cursor and emitted outputs beside
it, then publish all of them only after queue commit succeeds**. Real runtime
integration must test that enclosing rollback boundary. Do not send messages,
apply world effects, or mutate other Units from an uncommitted callback.

Only stable between-dispatch snapshots are exposed. They retain clock, next
event-ID and insertion counters, aborting, all queued events in canonical deadline/insertion
order, and retained detached events in ID order; every record carries kind,
source add/exec times and abort state. Restore rejects duplicates, invalid IDs,
noncanonical ordering, deadline/exec mismatch, future add times, impossible
future detached exec times, insertion reuse/counter regressions, invalid cast
IDs, version changes, unknown fields and excess live counts. Deserialization
bounds each event sequence before unbounded allocation; the enclosing transport
must also impose its normal total byte limit. Restore is validation of this
queue, not authority to introduce casts or arbitrary source event kinds.

## Differential checks

Run `python3 crates/benilla-sim/tools/generate_event_processor_fixture.py --check`.
The tool verifies both exact source hashes and compiles the original .cpp with
C++17, warnings-as-errors and a tiny type shim. Missing compiler is an error.
`--upstream <src/framework/Utilities>` additionally compares local upstream
copies. `--trace-output <path>` emits all readable C++ traces for diagnosis.
No installed server, game files or network are required.

Selected readable boundary traces are committed alongside SHA-256 digests and
row counts for 24 deterministic generated command sequences. Rust independently
replays **every command and every callback/deletion/queue-state trace**, checks
the full per-scenario digest, and repeats with JSON snapshot reopening after
every command. The compact fixtures retain complete comparison coverage.
The C++ oracle tests equal keys and modifications, coarse/zero/uint32-max diffs,
callback additions/removals/modifications/abort flags, same-now recurrence,
future recurrence, detached retention, add-time preservation, KillEvent without
Abort, nondeletable repeated Abort, post-Abort IsDeletable changes, force cleanup
and additions after KillAll. Separate Rust adversarial tests cover count limits,
clock/counter overflow, unsupported lifetimes, malformed snapshots, poisoned
transactions, incomplete dispatch and execution-budget rollback.
These are source queue tests, not full Classic or playable-world evidence.
