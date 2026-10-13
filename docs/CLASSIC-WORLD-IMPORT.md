# CMaNGOS Classic local-data integration boundary

## Decision and legal boundary

Use the player's existing, separately obtained local client data and local world
database. Distribute original code, notices, and synthetic tests, not a world
content pack. This architecture reduces accidental redistribution; it does not
establish that every local use or emulation use is permitted.

The [CMaNGOS core license](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/README.md)
is GPL-2.0-or-later, with separate third-party notices. The
[Classic-DB README](https://github.com/cmangos/classic-db/blob/ec4f596146be6467ea93c57397858e329e2db852/README.md)
identifies GPL v3 **and** copyrighted material outside GPL. Its
[copyright notice](https://github.com/cmangos/classic-db/blob/ec4f596146be6467ea93c57397858e329e2db852/COPYRIGHT.md)
asserts an intended fair-use purpose and notes national-law differences. That is
not a blanket grant from Blizzard. Blizzard's
[official EULA](https://www.blizzard.com/en-us/legal/simple/bfbbb648-bcc6-4b78-a5c7-e2fe20c135df/blizzard-end-user-license-agreement)
contains restrictions involving unauthorized connections, emulation/matchmaking
and data mining. The applicable agreement and jurisdiction need qualified legal
review before promising legal safety or publishing a content distribution.

Never commit MPQs, extracted DBC/maps/models, client interface files, local SQL
rows, generated world exports, character/account data, or credentials. Keep local
exports below the existing gitignored `benilla-config/` state root. Do not send
content exports to peers automatically. Peers provide their own local content;
only compatibility hashes and authorized mutable game state cross the wire.

## Pinned evidence

The reviewed sources are:

- Core `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`
- Classic-DB `ec4f596146be6467ea93c57397858e329e2db852`
- Client profile 1.12.1 build 5875; Classic-DB also describes 6005/6141 support,
  but this boundary intentionally does not assume those builds are equivalent

[Core base schema](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/sql/base/mangos.sql),
[object loading and validation](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Globals/ObjectMgr.cpp),
[SQL stores](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Server/SQLStorages.cpp),
and [DBC stores](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Server/DBCStores.cpp)
are the implementation references, rather than an unversioned MaNGOS wiki schema.
The base schema is not by itself a complete/current populated world database.
[Classic-DB's installer](https://github.com/cmangos/classic-db/blob/ec4f596146be6467ea93c57397858e329e2db852/InstallFullDB.sh)
combines a full database with migrations/content updates. The adapter must inspect
the user's installed schema, `db_version` and content-version metadata; it must
not infer an installed revision merely from the repository checkout.

## What local client data supplies

The client's MPQ patch chain contains rendering assets, terrain/world models,
textures, audio, interface code and DBC client tables. benilla already reads these
locally through its format/asset crates. Creature display/model data, map and
faction tables, and spell presentation/range/cast-time/duration tables can be
read from that local install. They do **not** replace server spawn placements,
complete creature/item templates, quest objectives and rewards, loot tables,
server AI or mutable character/world state.

A key current-core distinction: `SQLStorages.cpp` binds authoritative spells to
SQL `spell_template`, and `ObjectMgr::LoadSpellTemplate` loads it. The core still
loads `SpellCastTimes.dbc`, `SpellDuration.dbc`, `SpellRange.dbc` and other client
tables. benilla's `Spell.dbc` reader supports presentation and client-side gates;
copying it into a simulation is not the complete server spell implementation.

## Minimum table closures, by capability

There is no small universal table set that makes all of Classic playable. For a
first selected region, take a transitive dependency closure and fail on required
mechanics outside the supported profile.

### Static creatures and melee

- `creature`: `guid`, `id`, `map`, `spawnMask`, positions/orientation,
  `spawntimesecsmin/max`, `spawndist`, `MovementType`
- `creature_template`: identity/name, levels, display choices/probabilities,
  faction, flags, unit class/rank, stat multipliers and fallback fields, attack
  periods, damage school, loot IDs, kill credits, movement and AI/script references
- `creature_template_classlevelstats`: class/level base health, mana, damage,
  attack power, armor and attributes. Current
  [Creature::SelectLevel](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Creature.cpp)
  combines these with template multipliers; raw template health is insufficient
- `creature_model_info`: combat reach, bounding radius and speed/model choices
- Inspect `creature_spawn_data` + `creature_spawn_data_template`,
  `creature_spawn_entry`, `creature_addon`, `creature_template_addon`,
  `creature_equip_template`, movement tables, linking/conditional spawn tables,
  pools, spawn groups and game-event tables. Resolve supported overrides or reject
- Inspect `AIName`, `ScriptName`, `creature_ai_scripts`/summons,
  `creature_template_spells`, `creature_spell_list_entry`, `creature_spell_list`
  and `creature_spell_targeting`. Presence does not mean the Rust runtime supports
  those behaviors; no embedded SQL/C++/EventAI interpreter is supplied

The first slice deliberately supports only authored static/passive or stationary
fixed-melee records. Actual Classic stat and combat parity must be ported and
fixture-tested separately, including RNG, armor, hit/miss/crit and faction rules.

### Kill quests and rewards

- `quest_template`: eligibility, objectives, text, reward/choice items, money,
  spell rewards, chain/exclusivity, time limits, condition and script references
- `creature_questrelation` starts and `creature_involvedrelation` finishes quests;
  these reference template IDs, not spawn GUIDs
- `gameobject_questrelation`/`gameobject_involvedrelation` require a separate
  gameobject implementation. Reject those in the first creature-only profile
- `conditions` is needed whenever an eligibility/loot/script record references
  one. `StartScript`/`CompleteScript` and DB script/ScriptDev2 hooks need explicit
  implementations, not an ignored string field
- `item_template` for every source/required/reward item; creature/reference loot
  tables for item objectives or drops. Plain kill objectives alone need no loot
- `locales_quest` and greeting/gossip tables are presentation dependencies if
  localized/full questgiver interaction is required

Positive `ReqCreatureOrGOIdN` describes a creature target and negative values a
GO target; `ReqSpellCastN` changes the objective semantics. A converter must
inspect these with item/source and special flags, never reinterpret every row as
"kill N creatures". Fixed `reward_xp` in the small boundary is not a direct SQL
column: [Quest::XPValue](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Quests/QuestDef.cpp)
derives XP using `RewMoneyMaxLevel`, quest level and player level. A parity
adapter needs that rule or must reject the dynamic reward; no silently fixed XP.

### One ability, then spells

- SQL `spell_template` supplies core effect/target/attribute/cost/timing data
- The player's local spell cast-time/duration/range/radius DBC tables supply
  referenced values; `Spell.dbc`/icons/visuals support the client presentation
- Inspect `spell_chain`, `spell_learn_spell`, `spell_affect`, `spell_bonus_data`,
  `spell_proc_event`, `spell_proc_item_enchant`, `spell_script_target`,
  `spell_target_position`, `spell_threat`, `spell_area`, spell groups/stacking,
  conditions, script hooks and core effect code as the chosen spell requires
- Start with one explicitly supported direct ability. Reject additional effects,
  implicit targets, proc/aura/trigger dependencies or special mechanics until
  they are implemented and tested. A spell ID alone never enables a spell

### Items and new-character baseline

`item_template` contains gameplay stats, damage, restrictions, stacking,
equipment/use spells and script references; client item display data alone does
not. For initial characters use an independently reviewed closure of
`playercreateinfo`, `playercreateinfo_item`, `playercreateinfo_spell`,
`playercreateinfo_action`, `playercreateinfo_skills`, `player_classlevelstats`,
`player_levelstats` and `player_xp_for_level`, plus local race/class/outfit DBCs.
Do not read `realmd`, account or character SQL databases for content import.
Persist new P2P character/world state in the fork's own versioned format.

## Adapter approach: bounded local export, no SQL-dump execution

1. User already has a local, consistent world database. Stop its writer or use a
   reviewed consistent snapshot. The reviewed base uses MyISAM tables; an InnoDB
   read-only transaction alone does not guarantee a whole-database snapshot
2. Use an existing dedicated SELECT-only account through the user's local SQL
   client. Do not store credentials in exported JSON or command-line arguments.
   Creating that account/grant is a separate authorized setup action
3. A future original exporter uses a fixed query allowlist with explicit columns
   and numeric region/entry parameters. It reads table/column metadata and version
   rows first. Use the installed schema fingerprint and pinned adapter profile;
   reject unknown/missing columns needed for semantics. Never load the dump into
   a server, call `SOURCE`, execute arbitrary input SQL or run upstream installers
4. Fetch bounded rows for the region and their dependency closure. If `MAX+1`
   rows are returned, fail rather than truncate. The first adapter should export
   a small verified region, not the entire game world. No remote DB connections
5. Validate numeric ranges and deterministic rounding; resolve supported
   references; emit all unsupported required semantics with table/key/reason.
   Only a report with no unresolved mechanics may produce a runnable profile.
   Script/condition rows are data to inspect, never instructions to execute
6. Emit a versioned JSON file to the gitignored state root. Validate through this
   crate before activation. Keep the previous active file if validation fails;
   use a temporary file + sync + atomic rename for any later activation step
7. Compute a compatibility digest. All peers supply their own local asset/content
   inputs and must match the explicit content/simulation profile before joining

The Rust world boundary supplies a **pure validator and original fixtures**.
The separate Python capture tool now supplies a bounded native-source projection
and explicit blocker report, not a native-to-runtime compiler. Neither has
connected to a real SQL database, converted a playable Classic region or tested
a Blizzard install. A raw SQL parser was deliberately avoided: versioned MySQL
comments, escapes, session modes, expressions and migration statements make an
apparently small parser unreliable and unsafe to pretend is general-purpose.

## Next executable gameplay slices

Each step should remain an isolated, fixture-tested change. A data validation
success is never the acceptance test for a playable world.

1. **Solo local world:** a local simulation driver exists independently of P2P
   discovery/configured peers. With one active player it advances immediately;
   no handshake, minimum-two roster, configured-peer quorum or network timeout
   is required. Keep the existing synthetic all-peer session clearly separate
2. **One visible NPC:** consume the small validated fixture, create/render one
   creature through the app entity/event path, target it, enforce map/range,
   apply attack periods, health/death/respawn and one reward exactly once. Use
   separate stable spawn ID and respawn generation for reward identities
3. **One quest and one ability:** accept at the start relation; kill credit only
   for a committed matching death after acceptance; turn in at the finish
   relation once with atomic reward. Enforce ability-known, target, mana, range,
   cast completion and cooldown; commit effects in deterministic tick order
4. **Persistence:** atomically checkpoint world ID, schema/simulation/content
   hashes, tick, RNG state, spawn generations/respawns and owner-bound character
   health/resources/location/XP/inventory/quest status/cooldowns. Load/validate
   before mutation; fail on wrong version/content/world. Test crash/retry and
   double reward/turn-in attempts, not just a clean save/load
5. **Friend joins this world:** explicit invitation names the world ID and current
   checkpoint hash. Establish a committed join boundary and transfer/validate
   only needed mutable state. Match content and engine versions first. Do not
   transfer local world assets/text databases, trust peer-supplied progression,
   or merge two independently advanced offline worlds automatically
6. **Private Classic subset:** implement the pinned local exporter and first
   provenance-tracked CMaNGOS rules, comparing selected input/output fixtures
   against the matching upstream revision. Enable only that proven region,
   creature type, quest shape and ability. Expand from an explicit reject list

Acceptance scenarios: create a new character with no peers configured; enter a
world; fight/respawn; accept/complete/turn in once; cast/cooldown once; restart and
recover identical state; join a friend at the agreed checkpoint; reject wrong
world/content/owner; partition without silently forking/merging progression.
No global Classic/world/spell parity claim follows from passing this slice.

The [read-only exporter contract](CLASSIC-EXPORTER-CONTRACT.md) specifies the next
schema probe, bounded native capture, offline compiler report and fake-driver
tests. Its capture/validation commands are implemented and fake-driver tested; real SQL
connectivity and the native-to-runtime compiler remain unverified/unimplemented.
