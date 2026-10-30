# Source-backed reputation panel controls

The existing stock `FactionToggleAtWar`, `SetFactionInactive` and `SetFactionActive`
Lua sends now reach the native owner through `SetFactionAtWar` and
`SetFactionInactive`. This is a bounded component/controller integration, not
production native admission or a claim of user-verified graphical gameplay.

## Source and ownership

CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

- `src/game/Entities/CharacterHandler.cpp`: `HandleSetFactionAtWarOpcode`,
  `HandleSetFactionInactiveOpcode`.
- `src/game/Reputation/ReputationMgr.cpp`: both `SetAtWar` overloads and both
  `SetInactive` overloads. The existing primitive's source oracle and provenance
  are in `benilla-sim/REPUTATION.md`.

The packet addresses a **reputation-list slot**, not a faction entry. Source has
no living/combat guard at this handler boundary. The stock client may suppress
its own at-war send during combat; that is not an invented server guard.
Unknown slots are silent no-ops. Hidden/forced-invisible factions reject both
at-war directions. Peace-forced war entry compares rank of raw standing (not
standing plus base). Repeated war-on is a no-op, whereas war-off still marks
send/save dirty. Inactive-on requires visible/nonhidden; inactive-off may clear
hidden state. Repeated inactive equality is a no-op.

The immutable existing reputation catalog and retained current state supply all
operands. No new caller-supplied relation, modifier absence, or database facts are
accepted. Missing reputation ownership remains unsupported. Each sequenced owner
command is staged with the normal atomic tick, then checkpointed before its
outbox is released. The bounded source-call history replays flags and dirty bits;
this proves source reachability, not independent authentication of the person who
requested a toggle. Existing owner/sequence checks and TickRecord replay remain
the input provenance boundary.

There is **no immediate source acknowledgment or faction-state packet** for these
controls. The client mirrors its toggle; no synthetic delta/initial packet is
invented. Existing reconnect initialization displays committed flags and its
acknowledgment clears dirty-send state at the next natural tick. A later ordinary
source reputation operation retains its existing send behavior.

## Effects and limits

Current source relation queries consume the retained reputation state, so at-war
changes player-to-NPC relation/eligibility. Inactive only changes presentation
flags, not hostility. Existing historical mainhand proof still excludes tracked
factions; this change does not enable those attacks or weaken that proof. Faction cheats, PvP control, and new source content are not added. The separate
player watched-faction descriptor is described in [WATCHED-FACTION.md](WATCHED-FACTION.md).
This integration introduced native format **57** and native reputation policy **v3**;
current native format **81** also owns the separate watched field and action-bar display byte and rejects earlier snapshots;
there is no migration and the production capability remains uninhabited.

## Adjacent work not claimed

This reputation integration did not supply health regeneration or infer a rate
from `world_mana_rate`. The later optional host-selected standing-mage component
is described in [PLAYER-HEALTH-REGENERATION.md](../PLAYER-HEALTH-REGENERATION.md).
It is not an authenticated external server configuration capture and does not
cross the held private compiler/consumer or production native-entry boundary.
Stand/sit remains separate: spell session evidence and historical restore currently
assume standing, and source animation/aura interruption plus automatic stand-up
must be composed before accepting those commands.

Reviewed source SHA-256:

- CharacterHandler.cpp: `a6fec8ebdd297d5c789a07428be896b78a4597ea4b8410465da358c60073aee1`
- ReputationMgr.cpp: `87094877ab19cfa6e3b297b6d68afeedf5e9f312efd84e09be0293879796f63b`
