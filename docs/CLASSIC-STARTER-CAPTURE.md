# Selected Classic player-source capture

This is an input boundary for native mechanics, not a player-creation engine or
native-to-runtime compiler. A starter capture is private, `capture_complete=false`
and `runnable=false`. Only original fake-source tests have run: no real SQL
connection, installed schema, DBCs or gameplay assets have been tested.

## Selection and compatibility

The v1 region selection and `benilla-classic-source-slice-v1` remain accepted
unchanged. To request player inputs, use selection `format_version: 2` and the
additional required object `starter: {"race": 1, "class": 1, "level_cap": 3}`.
This is a selector example, not a populated character or a canonical start area.
See [the complete selection example](../scripts/classic_export/selection.starter.example.json).
One race (1–8), one playable Classic class ID (1–5, 7–9, 11), and a cap (1–60)
are allowed. A selected `playercreateinfo` row must actually exist. This does not
substitute for client DBC validation of that race/class combination.

Starter captures/plans use `benilla-classic-source-slice-v2` and
`benilla-classic-capture-plan-v2`. A source v2 bundle additionally records
`starter_required_keys`, the sorted native composite identities for each of the
eight player tables. The existing schema and per-table SHA-256 hashes cover all
captured columns, including nullable notes and original decimal coordinates.

There is no lossless offline conversion from v1 to a starter capture: v1 does
not contain these rows. Keep an existing v1 artifact, reinspect the local schema,
and recapture to a new destination with a v2 selection. Renaming the format or
adding an empty manifest fails validation; no migration fabricates missing data.
V1 does not require, query or accept captured player rows.

## Exact projected schema

Columns below are from the pinned [base schema][schema]; loader references are
from [`ObjectMgr::LoadPlayerInfo`][objects]. Every listed column is preserved.
For these eight tables and the two learning tables described below, capture
requires the exact column set, order,
SQL value domain and nullability. Integer display widths are ignored, but integer
size, signedness and nullable semantics are not. Engine restrictions and the
operator-confirmed MyISAM snapshot requirement remain unchanged. Installed key
constraints are not inspected: compiled identity rules plus duplicate-row checks
are an additional capture restriction, not proof of installed index equivalence.

- `playercreateinfo`: race/class unsigned tinyint; map unsigned smallint; zone
  unsigned mediumint; position_x/y/z and orientation float; all NOT NULL.
  Identity is (race, class), exactly one row
- `playercreateinfo_item`: race/class unsigned tinyint, itemid unsigned mediumint,
  amount unsigned tinyint; all NOT NULL. Upstream has a non-unique race/class
  index only. Capture rejects repeated (race, class, itemid), including differing
  amounts, rather than merging rows or relying on unspecified SQL ordering
- `playercreateinfo_spell`: race/class unsigned tinyint, Spell unsigned mediumint,
  all NOT NULL; Note nullable varchar(255). Identity is (race, class, Spell)
- `playercreateinfo_action`: race/class unsigned tinyint, button/type unsigned
  smallint, action unsigned int; all NOT NULL. Identity is (race, class, button).
  Button zero is valid; values must fit the actual core's 0–119 button range,
  24-bit action and 8-bit type without narrowing/wrapping
- `playercreateinfo_skills`: raceMask/classMask unsigned int, skill/step unsigned
  smallint, all NOT NULL; note nullable varchar(255). Identity is
  (raceMask, classMask, skill). Zero masks and step zero are preserved. Step must
  be 0–16, matching the pinned [SkillTiers layout][dbc]
- `player_classlevelstats`: class/level unsigned tinyint, basehp/basemana unsigned
  smallint; all NOT NULL. Identity is (class, level)
- `player_levelstats`: race/class/level/str/agi/sta/inte/spi unsigned tinyint,
  all NOT NULL. Identity is (race, class, level)
- `player_xp_for_level`: lvl/xp_for_next_level unsigned int, all NOT NULL.
  Identity is lvl

The query predicates select the exact race/class pair. Skills select rows for
which each mask is zero or contains the selected race/class bit, matching the
SQL loader's first-stage selection. Skill obtainability is a separate DBC check;
these are candidate native skill rows, not an asserted effective skill list.

