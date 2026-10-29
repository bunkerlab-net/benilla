# Solo world runtime

The separate source-bound native reconstruction is documented in
[NATIVE-QUEST-PROGRESSION.md](NATIVE-QUEST-PROGRESSION.md),
[NATIVE-INTERACTIONS.md](NATIVE-INTERACTIONS.md) and
[LOOT-INVENTORY.md](LOOT-INVENTORY.md). Selected combat-entry Unit events are
documented in [COMBAT-ASSISTANCE.md](COMBAT-ASSISTANCE.md). Selected default random
movement is documented in [WANDER.md](WANDER.md). Its capability remains closed;
the original fixture described below is a different runtime. The separately
selected [player mainhand path](PLAYER-MELEE.md) retains that closed admission.
Current native root envelopes are format57; older linked format51 descriptions
are historical schema, not accepted save versions. Existing callback subdomain
hash versions are not implicitly changed by the root format.

A pure-Rust, GPL-3.0-or-later, solo-first runtime for the original world-data
fixture. One selected owner/character starts and ticks immediately, without peer
configuration or quorum. No game install, realm, Bevy, browser, network, private
key, script interpreter, floating-point authority, or host clock is involved.
This is an original mechanic slice, **not CMaNGOS combat or Classic world parity**.

## Run the real loop

```sh
cargo test -p benilla-world-runtime
cargo run -p benilla-world-runtime --example solo_loop -- /tmp/benilla-new-world-demo
```

The output directory must not exist. The executable creates a world, accepts the
steward's quest, moves, attacks and casts, kills two respawning constructs, loots
both corpses, turns in the quest, writes versioned genesis/journal/final snapshot
files, reloads the final snapshot, and replays from genesis. Both recovered state
hashes must match. Expected rewards are 125 XP, 12 money and 3 practice tokens.
The demo's IDs and content are authored public test values, not credentials.

## API boundary

`World::new(data, rules, world_id, seed, players)` accepts one to sixteen genesis
players, each a 16-byte owner ID plus a 16-byte character ID. This matches the
profile/owned-transport identifiers without importing their secrets. All players
start with the same immutable ruleset stats/abilities, level one and empty bags.
Character appearance, names, racial/class starts and profile selection belong to
the app; they are not fabricated here. There is one active character per owner.

`World::step(next_tick, complete_batch)` commits one fixed tick. An empty batch is
valid in solo mode. The external adapter supplies authenticated owners, and must
never accept a network packet's self-asserted owner as authentication. No caller
can submit XP, items, quest rewards, health, arbitrary state or a spawn command.
Read-only accessors expose `state`, `player`, `npc`, `data`, `rules` and a state hash
for projection. `TickResult` contains command rejections and gameplay events.

Each complete batch is sorted by tick, owner, sequence and character. One command
per owner per tick bounds both movement and work. Sequences are contiguous and
persist across saves. Invalid world/tick/owner/character/sequence or duplicate
input rejects the entire tick with no mutation. A valid authenticated action
that fails game rules consumes its sequence and records its rejection, so every
replica sees the same result. The order within a tick is:

1. Respawn due NPCs in stable spawn order
2. Complete due casts in owner/character order
3. Apply the canonical command batch
4. Retaliate with tagged stationary NPCs in stable spawn order

Milliseconds round up to whole fixed ticks. Coordinates/ranges are thousandths of
a world unit, not metres. Movement is bounded Euclidean displacement, with no
collision, gravity, navigation, facing, line-of-sight or map transitions. An app
must not interpret this as a terrain-aware movement authority.

## Implemented semantics and deliberate deviations

- Fixed melee and direct-damage/heal abilities; range, mana, cast time, personal
  cooldown and global cooldown checks. Positive-time casts finish before commands
  on their completion tick and recheck the target then. Instant casts resolve in
  the command phase. Moving/attacking during casting is rejected. Fizzles retain
  the already-paid mana/cooldowns. Only self healing and NPC damage are supported
