# In-game Admin menu

The Escape menu includes an **Admin** button using the existing Vanilla-style
FrameXML templates. The panel covers item/bag search, gold, mounts, movement
speed, city teleportation, one-level increments, XP gain, faction reputation,
Classic honor contribution, time of day and current-location weather.

**This increment provides the menu and its typed backend interface. No live
admin backend is attached yet, so operations are disabled with an explanation.**
Opening the panel does not grant privileges or mutate character/world state.
The group passphrase does not confer admin permission. There is no realm chat
command fallback.

## Backend contract

`benilla-ui::script::admin::AdminBackend` supplies a world/session-bound snapshot,
accepts typed requests and returns results. Catalogs and location-valid choices
come from that backend, not hard-coded example item IDs or city coordinates.
The backend must independently verify the authenticated actor, explicit world
admin policy, current character, catalog revision and location. UI checks are
convenience checks, not authorization.

Requests remain pending until a matching authoritative result arrives. A success
result must follow durable, idempotent world-state publication. Switching world
or session invalidates old result presentation. UI numeric ceilings are bounded
per-request limits, not claims about Classic ruleset maxima.

The requested item scope is the complete Classic catalog, including bags. The
current bounded native inventory reducer excludes several complex template
classes, so catalog completeness and useful behavior of every item remain work
in progress. An item being searchable will not establish that its spells or set
bonuses are implemented.

Classic honor contribution is distinct from honorable kills, PvP rank and modern
spendable honor currency. Future time/weather commands must use a shared world
policy and current-location validation; changing daylight must not rewind
simulation timers. Automatic time/weather reset is represented explicitly.

## Verification

Engine-only tests cover typed requests, malformed catalogs, stale contexts,
pending/duplicate handling, receipts and menu lifecycle. These tests do not prove
asset-backed rendering, actual grants or a playable populated world. Native
gameplay admission and the durable admin backend remain separate prerequisites.
