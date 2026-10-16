# Saved P2P characters

Exploration uses a local profile and the existing character-select and character-create
screens. It does not require a realm account or an online peer to play alone.

1. Configure the install and P2P connection in Connection setup, then restart if changed.
2. Choose **Create / select character**. On first use this explicitly creates the local
   profile. It opens no listener, dials no peer and requests no router mapping.
3. Create a named Classic race/class/gender character with the existing appearance controls,
   or select a saved character. Creation is persisted before success is shown.
4. **Enter World** saves the selection, opens the owner-authenticated transport and enters
   immediately. Configured peers may connect later; offline peers do not hold the local
   character at a multiplayer barrier.
5. Logout saves position and returns to the roster. Back returns to login, where the roster
   can be reopened. Normal application exit also checkpoints before the worker stops.

Names are 2–12 ASCII letters, normalized to title case, unique within the active local roster.
There are at most ten active characters and 32 retained records total, including the archive.
Deletion is recoverable; rename remains unsupported.
Appearance must be valid both for the profile's Classic bounds and the actual client DBCs.
The world body display and faction are resolved locally from those DBCs, never trusted from
peer-supplied display IDs. The saved name, race, gender, class and appearance are used for
both local and remote player objects.

## Delete and restore

Select a character, choose Delete, and type the displayed confirmation word. The exact
character shown in the dialog moves into a local archive after saving; Cancel or Escape
leaves it alone. Other characters, the profile identity/signing key and all native-world
saves are preserved. The character disappears from the active roster and stays archived
after restarting. Removing the selected character clears its saved selection.

To restore in the player, choose **Recover Characters** on the local character-select
screen. Select a deleted character (use Next/Previous for additional pages), choose Recover,
and confirm the displayed name with Restore. Cancel or Escape leaves the archive unchanged.
After saving, the character returns to the active roster with its original identity, position
and world saves. This fork-only recovery panel uses the same Vanilla button and dialog art;
it is not available for realm-server characters.

The terminal route is also available. Close the player and open a terminal beside the shipped executable:

```sh
./benilla characters list-archived
./benilla characters restore <32-hex-character-id> --confirm
```

On Windows use `benilla.exe` in Command Prompt or `.\benilla.exe` in PowerShell. Copy the
exact archived ID from the list. These commands are built into the player; no Cargo, game
assets or network connection is needed. They use the same profile directory and `BENILLA_HOME`
as normal startup. `benilla characters --help` prints usage without opening a profile.

Restore refuses to overwrite an active character with the same name or exceed ten active
characters. Archive the conflicting active character (or another to free a slot), then retry.
At 32 retained records, new creation is refused; archive/restore transfers remain available.
Nothing is automatically evicted and no irreversible purge command is provided. Opening a
missing or damaged profile never creates a replacement. A failed/uncertain save requires
closing and reopening before trying again; the list then reflects the validated disk state.

## Storage and privacy

The profile is `p2p-profile/` inside the app's `benilla-config/` state directory: at the
primary checkout in development and beside the executable in a player build. `BENILLA_HOME`
can explicitly select a different state directory, including for running two copies on
one computer. Each profile has its own persistent signing identity; two processes must
not concurrently open the same profile.

Keep the entire profile private. It contains an unencrypted signing key and a previous-good
snapshot. Do not publish, attach, upload or paste profile files or logs containing their
contents. Copying a profile also copies its ownership identity. Copying only the executable
and public configuration does not copy the identity. Opening a missing/corrupt profile never
silently replaces its key. See the [profile storage contract](../crates/benilla-profile/README.md).

Only the selected local character's position is checkpointed, at most once every ten seconds
and on logout, normal exit or session termination. Remote positions are never written into
local saves. A save failure stops that local session and reports the error instead of claiming
that it saved. A crash or forced process kill may lose movement since the last checkpoint.
Map/position/facing persist; zone metadata comes from the local player's loaded terrain.

## Ownership and peer validation

The PSK admits trusted group members. Saved-character sessions additionally prove possession
of each profile's Ed25519 key through the transport's fresh authenticated handshake. They do
not accept legacy PSK-only sessions. The introduction is versioned, contains public character
state only, and must claim the owner authenticated for that connection. Player GUIDs are a
48-bit SHA-256-derived identifier scoped to both owner ID and character ID. Active collisions
are rejected before an existing entity can be overwritten.

Character traits, level, name, appearance and bounded finite positions are validated before
admission. Movement/chat cannot affect a player before its valid introduction. Invalid,
forged or repeated introductions and malformed frames remove only that peer's entity and
connection, leaving the solo world and other peers running. Protocol, exploration contract
and ruleset compatibility remain part of the authenticated handshake.

This is not anti-cheat, global name reservation, account recovery or an authoritative game
server. An owner can edit/roll back their local files, copy their key, or send false but bounded
movement. Different owners can use the same display name. Internet endpoint tickets are still
transport-session-specific, not permanent character identifiers. Different-map peer avatars
are refused until cross-map membership is implemented.

## Deliberate first-slice limits

New characters can use [configured local racial starts](CLASSIC-EXPLORATION-STARTS.md).
Without that configuration, all races retain the legacy Northshire meeting point, with a
diagnostic warning. Existing saved positions are never reset. No world rows are bundled.
Characters remain level one.
Health/power fields are exploration UI baselines, not implemented class statistics. There are
no implemented world NPCs/AI, combat abilities, spells, XP awards, quests, inventory, loot,
professions, trades or progression in this slice. The separate synthetic encounter harness
is unchanged and does not persist its temporary camera-anchor avatar as a playable character.

The login's endpoint list is read-only local interface metadata. “Configured endpoints
(not listening)” means no gameplay listener is active. Addresses include the configured
port; wildcard binds expand to active matching interfaces, including loopback and VPNs.
Neither that list nor “Listening endpoints” is proof of Internet reachability or port forwarding.
