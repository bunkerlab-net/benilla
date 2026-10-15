# Explicit native-world admin operations

This fork-only interface implements data-bounded one-level grants, an XP-gain
toggle that preserves current XP, and positive carried-item grants.
It is not a CMaNGOS chat command, a realm-admin login, a PSK permission, or a new
simulation authority. Native format 23 binds the policy, receipts, XP flag and
carried-item metadata, including real empty container instances.
`NativeCapability` remains uninhabited. The exact app/UI/store route is exercised
only with authored component fixtures; no production host is installed.

## Authorization and identity

A profile has no admin permission by default. Its optional `AdminPolicy` names one
exact world, an explicit owner, up to 64 additional owner IDs and a nonzero policy
revision. The policy participates in the admitted profile digest. Construction and
restore check the world binding. Policies are immutable in this slice; changing or
revoking one is not a supported in-place operation and an incompatible open preserves
the existing save. There is no self-grant or permission-edit command.

`OwnedInput.authenticated_owner` must match the controller's authenticated selected
owner before any frame is staged. The request separately binds the same world and
selected character. Its serialized `player` is a target binding, never proof of the
actor. The existing solo-owner contract stays intact; an allowlisted owner can
administer their selected owner-bound character, not send commands for another
player. No central server, elected-host switch, PSK shortcut, peer result trust or
multiplayer authority claim is introduced.

## Operations and source behavior

`GainOneLevel` invokes the existing source progression host and `pg::give_level`.
The host runs skill caps, live training, attributes, derived statistics, current
resources and other admitted level hooks; the committed projection emits the
existing level and field events. Current XP is unchanged. At the supported bound,
the operation records a successful no-op with equal before/after levels.

Pinned CMaNGOS `Player.cpp:2668–2778` supplies `GiveXP` and `GiveLevel`; its direct
`GiveLevel` explicitly leaves XP to the caller. The requested +1 operation uses
that primitive with an explicit cap, rather than invoking the broader administrator
chat command. The progression kernel admits supplied levels 1–60 with unspent
talent points and the bounded, fixed-equipment closure.
The usable bound is the minimum of the consecutive, checked supplied rows, the
captured configured cap, that kernel bound, and the Classic ceiling of 60. These
values are separate in the backend snapshot. The authored fixture supplies levels
1–3 with configured cap 60, so its UI reports maximum 3. No rows are synthesized,
extrapolated or substituted and no source/dependency audit is relaxed. Ordinary XP
still uses the captured configured cap; an award requiring an unsupplied next level
atomically refuses and pauses the existing controller. The admin button does not
silently rewrite that cap or automatically disable XP. Full playable level 60
still requires complete data and lifecycle admission; a component level bound
is not a production-content claim.

`SetXpGain` changes only the durable XP flag; it does not erase or convert existing
XP. This user-requested fork behavior is not a claim about a stock Classic toggle.
Both admitted award paths are covered:

- Quest rewards use effective XP rate 0 and skip `GiveXp`, so there is no nonkill XP
  log, level-up or XP-field award and the completion packet reports XP 0
- Creature kills skip the entire XP transaction, including kill XP logs and rested
  consumption, but retain original death/tap checks, party-kill notification and
  source-ordered quest kill credit and aliases
- Quest completion, money, inventory rewards, reputation and ordinary corpse loot
  continue normally; XP lock does not pretend the character is at the configured
  level cap and does not create extra XP-to-money conversion
- Re-enabling restores ordinary subsequent XP awards; there is no backlog replay

The current native kill-rest contract admits fresh-player rest only, and excludes
rest areas/offline accrual and group/raid participation. Those exclusions remain;
this work does not claim rested or group gameplay admission. Exploration, PvP,
other player classes and other XP sources are not admitted native award paths. Any
future award path must respect this persisted gate before it can be enabled.

## Positive carried-item grants

`GrantItem` uses the named `CmangosAddItemV1` policy from pinned
`Chat/Level3.cpp:1602–1689`. The trusted host computes destinations and records
requested, granted and ungranted counts plus the original storage result. Partial
capacity and unique-limit awards succeed for the available amount; a zero award
retains a recomputable pre-state witness without allocation or quest credit.
Creation reserves the original item ID before `ItemAddedQuestCheck`, then stores
source-ordered destinations and unbinds every destination for the selected self
target, including previously bound merged stacks. Ordinary loot/reward storage
continues to require a fully successful plan. No deletion or overflow route exists.

