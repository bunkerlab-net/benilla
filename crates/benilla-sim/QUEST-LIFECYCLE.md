# Bounded ordinary Classic quest lifecycle

`benilla_sim::quest_lifecycle` translates ordinary, solo quest acceptance,
kill/talk/collection credit, completion and reward orchestration from CMaNGOS
Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. The policy identity is
`cmangos-8ec338a1-ordinary-quest-lifecycle-v4`. This is an independent simulation
reducer, not a native content compiler, complete inventory/level-up implementation,
world-runtime integration, durable store, or claim that a real starter quest works.
The authored world-runtime fixture is unrelated and is not evidence of Classic
quest parity. No native quest rows, text, assets or database are bundled.

## Admission and actual relationships

`Catalog::new(rows, relations, audit)` takes explicit loader-resolved gameplay
columns, native creature starter/finisher relations, and a complete dependency
scan. No row, rate, eligibility or evidence type has a default. Unknown, present,
or unscanned hooks fail closed. The compiler must establish actual source rows,
resolve all creature/item IDs, apply upstream loader normalization or reject
invalid inputs, and attest the complete relevant relation scan. This API cannot
cryptographically prove that a caller supplied honest/complete evidence.

The native progression input instead requires `RelationshipCapture` and calls
`Catalog::from_relationship_capture`. This constructor checks exact selected
roots, raw field agreement, and complete incoming evidence before deriving
`complete_relation_scan`. It bounds the closure to 1,024 quests.
`Error::Relationship` identifies the source quest, field, signed target, and
refusal reason. Missing incoming coverage is not an empty scan. External
incoming quests and uncaptured reachable references refuse rather than
becoming playable quests or disappearing. The Python preparation validates
the snapshot-bound sidecar and supplies this projection, its source digest,
and normalised relationship lists. See the
[capture contract](../../scripts/classic_export/QUEST-RELATIONSHIP-CONTRACT.md).

`QuestRow` includes the supported fields and `ExcludedFields` retains unsupported
behavioral columns so nonzero values cannot silently disappear. Descriptive text,
map POI and emote/UI columns remain the presentation layer's responsibility.

The admitted domain is ordinary method-2, nonrepeatable quests, creature questgivers,
player levels 1..255 with explicit native min/max/class/race masks, up to four
item and four positive-creature objectives, and signed money rewards/requirements.
Creature objective counts and admitted item requirements are bounded to 1–63,
respecting the source log fields and item-credit packet assertion.
The compiled rules and player identity must separately establish a legal native
race/class combination. No particular starter quest or faction is assumed.

The pinned loader sets BOTH `KILL_OR_CAST` (0x20) and `SPEAKTO` (0x10) for every
nonzero creature/GO objective, and `DELIVER` (0x08) for item objectives. The catalog
checks that exact resolved flag combination for supported positive creature rows;
it does not invent a kill-only flag. `TalkCredit` preserves the source's shared
kill/talk objective counters. The caller must already have established an actual
admissible NPC interaction before issuing that command.

Relationships are reconstructed from the full admitted closure:

- A nonzero signed `PrevQuestId` becomes a prerequisite of that row
- A nonzero signed `NextQuestId` on another row contributes the signed predecessor
  ID to its absolute target, preserving reverse/inbound relationships
- `NextQuestInChain` supplies both the forward exclusion and the target's
  `prevChainQuests` exclusion
- A positive prerequisite requires rewarded history, not just completion
- A negative prerequisite requires a current incomplete or complete-unrewarded
  quest; multiple admitted prerequisites are an OR
- A forward chain target already started/completed blocks acceptance, and any
  current predecessor in `prevChainQuests` blocks acceptance

Missing referenced rows are errors. A full inbound scan is necessary: passing a
single selected row with an empty list is not proof of no prerequisites. Signed
minimum-int relationships and oversized IDs are rejected. Positive/negative
exclusive groups, breadcrumb relationships and dependent breadcrumbs remain
unsupported, so their all-member/branch/recursive rules are never approximated.

The following also remain excluded: required skills/conditions;
timed, weekly, repeatable, auto-reward/auto-complete,
exploration, escort, cast and GO objectives; group/raid/shared acceptance;
source items/spells, source-drop rules, reward spells,
mail and all scripts. `SHARABLE` may be present as a flag, but only solo acceptance
is implemented. Hidden-reward and no-money-from-XP flags retain their existing
source meanings. Other flags fail. Required collection items must have explicitly
resolved, inert deletion effects. Global/zone/area SpellArea effects and all selected
giver/finisher ScriptDevAI hooks must be proven absent across permitted locations.
Ordinary chosen/fixed item rewards require the audited inventory host; sparse
reward arrays and unresolved/effectful templates reject. The six choice and four
fixed pairs must form dense prefixes with counts at most 10,000.

