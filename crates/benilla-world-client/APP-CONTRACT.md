# Saved-character app integration contract

This is the concrete next wiring boundary, not a claim that the app already uses
it. Existing exploration and synthetic encounter behavior remain separate.

## Entry, identity and content

1. After the user selects a saved character, form `PlayerId { owner:
   character.owner_id, character: character.id }`. Verify it matches the open
   profile identity. The runtime and checkpoint must never receive signing keys
2. Load a separately identified world and its exact content/rules pair. World saves
   are keyed by WorldId and live under the app's executable-adjacent local state
   folder, separate from `p2p-profile` and character position. No coordinate save
   may overwrite a world snapshot. A missing world is an explicit new-world action;
   incompatible/corrupt state cannot silently start a replacement
3. Resolve the actual user-local build-5875 DBC catalog. Implement `ClientCatalog`
   with an immutable manifest that also binds the selected profile's supported
   class/power presentation, full item metadata, full quest presentation and giver
   greetings. Validate these rows against both the actual selected server metadata
   and local client bindings; absence is a blocker. Never fabricate missing values
4. Create/restore `World`; then construct `Projection` for the selected PlayerId.
   `initial_events` supplements the existing selected-character avatar creation;
   it does not replace profile appearance/name resolution. Keep native-source
   runtime gates and client-binding gates independently enforced

The current profile path creates classes with their real presentation power type,
but the original runtime uses mana only. The adapter rejects a non-mana/unknown
selected-player binding. Native player/class stats and abilities are needed before
other classes can enter this gameplay runtime honestly.

## Commands and tick pump

The existing app input enum is `benilla-app/src/net.rs::ClientCommand` and the saved
profile entry loop is `net/p2p.rs::run_character`. The pure adapter purposely does
not depend on this Bevy-facing enum.

- `CastSpell { spell_id, target }` maps to exact-ID `CastSelf` or `CastAtNpc`; reject
  item/ground/corpse/area/aura/channel forms that have no corresponding runtime rule
- `QuestgiverAccept { npc, quest }` maps to `AcceptQuest`. Only
  `QuestgiverChooseReward { npc, quest, choice: 0 }` maps to `TurnInQuest` for this
  no-choice fixed-reward model. `QuestgiverComplete` and `RequestReward` are panel
  queries, never reward grants
- `AttackSwing { guid }` starts an app-owned autoattack request; schedule individual
  `MeleeOnce` intents at runtime eligibility ticks and stop on `AttackStop`, death,
  target destruction or refusal. Do not claim one `MeleeOnce` implements the full
  autoattack protocol. This controller is not yet in the projection crate
- `Loot { guid }` maps only to `OpenLoot(captured_npc)`. `AutostoreLootItem { slot }`
  and `LootMoney` first call `capture_loot(world)` at receipt, then retain that exact
  NpcHandle in `TakeLootItem { target, slot }` / `TakeLootMoney(target)` until input
  admission. `LootRelease { guid }` captures the original handle and maps to
  `ReleaseLoot`; stale releases cannot close a newer window. The adapter rejects
  the runtime's legacy headless atomic Loot command, including remote envelopes
- `ItemQuery` maps to `item_template(entry)`. `QuestQuery` maps to
  `quest_template(quest)`. Questgiver status/hello/query/complete/request-reward
  map respectively to `quest_status`, `quest_greeting`, `quest_detail`,
  `quest_progress`, `quest_offer`. These queries emit panels only. Reward choice
  zero is a separate TurnInQuest intent; nonzero choice is unsupported
- Movement arrives as floating client presentation. Validate finite values and
  quantize only at input admission into integer thousandths of a world unit.
  Capture desired movement and sample bounded deltas each simulation tick; a 2 Hz
  absolute pose cannot be treated as one 100 ms displacement. Retain visual facing
  separately until facing is part of authoritative state. Apply authoritative
  correction with the app's existing movement pipeline; this adapter does not
  currently synthesize selected-player pose feedback
- Revive is an original explicit policy; it cannot silently replace Classic corpse
  release, corpse runs or resurrection UI

At each fixed tick, select at most one admitted input per owner, stamp the exact
next tick/sequence, call `World::step`, then `Projection::tick` and emit its returned
SessionEvents. Empty batches always advance solo casts, NPCs and respawns. Offline
configured endpoints do not join membership and cannot create a solo quorum.
Bound pending input queues, coalesce movement, and return explicit backpressure;
never advance multiple game ticks merely because many input packets arrived.

