# benilla-profile

Fork-only local owner identity and level-one exploration character state. No Bevy dependency,
world database, Blizzard assets, realm service, account service, or gameplay rewards.

## API and trust boundary

`ProfileStore::create(directory)` is the explicit persistent-identity creation operation;
`open(directory)` never generates a replacement key, repairs corruption, or restores a backup.
The app must resolve the directory under its existing local-state/config path and obtain the
user's deliberate create-profile action first. Parent directories must already exist. Never
point this API into the user's game install or automatically retry `create` after `open` fails.

A store holds one exclusive OS file lock until dropped. Its `identity()` returns an
`Arc<ProfileIdentity>` exposing only a public key, its derived owner ID, and a narrowly
scoped signing function; this immutable handle can be passed to the network without locking
or sharing the mutable store. There is no secret accessor, Debug implementation, or public
serialization for identity/store. Ed25519-dalek zeroizes the signing key on final drop;
serialized secrets and complete secret-containing file buffers use `Zeroizing`.

- Owner ID: first 16 bytes of SHA-256(`benilla-owner-id-v1\0` || 32-byte public key)
- Transport signature: Ed25519(`benilla-transport-identity-v1\0` || 32-byte challenge)
- The transport must derive that challenge from its fresh authenticated connection transcript,
  both parties' claims, and protocol context. This API alone does not establish a connection.
- `create_character` validates before writing, creates a random 128-bit nonzero ID and selects
  it. Names are 2–12 ASCII letters, normalized to initial-capital lowercase and unique within
  the profile. There is no network-wide name registry, offensive-name filter, or rename/delete.
- `select_character` and `save_position` accept only IDs already owned by the local profile.
  Call position checkpoints only from local-player state, never from peer messages.
- `characters`, `character`, and `selected_character` expose immutable public state. Canonical
  character bytes are version-one compact, field-ordered serde JSON, not an RFC 8785 claim.

The Classic race/class matrix uses the 1.12 IDs and 40 permitted combinations. Cosmetic
integers are conservatively bounded to 0–15 (facial hair 0–8); **bounds do not prove a cosmetic
row exists**. The app must validate actual options against the player's own `CharSections`,
`CharHairGeosets`, and `CharacterFacialHairStyles` data before rendering/accepting a character.
No client data is redistributed here. Names/IDs are ordinary clean-room protocol facts; the
numeric IDs already appear throughout `benilla-formats` and `benilla-app`.

`SpawnPolicy::NorthshireMeetingV1` persists an explicit fork deviation: every race starts at
the existing P2P exploration meeting point `[-8949.95, -132.49, 83.9]`, map 0, zone 12. It is
**not a canonical racial starting-location table**. A future genuine race-start implementation
requires a new policy and migration, not silent reinterpretation of existing saves. Position
validation allows only maps 0 and 1, finite coordinates within ±20,000 yards, bounded zone IDs,
and finite orientation within ±100 radians. It is not map collision, speed, zone-membership,
teleport authorization, or gameplay anti-cheat validation.

## Files, failure, and durability

The dedicated directory contains `profile.json`, `profile.previous.json` after the first
mutation, and a persistent `profile.lock`. **Both JSON files contain an unencrypted signing
secret. Never log, transmit, publish, or attach them.** They have a strict schema, version,
checksum over deterministic payload encoding, and a maximum size of 64 KiB. Unknown fields,
unsupported versions, invalid characters/owner/selection, duplicate names/IDs, malformed JSON,
and checksum failure are errors. `open` fails closed even if a good backup exists.

Each mutation validates a candidate, compares current file bytes with the loaded snapshot,
then saves the last validated snapshot as `profile.previous.json` before replacing current.
Both replacements use an exclusive random same-directory temporary file, full write, flush,
file sync, and rename. Memory changes only after the write succeeds. A pre-rename failure
leaves the last valid current file intact; a post-rename directory-sync failure reports
`CommitUncertain`, preserves the previous snapshot, and prevents further mutations until
reopen. Never blindly retry that error. A crash may leave private `.profile-*.tmp` files; they
are never loaded automatically. Recovery or deletion is a separate explicit local-user action.

The lock is never removed during normal operation. Cooperating processes cannot open the same
profile concurrently. An unexpected on-disk edit fails with `ConcurrentModification` instead
of silently overwriting it. This is protection against accidental concurrent writers, not a
security boundary against the operating-system account that owns the profile.

### Unix

The leaf directory must belong to the current effective user and have no group/other access
(new directory mode 0700; files 0600). Existing permissive paths are refused, not chmodded.
Every directory component is opened relative to a held parent with `O_NOFOLLOW|O_DIRECTORY`;
all subsequent file opens/renames/unlinks use that held directory descriptor. Files use
`O_NOFOLLOW`, must be regular and singly linked, and must belong to this user with no group or
other access. Symlinks, hardlinks, FIFOs, and renamed-parent traversal cannot redirect a write.
File data and directory metadata are synced; guarantees still depend on the filesystem and
storage honoring fsync. Use a local filesystem with working advisory locks and atomic rename.

### Windows and other non-Unix targets

Windows uses Rust's OS file lock, same-directory rename, and pre-rename file flush. Reparse
points (including junctions) and symlinks are rejected before operations; file handles use
`FILE_FLAG_OPEN_REPARSE_POINT` and reject reparse metadata. Unlike the Unix implementation,
ancestor checks are path based and are not race-resistant against an adversary able to replace
parent directories. Use a trusted, user-private local directory tree. No ACL is granted or
changed by this crate; the application/user is responsible for private inherited ACLs.

Path checks start at the complete drive or share root, including extended-length Windows
paths returned by `canonicalize`, then inspect each descendant. Drive-relative paths such as
`C:profile` are refused because they do not resolve against the process's current directory.

Rust std has no portable directory fsync on Windows: sudden power loss may roll back the
renamed directory entry even after successful return. A preserved previous-good snapshot
supports explicit recovery. Antivirus/open-handle sharing can cause rename failure; current
is never deliberately unlinked first. Network filesystems and concurrent noncooperating
writers are unsupported. Non-Unix/non-Windows targets only have generic symlink guards and
must not be presented as hardened profile storage without platform review.

The checksum detects accidental corruption, not malicious editing. Owners can edit files,
recompute checksums, clone identities, or roll back their own backups. This first slice does
not promise anti-cheat, unique humans, globally unique names, or authoritative progression.

## Verification

`cargo test -p benilla-profile` uses only generated temporary fixtures, never a player identity
or profile. `cargo clippy -p benilla-profile --all-targets -- -D warnings` and
`cargo fmt --all -- --check` cover the platform compiled by the caller. Unix and Windows seams
need their own CI/runtime coverage; Linux tests do not prove Windows crash behavior.
