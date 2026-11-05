# Local Classic native-source capture

This original tool captures a **private, non-runnable native-data projection**
from an existing local CMaNGOS Classic world database. It does not install a DB,
execute SQL dumps, download content, run imported scripts, grant privileges,
compile playable content, or activate/change the current world.

Historical source-v1/v2 capture commands produce bounded projections. Their
reviewed columns, dependency checks and unsupported-mechanic refusals remain
separate from the complete catalog commands below. The catalog reader accepts
only catalog-v3, never an adapted historical projection. Tests use original
authored rows. Private whole-database receipts establish schema and storage
coverage, not source execution, loaded managers or gameplay support.

Requires Python 3.11 or later. Pure tests and offline commands need only the
standard library. Live SQL support is optional.

## Complete catalog schema inventory

`catalog-schema --credentials-file FILE --maps 0 1 --pilot world_template
--output NEW_DIRECTORY` records all 194 tables and 2287 columns, full column
defaults, charset, collation, indexes, engine and allocator metadata.
It uses only SELECT statements through the local port-33306 reader. The two-row
pilot retains raw SQL bytes as hex, including both world-template maps.
`catalog-schema-verify NEW_DIRECTORY/pilot.json` independently checks the frozen
schema and loader-policy identities. The Rust `benilla-catalog schema FILE`
command implements the same checks independently.

The loader ledger records pinned source consumers and the startup call list.
Unresolved tables and per-loader conversion or occurrence obligations remain
explicit. Owner review is pending. Missing, extra, empty-name or schema-mismatched
tables, unknown loader IDs and forged completion claims refuse. A present empty
relation is not a missing table. This sequential phase0 inventory is diagnostic,
not an atomic multi-engine snapshot, F1 input identity or gameplay admission.
Historical source-v1/v2 projections remain separate from catalog-v3.

## Immutable catalog storage

`catalog-capture --credentials-file FILE --dbc-directory DIRECTORY --output ROOT`
retains every world table and all 158 extracted DBC files. It streams raw SQL
protocol bytes without text or numeric conversion. SQL NULL, empty bytes,
embedded NUL and signed zero remain distinct. Acquisition uses a disk-backed
SQLite key sort with a 1 MiB page cache. SQLite is not runtime authority.

The output starts with `candidate.json`, not a published manifest. Validate it
with both independent readers before publication:

```sh
mbx run --locked -p benilla-world-runtime --bin benilla-catalog -- \
  candidate ROOT NEW_RUST_RECEIPT
python3 scripts/classic-export.py catalog-publish ROOT \
  --rust-receipt NEW_RUST_RECEIPT --receipt NEW_PYTHON_RECEIPT
```

`catalog-publish` validates all bytes again in Python and compares the Rust
identity, row hash and inventory counts. It fsyncs the object directories and
publishes checked manifest bytes with an atomic no-replace link. Failure leaves
no published manifest. Existing directories, files and published manifests are
never replaced. `catalog-verify ROOT` and Rust `verify ROOT` read published data.
Both readers reject historical source-v1/v2 formats.
The current coverage policy classifies all 194 tables, with 173 consumed and
21 archive-only. Archive entries retain the exact case-insensitive source-name
search and its result. Character-database auction and bug-report operations do
not read the identically named world tables. Zone tables are write-only
diagnostic outputs. Warden scans have a reader even though anticheat is disabled
for the selected execution. Catalogs bound to older coverage policies refuse;
their immutable files remain historical evidence.


The canonical JSON manifest binds schema, coverage, relation, index and DBC
roots. It has no per-row or per-page array. Each relation has ordinal and raw-key
tree roots. Every descriptor contains a lowercase SHA-256 and exact byte length.
The catalogue hash covers the manifest without its own hash field. The row hash
starts with `benilla-catalog-rows-v3` and a NUL byte, then each schema ID, ordinal,
frame length and complete frame in relation-name and occurrence order.

The version-three binary layout uses little-endian integers:

