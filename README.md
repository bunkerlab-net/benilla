<div align="center">
  <h1>benilla</h1>
  <p><b>A complete World of Warcraft 1.12.1 client, written from scratch in Rust and <a href="https://bevy.org">Bevy</a></b></p>
  <p>
    <a href="https://discord.gg/wJSJx467G4"><img src="https://img.shields.io/discord/1529280129518538922?style=for-the-badge&logo=discord&logoColor=white&label=discord&color=5865F2" alt="Discord"></a>
    <a href="https://www.youtube.com/playlist?list=PLdCnpZNKxyb8"><img src="https://img.shields.io/badge/devlog-youtube-FF0000?style=for-the-badge&logo=youtube&logoColor=white" alt="YouTube devlog"></a>
    <a href="DISTRIBUTION-NOTICE.md"><img src="https://img.shields.io/badge/combined_player-GPL--3.0--or--later-blue?style=for-the-badge" alt="Combined player license"></a>
  </p>
</div>

## This fork: experimental peer-to-peer mode

This fork adds an **experimental trusted-friends P2P exploration backend**. A
`config.toml` beside the executable selects it with `[p2p]`, supplies a locally kept
shared passphrase of any length (derived into a PSK at startup; strong random words recommended), and points `[wow].data` at your own read-only 1.12.1 install. See
[configuration and security](docs/P2P-CONFIG.md). No realm process, login account,
matchmaking service or elected game host runs in P2P mode.

- [Login Settings](docs/LOGIN-OPTIONS.md): the existing WoW-style Options window; a separate
  [Connection setup](docs/P2P-SETTINGS.md) edits startup config and runs explicit diagnostics
- [Saved characters](docs/P2P-CHARACTERS.md): local Classic character creation/selection,
  persistent owner identity, appearance and position checkpoints; deliberate Enter World
- Implemented: authenticated, encrypted TCP and optional iroh QUIC peer links; matching network,
  protocol, exploration-contract version and ruleset fingerprints; owner-authenticated saved
  avatars with optional [local source-backed racial starts](docs/CLASSIC-EXPLORATION-STARTS.md)
  (legacy Northshire default when unconfigured); peer movement and `/say`; join/disconnect cleanup
- Connectivity: explicit peer addresses on a LAN, reachable IPv6, or manually
  forwarded TCP ports by default. Opt-in `[p2p.internet]` uses private endpoint tickets,
  iroh NAT hole punching and encrypted relay fallback through explicitly configured
  relay URLs. Router mapping (UPnP/PCP/NAT-PMP) is off unless explicitly enabled.
  Each participant still needs a logical link to every other participant. A passphrase
  alone cannot discover peers; no public discovery or relay service is enabled by default.
  Real household NAT/CGNAT coverage has not been verified
- Not implemented in P2P gameplay: combat/AI, spells, XP awards, drops/inventory,
  quests, progression, durable shared-world simulation, dynamic
  simulation membership, authority transfer, dungeons, raids, economies or PvP
- `[ruleset]` is validated and checked for equality when peers join. Its XP/drop
  settings do not create a missing gameplay system and do not currently award
  anything in exploration mode
- [Classic parity ledger](docs/CMANGOS-PARITY.md) tracks the named ports and remaining
  dependencies. A [local source exporter](docs/CLASSIC-WORLD-IMPORT.md) captures bounded
  user-owned server data; its output is not yet runnable Classic content. Separate
  world runtime/client/store crates test original solo combat, quests, per-slot loot,
  protocol projection and durable checkpoints. The app now contains a tested
  command/tick/checkpoint controller, but native-content entry remains gated; these
  mechanics are not yet available through the player UI
