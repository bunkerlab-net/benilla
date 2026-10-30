# Raw creature entry-alternative exclusion evidence

The optional Python API `capture(..., capture_spawn_entries=True)` records
`creature_spawn_entry` rows for every selected creature GUID. It is diagnostic
source evidence, not a native compiler, runtime selector or gameplay capability.
It does not add a CLI activation path. Legacy captures without this extension
remain unknown, not complete-empty. `runnable` and `capture_complete` stay false.

## Source boundary

The pinned [ObjectMgr::LoadCreatureSpawnEntry and LoadCreatures](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Globals/ObjectMgr.cpp#L1826-L1948)
load alternative entries by GUID, skip nonexistent templates, retain unspecified
query order and use alternatives for unresolved zero-entry spawns. The loader can
also derive active-GUID state from alternative templates. [Creature::LoadFromDB](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Creature.cpp#L1612-L1658)
has distinct forced/static, group, conditional, random and event fallback owners.
The pinned [Classic DB shared-spawn update](https://github.com/cmangos/classic-db/blob/ec4f596146be6467ea93c57397858e329e2db852/Updates/4720_TDB-0543_stonetalon_shared_spawn.sql)
uses alternative rows and zeroes the static entry; a static-entry join alone is
not a complete spawn identity proof.

Capture preserves raw alternatives, including zero entries and rows for
positive-entry selected GUIDs. It does not establish whether a particular row is
reachable after native loader normalization, whether its template exists, or
which alternative wins. Each retained row produces
`spawn.raw_entry_alternative_unimplemented`. A selected zero-entry spawn remains
blocked as `spawn.entry_substitution_unresolved`, even with a complete-empty
scan. Without the extension its existing `spawn.entry_substitution_not_captured`
blocker remains. No alternative templates or their dependencies are promoted.
Groups, conditional entries, event overrides, linking and other hooks remain
unproved; this extension does not alter the seven-table native spawn contract.

## Read and evidence contract

The exact pinned schema is unsigned `int guid`, unsigned `mediumint entry`, both
non-null, in that order, with primary key `(guid, entry)`. Integer display widths
may vary, not SQL family, signedness, nullability, column order or primary key.
Views, extra/missing columns and missing requested tables fail. Actual schema
metadata is inspected; duplicate-result rejection is not a substitute for keys.

The single allowlisted query binds sorted unique selected GUIDs to `guid IN (...)`,
orders retained evidence by `(guid, entry)` and requests `LIMIT MAX_ROWS+1`.
It applies no loader/nonzero/template-validity filters. Empty selected roots
require the schema but no invalid `IN ()` query. Maximum selected roots and
returned rows are each 4,096; existing 64 KiB row, 16 MiB table and 64 MiB aggregate
read/bundle limits apply. Overflow and interruption fail instead of truncating.
All reads share the existing read-only snapshot; MyISAM requires explicit
operator-quiescent acknowledgement, and version notes must match at both ends.

The paired table and `spawn_entry_capture` object bind source pins, exact roots
and selector, schema, selection, consistency mode, version notes, selected
creature rows and alternative-row hashes. Offline validation recomputes this
contract and the required blockers. A table without its evidence or evidence
without its table is rejected. Missing legacy coverage is not upgraded.

Hashes establish internal integrity, not authentication of the external source.
A coherently forged table, evidence and report cannot be detected from hashes
alone. Canonical sorting is not proof of native query/RNG order. No real database,
assets, native admission or playable Classic behavior is established by the
authored fake-source tests.
