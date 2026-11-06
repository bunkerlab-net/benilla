# Solo world runtime

The separate source-bound native reconstruction is documented in
[NATIVE-QUEST-PROGRESSION.md](NATIVE-QUEST-PROGRESSION.md),
[NATIVE-INTERACTIONS.md](NATIVE-INTERACTIONS.md) and
[LOOT-INVENTORY.md](LOOT-INVENTORY.md). Selected combat-entry Unit events are
documented in [COMBAT-ASSISTANCE.md](COMBAT-ASSISTANCE.md). Selected default random
movement is documented in [WANDER.md](WANDER.md). Its capability remains closed;
the original fixture described below is a different runtime. The separately
selected [player mainhand path](PLAYER-MELEE.md) retains that closed admission.
Current native root envelopes are format`NATIVE_FORMAT_VERSION`; older linked format51 descriptions
are historical schema, not accepted save versions. Existing callback subdomain
hash versions are not implicitly changed by the root format.
The current `NATIVE_FORMAT_VERSION` rejects previous profiles, states, and
checkpoints even when their outer hashes are recomputed. The retained combat
policy binds later host changes as well as the initial configuration.

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
and native simulation checkpoint/record format `NATIVE_FORMAT_VERSION`. This is not production content
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

Native format `NATIVE_FORMAT_VERSION` retains exact selected ordinary-instance destruction even when
a registered original with the same entry remains carried. The selected receipt
must preserve every registered instance; unselected quest consumption and
registered-item destruction remain unsupported. No old-save migration or native
admission is provided.

Native format `NATIVE_FORMAT_VERSION` (`NATIVE_POLICY`) requires the optional shared
spline-ID owner. Its bounded source-thread domain includes actual NPC allocation
callbacks and the first-life player forced stop; profiles outside that domain
retain the older partial motion lane. Compact endpoints prove reachable
interleaving, while TickRecord replay establishes callback order. No old-save
migration, ghost/recovery capability, or native admission is provided.

Native format `NATIVE_FORMAT_VERSION` (`NATIVE_POLICY`) retains an optional immutable
selected-character appearance owner from exact loaded source-row choices.
`PLAYER_BYTES` projects only its four owned cosmetic bytes; facial hair remains
retained without inventing the bank/rest siblings of `PLAYER_BYTES_2`. No old-save
migration, native admission, corpse creation, or ghost release is provided.

Native format `NATIVE_FORMAT_VERSION` (`NATIVE_POLICY`) retains source hide-helm/hide-cloak
XOR commands under the combined initial-control, spell and fresh-rest owner
contract. The full player-flags descriptor is not inferred for other profiles.
No old-save migration, native admission, guild state, or ghost release is added.

Native format `NATIVE_FORMAT_VERSION` (`NATIVE_POLICY`) binds optional exact loaded Map.dbc classification
to the native profile map identity. Classification is not geometry, admission,
or evidence that a corpse was created. No old-save migration or ghost release
is added.

Native format `NATIVE_FORMAT_VERSION` (`NATIVE_POLICY`) supports selected detached-original armor destruction
with retained tombstone custody; registered identities remain reserved. Generic
and indirect consumption remain unsupported. No old-save migration, native
admission, or ghost release is added.

Native format `NATIVE_FORMAT_VERSION` (`NATIVE_POLICY`) binds bounded original mainhand membership
to source-ordered equipment observations and retained weapon fields. This does
not admit generic weapons, in-combat switching, or held private consumers.
No old-save migration or native admission is added.

Native format `NATIVE_FORMAT_VERSION` (`NATIVE_POLICY`) binds bounded original mainhand AutoEquip for singleton types 17/21
to existing source-ordered membership and retained weapon fields. This does
not admit generic weapons, in-combat switching, or held private consumers.
No old-save migration or native admission is added.

Native format `NATIVE_FORMAT_VERSION` (`NATIVE_POLICY`) binds bounded carried-original-mainhand relocation and AutoStore
to existing exact custody receipts without equipment or stat callbacks. This does
not admit generic consumers, mainhand destruction, or held private consumers.
No old-save migration or native admission is added.

Native format `NATIVE_FORMAT_VERSION` (`NATIVE_POLICY`) binds selected destruction of detached pristine original mainhand
to exact quest callbacks and permanent identity tombstones without stat callbacks.
This does not admit generic consumption or held private consumers.
No old-save migration or native admission is added.

