# Captured quest relationships and bounded ordinary closure

The optional Python capture extension uses
`capture(..., capture_quest_relationships=True)` to preserve raw incoming evidence.
The native starter preparation calls `quest_relationships.resolve` on validated
evidence. A resolved closure supplies `quest_relationships.capture` for Rust's
`quest_lifecycle::Catalog::from_relationship_capture`. Neither path grants native
admission. Raw `runnable` and `capture_complete` remain false. Missing evidence
means unknown incoming coverage, never an empty set.

## Source contract

CMaNGOS core is pinned to
[`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, ObjectMgr::LoadQuests](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Globals/ObjectMgr.cpp#L4789-L4876),
with ClassicDB pinned to `ec4f596146be6467ea93c57397858e329e2db852`.
The full quest map contributes incoming signed NextQuestId prerequisites,
NextQuestInChain reverse chains, and transitive breadcrumb dependencies. Explicit
PrevQuestId excludes breadcrumb predecessors, whereas incoming NextQuestId does
not. Missing-reference clearing and breadcrumb-cycle mutations differ by field.
The raw capture preserves these fields without loader transformations.

For exactly the captured quest IDs, preserve each selected row's entry,
PrevQuestId, NextQuestId, NextQuestInChain, ExclusiveGroup and
BreadcrumbForQuestId. Scan the entire quest table using precisely:

* NextQuestId in positive **or negative** selected IDs;
* NextQuestInChain in selected IDs;
* BreadcrumbForQuestId in selected IDs.

Other rows' PrevQuestId affects those other rows, not selected reverse stores;
it is deliberately not scanned. The selected rows and their incoming scans can
establish a bounded closed ordinary graph, not a complete global graph.
Raw reports retain `quest.chain` and `quest.inbound_relationship` diagnostics.
Preparation replaces these two obligations only when the entire selected closure
resolves. It retains other eligibility, credit, reward, giver, and item obligations.

## Ordinary closure consumer

Every referenced quest must have its full selected row and its own incoming scan.
Preparation never promotes an external incoming row into playable content.
An external incoming row, dangling reference, conflict, missing scan, or closure
above 1,024 quests refuses the closure. Refusals identify the quest, field, signed
target, and reason. Nonzero exclusive groups, breadcrumbs, scripts, conditions,
and source-item or source-spell paths remain unsupported.

The preparation output retains the exact source digest, raw capture projection,
normalised signed prerequisites, and separate reverse-chain lists. Rust compares
the raw projection against every supplied gameplay row and checks selected
incoming edges before deriving `complete_relation_scan`. The existing catalog
constructs the authoritative prerequisite and reverse-chain stores.

Positive prerequisites require a rewarded predecessor. Negative prerequisites
require a current predecessor. These are alternatives, as in
[`Player::SatisfyQuestPreviousQuest`, Player.cpp:12906-13004](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Player.cpp#L12906-L13004).
`NextQuestInChain` selects a next offer at the same giver, not a prerequisite.
[`Player::GetNextQuest`, Player.cpp:12210-12240](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Player.cpp#L12210-L12240)
and the existing reward/dialog path retain that distinction.
[`Player::CanTakeQuest`, Player.cpp:12265-12273](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Player.cpp#L12265-L12273)
still gates acceptance.

Unlike the upstream loader, the bounded consumer refuses missing references
instead of clearing or ignoring them. Breadcrumbs refuse before their transitive
normalisation or cycle repair. Complete-empty direct incoming breadcrumb coverage
excludes any incoming transitive breadcrumb path.

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
