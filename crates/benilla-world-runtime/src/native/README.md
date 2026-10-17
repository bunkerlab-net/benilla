# Native component integration boundary

This is a distinct component-test domain inside the existing world runtime.
`OriginalFixture` World and its v2 saves remain unchanged. `NativeWorld` v43 cannot
admit production content: `NativeCapability` is uninhabited until the raw compiler
validates the source closure and every critical semantic dependency. That is an
implementation condition, not a requirement that developers possess game assets.
A complete compiler should issue a capability from each user's validated local data.

The opt-in `synthetic-native-tests` feature exposes only synthetic construction and
restoration. Feature unification, preparation JSON, hashes, and source initializer
outputs cannot produce `NativeCapability`. Fabricated test rows are not native data.

Native format 43, `benilla-native-dry-habitat-v43`, binds source-owned player
melee timers, retained reset witnesses, initial-cast plans, ordinary creature addon
source rows, explicit capability/Home/visitor addon closures, source-owned dry
habitat-3 initialization and the selected full-raw spell domain in every profile,
state, record, checkpoint, melee and
callback hash domain. Native42 and earlier checkpoints and
journals reject without migration, including rehashed outer envelopes and stale
profile context. This format boundary does not open production admission or
loaded-native cache restore;
the existing explicitly reviewed cold `+0.0` distance exception is unchanged.

Optional [creature generation ownership](../../CREATURE-GENERATION.md) retains
complete initialization inputs, selected per-life stats, virtual mainhand metadata,
static disarm, spawn/template addon rows and shared-counter draw history. Complete
spawn-addon selection takes precedence over template-addon selection and applies
only bounded sheath state, preserving the remaining initialized misc bits. Both query
results remain retained even when empty or overridden; unsupported mount, stand,
emote and aura fields reject. Addon movement flags are retained source input but
are not applied by the pinned loader. Scripts remain closed. Creation, death-delay
redraw, corpse removal and respawn are explicit lifecycle transitions. Static
copied NPC levels cannot stand in for this owner.
The synthetic cold-entry schedule does not certify a real map's load order.

Selected capability, Home and ordinary visitor sources classify addon closure as
`Absent` or `GenerationSource { source_identity }`. The latter must resolve to the
exact validated generation source and stable spawn/entry; a digest cannot supply
a missing owner. Absence rejects any retained spawn/template row, even all-zero
or overridden rows. The supported immutable Home reload is an idempotent sheath
write. No addon movement flags are applied, and other callback/script exclusions
remain closed. The separately selected [dry habitat-3 domain](../../CREATURE-DRY-HABITAT.md)
retains `CanSwim=true`, observes initialization before level selection, uses the
50-yard swimming-capable allowed-Z branch and carries filter 13 over ground-only
polygons. Wet execution and complete actual entry-6 admission remain closed.
Addon composition alone supplies no startup, geometry or content-admission proof.

Player pose and orientation persistence are documented in [ORIENTATION.md](ORIENTATION.md).
Explicit owned cached player terrain phases and their closed callback boundary are
documented in [ENVIRONMENT.md](ENVIRONMENT.md).
Backpack drag commands and durable item projection are documented in
[INVENTORY-MOVEMENT.md](INVENTORY-MOVEMENT.md).
Bounded source creature habitat/current-flag ownership and reachable NearPoint
request binding, including the bounded runtime-owned IGNORE_PATHFINDING receipt,
are documented in [CREATURE-CAPABILITIES.md](../../CREATURE-CAPABILITIES.md).
Initial selected-model collision-width ownership, optional Route witnesses and
respawn invalidation are documented in [CREATURE-COLLISION-WIDTH.md](../../CREATURE-COLLISION-WIDTH.md).

## Source-owned spell combat timers

Selected spell authority v7 retains the complete raw ID 133 row and proves a
neutral modifier domain before admitting its bounded full-raw fields. The
`InterruptFlags` COMBAT bit resets owned BASE/OFF timers at successful release,
after hit/damage/critical draws and before cooldown/mana/start-result events.
The same Unit update then subtracts its tick diff. The supported player domain
is idle, form-zero, non-disarmed, speed-one and mainhand-only; source equipment
Delay initializes the attack-time field and breaking that item restores its
baseline. This supplies no player melee attack command or extra RNG.

