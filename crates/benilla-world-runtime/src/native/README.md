# Native component integration boundary

This is a distinct component-test domain inside the existing world runtime.
`OriginalFixture` World and its v2 saves remain unchanged. `NativeWorld` v10 cannot
admit production content: `NativeCapability` is uninhabited until the raw compiler
validates the source closure and every critical semantic dependency. That is an
implementation condition, not a requirement that developers possess game assets.
A complete compiler should issue a capability from each user's validated local data.

The opt-in `synthetic-native-tests` feature exposes only synthetic construction and
restoration. Feature unification, preparation JSON, hashes, and source initializer
outputs cannot produce `NativeCapability`. Fabricated test rows are not native data.

Player pose and orientation persistence are documented in [ORIENTATION.md](ORIENTATION.md).

## Initialization and callbacks

`Profile::initialize_player` runs the source starter reducer and first-entry skill
gate. It retains created current HP/mana without refilling, final attributes/maxima,
known active availability, passive modifier recipes and raw aura attributes, skill
values, initial inventory, weapon metadata/proficiencies, and actual item durability.
Private genesis cannot be deserialized. Available spell ranks and damage-interruption
policy must match source skills and SpellEntry rows. The raw compiler still owes
complete spell/content proof; unsupported active IDs stay visible and unavailable.

The corrected `npc_melee` kernel supplies ordinary NPC damage/outcome/armor and
callback requirements. Authority replies carry geometry and combat predicate facts,
never damage. The runtime invokes it inside each NPC AI update, after validated
movement and before that NPC's CombatManager update. CombatStarted enters the player
before the first same-tick melee hash/query. Queries bind the post-movement endpoint.

Outgoing NPC attacks do not refresh their pursuit timer: the source call
`EngageInCombatWithAggressor(victim)` targets the player in TriggerCombatTimer, and
players cannot hold threat lists. Existing stationary-melee/direct-threat refresh
rules remain. The legacy synthetic fixed-outcome mode retains its original
all-observations-then-swings order; neither mode claims full map-update parity.

Ordered melee callbacks are combat log, critical wound, stand-up, health/combat,
durability chance/slot, generic cast cancel or delay, post-health combat predicates,
defense-skill chance and dodge reactive. Channel/auto-repeat slots and rear-daze
branches reject explicitly. A profile with captured DeathConfig and checked callback
closure runs source-ordered Player::Kill, durability, cancellation and body lifecycle.
A profile missing those dependencies still rejects lethal damage before commit.
See [DEATH.md](DEATH.md) for the bounded first-life policy and pending ghost lifecycle.

Breaking an item removes source modifiers before committing durability zero. Current
HP/mana clamp after every modifier, while effective stats, regen, crit and maxima
recompute. Resources never refill and cooldown/mana timers never reset. Empty,
already-broken and nondurable selected slots are source no-ops. Source defense
values, 4000-ms dodge reactive, combat counters and worn equipment are durable.

## Frame, randomness and persistence

- One authenticated solo owner and intent per fixed tick; no wall clock, networking,
  central host or trusted peer result frames
- Commands, monotone cast IDs, spawn/spell-ordered auras, player reactive update,
  death update, receipt-only event prelude, spawn-ordered AI/melee, player combat
  update, and mage regeneration
- One generic casting slot, multiple traveling casts; at most 16 flights and 16
  harmful holders per target
- Navigation outcomes bind request, tick, generation, revision, position, elapsed
  time and speed. Missing geometry blocks the whole tick; no stationary chase or
  teleport fallback
- Whole-frame staging covers components, resources, movement, facts, RNG, journal
  and checkpoint size. Any error leaves the committed world unchanged
- SHA-256 counter RNG takes the first little-endian u64 and rejects words below
  `2^64 % inclusive_width`; every rejection consumes a counter, at most 64 per die
- Spell draws are hit, direct slot dice only on hit, eligible critical, accepted
  impact periodic slot dice, then eligible current-cast pushback
- Melee draws are damage even for singleton ranges/misses, outcome 1..10000,
  surviving positive-hit durability fraction53 even at configured 0%, equipment
  slot 0..18 only on success, eligible pushback resistance, then required defense
  fraction53 even at configured gain 0. Capped/absent skills draw nothing
- Fraction53 is uniform 0..2^53-1, converted to fraction then percent in software
  binary64; source comparison is strict promoted-float chance > double roll
