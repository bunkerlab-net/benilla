# Raw quest relationship exclusion evidence

This optional **diagnostic-only** Python API extension is requested with
`capture(..., capture_quest_relationships=True)`. It is not wired into a private
compiler, gameplay consumer, CLI activation path or native admission. No database
connection or real content was used to test it. `runnable` and `capture_complete`
remain false. Legacy captures without the extension remain unproven; absence of
`quest_relationship_capture` must never mean an empty inbound relationship set.

## Source contract

CMaNGOS core is pinned to
[`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, ObjectMgr::LoadQuests](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Globals/ObjectMgr.cpp#L4789-L4876),
with ClassicDB pinned to `ec4f596146be6467ea93c57397858e329e2db852`.
The full quest map contributes incoming signed NextQuestId prerequisites,
NextQuestInChain reverse chains, and transitive breadcrumb dependencies. Explicit
PrevQuestId excludes breadcrumb predecessors, whereas incoming NextQuestId does
not. Missing-reference clearing and breadcrumb-cycle mutations differ by field.
This extension does **not** reproduce those loader transformations.

For exactly the captured quest IDs, preserve each selected row's entry,
PrevQuestId, NextQuestId, NextQuestInChain, ExclusiveGroup and
BreadcrumbForQuestId. Scan the entire quest table using precisely:

* NextQuestId in positive **or negative** selected IDs;
* NextQuestInChain in selected IDs;
* BreadcrumbForQuestId in selected IDs.

Other rows' PrevQuestId affects those other rows, not selected reverse stores;
it is deliberately not scanned. This is not a complete relationship graph.
Nonzero own ExclusiveGroup is already blocked; no global group-membership claim
is made. Nonzero own relationship fields retain `quest.chain`. Every matching
incoming field adds `quest.inbound_relationship`, even dangling, cyclic or
loader-ignored raw edges. There is no chain admission or recursive expansion.
Complete-empty direct incoming breadcrumbs imply absence of any incoming
transitive path (every path has a last direct edge). A present direct edge blocks
immediately; it does not establish the normalized dependent-breadcrumb list.

## Bounded evidence and limitations

Fixed allowlisted parameterized SQL selects existing projected columns with
`LIMIT MAX_ROWS+1`; values are never interpolated. Existing typed normalization,
duplicate rejection, per-row/table/aggregate limits and snapshot guards apply.
Only the six raw relationship fields survive into the separate sidecar; external
quests do not become selected quests or item/spell/giver dependencies. IDs and
relationship magnitudes outside 1..INT32_MAX (zero allowed for absent fields) are
unsupported; signed minimum is rejected rather than passed through absolute-value
arithmetic. Relationship columns must be non-null integer columns and signed
fields cannot have unsigned SQL types. This is a conservative raw numeric
contract, not a claim that every accepted SQL schema matches the pinned DB.

The sidecar binds exact roots/selectors, row limit, own/inbound rows and digests,
source pins, schema, selection, consistency mode, version-note and selected-quest
digests. Validator recomputes this contract and requires all diagnostic blockers.
A selected matching row cannot disappear from its own inbound evidence. Missing
schema, bad domains, overflow, duplicate/conflicting rows or interrupted reads
fail, never becoming complete-empty. Empty selected roots need no SQL scan.

Completeness is an assertion of the bounded read procedure under its existing
snapshot assumptions, **not cryptographic authentication**. Self-consistent
fabrication of external rows/evidence cannot be detected by recomputed hashes.
Removing this optional extension returns a legacy unknown capture, never an
admitted world. No private SQL, dump execution, native admission, quests usable
in normal gameplay, or Classic parity is established by these fake-source tests.