A required reset witness survives cast deletion and target death/respawn. Restore
validates historical release identity, equipment/attack-time derivation and exact
current countdown independently of outer state/checkpoint hashes. Full-raw
fields remain synthetic source-shaped evidence: actual DBC equality is unknown,
the raw producer/compiler is still absent, and production admission remains
closed. See [the spell authority contract](../../SPELL-AUTHORITY.md).

## Initial-cast genesis

Native43 selects `FirstWorldEntryOnly`: profile construction retains separate
creation and first-entry plans without sampling, and native construction realizes
the selected entry plan exactly once before tick0. The immutable preparation and
both epoch/order identities remain bound into combat, source-spell, training and
quest genesis. The realized receipt binds the final profile, world, player, seed,
ordered operations, trace and exact RNG counter span; it is required even when
initialization draws no words. Missing or null receipts are invalid.

The existing `benilla-native-sha256-counter-le64-rejection-fraction53-v2` sampler
is unchanged. Initialization draws use its counter0 onward without consuming a
gameplay cast ID, input sequence, TickRecord or projected spell packet. Runtime
draws continue at the verified receipt end. Restore recomputes initialization,
requires that lower bound, and applies the checked per-tick upper bound relative
to it. Checkpoint replay, compaction and reopening do not charge it twice.

This verifies initialization against the checkpoint-selected seed; NativeStore's
selected identity does not independently pin an expected seed. The genesis lower
bound and checked tick budget do not authenticate arbitrary compacted gameplay
history or prove that every later in-window counter was actually reached.

The prepared stochastic subset is the audited targetless Dodge81 empty handler;
it retains its amount request despite no surviving holder or state effect.
Detect3050 retains its no-execution occurrence and consumes no RNG. Authored
order evidence is restricted to the explicit synthetic component domain.
Unknown actual loaded order, indexed dependencies, source rows or reachable
closure cannot become a Human admission certificate. No source-global mt19937
word parity is claimed, and production admission remains closed.

First-entry area evidence uses a separate exact-start-pose query after final profile,
world and player binding. An owned terrain/DBC reader supplies the actual area flag;
the receipt retains the query and re-resolves zone/area from the complete bound DBC.
The complete raw spell_area table may be nonempty: reachable initial spell/aura keys
and nonzero current zone/area keys remain closed. Index zero is never inserted in
the source area index. A missing entry witness is accepted only for an explicitly
complete empty table with no location source selected. Restore checks content
consistency; it does not rerun terrain readers or authenticate a regenerated
historical area flag. Normal native admission remains closed.

## Owner pathfinding bit receipt

Typed Route requests optionally carry `OwnerIgnorePathfindingReceipt`, privately
minted from the staged NPC capability owner, explicit interaction control/audits,
NPC motion closure and the current executable ability set. The single source
`IGNORE_PATHFINDING=0x10000000` bit remains clear in this closed lifecycle; other
UnitState bits are not projected. Consumers use
`CreatureRouteRequest::require_owner_ignore_pathfinding(profile)`.

