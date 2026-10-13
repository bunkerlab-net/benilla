# Native starter compiler provenance

All compiler/parser/tests here are distributed under GPL-3.0-or-later. No
Blizzard client data, database rows, artwork or other game content is included.
All test inputs are newly authored synthetic values and names, never asserted to
be an authentic race/class, spell, creature or quest setup.

Reviewed CMaNGOS Classic revision:
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`.

- [DBCStructure.h](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Server/DBCStructure.h):
  named table/field offsets and scalar signedness, Spell's 173 client columns,
  all three effects, cast times, ranges, factions and skills. CMaNGOS's extra
  server-side fields after client column 172 are not misread as local DBC columns
- [Player.cpp, Player::Create](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Player.cpp#L855-L919):
  packed race/class/gender comparison, skip nonpositive outfit slots, starting
  item quantity branch, then SQL custom item grants. Only the quantity branch is
  translated into executable Python. Upstream comparison/ordering informs the
  fail-closed resolver. Inventory placement is not ported
  `outfit_cases.json` contains all 378 original boundary tuples and C++ results;
  its source hash is SHA-256 of UTF-8 source with LF-normalized line endings so
  native platform checkout line endings cannot silently skip numeric checks.
  `check-native-outfit-oracle.py` regenerates results in memory and compares them
  with the fixture, failing if the C++ compiler is unavailable
- [Player.cpp, learning paths](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Player.cpp#L3397-L3448):
  passive-like auto-casting and skill-step handling require learning-sensitive
  initial spells to remain entry-critical. Classification is conservative;
  it does not execute learning or assert a supported active spell
- [CreatureEventAIMgr.cpp, selector loading](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/AI/EventAI/CreatureEventAIMgr.cpp#L203-L253)
  and [CreatureEventAI.cpp, InitAI](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/AI/EventAI/CreatureEventAI.cpp#L102-L163):
  positive template selectors and negative spawn selectors are additive. The
  narrow SQL extension records all matching event identities and rejects every
  match, avoiding loader-validation/phase/chance/debug-state assumptions. No
  EventAI action, loader filter or current-entry substitution is executed
- Existing original-client observations in
  [characters/customization.rs](../../crates/benilla-formats/src/characters/customization.rs)
  establish the on-disk 2-byte CharBaseInfo records and 41-field/152-byte
  CharStartOutfit layout. The latter has one ID word, four packed bytes and three
  12-word arrays; the header field count does not imply 164-byte records
- The existing private [capture contract](../../docs/CLASSIC-STARTER-CAPTURE.md)
  and `classic_export` validator establish source v2 type/key/hash/selection
  semantics. Preparation calls the validator; it does not weaken its gates

CMaNGOS copyright belongs to CMaNGOS Project contributors; upstream permits GPL
version 2 or, at your option, any later version. The modified quantity translation
and standalone C++ oracle are distributed under GPL version 3 or later, WITHOUT
ANY WARRANTY, including MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
The repository's [GPL license](../../LICENSE-GPL-3.0) and simulation's
[upstream authors](../../crates/benilla-sim/licenses/AUTHORS.md) apply.

## Deviations and limits

- The compiler refuses missing/ambiguous outfit records and missing item
  templates rather than silently skipping them. It also rejects zero/oversize
  BuyCount values in its bounded quantity domain; it does not emulate invalid
  source-row behavior
- It preserves SQL decimal positions and binary32 metadata bits, without
  quantizing a start or casting range into the original runtime model
- Exact decimal-to-binary32 comparison uses integer rational ties-to-even logic,
  not Python's intermediate binary64 conversion. A SQL-normalized zero cannot
  retain its original sign; comparison permits either sign of DBC zero
- It retains whole server spell rows plus DBC-only metadata, but blocks mismatches
  and server overlays/learning closure. It does not assume that a client DBC can
  fill uncaptured server overrides authoritatively
- No runtime native gameplay gate was changed, and no real local SQL/DBC/native
  gameplay or graphical run was performed
