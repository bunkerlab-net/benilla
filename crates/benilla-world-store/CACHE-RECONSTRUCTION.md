# Bounded native reconstruction recipes

`native::reconstruction::RecipeStore` is a separate persistence format for a
cache-free anchor and its complete subsequent accepted tick history. Creation
still requires fresh tick-zero genesis. It uses the existing exclusive lock,
backup-first publication and durability rules, with magic `BNNREC2\0`, envelope
version 2 and checksum domain `benilla-native-reconstruction-store-v2`. Version 1
recipes are explicitly rejected with `UnsupportedVersion`; there is no silent
migration. This does not change native runtime checkpoint compatibility or
migrate ordinary `NativeStore` files.

The store restores only the cache-free anchor through the caller's checked local
constructor, verifies its exact checkpoint and cache-free reopen guard, then
privately receipt-replays the records. It exposes an immutable recipe and
expected checkpoint, never that replayed world. Ordinary adjacent generations
must have the identical anchor and an exact record prefix, including when the
anchor has a nonzero tick. Saving appends the accepted suffix even after the
ordinary runtime journal is cleared.
Preparation decodes its bounded wire envelope before publication, so returned
recipe records contain no process-local dispatch seals even before reopening.

## Explicit cache-free reanchor

`prepare_reanchor(&world)` prepares one new generation whose anchor is the exact
already durable expected checkpoint and whose retained records are empty.
`commit(prepared)` publishes it with the normal stale-generation, disk-digest,
exclusive-lock and fault controls. `checkpoint_reanchor(&mut world)` combines
preparation, publication and receipt acknowledgement for that one generation.

The caller must first complete an ordinary `checkpoint` and acknowledge its
receipt. Reanchor rejects an unsaved suffix, an unacknowledged journal, a stale
or divergent checkpoint, or any world failing the unchanged
`validate_saved_session_reopen` guard. It checks the supplied live world and
preserves normal preparation's bounded retained recipe and private replay
comparison. It never clears caches or promotes serialized native references to
live ownership. Home-arrival Reset or death can naturally leave a cache-free
boundary; the store does not manufacture one or decide when lifecycle work runs.

The new envelope retains the previous envelope's exact digest as its parent.
Reopening validates both generations independently. An anchor change requires
an empty current record list and an anchor exactly equal to the complete privately
replayed previous expected checkpoint, including tick, history tip and identity.
Reanchor and a new unsaved suffix cannot be combined into one generation. Normal
backup-first interruption handling still accepts identical current/previous
files; it never falls back to a different history. Failed publication preserves
the live checkpoint and retained recipe; an uncertain result requires reopening.

## Loaded ownership

The app-private `source_cache_reconstruction` adapter requires a cache-free world
matching the anchor and pristine loaded route providers. A rolled-back attempt
is not pristine: constructor registrations, proposals, preparations and pending
jobs cannot be reused. A store requiring reopen after uncertain publication or
external modification also cannot supply a resumed session.

Each saved input is executed with `step_batch` against the actual fresh authority.
Every generated record must equal the saved record, including requests, replies,
draws, events and before/after digests. The existing concrete provider must ACK
each newly generated tick and its actual certified native candidates. Stored
Route facts, even attached to a fresh dispatch, cannot supply that ownership.
The final checkpoint and loaded commit boundary must match before the owned
world, providers and store are returned together. Failure drops the candidate
session and lock without modifying the saved files or an existing live session.

This reruns constructor selection, Wander's pre-draw Prepare, accepted NOPATH
mutations and retained calculations in their original order. Serialized polygon
references and cache DTOs are comparison outputs only. Generic cached-checkpoint
restore and ordinary native store reopening remain closed.

Fresh meshes require identical validated products, source epochs, inventory,
configuration, source1024 query selection and canonical tile insertion order.
Pinned Detour references contain insertion-order tile slots: a numerically valid
reference after reordered loading may identify a different tile. Equal bytes
and references never transfer an old allocation's native handle.

## Limits

The complete retained recipe is capped at 2048 accepted ticks and 8 MiB of encoded
records, plus the separately bounded anchor. At 100 ms per tick the count limit
is 204.8 seconds since the latest anchor; encoded size may bind earlier. Overflow
is rejected before publication and preserves the caller's unsaved journal.
Ordinary saves and retries never reset this bound. An explicit reanchor at an
already saved, naturally cache-free boundary can start another bounded segment;
synthetic store tests cross 2048 total ticks through repeated save/reanchor/reopen
cycles. This does not guarantee an eligible lifecycle boundary before exhaustion.
If retained loaded cache state remains, the caller must stop before the cap;
arbitrary cached-checkpoint compaction and indefinite durable play are not promised.

Reanchoring does not erase other source-owned histories. Current independent bounds
include 256 creature generations, 256 KiB per creature owner, 65,536 aggregate
checkpoint history calls and 8 MiB snapshots. Every NPC must be cache-free at the
same boundary; one retired owner cannot authorize dropping another live owner's
history. No periodic death or Home Reset is inserted by persistence.

The executable fixture covers an authored entry-6-shaped dry same-polygon Wander
loop: accepted minimum-distance NOPATH, normal leg, mid-leg save, destruction of
old query objects, fresh reconstruction, arrival/pause/relaunch, and another
save/reconstruction. It compares continuation against uninterrupted execution.
This does not establish authentic content startup, arbitrary movement recovery,
terrain installation, native admission, or delivery of replayed UI/audio effects.
Recovery must remain isolated from external projections until it succeeds.

See [the corridor tests](../benilla-native-corridor-tests/README.md) and
[store durability](README.md) for their separate contracts.
