# Source-owned fresh selected-character cosmetics

This optional owner retains five requested appearance bytes for the exact selected
player. It does not create a corpse, enable ghost release, import a character,
or provide native-content admission. The production native capability remains
uninhabited; actual controller integration is exercised through its private test
entry with an actual ProfileStore-selected character.

## Loaded selection, not dial-count inference

Pinned CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`,
`Player.cpp:714–745` (`ValidateAppearance`) and `Server/DBCStores.cpp`
(`GetCharSectionEntry`, `GetCharFacialHairEntry`) require exact available row
combinations: skin type/colour, face/skin colour, hair style/colour, and conditional
facial hair/colour. Tauren and females other than Night Elf/Undead skip the facial
section lookup; every selection still requires its CharacterFacialHairStyles row.
Race/class validity is independently bound to the existing initialized player.

The formats catalog keeps separate full-width raw rows and complete 40/36-byte
coverage only. The GPL simulation selector does not reuse narrowed UI dial counts or infer
missing tuple combinations. Unsupported widths, alias domains, duplicate source
identities or ambiguous selected keys fail closed. These conservative supported
input restrictions are not new upstream character-creation errors. Existing UI
dial construction is unchanged. New source-derived selection semantics reside only in the GPL simulation crate; no
repository or formats-crate license is changed. The returned selection is opaque and
Serialize-only; arbitrary request bytes are not source evidence.

## Identity, persistence and projection

The initializer validates that selection against the retained starter
race/class/gender and nonzero selected owner/character. Reinitializing to different
choices is refused atomically. The derived immutable state binds the world,
player, current profile digest, source selection digest and all five bytes.
Checkpoint creation and restore compare it with an independent reconstruction;
missing required option fields, invented choices or transplanted bindings are
rejected even after recomputing outer hashes. An absent optional source remains
unknown. No checksum is claimed to authenticate an external user request.

The actual controller's selected-character check precedes world container
creation/open: matching owner/character alone cannot substitute different cosmetic
choices. This does not manufacture a native admission capability or connect the
held private compiler path.

Pinned `Player::Create` writes skin, face, hair style and hair colour into the four
bytes of `PLAYER_BYTES` (field 193). The adapter publishes that complete word from
the owner on initial/update/reopen semantic descriptors. Facial hair is retained, but appearance alone still cannot publish
`PLAYER_BYTES_2` (194). Where the validated quest progression owner is also
present, its projection now composes the complete word: facial hair in byte 0,
source fresh reserved byte 1 and bank-slot byte 2 both zero, and the actual quest
rest state in byte 3. `Object::_InitValues` zeroes the fresh descriptor before
`Player::Create` writes facial hair/rest; `Player.h::SetBankBagSlotCount` and
`ItemHandler.cpp::HandleBuyBankSlotOpcode` are the bank-byte producer, and native
bank purchase/import transitions remain unsupported. No raw imported character
word is treated as fresh. This is a closed native-source subset, not general bank
support. The same owned lower prefix is reused for initial/update/reopen fields
and ordered `RestStateByte` callbacks; rest is never hardcoded. Source-owned
composition does not depend on client metadata for those lower bytes. Without
appearance, the existing required client lower 24-bit metadata contract remains;
without the quest/rest owner, field 194 remains omitted. No new mutable state,
format bump or migration is required for this derived projection. Guild remains unknown; hide flags have a separate
[bounded source owner](PLAYER-HIDE-FLAGS.md), not this cosmetic owner. No ghost aura, transformed display, corpse identity/registry,
placement, graveyard, resurrection or rendered-pixel behavior is implied.

Native format 81 requires this versioned optional snapshot field and rejects
earlier formats without migration. Source shape, opaque loaded-row tests,
rehashed-state adversarial tests, actual field parsing and controller
checkpoint-failure/retry/reopen are separate evidence from graphical gameplay.
