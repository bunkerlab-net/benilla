# Bounded native helm/cloak visibility

Pinned CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`
`CharacterHandler.cpp:1137–1147` handles the two bodiless requests by toggling
`PLAYER_FLAGS_HIDE_HELM` (`0x400`) or `PLAYER_FLAGS_HIDE_CLOAK` (`0x800`).
There is no alive, combat, equipment-presence or change-to-requested-value guard.
Every received request is a flip; two identical requests restore the original bit.
This source-derived implementation remains in the existing GPL-covered runtime.

## Existing owner closure, not general player flags

A single eligibility check is used by the live commands, checkpoint validation,
restore and semantic descriptor projection. It requires the exact selected fresh
appearance owner, initialized combat owner, full raw spell/interaction owner and
quest progression owner. These are previously validated source owners, not new
caller-provided absence booleans:

- The interaction source validates initial `PLAYER_FLAGS == 0` and excludes
  external interaction mutations, PvP/duel/sanctuary, controlled units and
  interaction-changing auras. The seed stays initial zero; current hide bits do
  not rewrite it or synthesize a ghost bit.
- The mandatory quest kill catalog covers the profile's NPCs and excludes group,
  civilian/racial-leader and unsupported kill dependencies. Its existing clock
  contract excludes rest-area entry; durable progression retains fresh rest state.
- The raw spell initializer and exhaustive admitted runtime effects do not add
  ghost or other player-flag auras. Release remains unavailable. Admitted commands
  have no AFK/DND/group/taxi/PvP or GM-mode transitions; the separate admin owner
  grants levels, XP policy or items, not GM status.

This is the **existing no-rest-entry supported-host contract**, not proof that
an arbitrary loaded map has no capital or inn. Upstream `UpdateZone`/`SetRestType`
can set RESTING and FFA flags, and login/world policies can set other flags.
Those domains remain outside this composition. Extending any producer requires
extending the owner and its projection; fresh appearance alone is insufficient.
Profiles without the complete owners omit field 190 and reject toggle commands.
Their default empty ledger stays valid; a transplanted nonempty ledger does not.

## Persistence and real client path

The required ledger retains each toggle's native sequence and kind, replays XOR
from source initial zero, and validates the final flags plus strictly increasing
sequences below `next_sequence`. Sequence 0 is valid. The ledger proves reachable
source operations under its owners, not an authenticated historical trajectory;
normal owner-checked input/TickRecord replay establishes exact command execution.
The 2048-toggle limit is an explicit bounded-host resource limit, **not** a CMaNGOS
rule. Exhaustion refuses before mutation, and decode enforces the same bound.

The adapter emits the complete owned word as field 190 on initial/update/reopen
semantic descriptors. It does not claim byte-identical upstream dirty-mask or
packet timing. Stock Lua `ShowHelm`/`ShowCloak` already decides when a flip is
needed; the existing app sender maps those queued flips to stock commands. The
native controller now forwards those exact commands. The actual `ObjectFields`
parser and equipment renderer already consume these hide bits. Tests exercise
Lua, the extracted actual sender, controller, durable save failure/retry/reopen,
and parser predicates—not a full Bevy run or rendered pixels.

Appearance bytes, item membership, visible entry words and guild state are not
changed. No guild-zero owner, corpse registry, ghost aura, release, migration or
production native admission is introduced. Native 70 requires the new ledger and
rejects previous 69 identities; `NativeCapability` remains uninhabited.
