# Local Classic native-source capture

This original tool captures a **private, non-runnable native-data projection**
from an existing local CMaNGOS Classic world database. It does not install a DB,
execute SQL dumps, download content, run imported scripts, grant privileges,
compile playable content, or activate/change the current world.

The schema probe, planner, typed normalization, dependency checks, blocker report
and optional PyMySQL driver are implemented. Tests use original fake rows only.
**No actual MySQL/MariaDB connection or real Classic content has been tested.**
The schema can reject a particular installation: the exact reviewed projected
columns must exist, names/types must validate, and unsupported native mechanics
remain blocked. This is a bounded projection, not exact whole-schema equivalence
or a full dependency closure.

Requires Python 3.11 or later. Pure tests and offline commands need only the
standard library. Live SQL support is optional.

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
distinguishes selected-key completeness from unproven source-loader order; no
generation, item admission or whole-table provenance is supplied.

Not captured: other event override/group/linking state; gameobjects; non-body loot;
conditions; other AI/script data; local starter outfits/DBC records; quest chains
beyond referenced fields; spell procs/stacking or the local client DBCs.
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
