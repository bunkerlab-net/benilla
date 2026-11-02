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
target, and reason. Nonzero exclusive groups, breadcrumbs, start scripts, quest
conditions, and source-spell paths remain unsupported. Source items and completion
scripts require the checked mechanics described below.

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

## Captured ordinary givers and presentation

For source-v2, `capture --quest-presentation` also captures incoming relationships.
The optional `quest_giver_presentation_capture` extension preserves complete
selected quest rows, including both request-emote delays, without changing legacy rows.
It captures greeting rows, quest and greeting locales, spawn-keyed `npc_gossip`,
both NPC-text sources, NPC-text locales, and reachable broadcast text and locales.
All reads use the same snapshot and fixed, bounded, parameterised selectors.
The extension binds the schema, base rows, selected keys, version notes, and text bytes.
Legacy absence means unknown coverage.

Canonical relation sorting does not establish loaded menu order.
The SQL extension therefore records unknown loaded order, effective giver selection,
callback registration, global SpellArea and item effects, session locale, and dialog policy.
The preparation `quest_giver_presentation_plan` retains each independent refusal.
It never turns zero script IDs or a blank template ScriptName into absent callbacks.

`CapturedOrdinaryQuests` accepts independently resolved source projections.
`QuestDependencySource::resolve` checks the actual selected giver identities and
loaded script registration before it creates private `ResolvedQuestDependencies`.
The bounded resolver refuses present callbacks, nonempty global SpellArea, and
item-deletion effects. Checked ordinary menu/text/option/condition projections
support the questgiver gossip branch without bypassing menu dispatch.
The runtime binder checks actual profile spawns and effective flags.
The existing progression constructor still owns rewards, rates, skills, XP, reputation, and loot.

`CapturedQuestPresentation` derives the existing quest metadata and greeting values.
The full client catalog delegates quest, NPC-text, and script-text lookups to that immutable output.
The caller combines its digest with the full client manifest.
It supplies the explicit session database-locale mapping and complete Emotes lookup.
Extraction v4 adds Emotes to the exact 21-table contract.
Extraction v3 remains valid for its original 20 tables, but does not prove greeting-emote validity.
The adapter preserves `$N` and `$B` and rejects differing same-entry spawn greetings.
The existing projection independently checks gameplay metadata and selects single-quest
details, request/offer panels, or the ordered menu.

The eager closure is unchanged. Every quest attached to a selected giver remains selected.
Quest 15's reference to 21 cannot disappear. A captured completion script must
compile its exact commands and dependencies. Source-item quest 3104 requires
the ordinary inventory host for grant, storage refusal, and source-item removal.
There is no ignore switch, endpoint truncation, dormant-row admission, or production capability.
Authored positive short-chain tests are not evidence that the real Northshire catalog is admitted.

### Completion scripts, source items, and ordinary gossip

`capture --quest-mechanics` implies `--quest-presentation` and adds
`quest_mechanics_capture`, format `benilla-classic-quest-mechanics-v1`.
It captures selected completion-script IDs from `dbscripts_on_quest_end`,
default gossip menus plus menu zero, options, direct conditions, NPC text and
reachable broadcasts/locales in the same snapshot. Exact schemas and primary
keys live in `quest_mechanics_columns.py`. Script rows have no upstream primary
key, so canonicalisation retains duplicates instead of merging repeated commands.
Hashes bind selectors, lookup coverage, rows, schema, source selection, base-table
hashes and version notes. MyISAM requires the explicit quiescent-snapshot mode.

The capture does not claim loaded equivalent-priority script order.
`loaded_script_order` remains null, and preparation requests independent evidence.
The Rust adapter consumes independent loader positions and preserves delay/tie
order. Only ordinary TALK and single-choice EMOTE compile. Source strings, random
alternatives, buddies, flags, conditional script execution and other command types
retain precise refusals. The native runtime persists scheduled work and checks
the explicit SAY configuration and source-owned camera traversal before delivery.

Source items use `CanGiveQuestSourceItemIfNeed`, including existing bank counts
for grant capacity. Abandon removes the source count before changing quest status.
The current native inventory excludes bank/keyring and equipped containers.
Turn-in removes required delivery items only; it does not blindly delete every
`SrcItemId`. The captured loader normalises a zero source count to one when the
item exists, as `ObjectMgr.cpp:4463-4482` does.

Gossip preserves the menu-zero fallback, the hidden questgiver option, conditional
quest-list suppression, and highest-satisfied-condition text selection.
Conditions NONE, QUESTREWARDED, QUESTTAKEN and QUEST_NONE support ordinary player
targets and the reverse-result flag. Other conditions and reached services refuse.
Givers with the GOSSIP flag send `GossipMenu` with the available quest list.
NPC text requests use the captured full eight-block source, not a quest-list text ID.
Without that flag, nonempty quests use the original direct quest-dialog branch.

These authored content ports don't establish production admission or the
independent hook, order, geometry, and full-profile contracts.


The GPL-derived algorithms use CMaNGOS Classic at the pin above:

- [LoadQuests and locale loading, ObjectMgr.cpp:4231-4990](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Globals/ObjectMgr.cpp#L4231-L4990).
- [LoadQuestRelationsHelper and creature relation loaders, ObjectMgr.cpp:7940-8035](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Globals/ObjectMgr.cpp#L7940-L8035).
- [Greeting and locale loaders, ObjectMgr.cpp:5775-5909](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Globals/ObjectMgr.cpp#L5775-L5909).
- [GetGossipTextId, Player.cpp:11963-11972](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Player.cpp#L11963-L11972).
- [SendPreparedQuest, Player.cpp:12087-12181](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Player.cpp#L12087-L12181).
- [Dialog status, QuestHandler.cpp:628-744](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Quests/QuestHandler.cpp#L628-L744).
- [Gossip/dialog/accept/reward callback selection, ScriptDevAIMgr.cpp:164-328](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/AI/ScriptDevAI/ScriptDevAIMgr.cpp#L164-L328).
- [Quest packet bindings, GossipDef.cpp:300-585](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/GossipDef.cpp#L300-L585).
- [Quest query handling, QuestHandler.cpp:219-235](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Quests/QuestHandler.cpp#L219-L235).
- [AddQuest and RewardQuest, Player.cpp:12460-12735](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Player.cpp#L12460-L12735).
- [BroadcastText::GetText, ObjectMgr.h:113-128](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Globals/ObjectMgr.h#L113-L128).

Copyright belongs to CMaNGOS Project contributors. Upstream permits GPL-2.0-or-later.
The Rust and Python translations use GPL-3.0-or-later, without warranty.


## Persisted presentation identity gap

The compiled presentation digest changes when its text, greeting, locale, or raw
presentation input changes. The full client catalog can use that digest in its
manifest. `NativeProjection::new` validates the current catalog's map, display,
spell, item, and quest bindings, but accepts any nonzero current manifest digest.
It has no previous expected manifest to compare on reopen.
`NativeStore::Identity` retains world, player, gameplay profile, and domain only.
Therefore a text-only or locale-only change does not currently refuse `Entry::Open`.
Persisted client-manifest binding remains an explicit gap.
This adapter does not add saved fields or change native identity or restore policy.
The controller tests assert digest changes and unchanged-catalog save/reopen,
not rejection of a changed presentation catalog on reopen.