Native format `NATIVE_FORMAT_VERSION` (`NATIVE_POLICY`) binds optional explicit host-selected health regeneration
to validated standing-mage owners and the shared source regeneration cadence.
No rate is inferred when unset; no external server configuration is claimed.
No old-save migration or native admission is added.
See [PLAYER-HEALTH-REGENERATION.md](PLAYER-HEALTH-REGENERATION.md) for the
explicit configuration boundary, modifier closure, numeric oracle and conditional
component-entry limitation.

Native format `NATIVE_FORMAT_VERSION` (`NATIVE_POLICY`) binds bounded player sheath controls
to qualified fresh-player and no-enchantment owners, with field 164 composition.
Unsupported profiles do not gain source-backed sheath state.
No old-save migration or native admission is added.

See [PLAYER-SHEATH.md](PLAYER-SHEATH.md) for exact player field164 composition,
current source owner closure and conditional UI/controller evidence boundaries.

Native format `NATIVE_FORMAT_VERSION` (`NATIVE_POLICY`) binds bounded occupied-backpack exchange
to exact both-endpoint original custody and existing source swap receipts.
No equipment replacement or generic consumption admission is added.
No old-save migration or native admission is added.

Native format `NATIVE_FORMAT_VERSION` (`NATIVE_POLICY`) binds optional source-qualified
static creature identity and committed-pose cell observations. These are not source
CurrentCell, loaded-grid, registration, lookup or selection authority.
No old-save migration or native admission is added.

## Source clock and scheduler components

`native::source_execution` consumes recorded, owner-qualified clock samples.
`RunnableClock` uses `benilla_sim::source_time`, not the native fixed-frame clock.
World and map calendar observations have separate owners. Timezone and localtime
conversion bindings remain `ClockSchedulerPending::CalendarTimezoneAndConversion`.
No host clock or host timezone supplies a missing observation.

At CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

- `World.cpp:1511-1702` orders the world phases. Conditional manager work belongs
  to each `WorldWork` handler. Unsupported handlers must refuse.
- `MapManager.cpp:194-227` accumulates the entire map diff, submits maps in
  `MapManager.h:40-45,63` key order, waits, checks unloading, then clears time.
  With no active workers it calls maps inline.
- `MapUpdater.cpp:24-58,66-99` owns worker births, pending work, completion and
  joins. `ProducerConsumerQueue.h:64-78` removes the FIFO front. A recipient choice
  cannot select a later job. Completion order need not equal submission order.
- `MapManager.cpp:92-110` launches continent initializers on separate async
  threads before waiting for both. Their lifetimes are not map-worker lifetimes.
- `Messager.h:30-45` swaps a batch before executing it. Nested enqueue waits for a
  later batch. `GameEventMgr.cpp:828-864` forwards world work to map work.
  `GameEventMgr.cpp:1024-1033` skips an absent current creature lookup, whereas
  `GameEventMgr.cpp:843-845` asserts missing spawn data. A wrong map generation
  is a refusal, not a general stale-message drop.

The source components do not replace `engine.rs`, admit native execution, change
persistence, or implement the F4 message/manager handlers. `MessageWork` receives
the dispatching logical thread so callback draws use that thread's RNG owner.
`CLOCK_SCHEDULER_PENDING` names missing sealed owners, calendar conversions,
world inputs, generations, startup births, complete job/barrier causality and
message captures/handlers.

`BENILLA_F1_CLOCK_TRACE` enables the retained original-prefix test. Its versioned
`OriginalUnsealedSchedulerV1` projection uses the F3.02 comparator's distinct
diagnostic operation type. `TraceReader::open` remains F0-only. The first 62 job
records match. At original record 15,310,184, dispatch of job 22 appears before
job 21. The source logs dispatch outside the FIFO pop lock, so that observation
cannot supply pop linearization. The report preserves the first difference,
stops native comparison and continues only raw observation counts. It observes
52 submissions, 52 dispatches, 51 completions and one outstanding job, never a
fabricated completed barrier.

The 52 map begin/end pairs lack world diff and generation bindings for independent
map-update comparison. No `world_tick` records exist in this prefix. These remain
typed pending obligations, not world/map matches. The adapter invents no F0
envelopes or causal edges. A missing default private trace prints `SKIP`; an
explicit missing or different trace fails. Run with
`mbx test --locked -p benilla-world-runtime --lib source_execution -- --nocapture`.

