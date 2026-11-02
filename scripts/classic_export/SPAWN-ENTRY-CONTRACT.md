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

## Selected ordinary spawn context

The optional Python API `capture(..., capture_spawn_context=True)` also selects
the existing entry-alternative extension and captures complete selected lookups
for `creature_conditional_spawn`, `spawn_group_spawn`, `creature_linking`,
`creature_linking_template` and `game_event_creature_data`. All reads use the same
snapshot and existing row/byte budgets. Exact pinned column types, order,
nullability and primary keys are required. Linking queries inspect both slave
and master identities, including masters outside the selected region. Template
linking queries include every map, conservatively refusing even other-map rows.
Group membership is captured without filtering by group type, slot or chance.
Every membership refuses, so neither a missing group definition nor a formation
loader correction can turn it into a supported standalone spawn.

`spawn_context_capture` binds the selected roots, OR selectors, source pins,
schema, snapshot and row hashes. Legacy absence remains unknown. Orphan tables,
missing paired coverage and changed bindings fail offline validation. The
extension does not inspect server settings, authenticate source bytes, or prove
map load order or assistance traversal.

Preparation derives a per-entry `spawn_context_plan`. Supported positive static
spawns have mask 1, a captured effective spawn-template row, no alternatives,
conditional ownership, groups, linking, pools or game events. Event-data rows
also refuse when the spawn has no event membership. Negative event IDs and
neutral-looking zero-valued membership rows are not discarded. Any alternative
row refuses, including a zero alternative. Spawn relay/string, hover/gravity,
unsupported addon effects and respawn NPC-flag overrides remain blocked.

`CapturedSpawnContext` independently reconstructs the Rust loader projection.
It binds DB coordinates and orientation to the existing NPC/default movement
owners, and normalized respawn endpoints/effective spawn template to the
existing generation owner. Reversed respawn bounds clamp to the minimum as in
Core. Idle spawns discard their radius. Random movement with zero radius becomes
Idle. Negative random radius repairs refuse because Core retains a zero-radius
random generator in that branch, outside the runtime's supported radius domain.
Positive random radius uses the existing Wander owner, including radius 5.
The existing runtime limit is 150 yards. Waypoint/external movement refuses.
The generation binder independently validates all model, equipment, addon and
override dependencies; the loader plan alone does not satisfy them.

Missing default template zero is still a precise refusal when actually selected.
The pinned loader does not synthesize a default. A present explicit template
can supply the effective row without template zero. Captured absence must not
be replaced by an authored all-zero row.

The scoped plan replaces only supported spawn/movement and deterministic
respawn-kernel obligations. `creature.cold_loader_order_required` and
`creature.assistance_runtime_traversal_required` remain explicit. Empty group or
linking tables do not disable assistance. Its existing source owner still needs
complete live traversal and its other callback exclusions. Loaded Wander/Home
geometry and cache reconstruction retain their independent contracts.

Pinned Core mechanisms:

- [ObjectMgr spawn templates, alternatives and LoadCreatures](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Globals/ObjectMgr.cpp#L1758-L2070).
- [LoadSpawnGroups and formation/membership loading](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Globals/ObjectMgr.cpp#L1120-L1353).
- [Creature::LoadFromDB](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Creature.cpp#L1613-L1746).
- [SetDeathState and Respawn](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Creature.cpp#L1864-L1959),
  [SetRespawnDelay](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Creature.h#L805-L807)
  and [GetRandomRespawnTime](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Creature.h#L313).
  Scripted delay overrides remain excluded. The existing shared sampler owns
  initial/death delay draws, corpse deadlines and per-life reinitialization.
- [RandomMovementGenerator](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/MotionGenerators/RandomMovementGenerator.cpp#L83-L181).
- [CreatureLinkingMgr::LoadFromDB](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/CreatureLinkingMgr.cpp#L64-L162).
- [PoolManager creature loaders](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Pools/PoolManager.cpp#L654-L765)
  and [GameEventMgr membership](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/GameEvents/GameEventMgr.cpp#L236-L300)
  and [creature-data loaders](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/GameEvents/GameEventMgr.cpp#L409-L488).

These ports contain no Classic DB rows. The Classic DB pin remains
`ec4f596146be6467ea93c57397858e329e2db852`. All authored acceptance tests are
synthetic. No real private bundle, Northshire geometry or production admission
is established.
