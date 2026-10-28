# Watched reputation player field

The existing stock `SetWatchedFactionIndex` send now reaches the native player
owner and projects `PLAYER_FIELD_WATCHED_FACTION_INDEX` (descriptor index **1261**).
The existing protocol field parser and reputation UI consume it. This is tested
component/controller integration, not production admission or user-verified
rendered gameplay.

## Pinned source

CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

- `Entities/CharacterHandler.cpp`, `HandleSetWatchedFactionOpcode`: read signed
  `int32`, directly `SetInt32Value`, with no catalog, visibility, range, life or
  combat guard.
- `Entities/Player.cpp`: `Create` initializes **-1**, load retains signed value,
  save retains its raw unsigned bits.
- `Entities/Object.cpp`, `SetInt32Value`: unchanged values do not dirty the field.
- `Entities/UpdateFields.h`: `0x431 + UNIT_END(188) = 1261`.

This is a player descriptor, **not ReputationMgr faction state**. No reputation
catalog is required and no faction dirty flags, standings, or quest callbacks
are changed. All signed values are preserved, including -2, signed extremes,
slot zero, and unknown/out-of-range positive indexes. The UI may display no
matching watched row; that is not a reason to normalize the retained value.

## State, replay and delivery

The owner starts at -1 and retains changed writes with their actual sequenced
input identity. Each input in a same-tick batch consumes its own sequence. Restore
requires a bounded, strictly increasing sequence history below `next_sequence`,
nonredundant transitions, and exact replay to the saved field. Required fields
cannot be omitted and rechecksumming a divergent field or receipt does not repair
it. These receipts prove source reachability and internal consistency, not
cryptographic authentication of a rewritten whole ledger; authenticated inputs
and TickRecord replay remain the outer provenance boundary.

There is an explicit host bound of **2048 changed writes**. Exceeding it fails the
whole transaction without publishing a prefix. This is a resource limit, not an
invented upstream rejection. Same-value requests remain successful at the bound
and do not append a receipt. The input itself still consumes a sequence.

The controller uses the normal atomic tick and checkpoint-before-outbox release.
The stock UI does not mirror a watch locally: it waits for the descriptor. Initial
and reopened descriptors include the saved raw value, and command results flow
through the existing player field projection. That projection consolidates player
fields each frame and can repeat an unchanged value; this is semantic descriptor
delivery, **not byte-identical upstream dirty-mask/packet timing**. No separate
watch acknowledgment, reputation delta, or initial-faction packet is invented.

This field was introduced in native format **58**; current format **59** also owns
the separate action-bar display byte and rejects earlier context; there is no migration and no
production native capability is added. Existing ghost, native-admission, private
compiler/consumer, and tracked-mainhand holds remain unchanged.

## Coverage

Authored tests cover raw signed boundaries, no-catalog worlds, batch sequence gaps,
no-op writes, whole-batch rollback, replay/reopen, forged histories and saved
values, capacity and bounded decoding. The world-client test reads the real
`ObjectFields::player_watched_faction` parser from projected initial/update/reopen
fields. The actual Lua/controller path checks deferred UI visibility through
checkpoint failure/retry, clear-watch, and signed-value persistence.
