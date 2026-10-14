# Smallest honest native starter slice

The next target is one explicitly selected supported race/class, its actual
level-one start, one nearby non-scripted quest giver, one eligible kill quest,
one hostile creature template and one actual direct ability. A heal can only be
included if that selected class really knows it at the selected level. Do not
invent a class that begins with both a convenient nuke and heal.

Pinned upstream revision:
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, CMaNGOS Classic. The companion
`docs/CMANGOS-PARITY.md` and exporter column whitelist are the inspected source
inventory. No native rows/DBC files are bundled here. All following native ports
remain required; the original fixture is not their implementation.

## 1. Compile native selected content without losing meaning

Input: the selected-starter read-only export `benilla-classic-source-slice-v2`, its canonical source
hash, validated user-local build-5875 DBC records/manifest, explicit selected
race/class and a supported-capabilities version. Output: either a typed supported
native world/rules object plus an exhaustive field-disposition report, or an
unsupported-mechanics report with stable blocker IDs. Never emit runnable fixed
fixture rows as an approximation.

Keep these shared blocker identifiers from the importer:

- `creature.native_stats_combat_faction_required`
- `quest.native_eligibility_credit_reward_required`
- `quest.dynamic_xp`
- `spell.native_effect_target_scaling_rules_required`

The selected-region exporter already retains 99 native creature-template fields,
12 spawn fields, model metadata, class/level creature stats, full quest rows and
all three native spell effect slots. Selection v2 additionally captures the eight
selected player creation/progression tables with exact column domains and bounded
item/spell dependencies; see [the starter capture contract](../../docs/CLASSIC-STARTER-CAPTURE.md).
Local faction/DBC/outfit and full spell/skill learning dependencies remain blocked,
and no native-to-runtime compiler is available. These are required next inputs,
not permission to default them.

## 2. Starter character and creature stats

Port from [ObjectMgr.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Globals/ObjectMgr.cpp):
`LoadPlayerInfo`, `GetPlayerClassLevelInfo`, `GetPlayerLevelInfo`, `GetXPForLevel`,
`LoadCreatureClassLvlStats`, `LoadCreatureTemplates`.

Port from [Player.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Player.cpp):
`Create`, `InitStatsForLevel`, `GiveXP`, `GiveLevel`; and
[StatSystem.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/StatSystem.cpp):
`UpdateAllStats`, `UpdateArmor`, `CalculateMinMaxDamage` for the bounded domain.

Required local tables: `playercreateinfo`, its item/spell/action/skills tables,
`player_classlevelstats`, `player_levelstats`, `player_xp_for_level`, and the
selected rows' full referenced item/spell/skill metadata. The interface needs
race/class/level, resolved base attributes, class powers, base health/mana, actual
starting spell/action/skill/item sets and XP thresholds. Profile appearance alone
cannot supply these fields. Unsupported items/passives must block that selection.

Port [Creature.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Creature.cpp)
`LoadFromDB`, `UpdateEntry`, `SelectLevel`: resolve class/level statistics,
health/power/damage/armor multipliers and legacy overrides exactly, with an explicit
RNG/rounding contract where source values vary. Native class-level inputs include
BaseHealthExp0, BaseDamageExp0, attack power, BaseArmor and all five attributes.
The compiler must retain resolved metadata such as faction, rank, flags, models,
reach and movement/AI conditions; it cannot silently drop these into zero fields.

## 3. Combat, target eligibility and one real ability

Port [Unit.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp)
`CalcArmorReducedDamage`, `CalculateMeleeDamage`, `RollMeleeOutcomeAgainst`,
`DealMeleeDamage`, plus the actual faction/hostility/target checks reached by that
selected encounter. Require local Faction/FactionTemplate DBC records. The original runtime now calls the separately proven integer armor helper for
physical player melee, retaining its explicit rational-vs-f32 boundary deviation.
This single mitigation function does not implement native stats, rolls, immunity,
faction or player armor and cannot serve as a native-combat fallback. Implement and
persist explicitly seeded RNG before any native hit/damage/loot chance behavior;
never use global/random host state.

Port [Spell.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/Spell.cpp)
`Prepare`, `CheckCast`, `FillTargetMap`, `cast`, `HandleEffect` for only the actual
selected spell's supported attribute/effect/target path. Port
[SpellEffects.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/SpellEffects.cpp)
`EffectSchoolDMG` or `EffectHeal` as applicable, including their called scaling /
bonus / mitigation routines for the declared domain. Resolve cast time/range/
recovery/power metadata from the user's DBCs. Multi-effect, aura, trigger,
coefficient, immunity, target-mask or condition requirements must be supported
explicitly or block the spell. Presence of a spell ID is not enough.

## 4. Real kill quest and rewards

Port [QuestDef.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Quests/QuestDef.cpp)
`Quest::XPValue`, and the Player methods `CanTakeQuest`, `AddQuest`,
`KilledMonsterCredit`, `CanRewardQuest`, `RewardQuest`. Preserve selected quest
race/class/level/condition gates, prerequisite chains, signed objective kinds,
kill-credit aliases, item/spell requirements and reward semantics. Select a quest
whose actual path fits the first implemented domain; do not alter its objective
count or rewards merely to fit the fixture.

The bounded native integration now uses the exact source-derived Gain/GiveXP and
quest/player progression reducers with real transactional training/stat hosts.
See [the reconstruction contract](../benilla-world-runtime/NATIVE-QUEST-PROGRESSION.md)
for actual admission, presentation and durability boundaries. The original fixture
constant reward and approximate XP helper are not used by that native path.
Actual private rows, inventory/loot hosts and graphical validation remain required.

## 5. Interface and evidence before lifting gates

Add a versioned native mechanics profile, not a boolean `trust_classic` override.
Pure port functions should consume explicit typed stat/level/race/class/effect
inputs and return values or Unsupported reasons without reading files/network/
clocks. Keep upstream GPL attribution and per-function provenance in the port.
The separate starter-mechanics worker owns those next formula modules; runtime
`Rules::validate` must retain its native-source rejection until the compiler and
all selected capability gates can call those proven ports.

Required evidence:

1. Differential formula tests against the pinned implementation, including branch
   boundaries, rounding, critical inputs and deterministic RNG sequences
2. Native source-to-compiled field disposition tests, proving every relevant row
   field is used or explicitly rejected; missing tables/DBC references fail closed
3. One actual local selected content export and exact-ID client binding, with no
   bundled assets/database rows; record hashes and supported capabilities
4. Headless create -> accept -> combat -> credit -> loot -> turn-in -> save/reload
   using those native rows, plus invalid-target/range/duplicate/replay cases
5. Graphical tests of the real selected-character UI, NPC targeting, spellbook,
   quest panels/log, inventory/loot, death/respawn and durable reload. Headless
   adapter tests do not replace this step

Only then remove the native-source gate for that named capability profile. Other
classes, spells, quests, scripts, skills and regions remain blocked until their
own implemented support is established.
