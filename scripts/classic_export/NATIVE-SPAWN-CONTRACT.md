# Bounded native spawn dependency capture

This is typed source evidence, not native initialization or a runnable export.
New selected-starter v2 captures include seven full projected tables; older v2
captures without all seven and `native_spawn_capture` remain valid but unproven.
Any partial combination, or tables without their coverage object, is rejected.
The source format is unchanged; the new optional evidence object has its own
`benilla-classic-native-spawn-selection-v1` version.

## Exact selectors and snapshot

Each query selects all matching rows without event, pool, aura, chance, enabled,
nonzero or other loader filters. Numeric values are bound parameters. The table,
column, order and predicate shape are internal allowlisted constants.

| Table | Complete selector set |
| --- | --- |
| `creature_spawn_data` | `Guid` in every selected creature GUID |
| `creature_spawn_data_template` | `Entry` in every referenced `Id`, plus 0 always |
| `game_event_creature` | `guid` in every selected creature GUID |
| `pool_creature` | `guid` in every selected creature GUID |
| `pool_creature_template` | `id` in every captured current creature-template Entry |
| `creature_addon` | `guid` in every selected creature GUID |
| `creature_template_addon` | `entry` in every captured current creature-template Entry |

Selectors are sorted, unique and recomputed from source rows. An empty selector
set yields complete-empty without an invalid `IN ()` query. The Entry 0 default
is queried even in an empty region. These rows are read within the same
read-only transaction or operator-prepared quiescent snapshot as the selected
creature rows. Version notes are checked at both ends and rollback runs even on
failure. MyISAM still requires explicit quiescent-snapshot acknowledgement; an
InnoDB transaction alone does not protect MyISAM. Metadata is inspected first.
No database, credentials or native assets were used to test this implementation.

The evidence binds the consistency mode, selection hash, inspected schema hash,
version-notes hash and canonical hashes of both selector-root tables and all
seven dependency tables. Validation recomputes all evidence and checks each row
belongs to its selector. Source bundle hashes provide integrity, not authenticity
or proof that an externally forged bundle came from a database. Consumers must
validate the original source and recompute preparation; copying a hash alone
cannot establish absence or gameplay support.

## Schema, identity and budgets

Full column names, SQL families, signedness, nullability and ordinal positions
match the pinned [CMaNGOS base schema](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/sql/base/mangos.sql).
Integer display widths may vary. Extra or missing columns fail for these tables.
Key identity is GUID/Entry except event membership, which uses `(guid,event)`.
The event field remains signed smallint, including negative and zero rows. The
spawn-template Entry 0 key is allowed. Its native SQL composite primary key
permits repeated Entry values, but the native loader overwrites by Entry without
ordered selection, so capture conservatively rejects duplicate Entry values.

The spawn template preserves all twelve columns: Entry, NpcFlags, UnitFlags,
Faction, ModelId, EquipmentId, CurHealth, CurMana, SpawnFlags, RelayId, StringId
and Name. NpcFlags, UnitFlags and EquipmentId preserve -1 sentinels. Other negative
values are unsupported and rejected; UnitFlags above uint32 are rejected rather
than reproducing implicit truncation. Addons preserve all seven columns and keep
nullable, unparsed aura text. The two pool tables also preserve chance and
description. A zero-valued hook row remains a row.

Every positive referenced spawn-template ID must exist. ModelId references add
exact creature-model rows to the capture closure only when their spawn template
is actually selected for a GUID. All nonzero template DisplayId1..4 roots and
recursive nonzero modelid_other_gender references are captured; cycles terminate
at the visited set and the 4096-model budget. An unused Entry 0 model remains raw
context, not an applicable override. Nonzero modelid_alternative is preserved as
unsupported context without guessing its applicability or following that edge. Missing explicit
references fail rather than silently falling back to zero. An absent Entry 0 is
recorded as captured absence, and preparation blocks a spawn that needs it.
Equipment, faction, relay, string and addon aura semantics are not resolved by
this capture. Raw values remain blocked for gameplay until a supported consumer
has the required inputs and native behavior.

The existing limits apply: 4,096 rows/selectors per table, `LIMIT cap+1` overflow
rejection, 64 KiB per row, 16 MiB per table/query, 64 MiB aggregate fetched bytes
and source bundle. Default Entry 0 counts toward the selector budget. Text has
its existing 16 KiB bound. All queries count toward the same aggregate budget.
Metadata table enumeration is sized from the finite allowlist, so adding these
tables cannot silently exceed the previous fixed inventory limit.

## Preparation and remaining requirements

`native_spawn_capture: null` means unknown legacy coverage. For a covered current
entry, `native_spawn_dependencies` gives one record per selected static GUID.
Each table has `status: complete` plus actual rows, including an empty array when
absence was observed. The selected spawn template is a real row or an explicit
`captured_row_absent`; no neutral template is fabricated.

[ObjectMgr::LoadCreatures](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Globals/ObjectMgr.cpp#L1880-L2027)
selects Entry 0 unless a positive binding selects an explicit template.
[Creature::GetCreatureAddon](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Creature.cpp#L2295-L2353)
uses a GUID addon first, otherwise the current-entry addon. Even a neutral GUID
row shadows an entry addon. Preparation preserves both rows while recording
which one is applicable; it neither parses nor executes auras.

All captured event/pool rows and applicable addon rows remain explicit
unimplemented-hook requirements. `unimplemented_hook_tables: []` proves only
that none of these named hooks matched. The evidence explicitly states
`other_native_hooks_proven: false`. It does not cover event creature-data
overrides, entry substitutions, groups/linking, assistance, scripts or other
runtime context. Spawn-template overrides are raw inputs awaiting native
resolution. Resolved combat stats, AI profiles and runtime export remain null.
