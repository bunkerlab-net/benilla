# Local world checkpoints

A small GPL-3.0-or-later persistence crate for `benilla-world-runtime`. It uses
only the existing runtime/data crates, serde_json, SHA-256, std and Unix libc.
It does not link Bevy, access game installs, generate credentials, save character
profiles, start network peers, elect hosts, or merge independently advanced worlds.
All fixtures are authored synthetic test content. The filesystem primitives are
adapted from the original MIT/Apache-2.0 `benilla-profile`; no game assets or world
database rows are included. The runtime's simulation provenance remains in
`../benilla-sim/PROVENANCE.md`.

## App contract

The app must resolve a dedicated world leaf through its normal local-state helper,
e.g. `benilla-config/worlds/<selected-world-id>`. Create the `worlds` parent first;
this crate creates only the leaf. The path is caller supplied, never derived from
an untrusted filename inside a save. `p2p-profile` paths are rejected. Never place
world files in the install, character profile, or coordinate-save directory.

```rust,ignore
// Only after an explicit new-world action, with a newly constructed tick-zero World:
let (mut store, receipt) = WorldStore::create(world_path, &world)?;
receipt.acknowledge(&mut world)?;

// For an existing selected world, including startup after a failure:
let (mut store, mut world) = WorldStore::open(world_path, identity, data, rules)?;

// At an exact tick boundary, e.g. every 100 ticks, on checkpoint_due, and on logout:
store.checkpoint(&mut world)?;
```

`WorldIdentity` explicitly selects the world ID, normalized content digest and
runtime rules digest. The app must obtain these from the selected world/content
configuration, not take identity from whatever save is present. Opening missing,
corrupt, incompatible, or diverged files fails. It never creates a world or
silently selects a backup. New-world creation accepts only tick-zero genesis and
refuses existing checkpoints, backups, and pending recovery files. Store world
selection separately from the character selection; this API receives no keys.

`prepare(&world)` captures immutable data without clearing the journal.
`commit(prepared)` verifies its generation/parent digest, checks disk state under
the exclusive lock, publishes it, reads back the exact bytes, and returns a typed
`CommitReceipt`. Only `receipt.acknowledge(&mut world)` may clear the runtime
journal in this integration. It binds world, tick, revision, content, rules,
state digest, and complete snapshot hash (including the journal chain tip).
Private fields and no deserialization API prevent fabricating a storage receipt
through this crate. The lower-level runtime acknowledgement remains public;
callers must not call it speculatively before storage success.

`checkpoint` does all three steps synchronously. If an app moves the owned store
and preparation to a worker, newer simulation ticks remain safe: an old receipt
is rejected without discarding any history. The next preparation accepts only an
exact extension of the last committed chain. It preserves the older journal base
when the previous receipt was not acknowledged. A previously prepared checkpoint
is rejected after another generation commits. No retry chooses between branches.

Restoration replays a snapshot's associated journal, verifies the final boundary,
then starts an empty in-memory journal at that restored boundary. A caller must
retain the returned World; reconstructing its own journal base is unsupported.
Snapshot and journal schemas/hashes are delegated to the runtime rather than
hardcoded here, so incompatible runtime schema upgrades fail explicitly.

## One publication point, bounded files

The dedicated directory contains:

- `world.checkpoint`: the current checkpoint envelope
- `world.previous`: the previous validated checkpoint after a successful save
- `world.lock`: persistent OS advisory lock, never deleted on normal close
- `.world-checkpoint.pending`: at most one temporary staging file

Each envelope contains a fixed 60-byte header, a bounded canonical runtime base
snapshot, its bounded canonical journal, and a 32-byte domain-separated checksum.
It is at most 16 MiB + 92 bytes with current runtime limits. Framing uses fixed
little-endian lengths, not JSON arrays of byte values. The journal is replayed
against its actual base snapshot; pairing the current snapshot with older history
would be invalid. The final snapshot for the next journal base is reconstructed
and cached, not redundantly stored as an unrelated second authority.

Saving checks the exact current and backup bytes against loaded digests. It
first flushes and replaces the previous-good envelope, then flushes and replaces
the current envelope. Each replacement uses exclusive temporary creation, a full
write, file flush/sync, same-directory rename, and (on Unix) directory sync.
Publication is the single current-envelope rename. This is **not an atomic
multi-file transaction**. An interrupted backup-first save can leave two identical
valid checkpoints; restore explicitly recognizes that case. Otherwise the backup
must be the immediately preceding generation and match the current parent hash.
Missing/corrupt/unrelated backups also fail closed. External file edits are never
silently overwritten, including a changed backup.

