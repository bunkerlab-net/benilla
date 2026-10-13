# Peer configuration

This fork's startup settings live in `config.toml` beside the actual `benilla` executable,
including development builds (`target/debug`, `target/play`, etc.). They are separate from the
stock client's CVar settings in `benilla-config/config.toml`. The terminal's working directory
is never searched. Copy [the example](../config.example.toml) beside your binary and replace its
PSK placeholder before starting. No file means the original realm client; an existing malformed
or unreadable file stops startup before any network connection. Presence of `[p2p]` selects the
experimental peer backend, never a fallback realm connection.

## Peers and secrets

`p2p.psk` is exactly 64 hexadecimal characters representing 32 random bytes. Generate it locally
with `openssl rand -hex 32`, distribute privately, and keep the config private (on Unix, use
`chmod 600 config.toml`). Do not send it in bug reports, screenshots, issue text or logs. The
example intentionally has no usable key; constant-byte keys are rejected. Everyone holding this
shared key can authenticate as a group member; it is not individual identity or cheat prevention.

`p2p.network` is a public namespace of 1–64 ASCII letters, digits, dots, hyphens or underscores,
defaulting to `benilla` when omitted.
`listen` defaults to `0.0.0.0:7474`. `peers` lists reachable IP:port addresses; IPv6 uses brackets.
A PSK does not provide peer discovery, NAT traversal or port forwarding. Defaults allow 16 peers
and a 5000 ms timeout; valid ranges are 1–64 and 100–60000 ms respectively. Peer endpoints must
be unique, non-multicast, non-unspecified addresses with nonzero ports. Do not list your listener
as your peer. Unknown configuration fields fail closed so a typo cannot silently change a rule.

The configuration API redacts PSKs from debug output and does not expose serialization. Error
messages withhold source text and parser messages because those can include secrets; syntax and
schema errors report a line/column when available. Config files are limited to 64 KiB.

## Game data

`wow.data` names your own 1.12.1 `Data` directory. Relative paths resolve from the executable's
directory; absolute paths stay absolute. `~` and environment variables are not expanded. The
install is read-only and no Blizzard assets are distributed. `WOW_DATA` remains first priority,
including `WOW_DATA=` disabling data completely. With `wow.data` supplied, the resolver does not
fall through to a different project or bundled directory; without it, the original project and
executable-relative discovery is unchanged. An omitted `[wow]` section uses original discovery.

## Rules

`ruleset.xp_rate` and `ruleset.drop_rate` default to 1.0. Values must be finite, positive,
0.001–1000 inclusive and representable to three decimal places. Integers such as `1` are accepted
and equivalent to `1.0`. Internally rates are integer thousandths. Drop rate means probability
scaling capped at 100%, not quantity scaling. Configuring rates does not by itself implement XP,
loot, combat or a server simulation; the current peer backend must explicitly implement each.

Ruleset fingerprint version 1 is SHA-256 over the exact bytes `benilla-ruleset`, a NUL byte,
then version `1`, XP thousandths and drop thousandths as three unsigned 32-bit big-endian values.
It excludes PSK, local data paths and connection settings. All peers must agree on this semantic
fingerprint before entering a session. It is a compatibility check, not proof of asset ownership,
asset equality, individual identity or cheat resistance. Content/protocol compatibility must be
checked separately by the transport.

## Two-player exploration smoke test

Use two player-build executable folders (`cargo build -p benilla --no-default-features`),
with a `config.toml` beside each executable. Each player supplies their own lawful 1.12.1
Data directory. Keep each process's settings separate; development builds can set a
different `BENILLA_HOME` per process. Never point `BENILLA_HOME` into the WoW install.

1. Generate a new random 32-byte key locally and place the same 64-character hex value in
   both private files. Do not use a fixture key from tests or send the real key in logs.
2. For a same-computer test, A listens on `127.0.0.1:7474` and lists
   `peers = ["127.0.0.1:7475"]`; B listens on `127.0.0.1:7475` and lists
   `peers = ["127.0.0.1:7474"]`. Both use the same network name and ruleset.
3. Start both. Each should enter the fixed Northshire exploration spawn as a temporary
   human warrior and see the other avatar. The P2P startup banner names the listener.
   No realm login or external realm process is needed.
4. Move, turn, jump, stop and use `/say` on each. Confirm the other sees the movement
   and message. Close one client; the other should remove its avatar. Restart the first;
   it joins under a new temporary identity. Characters and movement are not saved.
5. Change one key or one ruleset rate, restart that client, and confirm no remote avatar
   is admitted. Connection failures are diagnostic events, not a fallback to a realm.
6. For separate machines, replace loopback addresses with reachable peer addresses.
   Internet connectivity needs reachable IPv6 or manually configured TCP forwarding.
   Test every required peer pair; no relaying or automatic NAT traversal is provided.

Headless tests exercise channels, authentication and ECS state, but do not prove rendering,
movement feel or installed-asset compatibility. The actual in-world smoke test requires
client assets and a working display. The exploration contract version exchanged at join
is not a hash or attestation of the player's asset files.
