# Login Settings

The login screen's **Settings** button opens the [existing WoW-style Options window](LOGIN-OPTIONS.md),
with its normal categories and typed graphics/audio/interface controls. **Connection setup** is
a separate native, WoW-skinned editor for connection configuration and explicit diagnostics.
Its **Connection**, **Iroh**, **World** and **Encounter** groups replace numbered pages.
Each field has a readable label and a brief explanation; boolean controls are checkboxes.
Switching groups keeps edits and the selected field in each group. Saving keeps the current
group and selection instead of resetting to the beginning.

The startup RealmList shortcut is removed. Legacy realm addresses remain configurable through
the normal `realmList` CVar in the client's local settings file. In P2P mode, the login display
shows configured local IP addresses and port; a wildcard expands active local interfaces,
including loopback and VPNs. These addresses do not prove external reachability, and no listener
starts merely because they are displayed.

The startup file is `config.toml` beside the executable. Existing comments and
untouched fields are retained. Save validates the complete configuration, checks
for changes on disk, and replaces it using a temporary file in the same directory.
This concurrency check is best effort, not an atomic compare-and-swap with other
editors. Failed validation or a conflicting disk edit leaves the draft available for correction.
Close or Escape discards unsaved edits. Unchecking P2P, Iroh or Encounter removes that
configuration section when saved, including its stored connection values; the checkbox help
identifies what will be removed. Keep the file private: it contains the shared passphrase.

## Applying saved settings

Supported P2P network, passphrase and ruleset changes can apply without restarting the game.
At an idle P2P login screen, the previous service is retired before new settings are installed;
no transport opens until the normal Start/Enter action. Saving unchanged settings is a no-op
for the active connection and does not replace endpoint tickets.

If a session or character roster is active, applying requires an explicit disconnect choice.
The current worker must finish its position checkpoint and release the profile before a new
service can be prepared. The app returns to login; it does not silently rejoin or change the
selected character. A checkpoint failure or worker panic blocks replacement and reports the
problem, rather than claiming the settings were applied. Saved settings and the currently
running connection are separate until applying succeeds. **Keep current connection**
leaves the running settings alone; return to login and Save again to apply the saved
values, or restart later. A persistent unsaved-draft indicator distinguishes edits
made after Save from the saved settings being applied.

Changing the game-data path still requires a restart because assets were loaded at startup.
Switching between realm and P2P backends, and offline/recovery/unattended processes, also
requires restarting. The panel reports these exceptions rather than claiming every setting
can be applied live. Reapplying Iroh settings replaces the endpoint identity: exchange a fresh
ticket after the new connection starts.

An invalid startup file opens interactive recovery with all networking inhibited;
unattended runs still fail. Semantic errors can be corrected in the form. For
malformed TOML, **Start blank repair** explicitly begins a new draft. Saving it
first preserves the original in a private same-directory backup. Cancelling does
not create a backup or change the file. Review every required field before saving
and restarting; the blank draft does not retain the previous settings.

Passphrases are masked by default. Reveal is deliberate. Any length, including empty, is
accepted up to 1024 UTF-8 bytes; short or empty values carry a non-blocking security warning. Endpoint tickets may
contain reachable addresses and relay information; reveal or copy them only when
sharing with intended friends. A ticket does not contain the shared passphrase.

## Diagnostics

- A local TCP test checks the local socket path
- A peer TCP test checks reachability of the configured direct TCP endpoint
- A local PSK test checks authentication and encrypted messaging on loopback

A successful local test does not prove that a remote friend can connect, that a
router mapping worked, or that relay/direct internet traversal works. Use the
matching iroh-mode instructions and test with both peers. Tests do not verify
WoW assets, rendering, combat completeness or persistent character saves.

The settings panel and headless tests have not been visually verified against a
real WoW installation in this cloud workspace. See [configuration](P2P-CONFIG.md)
and [synthetic encounters](P2P-ENCOUNTER.md) for the current mode limits.

The optional QUIC transport appears as `p2p.iroh.*` in the settings list. Existing
`[p2p.internet]` configuration tables are accepted as an alias and normalized to
`[p2p.iroh]` when saved. Configurations containing both names are rejected so no
transport settings are silently ignored. The rename does not enable iroh or add
relays automatically.
