# Bounded native reconstruction recipes

`native::reconstruction::RecipeStore` is a separate persistence format for a
fresh cache-free genesis and its complete accepted tick history. It uses the
existing exclusive lock, backup-first publication and durability rules, with
magic `BNNREC1\0`, envelope version 1 and checksum domain
`benilla-native-reconstruction-store-v1`. It does not change native runtime
checkpoint compatibility or migrate ordinary `NativeStore` files.

The store restores only the genesis through the caller's checked local
constructor, verifies its exact checkpoint and cache-free reopen guard, then
privately receipt-replays the records. It exposes an immutable recipe and
expected checkpoint, never that replayed world. Adjacent generations must have
the identical anchor and an exact record prefix. Saving appends the accepted
suffix even after the ordinary runtime journal is cleared.
Preparation decodes its bounded wire envelope before publication, so returned
recipe records contain no process-local dispatch seals even before reopening.

## Loaded ownership

The app-private `source_cache_reconstruction` adapter requires a tick-zero world
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
records, plus the separately bounded genesis. At 100 ms per tick the count limit
is 204.8 seconds; encoded size may bind earlier. Overflow is rejected before
publication and preserves the caller's unsaved journal. Retrying a save cannot
remove this lifetime limit. No recipe truncation or cached-state compaction is
implemented; a caller must stop before exhaustion rather than promise indefinite
durable play.

The executable fixture covers an authored entry-6-shaped dry same-polygon Wander
loop: accepted minimum-distance NOPATH, normal leg, mid-leg save, destruction of
old query objects, fresh reconstruction, arrival/pause/relaunch, and another
save/reconstruction. It compares continuation against uninterrupted execution.
This does not establish authentic content startup, arbitrary movement recovery,
terrain installation, native admission, or delivery of replayed UI/audio effects.
Recovery must remain isolated from external projections until it succeeds.

See [the corridor tests](../benilla-native-corridor-tests/README.md) and
[store durability](README.md) for their separate contracts.