The separate seal8-08 capture contains 464 `world_tick` records and 468 map
begin/end pairs with checked world and map-generation contexts.
`BENILLA_F1_SEAL8_FRAMING` selects its retained `seal8-full-framing-08` directory.
The diagnostic anchors the chunk manifest and every binary chunk, recomputes
world timer arithmetic and map interval dispatches, and checks map-end owners
against their actual begins. It compares those values through distinct F3.02
diagnostic operations, not F0 envelopes. Map bodies, end live-object counts,
calendar conversions, manager phases and in-lock FIFO pops remain unproved.
Both captures remain unsealed; their hashes, inputs and reports stay separate.

## Source GUIDs and registration

`source_execution::{guids,objects,cells}` separates pre-GUID construction,
GUID assignment, legacy life, cell linkage, in-world membership and live lookup.
It does not derive registration from `CreaturePoseCellIndex`. Pose observations
can compare against an independently populated registry but cannot mutate it.

The pinned source contract is:

- `ObjectGuid.h:59-88,103-119,168-185` defines high bits, both constructors,
  zero-counter behaviour and entry-dependent counter masks. Entry/counter overlap
  is not silently masked. `ObjectGuid.cpp:64-71` signals `StopNow` near overflow
  and still returns the pre-increment counter.
- `ObjectMgr.cpp:6781-6855` seeds player/item/corpse and static creature/GO
  ranges from source queries. `ObjectMgr.h:814-819` owns global allocation.
  `Map.cpp:172-173,2546-2563` owns map starts and the shared GO/transport counter.
  Recreating a grid cannot recreate its map allocator.
  Event spawn helpers request counter zero at `Object.cpp:2183,2207`;
  `Creature.cpp:1662-1672` and `GameObject.cpp:910-921` apply dynamic/group rules.
- `Object.cpp:48-59,115-125` constructs before GUID assignment. Its destructor
  refuses an in-world object at `61-68`. Creature legacy revival at
  `Creature.cpp:717-731` does not construct another object.
- `GridDefines.h:68-70`, `Map.cpp:237-328` and
  `TypeContainerVisitor.h:55-58` define the separate ordered type lists.
  `GridReference.h:31-35,58-61` inserts at the front and unlinks on destruction.
  A cell's lifetime follows its containing grid generation.
- `Map.cpp:1219-1241` preserves same-cell order and rejects an inactive move
  into an unloaded object grid. Active cross-grid movement invokes the loader
  seam before relinking. Viewpoint callbacks remain a reached owner obligation.
- `Creature.cpp:173-218`, `Pet.cpp:70-86`, `GameObject.cpp:115-120,144-168`
  and `DynamicObject.cpp:38-56` own map lookup lifetimes. Player accessor
  registration is separate at `CharacterHandler.cpp:832`; corpse accessor
  membership follows `Corpse.cpp:47-62`. Database lookup preserves insertion
  order and prioritises alive creatures, `Map.cpp:2426-2452,2515-2523`.

The seal8-08 object diagnostic uses observed `_Create` arguments only as a
GUID-composition fixture. It compares subsequent add counters and the full raw
GUID observed in object-update envelopes through the F3.02 comparator. Constructor
and destructor counts are observations, not an independently executed world
construction history. Allocator input joins, registration/cell state, native
generation associations and reached callbacks remain typed pending obligations.
No expected construction list drives gameplay. Native admission and persistence
remain unchanged.

## Reference PathFinder distance diagnostics

`ReferenceDistanceStorage` reads only the owner-approved unwritten-distance
family. Its independently supplied file receipt binds the bytes and record
count. Each read must match caller, logical thread, live-object generation,
thread occurrence, four-byte width and source event. Failed reads do not advance
the cursor. `finish` refuses surplus inputs. Both scalars declared at
`PathFinder.cpp:410` remain distinct, alongside declarations at 717, 830 and 1400.
Signed zero and NaN object representations are not canonicalized.

`DifferentialOracle::compare_path_distance_lookup` compares separately computed
nearest-query inputs and results, then the reference byte-driven append against
the native absent-distance predicate. Defined active/tail distances must match
bit for bit. Different polygons, query inputs, distances or native predicates
refuse. The ordinary strict trace comparator still stops at the first differing
append and does not waive later differences.