- Native component [format 38](crates/benilla-world-runtime/src/native/README.md#initial-cast-genesis)
  binds seed-independent initial-cast plans and per-life creature initialization
  to shared-RNG receipts. Creature selection owns current/historical stats through
  actual death and respawn; equipment and production content remain gated.
  Earlier native saves reject without migration; RNG v2, original World
  v2 and character saves retain their own contracts. Authored fixtures establish
  this mechanism only; actual Human source order/data and production admission
  remain unproved
- `benilla-sim` contains GPL-covered CMaNGOS-derived deterministic XP, selected
  stat/armor, quest-reward and target-validation primitives, plus bounded native
  spell/AI/melee, creation/equipment and death reducers with provenance and parity
  tests. An opt-in [current-pose facing service](crates/benilla-world-runtime/FACING-AUTHORITY.md)
  derives bearing and front arcs using pinned software arithmetic and replay checks.
  A separate native runtime/checkpoint path is tested but still awaiting
  real data/geometry admission. The crate also has an original headless fixed-roster
  encounter harness with deterministic combat fixtures, XP, guaranteed loot,
  snapshots and replay hashes. This is preparatory simulation code, not a full
  server port; an explicit [synthetic encounter mode](docs/P2P-ENCOUNTER.md) now
  links it through local chat controls, separate from exploration
- `benilla-session` adds a separate headless full-mesh adapter with agreed roster/epoch,
  matching input/state votes and fail-stop barriers. Its tests use synthetic encounter
  fixtures; opt-in encounter mode uses a fresh session and fixed configured roster.
  Local confirmed ticks do not
  provide atomic distributed persistence or anti-cheat

The PSK admits group members; short or empty values are accepted but provide little or no
private group access. Saved-character sessions additionally require a persistent owner-key
signature bound to the encrypted connection. Character introductions and movement are bound
to that authenticated owner. This does not prevent a member cheating about their own state,
editing/restoring their own save, or causing denial of service. Invalid character frames are
quarantined instead of being applied to another owner. The exploration-contract label is not
an attestation of installed assets. Synthetic encounter mode remains a separate temporary-avatar
fixture with an explicitly configured roster; it does not award persistent character rewards.

Without a `[p2p]` table, the existing realm-client path remains available. The
following feature list describes that path, with a compatible external 1.12.1
server supplying gameplay simulation; it does **not** describe P2P completeness.

### Licensing

The original client code remains MIT OR Apache-2.0 under its existing notices.
`crates/benilla-sim`, `crates/benilla-session` and `crates/benilla-encounter` are GPL-3.0-or-later. The
combined player now links these crates; its distribution is GPL-covered even when
launched in exploration or realm mode. See [distribution requirements](DISTRIBUTION-NOTICE.md). The
simulation crate retains CMaNGOS notices and source references in [its provenance](crates/benilla-sim/PROVENANCE.md). An executable
that links these crates is a GPL-covered combined work; distribute its corresponding
source and the applicable GPL notices and license. The root permissive license
files are not a license for the CMaNGOS-derived crate. No Blizzard assets or world
database are bundled.

## Existing realm-client features

benilla plays the whole game: character creation, questing and professions, dungeons and raids,
battlegrounds and honor, groups, guilds, trade, mail and the auction house, on the stock 1.12
interface and with your addons. It connects to a 1.12.1 server over the original protocol and
reads the game's data from your own 1.12.1 install. Every file format, the network protocol and
the interface engine are written from scratch, with no original client code, no third-party WoW
crates and no bundled game assets.

## What's inside

- **Formats:** readers for the whole asset stack (the MPQ patch chain, BLP, DBC, ADT, WDT, WDL, M2
  and WMO), wired into Bevy as an asset source.
- **World:** terrain streamed out to the horizon, portal-culled buildings with interior lighting,
  doodads and ground clutter, swimmable water, sky and weather, and the client's own day/night
  lighting, fog and gamma.
- **Models:** GPU-skinned M2s with the full animation controller, particles, ribbons and the spell
  visuals, and characters end to end: customization, the armor composite, weapons with their
  enchant glows, forms, stealth and mounts.
- **Movement:** networked movement in both directions, the server-granted modes from slow fall to
  roots, a follow camera with collision, boats, zeppelins and taxi flights.
- **Networking:** SRP6 auth through world-session crypto and the object mirror into the ECS,
  covering the game from movement and chat through combat, spells, groups and raids, quests,
  trade, mail, the auction house and battlegrounds.
- **Interface:** a FrameXML and Lua engine that runs the stock 1.12 interface off your install's
  patch chain, and third-party 1.12 addons: Questie, pfUI, Bagnon, Bartender2 and most others
  run. By choice, the options window and the ESC menu follow the Classic Era client's rather than
  1.12's.
- **Audio:** music, ambience and effects under the client's own selection and crossfade rules,
  with interior and underwater transitions and zone reverb.

The format readers and the UI engine core are plain Rust with no Bevy in them, and the world
renderer runs with no game attached. [`docs/MAP.md`](docs/MAP.md) maps every crate and subsystem,
generated from the code.

## Upstream realm-client status

The following describes the external-server client, not this fork's incomplete P2P simulation.

Complete and fully playable. What is left:

- The long tail of small features that separates a working client from a finished one, tracked
  as [issues](https://github.com/samwhosung/benilla/issues).
- Addons, options and performance, ongoing.

benilla is a faithful 1.12.1 client and the foundation people build on. There are no prebuilt
downloads: a packaged build, a particular server's changes or anything 1.12.1 never had belongs
in a fork, and forks are welcome. GitHub lists
[every public fork](https://github.com/samwhosung/benilla/forks).

Not planned: other expansions or client versions, Warden (anticheat).

## Running it

benilla builds and runs on macOS, Linux and Windows. You need:

- **An English 1.12.1 client (build 5875)** for the game data. benilla only reads it.
- **A 1.12.1 server with Warden off.** [vmangos](https://github.com/vmangos/core) is what
  development runs against, and it ships with Warden off; cMaNGOS and the other 1.12.1 cores speak
  the same protocol.
- **Stable Rust and a C compiler**, because the client's Lua is built from source: on macOS the
  Xcode command line tools, on Linux the ALSA and udev development packages and pkg-config, on
  Windows the MSVC build tools that the Rust installer sets up.

```sh
WOW_DATA=/path/to/WoW/Data cargo run --release -p benilla
```

On Windows, in PowerShell:

```powershell
$env:WOW_DATA="C:\path\to\WoW\Data"; cargo run --release -p benilla
```

Each release is a tag on the [Releases page](https://github.com/samwhosung/benilla/releases):
`git checkout <tag>` first runs that release, and `main` is the development tip.

`WOW_DATA` names the install's `Data` folder; a link to the install named `WoW` at the repo root
does the same (`ln -s /path/to/WoW WoW`, or on Windows a junction, which needs no admin rights:
`New-Item -ItemType Junction -Path WoW -Target C:\path\to\WoW`). The server defaults to
`localhost:3724`, the stock auth port. Point `WOW_HOST` at another (`WOW_HOST=play.example.com`, or
`play.example.com:5000` for a remapped port), or set it from the Realmlist button on the login
screen, which remembers it. Credentials go in at the login screen, or set `WOW_USER` and `WOW_PASS`
to skip it.

Settings, screenshots and addons live in `benilla-config/` at the repo root: a 1.12 addon goes in
`benilla-config/AddOns/`. [`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md) has the rest, from the
player build to the tests.

## Contributing

Issues and pull requests are open. [`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md) says where to
start, what gets in, how a change is judged and what happens to a pull request once it is open.
Bugs, questions and ideas are welcome on the [Discord](https://discord.gg/wJSJx467G4) too.

---

Early inspiration and file format guidance came from the
[wowemulation-dev](https://github.com/wowemulation-dev) community, and
[warcraft-rs](https://github.com/wowemulation-dev/warcraft-rs) in particular.

benilla is an independent fan project, not affiliated with or endorsed by Blizzard Entertainment.
It ships **no Blizzard content**: no art, models, sounds, maps, MPQ contents or FrameXML. You
provide your own legally obtained 1.12.1 client, and the stock interface runs off its FrameXML at
runtime. The few files under `crates/benilla-app/assets/ui/` are our own, not copies of it:
adapters over stock files, and the settings windows and script error log benilla draws itself.

World of Warcraft is a trademark of Blizzard Entertainment, Inc. Our own code is licensed under
[MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), at your option. The two vendored components
under `third_party/`, the kira audio engine and a Lua 5.1 patched to the 1.12 client's dialect,
keep their own upstream licenses, alongside each.