The app may use a host monotonic clock to decide when to call `step`; gameplay
never reads that clock. Catch-up policy belongs to the app and must not drop
simulation ticks or let wall-clock differences change the command stream.

## Projection details

The tested descriptor indices come from the existing protocol accessors:
health/max 22/28, mana/max 23/29, level 34, XP/next XP 716/717, coinage 1176,
quest-log slots 198+3*n. Four objective counters are 6-bit packed; completion is
byte-three bit 0. These descriptors are durable UI state; quest toasts alone do
not update the log. NPC entry is field 3, display/native display 131/132, health
22/28, level/faction 34/35, combat reach 130, dynamic flags 143 and NPC flags 147.

Cast starts and finishes now come from explicit authoritative runtime events;
there is no guessed spell success from a health delta. Restored pending casts and
remaining cooldowns are projected. The exact current player XP is converted to
within-level XP using the runtime's own threshold table; those original thresholds
are not a substitute for native `player_xp_for_level`.

Item descriptors use owner 6/7, contained-by 8/9, stack count 14 and backpack
GUID slots 532+2*n. Instances persist in runtime format 2 and keep their GUID/slot
on merges; restored ItemCreates do not emit fake acquisition notifications.
Fixed corpse slot indices remain stable after another slot is taken. Initial
projection restores an existing loot window from the remaining authoritative rows.

The item/quest/loot protocol seams are tested with an original mock catalog only;
no native-backed catalog compiler or graphical enablement is implied. Before a
playable stock-UI claim, finish that compiler, melee animation/combat logs with
provenance, movement correction and death flows. Missing native metadata must
continue to block projection rather than become zero-filled defaults.

## Durable checkpoint order

A normal session must not run into the 10,000-tick hard cap. Save on the app's
normal short cadence (for example every 10 seconds of simulation time), on orderly
logout/shutdown and whenever `checkpoint_due()` is true. Pause at a tick boundary
while publishing a checkpoint unless the storage layer can retain post-boundary
journal records correctly.

1. Use the separate world store's `prepare(&world)` to capture the current final
   snapshot and exact journal at one boundary. The store must retain the actual
   base snapshot from which that journal begins. A final snapshot and its own
   already-applied journal cannot be paired as a replay base
2. Under its exclusive writer lock, the store publishes bounded new generations,
   fsyncs the checkpoint file and its directory where supported, and atomically
   replaces the current bounded checkpoint envelope after preserving a previous
   good generation. The two file updates are not a multi-file atomic transaction. Ambiguous
   failures retain the in-memory journal and report unconfirmed durability
3. Only a durable typed commit receipt may acknowledge the world. It must verify
   both its boundary and full saved snapshot hash before calling
   `checkpoint_committed`. A stale asynchronous receipt must not clear any records;
   retain the old journal's actual base for the next save attempt
4. Opening a world validates the checkpoint envelope, restores the retained base, replays the
   exact journal, and verifies the recorded final state and compatibility. Corrupt,
   incompatible or old-format saves fail closed without a replacement world

These two crates provide serialization and boundary checks, not filesystem writes.
The separate `benilla-world-store` adapter owns the durable prepare/commit/receipt
transaction. The immutable-file example is not an atomic-save implementation.
Define the crash-loss policy for uncommitted ticks: periodic checkpoints do not
mean every intervening tick is durable. Format 1 to 2 has no silent migration.

XP config should map integer `Ruleset::xp_milli()` to runtime numerator and 1000
as denominator, without float conversion. The core supports the config's exact
0.001..1000 rate domain. Current guaranteed loot remains fixed: a drop multiplier
below 1 would require probabilistic loot, an explicit deterministic PRNG state,
native chance semantics and new tests. Reject that unsupported request rather
than silently ignore the configured rate or scale item quantity.

## Future peers

`Envelope` binds world/content/rules/client-manifest/frozen-membership hashes and
the exact base revision/state digest. Equal-tick divergent histories are refused.
Derive its owner from the authenticated transport session. Run `remote` before
putting the input in a tick batch, disconnect/report invalid peers without freezing
an unrelated solo tick, then pass only accepted `OwnedInput` values into `step`.
Never use self-asserted owner bytes or decoded result/state events as authority.

This is not a dynamic join protocol. A later peer must receive an authenticated
snapshot at `join_boundary`, match the content and rules, and participate in an
explicitly agreed membership transition. That transition is not implemented by
these crates. Independently progressed world revisions must remain separate;
there is no lossy offline merge or automatic winner selection.
