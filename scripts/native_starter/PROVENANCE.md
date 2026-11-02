# Native starter compiler provenance

All compiler/parser/tests here are distributed under GPL-3.0-or-later. No
Blizzard client data, database rows, artwork or other game content is included.
All test inputs are newly authored synthetic values and names, never asserted to
be an authentic race/class, spell, creature or quest setup.

Reviewed CMaNGOS Classic revision:
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`.

- [ObjectMgr::LoadPlayerInfo, ObjectMgr.cpp:3225-3330](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Globals/ObjectMgr.cpp#L3225-L3330)
  supplies SQL wildcard masks, DBC obtainability, and tier checks.
- [SpellMgr.cpp:1284-1770](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/SpellMgr.cpp#L1284-L1770)
  supplies Talent and ability-forward rank construction, SQL validation, global
  empty-table branches, and all three Effect36 slots with auto-learning rules.
- [SpellMgr.cpp:2340-2389](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/SpellMgr.cpp#L2340-L2389)
  supplies DBC row-ID insertion order and missing-SkillLine exclusion.
  Python publishes a scoped loader plan. Rust independently reconstructs these
  maps before the existing initializer runs; neither authenticates capture.
- [Player::UpdateSkillTrainedSpells, Player.cpp:5642-5698](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Player.cpp#L5642-L5698)
  supplies wildcard race/class masks, nonzero acquisition-method training,
  minimum skill-value gating, and the zero-method profession-repair branch.
  The pinned core uses this name, not `learnSkillRewardedSpells`.
- [SpellMgr::LoadSpellLearnSkills, SpellMgr.cpp:1625-1656](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/SpellMgr.cpp#L1625-L1656)
  selects the first valid server `SPELL_EFFECT_SKILL`, effect 118, with nonzero
  skill and simple value. Effect 44 does not create a repair-map entry.
  `skill_training_capture` records the effective SQL fields for all three slots,
  exact candidate IDs, missing lookups, and coverage hashes. Client bytes cannot
  prove that a server repair mapping is absent.
- [SkillLineAbilityEntry, DBCStructure.h:653-668](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Server/DBCStructure.h#L653-L668)
  supplies words 3/4 for masks, 7 for the threshold, and 9 for acquisition.
  Discovery reports every visited ability row's training disposition, including
  excluded rows. Independently reached spell and forward-rank metadata retain
  their separate SpellMgr lookup semantics; a zero-method sibling is not a
  learning dependency merely because it shares a skill.

To add the SQL training projection to a private, unchanged operator-quiescent
capture, run the following command. The password prompt does not echo its input.
The command validates the existing source and extraction, derives candidate IDs
to a fixed point, and publishes a new private file without overwriting the source.

```sh
python3 scripts/classic-export.py skill-training \
  --source benilla-config/source.json --dbc-dir benilla-config/dbc \
  --database LOCAL_DATABASE --user SELECT_ONLY_USER --port 33306 \
  --gender 0 --quiescent-snapshot --output benilla-config/source-training.json