The immutable captured abilities support transcript consistency checks. Replay
regenerates the receipt from live staged abilities and compares the exact request;
a self-consistent historical receipt alone does not establish current state.
Native v43 requires the `owner_ignore_pathfinding` option field: explicit null is
unavailable, omitted fields and v29 or earlier records/checkpoints reject.
See [the full source contract](../../CREATURE-CAPABILITIES.md#single-bit-owner-pathfinding-closure).
From the repository root, verify the six pinned source files and source expressions:

```sh
python3 crates/benilla-world-runtime/tools/verify_owner_ignore_pathfinding_source.py /path/to/mangos-classic-8ec338a1704e7dcb1c0213eb7ed58f9231ade40f
```

This guard is not a full lifecycle oracle. The receipt grants no MMap config,
calculate completion, Route admission or playable-world capability.

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
  optional environment countdown, death update, receipt-only event prelude, spawn-ordered AI/melee, player combat
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
- Checkpoints bind native format 43, policy, RNG mapping, domain, content, authority, geometry,
  history-chain tip, cooldown maps, recomputable casts/auras, AI lifecycle/motion,
  exact mana timers, skills/reactive/combat clocks, checked equipment recipes and
  [per-NPC event ownership](../../NPC-EVENTS.md) with DEAD/respawn clock continuity
  and [bounded instant NPC SpellStart lifetimes](../../NPC-SPELLS.md)
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
unavailable once and continues later world ticks. The actual local controller now allocates a shared session movement-order
counter for root and supported player slow speeds, with exact typed
acknowledgements. Ghost aura, corpse creation/placement, graveyard
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
front-angle remain separately typed spatial dependencies. The optional
[facing authority](../../FACING-AUTHORITY.md) derives Bearing and the exact
melee/spell arc checks from their executing request poses.

The selected [source spell host](../../SPELL-AUTHORITY.md) derives current
semantic facts and bounded hit/crit chances, while spatial dependencies remain
typed. Native format 43 changes experimental simulation checkpoints/records, not
the separate character/profile save format; incompatible open preserves files.

The [player decrease-speed holder boundary](PLAYER-SLOW.md) adds explicit raw
source and host closure, source-ordered apply/replace/expire/death, current
movement budgets, client fields/durations/speed orders and exact session ACKs.
Its source-effect entry and NPC triggered-cast host remain fixture-only; rear melee
remains rejected. Native format 43 does not change character/profile files.

Explicit per-world [admin level, XP and carried-item operations](ADMIN.md) share the durable
controller/store boundary. Its production UI host remains unavailable.

Retained Chase polygon ownership and the separate loaded warm-query dependency
are specified in [PATHFINDER-CACHE.md](PATHFINDER-CACHE.md).

Initial NPC [collision-height ownership](../../CREATURE-COLLISION-HEIGHT.md) is an
explicit profile initializer over the retained width/model source. Native26 freezes
its optional generation-zero witness onto Route and binds request/profile motion
height scalars. Respawn clears it; current-model mutation and native admission remain open.

Optional fresh-player model height ownership is documented in [Player collision height](../../PLAYER-COLLISION-HEIGHT.md). It retains the actual starter identity and loaded selection, with no production admission or cached-water claim.

Native v43 also requires actual first/retained Chase constructor dispatch evidence,
independent of cache or route success. The private live receipt is paired with a
separately loaded query; serialized replay evidence cannot recreate that lease.
See [PathFinder constructor dispatch](PATHFINDER-DISPATCH.md) for timing, Home
exclusion, one-use claims, transaction rollback and native30-and-earlier rejection.

The first source-owner cache [contract](PATHFINDER-CACHE.md) admits only explicit
first-dispatch/revision-one Complete NORMAL typed claims with exact endpoint and
all segment checks. Retained replies still need exact prior state and increment.
The runtime requires its own request-side live dispatch; this does not prove the
app's native query, source calculation or geometry. Structural motion replay is
separate from native loaded-cache restore, which remains rejected. No production
session, participant or source-epoch admission is opened by native43.

### Owner-keyed motion genesis

Native format 43 (`benilla-native-dry-habitat-v43`) requires an explicit
`MotionGenesis.npcs` map keyed by exact stable spawn. Each `NpcMotionGenesis` owns
its speed/body settings, orientation, initial activity, walking choices, detection
range and combat-entry flag. Player/spell inputs, ordinary initial/relocation
notify delays and the world aggro rate remain shared. Every AI aggro rate must
match that shared value; missing, extra, duplicate or player-owner keys reject.
There is no first-owner fallback or automatic scalar migration.

Each required `HomeAudit.addon_closure` is explicit absence or a reference to the
same immutable generation source. Validation proves that the pinned Home reload
would reapply the same supported sheath state; an absent owner, mismatched source,
missing field, or the previous `addon_effects` schema cannot supply a replacement
closure. Reset
position, temporary faction, scripted-home and summon callbacks remain excluded.

Construction, restore, model-height initialization, combat/spell semantics and
geometry readers select the exact owner. Stable-spawn lookup is not a generation
witness; existing full `NpcId`, route/cache and source witnesses remain required.
The unselected legacy source-motion visitor lane still requires exactly one NPC.
The explicitly selected complete source visitor catalog below admits only its exact
one/two-owner scene. Loaded app-geometry policies remain singleton-only; the
selected authored lane does not supply a loaded visitor/visibility producer.
Motion arithmetic, production admission/startup and loaded-native cache reopening
remain unchanged.

Native42 and older checkpoints/journals reject without migration. Profile, state,
record, checkpoint and melee digest domains are v43; original World v2, character
profiles and the store envelope are unchanged. Fixtures are authored component
evidence, not captured client/DB data.


### Selected complete source visitors

`Profile.source_visitors` selects an immutable `SourceVisitorCatalog`. Its source
builder validates shape before an executable profile exists; its private rows and
lack of deserialization prevent a recorded catalog from becoming live admission.
The selection binds an explicit authored owner-update permutation, raw map ID and
outdoor instance type zero, and one or two distinct-entry ordinary owners. Every
owner has separate complete entry and GUID EventAI lookups, no ScriptName or
script ID, idle default movement, no spell list, and complete-empty custom/named
AI, guard, pet/totem/control, script/addon override writers, linking/group and other
creation-hook coverage. `SourceVisitorOwner.addon_closure` separately binds
`Absent` or the exact generation-owned sheath-only source through
`SourceVisitorAddonClosure`; the selected source policy is
`cmangos-8ec338a1-complete-source-visitors-v3`. It does not remove the independent
`SourceVisitorOverride::Addon` exclusion. Missing and unknown sources never become
empty rows. Ground capability, initialized player, faction/control and combat
closure reuse the retained catalogs.
The original constructor requires empty EventAI vectors. The explicit
`validate_sources_with_event_ai` path retains the bounded synchronous AGGRO/TEXT
catalog described in [EVENT-AI.md](EVENT-AI.md), requiring explicit EventAI AIName
for each nonempty owner and complete-empty companion vectors. This explicit-name
restriction is conservative: pinned source can also choose EventAI by Permit for
an empty AIName. Passive or disabled-combat substitutions reject.

The selected path requests one complete ordered grid/world membership list per
notification. Its grid creatures, world players and explicit not-visited actors
must cover the exact current scene with no missing, duplicate, foreign or stale
generations. Self and dead members remain in membership and are skipped during
dispatch. The returned traversal is never spawn-sorted; grid runs completely
before world. Creature visits run visited-peer to notification-owner, then owner
to peer. The ordinary player's absent AI leaves only the creature-to-player side.
A callback-bound current-scene witness includes the actual executing owner while
it is removed from the staged map. External authority supplies membership and
directional AI visibility; runtime-owned semantic disposition receipts are
regenerated during replay. Static LOS is not AI visibility.

The peer callback is read-only. After current liveness/control/evade and visibility,
its source prefix returns for an active outdoor victim, raw observer neutrality,
false CanInitiateAttack, or false ordinary directional NPC/NPC CanAttackOnSight.
A reached CheckForHelp invocation fails closed before later initiate/relation
checks, even if they would deny attack. A reached peer attack/accessibility
continuation also fails closed before installing a peer target or threat. Empty
linking rows do not remove the core assistance branch. Active victim means Combat
phase with the retained supported target and no completed victim detachment at
this visitor boundary; it is not a general Unit::GetVictim snapshot API.

The explicitly synthetic fixture gives A a hostile-to-player faction and B a raw
faction-zero template with zero masks/lists. Both remain ordinary Aggressive,
combat-capable EventAI with independent motion, event queues and generations. B
initially returns from neutrality; A returns from its active outdoor victim or a
directional peer relation denial. This does not permanently classify B as harmless:
player retaliation can reach another branch. Heights, speeds and orientations are
independently authored. Map::Update's object collection is unordered, so the
selected owner permutation is fixture data, never universal spawn order.

This lane is for authored runtime and headless actual-controller injection. It
establishes no private raw-input capture, loaded map-visitor producer, graphical
create-to-reopen result, or production NativeCapability. The loaded geometry
singleton policies and loaded-native cache reopen guard remain closed. Native42
and older decode/restore/replay paths reject rather than migrating, and store
refusal preserves current and previous files.