`compare_seal8_unsealed_distances` produces a two-part conditional predicate
report from hash-anchored seal8-08 geometry observations and live logical births.
The strict part counts equal predicates only before the first recorded
difference. The intent part counts all conditional absent-distance differences.
These observations are not native geometry executions. The report leaves
strict P0/P1/P3 agreement, independently executed native nearest geometry and a
reviewed post-deviation oracle identity typed pending. It never supplies
captured geometry or storage bytes to native gameplay. No whole-prefix or
post-deviation gameplay equality is claimed.

Set `BENILLA_F1_DISTANCE_STORAGE` to round 5's `initial-storage-input.jsonl`
and `BENILLA_F1_SEAL8_FRAMING` to `seal8-full-framing-08` to run the retained-data
tests in `source_execution::path_distance`. Absent default private data skips
with an explicit message. An explicit missing, changed or malformed capture
fails. The two captures retain separate identities and counts.

## Source grid loading and camera visits

`source_execution::SourceGrids` owns NGrid generations and front-linked order.
`SourceCells` uses that same registry for registration, product-load receipts and
the object-data-loaded bit. Creation or terrain products alone do not make a grid
object-loaded. Grid configuration must bind before a real load can begin.

The CMaNGOS Classic `8ec338a1` contract is:

- `Map.cpp:336-403` links a new IDLE grid, converts product coordinates with
  `63 - internal`, and sets object-data-loaded before invoking factories.
  `NGrid.h:40-64,89-121` owns initial expiry, explicit locks and the separate bit.
  `GridReference.h:31-35` inserts at the front.
- `Map.cpp:151-152` sets the product flag from the terrain owner's returned
  GridMap pointer. `GridMap.cpp:726-742,1253-1306` retains that pointer even after
  a map-file load failure. The flag does not prove successful geometry content.
- `ObjectGridLoader.cpp:114-264` visits x outside y, grid types before world
  types, GOs before creatures, and each static GUID set before its persistent
  set. Each set sorts uint32 GUIDs. Duplicate static/persistent attempts remain.
- `Map.cpp:357-381,419-427,1582-1583` supplies entry, forced and active-object
  load triggers. Only a newly loaded entry grid gets the shortened expiry and
  ACTIVE state. An already-loaded forced grid does not acquire an explicit lock.
- `Camera.cpp:147-151` and `CellImpl.h:54-169,197-209` add bounding radius, cap
  distance, visit the standing cell first for small ranges, and use octagon
  strips for large ranges. Complete grid and world passes remain separate.
  `Map.h:594-605` preserves NoCreate and skips cold object grids.

`SourceGridLoader` consumes registered static/persistent sets and exact retained
F2 GUID/ordinal rows. It does not instantiate every catalog definition. One owned,
checked row frame survives recursive factory work. Product, constructor,
LoadFromDB, failed-candidate deletion, corpse and viewpoint owners are mandatory.
The module supplies no production no-op factories. Reached unavailable owners
refuse, and a blocked load cannot silently resume as a successful one.

The separate seal8-08 diagnostic labels member arguments as component fixtures.
It compares initial grid state, object-load branch eligibility, converted VMap
request associations and the actual forced UnloadAll visit order through F3.02.
It does not treat an unload visit as completed deletion. The extra VMap request
remains separately counted. Geometry execution, native generations, registration,
trigger causality, natural unload/reload and whole-world admission remain pending.
Set `BENILLA_F1_SEAL8_FRAMING` to the retained framing directory to exercise that
unsealed projection. An absent default path prints SKIP; an explicit bad path fails.

## Source gameobject constructors

`GridFactoryContext::load_gameobject` executes ordinary GO construction through
the same reentrant factory and authoritative object/cell registries as grid
loading. `GameObjectFactoryOwner` supplies bound source definitions, manager
lookups, calling-thread RNG, persistent respawn storage, calendar observations,
geometry, and executable callbacks. It has no production no-op implementation.

The pinned CMaNGOS Classic `8ec338a1` contract is:

- `ObjectGridLoader.cpp:123-141` chooses the factory from the database entry
  before group/random overrides and destroys a failed candidate.
- `GameObject.cpp:178-302,880-969` owns entry/GUID selection, transform/state,
  permanent defaults, positive saved respawn, and negative no-draw respawn.
  `ObjectMgr.h:600-604` draws signed `irand` even for a singleton random list.