- Passive NPCs are unattackable. Stationary NPCs retaliate at fixed damage against
  their first damaging owner, only in their own melee range. There is no threat
  table, faction logic, chase, evade, crit, miss, resist, aura or equipment
- The first damaging owner gets all kill XP and kill-quest credit, regardless of
  the final attacker. This is a deliberate bounded tagging rule, not Classic
  grouping/attribution. Dead owners keep their tag until respawn
- Death is processed once. Corpse IDs include a monotonically increasing spawn
  generation. Death initializes guaranteed fixed money/plain items exactly once
  from validated, hashed `Rules::loot`. Each corpse keeps fixed zero-based slots;
  taking an item leaves `None` in that slot and never shifts the remaining indexes.
  The corpse's owner may open it within range, take money or an individual slot,
  and release it. Open/reopen grants nothing, including after restoring a save.
  Taking requires the matching active corpse generation; duplicate/empty takes
  reject without changing rewards, inventory or the item-instance counter
- The active loot window is persisted. Leaving interaction range, player death or
  corpse respawn releases it and emits `LootReleased`. Explicit release is safe
  after death/out of range and idempotent; a stale release never closes a newer
  window. Taking the last reward leaves an empty window until release. Opening an
  owned empty corpse is allowed; `looted` is the checked derived exhausted flag.
  All remaining corpse loot expires on respawn. Old inputs never redirect to the
  new generation. A failed open does not replace the current window
- `Command::Loot` remains an explicitly legacy original **headless** atomic API
  that drains only the stored remaining rewards. Stock interaction adapters use
  `OpenLoot`, `TakeLootMoney`, `TakeLootItem` and `ReleaseLoot`. No input can create
  rewards. The world-data model has no creature loot table. There are no random
  drops, hidden PRNG state or probability claims. Seed is reserved and persisted
  as part of world identity but remains unused by these mechanics
- Inventory contains original plain `InventoryItem` stacks with per-player
  monotonic nonzero `instance` IDs and stable zero-based bag `slot`s. Granting
  fills existing partial stacks in slot order, preserving their IDs, then uses
  the first free slots. IDs are allocated only after the entire grant succeeds;
  capacity/counter-overflow failures cannot partially merge, award money/XP or
  consume loot. `next_item_instance` starts at one and is persisted. The vector
  has canonical ascending slot order. This is not native equipment, bag types,
  native item GUIDs, durability, item stats or item-use behavior
- Original physical player melee now calls `benilla_sim::classic::armor_reduced_damage`.
  Its versioned integer/rational policy rounds down after the 75% cap and enforces
  a one-damage minimum. It documents boundary deviations from the pinned CMaNGOS
  intermediate-f32 formula; this is not complete combat parity. Rules version 2
  hashes the numeric-policy version. Original direct abilities bypass this armor
  path because native spell-school semantics are not represented. Player armor is
  not modeled, so this does not port NPC-versus-player armor handling
- Kill XP calls the existing licensed `benilla_sim::xp::base_gain` with its
  documented ties-to-even quantization, then applies an integer rational ruleset
  rate rounded down. Quest XP is the model's fixed reward times that same rate.
  This is not full CMaNGOS `Gain`, rested/group/elite XP or quest XPValue behavior
- Total-XP level thresholds in `Rules::original_camp` are original fixture values,
  not the Classic XP table. Level-up does not change stats, restore resources,
  teach abilities or provide talents. Skills, professions, trainers and learning
  spells are unsupported; starting known abilities are ruleset-defined
- Quests have acceptance level gates, start/finish creature relations, up to four kill
  objectives, distance checks and nonrepeatable turn-in. No retroactive credit.
  Reward insertion is atomic and respects plain-item stack/bag bounds. Completed
  quests are retained to prevent duplicate rewards; chains, prerequisites beyond
  represented level gates, timed/repeatable quests, dialogs and scripts are absent