Ordinary pre-publication write/sync failures preserve the prior current envelope
and the runtime journal. Ambiguous rename results, post-rename sync failures, or
failed verification return `CommitUncertain`. That instance latches: no further
preparation or publication is allowed until close/reopen and validation. An
external modification likewise blocks writes. Do not discard the runtime journal
on any failed save. Once storage is blocked or its journal cannot safely grow,
pause simulation at a tick boundary and report the error rather than advancing
past the journal cap or claiming an unconfirmed save.

Crash loss is explicit: only the last successfully published checkpoint is
recoverable. A 100-tick save cadence can lose up to 100 not-yet-committed ticks in
a sudden crash; time paused/delayed by a failed save is not a durability promise.
This is checkpoint persistence, not a synchronous per-tick write-ahead log.
Save regularly and when `checkpoint_due()` warns, well before the hard 10,000-tick
or 8 MiB journal limit. Saving on orderly logout alone is insufficient.

Normal failures clean only this attempt's owned staging file. An abrupt process
exit may leave the fixed pending file; it is never read as current, removed, or
reused automatically. A valid current can be opened, but further writes stop
until explicit offline recovery handles the pending file. This bounds crash
debris while preserving evidence. Recovery is deliberately a separate local-user
action; this crate offers no automatic rollback or lossy repair. Preserve files
before repair, use the matching content/rules, and never remove an active lock.

## Platform guarantees and limits

Unix opens every directory component descriptor-relatively with
`O_DIRECTORY|O_NOFOLLOW`, and keeps the final directory descriptor for file opens,
renames and unlinks. The leaf must be owned by the effective user with no group
or other access; new leaf/file modes are 0700/0600. Files must be private,
user-owned, regular, singly linked and non-symlinks. Nonblocking opens reject
FIFOs without hanging. A held lock is checked against its named inode. A renamed
parent cannot redirect writes to a replacement path. Successful receipts report
`FileAndDirectorySynced`; guarantees still require the filesystem and device to
honor those sync calls. This does not promise protection against all controller
caches or platform-specific power-loss behavior.

Windows opens files with `FILE_FLAG_OPEN_REPARSE_POINT`, rejects reparse metadata
and checks every complete drive/share root and descendant, including canonical
extended-length paths. The lock handle refuses delete sharing. Drive-relative
paths, symlinks and junctions are rejected. Ancestor checks remain path-based,
so they do not defend against adversarial parent replacement races. Unlike Unix,
this backend does not enforce inode link counts or private permissions; use a
trusted user-private local tree with appropriate inherited ACLs. It never changes
ACLs for the user. Antivirus and sharing restrictions may reject renames; the
current file is never deliberately unlinked as a workaround.

Rust std cannot portably fsync Windows directories. Receipts explicitly report
`FileSyncedDirectoryBestEffort`: file data is synced and the rename read back, but
sudden power loss may roll back directory entries even after success. Neither
that label nor a Unix fsync is a claim to a portable atomic multi-file transaction.
Other non-Unix targets have only generic symlink guards and are not hardened
platforms. Network filesystems and noncooperating writers are unsupported.

Checksums and replay detect corruption/divergence, not cheating or rollback by
someone who controls the local files. Rewriting both generations can forge a
valid history. External trusted checkpoints would be needed for freshness or
remote authority; that is outside this crate.

## Verification

```sh
cargo test -p benilla-world-store
cargo test -p benilla-world-store --release
cargo clippy -p benilla-world-store --all-targets -- -D warnings
cargo fmt -p benilla-world-store -- --check
```

Tests use temporary synthetic worlds only. Coverage includes more than 12,000
real runtime ticks with save/reopen/continued advancement, pending casts and quest
state, v2 partial loot and stable item identities, a genuine v1 rejection fixture, exact-base journal replay, stale receipts and preparations, equal-state
but different chains, incompatible identities, missing/corrupt files, bounded
reads/framing, seven injected failure points, retry/latch behavior, backup
retention, two-process writer locking, and platform-specific path defenses.
The child lock probe is marked ignored for standalone test discovery but runs
as a subprocess from the main suite. Linux results do not establish Windows or
macOS runtime behavior: native CI must execute this crate on each release target.
No synthetic test proves actual power-loss behavior on every filesystem/device.
