# Ordinary native corpse loot and item rewards

This is a fresh GPL-3.0-or-later reconstruction from CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, not recovery of the lost patch.
`NativeCapability` remains uninhabited. Authored source comparisons and controller
integration tests do not admit real content or establish a playable starting zone.
Native format is 46 (`benilla-native-peer-help-v46`). Loot generation policy
is v2 and loot genesis is v3. Older saves do not migrate implicitly.

## Captured inputs and fail-closed scope

`QuestProgressionInput::loot` supplies an audited ordinary item catalog, complete
ordered selected creature-loot and reference-loot captures, explicit
quality/reference/money/empty-window rules,
and one resolved corpse row per selected creature entry. Capture identities bind
the pinned source and the complete observed loader result/order. No canonical sort
is substituted for the source query's observed order. Missing templates, unknown
closure, nested or grouped references, conditional/quest-only loot, item-started quests,
property or item-script hooks, bag contents/bank/keyring, trades, skinning/pickpocket, group or controlled
units, wield-loot, multi-tap, special corpse timers and death callbacks reject.

Items retain resolved source fields. Backpack slots are 23–38, normalized Stackable
is 1–255, reward counts are at most 10,000, and item objectives are 1–63. Sparse reward
arrays reject. Required items overlapping immutable starter equipment reject because
source equipment removal is not hosted. Carrying an ordinary gear-shaped item does
not authorize equipping/using/selling/trading it. Starter identity is cross-checked
against the actual initializer and the supplied client metadata.

## One-hop reference execution

Body and reference templates occupy distinct namespaces, even when their numeric
IDs match. Each namespace retains its own ordered-loader witnesses. The supplied
reference projection is exactly the targets of body references; placeholders in
reference rows are not item metadata dependencies. Every positive leaf has exact
item closure, including shared-item consistency and corpse-carried inventory checks.

An ungrouped body reference first performs its source roll using the referenced
item rate. Chance at least 100 succeeds without a roll or rate multiplication. On
success, `maxcount` invokes the complete child template that many times, at the
parent row's execution position. Child ordinary leaves use quality rates; groups
retain their explicit/equal selection rules. A row's group ID denotes membership
in its current template, never a subgroup selector in the referenced template.

Nested references and references inside groups remain unsupported. Admission
bounds expanded work and distribution draws independently of the sixteen-item
output cap: filling the corpse does not skip later source rolls or shuffles.
Neither dump order nor selected-row sorting supplies an observed loader result.
Authored component inputs exercise these mechanics without certifying real data.

## Source transaction order

Positive authoritative player damage binds the original recipient before kill
callbacks. Unit kill XP and quest credits precede ordinary PrepareBodyLootState.
The corpse stores generation, original owner, source-ordered slots, money and exact
distribution receipts. Nothing is automatically placed in inventory at death.
Loot::Process performs ungrouped entries then ordered groups, retains source float
comparisons, maximum sixteen items, money shift/rate branches and optional fake-empty
window draws. The native SHA counter supplies distribution outcomes; it does not
claim C++ MT or std::shuffle word-stream equivalence.

Opening uses the source inclusive 3D reach-based distance and alive, in-world,
standing and stunned gates. There is no invented LOS gate. It captures the panel,
runs InspectingLoot and then interrupts preparing casts. An exhausted original
nonempty item list instead sends release before registration or inspection; an
originally empty item list can still show an empty window. Collection rechecks owner,
generation and the published panel binding; source collection handlers do not
repeat the opening range/alive checks. A storage error keeps the full corpse item,
existing stacks, allocator and quest progress. CanStoreNewItem's partial plan is
never permission to grant a partial ordinary loot item.

