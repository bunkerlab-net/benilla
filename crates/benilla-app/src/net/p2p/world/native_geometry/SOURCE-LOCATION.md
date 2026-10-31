# Source-owned zone and area observations

The separately selected `SourceLocationProvider` owns a complete immutable
`TerrainAreaSnapshot`. It answers runtime-issued `Query::SourceLocation` through
`TerrainInfo::GetAreaFlag`, complete indexed DBC resolution, and
`TerrainManager::GetZoneAndAreaIdByAreaFlag` from pinned
[CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f](https://github.com/cmangos/mangos-classic/tree/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f).
The source functions are `GridMap.cpp:1049–1052,1415–1421`; the underlying reader's
source hashes and license notices are in [product provenance](products/PROVENANCE.md).

## Observation boundary

The reader retains its actual decoded terrain/VMap inventory and all three complete
AreaTable, WMOAreaTable and Map WDBC inputs. A distinct byte-derived runtime catalog
is decoded from those same bytes. Snapshot selection binds map, content, source epoch,
inventory, geometry policy and catalog identity; the provider compares the complete
catalog as well as the binding. The session freezes the selected profile, player,
body and all local transport capabilities. Queries carry the runtime's world, player,
profile/source binding, tick/action identity and exact binary32 pose. Every fact echoes
that query; both transport and runtime validate the row against the selected catalog.

`GetZoneAndAreaId` uses the resolved row's ID and its nonzero zone, otherwise its ID
again. It does not load or require a parent row. A complete-store miss produces scalar
zero/zero while retaining `None`; a real row with ID zero remains `Some(row)`, including
its nonzero zone if present. Final resolution preserves ascending indexed IDs, the first
same-map flag match, the last wrong-map match, linked-zone fallback and uint16 narrowing.

Missing inventoried terrain, missing/unsupported products, malformed input, wrong
binding, an unloaded neighbor or a deadline failure never becomes a successful zero-area
observation. The same absolute work deadline is checked before, during and after the
read. Mailbox retries retain the original deadline and ticket; wrong query kinds, stale
sessions and changed query echoes fail closed.

The liquid-status provider cannot supply this observation: its source branches may
skip area lookup. Zone/area observations also do not establish `spell_area` closure.
The runtime separately checks the selected source rows for the observed zone/area and
spell scope. No assertion that the entire database has no `spell_area` rows follows.

## Verification and limits

Authored-byte tests exercise loaded terrain/VMap/DBC ownership, area 9/zone 12 without
a parent row, real ID zero versus missing row, full catalog/source changes, player and
session changes, query/pose echoes, unavailable neighbors and original-deadline mailbox
behavior. The existing pinned original-source area oracle now also calls the original
`GetZoneAndAreaId` for its recorded rows. Its Rust comparison checks location results for
all 25,440 supported loaded-byte rows; the 3,024 packed/absent-height source-only rows
remain outside the admitted reader profile. These are numeric authored fixtures, not
captured installation or gameplay evidence.

`install_with_source_location` installs only this explicit provider. It does not create
participants, source provenance, spell source completeness or native gameplay admission.
Normal admission/reopen remains closed; actual gameplay still requires the user's
missing private assets and all independent source/participant gates.
