# Native ordinary quest/progression reconstruction

This is a newly reconstructed runtime integration, not a byte-identical recovery
of the lost schema-5 patch. Source basis is CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`; source-derived implementation retains
GPL-3.0-or-later attribution. All committed runtime fixtures are authored rows.
The native capability constructor remains closed. These tests do not establish
that any actual starting zone, mob, quest, client installation or selected data
capture fits the supported domain.

## Versioned inputs and real hosts

`Profile::initialize_quest_progression` takes the actual starter initializer and
`QuestProgressionInput`. It stages and validates the entire optional genesis;
merely loading a quest catalog does not bypass validation. Native format/policy
is native77 (`benilla-native-player-health-regeneration-v77`), quest genesis policy v11,
and live training policy v3 with its unchanged journal layout v2. Quest genesis
also binds the immutable prepared first-entry identity, when selected.
Older policies are rejected rather than silently migrated.

Required caller inputs include actual level/XP/class rows, source skill order and
slots, initial XP threshold and money, explicit XP/money/kill rates and resolved binary32 talent-rate bits, resolved NPC
flags, complete quest relations and dependency audits, actual raw spell/cast-time
recipes, and source-derived stat/equipment dependencies. No approximate XP module
or fabricated future stats are used. The admitted mage level domain is 1–60 with explicitly unspent talents, with
actual consecutive supplied rows required for every transition. Genesis probes
all supplied level transitions through the same real hosts used at runtime.

The quest host stages player progression, live training, derived stat caches,
equipment recipe, money and ordered observations together. GiveXP runs the real
player reducer at the source callback. GiveLevel runs skill callbacks before
create-stat replacement; training calls capture their transient source views.
Learning notifications are a separate post-order source trace, not initializer
insertion order or a sorted end-state spellbook diff. Existing live recursive
SkillSet callbacks, including same-value setters, are unsupported transactionally.
Unsupported passive/stat/skill effects cannot be hidden behind a visible but
unavailable active spell.

Derived caches retain source partial-update behavior. Item-break evidence binds
actual pre/post recipes and exact callback inputs, including the precommit weapon
skill and zero-delay damage-cache behavior. Snapshot restoration replays mandatory
level/training/derived history and checks actual equipment broken states. Training
and derived journals are bounded; exhaustion fails the whole tick without losing
XP, sequence, RNG, resource, quest or equipment changes.

## Source callbacks and commands

Ordinary quest inputs are `AcceptQuest` and `TurnInQuest`. The separately gated
admin policy exposes durable +1 level and XP-enable operations; it does not award
arbitrary XP, mark completion, submit kill credit or choose rewards. Quest input
NPC handle includes spawn generation. Runtime interaction checks bind current
alive state, NPC flags, combat/charm/reaction facts and exact strict 3D source
interaction distance. The source path does not require LOS, so none is invented.
Read-only previews do not mutate the owner journal and cannot overtake queued or
retained input in the controller. Mutations repeat authority checks in the tick.

Ordinary death awards source Gain/GiveXP before quest credit and both template
kill-credit aliases. Admission requires an exact solo tap, absent interfering
damage/procs/scripts and resolved honor/reputation no-effect evidence. IsNoXp alone
never proves those other paths inert. Awarded spawn generations are durable and
cannot be credited twice after retry or restart. Quest completion/reward flags,
source money clamping and callback-time XP/reward quotes remain in one transaction.
Same-giver next-in-chain details are emitted after RewardQuest, without inventing
a CanTakeQuest gate for that source presentation callback.

Rest admission is an explicit simulation-continuation rule with no source logout,
offline accrual or entered rest area. Reload does not claim CMaNGOS offline-rest
parity. Positive-rest numerical kernels exist separately; no missing area/clock
adapter is silently assumed.

## Prepare, release, and impact source bindings

Every preparing/traveling cast and applied aura carries a validated execution
binding. Historical bindings prove the mandatory derived/training history prefix,
not just broad skill bounds. Applied periodic effects retain their source rank
and level. Direct damage already fixed at launch survives later leveling; periodic
application resolves the current rank at impact and captures that new binding.
Newly learned abilities undergo the same ID, school, amplitude, raw pushback and
cast recipe checks as genesis abilities. Damage cancellation reads their actual
raw interrupt flags; pushback uses the cast's admitted binding.

Prepare captures the original spell ID, effect/basepoint metadata, timer, pushback
budget, and GCD. A supported level transition during preparation commits normally:
only source-evaluated rank and mana cost are rebound at release. A newly computed
cast time does not restart or shorten the original timer, and a newly learned ID
never substitutes for the prepared spell. Release rechecks power before drawing
launch dice; insufficient mana fails that cast while retaining an earlier reward.
Cancellation still removes only the prepare GCD. In-flight direct damage remains
fixed; periodic values use current impact rank.

Both prepare and successful-release witnesses independently replay their source
training/stat histories. The cast checkpoint records the exact prepare recipe and
release rank/cost, and restore also verifies the exact release timestamp against
projectile travel. Unsupported next-level source context still rejects the whole
transaction; true rank replacement and unsupported learning remain excluded.
Native format 51 explicitly rejects prior schemas; cast checkpoints use version 3,
while common resource and aura checkpoints use versions 1 and 2 respectively. No migration or production
NativeCapability admission is added. The authored controller harness drives the
actual stock command/projection/store path with save failures and pending/flight
reopen checks; this is not proof of graphical or real-asset playability.

## History references and capacity

Native17 replaces each execution witness's duplicated callback journals with
count/digest references into the durable owner's histories. The streaming digest
binds exact profile, genesis, policy, domain, world, owner and character identity,
kind and actual prefix. Restoration checks bounds/digests and replays the actual
prefix against captured player/recipe/training/derived snapshots. Equal-length
divergent histories and cross-world/character transplants are invalid. A later
valid tail does not invalidate an older prefix. Clearing the tick journal after a
store commit does not clear these owner histories. Hash/replay consistency is not
proof that arbitrary supplied rows were captured from an actual installation.

Genesis probes actual consecutive levels and conservatively reserves complete
progression history, all possible remaining defense increments and one-way item
breaks. Each journal remains capped at 4,096 callbacks; the ability cap remains 64.
The widened-integer JSON bound reserves at most half of the unchanged 8 MiB envelope
for progression history. Runtime combined-size admission still applies atomically.
Defense-sensitive learning branches absent from the level-only probe are rejected.
Before replay, encode and typed restore enforce 65,536 aggregate work units: four
owner training replays, owner derived calls, and 64 plus training/derived prefix
lengths per execution witness. Typed restore also streams the serialized size before
hashing/replay, so bypassing `decode` cannot bypass the byte bound. This is a logical
work bound, not a latency guarantee.
Large NPC/aura populations and long histories can exhaust the envelope or replay
budget; later history compaction is still required for broader long-lived worlds.

The authored loaded level-60 regression supplies 64 known abilities, retains 16
level-59 cast-applied auras and 16 concurrent traveling casts, and includes 299
real defense-change callbacks trained between consecutive levels plus one item
break. Its checkpoint is 3,090,142 bytes with 118 training calls, 1,008 derived
calls and 58,280 aggregate work units. Two optimized-test runs restored it in
1.083496 and 1.223802 seconds; this is measured evidence for that composition, not a platform
latency guarantee or a claim that every allowed NPC/aura population fits. Setup
acknowledges checkpoints regularly, retaining progression history while clearing
the tick journal. The seventeenth cast fails atomically; old bindings survive
store boundaries and exact reload.

## Client contract and persistence

`NativeProjection` uses actual caller-owned quest text and numeric metadata.
Client manifest checks cover the entire supplied raw spell closure, including
passives and recursive learning dependencies without active execution recipes.
`ClientCatalog::native_quest_greeting` explicitly captures whether a greeting row
exists, plus resolved localized greeting or NPC gossip fallback and emotes.
`native_quest_presentation` captures actual quest emote columns. Unknown values
reject; the current packet subset admits only zero details/offer arrays and zero
request emote delays. No Classic text or missing emote data is fabricated.

`QuestDialogRules` binds explicit low/high-level visibility and
AlwaysShowQuestGreeting choices into genesis identity. Relations retain captured
source query order for menus. Empty hello menus emit nothing; single entries
follow source auto-open rules; completed no-item quests and empty request text
redirect to OfferReward. That redirect is presentation only and cannot authorize
an incomplete turn-in. Offer trailing fields represent admitted RewSpell and
RewSpellCast, both zero, despite inherited DTO field names.

The app controller routes status/hello/query/complete/request-reward reads and
accept/choose-reward mutations to this boundary. Generic status queries preserve
their distinct source reaction/flag gates. Source learning packets occur at their
training callback, before subsequent create-stat/reward packets. Initial/reopened
projection contains current state, not replayed reward notifications.

Every quest-enabled committed tick is synchronously checkpointed before its event
batch is exposed, including ticks with only played-time/resource changes. The
bounded outbox retains at most one unpublished batch. After recoverable save
failure, inputs have already been retired; explicit retry saves that exact state,
then publishes once. It does not rerun gameplay or random draws. Uncertain commit
or concurrent-store state requires reopen. A crash after durable save but before
publication can omit a transient notification; there is no unacknowledged network
exactly-once delivery claim. Durable quest/XP/money awards themselves remain once.

## Distinct-actor source-selected component loop

The authored native51 [two-actor fixture](src/native/synthetic_distinct_quest_fixture.rs)
composes the selected interaction, combat, spell-133, current-pose facing, complete
visitor, quest, progression and loot hosts. A is the hostile kill target; neutral-to-all
B is both starter and finisher with matching QUESTGIVER flags and relations. B keeps
ordinary Aggressive empty-AI/script EventAI selection, full motion/event ownership
and retaliation. The external adapter supplies only expressly synthetic spatial
observations and refuses escaped semantic quest, loot, combat or legacy spell requests.
The raw 173-word spell rows, initializer, progression recipe and motion hit metadata
match each other; all rows and the spell values are original test inputs.

The quest requires both one A kill and one ordinary positive-chance corpse item,
with the source loader's exact KILL | SPEAK | DELIVER flags. Both creature entries
have complete kill and loot rows, and the progression catalog supplies the full
configured level-1–60 stat/training closure. Killing alone is incomplete. Collection
updates the objective before item storage, and turn-in destroys the required item
before rewarding XP/money and updating reached levels.

The [runtime guards](src/native/distinct_quest_tests.rs) cover stale/wrong actors,
relations, flags, exact interaction range, missing closures, B retaliation and
whole-frame rollback after a late B receipt at lethal-credit and reward ticks.
The actual app [controller fixture](../benilla-app/src/net/p2p/world/native_controller/distinct_quest_loop_tests.rs)
uses stock quest/spell/loot commands through NativeController, NativeProjection and
NativeWorldStore. Its tests cover both owner schedules and reversed numeric roles,
XP-before-kill-credit and requirement-destruction-before-reward ordering, checkpoint
faults at kill/collection/reward, and save/reopen at accepted, post-kill, pre-loot,
post-collection and post-reward boundaries. Save retries publish the already committed
batch once without another authority call or gameplay replay.

This component contract does not establish installed data, graphical playability or
loaded geometry for the combined quest loop. Its positive reopen checks require
synthetic spatial/cache authority. Loaded native PathFinder cache reopening and
production NativeCapability/startup remain closed; neither is inferred from these tests.

## Explicit remaining native content blockers

- Level-60 numeric progression is not complete melee support. The separate
  [defense kernel](../benilla-sim/NPC-MELEE.md) admits source logarithmic catch-up
  for actual levels 1..60 and pure skill maxima 1..300, with versioned canonical
  log bits and software arithmetic. An authored level-60 test leaves defense at 1,
  executes real melee skill draws/gains, and checks exact replay across checkpoint
  reopen. This does not widen content or combat-host guards. The existing loaded
  capacity fixture retains its 299-callback history from training between levels.
- Ordinary backpack items, corpse loot/money, required-item destruction and fixed/
  choice item rewards now use the source transaction host described in
  [LOOT-INVENTORY.md](LOOT-INVENTORY.md). Conditional/nested-reference/grouped-reference/quest-only loot,
  item-started/source quests, equipment mutation and property/script callbacks
  remain explicit admission failures.
- Audited private row closure, geometry/movement authority, graphical client
  evidence and capability review are still necessary. The blocked offline content
  compiler was neither inspected nor approved by this reconstruction.

## Fresh reconstruction verification

The native17 reconstruction passed 3,343 test executions in the combined six-package
run: 328 simulator tests/doctests, 2,348 UI tests, 80 client/adapter tests, 223
controller source-harness tests, 317 runtime tests plus two capability/numeric-policy
doctests, and 44 store tests plus its separately invoked lock child probe. Two
parent-process tests remain intentionally ignored (the child lock probe and the
existing opt-in UI dispatch benchmark). Both default-library and all-target strict Clippy
pass, as do workspace formatting and diff checks. These are fresh checks on this
bounded reconstruction; they do not claim a full Bevy application build or live
Classic content admission.

For native17, pinned C++ extraction/regeneration freshly reproduces 1,620
derived-stat vectors, 2,429 player-progression cases (including three rejected
rage-domain probes), 21,743 spell numeric/timer/resistance records and 360 release
records exactly. Existing kill-XP and quest-lifecycle fixtures are retained.
These are bounded authored source comparisons; their numerical/trace scope and
unsupported oracle adapters remain documented in the respective simulator files.
They do not certify real exported content or substitute for graphical playtests.

```
cargo test -p benilla-sim -p benilla-world-runtime -p benilla-world-client \
  -p benilla-world-controller-tests -p benilla-world-store -p benilla-ui \
  --no-fail-fast -- --nocapture