Class and race/class stats require every level 1 through the selected cap;
XP requires every level 1 through cap−1. Cap one therefore has an empty XP set.
A cap is this capture's requested range, not an inferred server configuration.
Extra levels or other race/class rows cannot be smuggled into a validated bundle.
Missing rows, zero base health, zero strength or zero XP thresholds fail rather
than applying upstream gap-fill behavior. Genuine zero base mana is preserved.

Per-table maxima are 1 creation row; 2,048 custom item rows; 512 initial spell
rows; 120 action rows; 512 candidate skill rows; 60 rows in each stat table; and
59 XP rows. All use parameterized fixed SELECTs, canonical ordering and
`LIMIT cap+1` with overflow rejection. Existing aggregate byte/reference limits
still apply; no free-form query or player/account/character database is read.

## Reference closure and remaining blockers

### Additive server learning/rank evidence

New v2 captures require two additional exact-schema tables from the pinned base
schema. `spell_chain` retains nonnull signed mediumint spell_id, prev_spell,
first_spell, req_spell and signed tinyint rank, in native column order
(spell_id, prev_spell, first_spell, rank, req_spell). `spell_learn_spell` retains
nonnull unsigned smallint entry/SpellID and unsigned tinyint Active. The capture
requires Active 0 or 1 and nonnegative rank references without narrowing. Keys
are spell_id and (entry, SpellID), respectively; duplicate keys reject.

For every captured spell, fixed parameterized SELECTs retain all matching rows
without native-loader filtering. A visited-set fixed point follows all nonzero
prev/first/required spell references, SpellID learning links and all three raw
EffectTriggerSpell fields. Missing referenced spell_template rows or the
existing 512-spell/4,096-relation-row/aggregate-byte limits fail the whole capture.
Cycles terminate discovery; runtime semantic cycle rejection remains separate.

`learning_capture` binds the exact captured spell selector set, tables, cap,
unfiltered predicate and optional bounded native dependency request. Schema and
table hashes cover all retained values. Offline validation recomputes selector
coverage, rejects out-of-scope rows and enforces every referenced template.
Missing learning tables/coverage in older v2 artifacts remain accepted as
legacy unknown input; partial tables or missing/altered coverage with present
tables reject. An empty captured relationship array is never inferred from an
absent table. This is not a claim that the upstream loader accepted those rows.

A separate fixed, ordered `LIMIT 1` probe over the whole spell_learn_spell table
retains zero or one typed row in `global_learn_spell_probe`; it does not fetch
another selected spell's dependency graph. This distinction is required because
pinned `SpellMgr::LoadSpellLearnSpells` returns before scanning Effect36 when the
entire SQL table is empty. Selected-empty relationships alone cannot establish
that loader branch. Validation rejects more than one probe row, an empty probe
with captured learning rows, and disagreement when the probe's parent is in the
selected set. Probe bytes count toward the same snapshot/aggregate budget.

