# Selected EventAI text dependencies

New source-v2 captures retain all 25 `creature_ai_scripts` columns, then capture
positive ACTION_T_TEXT (type 1) references from all three parameters of all three
action slots. Entry and negative-GUID selection remains unfiltered: chance,
phase, flags, event type, unknown actions and zero-valued parameters are retained.
This is a raw reader, not an EventAI interpreter, content compiler or profile
issuer. Runtime execution remains unimplemented; `runnable`, `capture_complete`
and `revision_claims_verified_against_installation` remain false.

## Reviewed source and schemas

Core pin: `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`.
ClassicDB pin: `ec4f596146be6467ea93c57397858e329e2db852`.

- [EventAI loader](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/AI/EventAI/CreatureEventAIMgr.cpp#L210-L269)
  reads full event/action fields; its [text validation](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/AI/EventAI/CreatureEventAIMgr.cpp#L594-L614)
  can clear missing text IDs. This reader preserves originals instead
- [Broadcast loading](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Globals/ObjectMgr.cpp#L7782-L7880)
  reads base text and locale rows. Raw fields unused by that loader, including
  `ConditionID`, `EmotesID`, `Flags`, sound 2 and `VerifiedBuild`, also survive
- The pinned ClassicDB full dump defines signed INT `id`, signed INT
  `creature_id`, phase mask, six event parameters and all nine action parameters;
  unsigned TINYINT event/action types; unsigned INT chance/flags; and nonnull
  VARCHAR(255) comment. Primary key is `id`
- The pinned [core base schema](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/sql/base/mangos.sql#L784-L812)
  is an explicitly supported second variant: only EventAI `id` is unsigned INT.
  Other signedness changes, hybrid columns, widths, order and nullability reject.
  SQL integer display widths do not alter value domains. Actual raw schema,
  signedness and corresponding value bounds remain intact
- Broadcast base contains all 17 columns from the dump: signed INT fields except
  nullable TEXT `Text` and `Text1`. Locales contain signed INT `Id` and
  `VerifiedBuild`, nonnull VARCHAR(10) `Locale`, and nullable TEXT `Text_lang` and
  `Text1_lang`. Keys are respectively `Id` and `(Id, Locale)`
- Locale identifier aliases are explicitly limited to `Id`/`ID` and
  `Locale`/`locale`, matching the dump and the pinned
  [locale SQL](https://github.com/cmangos/classic-db/blob/ec4f596146be6467ea93c57397858e329e2db852/locales/BroadcastTextLocales.sql).
  Queries use inspected spellings with canonical output aliases. Arbitrary case
  coercion, duplicate columns and mismatched primary-key spelling reject

The final candidate's schema comes from the full-dump stage of the pinned
[installer](https://github.com/cmangos/classic-db/blob/ec4f596146be6467ea93c57397858e329e2db852/InstallFullDB.sh#L1522-L1580),
which replaces any earlier core base table. The reviewed default sequence does
not subsequently alter these three schemas. The later
[ACID stage](https://github.com/cmangos/classic-db/blob/ec4f596146be6467ea93c57397858e329e2db852/ACID/acid_classic.sql#L26)
truncates/repopulates EventAI rows, and the locale stage replaces localized rows;
neither changes these column types. Therefore the expected candidate EventAI ID
is signed INT. This is literal source-sequence review, not proof of an installed
database or permission to execute the general installer.

## Evidence and incomplete inputs

`event_ai_capture` retains entry/GUID selector coverage. The additional
`event_text_capture` independently records:

- Positive references with event ID, signed creature selector, 1-based action
  index, 1-based parameter index and broadcast ID; repeated references stay
  separate even when the query deduplicates their IDs
- Signed negative text parameters as unresolved inputs, never substituted IDs;
  other nonzero action types as dependencies outside this reader's text scope
- Per-table requested IDs, returned composite keys, missing base IDs, row hash,
  inspected column mapping, fixed query/parameter hash, row limit and whether a
  query was performed
- Selection/schema/version/owner/event/dependency hashes and snapshot mode

A queried base row missing for any reference is `missing`, including when a
locale row for that ID exists. A table absent from the schema inventory is
`unknown`, including for zero references. A present table cannot be omitted from
the captured tables or forged as empty. A fully queried empty locale result is
`complete` fallback evidence. With zero references, present validated tables
have complete empty selections without issuing invalid SQL `IN ()` queries.
Negative text parameters leave the overall positive-text dependency report
unknown. `complete` concerns this positive ACTION_T_TEXT dependency scope only;
it never establishes other action dependencies, loader disposition, speech,
locale fallback execution, RNG, audience selection or runtime admission.

Legacy source-v1 and v2 bundles without EventAI rows remain unproven. ID-only
EventAI projections cannot be promoted offline to full event evidence: recapture
is required. A consumer must not infer execution support from captured rows.

## Bounds, validation and tests

New queries are fixed parameterized SELECT modes, ordered by reviewed keys, with
at most 4,096 distinct positive signed-INT IDs and 4,096 rows per table. They use
limit+1 overflow detection and the existing text (16 KiB), row (64 KiB), table
(16 MiB), and aggregate response/bundle (64 MiB) byte limits. No truncation,
whitespace/Unicode normalization or NULL-to-empty conversion is performed.
Duplicate keys, including conflicting duplicates, reject rather than coalesce.

EventAI, bases and locales share the existing read-only snapshot and version
before/after check. MyISAM requires the operator-quiescent contract; a
transaction alone is insufficient. Every failed capture finishes the read.

Bundle reading revalidates raw schemas, rows, key order, bounds, hashes and
snapshot consistency, then derives the references and fixed queries again.
Unselected/orphan rows, independently removed tables, dropped provenance,
truncated evidence, altered query metadata and unsupported completion claims
reject even after outer hashes are recomputed. Semantic orphan tests also
recompute all evidence hashes before rejection. These hashes are integrity
checks, not signatures or independent proof that a database answered honestly;
a fully fabricated but internally consistent source cannot be authenticated by
public checksums.

`python3 scripts/test-classic-export.py -v` exercises this contract with small
original synthetic rows. The tests reproduce the reviewed shape with different
IDs and authored strings, not imported world content. Real final-database
materialization, loader diagnostics and capture remain separate unverified
steps. No client assets, production admission or loaded-cache reopening is
established by these tests.