## API and transactional contract

- `can_take`, `can_add`, `can_complete`, `can_reward` expose the admitted source
  predicates. Predicate calls are previews, not reservations or authorization
- `State::new`, `snapshot` and `State::restore` retain the ordered 20-slot quest
  log, progress, reward history, exact next command sequence and catalog identity
- `reduce(catalog, state, host, sequence, command)` deep-clones its inputs and
  returns a `Transaction { state, host, events }` only after every step succeeds
- The commands are `Accept`, `KillCredit`, `TalkCredit`, `ItemAdded`,
  `RemoveItems`, `Complete`, `MoneyChanged`, and `Reward`

A `Host` supplies explicit solo participation, native identity/configuration,
carried/bank counts, actual source stack-removal steps, and audited staged
`GiveXp`, `ModifyMoney`, `DestroyCarriedStack` and `StoreRewardItem` implementations.
`can_store_reward` preserves shared whole-position reservations across preflight. It must be a
deep in-memory clone. Applying an effect must not publish packets, write storage,
share mutable state with the original, or recursively run quest callbacks. The
reducer calls the required quest callbacks itself. Host effects in `events` are
already applied to the returned host: they are observations, not instructions to
apply the reward again. Unsupported or failed host operations return an error,
which discards all staged quest/host changes and events.

The fixed 20-slot log and four objectives are gameplay bounds, not an untrusted
input-size envelope. The compiler/store must bound catalog rows, relation lists,
retained history and encoded input sizes before constructing these values. The
inventory host must bound its removal plan by actual carried slots; this API
does not impose an allocation cap on its returned vector. Callback views include
rewarded history, so output size also depends on those caller-enforced bounds.

The owner must durably commit returned quest state, host inventory/currency/XP/
level/stats, sequence, and an event outbox in one transaction, before publishing
notifications. It must also bind that transaction to the complete native rules,
configuration and host-policy hashes. This module supplies no filesystem or DB
atomicity. A repeated sequence is rejected; a later sequence cannot reward an
already rewarded quest. Rewarded records stay `Complete` after leaving the log.
Dropping reward history or saving host/state separately breaks the contract.

`ItemAdded` runs after item creation and BEFORE the staged inventory store, as in
Player::StoreNewItem. Pass the pre-store host and atomically commit the resulting
quest state with the final inventory. `MoneyChanged` observes the staged new money.
Do not publish or persist either external mutation first. Loot acquisition,
bag placement and item capacity admission belong to the inventory host. `RemoveItems`
executes its audited destruction plan within this transaction. Source stack order
and each pre-deletion count matter; aggregate collection deletion is not a valid
substitute for multiple native stacks.

Snapshot restoration rejects changed catalog/policy, missing/duplicate log IDs,
rewarded quests still logged, lost active records, nonzero absent-objective counters,
out-of-range creature credit, and incomplete objectives marked complete. Rewarded
history must retain completed creature credit; item credit may decrease during
reward. Present item counters retain the full source uint32 domain, including
source-reachable values above the requirement. This is a typed boundary, not a
save-file format or authenticity check. There is no failure/repeatable
reset path.

## Source ordering

Acceptance checks source status, class, race, level, reputation, prerequisite, forward/previous
chain and active gates in `CanTakeQuest` order; other source gates are inert only
because admission proved their prerequisites absent. `CanAddQuest` checks the
first free slot. Source-item storage is absent by admission. Acceptance initializes
progress, calls reputation `SetVisible` for a nonzero reputation objective faction
before setting the first free log slot, records the inert acceptance hook, adjusts
collection counts including bank inventory, and updates world objects. The
operation composes `AddQuest` with a subsequent `CanCompleteQuest`/`CompleteQuest`
check; this composition is an explicit adapter operation, not a port of all
QuestHandler UI/admission code.

Kill/talk credit iterates log slots and then objective indices, increments only
incomplete quests up to their required counts, emits the native creature-credit
callback arguments and checks completion. The caller must derive kill aliases,
actual death/loot eligibility and trustworthy event identity; clients cannot claim
kill credit by supplying an entry or GUID. Exactly-once command sequencing does
not replace authoritative death-event admission.

Completion calls `SetQuestStatus`'s world-object update before setting the quest
log's completed bit. Item addition preserves the capped counter and its extra
world-object update when an individual objective finishes. Item removal preserves
upstream uint32 subtraction semantics and can reverse completion. Its source
formula can also leave item progress above the requirement: carried 4, required 2,
removal 3 changes the counter from 2 to 3 while carried stock falls to 1. This is
preserved, including when another quest consumes the same collection item; neither
clamping nor rejecting that source-reachable state is equivalent. `WorldObjects`
events contain an immutable callback-time `QuestView`: do not query only the final
state when replaying them. These views may represent intermediate states and are
intentionally not restorable checkpoints.

