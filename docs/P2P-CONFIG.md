# Peer configuration

This fork's startup settings live in `config.toml` beside the actual `benilla` executable,
including development builds (`target/debug`, `target/play`, etc.). They are separate from the
stock client's CVar settings in `benilla-config/config.toml`. The terminal's working directory
is never searched. Copy [the example](../config.example.toml) beside your binary and replace its
PSK placeholder before starting. No file means the original realm client; an existing malformed
or unreadable file stops startup before any network connection. Presence of `[p2p]` selects the
experimental peer backend, never a fallback realm connection. The client stays at the login
screen until you deliberately choose Start/Join. This opens local character selection and
creation; **Enter World** starts the listener and peer connections for the selected saved
character. Loading configuration or opening Settings does not dial peers/relays or request
router mappings. The optional synthetic encounter test also requires an explicit start.

Exploration provides saved characters, movement and chat. See [character profiles](P2P-CHARACTERS.md)
for private identity storage, checkpointing and recovery limits. Optional
[`p2p.classic_start_captures`](CLASSIC-EXPLORATION-STARTS.md) selects private source-v2
captures for new-character racial starts. Without it, new characters retain the legacy
Northshire meeting point and emit a warning. NPC population, combat, abilities, quests,
XP and loot remain unfinished. One peer can enter alone without waiting for configured
offline peers. Shared-world simulation and offline-world merging are not implemented.

## Automatic world clock

Optional `p2p.timezone` selects an exact IANA timezone name, for example
`timezone = "Africa/Johannesburg"` in `[p2p]`. Omission means `UTC`; the host's local
timezone is never used. Invalid names stop startup. Saved-character exploration uses the
current wall-clock date and time in that zone for both the minimap clock and world lighting,
refreshing at minute changes, including daylight-saving transitions and system-clock changes.
Keep each computer's system clock synchronized. The initial sample has minute resolution.

All exploration peers must use the exact same timezone name and bundled timezone rules version;
aliases are not normalized. Mismatches and older noon-clock clients fail the existing peer
compatibility check. This is a shared clock convention, not a network time-synchronization
service. Existing local manual lighting overrides still take precedence. Realm clocks and
synthetic encounter clocks are unchanged. The Time of Day admin action remains unavailable
until a supported authoritative backend implements it.

## Peers and secrets

`p2p.psk` is a shared private **passphrase**, not a raw hexadecimal key. Choose at least six
independently random words using a trusted password manager, distribute it privately, and keep
the config private (on Unix, use `chmod 600 config.toml`). Do not reuse an account password or
copy a phrase from this documentation, tests, songs or quotations. A long predictable sentence
is still guessable. There is no default secret: the key must be explicitly present. The public
example phrase is accepted but offers no privacy and should be replaced.

Passphrases may have any length, including an explicitly empty string, up to the 1024 UTF-8
byte resource-safety limit. Short or empty values are accepted but easily guessed; an empty
value provides no private group access. Hexadecimal-looking strings are ordinary passphrases,
never interpreted as raw keys. The
exact decoded TOML string is used: case, leading/trailing spaces, punctuation and Unicode
composition all matter. Nothing is trimmed, normalized or case-folded. All players must enter
the identical passphrase and `network`. Do not send either the passphrase or its derived key in
bug reports, screenshots, issue text or logs. Everyone holding it can authenticate as a group
member. Saved-character sessions separately prove their persistent owner key, but the group
passphrase is not cheat prevention or a password-authenticated key exchange.
Weak phrases remain vulnerable to guessing; Argon2id increases the cost, it does not create
entropy.

### Key derivation and compatibility