- `GameObject.h:590-594` retains database `goState` as signed32.
  `GameObject.cpp:926-927,2077-2079` preserves `-1` as the no-override sentinel
  and writes the selected state byte only through `SetGoState`.
- `GameObject.cpp:230,2083-2105` constructs and inserts the collision model during
  `SetDisplayId`, before initial state and create callbacks. A missing dynamic
  collision owner refuses there, not after a fabricated world addition.
- `Map.cpp:470-504` and `GameObject.cpp:115-141` register cells and lookup before
  world membership, enable collision, construct linked objects, activate, and
  issue viewpoint/visibility callbacks. `GameObject.cpp:1244-1271,2528-2536`
  initializes linked-trap AI before parent AI and requires the instance callback.

The repository has no production dynamic GO model/tree insertion owner.
Constructor tests use explicitly authored model operations to check ordering;
they don't establish F6 geometry or playable construction. Missing elevator,
pet, corpse, dynamic-object, and transport attachment constructors refuse at
their named owners. Transport movement is not ordinary GO update. Reached
scripts, geometry, bound catalog decoding, and owner-specific attachments remain
mandatory dependencies. Active membership and lock changes use the real source
owners described below. No native admission, persistence,
or format change follows from these constructor mechanisms.

`compare_seal8_unsealed_gameobjects` counts GO/other identity and destructor
observations and compares only recomposed GO/transport GUIDs against add/update
records. It reports independently executed constructors as zero and identifies
the first unavailable constructor binding separately from a GUID difference.
It never uses captured constructor outcomes as runtime inputs. Set
`BENILLA_F1_SEAL8_FRAMING` to run the unsealed seal8-08 comparison. Absent default
data prints `SKIP`; an explicit invalid path fails.

## Source creature construction

`GridFactoryContext::load_creature` calls `load_source_creature` from the real
grid factory. `CreatureFactoryOwner` supplies raw definition and manager lookups,
the map allocator, the calling source RNG, persistent respawn state, an
owner-qualified wall-clock sample, and executable handlers. No default handler
returns success. An unavailable handler returns its phase-specific refusal.

The pinned `8ec338a1` source contract is:

- `ObjectGridLoader.cpp:133-140` constructs before the event-GUID check and
  deletes a candidate after a source `LoadFromDB` failure.
- `Creature.cpp:1623-1677` skips living duplicates, resolves group, conditional,
  random and event fallback entries, validates the template, and allocates the
  GUID. The random fallback uses `GetDbGuid()`, still zero before `_Create`.
- `Creature.cpp:1602-1610,322-324` assigns the GUID using the original entry
  before `InitEntry` applies an event entry override.
- `Creature.cpp:346-358,657-702,1273-1279` selects display, conditional gender,
  and conditional level. Equal display bounds still draw. Equal levels do not.
  `ObjectMgr.cpp:1615-1632` retains the original model when a selected alternate
  gender model is absent. A missing model fails after earlier draws.
- `Creature.cpp:1690-1718` draws respawn delay before persistent lookup and
  wall-clock comparison. Future dynamic spawns without a group fail. Static or
  grouped future spawns become dead. An expired nonzero time writes zero.
  `Object.cpp:3076-3079` defines new spawning by unequal database/runtime counters.
- `Map.cpp:470-504` registers the cell and world membership before active,
  viewpoint and visibility work. `Creature.cpp:880-895,1738-1765` then orders
  motion, AI, relay, linking, formation, group and rabbit-day handlers.
- `Creature.cpp:158-170` runs formation and Unit cleanup before deletion.
  A reached unsupported owner stops execution instead of inventing source failure.

`resolve_supported_creature_kernel` reuses `creature_init` arithmetic at the
SelectLevelStats boundary with the already consumed rolls and matching operands.
It does not run callbacks, move objects, or apply addon effects early.
The legacy `creature_generation` cold schedule is not a source loader.

The seal8-08 constructor diagnostic scans the hash-bound framing with an explicit
unsealed label. It counts observed creature `_Create` identities separately from
generic constructions. These observations do not bind the loader's definition
and manager input occurrences. The report therefore accepts no complete
constructors and identifies the first unavailable owner without using expected
entries, GUIDs or draws as gameplay inputs. The reported reference event is the
first observed creature identity, not a fabricated `LoadFromDB` entry record.
Set `BENILLA_F1_SEAL8_FRAMING` to exercise this diagnostic. Absent default data
prints SKIP. Explicit missing or changed evidence fails.