- Dead players cannot perform gameplay actions; releasing stale loot UI remains
  safe. An explicit revive command after a ruleset delay restores health/mana at the configured start. This is an original test policy,
  not Classic corpse recovery, spirit healers, durability loss or resurrection

The runtime refuses `local_cmangos_classic` source data and nonzero faction
templates. Only original content may currently exercise the integer armor hook. The importer already rejects unknown/unsupported features.
Future real content must gain reviewed rules/data representations; relabeling
Classic rows as an original fixture does not make them supported or licensed.

## Persistence and future network boundary

`snapshot_bytes` emits canonical versioned integer-only JSON, covering world ID,
revision, tick, seed, owner/character state, inventory, quests, XP/level, cooldowns,
casts, NPC generations, death/loot/respawn state, content/rules hashes, journal tip
and a snapshot checksum. Format 2 and the v2 state/snapshot/record hash domains
cover partial corpse slots, the active loot window, item instances and allocator
counter. Format 1 saves/journals are rejected, with no implicit migration. The
existing Rules version 2 and integer armor numeric policy are unchanged.
`restore`/`restore_reader` check compatibility, checksum,
canonical bytes and state invariants. Unknown fields, duplicate keys, bounds and
unknown references fail closed. Restore also checks live NPCs have no corpse
rewards, each remaining corpse slot exactly matches its fixed-rule entry, money is
unclaimed or zero, `looted` matches exhaustion, active windows name owned in-range
dead generations, bag slots/instances are unique and IDs precede the next counter.
These are state invariants, not proof of an unforgeable reward history. The
input-byte limit bounds deserialization,
which is not constant-memory parsing. Checkpoint bytes contain no private keys.

`journal_bytes` includes the exact base/final boundaries, chain tip and ordered
inputs/outcomes/events/state hashes for every tick. `replay` re-executes and checks
all of them, including the final boundary. After a storage layer durably commits
`snapshot_bytes`, it can call `checkpoint_committed(&persisted_boundary)` to begin the next journal.
The acknowledgement must match the current boundary; an old asynchronous save
cannot discard newer ticks. `checkpoint_due` warns at half the journal budget.
These are serialization/replay primitives, not a production filesystem store.
The example writes new immutable files and fsyncs them, but production save
replacement/recovery, directory durability and concurrent-writer exclusion belong
to the app's storage layer. A full journal stops advancement without mutation,
requiring a committed checkpoint; it never silently drops history.

Checksums detect corruption and divergence, not cheating or trusted authorship.
Someone able to rewrite the entire snapshot/journal can forge or roll back it;
a trusted external checkpoint is needed to establish authenticity/freshness.
There is no off-line merge of divergent worlds. `join_boundary` returns the exact
world/revision/tick/state/content/rules hashes that a future authenticated snapshot
transfer may target. It implements neither membership admission nor networking,
consensus, migration or host election. Multi-player tests prove same-batch-order
independence only; they do not prove distributed agreement.

Hard bounds: 16 players, 4096 NPCs, 64 known abilities, 64 lifetime quest entries per
player, 64 bag slots, 2048-byte input, 8 MiB snapshot, 8 MiB journal and 10,000 ticks
per journal. IDs, map/position, timers, rows and progression are checked. Sorted
vectors/BTree collections define serialization and iteration order.

## License and provenance

All runtime code/fixtures here are original GPL-3.0-or-later. The CMaNGOS-derived calculations invoked are the dependency's XP primitive and
versioned integer armor reformulation; upstream provenance,
GPL notices and deviations remain in `../benilla-sim/PROVENANCE.md`. This crate
adds no Blizzard data/assets, copied database rows or unlicensed implementation.

### Loot workflow provenance and scope

