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

## Learning dependency closure

`learning.py` is an original bounded dependency traversal based on the pinned
[DBCStructure.h layouts](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Server/DBCStructure.h#L620-L671),
[Player::LearnDefaultSkills](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Player.cpp#L5813),
and [SpellMgr learning loaders](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/SpellMgr.cpp#L1632-L1768).
The exact `spell_chain` and `spell_learn_spell` column domains are reviewed from
[the pinned base schema](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/sql/base/mangos.sql).
No upstream SQL data rows are included. Synthetic tests use newly authored IDs,
values, long chains, cycles, mismatches and missing-data/budget failures.

Every SkillLineAbility row touching a reachable skill or spell is retained,
including zero learnOnGetSkill and unmatched masks, because resolving native
obtainability and effect-118 profession repair belongs to initialization.
Effects start at Spell words 61–63, not equipped-item mask words 58–60; misc
skill IDs are words 106–108 and trigger spell IDs are 109–111. All three slots
remain raw and source-tagged. The canonical server numeric array overlays only
reviewed SQL fields on 173 client words and preserves their disagreements.
It does not synthesize uncaptured server fields, talent membership, external
hooks, effective learned spells, passive application or final player stats.

The source SQL query's global-empty branch returns before the Effect36-derived
learning scan. The exporter retains a bounded one-row whole-table existence
probe separately from exact selected-parent relationships, so a selected-empty
result never fabricates an initialized derived map. It neither executes the
loader nor interprets an unrelated probe row as a selected dependency. Existing
rank learning in Player::addSpell means passive predecessors stay entry-critical
when a higher active cast is unavailable. These are dependency/branch facts, not
initialization execution or a playable-content certification.


Native spawn dependency selection follows the same pinned
[ObjectMgr::LoadCreatureSpawnDataTemplates and LoadCreatures](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Globals/ObjectMgr.cpp#L1758-L2027)
and [Creature::GetCreatureAddon](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Creature.cpp#L2295-L2353).
The [bounded evidence contract](../classic_export/NATIVE-SPAWN-CONTRACT.md)
documents exact selectors, schema domains, missing/empty distinctions and
unexecuted semantics. Schema metadata only is transcribed; all test rows are
original synthetic inputs.


## Spell registry and creature display bindings

- [ScriptDev2 registry schema](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/sql/scriptdev2/spell.sql#L2-L5),
  [numeric registry loader](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/Scripts/SpellScript.cpp#L22-L106),
  [private numeric setters](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/Scripts/SpellScript.h#L146-L177)
  and [signed conversion](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/shared/Database/Field.h#L59)
  establish the bounded absence contract. No named native script is implemented
  by the producer; empty/unknown names remain raw rows
- [DBCfmt.h](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Server/DBCfmt.h#L34)
  consumes CreatureDisplayInfo words 0, 1, 3 and 4. The existing build-5875 client
  [creature display reader](../../crates/benilla-formats/src/creatures.rs)
  identifies strings 6–8 and signed SizeClass at 9, including the -1 test at
  client address 0x625509. The old portrait-string comment in DBCStructure.h is
  not followed. The complete 12-word record stays bound to its original bytes
- [ObjectMgr model loading](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Globals/ObjectMgr.cpp#L1635-L1695)
  establishes the other-gender/alternative relations. Capture follows the
  selected other-gender closure but preserves alternatives as unresolved
  context, without executing loader corrections or renderer substitutions
