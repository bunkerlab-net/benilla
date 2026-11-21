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
  the active roster. There is no network-wide name registry, offensive-name filter, or rename.
  Local deletion archives the full record for explicit restoration (see below).
- `select_character` and `save_position` accept only IDs already owned by the local profile.
  Call position checkpoints only from local-player state, never from peer messages.
- `characters`, `character`, and `selected_character` expose immutable public state. Canonical
  character bytes are version-one compact, field-ordered serde JSON, not an RFC 8785 claim.

The Classic race/class matrix uses the 1.12 IDs and 40 permitted combinations. Cosmetic
identifiers are bounded by their `u8` wire/storage representation, not guessed global option
counts; **byte bounds do not prove a cosmetic row exists**. The app must validate actual options against the player's own `CharSections`,
`CharHairGeosets`, and `CharacterFacialHairStyles` data before rendering/accepting a character.
No client data is redistributed here. Names/IDs are ordinary clean-room protocol facts; the
numeric IDs already appear throughout `benilla-formats` and `benilla-app`.

`SpawnPolicy::NorthshireMeetingV1` identifies the legacy fork policy: every race started at
the existing P2P exploration meeting point `[-8949.95, -132.49, 83.9]`, map 0, zone 12. It is
**not a canonical racial starting-location table**. Source-backed race-start creation
uses a new policy for new characters, never silent reinterpretation of existing saves. Position
validation allows only maps 0 and 1, finite coordinates within ±20,000 yards, bounded zone IDs,
and finite orientation within ±100 radians. It is not map collision, speed, zone-membership,
teleport authorization, or gameplay anti-cheat validation.

## Files, failure, and durability

The dedicated directory contains `profile.json`, `profile.previous.json` after the first
mutation, and a persistent `profile.lock`. **Both JSON files contain an unencrypted signing
secret. Never log, transmit, publish, or attach them.** They have a strict schema, version,
checksum over deterministic payload encoding, and a maximum size of 64 KiB. Unknown fields,
unsupported versions, invalid characters/owner/selection, duplicate active names or retained IDs, malformed JSON,
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

## Recoverable character removal

The P2P character screen's existing typed `DELETE` confirmation removes exactly that
owner-bound character from the active roster. This fork deliberately archives the full
character record rather than erasing it. The signing identity, other characters, and all
referenced native-world saves remain untouched. The selected character is cleared only when
that character is removed; restoring does not change the current selection. A changed roster
and `CHAR_DELETE_SUCCESS` are sent only after the atomic durable commit succeeds. Cancellation,
stale targets and repeated requests never remove a different character.

There are at most 10 active characters and 32 retained records total (active plus archived).
At the retained limit, creation is refused without evicting anything; archive/restore transfers
still work. There is no irreversible purge command. Archived names can be reused, but restoring
into an active name conflict or a full active roster is refused without replacing anyone.
Archive the conflicting active character, or another active character to free a roster slot,
then restore the desired original ID.

The distributed player includes these recovery commands; no Rust/Cargo installation is needed:

```sh
benilla characters list-archived
benilla characters restore <32-hex-character-id> --confirm
benilla characters --help
```

Close the running player first: recovery uses the same exclusive profile lock. Run the shipped
`benilla` executable (or `benilla.exe` on Windows) from a terminal. The commands use the normal
`benilla-config/p2p-profile` directory beside the player; `BENILLA_HOME` overrides the state
folder as in ordinary startup. Only an existing profile can be opened. Help and invalid arguments
do not open or create an identity. The list shows only names and IDs, never the signing key.
Recovery runs before Bevy, game-asset discovery or any network session, including in player builds.

Profile format v2 adds an archive and a version-specific checksum domain. Existing v1 profiles
remain readable: their original strict schema and checksum are verified before lossless
in-memory conversion. Opening alone never rewrites the file. The next explicit mutation writes
v2 and preserves the byte-exact original v1 file as `profile.previous.json`. A failed pre-rename
write retains original current bytes; an uncertain post-rename write requires reopening before
further actions. Older binaries cannot open v2; never downgrade by copying over current state,
which could discard characters retained by newer versions.
## Saved local-world declarations