StoreNewItem reserves the original instance before clones and calls ItemAddedQuestCheck
before StoreItem. Quest item credits therefore observe pre-store counts. Required
carried items are destroyed in source traversal/callback order. Reward preflight
uses one shared vector of whole-position reservations before deleting requirements.
Actual reward grants use fresh plans after deletion, choice first then fixed items.
A later ordinary unique/capacity refusal skips only that grant, as source does;
internal faults roll back the entire owner tick. Item events precede XP/level/money
callbacks and completion. Any source CanRewardQuest refusal reoffers the reward,
following an inventory error only for item/storage failures. Only owner
accept/choose-reward requests are exposed. The source reserves the original reward
instance before its item callback; the admitted cloned reward host stages the
callback first because callbacks cannot observe allocation IDs or creation hooks,
and any storage fault discards the whole tick. Expanding that callback domain
requires explicit allocator staging (see the provenance discussion).

## Release and packet semantics

Missing corpse slot produces ItemNotFound then the source Release path; an already
collected slot releases without that error. Explicit Release unregisters the player
before SendReleaseForAll, so the requesting solo player gets no release-response
packet. Money-last exhaustion follows that same path. Item-last exhaustion uses
SendReleaseForAll directly and does send the response. State transitions and actual
packet emissions are distinct. An explicit release queued after item-last still
performs the first LOOTED transition because its source handler resolves the explicit
GUID independently of the current player loot GUID. Duplicate release is inert.
UNIT_FLAG_LOOTING 0x400,
corpse lootability, item instances and source item pushes are projected through the
stock protocol adapter. Solo money does not invent the group's money notification.

Repeated LOOTED transitions are source no-ops. Empty loot's early shortening is
overwritten by JUST_DIED assigning the full corpse expiry; later release cannot
apply that shortening again. Inspection extends and newly LOOTED release reduces
expiry. Commands occur before the same-tick AI removal, and a last timer callback
at the removal time is valid. Corpse removal/respawn cannot retarget stale handles.

## Persistence and publication

Whole-tick cloning stages inventory, quest state, XP/money, RNG, identities and
observations together. Every committed quest-enabled tick is saved before exposing
its UI batch. A recoverable save failure retains exactly that unpublished batch;
retry saves the computed state without repeating loot or rewards. Reopening a saved
world projects current items and panel state without repeating reward notifications.
A durable save followed by a crash before publication may omit a transient packet;
there is no unacknowledged network exactly-once delivery claim.

Canonical state holds at most sixteen live-stack receipt witnesses and the latest
effective corpse timer witness per NPC, not a lifetime collection/open history.
Journal acknowledgment clears only replay records. Restoring regenerates each
current corpse from its exact recorded draws and validates present item metadata,
IDs, immutable equipment, receipts, owner and generation. Each live receipt
recomputably binds the current slot and full instance plus its reason and previous
digest commitment; it does not replay all past acquisitions. These are integrity and
reconstruction checks, not anti-cheat against an owner replacing a whole checkpoint.

## Remaining boundary

The source release-phase cast adapter, real private row closure, geometry/movement
admission, multiplayer world authority and live graphical/FrameXML evidence remain
separate work. The held offline starter compiler and old consumer patch were neither
read nor approved. No client assets or private databases are included.

Source witnesses and notices are recorded in `../benilla-sim/PROVENANCE.md`,
`INVENTORY.md` and the source-oracle tools. Use the five-package simulator/runtime/
client/controller/store tests and scoped strict Clippy without a heavy Bevy link.

## Fresh focused verification

The reconstructed five-package run on 2026-10-14 passed 533 test executions:
224 simulator tests/doctests, 56 client tests, 79 source-level app-controller tests,
133 runtime tests plus its compile-fail capability doctest, and 39 store tests plus
the invoked lock-child probe. The probe is intentionally ignored only in its parent
process. The new source generators reproduce 5,452 C++ vectors under UBSan: 1,577
storage plans, 515 creation/storage cases, 1,536 loot cases, 800 timer cases and
1,024 quest-item reward transactions. Strict all-target Clippy passed for those
five packages. The integration exercises real native reducer/controller paths with
authored rows; it does not establish real-content admission or a graphical playtest.