| Record | Layout and meaning |
| --- | --- |
| Data header, 32 bytes | `8s HH I Q II`: `BNCAT3` and two NUL bytes, version, kind, schema ID, chunk ordinal, row count, payload length. |
| Row header, 16 bytes | `I Q HH`: complete frame length, physical ordinal, column count, bitmap length. |
| Row payload | NULL bitmap, then a uint32 length and exact bytes for each non-NULL field. Unused bitmap bits are zero. |
| Index header, 32 bytes | `8s HH I QQ`: `BNCIDX3` and one NUL byte, version, kind, schema ID, first ordinal, entry count. |
| Ordinal entry, 48 bytes | `32s Q II`: digest, first row ordinal, row count, byte length. |
| Key entry, 80 bytes | `II Q 64s`: key length, reserved zero, ordinal, inline key or external-key digest. |
| Key internal entry, 120 bytes | Key entry, child digest and uint64 subtree row count. Its separator equals the child's last key and ordinal. |

Index kinds one and two are ordinal leaves and internal pages. Kinds three and
four are key leaves and internal pages. Every page is exactly 4096 bytes with
zero padding. Internal ordinal entries describe child pages, not row chunks.
Root heights decrease on every edge. Readers reject repeated descriptors,
cycles, gaps, overlaps, wrong membership and duplicate physical occurrences.
Duplicate values remain separate occurrences.

Raw keys concatenate uint32 lengths and field bytes in declared primary-key
order. NULL uses `0xffffffff`. A relation without a primary key uses all columns
as a multikey. Keys up to 64 bytes are inline. Longer keys use a checked blob
and zero slot padding. All keys are at most 64 KiB. These indexes do not define
numeric conversions, map/grid coordinates or source loader occurrence order.
F2.2 owns the corresponding source-derived views.

Both readers validate every referenced byte before use. They retain regular
file and ancestor-directory handles, reject symlinks and path escape, and
check inode identity, size, modification time and change time on later reads.
Every cache fill checks its complete digest. A missing chunk never means absent.
Verified key order, membership and occurrence counts prove negative lookups.
The Unix implementation refuses other platforms instead of using an unsafe
pathname fallback.

The shared limits are 1 MiB metadata or chunks, 64 KiB frames or key blobs,
4096-byte pages, eight index levels, 256 requested batch rows, an 8 MiB shared
raw cache and a 32 MiB decoded batch budget. SQL storage allows 2,000,000 rows
and 1 GiB of objects. Index storage allows 128 MiB. DBC storage allows 128 MiB
in total and 64 MiB per file. Limits refuse; they never clip a relation.

The measured whole-database acquisition has 874,579 rows, a 3012-byte maximum
frame, a 397,373-byte maximum chunk and 75,800,576 index bytes. Its SQL/key/metadata
objects use 142,725,246 bytes, separate from 28,041,880 retained DBC bytes.
These receipts establish the fixed caps without decoded world-row clones.
Reader reports separate requested reads, row-view size, shared cache bytes,
tree depth and process RSS. RSS uses `/proc/self/status` `VmHWM`, because
`getrusage` can include the launcher's pre-exec peak.

Rust scan callbacks borrow one row view. Python scan views expire on the next
iteration, and `cells()` explicitly copies fields. Readers do not keep decoded
row batches. DBC validation streams 64 KiB blocks without interpreting files.
The four zero-byte extractor outputs remain inventory evidence, not valid WDBC
tables. Loader conversions, registry and SQL occurrences, F1 binding and owner
coverage review remain unresolved. Every report has `complete=false`.

## Complete spawn catalog views

`classic_export.catalog_spawns.SpawnViews` and Rust `SpawnViews` consume the
verified catalog. They retain every map, all alternatives, global creature/GO
templates, models, equipment, class stats, spawn data and addons. Raw scans
retain source ordinals and duplicate values. Typed lookup absence comes from
the complete catalog index. Model closure includes other-gender and alternative
models and reports missing models instead of inventing them.