- Draw records bind semantic purpose/action/NPC generation. Action chains bind full
  current defense/cast/resource snapshots, so same-HP misses cannot be reused
- Checkpoints bind schema 3, policy, RNG mapping, domain, content, authority, geometry,
  history-chain tip, cooldown maps, recomputable casts/auras, AI lifecycle/motion,
  exact mana timers, skills/reactive/combat clocks and checked equipment recipes
- Live components do not deserialize. Inert DTO restore validates provenance and
  cross-state constraints; canonical bounded decode rejects old/malformed saves
- Replay reissues bound requests, consumes saved read-only authority observations,
  regenerates conditional dice, and compares events/state/hash chains. Digests
  establish integrity/continuity, not that an untrusted author witnessed history

## Client and remaining adapters

`NativeProjection` emits existing SessionEvents for spellbook/start/go/failure/delay,
damage/periodic logs, aura fields, live HP/mana/maxima, initialized stats/resistances/
skills, combat and defense-reactive flags, critical emotes, equipped descriptors and
durability, NPC lifecycle, and committed movement. These are headless component and
projection tests, not graphical gameplay verification. Full item-query metadata and
backpack/equip/repair/item-use command adapters remain pending.

The app controller still selects original World. It must explicitly select native
after raw capability validation and wire stock commands and actual authority/geometry.
`benilla_world_store::native::NativeStore` supplies a distinct envelope, exclusive
lock, previous-good retention and exact replay-verified receipts. Its Linux synthetic
tests do not prove native admission, app checkpoint cadence or platform power-loss
behavior. `checkpoint_committed` acknowledges only the writer's exact durable receipt.
Login/resume also needs an in-flight visual reconstruction policy. Native loot,
quests, progression, revive, general auras and multiplayer membership remain
limited by the separately documented component boundaries.

Actual client collision/terrain-stream access is main-thread-bound and needs bounded
request/reply snapshots. Unloaded geometry must never mean clear space. Existing
client host-float collision is not proof of CMaNGOS vmap or cross-peer parity.

## Death and next release adapter

Schema 3 applies source Player::Kill/SetDeathState/KillPlayer ordering, retains mana
except equipment clamping, preserves released flights and passive holders, and
cancels preparing casts through the existing execution path. Checked durable death
and reclaim timers continue across reload. The six-minute boundary records release
unavailable once and continues later world ticks. The client requires a real local
session movement-order counter for root; controller allocation/acknowledgement
handling remains to be wired. Ghost aura, corpse creation/placement, graveyard
selection/navigation, reclaim and resurrection are separate missing dependencies.

The receipt-only prelude performs no NPC timers, movement or AI. It preserves earlier
spell event-time validity when an earlier NPC kills the caster, while synchronous
CombatStop detaches all attackers and later ordinary AI selects NoTarget. This fixed
loop is documented explicitly in DEATH.md and does not claim full MapManager parity.

No Blizzard assets/database rows are distributed. Formula source/license details
remain in the simulation component documents and EQUIPMENT.md. Focused tests do not
replace workspace, platform, source-data or real-client gates.

## Ordinary creature regeneration

NPC profiles bind required captured health flags, creature kind, effective player-
control flags and explicit active-power evidence through the
[ordinary creature regeneration policy](../../../benilla-sim/CREATURE-REGENERATION.md).
Unknown/control/pet/overridden cases and relevant configured power regeneration
are rejected. Proven zero-maximum or disabled power paths are accepted.

The persisted NPC timer starts at zero, advances after its source-ordered AI and
CombatManager update, and performs at most one health pulse per living update.
Death, corpse, respawn-transition and despawn updates freeze it; evade/home never
reset it or instantly refill health. Actual gains emit NpcHealthRegenerated and
the client publishes the committed unit-health value. Checkpoint/replay and
client tests use authored fixtures only; production admission remains closed.
Source quest reputation state, synchronous objective callbacks, client packet ordering
and durable/session boundaries are specified in [REPUTATION.md](REPUTATION.md).

The explicitly selected [NPC and melee semantic host](../../COMBAT-AUTHORITY.md)
derives current relationships, combat entry, lifecycle and movement from captured
source metadata and the staged owner. Visibility, accessibility, LOS and victim
front-angle remain separately typed spatial dependencies.