The interaction separation follows the GPL-2.0-or-later CMaNGOS reference at
[`LootHandler.cpp`, revision `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Loot/LootHandler.cpp):
`HandleLootOpcode` shows content, `HandleAutostoreLootItemOpcode` requests a fixed
slot, `HandleLootMoneyOpcode` requests gold, and `HandleLootReleaseOpcode` releases
loot. This runtime is an original bounded implementation of that interaction
shape, not a line-by-line native loot port or Classic loot parity. Its fixed
integer range, first-damaging-player ownership, guaranteed drops, plain bag
stacks, empty-slot rejection/window retention, corpse expiry and lack of
cast-interruption on open are explicit original policies. Native loot tables,
item rules, group rolls, master loot, quests-as-drops and game-object loot remain
unsupported. This workflow change does not lift the `local_cmangos_classic`
source gate or change the integer armor policy.

## Selected source spell host

See [SPELL-AUTHORITY.md](SPELL-AUTHORITY.md) for the bounded ID133 source spell
host, complete raw input dependencies, phase/scheduler ownership, numeric oracle
and native simulation checkpoint/record format 74. This is not production content
admission or stock Fireball certification.

See [FACING-AUTHORITY.md](FACING-AUTHORITY.md) for explicit current-request
Bearing, mutual melee arcs and caster spell facing derivation with pinned
software numeric policy. Other spatial dependencies remain external.

See [initial NPC collision-height ownership](CREATURE-COLLISION-HEIGHT.md) for the
explicit source-owned height catalog derived from the retained width/model source,
atomic motion-height initialization, generation ownership and persistence boundary.


See [owner pathfinding bit receipts](src/native/README.md#owner-pathfinding-bit-receipt)
for bounded runtime issuance from source/control audits and live executable abilities,
required-option v33 Route evidence, replay guarantees and the pinned source guard.
This supplies only IGNORE_PATHFINDING, not config, calculate completion or native admission.

Native format 67 retains exact selected ordinary-instance destruction even when
a registered original with the same entry remains carried. The selected receipt
must preserve every registered instance; unselected quest consumption and
registered-item destruction remain unsupported. No old-save migration or native
admission is provided.

Native format 68 (`benilla-native-source-thread-spline-ids-v68`) requires the optional shared
spline-ID owner. Its bounded source-thread domain includes actual NPC allocation
callbacks and the first-life player forced stop; profiles outside that domain
retain the older partial motion lane. Compact endpoints prove reachable
interleaving, while TickRecord replay establishes callback order. No old-save
migration, ghost/recovery capability, or native admission is provided.

Native format 69 (`benilla-native-player-appearance-v69`) retains an optional immutable
selected-character appearance owner from exact loaded source-row choices.
`PLAYER_BYTES` projects only its four owned cosmetic bytes; facial hair remains
retained without inventing the bank/rest siblings of `PLAYER_BYTES_2`. No old-save
migration, native admission, corpse creation, or ghost release is provided.

Native format 70 (`benilla-native-player-hide-flags-v70`) retains source hide-helm/hide-cloak
XOR commands under the combined initial-control, spell and fresh-rest owner
contract. The full player-flags descriptor is not inferred for other profiles.
No old-save migration, native admission, guild state, or ghost release is added.

Native format 71 (`benilla-native-map-classification-v71`) binds optional exact loaded Map.dbc classification
to the native profile map identity. Classification is not geometry, admission,
or evidence that a corpse was created. No old-save migration or ghost release
is added.

Native format 72 (`benilla-native-armor-destroy-v72`) adds selected detached-original armor destruction
with retained tombstone custody; registered identities remain reserved. Generic
and indirect consumption remain unsupported. No old-save migration, native
admission, or ghost release is added.

Native format 73 (`benilla-native-mainhand-membership-v73`) binds bounded original mainhand membership
to source-ordered equipment observations and retained weapon fields. This does
not admit generic weapons, in-combat switching, or held private consumers.
No old-save migration or native admission is added.

Native format 74 (`benilla-native-mainhand-auto-equip-v74`) binds bounded original mainhand AutoEquip for singleton types 17/21
to existing source-ordered membership and retained weapon fields. This does
not admit generic weapons, in-combat switching, or held private consumers.
No old-save migration or native admission is added.
