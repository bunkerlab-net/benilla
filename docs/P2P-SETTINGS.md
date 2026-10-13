# Login Settings

The native login screen's **Settings** panel edits startup configuration and the
client's registered graphics/audio controls. It stages changes until you choose
Save; Cancel discards the draft. Startup networking, passphrase, ruleset and data
path changes require a restart. Saving does not replace an active peer session.

The startup file is `config.toml` beside the executable. Existing comments and
untouched fields are retained. Save validates the complete configuration, checks
for changes on disk, and replaces it using a temporary file in the same directory.
This concurrency check is best effort, not an atomic compare-and-swap with other
editors. Keep the file private: it contains the shared passphrase.

An invalid startup file opens interactive recovery with all networking inhibited;
unattended runs still fail. Semantic errors can be corrected in the form. For
malformed TOML, **Start blank repair** explicitly begins a new draft. Saving it
first preserves the original in a private same-directory backup. Cancelling does
not create a backup or change the file. Review every required field before saving
and restarting; the blank draft does not retain the previous settings.

Passphrases are masked by default. Reveal is deliberate. Endpoint tickets may
contain reachable addresses and relay information; reveal or copy them only when
sharing with intended friends. A ticket does not contain the shared passphrase.

## Diagnostics

- A local TCP test checks the local socket path
- A peer TCP test checks reachability of the configured direct TCP endpoint
- A local PSK test checks authentication and encrypted messaging on loopback

A successful local test does not prove that a remote friend can connect, that a
router mapping worked, or that relay/direct internet traversal works. Use the
matching internet-mode instructions and test with both peers. Tests do not verify
WoW assets, rendering, combat completeness or persistent character saves.

The settings panel and headless tests have not been visually verified against a
real WoW installation in this cloud workspace. See [configuration](P2P-CONFIG.md)
and [synthetic encounters](P2P-ENCOUNTER.md) for the current mode limits.