Catalog visibility is independent of grant/equip/use readiness. Five complete
spell records retain raw charges, cooldown/category values and binary32 PPM bits;
sets, StartQuest and ScriptId remain intact. Grant does not execute equip/use
spells or autoaccept item quests. Random-property generation, timers, scripts and
other unclosed lifecycle dependencies explicitly block grants for those rows.
The source oracle executes creation and storage hooks and verifies their absence
in the admitted domain. See [inventory source coverage](../../../benilla-sim/INVENTORY.md).

InventoryType 18 creates an actual empty Bag with capacity at most 36, owner and
contained fields, and zero contents. Bag creation differs from Item creation:
its charges remain zero even when raw template charges are nonzero. Carried bags
occupy one of the sixteen backpack positions; they cannot yet be equipped or filled.
An independent acquisition event publishes the source item-push result without
requiring a corpse or a loot-window removal event.

## Idempotency, replay and capacity

An admin request contains a nonzero 128-bit idempotency key, exact world/player,
policy revision and a typed operation. The key is public correlation, not a
credential or signature. Each admitted new request produces an immutable receipt
with its original tick, sequence and operation effect. Repeating its full request
returns that receipt without running hooks again. Reusing the key with different
content is rejected. New requests with missing permission, stale policy, wrong
selection or no progression closure are rejected before effects.

The receipt collection is canonical, bounded at 1024 and 1 MiB, and never evicted.
Every new operation reserves 32 KiB plus a separator and enforces its actual
serialized receipt size before insertion. A cached byte count is maintained on
insertion and fully recomputed on restore; altered cache values reject. This
budget composes with the separate 4 MiB progression-history admission limit and
the unchanged final 8 MiB checkpoint bound. Capacity
exhaustion is an explicit admin rejection; ordinary ticks and exact old retries
continue. Restore validates sorted unique IDs, request/selected-world/policy
bindings, tick/sequence bounds, effect shape, level bounds and the ordered XP-toggle
chain against current XP state. Journal replay reexecutes the typed command and
compares complete receipts, events and state. Checksums establish continuity, not
that an untrusted author witnessed history; they are not authentication evidence.

## UI, storage and recovery

The actual `AdminBackend::{snapshot,submit,poll}` implementation is a bounded
nonblocking mailbox. It installs only through the native controller, with no realm
chat fallback. A public random bridge incarnation binds the context to this world,
selected owner/character and admitted profile. UI correlation sequences derive
stable per-incarnation operation IDs. A fresh UI VM gets a fresh context while old
requests remain in the bounded ledger; unresolved prior-context requests block new
operations until reconciled. A separate FIFO preserves arrival order. The UI thread does no filesystem work; full/busy/stale/unsupported requests
are rejected independently of the simulation. Other admin operations remain unavailable.

The controller appends admitted requests to its ordinary atomic tick queue.
Progression ticks save before publication. On a failed save, the computed state,
exact input retirement and outbox remain held; the visible admin snapshot remains
at the last durable world and `poll` remains pending. Explicit checkpoint retry
saves the same boundary and publishes once, without reapplying the grant.

`NativeStore::committed_admin_receipt` reads only its successfully published or
opened-and-replay-verified `final_world`. Its non-deserializable result carries the
original operation receipt and actual store envelope digest, generation and exact
checkpoint identity. A controller tick comparison, UI callback, computed runtime
receipt or failed checkpoint cannot manufacture UI success.

A reopened exclusively locked store can reconcile retained prior request IDs:
committed IDs recover their original receipt; absent IDs are explicitly reported
uncommitted. Nothing is silently resubmitted with a fresh key. An old context cannot
admit new work in the new session. Across full process loss, callers must retain
the native request key and complete bound payload to query that exact outcome; the
opaque UI context/sequence alone cannot reconstruct its lost namespace mapping. A
fresh UI opens from the durable current state and does not automatically retry
unknown operations. Old native formats are rejected,
not migrated or relabeled. Existing character/exploration-profile files are not
changed.

## Verification boundary

The headless harness compiles the actual app adapter, controller, UI trait, runtime
and store. Tests exercise source hook/XP preservation, explicit owner/allowlist
checks, context and policy failures, request conflicts, exact retries, data-bounded
caps, all admitted XP award paths, receipt exhaustion, canonical restore/replay,
failed-save withholding and reopened-ID reconciliation. These authored tests do
not establish raw content admission, multiplayer consensus, Bevy linking, native
platform power-loss behavior or real graphical client fidelity.
