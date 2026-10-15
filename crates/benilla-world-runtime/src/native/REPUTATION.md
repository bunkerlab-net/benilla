# Native quest reputation integration

This integration uses CMaNGOS Classic commit
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, with the provenance and licenses in
[the simulation crate](../../../benilla-sim/REPUTATION.md). It does not open
production admission and contains only authored placeholder test rows.

## Immutable inputs and transaction boundary

`QuestProgressionInput.reputation` supplies the exact typed reputation catalog.
Its hash binds faction rows, their four race/class slots, explicit resolved reward
rates and spillover results, world rates, and excluded dependencies into quest
progression genesis v4 and native format 25. Quest lifecycle v3 binds all reputation
reward slots/flags, min/max eligibility gates and objective columns. Every relevant
faction lookup must resolve; unlisted lookup is an error. `None` means unavailable
and is accepted only when all reputation columns in the selected quest closure are
zero. It is not evidence that a database contains no factions or reputation effects.

Initialization uses the source race/class masks and `SendInitialReputations` flag
transition. The current bounded quest-level domain remains unsigned 0..=255.
The runtime does not certify the supplied initialization set or invent raw absence
proofs. The full native raw producer remains separate and `NativeCapability`
remains uninhabited.

`QuestHost::reward_reputation` invokes the hosted reputation reducer. Each
`ReputationChanged` receives the exact intermediate reputation state and executes
the source quest-log scan synchronously. Objective completion/demotion and their
ordered quest views are interleaved at that callback position, before later
spillover slots or primary updates. The source loop reads the current staged level
for every reward slot. Reward items precede reputation; reputation precedes quest
log clearing and XP/level-up. Nothing publishes until the entire native tick and
its store checkpoint are committed.

Quest minimum reputation is inclusive and maximum reputation is exclusive.
`CanCompleteQuest` checks the reputation objective. Acceptance makes the objective
faction visible before writing the quest-log slot. Client menu/status queries use
the source reputation query, never the client `FactionInfo` race-slot helper.

## Persistence and session delivery

The persisted primitive state is reconstructed from the immutable catalog and a
bounded source-operation history. It is not trusted merely because checkpoint
hashes match. Each objective visibility operation and non-repeatable quest reward
is replayed. Rewards retain pre-XP level, its source-derived progression-history prefix,
money and compact before/after logged quest witnesses; replay executes the same
hosted callbacks and checks their exact result. The prefix must reconstruct the
recorded pre-XP level at a completed source-operation boundary and cannot move
backwards. A prefix ending inside the mandatory GiveLevel hooks is invalid. The
final accepted/rewarded quest history must match visibility/reward operations, and all standings, flags and send/save bits must match reconstruction.
The witness carries at most the 20 logged quests, not the growing completed quest
map. There are at most 2,048 operations, bounded by the 1,024-quest ordinary catalog.
Missing entries, duplicate rewards, invalid prefixes, out-of-order calls and
mismatched replay results fail closed. Logged incomplete reputation objectives
whose requirements are all met need a compact `ItemRemovedQuestCheck` witness:
the actual pre-removal carried count, previous counter and removed stack replay
the exact unsigned formula and explain the final carried count. This preserves
the source over-count/demotion quirk. Completion or log removal clears the witness.

These witnesses establish mechanical consistency; they do not authenticate an
adversarially rewritten entire ledger against an external database. In particular,
historical money and quest-counter contexts are bounded source inputs, not a full
inventory/currency ledger. `NativeStore` separately verifies exact committed
commands, facts and events against its prior durable anchor. No standalone
checkpoint or witness grants native content or network authority.

The local persistence model continues the simulation without executing native
logout/database save hooks. Source `needSave` is retained as reducer state; this
checkpoint protocol is not CMaNGOS `SaveToDB`.

The initial client projection emits exactly 64 `(uint8 flags, int32 stored delta)`
entries, zero-filling missing reputation indices. It never replays reward, XP or
visibility notifications. Reopening does not advance the clock, NPC timers or
combat. If source `needSend` bits remain dirty, the controller queues an internal
`AcknowledgeInitialReputations` before every user action and simulation callback
in its next naturally scheduled tick. This clears the bits using the source
`SendInitial` transition; it emits no second packet. Deferring delivery bookkeeping
is observationally equivalent because no subsequent reputation operation can run
before this acknowledgement. Failed ticks retain the ordered input; failed saves
publish nothing. A reopen before acknowledgement safely sends a fresh full list.
The acknowledgement compacts into the latest receipt, so repeated opens do not
append unbounded history. It has no public UI/network request mapping.

The projection sends exact source `ReputationVisible` and `ReputationDelta` rows
at their source positions. Reputation callbacks do not invent NPC marker packets;
source `UpdateForQuestWorldObjects` affects gameobjects, which this domain excludes.

## Boundaries

This adds quest reputation state and query semantics. It does not claim tracked
faction combat reactions, NPC hostility or interaction-authority admission; those
require the separate source `classic_targets` relationship closure. Kill reputation,
spell reputation, client at-war/inactive commands, reputation auras, forced
reactions, repeatable/auto-reward quests and scripts remain unavailable under their
existing explicit exclusions. A future callback that performs nested rewards or
level changes must extend the replay witness and policy, not silently reuse an
observation-only callback path.

## Verification

- 10,496 pinned reputation oracle cases from the simulation primitive
- 1,212 additional hash-pinned quest reputation gate/callback/order oracle cases
- Authored runtime tests for three sequential spillover changes toggling one logged
  objective complete/incomplete/complete before XP, rank/at-war transitions, admission
  refusal, rechecksummed corruption, exact replay and reopen
- Actual native projection tests for the 64-entry initial packet, base-excluding
  delta rows, intermediate objective updates and pre-XP packet order
- Actual app-controller tests for repeated bootstrap without clock advance,
  internal acknowledgement before multiple queued actions, authority failure,
  failed-save retry, dropped failed save, duplicate reward and reopen