Bound active-lock execution, full F4/F5 handlers, F6 geometry, sealed input
occurrences and whole-startup acceptance remain required. Capability and format
stay unchanged.

## Source active objects and relocation

`GridFactoryContext` owns executable active-state requests, map additions and
removals, creature movement and respawn relocation, and GO movement. Its loaders
and callbacks reenter the same object/cell/grid registries. The creature and GO
constructors call these owners instead of accepting empty active callbacks.
`SourceObjects` retains authoritative pose, respawn definition presence, active
flag and source-allocation bindings. `SourceCells::active` stores the actual
pointer-ordered membership and mutation-safe next iterator.

The pinned `8ec338a1` source contract is:

- `ObjectMgr.cpp:2040-2045,2233-2242,9132-9157` adds eligible ordinary active
  spawns in loader occurrence order. Creatures precede GOs during preload.
  A dynamic GO without a game event takes an earlier branch and cannot enter
  ordinary preload. A dynamic creature has no corresponding exclusion.
  `ObjectMgr.h:1351` uses a multimap, so equal-map keys retain insertion order.
- `Map.cpp:419-427` sets an explicit lock only when ForceLoadGrid enters its
  initially object-unloaded branch. `NGrid.h:40-64` keeps that lock separate
  from the uint16 active count. Increment wraps at that width. Decrement at
  zero stays zero. GridUnload=false starts with an explicit lock.
- `Object.cpp:2566-2580` ignores repeated flag requests and never deactivates
  players. In-world activation inserts before assigning the new flag.
  `Creature.cpp:204-206`, `GameObject.cpp:139-141` and `Map.cpp:496-497` can
  reach AddToActive twice during one addition. Duplicate set insertion does
  not suppress the second spawn-lock increment.
- `Map.cpp:1579-1639` loads the current grid but locks the respawn grid for a
  non-pet creature with database spawn data. `Creature.cpp:2740-2744` checks
  definition presence, not equality of database and runtime GUIDs. A missing
  respawn grid produces a typed error-path outcome and does not create it.
- `EntitiesMgr.h:43`, `Map.cpp:945-952,1609-1618` define pointer-set order and
  advance the iterator before callbacks. Removing its next member advances
  again. Inserting after the iterator reaches end does not restart the visit.
- `Map.cpp:1546-1576` expands inclusive grid cell bounds by
  `ceilf(visibility / cell_width) + 1`. The upper bound starts one cell beyond
  the grid. Both linked map players and active objects prevent unloading.
- `Map.cpp:1166-1183,1219-1265` refuses an inactive cross-grid move into cold
  object data. An active move calls the real loader. Registration changes
  before ViewPoint notification and before position changes. Failed moves
  try respawn after combat-stop and motion-clear, then initialize motion and
  notify relocation only when that move succeeds.
- `Map.cpp:1186-1216` has separate GO absent/cold-grid branches and orders
  registration, viewpoint, pose, collision-model movement and visibility.

Source allocation bindings validate live, nonoverlapping extents and object
offsets. Active order uses those private source pointers, never native host
addresses, GUIDs or construction handles. Missing allocation identity refuses.
These component bindings do not certify the still-unbound F1 allocation inputs.
The forced-map preload branch requires the actual unordered CreatureDataMap
traversal owner from `ObjectMgr.h:162`; it refuses that unavailable owner rather
than sorting the catalog. Normal preload executes its registered occurrence
order. Missing relocation, motion, viewpoint, collision or visibility handlers
refuse at the reached source phase.

`compare_seal8_unsealed_active` compares uint16 lock arithmetic from zero
constructor fixtures keyed by observed GridInfo pointer. It does not bind
pointer reuse to NGrid generations or derive membership from active_add records.
The first count difference stops arithmetic comparison without resynchronising.
Active additions, explicit locks and relocation-named records remain separate
observations. Actual active-change acceptance stays zero while construction,
allocation, definition, grid-lifetime and handler causes are unbound.
`BENILLA_F1_SEAL8_FRAMING` selects the retained unsealed evidence. An absent
default prints SKIP; an explicit invalid or changed capture fails.

## Source update membership and pinned traversal