cargo clippy -p benilla-sim -p benilla-world-runtime -p benilla-world-client \
  -p benilla-world-store --lib -- -D warnings
cargo clippy -p benilla-sim -p benilla-world-runtime -p benilla-world-client \
  -p benilla-world-controller-tests -p benilla-world-store --all-targets -- -D warnings
```

## Corpse-carried metadata boundary

Native48 retains corpse acquisition of inert carried metadata and
ordinary empty bags through the checked inventory corpse policy. Unused spell
cooldowns, all five charges and durability retain source values; Bag instances
retain source container slots and zero creation charges. The existing sixteen
backpack positions, unique limits, binding, checked IDs and ItemAddedQuestCheck
before storage stay authoritative. Collection remains all-or-nothing, with the
quest host and corpse mutation committed together. Admin partial grants and
self-unbinding are not used. Quest rewards and starter inventory still use the
unchanged ordinary readiness; timed/auto-reward quest and effect audits remain.

Inventory policy v5, corpse policy v1, loot genesis v3 and corpse-item receipt v2
are bound under native51 profile/state/record/checkpoint identities. Native50 and earlier
headers, policies, legacy digest domains and profile contexts reject, including
rehashed outer checkpoints/journals; no serde defaults or replay migration exist.
One-hop ungrouped body references have separate catalog/rate semantics; nested
and grouped references remain excluded. Every namespace still requires genuine
whole-table loader-order evidence. Selected SQL reader closure is not that proof.
No item use/equip, extra bag capacity/contents, playable Northshire, actual Vermin
completion, production admission or saved-cache opening is established here.

## Quest-7-shaped component scene

The authored three-actor regression retains the public quest-7 contract: prerequisite
783, Marshal McBride's starter/finisher role, ten entry-6 kills, no required items,
and the quest-15 predecessor gate. Quest 783 starts at the separate Willem role and
finishes at McBride. Ten distinct lethal spawn generations drive the existing quest
reducer; repeating an attack on a corpse grants no additional kill credit or XP.
Each death executes an authored one-hop reference drop with its own rate draw.
Completion precedes loot collection, and reward money/reputation/XP use the normal
reducers. Serialized checkpoint reopening and segmented journal replay retain each
incomplete counter and the completed/rewarded boundaries.

Only the specified quest scalars and relations are source-shaped. Stats, levels,
spells, loot, spawn times, factions, AI/interaction witnesses and geometry are
authored controls. The scene uses the legacy component authority and does not
select a three-owner source visitor, motion, combat or spell host. The existing
one/two-owner source scheduler remains unchanged. An explicitly synthetic unoffered
quest-21 boundary closes quest 15's chain pointer without claiming its downstream
content. This test does not certify actual Northshire scene closure or admission.

## Original armor membership

The separate [armor membership owner](ARMOR-MEMBERSHIP.md) reconstructs custody
and real modifier callbacks before combat restore. Required recipe-v3 inactive
membership is not accepted from snapshot flags alone. This bounded host exception
uses carried-items-v12; it does not add generic equip, native admission or save
migration. Historical contexts bind their own validated journal prefix.
