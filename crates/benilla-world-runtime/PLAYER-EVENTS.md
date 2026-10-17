# Native per-player source events

The native runtime's player owns the bounded source EventProcessor and one
lifetime record per SpellEvent. It shares insertion/deadline order with the
player's AI-notify visitor. The queue is bound to the full world and
owner/character identity; cast IDs and event IDs are independent monotonic
lifetimes. NPC generation identity remains part of the spell's target binding.
There is no production admission capability in this integration.

## Source phases

Pinned CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`,
`src/game/Spells/Spell.cpp` SpellStart, Prepare, update, cancel and SpellEvent;
`src/game/Entities/Unit.cpp` Update, _UpdateSpells, SetCurrentCastedSpell,
InterruptSpell, ScheduleAINotify and destructor:

- SpellStart allocates/enqueues at Unit queue time +1 before PreCastCheck;
  a refusal reached through the bounded prepare adapter still owns an event until its due callback
- Each due SpellEvent receives the full Unit diff. The existing bounded spell
  implementation skips its creation map tick. The queue clock starts at zero;
  map/world timestamps include the profile clock origin. Spline elapsed time is
  a separate component clock
- Casting polls by re-adding at the callback queue time +1, preserving add time.
  Launch records delay start at callback time and re-adds at its flight deadline.
  Traveling spells have no fabricated callbacks on intervening world frames
- The due delayed handler may finish the spell, but that switch arm still
  re-adds at callback time +1. A later finished/deletable callback destroys it
- Current generic references survive launch. A new successful prepare replaces
  the prior slot reference. Unit::_UpdateSpells clears a finished reference only
  after the event queue has completed, before the later Unit phases
- Explicit interruption cancels immediately, clears the current slot, and leaves
  the event for its scheduled cleanup. Finished spell objects are represented by
  lifetime records; they are not projected as executable active casts
- Source SpellEvent uses AddEvent(this, deadline, false); it does not call
  ModifyEventTime. Forced AI-notify replacement uses KillEvent followed by a
  fresh event identity, rather than modifying an existing event

The source event-decision helper and exact extracted C++ scheduler oracle are
in benilla-sim. Authored host drivers are deliberately distinguished from the
unmodified scheduler methods; they do not establish full Spell/Unit parity.

## Persistence and transactions

Active source-scheduled casts use an explicit restore entry point that binds the
last actual callback timestamp to their owning lifetime. Ordinary frame-driven
cast restore retains its stricter equal-current-time checks. Queue deadlines,
source add times, current references, cast identities, delay starts, terminal
reason/time and the complete active/retained event sets are cross-validated.
Insertion identities are checked against independently derived input/callback enqueue
moments and retained predecessor keys; source-fixed-step dispatch deadlines and
predecessor launch times reject impossible equal-key reorderings. These are
source-consistency constraints, not authenticated history or authority to edit saves.
No detached spell, channeled cast, unrepresented triggered dependency, arbitrary
callback, external reference or nondeletable leak is admitted.

The entire Frame is staged: queue, lifetimes, world effects, RNG, callbacks,
outputs and destructor effects publish together. An incomplete/failed queue
transaction or later world error leaves the original world untouched. The
controller's existing durable publication boundary holds outputs on save
failure and retries the same committed tick without dispatching it twice.

A durable session suspend/checkpoint preserves the Unit and its events.
`NativeWorld::teardown` instead consumes a permanently discarded world: it
clears current references, runs forced queued Abort callbacks, and produces
terminal cancellation effects exactly once. Cloning/dropping a snapshot is not
source Unit destruction and invokes no host side effects.

The existing command boundary still rejects dead casters, already-casting requests,
and unknown/dead targets before the bounded prepare adapter. Those early rejections
do not claim the full SpellHandler/SpellStart failed-attempt lifecycle. The synthetic
no-death-host compatibility path can cancel flights and retains their original cleanup
deadlines; the reviewed real death host preserves released flights.

This remains the existing bounded non-triggered, non-item, non-channel player
projectile model. It does not admit instant entry, multi-target offsets,
reflection, arbitrary reference graphs, proc chains or general
server map ordering. Neither these tests nor the source decision oracle provide
client-asset, live-realm or complete Classic gameplay evidence. The separate
[NPC Unit queue](NPC-EVENTS.md) owns [bounded triggered spells](NPC-SPELLS.md)
without reusing this player-to-NPC adapter.
