# Full Classic implementation roadmap

The goal is the complete WoW 1.12.1 game, playable with one local peer and with
friends sharing an agreed P2P world. A starter region, one class, or an ordinary
creature/quest loop is an intermediate integration milestone, not the final
product scope. The current fork does not yet provide that complete playable world.

## What the reference supplies

The implementation reference is [CMaNGOS Classic core
8ec338a1](https://github.com/cmangos/mangos-classic/tree/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f).
[Classic-DB](https://github.com/cmangos/classic-db) supplies the corresponding
server world-content project. The client's Data directory supplies client assets
and tables; it does not contain the complete server database or server behavior.
The [local-data boundary](CLASSIC-WORLD-IMPORT.md) records pinned schemas,
transitive dependencies, revision checks and redistribution limits. This project
must not invent replacement world placements or quest text and call them Classic.

Core, database updates and AI/script revisions must agree. The full database,
updates and ACID EventAI data are separate from complex C++ scripted behavior.
A populated SQL table alone does not implement the behavior that consumes it.

## Current source coverage and gaps

### Creatures and spawns

`Creature.cpp`, `StatSystem.cpp`, the AI selector, ordinary UnitAI/CreatureAI,
EventAI selection, combat and threat sources have been inspected. Bounded ports
cover creature initialization, ordinary AI/combat decisions, corpse/respawn
transitions and source-derived numeric behavior. See
[creature initialization](../crates/benilla-sim/CREATURE-INIT.md) and
[creature AI](../crates/benilla-sim/CREATURE-AI.md).

Still required for a populated ordinary world: compatible local spawn/template
closure, model/stat overrides, active spawn groups/pools/events, truthful unit
lifecycle/aggro state, complete motion integration and the remaining AI/script behaviors. Ordinary
uncontrolled-creature health regeneration is integrated with persisted timers
and source evade ordering; power regeneration and controlled/pet branches remain
explicitly unsupported. See [regeneration coverage](../crates/benilla-sim/CREATURE-REGENERATION.md). Unsupported records must not silently lose their
behavior or become invented passive creatures.

### Pathing and collision

The pinned VMap parser/BIH traversal and Detour MMap loading/query implementations
are present with authored serialized fixtures, bounded validation and native
memory-safety tests. They are additional constraints on the existing conservative
geometry proof. See the [VMap provenance](../crates/benilla-app/src/net/p2p/world/native_geometry/products/vmap/PROVENANCE.md)
and [Detour provenance](../crates/benilla-native-nav/PROVENANCE.md).

The [raw M2 collision producer](../crates/benilla-native-vmap/PROVENANCE.md)
converts bounded MD20 collision arrays to the pinned extractor's `VMAPs05`
bytes. Authored inputs pass through the original extractor and assembler to
exercise the actual VMap reader, including source M2 zero group bounds.
The [dry WMO raw producer](../crates/benilla-native-vmap/WMO-PROVENANCE.md)
requires every root-declared group and preserves the original small-VMap
material mask, skipped groups, ascending vertex remap and raw bounds. Its raw
geometry projection does not extract or place WMO doodads, and rejects liquid
chunks pending separate source-order proof.
The separate [bounded acquisition library](../crates/benilla-native-vmap-source/README.md)
captures the global dry-WMO pass and physical map prefix through a selected map,
with ordered archive winners, dummy/absence receipts and changed-input rejection.
It retains complete raw candidates and source-order placements without publishing
a partial map. The [doodad dependency layer](../crates/benilla-native-vmap-source/DOODADS.md)
extracts MODN models in source order and retains selected-set membership with raw
transforms, including skipped-group and no-hull filtering. Final G3D transforms
and unordered-source emission/ID order remain unproved. Unsupported liquids,
name profiles or resource bounds can stop the global pass before map discovery; real-data acceptance for this library remains open.
Static map production still needs all map placements in its BIH,
WMO liquid/doodad closure and exact archive/encounter ordering; selected tile
geometry alone cannot establish the absence of off-tile collision.

`TargetedMovementGenerator.cpp`, `HomeMovementGenerator.cpp` and `PathFinder.cpp`
have been inspected. A bounded one-player/one-creature chase, spline, Home and
notification adapter now updates transactional runtime state and projects ordered
client movement packets, including save/reopen and rollback checks. See
[runtime motion](../crates/benilla-world-runtime/NATIVE-MOTION.md). The complete
owner-dependent pathfinder, multi-unit visitor coverage, waypoint/script execution,
liquid accessibility, slopes and world-streaming transitions remain incomplete.
A successful Detour query alone does not establish those behaviors. Reachable,
accessible, visible and line-of-sight are distinct source predicates.

### Quests and progression

Ports cover bounded `Player.cpp` eligibility, prerequisite/status transitions,
accept/complete/reward flow, source XP/level calculations and item objective/reward
callbacks, with runtime persistence and client packet projection. `QuestDef.cpp`,
`QuestHandler.cpp` and loader relations are part of the source audit. See
[simulation provenance](../crates/benilla-sim/PROVENANCE.md).

Ordinary quest reputation now uses an immutable source catalog and persisted
reducer state. Synchronous objective callbacks run between spillover/reward slots
before XP; client initialization/delta packets and save/reopen ordering are
integrated. Replay checks callback witnesses and completed progression prefixes.
See [runtime reputation](../crates/benilla-world-runtime/src/native/REPUTATION.md).

Remaining coverage includes kill/spell reputation, forced reactions and aura
modifiers, script/condition closure, gameobjects, timed/escort/exploration/cast
objectives, other ordinary dependency branches and all relevant content. An authored quest
fixture or source oracle is not evidence that real world quests are admitted.

### Loot and items

The reconstructed ordinary solo corpse implementation follows `LootMgr.cpp`,
`LootHandler.cpp`, creature loot timers and `Player.cpp` inventory/quest-item
callbacks. It stages rewards and durable state before client publication, with
source-derived roll, money, stack/storage, release and reward-order tests.

It does not establish complete table/reference/condition/script coverage, group
loot policies, every item effect, equipment operation, trade, mail, auction,
banking or profession behavior. Complete local item/loot dependencies and source
order must be proven before admitting additional records.

### Spells, units and the shared world

Cast prepare/release/impact, selected resource/stat/training branches and ordinary
melee are incremental source ports. Full classes, aura/proc/immunity/resistance
semantics, source target relations, death/recovery and world services remain.
Player orientation now follows sequenced transactional pose updates and durable
client publication. Source reputation and tracked player/NPC relation predicates
feed an internal quest-status, quest-interaction and loot-interaction host using
current transactional state and ordered replay witnesses. A selected bounded
ordinary-creature host now derives NPC and melee relationship/combat-entry decisions
from the actual staged AI, health and motion state. Complete captured flag rows must
agree with the initializer; visibility, accessibility and LOS remain distinct
spatial dependencies. A separately selected [facing policy](../crates/benilla-world-runtime/FACING-AUTHORITY.md)
now derives source bearings, mutual melee front arcs and spell-facing checks from
the actual phase-owned poses using pinned software arithmetic, and recomputes
them on replay. Other spatial predicates retain their explicit dependencies. See [combat authority](../crates/benilla-world-runtime/COMBAT-AUTHORITY.md).
A further selected spell host derives prepare/release/impact decisions and release
hit/critical chances from raw source rows and current transactional state, with
historical launch validation and source SpellGo publication. Its explicit ID133
fire/MAGE boundary and 350 numeric probes do not certify real content or general
spell parity. See [spell authority](../crates/benilla-world-runtime/SPELL-AUTHORITY.md).
Other spell and creature/control branches remain separate integration work.
The source EventProcessor
queue primitive and bounded per-player notification/SpellEvent integration now
preserve due order, retained spell lifetimes, transactional rollback and durable
reopening. A [per-NPC source event queue](../crates/benilla-world-runtime/NPC-EVENTS.md)
now owns AI notifications through corpse, DEAD and respawn phases, with
generation-bound handles and durable retry. A bounded [player slow-holder path](../crates/benilla-world-runtime/src/native/PLAYER-SLOW.md) now binds explicit raw source rows to active holders, source speed aggregation, movement budgets and durable client/ACK publication. Its authored effect entry is fixture-only. NPC SpellStart/target/hit callbacks and rear-melee integration are still required before rear melee/daze can be admitted.
General spell effects, NPC casts and the complete failed-attempt
lifecycle remain incomplete. See [player events](../crates/benilla-world-runtime/PLAYER-EVENTS.md)
and the
[interaction host boundary](../crates/benilla-world-runtime/NATIVE-INTERACTIONS.md). State must be shared transactionally by all systems:
orientation, movement, reputation, auras and lifecycle cannot be guessed from
rendering or copied into an independent mutable authority mirror.

## Integration order and evidence

1. Close reusable current-state gaps: persisted pose, reputation, source target
   relationships, rear-hit/daze behavior, motion/lifecycle and regeneration
2. Resolve semantic decisions from the current transactional state at the exact
   action phase; request only genuine geometry facts from spatial providers
3. Complete and independently validate local content admission, with core/database
   revision and dependency closure. Production native entry remains closed until
   this boundary and all required providers are available
4. Verify a real local accept → combat → loot → reward → level → next-quest loop,
   including save/restart and failure rollback, then broaden content/classes and
   mechanics systematically rather than treating that loop as full-game completion
5. Extend the same world to peers with authenticated ownership, agreed content and
   rules, consistent joins/leaves and explicit world-branch reconciliation. Solo
   progression must not wait for configured but absent friends
6. Cover remaining Classic systems: all classes and spell effects, gameobjects,
   NPC services, professions, groups, instances/scripts, transports, battlegrounds,
   social/world services and the corresponding content/dependency tests

Every increment needs source provenance, authored oracle/regression coverage,
durable replay tests and actual client integration. Full workspace CI and native
platform tests remain gates. Real asset-backed visual/gameplay verification is a
separate observation; a green headless suite alone cannot establish full parity.
