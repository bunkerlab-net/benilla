# Read-only Classic exporter contract and compiler plan

The original `scripts/classic-export.py` now implements schema inspection,
offline planning, an optional local read-only capture adapter, and offline bundle
validation. Its tests use original fake-source rows; actual MySQL/MariaDB
connectivity and a real Classic schema remain unverified. See the
[tool README](../scripts/classic_export/README.md) for exact available commands,
limits, consistency requirements and remaining gaps. The native-to-runtime
profile compiler below remains a design, not an available command. No captured
native data is runnable yet.

## Separate source capture from gameplay compilation

A real Classic row must not be made compatible by changing its damage, quest XP,
spell effects or AI behavior to fit the original fixture model. Use two stages:

1. A read-only **capture adapter** emits a private `benilla-classic-source-slice-v1`
   (region) or `benilla-classic-source-slice-v2` (selected starter) bundle with
   selected native rows, exact schema inventory, version metadata and source
   hashes. It does not declare those rows playable
2. An offline **profile compiler** transforms only mechanics supported by the
   target simulation, with an explicit field-disposition report and parity
   evidence. It either emits the runtime's accepted format or a blocker report

Initially the compiler should report that native melee variance/armor/hit rules,
level-dependent quest XP, or unimplemented spell effects need support. Native
capture can still be useful and complete without a runnable output. Do not set
`sql_importer_available` or imply Classic gameplay based solely on capture.

## Implemented capture surface and future compiler

Available commands are `python3 scripts/classic-export.py inspect`, `plan`,
`capture` and `validate`. Inspect/capture require a local database name, existing
SELECT-only username and optional loopback port; credentials are read through a
secure interactive local terminal. Plan/validate are offline. A selection JSON
specifies a bounded numeric map/region/quest/spell set. No command accepts SQL,
remote hosts, scripts or arbitrary table/column identifiers.

The proposed future `benilla-classic-compile` would consume the validated native
source bundle and emit either a reviewed runtime format or blocker report. It
must not activate output automatically. Connection/account/privilege setup stays
separate, explicitly authorized and outside this tool.

## Schema probe contract

The metadata probe is implemented; its live-schema behavior remains unverified:

- Selected allowlisted table/column metadata; server-version/case-mode profiling remains a future extension
- For allowlisted tables: name, BASE TABLE versus view, storage engine, column
  name/type/nullability/order; exact primary/unique-key inventory remains a future extension
- Fixed `db_version.version` and `creature_ai_version` notes only after the base
  table and column metadata validate; separate content-version tables remain a
  future extension
- Core/database revision claims from the chosen adapter, plus independently
  observed installed schema/version. Never turn a Git checkout SHA into a claim
  about the installed SQL database

Probe only the selected world's `information_schema` rows; never enumerate other
users/databases or read realm/character/account tables. Reject views for content
capture to avoid hidden cross-database dependencies. Compare column names/types
and semantic expectations with the adapter's checked-in schema profile. Hash the
sorted inventory using a documented canonical integer/string encoding.

The full current `spell_template` table is not described by the base mangos.sql
file alone. Its exact installed columns remain a prerequisite. Inspect the
[reviewed SpellEntry layout](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Server/DBCStructure.h)
and SQL store format, then compare against that local schema; do not guess
indices from another expansion. The base schema and update history identify
what to review, not permission to download a populated database here.

## Selection and dependency closure

`selection.json` has a fixed version, one numeric map ID, an integer bounding box
in thousandths of world units, and optional explicit quest/ability IDs. A
separate spawn-ID selector is not implemented. Selection v2 additionally chooses
one race/class and a level cap, preserving all eight player creation/stat/XP tables
and their selected item/spell references. Its exact source-backed contract,
composite identities, extra bounds and non-fabricating v1 compatibility policy are
in [Selected Classic player-source capture](CLASSIC-STARTER-CAPTURE.md).
Initial hard bounds: one map, 4,096 selected spawns, 4,096
creature templates, 512 quests, 2,048 item templates and 512 spells. A compiler
can impose smaller limits. No raw query expressions or user table names.

The implemented capture follows these bounded reference sets:

- Region spawns to creature templates, kill-credit templates, models and class/level stats
- Start/finish relations to selected quests, objective/giver creature templates,
  required/source/reward item templates and quest/item spell references
- Explicit spells and recursively referenced triggered spells
- In v2, selected native player creation/item/spell/action/skill-mask rows;
  class and race/class levels 1..cap, XP levels 1..cap−1, and required item/spell rows

Selected v2 spawn overrides, EventAI/text and spell registry dependencies have
separate reader contracts in the tool README. Optional `--body-loot` adds
[bounded body/reference loot capture](../scripts/classic_export/LOOT-CAPTURE.md).
Source-loader ordering and generation remain unproven; non-body loot, conditions,
other AI/script data and quest-chain closure remain uncaptured or unsupported. A full edge-provenance graph and additional dependency families are
future work; current `required_ids` records the captured identity sets rather
than a reason graph for every edge.

Referenced templates need not have a spawn within the selected region. Report an
objective that cannot be completed in the region; do not invent a spawn or drop
the objective. Keep cyclic reference detection and a bounded visited-key set.
Fail on missing required rows, non-unique IDs, invalid ranges or budget overflow.
Do not silently truncate or treat missing dependency tables as empty tables.