Reward first checks complete, unrewarded status, carried items (bank is excluded),
and existing required money, then preflights `quest::reward_quote`. In source order:

1. Destroy required items, including each stack's pre-removal quest callbacks.
   The rewarding quest can temporarily become incomplete here
2. Remove timed-quest membership, store the selected then fixed reward items with
   their pre-store callbacks, and call the synchronous reputation reward host
3. Clear the quest-log slot
4. Below the actual configured cap, invoke `GiveXP`; at/above cap apply the separate
   XP-to-money conversion. XP itself is calculated by the existing software-binary32
   `quest` policy, not fixed starter XP
5. Apply ordinary reward/required money as another `ModifyMoney`; verify its exact
   source clamp and run `MoneyChanged` for other logged quests after EACH operation
6. Set status complete (including the immediate world-object update), set rewarded
   history, optionally announce with source display XP, record the inert reward
   hook, then perform the final world-object update

The required-money check precedes either monetary reward; cap conversion cannot
pay that requirement. Combining the two money operations would change clamping
and callbacks. Reward events retain the source's intermediate state before and
after the rewarded flag changes.

## Concrete next host dependencies

`GiveXp` must implement `Player.cpp:2668` `GiveXP(xp, nullptr)`, not increment an
unbounded experience counter. It needs actual alive state, current `PLAYER_XP`,
`PLAYER_NEXT_LEVEL_XP`, configured player cap, and native uint32 arithmetic. Zero XP,
dead players and players already at cap return without XP. Quest XP has no rested
bonus because the victim is null. Emit the non-kill XP-gain notification, iterate
level thresholds and call `GiveLevel`, then write the remaining XP. Missing levels
or undefined numerical domains must reject the whole staged reward, never fall
back to level-one stats or a guessed threshold.

For each admitted next level, capture the selected actual rows:

- `player_levelstats(race,class,level,str,agi,sta,inte,spi)`
- `player_classlevelstats(class,level,basehp,basemana)`
- `player_xp_for_level(lvl,xp_for_next_level)`

`GiveLevel` (`Player.cpp:2707`) emits level-up deltas, sets the next XP threshold,
resets level-played time, sets level, runs `UpdateSkillsForLevel`, replaces create
attributes/health/mana, initializes talent points, updates all stats, refills
health only when alive, fills mana/energy and clamps rage, synchronizes pets when
present, then updates the session's player level. A bounded mage 1→2 implementation
can explicitly exclude pets/talents/external effects; their absence must be proven.
`UpdateSkillsForLevel` (`:5595`) requires native always-max-skill configuration,
personal/global skill caps, actual SkillLine and SkillRaceClassInfo dependencies,
and all SetSkill/trained-spell consequences. Existing equipment, passive aura
modifiers, stats and max-resource policies must be applied at the new level.

Collection deletion must implement native `GetItemCount` and `DestroyItemCount`
(`Player.cpp:10415`, per-item overload `:10571`): traverse backpack, keyring,
inventory bags, then equipment/bag list; skip traded items, follow the admitted
unequip policy, and preserve actual per-stack order. Required reward items are
carried-only, with no bank deletion. Each step invokes `ItemRemovedQuestCheck`
before changing/removing the stack. Relevant item effects must be resolved and
inert; inability to consume the full requested count must reject. The reducer
verifies each carried-count delta. It does not invent container slots, item
capacity, random properties, equip effects, loot distribution or trade behavior.

## Evidence and reproduction

The generator checks full-file SHA-256 for Player.cpp, Player.h, QuestDef.cpp,
QuestDef.h and ObjectMgr.cpp. It compiles 26 unchanged Player method bodies, three
unchanged Quest numeric bodies, the unchanged loader relation block, and the
unchanged inline ModifyMoney body (only its method name is renamed for a tracing
wrapper). Unsupported gates are explicit inert adapters on admitted synthetic rows;
source/reward item branches assert if reached. The mock GiveXP records the exact
argument; it is not evidence of native XP/level-up parity. Fixture counts are
4,320 lifecycle state/ordered-callback outputs, 120 relation/eligibility probes,
and 32 surplus/shared-item removal/reward snapshots with carried-stock counts.
Those 32 snapshots also compare every callback-time log and progress record, and
Rust failure injection verifies rollback after each staged host effect.
All rows are original synthetic algorithm probes, not exported game content.

```
python3 crates/benilla-sim/tools/generate_quest_lifecycle_fixture.py \
  /path/to/pinned/player-and-quest-sources /path/to/pinned/ObjectMgr.cpp
cargo test -p benilla-sim --test quest_lifecycle
cargo test -p benilla-sim --release --test quest_lifecycle
cargo clippy -p benilla-sim --lib --test quest_lifecycle -- -D warnings
```