The in-game P2P exploration interface exposes **ESC → Admin → Local World Setup**.
Saving a new setup declares the current profile as owner of a newly generated public world ID,
with an explicit immutable list of up to 64 other admin owner IDs. It also selects that setup.
Existing saved setups can be selected explicitly. These actions do not start a native world,
change exploration characters, grant inventory/spells/levels, or establish authority over a
foreign world. The production native capability remains unavailable. Changing admins requires
saving a distinct new world; there is no live-policy editing operation.

`world_setups` reads the separate `local-worlds.json` sidecar. Merely opening a profile or this
screen never creates a declaration. `apply_world_setup` accepts bounded create/select requests
under the profile's existing exclusive lock. Its replay key is a public random session nonce
plus a monotonic request sequence; it is not an authentication credential. An exact duplicate
returns its original world ID, a conflicting duplicate is rejected, and a stale expected
revision cannot change the saved selection. At most 16 worlds and 256 never-evicted setup
receipts are retained, within a 256 KiB file bound. Reaching a bound preserves existing records.

Declarations use Ed25519 with `benilla-local-world-declaration-v1\0`, followed by compact,
field-ordered serde JSON of `(format_version, label, public_key, policy)`. Policy contains world
ID, owner ID derived from that same public key, sorted unique other-admin IDs, and immutable
revision 1. The complete saved state uses a distinct `benilla-local-world-setups-v1\0` domain
and is signed independently, binding selection and replay history. No generic signing API or
secret accessor is exposed. `WorldDeclaration::verify` checks the owner binding and signature;
its `digest` binds the complete signed declaration for a future validated genesis/profile.
Such a future entry path must bind that exact digest, not trust a display name or a claim that
an existing world belongs to the signer. A valid declaration alone never admits gameplay.

The sidecar shares the profile's audited filesystem primitives: bounded private regular files,
last-valid `local-worlds.previous.json`, same-directory temporary writes, flush/fsync/rename,
file-content conflict checks, and an uncertain-commit latch requiring reopen. It does not change
or re-encode the existing profile schema, checksum, signing key, character roster or position.
Missing-current/previous-present, corrupt, foreign-profile, unsupported, or unsafe-path setup
files are preserved and rejected. Setup failure does not silently replace a profile or switch
worlds. The UI reports queued/pending until the profile worker confirms durable save, and keeps
ambiguous results latched across UI reloads. After reopening, inspect saved worlds before
retrying an uncertain creation.

All platform durability and local-owner limitations above apply equally to the sidecar.
Signatures detect unauthorized changes by someone without the key; the operating-system owner
can still copy keys and roll back complete saved directories. This is not global world-name
ownership, anti-cheat, a remote permission service, or distributed admin consensus.

## Configured local racial starts

`starting_location::ClassicStart` reads the selected `playercreateinfo` position from
an existing private `benilla-classic-source-slice-v2` capture.
`ProfileStore::create_character_from_start` explicitly opts a new character into that
selected map, zone, XYZ and orientation. A missing, mismatched, invalid or unsupported
race/class does not fall back to Northshire. The original `create_character` API retains the legacy policy. The app uses the
source-backed API when `p2p.classic_start_captures` is configured; otherwise its
normal creation path supplies built-in racial positions to `create_character_at_racial_start`
and records `ClassicRacialV1`. Neither path reinterprets existing character positions.
See [the integration boundary](../../docs/CLASSIC-EXPLORATION-STARTS.md).

The new policy records the exact capture byte digest. Every login, movement checkpoint,
reopen and archive restoration continues to use the saved position; no table is needed
again and no legacy character is reset. Older builds do not understand the new policy;
back up the private profile before testing an explicitly opted-in character, and do not
expect such a profile to open in an older build.
