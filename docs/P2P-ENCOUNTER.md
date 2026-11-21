# Opt-in synthetic test encounter

This is a small **synthetic, text-controlled encounter**, not WoW combat or a full
Classic 1.12.1 server. It uses the existing client chat window as a test panel.
Your own read-only install is still needed to open the graphical client. The
ordinary local avatar is a camera anchor; movement, range, action bars, targeting,
character HP/XP and bags are unrelated to this non-spatial test. No creature model
or real item is fabricated. Test-token ID 1 is private to the simulation and is
never sent to WoW item lookup or inventory APIs. No state is saved.

## Configure a frozen group

Start with [the P2P configuration](P2P-CONFIG.md). Keep `[p2p]` and the shared key,
network name and ruleset identical as usual, with a different local listening
endpoint for each member. Set `p2p.peers = []`: encounter mode obtains its full
mesh from the roster instead. Append this separate table to `config.toml` beside
the executable (after any existing tables):

```toml
[p2p.encounter]
# Public ID for this participant, 16 bytes hex. Different on each machine.
local_id = "01010101010101010101010101010101"
# Public fresh 32-byte session nonce, identical for everyone. Generate a fresh
# value for EVERY restart using: openssl rand -hex 32. Never reuse the example.
epoch = "REPLACE_WITH_FRESH_64_HEX_SESSION_NONCE"
tick_limit = 1200
# Same complete roster on EVERY machine, including yourself. These endpoints
# must be directly reachable; no central host, relay, discovery or NAT traversal.
roster = [
  { id = "01010101010101010101010101010101", address = "192.168.1.10:7474" },
  { id = "02020202020202020202020202020202", address = "192.168.1.11:7474" },
]
```

For the second machine, change only `local_id` (and its ordinary local listen
setting). There must be 2–16 distinct nonzero IDs and distinct reachable unicast
addresses. The local roster address matches `p2p.listen`, or its port when listen
uses a wildcard address. `p2p.max_peers` must cover the other roster members.
IDs are sorted to assign test player numbers deterministically, so roster table
order does not change identity. Higher IDs dial lower IDs; every member still
listens, simulates and votes. No member is authoritative. The PSK authenticates
group membership, not individual IDs: trusted members can impersonate one another.

`ruleset.xp_rate` affects synthetic XP only. `ruleset.drop_rate` must equal 1;
this encounter has one guaranteed test token per kill, not implemented random
loot/drop chances. Players and the target start with 100 HP; players hit for 10,
the target hits for 5, with four-tick swings. Players respawn after 20 ticks and
the target after 40. These are labeled test fixtures, not claimed WoW formulas.

## Play through chat

Enter commands into the existing chat box, including `/say`:

- `/say !ready`: accept the displayed fixed roster and propose the configured
  manifest. Until this explicit action, no encounter socket is opened. Everyone
  must become ready within 60 seconds of the first ready. All must match the
  epoch, protocol, rules, content and initial state before any simulation advances
- `/say !attack`: queue an attack on synthetic target #1 for your next unsent
  logical tick. While attack is armed, it is resubmitted every tick, including
  after player or target respawn; use `!stop` to disarm it
- `/say !stop`: queue stop-attack for your next unsent tick
- `/say !pause`: stop supplying input at your next unsent tick. An input already
  submitted may still commit. The next barrier freezes the entire group, including
  target swings/respawns. This does not extend the existing 60-second deadline
- `/say !resume`: resume inputs before timeout; other missing peers still block
- `/say !status`: inspect the last all-peer-confirmed tick, per-player HP/XP/test
  tokens, target HP, last local submitted tick and missing input count. Submitted
  input is not a committed outcome; zero missing inputs can still mean votes pending
- `/say !leave`: abort. Every other member stops rather than shrinking the roster

Every tick needs each frozen member's input and matching state/input votes. The
local driver supplies at most one input every 250ms and never skips a logical tick.
Ordinary chat is not transmitted in this mode. Combat results appear as system
chat lines only after confirmation, and `!status` can show them again.

A timeout, disconnect, malformed frame, differing hash or peer abort is terminal.
Final reports retain only confirmed outcomes; unconfirmed rewards are discarded.
At the tick limit, the finished client keeps its transport open for up to one
60-second barrier window so queued final votes can reach other peers, then closes
it. This is not an atomic distributed completion guarantee: process exit or
explicit leave can still discard queued votes and leave different final confirmed
ticks on different peers.
Restart all clients with a fresh shared epoch. Repeated `!ready` cannot reset an
active or failed session. Removing `[p2p.encounter]` restores exploration mode on
restart; there is no in-place network-envelope transition.

## Tests and limits

`cargo test -p benilla-config -p benilla-encounter -p benilla-session` covers
configuration, actual localhost controller peers, readiness, attack, pause/resume,
confirmed rewards, disconnect and the session's adversarial-frame tests.
The app's `net::p2p::encounter` test exercises the chat bridge and proves the lobby
opens without binding its still-occupied endpoint. A graphical player run with
owned assets is separate validation, not implied by headless tests.

All peers are trusted and available, and every peer must directly reach the mesh.
No reconnect, dynamic roster, catch-up, rollback, persisted progression,
Byzantine protection or atomic distributed saves are provided. This opt-in mode
links GPL-covered simulation: see the root licensing notice and distribution
instructions before shipping binaries. Never package Blizzard assets or databases.