The C++ build uses C++17, FE_TONEAREST, no fast math/contraction, and undefined/
float-cast-overflow sanitizers. Reproduction must leave the fixture byte-identical.
Cargo tests need no C++ compiler, native database, installed game files or network.
Focused tests also cover partial-progress restoration, failure after a host mutation,
carried-vs-bank reward rejection, duplicate finalization across restoration,
source callback-time snapshots, log capacity/first-free-slot, chain semantics,
and invalid/unknown dependencies. Linux amd64 is the verified platform; this does
not establish cross-platform replay, real starter rows, native UI or full-world
playability. Native quest capability gates remain closed until the compiler,
actual host, authoritative command admission and durable runtime are integrated.

## Ordinary reward-item host

`can_reward_choice` retains base gates, then reserves selected and fixed item
positions in one shared destination vector before any requirement is removed.
An empty in-range choice is legal; indices at least six reject. Actual RewardQuest
removes requirements, removes timed registration, then replans/stores the selected
item and each fixed reward against evolving inventory. A later legitimate storage
refusal skips that grant and continues XP/completion. Internal host failures still
roll back the complete transaction. Item-added callbacks occur before each store
while the rewarded quest is still logged, including source recompletion behavior.

See [INVENTORY.md](INVENTORY.md) for the actual backpack host and
[../benilla-world-runtime/LOOT-INVENTORY.md](../benilla-world-runtime/LOOT-INVENTORY.md)
for its source-ordered native durable integration. The original lifecycle oracle
retains its explicit no-reward-item adapters; the separate reward-item oracle
exercises the newly admitted storage/callback path. Neither admits real content.


## Reputation host and callback evidence

Required-min reputation is inclusive; required-max is exclusive. Both gates query
only nonzero factions and precede predecessor checks. `can_take_view_with_reputation`
and `can_see_start_view_with_reputation` accept fallible callback-time standing
queries; legacy view calls reject when a reputation query is needed. A nonzero
reputation objective is checked after item, creature and money requirements.
`reputation_changed` walks quest-log slots in their stored order, completes an
incomplete quest only after all completion checks pass, and makes a completed
quest incomplete when its standing falls below the objective. It preserves each
intermediate world-object view and completed-bit callback.

`Host::reputation` returns absolute standing. `reputation_visible` stages the exact
source visibility mechanism before the `ReputationVisible` marker and `LogSet`.
`reward_reputation` stages the source reward mechanism and all synchronous quest
callbacks before the `RewardReputation` marker, log clearing and XP. Native hosts
may queue their reputation outputs and callback quest events for expansion at the
marker; they must not also append duplicate callbacks to the outer event stream.
The default query/visibility hooks fail closed. The default reward hook succeeds
only when every reputation reward faction, value and flag is zero. Any hook
failure discards the reducer's cloned host, quest state and events together.
Reputation fields remain in `ExcludedFields` for schema compatibility; they are
admitted host inputs, unlike the remaining unsupported fields in that structure.
The caller must resolve faction dependencies and use the audited reputation host;
this reducer does not implement standing rates, racial modifiers or spillover.

The supplemental generator hash-checks the same five pinned files and compiles
29 unchanged Player method bodies, including `SatisfyQuestReputation`,
`ReputationChanged` and `CanSeeStartQuest`. Its 1,212 synthetic probes cover signed
min/max boundaries (900), acceptance/visibility and competing requirements (180),
slot-ordered completion/demotion callbacks (108), and synchronous reward callbacks
before log clearing/XP (24). Callback-time views are compared exactly. The harness
reputation host applies authored signed deltas; these probes establish quest
orchestration, not reputation reward arithmetic or spillover parity. Rust tests
also inject failures after visibility, after each reputation delta/callback, and
after a later XP effect, and verify that unsupported hosts fail closed.

```
python3 crates/benilla-sim/tools/generate_quest_reputation_fixture.py \
  /path/to/pinned/cmangos-classic --check
cargo test -p benilla-sim --test quest_lifecycle
cargo test -p benilla-sim --release --test quest_lifecycle
```

## Abandonment and reacceptance

The source QuestlogRemove handler preserves a status-NONE record and its objective
history. Its world-object callback runs while the slot is still occupied, before
slot clearing; durable snapshots forbid that transient logged-NONE state. Invalid
slots are silent and empty valid slots still clear. Reacceptance resets objectives
and recounts retained deliver items without erasing reward history. Source-item,
timer and script dependencies stay excluded; no ordinary deliver items are deleted.

The composed native route and its reputation-objective proof limitation are
specified in [QUEST-ABANDON](../benilla-world-runtime/src/native/QUEST-ABANDON.md).
