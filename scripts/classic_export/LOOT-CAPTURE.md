# Selected body/reference loot capture

`classic-export.py plan --body-loot` and `capture --body-loot` opt a source-v2
starter selection into `benilla-classic-body-loot-selection-v1` evidence. The
reader follows every already-selected creature template's positive `LootId`,
including quest/objective/kill-credit templates. It does not select by creature
entry, query entry zero, or special-case one creature. Old captures without this
option retain their existing missing-loot blocker. This option changes no native
runtime format and emits no profile or `WholeTableLoaderResult`.

## Source and schema contract

The source pin is CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`; the database revision claim is
ClassicDB `ec4f596146be6467ea93c57397858e329e2db852`. These pins are adapter
identities, not proof of an installed database. Relevant source:

- [LootMgr.cpp:134–185](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Loot/LootMgr.cpp#L134-L185)
  loads whole tables without ORDER BY, converts fields, validates and assigns
  accepted-row positions. SHA-256 of that source file:
  `7ffc34395011773310f944cfd7f1c59c23ae3043d0c8634d7665c0bbb5752cce`
- [LootMgr.cpp:309–365](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Loot/LootMgr.cpp#L309-L365)
  distinguishes positive leaf counts from negative reference IDs; a reference's
  placeholder item is not an item-prototype dependency
- [mangos.sql:1052–1063](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/sql/base/mangos.sql#L1052-L1063)
  and [10306–10317](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/sql/base/mangos.sql#L10306-L10317)
  define the two tables. SHA-256 of that source file:
  `229660cdd6d108d822a491a70a59bdfb6141e61a60ac488d126bee3780e5e5fa`

Both loot schemas must match the eight reviewed columns, ordinal order,
nullability and primary key `(entry,item)`: unsigned MEDIUMINT entry/item and
condition_id, signed MEDIUMINT mincountOrRef, FLOAT ChanceOrQuestChance,
unsigned TINYINT groupid/maxcount, nullable VARCHAR(300) comments. Integer
display widths alone may differ. Current metadata is inspected, never inferred
from source or content uniqueness. A present malformed table fails; an absent
loot table stays unknown. Present MyISAM tables require the existing explicit
operator-quiescent snapshot mode.

All fields stay raw, including negative references, nullable comments and
condition IDs above the loader's uint16 domain. Tagged decimal values preserve
the driver's numeric rendering; they are not measured CMaNGOS float conversion
or original SQL binary bytes. Placeholder item zero is allowed by the raw SQL
key domain; positive-count item zero is retained with an unsupported blocker,
never silently converted to an item or queried as one.

## Bounded enumeration, not execution

The existing fixed SELECT mechanism queries body roots first, then each sorted
frontier of previously unseen reference IDs. Negative mincountOrRef selects
`-mincountOrRef` in reference_loot_template. Positive counts contribute their
positive item ID to the existing quest/starter/explicit-item union and existing
item-to-spell reader. Zero counts remain raw and unsupported. Shared roots,
shared references, grouped children and missing targets do not disappear.

Visited IDs terminate chains and cycles without an arbitrary depth cutoff.
Iterative strongly connected components identify every cyclic component;
nested/grouped references, cycles, unresolved conditions, unsupported quest-leaf
shapes and unsupported count/group shapes remain semantic blockers. Enumeration neither
repairs rows as the native loader may nor rolls probabilities or generates loot.

An ungrouped negative-chance body leaf with positive valid counts and no condition
has an interpreted quest-only shape. Preparation retains its absolute chance and
count bounds in `quest_loot_inputs`, with explicit quest-drop-rate and active
incomplete owner-need policies. It does not turn the row into ordinary or guaranteed
loot. Grouped leaves, negative reference chances and quest leaves inside reference
tables retain `loot.quest_leaf_shape_unsupported`. Whole-loader order,
generation/item admission, unknown quest state and group ownership remain separate
requirements. This interpretation removes only `loot.quest_chance_not_interpreted`
for supported body leaves and never manufactures native admission.


Existing limits remain: 4,096 rows per selected loot table, 4,096 cumulative
reference IDs, 2,048 total item IDs, 512 spells, 64 KiB per row, 16 MiB per
query/table, 64 MiB aggregate responses/bundle and 2 MiB schema. Repeated
reference batches share the row/table-byte budget. Every productive frontier
visits at least one new ID; repeated edges never cause repeat reads. LIMIT N+1
detects overflow, which fails the entire capture without partial output.
Missing required item/spell rows continue to fail the existing closure checks.
This is deliberate: no missing leaf is presented as an admitted complete item.

## Evidence and offline readback

`loot_capture` binds the body-only request, all creature origins with LootId,
MinLootGold/MaxLootGold and separate non-body selectors, roots, raw reference
edges, leaf IDs and cyclic components. Each deterministic query batch records
its mode, requested IDs, SQL-plus-parameters hash, returned keys, normalized-row
hash, missing IDs and completion state. Snapshot evidence binds consistency,
selection, schema, version notes, revision claims and creature/loot/item/spell
hashes. No connection secret is recorded.

- Missing table: unknown, no performed query or row hash
- Present table and empty derived selector: not_needed, no SQL query
- Performed empty query for positive IDs: missing, with observed empty result
- Complete selected queries: complete rows for those requested selectors only

`selected_rows_complete` describes query/dependency enumeration, independently
of unsupported semantics. Even a fully enumerated cycle can have complete
selected rows while remaining non-executable. `query_order` is always
`entry_item_key_order`; `loader_order` is always `unproven`. No whole-table count,
loader positions, result fingerprint or global completeness witness is emitted.
All selected leaf rows are required by offline readback.

Readback reconstructs roots, frontiers, edges, leaves and cycles from the typed
rows and compares the canonical evidence. Optional loot tables require evidence;
evidence requires coverage for every available loot table and a v2 selection.
Unselected rows, changed query origins/statuses, removed dependencies and stale
snapshot hashes reject even after outer hashes are recalculated. Hashes are
integrity checks, not signatures: a coherent rewrite of all source rows and all
evidence cannot independently prove what a real SQL server returned. Actual
source/final-installation validation remains a separate prerequisite.

PickpocketLootId and SkinningLootId stay explicitly unknown/outside body scope;
zero body LootId is an explicit no-template reference. Gold fields are retained,
not interpreted. Body capture replaces the broad not-captured blocker with
specific source-order, generation, item-admission, missing-template and
non-body blockers. `revision_claims_verified_against_installation`,
`report.runnable` and global `report.capture_complete` remain false.

## Candidate boundary and separate gates

A read-only local archive audit, without running SQL, observed seven base rows
for Vermin LootId 6 and five/23 leaves under references 60441/60000. The known
update 4562 deletion of item 2772 implies an expected six-row/32-leaf slice after
that update. It does not establish a fully applied final database. A base capture
must preserve its seventh row; this reader contains no hard-coded deletion or
candidate ID list. No populated database or public loot data is bundled here.

The candidate has no nested references, gold 1–4 and a separate nonzero
PickpocketLootId 6. All 32 expected leaf items have nonzero spell cooldown metadata,
often unused-slot -1 sentinels. Capture retains it unchanged. Five bag candidates
and food/water spell dependencies do not establish bag capacity, eating/drinking,
equipment use, corpse storage readiness or item admission. Those and reference
loot generation/source-loader ordering are independent runtime follow-ons.

Authored fake-source tests prove reader behavior only. No live SQL connection,
materialization, content redistribution, new compiler/native-profile producer,
client DBC/terrain evidence or production-admission/cache reopening is supplied.
The existing native loader-order guard and item/quest/replay guards are unchanged.