The immutable `spawn-indexes.json` sidecar binds the catalog identity and
4 KiB paged map/grid/cell indexes in the catalog's content-addressed index
directory. Both readers recompute every index member before using it. Index
keys are big-endian map, grid, cell and GUID uint32 values. Their values are
source uint64 ordinals. Numeric index order never replaces occurrence order.
Invalid coordinates remain in raw views and have an explicit unindexed count.

Float conversion uses source `atof` to double followed by binary32 rounding,
including signed zero, from `Field.h:53`. Rust independently parses the SQL
decimal prefix. Hexadecimal and named nonfinite inputs refuse rather than
claiming an unimplemented conversion. Float-bit hashes concatenate little-endian
uint64 row ordinal, uint32 column ordinal and uint32 result bits. Raw family
hashes cover complete original frames in occurrence order.

`catalog-spawn-capture --credentials-file FILE --output NEW_DIRECTORY` retains
the two complete joined loader cursors as bounded catalog row frames. Their
queries are pinned at `ObjectMgr.cpp:1877-1890,2086-2094`. Both readers reproduce
the full outer-join multiplicities and NULL extensions from catalog rows.
They compare the resulting multisets and separately hash the recorded cursor
order. Missing, exhausted, surplus, forged or changed streams refuse.

`catalog-spawn-verify ROOT --occurrences DIRECTORY --receipt NEW_FILE` checks
the family and emits its independently reproducible view hash. Add
`--build-indexes` once to write new content-addressed pages and their sidecar.
Rust uses `benilla-catalog spawns ROOT DIRECTORY [NEW_RECEIPT]`.
Each joined cursor has a 1 GiB byte quota and a 2,000,000-row limit. Family
indexes and joins have a separate 32 MiB decoded budget. Limits refuse instead
of clipping at 4,096 rows. Diagnostic cursors are not F1 occurrences. Reference-execution
binding and gameplay admission still refuse, and native format stays 89.

## Group, linking and core waypoint views

`catalog-group-capture --credentials-file FILE --output NEW_DIRECTORY` retains
all 15 pinned loader cursors, including the three grouped `COUNT(Point)` queries.
`catalog-group-verify ROOT --occurrences DIRECTORY --receipt NEW_FILE` recomputes
their complete projections and counts from the checked catalog. Rust independently
does the same with `benilla-catalog groups ROOT DIRECTORY [NEW_RECEIPT]`.
Both compare every projected occurrence, preserve duplicates and NULLs, and bind
recorded cursor order separately from catalog index order. Rehashed forged,
missing, exhausted, surplus and wrong-query streams refuse.

Pinned loader owners at CMaNGOS `8ec338a1` are:

| Table | Source under `src/game/` |
| --- | --- |
| `spawn_group` | `Globals/ObjectMgr.cpp:1125` |
| `spawn_group_formation` | `Globals/ObjectMgr.cpp:1192` |
| `spawn_group_spawn` | `Globals/ObjectMgr.cpp:1250` |
| `spawn_group_entry` | `Globals/ObjectMgr.cpp:1355` |
| `spawn_group_linked_group` | `Globals/ObjectMgr.cpp:1398` |
| `spawn_group_squad` | `Globals/ObjectMgr.cpp:1426` |
| `creature_linking_template` | `Entities/CreatureLinkingMgr.cpp:75` |
| `creature_linking` | `Entities/CreatureLinkingMgr.cpp:120` |
| `creature_movement` | `MotionGenerators/WaypointManager.cpp:173,198` |
| `creature_movement_template` | `MotionGenerators/WaypointManager.cpp:302,329` |
| `waypoint_path` | `MotionGenerators/WaypointManager.cpp:400,441` |
| `waypoint_path_name` | `MotionGenerators/WaypointManager.cpp:427` |

