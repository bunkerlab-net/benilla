# Configured source-backed exploration starts

The app can create new exploration characters at their selected local `playercreateinfo`
position when `p2p.classic_start_captures` names a directory of private source-v2 captures.
No world rows, client assets, SQL connection, new capture command, subprocess, native
compiler, or gameplay admission is added. Unconfigured installations retain the legacy
Northshire policy and emit an explicit warning. Existing saved characters are never reset.

## Existing inputs and evidence

The approved read-only [source capture](CLASSIC-STARTER-CAPTURE.md) already captures
one selected `playercreateinfo` row with exact field types and per-table SHA-256.
Its existing Python `classic_export.capture.validate_bundle` validates the full
capture. The bounded Rust `benilla-world-data` world-export reader does not expose
`playercreateinfo`; a world export, a source-v1 capture, or a client DBC alone is
not sufficient input for racial creation.

The position mechanism is `Player::Create` and `ObjectMgr::LoadPlayerInfo` in
[CMaNGOS Classic 8ec338a](https://github.com/cmangos/mangos-classic/tree/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f).
Creation chooses an existing race/class row, sets its map, and relocates to its XYZ
and facing. Schema reference:
[`sql/base/mangos.sql`](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/sql/base/mangos.sql)
(SHA-256 `229660cdd6d108d822a491a70a59bdfb6141e61a60ac488d126bee3780e5e5fa`).

The separately pinned [Classic-DB ec4f5961](https://github.com/cmangos/classic-db/tree/ec4f596146be6467ea93c57397858e329e2db852)
full database `Full_DB/ClassicDB_1_12_1_z2815.sql.gz` has Git blob
`0a77f5230a3d5d6db968678203dfe3b30c34b8a9`, SHA-256
`4f92db520868ab4e566726f68b5b2e380ae781209beaf22237b4f7f04600d0c0`.
Its forty selected rows cover all eight playable races and their valid Classic
classes. Its Night Elf coordinates/facing and Undead coordinates differ from the
core base SQL. No file under that pin's `Updates/` references `playercreateinfo`.
Thus core defaults must not silently replace missing installed data. None of these
rows is bundled. The [no-bundled-world-content boundary](CLASSIC-WORLD-IMPORT.md)
and upstream copyright qualifications remain in force; GPL notices alone do not
establish permission to redistribute game data.

## Reader contract

`benilla_profile::starting_location::ClassicStart::read` consumes at most 64 MiB +
one byte. `from_json` enforces the same byte limit and serde's depth limit. It
requires source-v2, the reviewed core/database revision declarations, build 5875,
a selected starter, a supported snapshot mode, the source's explicit unverified
installation-provenance flag, and the non-runnable/incomplete report flags.

Only the position projection is interpreted. It requires exactly one row, the
selected valid Classic race/class, exact row fields, canonical finite decimal
strings, the original table checksum, and the profile's supported open-world
map/coordinate/zone/facing bounds. No class receives another class's row, including
classes of the same race. Numeric decimal strings round directly to Rust `f32`,
matching the profile's stored coordinate representation; no axis conversion,
height adjustment or facing reset is applied.

Other source tables, schema, dependency reports and their hashes are **not**
validated by this reader. It is not a substitute for the existing complete capture
validator, an authenticity check, or a canonical-location validator. Revision
strings describe reviewed compatibility, not proof of the user's installed DB
revision. A local owner can edit a capture and recompute its hashes, just as they
can edit their local profile. The exact-byte capture SHA-256 is retained in the
new `ClassicSourceV1` policy for correlation only.

`create_character_from_start(input, start)` validates the selected pair before any
profile mutation. It shares normal owner/ID/name/appearance/limit/durability checks.
It persists map, zone, position and orientation at creation. Reopen, selection,
archive restoration and checkpoints do not read or reinterpret the capture.
`NorthshireMeetingV1` remains readable and its saved characters stay where they
were. The existing first-login worldport and roster project the saved position;
the production entry path uses the same saved-pose projection exercised by app-free tests.

## Setup and user-visible behavior

1. Obtain your own compatible local Classic database and an existing SELECT-only
   account. Follow the existing [capture setup](../scripts/classic_export/README.md)
   with a v2 starter selection for each desired race/class. Use a quiescent snapshot
   where required. This feature never imports or executes a downloaded SQL dump.
2. Keep captures private under `benilla-config/classic-starts/`, with names
   `<race>-<class>.json`. IDs are the Classic numeric race/class IDs. For example,
   `6-11.json` must itself select Tauren Druid. The filename alone cannot override
   the selected pair inside the capture. One capture is needed per desired pair;
   forty captures cover all supported combinations. Run the existing
   `python3 scripts/classic-export.py validate <local-capture>` command for each.
3. In executable-adjacent `config.toml`, set `classic_start_captures =
   "benilla-config/classic-starts"` under `[p2p]`. Relative paths are anchored at
   the executable, never the shell working directory. Absolute paths also work.
   The existing Connection setup form exposes `p2p.classic_start_captures` as a
   text field; Save and restart applies changes. Clearing it removes the setting.
   There is no file-picker dialog, new producer, converter or runtime subprocess.
4. Create the corresponding character normally. Existing client-DBC appearance
   validation still runs. Only the selected file is opened, as a bounded regular
   local file; final symlinks/reparse points and special files are rejected. A
   configured missing, malformed, mismatched or unsupported capture produces the
   stock race/class-creation-disabled result and a local diagnostic identifying
   the numeric filename and setting to check. It does not fall back to Elwynn,
   mutate the profile, or close the lobby. Existing characters can still enter,
   including if every capture is missing. Fixing/replacing the selected capture
   can be retried without restarting; changing the configured directory requires
   a restart. An unrelated missing capture cannot block another pair's creation.
5. With no setting, new characters still use the legacy Northshire meeting point;
   a warning explains how to configure racial starts. Login help also identifies
   that default. This compatibility behavior is deliberate, not a claim that an
   unconfigured installation has canonical starts.

Exploration content version is now `benilla-saved-exploration-classic-starts-v4`,
so all peers must update; older peers cannot parse the new spawn policy. The new
policy is not portable back to older builds. Existing cross-map peer rejection is
unchanged: characters on different continents do not see each other, and this is
not cross-map membership support. Captures remain local; their paths and raw data
are never transmitted. The profile carries only the source digest and saved pose.

## Verification and limits

Authored tests cover all forty race/class combinations through the production app
selector, creation API, first-entry worldport projection and saved-pose recovery,
plus unconfigured legacy behavior, invalid/missing/mismatched inputs, source hash
and domain checks, executable-relative configuration and editor preservation.
They prove routing and persistence, not that authored positions are real world
locations. Only local captured source data determines actual starts.

No actual SQL connection, user-owned capture, game install, live create/login UI,
terrain rendering or cross-platform app run was verified here. The user still needs
their own compatible captures and client installation. The existing exporter may
reject their installed schema or dependency closure; that is not bypassed. Full
source-capture validation is separate from this narrow position projection. Native
NPCs, quests, spells, combat and progression remain outside this exploration feature.