Passphrase KDF version **1** uses Argon2id version **0x13**, with **65536 KiB (64 MiB)** of memory,
**3** iterations, **4** lanes and a **32-byte** output. These explicit, fixed parameters follow
[RFC 9106's second recommended profile](https://www.rfc-editor.org/rfc/rfc9106.html#section-4),
using the [RustCrypto Argon2 implementation](https://docs.rs/argon2/0.5.3/argon2/). They are never
negotiated with peers or read from network packets; configuration cannot reduce them. The work
runs once while loading valid startup configuration, before networking. The resulting key is
reused by the existing Noise transport without repeating the KDF for connections or packets.

The 16-byte public salt is the first 16 bytes of SHA-256 over the ASCII bytes
`benilla-p2p-passphrase`, a NUL byte, KDF version `1` as unsigned 32-bit big-endian, then the exact
ASCII `p2p.network` bytes. The passphrase itself goes directly to Argon2id as UTF-8 bytes. SHA-256
is used only to domain-separate the group salt; it is not the password KDF. Same passphrase and
network produce the same key on every platform. This reproducible group salt is not a randomly
unique per-account password-storage salt; choose a distinct network name and avoid reusing your
phrase between groups. No secret, salt-derived identifier or derived key is advertised for
discovery by this configuration code.

**Migration:** peer protocol version **2** and Noise prologue `benilla-p2p-v2` establish an
intentional break from the previous raw-key configuration. Every peer must update. A
64-character hexadecimal `psk` is accepted as literal passphrase text and derived with Argon2id;
it is never used as a raw key or through a fallback, so old raw-key clients cannot join. Changing KDF
version, salt encoding, parameters or passphrase changes the authentication key and requires a
coordinated peer upgrade; rates and local data paths do not affect the key.

Passphrase buffers, derived-key containers and the explicitly allocated Argon2 working memory
are zeroized on drop. This is defense in depth, not a guarantee that a TOML parser, operating
system, crash dump or allocator leaves no copies. Protect the machine and the plaintext config.

`p2p.network` is a public namespace of 1–64 ASCII letters, digits, dots, hyphens or underscores,
defaulting to `benilla` when omitted.
`listen` defaults to `0.0.0.0:7474`. `peers` lists reachable IP:port addresses; IPv6 uses brackets.
The passphrase alone does not provide discovery or connectivity. See internet mode below for NAT traversal. Defaults allow 16 peers
and a 5000 ms timeout; valid ranges are 1–64 and 100–60000 ms respectively. Peer endpoints must
be unique, non-multicast, non-unspecified addresses with nonzero ports. Do not list your listener
as your peer. Unknown configuration fields fail closed so a typo cannot silently change a rule.

The configuration API redacts PSKs from debug output and does not expose serialization. Error
messages withhold source text and parser messages because those can include secrets; syntax and
schema errors report a line/column when available. Config files are limited to 64 KiB.

## Optional iroh connectivity

The canonical table name is `[p2p.iroh]`. Existing `[p2p.internet]` tables remain
accepted as a legacy alias; specifying both is an error. Saving through Connection
setup writes the canonical name while preserving the configured values.

Without `[p2p.iroh]`, the transport is direct TCP. Adding that table selects iroh **1.3.0**
QUIC, with the same Noise PSK authentication and bounded message/session rules over one stream.
Use empty `p2p.peers` in this mode; TCP endpoints are never silently used as a fallback.
Internet mode currently supports exploration; fixed-roster encounters must remain on TCP until
endpoint identities can be bound to their roster.

```toml
[p2p.iroh]
relay_urls = []
peer_tickets = []
port_mapping = false
```

- `relay_urls`: up to 8 explicitly chosen HTTPS relay URLs. Credentials, query parameters,
  fragments and duplicates are rejected. Empty means direct QUIC only: there is no relay
  assistance or fallback. Use a trusted compatible relay you operate or whose service terms
  you accept. Benilla does not create accounts, subscribe, or select public infrastructure.
- `peer_tickets`: private-share JSON endpoint tickets, each at most 8192 bytes and 16 addresses,
  with unique endpoint identities and at most `max_peers` entries. Every relay contained in a
  ticket must also appear in the explicit relay allowlist. Bad input fails before networking.
- `port_mapping`: false by default. Setting true explicitly allows the underlying library to
  probe the local gateway and request UPnP, PCP or NAT-PMP mappings. This can trigger a local
  firewall prompt. It is unnecessary for relayed connectivity and may not work with CGNAT.

A ticket contains an independently random public endpoint identity and its reachable addresses,
not the passphrase, group key or a hash derived from them. Share it privately; it exposes network
metadata. It is maintained in memory for deliberate reveal/copy, never automatically logged or
saved. The identity changes on restart, so get a fresh ticket each time. With a relay configured,
wait until its address appears in the current ticket before sharing it across NATs. Add an
active friend's ticket to your config and start your client; they can accept without your ticket.
For more players, ensure every pair has a link. Tickets do not implement global or LAN discovery.

Iroh can establish an encrypted relay path and attempt direct UDP hole punching, moving between
paths without replacing the application stream or changing game authority. Symmetric NAT,
CGNAT and restrictive firewalls may prevent a direct path; a reachable configured relay can then
carry encrypted traffic. The relay can observe endpoint/network metadata and traffic volume,
but it does not receive the PSK or clear game messages. A relay outage can still disconnect a
session. Use a longer handshake timeout (for example 15000 ms) on slow networks.

The optional [iroh community service](https://www.iroh.computer/blog/shared-relays) is advertised
as free and rate-limited, with [service terms](https://services.iroh.computer/legal/terms).
It is **not configured automatically** and has no promised capacity or availability here.
Self-hosting avoids a vendor account but still needs reachable infrastructure and bandwidth.

Verification distinguishes real local TCP/QUIC and loopback relay tests from internet conditions:
no home router was modified, no live public relay used, and no cross-household NAT/CGNAT success
is claimed. Test those environments with consenting players before relying on this mode.

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

1. Choose a new private passphrase of at least six independently random words and enter
   the identical phrase in both private files. Do not use an example or test fixture phrase,
   and never send the real phrase or derived key in logs. Use matching owner-authenticated character builds on both; old temporary-avatar builds cannot join.
2. For a same-computer test, A listens on `127.0.0.1:7474` and lists
   `peers = ["127.0.0.1:7475"]`; B listens on `127.0.0.1:7475` and lists
   `peers = ["127.0.0.1:7474"]`. Both use the same network name and ruleset.
3. Start both, choose Start/Join, then create or select a local character and Enter World.
   New characters use racial starts by default. Choose the same race to start together,
   or meet on the same map; different-continent avatars are not shown. The login display
   lists configured local endpoints; this is not a reachability test. No realm account is needed.
4. Move, turn, jump, stop and use `/say` on each. Confirm the other sees the movement
   and message. Close one client; the other should remove its avatar. Restart the first;
   select the same saved character and confirm its identity/appearance and last successful
   position checkpoint are restored. Keep separate profile folders for independent players.
5. Change one passphrase, network name or ruleset rate, restart that client, and confirm no
   remote avatar is admitted. Separately verify a 64-hex string is derived as an ordinary
   passphrase, and a protocol-version-1 client cannot join version 2. Connection failures are
   diagnostic events, never a fallback to a realm.
6. For separate machines, replace loopback addresses with reachable peer addresses.
   Internet connectivity needs reachable IPv6 or manually configured TCP forwarding.
   Test every required peer pair. This TCP smoke test does not exercise internet mode.

Headless tests exercise channels, authentication and ECS state, but do not prove rendering,
movement feel or installed-asset compatibility. The actual in-world smoke test requires
client assets and a working display. The exploration contract version exchanged at join
is not a hash or attestation of the player's asset files.

Internet mode currently applies to exploration only. A configuration containing both
`[p2p.iroh]` and `[p2p.encounter]` is rejected before networking starts; the synthetic
encounter still requires its fixed direct-TCP roster. Neither mode silently overrides
the other. Use [Login Settings](P2P-SETTINGS.md) to edit the startup file and restart.