The family report binds all raw frames, float-conversion bits, loader projections,
grouped counts and reference summaries. GUID membership uses the group's creature
or GO namespace. Missing formation is valid. Linked groups and squad overrides
remain explicit inputs, not empty exclusions. The summaries retain duplicate
membership, missing endpoints, cross-map associations, invalid linking flags,
self-despawn links, nonunique entry masters, point-zero paths and waypoint gaps.
`COUNT(Point)` counts non-NULL rows across every PathId, not the maximum point.
Template path keys use the source's wrapping `(Entry << 8) + PathId`.

These are complete source input views, not effective executable managers.
Reference-dependent condition/expression checks, pool/event exclusions, formation
slot corrections, squad selection, script/terrain waypoint corrections and the
dynamic unique-master query's reached occurrence still require the selected
execution. Raw invalid rows remain evidence. Summaries do not choose a duplicate
winner or make a source skip into a catalog rejection. All reports have
`complete=false`; `groups.reference_execution_missing` and
`groups.gameplay_not_admitted` remain refusals. No legacy selected spawn plan
can use this diagnostic report to admit gameplay. Native format stays 89.


## Run the dependency-free checks

```sh
python3 scripts/test-classic-export.py -v
python3 scripts/classic-export.py --help
python3 scripts/classic-export.py plan --schema benilla-config/schema.json \
  --selection benilla-config/selection.json --output benilla-config/capture-plan.json
python3 scripts/classic-export.py validate benilla-config/classic-source.json
```

`plan` and `validate` never connect to a database. The plan is an inspection
artifact; it is not executable SQL and `capture` does not execute plan contents.
Capture independently validates selection/schema. Paths above are examples;
create the private state directory first. Outputs must be new files.

## Optional complete spell numeric capture

For source-v2, `capture --spell-numeric` adds
`benilla-classic-spell-numeric-selection-v1` as `spell_numeric_capture`.
It reads every selected spell's complete pinned numeric SQL projection and
the complete selected `spell_facing` lookup inside the existing read snapshot.
Both tables require reviewed numeric types and observed primary keys.
The extension binds exact selected IDs, field coverage, row limits, schema,
selection, DB version, legacy rows, numeric rows, and facing rows by canonical
hashes. Validation rejects duplicate, missing, extraneous, or conflicting rows.
It never changes the mandatory 108-column legacy `spell_template` rows.

SQL supplies `EffectBonusCoefficient1=0.123`; there is no corresponding client
DBC field. The extension also retains `IsServerSide`, totems, reagents/counts,
target limits, the full uint64 family mask, damage multipliers, faction,
reputation, aura-vision constraints, and numeric visual/unused annotations.
Each decimal stays a canonical tagged decimal until the preparation verifier
projects it directly to binary32. Names and ranks have explicit
presentation-only dispositions in the positive audited mage domain.
Every original client word remains a separate local DBC input.

Legacy bundles still validate without the extension, but have unknown effective
numeric coverage. Additional installed columns still produce
`schema.unreviewed_columns`; this extension does not clear table-wide warnings.
Facing absence is neutral only after a complete selected read.
The extension grants no registry, hook, learning, initial-cast, global-area,
NPC, geometry, or world-entry authority.

## Optional reputation policy recapture

`reputation-policy --source OLD --dbc-dir DBCS --gender 0
--quiescent-snapshot` extends an unchanged operator-quiescent source-v2 epoch.
Supply the usual database, reader, port, and new output path. Keep the old file.
The command performs SELECT-only bounded reads of `reputation_reward_rate`,
`reputation_spillover_template`, and `creature_onkill_reputation`. Each table
query has a 4,096-row limit and retains every loader column. Float columns use
the database's text representation and exact decimal-to-binary32 conversion.
The capture also checks the three tables' SQL types, nullability, and primary
keys against the pinned layouts. Their normalized metadata remains bound in the
extension, so a decimal or double column cannot claim FLOAT conversion.

`reputation_policy_capture` uses `benilla-classic-reputation-policy-v1`.
It includes an explicit row-or-null lookup for every ID in the complete bound
Faction DBC and every selected creature template. The full Faction domain closes
all valid spillover, team, quest, and kill faction references without filtering
policy targets to the immediate route. Missing lookup entries and rows outside
their selected keys reject. Bindings cover the selection, original source tables,
schema, DB version, consistency mode, Faction bytes, and all policy lookups.
As with skill-training recapture, unchanged epoch provenance depends on the
operator's quiescence assertion, not a claim of reference-loader observation.

