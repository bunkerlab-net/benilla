# Selected spell and aura registry capture

New selected-starter source-v2 captures include `spell_scripts` and the optional,
versioned `spell_script_capture` object. Legacy v2 captures without both remain
valid with **unknown** coverage. A table without evidence or evidence without its
table is rejected. No existing capture is upgraded by adding an empty array.

## Exact current registry schema

The pinned [ScriptDev2 spell registry DDL](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/sql/scriptdev2/spell.sql#L2-L5)
defines ordinal 1 `Id INT NOT NULL` (signed), ordinal 2
`ScriptName CHAR(64) NOT NULL`, and primary key `Id`. Integer display widths do
not affect the domain. Inventory includes `primary_key: ["Id"]`, inspected with
a fixed `information_schema.KEY_COLUMN_USAGE` read. Missing/different key,
wrong column order, unsigned Id, changed text width/type, nullable or additional
columns fail. The historical command/delay `spell_scripts` table is not this
registry. The actual engine must satisfy the same read-only transaction or
operator-quiescent snapshot policy as every other captured table; no automatic
locks, server changes or driver installation are performed.

## Selection, signed domain and bounds

After the selected spell-template closure stabilizes, select `Id, ScriptName`
for every captured `spell_template.Id`, sorted and deduplicated. Queries use
allowlisted identifiers and bound numeric parameters, `ORDER BY Id`, and
`LIMIT 513` for a 512-row cap. There are no name, passive, effect, known-handler
or successful-loader filters. Empty and unknown names stay exact present rows.
An empty selector produces complete-empty without issuing `IN ()`. There is no
global registry-presence probe; the separate `spell_learn_spell` branch probe is
unrelated.

This coverage version accepts only IDs 1 through 2147483647. Validate the entire
selector **before the registry query**. Pinned
[Field::GetUInt32](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/shared/Database/Field.h#L59)
casts a signed SQL value to uint32: SQL `Id=-1` can register numeric ID
4294967295. A positive-only query for that number would falsely prove absence.
Zero and the wrapped-negative half therefore reject with
`spell_registry.selector_domain`, never complete-empty. Future uint32 support
would require a new signed-selector normalization contract, preserving raw SQL
keys and tests for -2147483648/-1; this version does not implement it.

The normal row/text/query/table/aggregate byte limits still apply. Duplicate keys,
truncation or out-of-selector driver rows reject. Every registry read happens
inside the spell rows' snapshot and counts against its budgets. DBC-discovered
spell dependencies still require the existing exact-ID local recapture flow.
Evidence covers all captured spells; it cannot cover an undiscovered or missing
source spell merely because its ID later appears in a DBC.

## Evidence and preparation API

`spell_script_capture` contains:

- `format: benilla-classic-spell-script-selection-v1`
- `selector: {column: Id, ids: [...]}` for all captured spell-template IDs
- `filter: all_matching_rows_no_loader_filters`, `row_limit: 512`
- `scope: spell_and_aura_numeric_registry_for_selected_spells`
- `snapshot`: source consistency, selection/schema/version-note SHA-256s and
  canonical hashes for both `spell_template` and `spell_scripts`

Validation recomputes the evidence. Hashes establish integrity and lineage, not
signatures, source authenticity or a gameplay capability. Preparation carries the
validated object unchanged and each `spells[].script_registry` is either null for
legacy unknown coverage, or `{status: complete, rows: [...], coverage_sha256}`.
The digest binds the original evidence; `rows` contains every actual matching row.

The pinned [numeric maps and loader](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/Scripts/SpellScript.cpp#L22-L106)
start empty. Built-in registration populates name maps; only the reviewed SQL
loader pairs numeric IDs with names. Numeric setters are
[private](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/Scripts/SpellScript.h#L146-L177).
Thus complete selected absence supports a null SpellScript **and** AuraScript
pointer for a fresh load of the pinned core. Present rows, including empty or
unknown names, require named-resolution/handler semantics rather than a guessed
no-hook value. This closes only this registry: actual initial-cast paths, area
conditions, procs and other semantics remain separate consumer requirements.
Preparation stays diagnostic, blocked and non-runnable.
