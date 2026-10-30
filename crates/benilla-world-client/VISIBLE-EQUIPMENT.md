# Source-owned visible equipment entries

The native projection now supplies the public entry words used by the existing
player equipment renderer, from the already authenticated immutable equipped
owner. This is not equipment movement, a new loadout, or native gameplay admission.

## Source and actual consumer path

Pinned CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:
`src/game/Entities/Player.cpp::VisualizeItem` / `SetVisibleItemSlot` (10173–10227)
and `src/game/Entities/UpdateFields.h` `PLAYER_VISIBLE_ITEM_1_0` define entry word
`260 + 12 * slot`, for the 19 equipment positions. An equipped item publishes its
entry even when broken; an absent item publishes zero. The creator, enchantment,
random-property and suffix words are separate. Pinned CMaNGOS writes two inspected
enchantments; the protocol parser can read seven. This change writes **only entry
words**, not guessed creator/enchantment/property data.

Actual downstream code is already present:

- `benilla-protocol` `ObjectFields::player_visible_item_entry` parses these words.
- `benilla-app/src/entities/equipment/resolve.rs` reads entries for body clothing,
  helm, cloak and held equipment, then calls `templates.held(entry, &net)`.
- That lookup requests an `ItemQuery`; the real native controller routes it to
  `NativeProjection::item_query`, returning the existing bound template/display.
- Dress-up and inspection also consume the public entry fields.

The local tests exercise real descriptors/parser and controller ItemQuery responses.
They do not claim rendered pixel verification or user-tested visual behavior.

## Metadata closure and known empty versus unknown

The complete 19-word block is published only when the existing quest/loot genesis
owns the validated item-query catalog. Projection construction retains the prior
quest-then-item metadata validation order and rejects missing, mismatched or
unavailable display metadata, including each equipped entry. No synthetic display
fallback or partial per-item omission is permitted under this owner.

Strict equipped origin/loadout restore authenticates occupied and absent slots.
Within that closed domain, a zero word means **known empty**. An initialized
combat/equipment-only profile without the query owner retains the prior absence
of this whole block: it is **unknown/unavailable**, not falsely declared empty or
renderer-ready. Parser `None` alone cannot distinguish these cases, so tests also
check exact raw-field presence and explicit zeros.

The same source entries appear in initial and regular player descriptors and after
reopen; durability loss does not erase them. Existing item GUIDs and equipment-slot
links are unchanged. Consolidated descriptor repetition is not an assertion about
upstream dirty-mask packet timing. Checkpoint failures still withhold the batch,
and retry publishes without re-executing input.

The current native schema is format81. There is no save migration,
no private input/compiler work and no wider source/admission domain. Mutable
loadout membership, equip/unequip callbacks and additional appearance fields remain
separate work requiring their own authenticated owners.