`Field.h:53-59` at CMaNGOS `8ec338a1` defines float conversion, unsigned wrapping,
and booleans as strictly greater than zero. `ObjectMgr.cpp:7480-7545` accepts
selected kill rows only when their nonzero factions exist. The consumer retains
present rows as unsupported native kill awards; proven absence is not a zero-rate
row. The reputation policy tests run in `python3 scripts/test-native-starter.py`.


## Optional ordinary quest presentation capture

For source-v2, `capture --quest-presentation` captures quest text, both request-emote
delays, greetings, NPC-text fallbacks, and locales in the same snapshot.
It also captures incoming quest relationships.
The [ordinary giver contract](QUEST-RELATIONSHIP-CONTRACT.md#captured-ordinary-givers-and-presentation)
defines the exact scope and the independent runtime dependencies.
All quests attached to a selected giver remain selected.
Unsupported script commands, gossip services, and unknown loaded callback/order
inputs remain precise refusals, not filtered quests or a runnable Northshire claim.
`capture --quest-mechanics` also captures checked completion scripts, source-item
dependencies, ordinary gossip, direct conditions, and their text dependencies.
It implies `--quest-presentation`. The
[mechanics contract](QUEST-RELATIONSHIP-CONTRACT.md#completion-scripts-source-items-and-ordinary-gossip)
defines its versioned evidence and remaining host requirements.


## Optional local read-only capture

This requires an already prepared local world DB, an existing SELECT-only account
and an interactive non-echoing terminal. Credentials are prompted locally, never
accepted as CLI arguments, saved, exported or logged. Insecure getpass fallback
is rejected. This tool does not create that account or change its grants. Do not
paste credentials into chat, issue reports, committed files or shell arguments.

The optional driver dependency is pinned to the
[official PyMySQL release](https://pypi.org/project/PyMySQL/1.2.3/) with distribution
hashes. Install it in a local virtual environment if needed; no package installation
is performed by the script. Some server authentication mechanisms require extra
PyMySQL dependencies; this script never installs them or downgrades authentication.

```sh
python3 -m pip install --require-hashes -r scripts/classic_export/requirements.txt
python3 scripts/classic-export.py inspect --database YOUR_WORLD_DB \
  --user YOUR_EXISTING_READER --output benilla-config/schema.json
python3 scripts/classic-export.py capture --database YOUR_WORLD_DB \
  --user YOUR_EXISTING_READER --selection benilla-config/selection.json \
  --output benilla-config/classic-source.json
```

Only 127.0.0.1 is accepted; `--port` defaults to 3306. No remote-host option or
free-form DSN exists. The driver uses fixed allowlisted SELECTs and parameterized
values. Multiple statements, local-file loading and user option files are not
enabled. The only explicit transaction commands set repeatable-read, request a
read-only consistent snapshot and roll back. See
[PyMySQL connection](https://pymysql.readthedocs.io/en/latest/modules/connections.html)
and [cursor](https://pymysql.readthedocs.io/en/latest/modules/cursors.html) APIs.

MyISAM is not protected by an InnoDB snapshot. If the source includes MyISAM, the
operator must first prepare a quiescent read-only snapshot with no active writer,
then explicitly add `--quiescent-snapshot`. The script does not stop the server,
create backups, acquire global locks, or prove that no other writer exists.
Version rechecks detect some changes but are not a substitute for consistency.
A transaction-start failure stops capture; there is no less-safe fallback.

## Selection and captured data

Copy `selection.example.json` and choose a small local region. The example is an
original bounding box around zero, not a canonical WoW starting area. Coordinates
are thousandths of a world unit; no metre conversion is assumed. Explicit
quest/spell IDs are optional. Values must be integers; no SQL expressions.

For one selected race/class and levels 1 through a chosen cap (at most 60), use
`selection.starter.example.json`. This requires selection v2 and emits source v2
with composite player keys. V1 region selections/bundles remain supported;
missing player rows cannot be migrated offline, so recapture to a new file with
v2. See the [starter contract](../../docs/CLASSIC-STARTER-CAPTURE.md) for exact
columns, types, row limits, dependency semantics and remaining DBC blockers.

Capture follows these reviewed references:

- Region `creature` spawns to templates, kill-credit templates, models and
  class/level stats
- Creature quest start/finish relations and selected quests, their objective
  creature templates, giver templates and source/required/reward items
- Selected spells, quest/item spells and bounded triggered-spell references
- With an explicit starter selector: eight native creation/stat/XP tables;
  custom item and action references, initial spells, zero-wildcard skill masks,
  every selected level and XP threshold, item requiredspell references, and
  exact-schema spell_chain/spell_learn_spell selector coverage and references

The full projected column allowlist is `columns.py`, from the pinned source
schemas/structures linked in `docs/CLASSIC-WORLD-IMPORT.md`. `db_version` metadata
is inspected as a base table before its fixed singleton read. Column inventory
includes extra columns, but they are not silently treated as supported mechanics:
unreviewed columns are reported. A missing projected column fails.

New v2 captures also include exact-schema [native spawn dependency evidence](NATIVE-SPAWN-CONTRACT.md):
spawn-data bindings and referenced templates plus Entry 0, selected event/pool
membership, and GUID/template addon rows in the same read-only snapshot.
Legacy v2 captures without that evidence stay unproven; applicable hooks remain
unexecuted, including neutral-looking rows.

New v2 captures also include exact selected [spell/aura registry evidence](SPELL-REGISTRY-CONTRACT.md).
Every captured spell-template ID is queried without filtering names, and registry
schema/key/selector/snapshot hashes are validated. Legacy missing coverage stays
unknown. IDs outside 1..INT32_MAX reject before the registry query because the
native signed-SQL-to-uint32 cast would otherwise permit false absence.

New v2 captures retain full reviewed EventAI rows and selected
[broadcast text dependency evidence](EVENT-TEXT.md), including all associated
locales. Missing base rows remain missing; absent broadcast tables remain
unknown. No event or text action executes, and no production profile is issued.

With `--body-loot`, v2 also captures bounded selected body/reference loot rows
and leaf item/spell dependencies. The [loot capture contract](LOOT-CAPTURE.md)
distinguishes selected-key completeness from a separate whole-loader query
observation. Preparation can resolve bounded ordinary/reference loot shapes;
source authenticity, explicit rates, inventory audits, and native admission remain separate.

Without the optional extensions, the capture omits other event/group/linking
state, gameobjects, non-body loot, conditions, other AI/script data, quest chains
beyond referenced fields, spell procs/stacking, and the local client DBCs.
The optional Python API `capture(..., capture_spawn_entries=True)` adds
[raw entry-alternative exclusion evidence](SPAWN-ENTRY-CONTRACT.md) for all
selected GUIDs. It does not resolve alternatives, promote template dependencies,
or cover other spawn owners. Without that extension, zero `creature.id` rows
remain raw and carry `spawn.entry_substitution_not_captured`; with it they remain
blocked as `spawn.entry_substitution_unresolved`, including after an empty scan.
A nonzero captured `NextQuestInChain` is a `quest.chain`
blocker, like the other captured chain fields. Zero own-link fields do not prove
absence of inbound links from uncaptured quests; global chain closure remains
unproven. Offline validation recomputes the exact required findings. Older
bundles missing a newly required finding fail validation and should be
recaptured; unchanged raw table hashes cannot excuse missing report findings.

Legacy v2 captures without server learning coverage remain valid but unproven;
new v2 captures require both reviewed learning tables. A bounded native compiler
request may add exact item/spell roots through `--dependency-request`; see the
[local dependency flow](../native_starter/README.md#local-dependency-recapture).
The tool does not turn missing requirements into default values. These gaps and
unimplemented gameplay rules are reported; `capture_complete` and `runnable` are
always false. A zero `unsupported` count in an empty region still cannot make it
runnable. No native-to-runtime compiler is included.

Native integer signedness/ranges, bit widths, decimal precision/scale and string
column lengths are checked against inspected SQL types, plus conservative input
limits. Native decimal strings preserve value without runtime quantization;
insignificant trailing zeros are normalized. FLOAT/DOUBLE use the driver's finite
numeric value and decimal rendering, not a claim of original binary payload
identity. Exact gameplay arithmetic still needs its own reviewed port.

## Bounds and output safety

Hard caps: 4,096 spawns/templates, 512 quests, 2,048 items, 512 spells, sixteen
creature rounds and at most 512 newly visited spell nodes, 64 KiB/row, 16 MiB/query/table and 64 MiB aggregate responses/output. SQL
uses `LIMIT cap+1` and rejects overflow without truncating. Row budgets apply
while streaming, before retaining the next row. A single driver row is received
before its limit is checked; the tool does not claim a malicious SQL server
cannot send an oversized protocol packet. Use only the trusted local database.
Metadata reads and repeated closure queries count toward response budgets.

An output is written to a private temporary file, synced and published with an
atomic no-clobber hard link. Existing files are never overwritten, including a
race at publication. The filesystem must support same-directory hard links.
If post-publication directory sync fails, the complete file may exist but success
is not reported; validate it instead of blindly retrying or deleting it. Never
activate a partial capture or upload its content automatically.

The output hashes are integrity/compatibility checks, not signatures or proof of
licensing or trustworthy authorship. Bundle validation rechecks canonical rows,
references, snapshot claims and required blockers. It always rejects a runnable
claim. Content remains private and must not enter source control or CI artifacts.
GPL notices for the tool do not grant rights to the user's imported content.

## Optional initial spell hook evidence

For a source-v2 starter selection, add `--initial-hooks` to `plan` and `capture`
to include exact selected `spell_area` and `spell_proc_event` rows. Missing legacy
coverage remains unknown. This captures all raw matching rows and does not
execute conditions or resolve inherited proc semantics. See the
[bounded initial hook capture contract](INITIAL-HOOKS.md).

### Optional quest relationship diagnostics

The Python API has an opt-in, exclusion-only raw inbound quest relationship
capture; see [the contract](QUEST-RELATIONSHIP-CONTRACT.md). It preserves signed
incoming NextQuestId, incoming chain and breadcrumb edges without importing
external quests as playable content. Legacy missing evidence remains unknown.
This diagnostic is not connected to a compiler/consumer or native admission.

## Optional participant evidence

For a source-v2 starter selection, use `capture --participants` or the Python
API `capture(..., capture_participants=True)`.
The versioned `participants_capture` binds the selector, source pins, schema,
snapshot, selected roots, retained rows, and whole-table loader observations.
Strict checks cover exact columns, types, nullability, order, and primary keys.
DB script tables have no primary key at this pin. Their observation retains
duplicate rows and source priority order rather than inventing a unique key.

The extension captures selected gameobject spawns and entry alternatives.
It captures complete template data fields, addon state, event/pool membership,
all transports, EventAI summon positions, and all ten DB script table scans.
It keeps non-text script commands as conservative candidates across all maps.
Missing tables have explicit Unknown observations, never captured-empty rows.
Whole-table scans have separate bounds of two million rows and 512 MiB per table.
Selected rows remain bounded to 4,096 per table and 16 MiB in total.
These scans do not authenticate source bytes or establish a live database lease.

Preparation refuses every positive participant type under the current
empty-participant geometry contract. It also refuses unproved exterior bounds,
transport paths, and runtime creation paths.
See the [participant consumer requirements](../native_starter/CONSUMER-CONTRACT.md#non-render-participant-requirements).
No raw SQL rows or extracted assets belong in repository fixtures.

