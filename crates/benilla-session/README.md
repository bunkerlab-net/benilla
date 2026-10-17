# Synthetic peer encounter session

Fork-only GPL-3.0-or-later library. It joins `benilla-p2p` to the synthetic
`benilla-sim::encounter` harness. It does **not** implement authentic WoW combat,
assets, spells, database content, movement, rendering, or a full server port.
The Classic 1.12.1 target does not change that boundary. No Blizzard data or
Playerbots code is bundled. Existing simulation provenance and license notices
remain in [`../benilla-sim/PROVENANCE.md`](../benilla-sim/PROVENANCE.md), and the
GPL text in [`../benilla-sim/licenses/`](../benilla-sim/licenses/).

## Integration contract

1. Configure direct links forming a full mesh with `benilla-p2p::Network`.
   Use `should_dial_pair(local, remote)` to configure exactly one dial direction
   per pair: higher transport ID dials lower ID, and every member listens. Do
   not configure both directions: transport duplicate-link replacement emits
   PeerLeft, which deliberately aborts even an unstarted session. This is a
   connection rule only, not leadership or simulation authority.
   All participants already have the same PSK and explicit addresses. Use an
   encounter-specific Hello network/content label, separate from exploration
   traffic. Session owns Network and consumes its events from startup.
2. Out of band, agree on a fresh nonzero epoch, strictly transport-ID-sorted
   `Manifest::roster`, unique simulation IDs, protocol/simulation versions,
   actual rules/content digests, initial state hash and finite tick limit.
   Every peer constructs the same fresh tick-zero `World`. The supplied local
   ID must match its Network Hello; the caller owns that pairing. The initial
   world digest independently commits to all actual rules, content and players,
   so matching caller-provided labels cannot hide different initial worlds.
3. `Session::new(network, local, manifest, world, timeout)` validates the
   manifest and local state. Each member sends its exact proposal to every
   member, then Ready with its manifest digest only after all proposals match.
   `Event::Started` requires every member's Ready. There is no elected peer.
4. Poll regularly. Submit exactly one bounded command vector per current tick,
   including an empty vector for idle input. The input sender is the transport
   event source, never an embedded claimed identity. Commands are Attack or
   StopAttack; targets must exist. The sim processes inputs in simulation-ID
   order and commands in sender order, independent of network arrival.
5. Each peer computes a private candidate state, broadcasts a vote on both the
   ordered input digest and resulting state hash, and exposes a Committed event
   only when every vote matches. `world()` exposes only committed state; the
   caller can retain CommittedTick values and replay from the initial snapshot.
   A missing input/vote never advances the committed world. One next-tick round
   may arrive early, accommodating uneven peer scheduling; nothing further is
   buffered. Future/old epochs, ticks, duplicate inputs/votes, malformed input,
   invalid commands or differing hashes abort.
6. A finite 1 ns–60 s local timeout bounds every start/tick barrier, and a
   manifest-agreed tick limit (1–100,000) finishes the encounter. Timers require
   polling; there is no autonomous session worker. `stop()` aborts explicitly.
   A frozen member leaving, including a transport replacement/reconnect, aborts
   permanently. Restart means a new Session, full re-agreement and fresh epoch;
   do not silently resume or shrink membership. Drop Session to close Network.

Memory is bounded by at most 16 members, two input/vote rounds, 16 commands per
member per round and the existing 16 KiB transport frame/queue bounds. Poll
handles at most 256 transport events. The library does not accumulate a replay
log. The owner may drain events and choose its own bounded retention.

This is trusted-group fail-stop lockstep, not Byzantine consensus: anyone with
the PSK can impersonate a peer or lie about votes. Peers can see a committed tick
at different wall-clock times. A crash or equivocation may stop another peer
before it sees the same commit; there is no atomic distributed persistence,
anti-cheat, reconnect catch-up, public lobby or NAT traversal. Session separation
and fresh epochs are caller responsibilities, not a persistent nonce registry.

## Verification

`cargo test -p benilla-session -- --nocapture` connects actual localhost
TCP/Noise transports (no fake transport for the integration tests). It checks
2- and 3-peer identical eight-tick synthetic combat, XP/loot and verified replay;
independently paced peers; mismatched start; no-progress timeout; duplicate local
and remote inputs; hash mismatch without exposing candidate rewards; stale epoch,
far-future input and command bound rejection; and same-ID reconnect refusal.

`cargo clippy -p benilla-session --all-targets -- -D warnings`

These are headless Linux tests, not a rendered client/gameplay acceptance test.
