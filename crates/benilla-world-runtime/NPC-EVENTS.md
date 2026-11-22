# Native per-NPC source events

Every represented native NPC owns one bounded `EventProcessor`, independent of
player and map clocks. The source-motion host uses it for real AI-notify event
ownership instead of a separate collapsed timer. Its world, full NPC generation,
monotonic event/insertion allocators, pending handle and `CanAggro` state are part
of the atomic native world checkpoint. Compatibility fixtures own an empty queue;
this does not turn their authored AI observations into source visitor evidence.

## Source and lifetime

Derived from CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

- `src/game/Entities/Creature.cpp:140,705–782`: initial `CanAggro=false`, ALIVE and
  CORPSE invoke Unit update; DEAD, including its respawn update, does not
- `src/game/Entities/Unit.cpp:490–537`: events precede spell/aura, spline, motion,
  AI and combat-manager phases
- `Unit.cpp:10923–10978`: notify Execute enables aggro only if alive, runs the
  actual visitor, then clears the handle and deletes the event; Abort clears the
  handle. Nonforced schedule retains a pending event. Forced schedule uses
  KillEvent, without Abort, followed by a fresh event
- `Unit.cpp:7984–7988`: entering combat while initially inhibited enables aggro
  and kills that pending notify; entering combat while already enabled leaves a
  relocation notification intact
- `Creature.cpp:249–303`, `Unit.cpp:10981–10997`: corpse removal relocates while
  DEAD and can schedule a notification that remains frozen until respawn
- `Creature.cpp:723`, `Unit.cpp:8556–8559`: respawn resets `CanAggro`, force-replaces
  any pending notify, and retains the same Unit processor clock and allocators
- `Unit.cpp:429–461,9428–9437` and EventProcessor destructor: permanent deletion
  differs from death, corpse removal, respawn and durable suspension

The [source queue port](../benilla-sim/EVENT-PROCESSOR.md) preserves stable
same-deadline insertion ordering, add/exec times, full-diff callbacks and Abort
semantics. Its pinned C++ oracle remains the queue differential reference.
The [bounded instant NPC SpellEvent host](NPC-SPELLS.md) now shares this queue;
mixed event ordering tests remain authored component coverage.
The new host and tests are GPL-3.0-or-later translations/integration of the
GPL-2.0-or-later source; [the existing provenance](../benilla-sim/PROVENANCE.md)
and CMaNGOS notices apply.

## Atomic callbacks and owner validation

The whole Frame stages queues, callback effects, health, motion, RNG and outputs.
During dispatch the transaction owns the mutable queue; a read-only callback
snapshot exposes its actual clock, detached executing event and pending peers to
authority bindings. The NPC handle remains set throughout the visitor, so any
ordinary rescheduling from synchronous sight/chase callbacks remains a no-op.
This bounded visitor does not support forced replacement or arbitrary queue
mutation during its own callback; those paths fail closed. The explicitly selected
[complete visitor lane](src/native/README.md#selected-complete-source-visitors)
keeps that lifetime across the entire ordered grid/world traversal and both peer
directions. The owner update order is an authored catalog permutation; each
owner's notify, aura, motion/AI and regeneration phases remain consecutive.
Bounded triggered
starts run outside dispatch; nested starts inside a visitor remain unsupported.

External event handles bind world, spawn, generation and event ID. Respawn keeps
the source processor while changing the external generation, so prior-generation
handles cannot resolve even if presented with a current event number. Death
alone does not clear `CanAggro` or abort the queue. A dead creature's due visitor
can set `CanAggro=false`; ordinary corpse removal preserves whichever value the
source had until respawn resets it.

Checkpoint format `NATIVE_FORMAT_VERSION` (`NATIVE_POLICY`) binds source policy,
owner, queue, pending notify, aggro state and count of DEAD updates skipped by
the processor. The exact queue clock equals `(tick - skipped_updates) * diff`;
the AI's actual corpse-removal timestamp independently bounds the current DEAD
interval. Notify identities/insertion keys are fresh and never re-added,
deadlines use the configured scheduling delays, and stable snapshots cannot
retain already-drained callbacks, foreign SpellEvents, detached events, abort
flags or impossible initial/combat/respawn states. Previous-generation elapsed
history is a consistency claim, not an authenticated event history.

Replay checks the same callback-bound requests and regenerated state. Store and
exact headless app-controller tests cover pending and due notifications, save
failure, retry without redispatch, and reopening. Preceding native formats, including the format-12 fixtures,
are rejected without replacing or deleting current/previous files. The separate
character/profile save format and original `World` format 2 are unchanged.
`NativeWorld::teardown` explicitly forces NPC Abort callbacks before permanently
discarding the Unit. Checkpoint suspension does not run teardown.

## Deliberate remaining boundary

The [bounded NPC triggered spell host](NPC-SPELLS.md) now owns instant
NPC-to-player slow SpellStart objects and their failed/finished cleanup events.
It requires complete source-bound checks and additional current-host closure.
The player projectile adapter is not reused. Actual spell1604/rear-trigger
admission remains closed pending its raw source dependencies and all reachable
lifecycle behavior. Authored tests are not raw-content, client-asset, live-realm,
complete Classic combat or multiplayer gameplay evidence.

## Selected assistance events

The optional [combat-entry assistance owner](COMBAT-ASSISTANCE.md) adds its
help-check re-enable and delayed linking/assistance lambda payloads to this same
processor. Equal deadlines preserve insertion order. CombatStop resets only the
assistance latch, and pending payloads survive corpse/dead/respawn with the
physical Unit. Required selected state and retained history are validated with
queue ownership during restore; no second clock or generation-based cancel is
introduced. The initial domain requires authored complete-empty receiver
traversal and excludes entry inside this NPC's active event callback.