`collect_source_updates` derives membership from the live object, cell and active
registries. `SourceVisitInputs` supplies actual visibility data and PLAYER_FARSIGHT
GUIDs. It cannot supply cells, member lists or a permutation. The collector starts
after the earlier Map::Update message, spawn-manager, transport and player phases.
Those phases and entity Update implementations remain separate mandatory owners.
`SourceObjectUpdateOwner` executes each collected object's Update with the map diff.
Dispatch returns the closed set. The Map owner must retain it through the later
phases to scope end, as `Map.cpp:1006-1031` does. Further mutation/traversal refuses.
There is no default successful handler or production engine cutover.

Pinned CMaNGOS `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f` defines:

- `Map.cpp:667-689,894-927` visits player and resolved farsight rectangles,
  x outside y, with the object's visibility distance. `CellImpl.h:37-49`
  computes whole-cell bounds without a bounding radius or visibility-circle test.
  `Map.cpp:2382-2402` resolves registered creature, pet, GO and dynamic-object
  targets before AddToWorld, but requires players and corpses to be in world.
  `Map.cpp:670` uses the map radius for a target not yet in world.
- `Map.cpp:678-686,978-988` marks cells before visiting grid then world lists.
  Marked cold cells stay marked. `Map.h:594-605`, `Map.cpp:660-663` make NoCreate
  depend on object-data-loaded state, not terrain or NGrid existence.
- `GridDefines.h:68-83`, `TypeContainerVisitor.h:55-65` order the type lists.
  `GridNotifiers.cpp:185-189`, `GridNotifiersImpl.h:43-46` insert through both
  updater overloads. `GridNotifiers.h:119-122` excludes players, all corpses
  including bones, and cameras. Pets use the world creature list.
- `Map.cpp:945-992` advances the live active cursor before processing, skips
  out-of-world or invalid-position actors, inserts each active actor directly,
  then visits its neighbours using the map visibility distance.
- `EntitiesMgr.h:44`, `Map.cpp:995-1000` iterate the pointer unordered set without
  another visibility, world-membership or cell-membership filter.

`SourceUpdateSet` translates GNU libstdc++ GCC 12.2's unique pointer set.
Logical indices replace node addresses, preserving the before-begin sentinel,
bucket predecessors and linked iteration. `hashtable.h:1985-2008,2059-2083,
2144-2164,2231-2256,2560-2591` defines insertion, duplicates and rehash relinking.
`functional_hash.h:107-112` hashes the source pointer as its integer value.
`hashtable_policy.h:478-488,500-545` defines modulo buckets, default load one
and growth factor two. `hashtable_c++0x.cc:46-87,101-122` and
`hashtable-aux.cc:30-45` derive the default growth sequence. The supported
100,000-member resource bound refuses larger sets before insertion.
Map::Update never reserves, erases or changes this local set's load factor.
Insert and emplace share the membership algorithm. A duplicate emplace's
temporary allocation cannot change the existing chain.

The node-to-index mapping preserves each next link and bucket predecessor.
Empty-bucket insertion updates the head and its former bucket predecessor.
Nonempty-bucket insertion changes only that bucket's first link. Rehash walks
the old mapped chain and applies the same head-insertion transitions.
These are all membership mutations in the local source set, so induction over
the insertion history preserves its complete iteration order. Node allocation
addresses cannot affect pointer hashing or those links. Empty sets allocate
neither nodes nor buckets. The map retains and clears its cell-mark bitmap.