An optional `--dependency-request` contains only bounded sorted unique item/spell
IDs and their discovery source/DBC hashes. It must match the exact starter
selection and pinned core revision, and cannot contain query text. The capture
retains the request and adds its roots before ordinary closure. Hashes describe
lineage rather than SQL snapshot identity or authenticity. Reprepare against
the resulting new snapshot to discover any additional edges. See the
[native preparation flow](../scripts/native_starter/README.md#local-dependency-recapture).
Talent membership, proc/affect/condition/script hooks and DBC-derived native
loader behavior are not established by these two tables.

### Additive EventAI selector evidence

New v2 captures require the full reviewed `creature_ai_scripts` schema,
including every event/action parameter and comment. The pinned ClassicDB dump
uses signed INT `id`; the pinned core base's unsigned INT `id` is a separate
reviewed variant. All other widths, signedness, order, nullability and primary
keys are exact. Legacy bundles without captured EventAI rows remain unproven;
old ID-only projections require recapture rather than invented full rows.
The fixed parameterized query selects every matching row for both all captured
positive creature-template entries and all negative captured spawn GUIDs, in
`id` order with the existing row cap plus one. It runs in the same snapshot;
MyISAM still requires the explicit quiescent-snapshot contract. Schema and table
hashes cover the projection, and `event_ai_capture` records the exact selector
set, limit and unfiltered selection contract. Validation recomputes coverage
and rejects missing/altered coverage or out-of-scope rows. INT32_MIN and extreme
GUID/entry selectors fail instead of invoking signed-negation/coercion behavior.

CMaNGOS `CreatureEventAI::InitAI` appends both template-entry and spawn-GUID event
lists. A spawn-specific list does not override the template list. Every matching
row blocks the no-events capability, including alleged zero-chance, inactive or
debug-only events; no event fields are used to manufacture absence. Empty rows
prove only this captured query's result, not database authenticity or behavior
after random/pool/event entry substitution. Other AI hooks, groups/linking,
addons, assistance, movement and server settings remain blocked. Full raw rows
now derive positive ACTION_T_TEXT references with event/action/parameter
provenance and bounded base/locale reads in the same snapshot. Missing base rows
and unknown tables never become complete-empty evidence. See the
[EventAI text dependency contract](../scripts/classic_export/EVENT-TEXT.md) for
schema variants, exact aliases, readback validation and incomplete states.
No EventAI action executes here, and no production capability is issued.

Custom creation items and item-type actions (128) require item_template rows.
Initial spells and spell-type actions (0) require spell_template rows. For v2,
item requiredspell and all five item spell slots are followed, then the existing
bounded recursive EffectTriggerSpell and server learning/rank closure. Missing referenced rows fail.
Action existence does not establish that the player learned an active spell;
an action absent from the initial SQL spell set receives a learning-resolution
blocker. Other action types are preserved and blocked, never interpreted as
spell IDs, item IDs or commands.

These inputs still cannot create a native character:

- `player.chr_races_classes_map_dbc_not_captured`: ChrRaces gives models/race
  identity; ChrClasses gives power type; map/start validation needs Map DBC
- `player.faction_reputation_dbc_not_captured`: Player::Create's faction and
  starting reputation require ChrRaces/Faction/FactionTemplate information
- `player.char_start_outfit_dbc_items_not_captured`: [Player::Create][player]
  selects CharStartOutfit by race/class/gender, equips those items and then adds
  the SQL custom items. SQL item rows alone are never a complete outfit. Gender
  and those DBC-derived item references remain future compiler inputs
- `player.skill_line_race_class_tiers_ability_dbc_not_captured`: obtainability,
  range, maxima and linked skill/spell behavior depend on SkillLine,
  SkillRaceClassInfo, SkillTiers and SkillLineAbility plus core rules
- `player.spell_learning_passive_dependencies_not_captured`: learning can add
  passives, auras and other skill/spell dependencies beyond trigger references
- `player.equipment_aura_stat_modifiers_required`: SQL attributes/base health/
  mana are preserved, but equipment and passive/aura modifiers are unresolved
- `player.start_configuration_not_captured`: start level/money, configured cap,
  rates and other server settings are not obtained from these SQL rows
- `player.native_creation_progression_rules_required`: no creation/progression
  lifecycle is executed by this capture or made runnable by a validated hash

The report also identifies starts outside the selected region. Required skill/
faction fields and startquest references on captured items receive separate
missing-dependency blockers. Existing world/quest/spell/AI/loot blockers remain.
A missing optional table is an error; an inspected optional table with no rows
is preserved as empty while the DBC/outfit/learning blockers remain.

For downstream typed stat kernels, retain basehp as create health, basemana as
create mana, and all five attributes (including stamina/intellect). Do not assume
zero equipment, aura or other modifiers. Formula-level support and these SQL
inputs alone do not establish authoritative runtime stats, spell execution,
quest eligibility or a fully compiled native starter.

Hashes detect accidental changes; they do not prove that an operator supplied a
complete authentic database or valid game content. Offline validation recomputes
row identity, typed values, exact selected level coverage, referenced item/spell
existence, composite-key manifests and required blockers. It cannot infer deleted
optional source rows from a maliciously reauthored bundle and its new hashes.

[schema]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/sql/base/mangos.sql
[objects]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Globals/ObjectMgr.cpp#L3071-L3717
[player]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Player.cpp#L747-L950
[dbc]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Server/DBCStructure.h