The first implementation should request **one spawn plus one quest plus one
spell**, inspect the actual closure, and report blockers. This is a practical
prerequisite test, not a promise that any arbitrarily chosen row is supported.

## Query contract and safety

Use the optional real SQL driver with parameterized numeric/string values. Query text,
identifiers, allowed columns and joins are compiled into the reviewed adapter.
The capture source exposes `inspect()` and `fetch(Query)` with validated query
IDs and parameters, never `execute(user_sql)`. PyMySQL binds/escapes parameters
client-side; this is not a claim of server-side prepared statements.

Every content query is a single SELECT with explicit columns, stable primary-key
ordering and a hard `LIMIT N+1`. Only a small allowlist of metadata SELECTs and
session transaction commands is permitted. No DDL/DML, `CALL`, `LOAD DATA`, file
functions, `INTO OUTFILE`, locking/admin statements, temporary tables, installers,
stored procedures or SQL supplied by input. Disable multiple-statement execution
and local-file loading at the driver. Never feed result text back into SQL.

`READ ONLY` transactions are an additional guard, not the permission boundary:
an existing SELECT-only account and the fixed query set are required. The
[MariaDB transaction documentation](https://mariadb.com/docs/server/reference/sql-statements/transactions/start-transaction)
explains transaction modes/snapshot behavior, while the reviewed CMaNGOS base
schema includes MyISAM tables. For mixed or nontransactional tables require a
user-prepared quiescent/read-only snapshot; a transaction alone is insufficient.
Do not stop their database/server or acquire global locks automatically. Record
consistency mode in the manifest and reject an unsupported live mixed-engine
capture. Re-reading version metadata does not prove row-level consistency.

Stream response rows with limits per row, per table and total bundle (initially
64 KiB/row, 16 MiB/table, 64 MiB total), plus a bounded query timeout and dependency
depth. Error messages use table/key/reason, never connection strings or row text.
Honor cancel without activating partial output. Content remains in gitignored
private local state, never CI logs, artifacts or automatically uploaded bundles.

## Private native bundle

A versioned manifest records adapter/core/database/profile identifiers, observed
schema hash, client build, region, consistency mode and table hashes. Array lengths
determine row counts. The current bundle is one bounded JSON document containing
table arrays; table names come from the allowlist;
keys and values are typed according to the reviewed schema. Preserve signedness,
NULL versus empty, and decimal coordinates as canonical decimal strings or exact
scaled integers. Do not round native precision until the compiler has a named,
tested rounding policy. Imported strings remain inert data; there are no operational SQL or filesystem
path fields. The capture layer never executes a string from a row.

Store only selected rows needed by the closure. Native source bundles can contain
copyrighted world text or private installation information and therefore are not
distribution artifacts. The normalized runtime digest and the native source
manifest hash are distinct; a native bundle hash is not proof of license rights,
correct gameplay, trustworthy authorship or a verified world checkpoint.

## Compiler report and enablement contract

For every selected record, a report includes `table`, `key`, `capability`, and one
of `supported`, `presentation_only`, or `blocked`, with a stable reason code.
Every source column has an explicit reviewed disposition: consumed, known unused
for this profile with a reason, or unsupported. For flag bitmasks, unknown set
bits are blocked. Unknown attributes/effect IDs/target selectors are blocked.

Examples of initial blockers:

- `creature.variable_level`, `creature.stat_rule_unimplemented`,
  `creature.event_ai`, `spawn.pool_or_event`, `movement.path_required`
- `quest.dynamic_xp`, `quest.item_or_spell_objective`, `quest.condition`,
  `quest.script`, `quest.chain`, `quest.objective_outside_selection`
- `spell.multiple_effects`, `spell.target_rule`, `spell.school_or_resist`,
  `spell.aura_or_proc`, `spell.level_scaling`, `spell.server_script`
- `item.equipment_or_use_effect`, `loot.reference_or_condition`

No output can become runnable while a required record is blocked. A supported
native mechanic needs a simulation version and upstream parity fixtures, not a
manual boolean override. A later runtime schema version will be needed for
native quest XP, damage distributions and genuine spell semantics. The present
fixed-damage/fixed-XP/direct-effect fixture model must remain honestly labeled.

## Test contract before real database use

- Fake read-only driver proves only known QueryIds/columns/parameter types are
  reachable; injected IDs/identifiers, remote targets and arbitrary statements fail
- Original synthetic schema/rows cover duplicate/missing keys, cycles, boundary
  counts, mixed engines, unknown columns and flags, signed objective IDs, decimal
  rounding, NULL handling, result truncation and cancellation
- A temporary user-authorized test DB with original rows checks actual driver
  behavior, SELECT-only enforcement, transaction mode, keyset pagination and
  clean disconnect; no real game contents or credentials enter tests
- Native unsupported rows produce specific blockers and no runtime export;
  exactly supported original rows compile to a stable reviewed runtime digest
- An explicit real-data local validation remains required later. A mock DB pass
  cannot establish compatibility with the user's CMaNGOS installation

The immediate missing input for live validation is the **schema-only inventory of an existing local
world database**, not a request to obtain or upload its content or credentials.
The pure planner, closure checker and blocker report are tested against original
fake-driver fixtures. Do not claim the real SQL adapter has been exercised until
an explicit local test has done so; the native gameplay compiler remains unimplemented.