The three headers were extracted from Debian
[`libstdc++-12-dev_12.2.0-14+deb12u1_amd64.deb`](https://deb.debian.org/debian/pool/main/g/gcc-12/libstdc++-12-dev_12.2.0-14+deb12u1_amd64.deb),
SHA-256 `d28def6c23630432b57cb38a4c2fd67a79d4e0484027386ca6e8d6005c3d7a73`.
Their SHA-256 identities are:

- `hashtable.h`: `d1cad098a5169c9bcb04ce63a58697b91f3f441b5f360e64a01e23ba3bed8a9c`.
- `hashtable_policy.h`: `59d76541faeda87c8936f4b0455ed09d14cae3b39fda2141815e4f4091b86ac0`.
- `functional_hash.h`: `7dc5a4e3677ab369da48909df74eaea00f516f3107303855258ac8c7560d79f6`.

The two runtime policy sources are from the upstream
[GCC 12.2.0 release](https://github.com/gcc-mirror/gcc/tree/releases/gcc-12.2.0/libstdc%2B%2B-v3/src),
SHA-256 `e63690b5ef14c78d0414148e6cc1f6da982de3afed2d0adac7faecbaa649c7fb`
and `68fe03dda8fe9d33ec38b535cd9a750119da0da461b6040117fc114674ce893e`.
These translations retain GPL-3.0-or-later with GCC-exception-3.1 provenance,
copyright Free Software Foundation 2007-2022. No GCC source bytes or private
addresses are committed as fixtures.

`compare_seal8_unsealed_update_sets` recomputes the chain from labelled observed
insertion-history fixtures. It compares every represented insertion/duplicate,
hash, bucket, rehash, yield and paired Update begin/end. Logical lifetime and
node-token checks reject stale reuse, live overlap and changed duplicate nodes.
Recorded yields never supply the iteration order. A failed proof refuses without
resynchronisation. These represented token checks do not prove full allocation
extents, native construction causes or independently derived cell membership.
The report keeps cell-membership acceptance at zero and F1 explicitly unsealed.
`BENILLA_F1_SEAL8_FRAMING` selects the retained hash-anchored capture. Absent
default data prints SKIP. Explicit missing or changed evidence fails.
Authored component fixtures remain labelled. `source_visitors.rs`, capability,
native format and persistence stay unchanged until F3.13.

## Source persistent map state

`source_execution::SourceManagers` owns the F4.01 persistent-state projection.
It creates world states in effective Map.dbc ID order, then loads creature and
GO deadlines in independently bound SQL occurrence order. The character dump
reader requires declared `creature_respawn`, `gameobject_respawn`, and `instance`
relations. Missing relations, empty results, and SQL errors remain distinct.
DB-time cleanup uses `<=`; storing a deadline uses `>` against the World snapshot.

The GPL-3.0-or-later Rust translation derives from CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, originally GPL-2.0-or-later,
modified by Benilla fork contributors in 2026. Upstream notices remain in
`../benilla-sim/licenses/`. The focused source contracts are:

- `Maps/MapPersistentStateMgr.h:43-51,75-137,186-190` defines typed cell GUID
  sets, map binding, deadline maps, per-state pool data, and dungeon bind lists.
- `Maps/MapPersistentStateMgr.cpp:73-155` mutates memory before beginning the
  delete/conditional-insert transaction. Battleground saves do not issue SQL.
- `Maps/MapPersistentStateMgr.cpp:166-216,246-249,338-342` defines the cell key,
  retained empty cells, world-state retention, and dungeon/BG unload predicates.
- `Maps/MapPersistentStateMgr.cpp:638-691,708-731,981-1118` defines state lookup,
  lifetime, world creation, DB-time cleanup, LEFT JOIN validation, and loading.
- `Pools/PoolManager.cpp:35-107` retains duplicate-insert count increments,
  zero child entries, child-count overwrite, and guarded removal decrements.
- `Grids/ObjectGridLoader.cpp:192-218` reads live persistent cell sets after
  static sets. `PersistentGridDefinitions` supplies that live lane, not a copied
  `GridSpawnSets` snapshot. An unwrapped catalog reader refuses the missing owner.

`ManagerExecutionBinding` anchors the execution, snapshot, configuration,
registry, environment, and source query occurrences. Retained handles reject
changed files on Unix. On Windows, read-only sharing prevents overwrites and
renames while the handles are held. Typed catalog access returns only raw loader
operands, not ObjectMgr acceptance, pool selections, or callback results.
Effective definitions, SQL effects, and a reached reset scheduler require
executable owners.
`PersistentEffects::query_respawns` computes Rows/Empty/Error. The binding cannot
supply that result. Successful results must match the snapshot and complete
source occurrence permutation; query errors remain distinct executable outputs.

The source Save methods can continue after their Set method unloads and deletes
the state at `MapPersistentStateMgr.cpp:75-78,100-103,136-155,63-70,708-731,847-854`.
That undefined path refuses `execution.persistent.save_after_unload_undefined`;
it does not reorder the source operations or dereference the deleted state.

Versioned inventories retain map/state lifetimes, dungeon binds, deadlines,
empty cells, typed pool membership/counts, initialization, and generation cursors.
Their validators check structural identity. They are not a durable restore format;
foreign-owner joins, legal unordered traversal history, prior SQL causality,
sealed F1 acceptance, and F7 continuation remain required. No later F4 manager
runs in this projection. Production admission and native format are unchanged.
