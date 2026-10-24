# Bounded initial hook capture

This is a source-evidence contract for the pinned CMaNGOS Classic revision
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. It grants no spell-execution or
native-world admission capability. No database or client content is included.

## Optional selected coverage

For a source-v2 starter selection, `plan --initial-hooks` requires the exact
`spell_area` and `spell_proc_event` schemas and primary keys. The corresponding
`capture --initial-hooks` reads both tables for every captured `spell_template`
ID in the same transaction or operator-confirmed quiescent snapshot as the other
source rows. Selectors must fit the unsigned MEDIUMINT key domain.

The capture preserves every selected raw matching row, including zero-valued,
conditional, and loader-rejected rows. It does not apply source loader filters,
truncate condition IDs, evaluate conditions, or inherit proc entries across
ranks. Schema/type/key mismatch, duplicate keys, or row-budget overflow rejects
the capture instead of producing a partial successful result. Parameterized
queries fetch at most the table budget plus one row to detect overflow.

`initial_hook_capture` uses `benilla-classic-initial-hook-selection-v1`. It binds
the selectors, table limits, snapshot mode, selection/schema/version hashes, and
the spell, rank, learning, area, and proc table hashes. Both hook tables and this
coverage record must be present together. A validated empty selected table proves
only that the selected raw query was empty. Missing legacy coverage is unknown,
not equivalent to an empty result. These hashes bind supplied evidence; they do
not authenticate an installation or prove the claims of an untrusted producer.

An empty direct proc selection is not proof that a spell has no inherited proc
behavior. `LoadSpellProcEvents` applies rank handling after reading raw rows.
The capture explicitly leaves rank and condition execution unresolved. Neither
this evidence nor the `status: complete` selected-query label permits effects,
scripts, rank behavior, or gameplay to be silently admitted.

## DurationIndex0 and the initializer

The preparation producer retains an actual `SpellDuration` row zero when it
exists, including all three signed duration words and its raw DBC binding.
An absent row stays absent in preparation evidence. Index zero must not silently
discard a real row or manufacture a duration value.

At the pin, `GetSpellDuration` returns zero for a missing lookup, preserves -1,
and otherwise takes the absolute base duration. `SpellAuraHolder` marks a holder
permanent when its calculated duration is -1 or when the spell is passive and
has DurationIndex0. The bounded initializer admits resolved -1 and the narrower
passive/index-0/resolved-0 case; other values, unresolved duration, active zero
duration, and nonzero-index zero duration remain unsupported. Source permanence
for other passive row-zero values is not a claim of support for those inputs.

The caller must establish the resolved duration and every other required initial
cast input. See [initializer scope and policy compatibility](../../crates/benilla-sim/STARTER-INIT.md).

## Pinned references

- [Raw SQL schemas and keys](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/sql/base/mangos.sql)
- [SpellMgr.cpp: GetSpellDuration, LoadSpellAreas, LoadSpellProcEvents](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/SpellMgr.cpp)
- [SpellAuras.cpp: SpellAuraHolder constructor](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/SpellAuras.cpp)

The producer and initializer are GPL-3.0-or-later; CMaNGOS references are
GPL-2.0-or-later. Existing provenance and license notices apply.