```

This operation requires the database to have remained unchanged since the source
capture. Matching schema, version, and selected spell fields do not establish that
historical fact. The extension retains the operator provenance limitation.
Its 4,096 candidate-row bound is separate from the unchanged 512 spell-node bound.
Unknown repair lookups remain explicit blockers and never become an invented
client-derived absence. Unknown skill state remains conservative. The discovery
graph and loader plan do not prove effective learning or gameplay admission.


- [DBCStructure.h](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Server/DBCStructure.h):
  named table/field offsets and scalar signedness, Spell's 173 client columns,
  all three effects, cast times, ranges, factions and skills. CMaNGOS's extra
  server-side fields after client column 172 are not misread as local DBC columns
- [Player.cpp, Player::Create](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Player.cpp#L855-L919):
  packed race/class/gender comparison, skip nonpositive outfit slots, starting
  item quantity branch, then SQL custom item grants. Python reports the bounded
  source-ordered placement operations. Rust `CapturedStarterItems::compile`
  supplies the existing initializer with grants and item rows. The initializer
  uses `StoreNewItemInBestSlots` at lines 952-985, `FindEquipSlot` and
  `ViableEquipSlots` at lines 7858-8042, `CanEquipItem` at lines 9432-9555,
  `CanUseItem` at lines 9779-9847, and the second inventory pass at lines 913-945.
  The admitted subset excludes offhands, ammo, and equipped bags.
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
- [QuestDef.cpp:58](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Quests/QuestDef.cpp#L58)
  loads request-items text through
  [Field::GetCppString, Field.h:49-52](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/shared/Database/Field.h#L49-L52),
  which maps SQL NULL to an empty string.
  [PlayerMenu::SendQuestGiverRequestItems, GossipDef.cpp:510-534](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/GossipDef.cpp#L510-L534)
  applies nonempty locale overrides first, then sends offer-reward instead when
  the resulting text is empty, or when a completable quest requires no items.
  Preparation preserves the captured NULL and accepts it as empty request text,
  without inventing prose or removing other presentation refusals. The client
  distinguishes progress and reward presentation through `QUEST_PROGRESS` and
  `QUEST_COMPLETE`, recorded at addresses 0x51b185 and 0x51b18f in
  [events.tsv](../../reference/events.tsv).
- [GetSpellCastTime, SpellMgr.cpp:137-145](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/SpellMgr.cpp#L137-L145)
  returns zero when the cast-time lookup is absent, before rank and cast modifiers.
  Preparation represents the absent zero selector as zero cast-time inputs,
  without requesting a nonexistent record zero. Missing nonzero selectors still
  require exact rows.
- Selected positive CharStartOutfit slots are explicit item roots in dependency
  discovery. Pass preparation's `dependency_request` to the next SQL capture,
  then prepare again. SQL-only initial capture cannot discover DBC outfit items.
  Recapture resolves their item-template and spell dependencies without copying
  client rows into the source bundle.
- [LootMgr.h, LootStoreItem](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Loot/LootMgr.h#L215-L217)
  splits a negative SQL chance into absolute chance and quest-only identity.
  [LootStoreItem::Roll](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Loot/LootMgr.cpp#L374-L389)
  applies the distinct quest-drop rate.
  [LootItem::AllowedForPlayer](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Loot/LootMgr.cpp#L463-L483)
  uses [Player::HasQuestForItem](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Player.cpp#L13777-L13807).
  Preparation reports only ungrouped, unconditional body leaves with valid positive
  counts as `quest_loot_inputs`. It retains whole-loader order and full item-admission
  blockers, and requires explicit owner quest state and quest-drop policy at runtime.

- [ReputationMgr.cpp, initialization and base queries](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Reputation/ReputationMgr.cpp#L74-L84)
  and `:140-150,197-229,243-263` define base/flag slot selection, zero stored
  deltas, dirty bits, and the 64-entry initial packet.
  [FactionEntry](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Server/DBCStructure.h#L359-L386)
  defines all four mask/base/flag slots. Preparation reports
  `player.initial_reputation` as supported only with the complete Faction table,
  selected template references, and required quest/item faction lookups.
  Missing rows, unknown flags, invalid indices, duplicate indices, and conflicting
  substantive matches retain precise reasons and the initialization blocker.
  Trailing zero slots remain wildcard fallbacks. Every numeric Faction column
  is bound. The Rust adapter documents the same bounded input contract in
  [REPUTATION.md](../../crates/benilla-sim/REPUTATION.md#dbc-initialization-adapter).
  [Player::CalculateReputationGain](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Player.cpp#L6304-L6314)
  consumes aura156 only for awards, not initialization. Human Diplomacy 20599
  therefore does not multiply initial base standing. SQL reward policies and
  effective racial/passive closure remain separate requirements.

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
