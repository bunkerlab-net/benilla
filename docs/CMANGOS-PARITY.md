# CMaNGOS Classic port and parity ledger

## Contract and evidence

The target is a playable 1.12.1 world with peer-to-peer simulation. The existing
renderer/realm client, peer exploration, authored encounter and local data model
are useful foundations, but do **not** establish a full CMaNGOS gameplay port.

All upstream entrypoints below were read from the official
[`cmangos/mangos-classic` source at the pinned revision][core]
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. Links are commit-pinned and identify
implementation files and line ranges, not similarly named code in another expansion.
This ledger contains original analysis, no copied implementation, database rows,
quest text or Blizzard assets.

Local evidence was inspected on 2026-10-13. The saved-character/owner-authenticated
peer bridge supports local race/class/appearance selection, durable profiles and
position saves. Full app CI, native-platform tests and asset-backed visual checks
are distinct gates; a passing pure reducer test is not a playable-world result.

The original solo runtime, protocol projection and durable world store have
headless regression tests, including replay, partial loot and checkpoint continuity.
The app controller has tests for command admission, correction, checkpoint cadence
and failed-save retry, but native-content entry remains closed.

The [Vanilla-style Admin panel](P2P-ADMIN.md) has a typed, capability-gated UI
interface. Its production backend is not attached; no admin grants are enabled.

The separate native component path now integrates source-derived spell execution,
ordinary NPC AI/melee, player stat/skill/equipment initialization and durable replay.
Its production capability remains unavailable pending a complete local-data compiler
and real geometry/authority adapters. Named ports, differential domains and explicit
exclusions are documented in:

- [Stat/armor primitives](../crates/benilla-sim/CLASSIC.md)
- [Quest reward arithmetic](../crates/benilla-sim/QUESTS.md)
- [Targeting](../crates/benilla-sim/CLASSIC-TARGETS.md)
- [Cast/projectile/periodic execution](../crates/benilla-sim/SPELLS.md)
- [Mana/resources](../crates/benilla-sim/MAGE-RESOURCES.md)
- [Fresh mage initialization and first-entry replay](../crates/benilla-sim/STARTER-INIT.md)
- [Ordinary NPC melee](../crates/benilla-sim/NPC-MELEE.md)
- [Creature initialization](../crates/benilla-sim/CREATURE-INIT.md)
- [Bounded Chase straight-selection gate](../crates/benilla-sim/CHASE-GATE.md)
- [Player death/corpse lifecycle reducer](../crates/benilla-sim/PLAYER-DEATH.md)
- [Integrated bounded native death](../crates/benilla-world-runtime/src/native/DEATH.md)
- [Bounded mage XP/level transitions](../crates/benilla-sim/PLAYER-PROGRESSION.md)
- [Ordinary quest lifecycle and host requirements](../crates/benilla-sim/QUEST-LIFECYCLE.md)
- [Integrated quest reputation and durable callbacks](../crates/benilla-world-runtime/src/native/REPUTATION.md)
- [Loaded dry static VMap ray subset](../crates/benilla-app/src/net/p2p/world/native_geometry/products/vmap/PROVENANCE.md)
- [Conservative local geometry service](../crates/benilla-app/src/net/p2p/world/native_geometry/README.md)
- [Bounded current-state spell authority](../crates/benilla-world-runtime/SPELL-AUTHORITY.md)
- [Pinned portable bearing/arc arithmetic](../crates/benilla-sim/FACING.md)
- [Current-pose facing authority and replay](../crates/benilla-world-runtime/FACING-AUTHORITY.md)
- [Native runtime integration](../crates/benilla-world-runtime/src/native/README.md)
- [Selected spell registry evidence](../scripts/classic_export/SPELL-REGISTRY-CONTRACT.md)

Local extraction now requires manifest v3 with 20 DBC tables, including raw
CreatureDisplayInfo. Older extraction manifests require explicit re-extraction;
selected-empty script evidence does not establish absence of other spell hooks.

These are bounded ports with synthetic source-derived probes. They do not establish
full combat, real starter-data admission, playable native quests or an asset-backed world.

Three different claims require different evidence:

1. **CMaNGOS parity:** named behavior matches this pinned implementation over a
   declared input domain, with differential tests
2. **Documented fork behavior:** a deliberate deterministic or architectural
   deviation has a versioned contract and tests; it is not exact upstream parity
3. **Playable client integration:** the real UI/rendering/input path works with
   the player's own data; headless tests alone do not prove it

CMaNGOS itself is an emulator, not a perfect original-server oracle. For example,
[`XP::xp_in_group_rate`][xp] expressly identifies its formula as an assumption.
Matching it does not prove original Blizzard behavior. No completion percentage
is meaningful without a bounded content/mechanics inventory.

## 1. World data and world lifecycle

- **Verified upstream:** [`World::SetInitialWorldSettings`][world] establishes
  the loading sequence. [`ObjectMgr::LoadCreatureTemplates`, `LoadCreatures`,
  `LoadItemPrototypes`, `LoadPlayerInfo`, `LoadQuests` and
  `LoadCreatureQuestRelations`][objects] provide distinct loaders.
  [`LoadDBCStores`][dbc] and [`Map::Update`][map] are additional dependencies,
  not replaced by a JSON import.
- **Current fork:** `crates/benilla-world-data/src/{model,lib,validate}.rs`
  provides bounded `load_json`/`load_reader`, typed lookups, reference checks and
  a normalized digest. Its `CAPABILITIES.json` explicitly says data-model-only,
  no SQL importer and no runtime integration. Its authored fixture represents
  fixed creature/spawn, kill-quest, plain item and direct ability records only.
  The separate `scripts/classic-export.py` now offers a fake-driver-tested native
  SQL projection, schema probe and blocker report, with no real DB connectivity
  verification or native-to-runtime compiler. Its output is explicitly non-runnable.
- **Dependencies:** native schema/version inspection; local data access and
  provenance; DBC lookups; creature class/level stats; game objects; conditions;
  pools/events/spawn groups; map/instance identity and lifecycle. Native source
  capture and a mechanics-aware compiler are separate contracts.
- **Required parity tests:** per-field conversion/disposition audit; loader
  rejection of missing dependencies and unsupported mechanics; spawn overrides,
  random levels, respawns and unloaded grids; source-vs-compiled semantics.
  Schema/digest/fixture tests cannot satisfy these tests by themselves.
- **Next port entrypoints:** trace the selected spawn through `LoadCreatures`,
  `Creature::LoadFromDB` and `Creature::UpdateEntry` in [Creature.cpp][creature];
  keep unsupported fields as explicit blockers. Follow the local
  [exporter contract](CLASSIC-EXPORTER-CONTRACT.md), then add only the smallest
  reviewed runtime profile. Never silently flatten native records to fixed data.

## 2. Character creation, stats and progression

- **Verified upstream:** [`Player::Create`, `GiveXP`, `GiveLevel`,
  `InitStatsForLevel`][player]; [`ObjectMgr::LoadPlayerInfo`,
  `GetPlayerClassLevelInfo`, `GetPlayerLevelInfo`, `GetXPForLevel`][objects];
  [`Player::UpdateAllStats`, `UpdateArmor`, `UpdateAttackPowerAndDamage`,
  `CalculateMinMaxDamage`, `UpdateManaRegen`][stats]; [`MaNGOS::XP`][xp].
- **Current fork:** `benilla-sim/src/xp.rs` ports the integer XP helpers and a
  rational reformulation of `BaseGain`. `PROVENANCE.md` explicitly excludes
  complete `Gain`, creature eligibility/modifiers, group/rested XP and level-up
  tables. `benilla-profile/src/lib.rs` supplies owner/character records,
  race/class validation and an explicitly noncanonical meeting-spawn policy,
  not a stat/progression engine. The separate `starter_init` reducer now translates
  bounded level-one mage creation, deferred stat modifiers, first-login skill/spell
  replay and equipped-item effects. It preserves created current resources rather
  than refilling to recomputed maxima. Unsupported passive/rank/hook semantics fail
  explicitly. The separate progression host now admits supplied consecutive
  mage rows through 60 with explicitly unspent talents, captured binary32
  Rate.Talent, real training/stat callbacks and recipe rebasing. Full all-class
  powers and talent spending remain separate work. Defense catch-up now uses
  a versioned source-derived log table and software binary64 arithmetic for
  levels 1..60 and pure skill maxima 1..300, with exhaustive final-f32/RNG checks.
  Authored native tests verify undertrained level-60 gains and exact checkpoint
  replay; native row admission and other combat dependencies remain separate.
- **Bounded component ports:** the separate
  [player progression reducer](../crates/benilla-sim/PLAYER-PROGRESSION.md) and
  [solo kill-XP reducer](../crates/benilla-sim/KILL-XP.md) preserve source level,
  binary32 gain, rested arithmetic and event ordering in their declared domains.
  Kill awards require proven owner-generated death/tap state and explicit
  no-effect honor/reputation and fresh-online clock contracts. Native ordinary
  quests, kill XP and gated admin +1 already compose these reducers, project
  talent field 1102 and persist their source callback histories. Native17 binds
  historical casts/auras to owner-specific journal prefixes within unchanged
  count/byte bounds. Authored headless fixtures do not establish real-data
  playability or full Classic parity.
- **Dependencies:** player creation/level tables; class/race DBC information;
  item/aura modifiers; skills, talents, spell learning, faction/reputation;
  death/rested state; kill attribution; local rates with explicit semantics.
- **Required parity tests:** all valid creation combinations and start states;
  each level transition through 60, multi-level XP grants and cap handling;
  modifier application/removal without drift; attack/defense skill boundaries;
  rested/elite/no-XP/pet/critter and group cases. Existing
  `benilla-sim/tests/xp_parity.rs` covers helper fixtures and declared rounding
  differences, not these lifecycle tests.
- **Next port entrypoints:** `InitStatsForLevel` plus its table lookups, then
  `GiveLevel`/`GiveXP` and the complete `XP::Gain` caller path. Preserve and
  version the existing rational-vs-float rounding deviation rather than calling
  it exact parity. Keep progression state separate from secret-bearing profiles.

## 3. Movement, collision and navigation

- **Verified upstream:** [`WorldSession::HandleMovementOpcodes`,
  `VerifyMovementInfo`, `ProcessMovementInfo`, `HandleMoverRelocation`][movement];
  [`MotionMaster::UpdateMotion`, `MoveChase`, `MoveWaypoint`, `MoveFleeing`][motion];
  [`MoveSpline::Initialize`, `ComputePosition`, `_updateState`][spline].
- **Current fork:** `benilla-app/src/net/p2p.rs` relays avatar poses, jump fields
  and chat with finite/bounded-value checks. It explicitly rejects transport
  movement in the inspected bridge. This is not replicated authoritative player
  movement, NPC navigation, melee reach or line-of-sight validation.
- **Bounded native components:** source-owned model and cached player-environment
  witnesses feed exact Chase distance/vertical predicates, actual static LOS and
  owner dry-liquid observations. One immutable inventory can project proved-dry
  navigation without admitting wet geometry. Passing this gate reaches retained
  warm straight/retry dependencies; a rejected gate selects unavailable normal
  routing. Each bounded calculate attempt validates exact source map coordinates,
  requires the staged owner's generation-bound IGNORE_PATHFINDING receipt, and
  observes endpoint tile presence on the same loaded native mesh before its
  filter update. The receipt proves only that single bit is clear under the
  explicit control/script/aura closure, including current executable abilities;
  it does not claim complete UnitState or effective MMap settings. Native record
  format 32 retains this required-option evidence and rejects earlier formats.
  The first source-owner cache contract requires explicit first dispatch, revision
  one and Complete NORMAL with exact endpoints/all segment predicates. Retained
  use still requires exact prior and increment. Runtime-owned live dispatch is
  mandatory; typed claim tests prove no native query or physics execution. Motion3
  structural first-spline restore/replay is coherent while native loaded-cache
  reopen and production admission remain closed.
  Actual Chase dispatch also records first construction or retained use after
  forced relocation and before the straight gate, independently of cache/result.
  Its runtime receipt binds generation and selected MMap configuration; a skipped
  initial Chase has no event until dispatch. The one-use live receipt must still
  be paired with a separately loaded query; deserialized evidence grants no lease.
  Home's distinct local constructor remains outside this Chase seam. See the
  [constructor contract](../crates/benilla-world-runtime/src/native/PATHFINDER-DISPATCH.md).
  Actual tile absence stays unavailable even when nearby polygons or the retained
  cache can be found. An independent unchanged-source oracle covers coordinate
  and nontransport tile predicates. Configuration/query selection and the complete
  calculate prefix remain separate gates. Controller tests preserve the
  explicitly synthetic initial warm seed
  and verify complete rollback at the still-unavailable full Route boundary.
  Cold owner initialization, complete Route evidence and gameplay admission remain
  separate gates; see the [geometry contract](../crates/benilla-app/src/net/p2p/world/native_geometry/README.md).
- **Dependencies:** common map geometry, collision/height/liquid queries,
  navigation data, speed/root/knockback state, transport transforms, spatial
  indexing and deterministic simulation time. Rendering geometry alone is not
  evidence of equivalent server-side movement decisions.
- **Required parity tests:** slopes, falls, water, doors/LOS, unreachable targets,
  chase range and evade; spline endpoints/timing, rooted movement and forced
  movement acknowledgments; identical replay on supported platforms. Test
  malformed and stale movement separately from gameplay correctness.
- **Next port entrypoints:** define the authoritative position/time contract;
  extract movement validation and one chase generator with geometry adapters.
  Do not feed unvalidated owner pose updates directly into combat outcomes.

## 4. Combat and death

- **Verified upstream:** [`Unit::CalculateMeleeDamage`, `DealMeleeDamage`,
  `DealDamage`, `Kill`, `CalcArmorReducedDamage`, `AttackerStateUpdate`,
  `RollMeleeOutcomeAgainst`][unit]; [`ThreatManager::addThreat`,
  `getHostileTarget` and `ThreatContainer::selectNextVictim`][threat].
- **Current fork:** the original encounter remains synthetic. The separate
  `npc_melee` port now supplies ordinary NPC damage/outcome/armor and ordered
  callback requirements over a declared bounded domain. The native runtime invokes
  it within source-ordered AI updates, preserves conditional random draws, applies
  equipment breaks and maintains defense/reactive state. Native schema3 integrates
  lethal melee, ordered death/durability/stat effects, cast interruption and corpse
  timers with replay/restore. Root movement orders still require the actual controller
  counter and acknowledgements; ghost/graveyard release and resurrection are absent.
  Creature initialization translates explicit raw classlevel/model/spawn inputs,
  with source loader preprocessing and no fabricated missing stats.
  An explicitly selected current-pose numeric host derives both ordinary melee
  front arcs, spell-facing arcs and motion bearings with a pinned software-IEEE
  glibc profile. Receipts are recomputed during replay; unrelated LOS, range,
  visibility, navigation and raw-content dependencies remain open. This bounded
  service uses experimental native checkpoint format 15, without changing the
  separate character/profile save format or admitting production gameplay.
  Each native NPC now owns a real source EventProcessor for AI notifications,
  preserving corpse updates, DEAD clock suspension, forced respawn replacement,
  generation-guarded handles and transactional replay/save/reopen. See the
  [NPC event contract](../crates/benilla-world-runtime/NPC-EVENTS.md). A separate
  [bounded invisible instant NPC SpellStart host](../crates/benilla-world-runtime/NPC-SPELLS.md)
  preserves ordered target/hit checks and Unit-owned failed/finished cleanup.
  Authored triggers reach the source-bound [slow holders](../crates/benilla-world-runtime/src/native/PLAYER-SLOW.md),
  speed aggregation and durable projection/ACKs. Raw spell1604 and rear-melee
  admission remain closed; these fixtures do not complete rear daze.
- **Dependencies:** derived stats, weapon skill and school, attack timers,
  deterministic random draws, armor/resistance, block/absorb, auras/procs,
  reach/facing/LOS, threat, loot credit, quests, corpses and resurrection.
- **Required parity tests:** miss/dodge/parry/block/crit/glancing/crushing and
  offhand/ranged outcomes; armor boundaries; absorb/reflection/immunity;
  simultaneous lethal events, extra attacks, tagging, duel and resurrection;
  a seeded random-draw trace and exact event ordering, not only aggregate DPS.
- **Next port entrypoints:** isolate `CalculateMeleeDamage` and outcome rolls
  behind pure stat/RNG inputs, then `DealMeleeDamage`/`DealDamage` side effects.
  Port the death/reward transaction after attribution is defined. Fixed damage
  passing lockstep tests does not validate the future damage calculation.

## 5. Spell effects and auras

- **Verified upstream:** [`Spell::Prepare`, `CheckCast`, `FillTargetMap`, `cast`,
  `update`, `HandleEffect`][spell]; the [`SpellEffects` dispatcher and
  `EffectSchoolDMG`, `EffectHeal`, `EffectApplyAura`, `EffectDispel`,
  `EffectTriggerSpell`][effects]; [`AuraHandler`, `Aura::ApplyModifier`,
  `PeriodicTick`, `SpellAuraHolder::Update`, `ModStackAmount`][auras];
  [`SpellMgr::LoadSpellChains`, `LoadSpellProcEvents`,
  `LoadSpellScriptTarget`][spellmgr].
- **Current fork:** the original fixture boundary remains simple. The separate
  native component runtime supports bounded mage casts, projectile travel, periodic
  damage, mana timing, interruption/pushback and checked aura/cast persistence.
  Source spell/learning/initial-effect inputs must be complete; unknown critical
  semantics cannot disappear. Unsupported learned active executors remain visibly
  unavailable only after their learning/stat effects have been validated. General
  channels, procs, triggered scripts and arbitrary aura interactions remain open.
  Existing realm-client spell UI/protocol code alone is not a local executor.
- **Dependencies:** the exact spell schema and local DBCs, target selection,
  modifiers, resources, reagents/equipment, immunities, spatial rules, scheduling,
  interrupt state, proc ordering and script registry.
- **Required parity tests:** cast rejection before resource consumption; target
  lists/order and each selected effect slot; cast cancel/pushback/channel ticks;
  aura application, first/last tick, refresh/stack/rank replacement, dispel,
  death removal and proc recursion. Maintain explicit support/unsupported lists
  for effect IDs, aura IDs and referenced scripts.
- **Next port entrypoints:** `CheckCast` plus `HandleEffect` for a bounded native
  direct-damage/heal subset, then holder/aura lifecycle. Compare real native
  semantics before enabling any spell; never replace an unsupported native spell
  with an authored one-effect ability and label it ported.

## 6. Items, inventory and loot

- **Verified upstream:** [`Item::Create`, `CreateItem`, `LoadFromDB`,
  `SaveToDB`, `SetEnchantment`][item]; [`Player::CanEquipItem`, `StoreNewItem`,
  `EquipItem`][player]; [`LootStore::LoadLootTable`, `LootStoreItem::Roll`,
  `Loot::FillLoot`, `LootTemplate::Process`][loot].
- **Current fork:** source-shaped ordinary backpack/corpse loot and a separate
  positive `CmangosAddItemV1` admin policy have synthetic differential and durable
  component coverage. Admin grants allow exact partial capacity/unique results,
  self-unbinding, retained spell/set/quest metadata and real empty carried Bags.
  Backpack-only positional move/swap/merge now has a source policy, transactional
  runtime command and existing stock-UI command/protocol integration. It does not
  enable production inventory. See the [movement boundary](../crates/benilla-world-runtime/src/native/INVENTORY-MOVEMENT.md).
  Random properties, scripts and timers remain explicit blockers. Empty carried
  bags do not establish usable equipped containers, item use or equipment/stat
  application. The complete native catalog and production admission are not yet
  established; no item grants are enabled in the live application.
- **Dependencies:** item templates, equipment slots, ownership/binding, capacity,
  durability/enchantments, random properties, item spells, recursive loot
  references, conditions, quest needs, group rights and persistent transactions.
- **Required parity tests:** stack/split/equip/full-bag cases; unique/bound and
  class/level restrictions; loot group/reference/exclusive selection; random
  count and quality/quest/reference rate handling; exactly-once allocation under
  retries and crash. A global probability multiplier is not automatically equal
  to upstream's distinct configured rate categories.
- **Next port entrypoints:** plain inventory invariants and `CanEquipItem` before
  stat application; isolate `LootStoreItem::Roll` and `LootTemplate::Process`
  with an explicit deterministic RNG stream and a fully resolved loot closure.

## 7. Quests

- **Verified upstream:** [`WorldSession::HandleQuestgiverAcceptQuestOpcode`,
  `HandleQuestgiverChooseRewardOpcode`, `CanInteractWithQuestGiver`][questhandler];
  [`Player::CanTakeQuest`, `AddQuest`, `CanRewardQuest`, `RewardQuest`,
  `KilledMonsterCredit`][player]; [`Quest::XPValue`][questdef]; loader/relation
  validation in [ObjectMgr.cpp][objects].
- **Current fork:** `KillQuest` represents one to four ordered creature-kill
  objectives, fixed rewards and start/finish relations. The data crate executes
  none of them. An original accept/kill/turn-in runtime does not establish native
  eligibility, objective credit or reward semantics. The separate `quest_lifecycle`
  reducer now ports bounded ordinary acceptance, signed prerequisites, kill/talk/item
  credit, completion and reward orchestration against pinned source probes. It needs
  an audited transactional inventory/XP/level host, authenticated commands and atomic
  persistence before native runtime admission; unsupported quest features reject.
  The native authored-input path integrates explicit faction/rate/spillover closure,
  persisted reputation state, synchronous objective completion/demotion, min/max
  reputation gates and source-ordered initial/visibility/standing events before XP.
  Source-operation replay rejects forged standing/flags and callback snapshots;
  controller tests cover failed-save/reopen and session initialization without an
  extra gameplay tick. Combat reactions, kill/spell reputation and raw-data
  admission remain separate, unavailable capabilities.
- **Dependencies:** prerequisites/exclusive groups, reputation/race/class/level,
  timed/repeatable state, inventory, spells, exploration, escort/game-object
  scripts, group sharing/credit, condition evaluation and persistent quest log.
- **Required parity tests:** eligibility failures and prerequisite branches;
  kill/cast/item/explore objectives; group/dead/out-of-range credit; abandon and
  repeatable transitions; full bags, chosen rewards, max-level money conversion
  and level-dependent XP; duplicate turn-in/reconnect cannot duplicate rewards.
- **Next port entrypoints:** `CanTakeQuest`/`AddQuest`, one native kill-credit
  path, then `CanRewardQuest`/`RewardQuest` and `Quest::XPValue`. The exporter must
  reject a native quest that requires semantics outside that supported profile.

## 8. Creature AI, event AI and scripts

- **Verified upstream:** [`Creature::AIM_Initialize`, `Update`, `SelectLevel`,
  `Respawn`][creature]; [`CreatureAI::AttackStart`, `EnterEvadeMode`][creatureai];
  [`UnitAI::MoveInLineOfSight`, `DoMeleeAttackIfReady`,
  `DoCastSpellIfCan`][unitai]; [`CreatureEventAI::ProcessEvents`, `CheckEvent`,
  `ProcessAction`, `UpdateEventTimers`][eventai];
  [`ScriptDevAIMgr::GetCreatureAI`, `OnQuestAccept`, `OnQuestRewarded`,
  `CreateInstanceData`][scripts].
- **Current fork:** the data profile allows passive or stationary fixed-melee
  behavior. Synthetic target selection is an original harness, not EventAI,
  ScriptDevAI, faction reaction or production creature behavior. The native
  runtime integrates [ordinary health regeneration](../crates/benilla-sim/CREATURE-REGENERATION.md)
  after AI/CombatManager with a persisted 5,000-ms accumulator, current combat/evade
  gates and committed client health. Captured flags/control/power evidence is
  required; active mana/focus/energy regeneration and dynamic overrides are
  explicitly unsupported. This remains authored-fixture component coverage.
- **Dependencies:** faction/reaction and detection, threat, motion/LOS,
  spell/quest hooks, spawn groups/events, script identity, timers and random
  decisions. A script name is a required dependency, not decorative metadata.
- **Required parity tests:** aggro/assistance, threat switches, evade/leash/reset,
  death/respawn hook ordering; EventAI trigger conditions/cooldowns/phases;
  selected escort/gossip/boss scripts; replay independence from hash-map order
  and wall clock. Coverage must name the scripts actually implemented.
- **Next port entrypoints:** `CreatureAI`/`UnitAI` lifecycle before EventAI's
  dispatcher; introduce typed supported actions and fail unsupported ones.
  Port individually needed ScriptDevAI scripts with their dependencies and
  notices. Do not execute imported SQL/script strings as application code.

## 9. Groups and social state

- **Verified upstream:** [`Group::Create`, `AddInvite`, `AddMember`,
  `RemoveMember`, `LoadGroupFromDB`][group]; [`PlayerSocial::AddToSocialList`,
  `RemoveFromSocialList`, `SocialMgr::LoadFromDB`][social].
- **Current fork:** authenticated exploration chat and a fixed simulation roster
  exist. Neither is a Classic party/raid, loot group, friend/ignore system,
  guild, mail or trade implementation. Simulation membership and party
  membership are distinct state machines.
- **Dependencies:** persistent character IDs and names; invitations/leadership,
  raid subgroups, chat permissions, quest/XP/loot sharing, instance bindings,
  privacy/ignore rules, trade/mail transactional item ownership.
- **Required parity tests:** invitation acceptance/expiry, leader change and
  disconnect, subgroup/size limits, group-only field visibility, sharing range,
  looting permissions; duplicate/stale actions cannot create dual membership.
  Extend the inventory for guilds/mail/trade before claiming social completion.
- **Next port entrypoints:** `Group::Create`/`AddMember` with explicit group IDs
  and authority rules, then reward/instance integration. Retain local friend or
  ignore preferences separately from replicated public gameplay state.

## 10. Instances, map transfers and transports

- **Verified upstream:** [`MapManager::CreateMap`, `CreateInstance`,
  `CreateDungeonMap`][mapmanager]; [`Map::CreateInstanceData`,
  `DungeonMap::Add`, `Update`, `Reset`][map];
  [`DungeonPersistentState::SaveToDB`, `DungeonResetScheduler::Update`][mapstate];
  [`MapManager::LoadTransports`, `Transport::Update`,
  `GenericTransport::AddPassenger`, `UpdatePassengerPosition`][transports].
- **Current fork:** exploration renders a fixed meeting-world context; the
  inspected peer movement bridge rejects transport movement. There is no
  reviewed replicated instance binding/reset, cross-map transfer or transport
  simulation. Realm-client map rendering/packets do not provide these services.
- **Dependencies:** map/instance IDs, party bindings, access checks, script state,
  respawn locks, geometry streaming, synchronized world time, transport paths,
  relative coordinates and pet/passenger transfer.
- **Required parity tests:** independent copies of one dungeon, bound-group entry,
  reset/lockout and process restart; transport boarding/leaving, stops, elevator
  motion, map-crossing and disconnect; no duplicate entity/reward on transfer.
- **Next port entrypoints:** `CreateInstance`/`DungeonMap::Add` with explicit
  persistent world-instance keys; then one transport path via `Transport::Update`
  and passenger transforms. Replace wall-clock dependencies with a documented
  common time model, not unsynchronized per-peer clocks.

## 11. Persistence and recovery

- **Verified upstream:** [`Player::LoadFromDB`, `SaveToDB`, `_SaveInventory`,
  `_SaveQuestStatus`][player]; item persistence in [Item.cpp][item];
  [`MapPersistentState::SaveCreatureRespawnTime`, `SaveGORespawnTime`,
  `DungeonPersistentState::SaveToDB`][mapstate]. These are centralized database
  paths; they are not a distributed save protocol.
- **Current fork:** `benilla-profile/src/lib.rs` and `storage.rs` implement local
  owner identity, character metadata and position checkpoints with validation,
  an exclusive writer lock and previous-snapshot handling. They do not persist
  authoritative XP, quests, inventory or a shared world. Synthetic session
  confirmed state is not a durable multi-peer transaction.
- **Dependencies:** complete serialized gameplay state including RNG/timers,
  world/content/rules versions, commit identity, event/idempotency keys,
  migrations, backup policy, transaction boundaries and ownership conflicts.
- **Required parity tests:** save/load equivalence across every supported system;
  crash before/after write/rename/durability boundary; interrupted reward or
  transfer; stale backup/duplicate journal and incompatible content rejection;
  same character joining twice; Windows and Unix storage seams separately.
- **Next port entrypoints:** extract the state categories from `SaveToDB` and
  its subordinate savers, then design explicit snapshots/journal commits for
  the fork. Keep private signing material out of shared snapshots. A local
  checksum or signature does not prevent an owner from rolling back or cheating.

## 12. P2P membership, determinism and authority

- **Verified upstream comparison:** [`World::AddSession`, `UpdateSessions`][world],
  [`WorldSession::Update`][session], [map updates][map] and the database save paths
  show a server/session architecture. No P2P membership algorithm is supplied by
  those entrypoints; distributed membership is original fork engineering.
- **Current fork:** `benilla-p2p` has PSK transport, optional internet transport
  and an owner-identity authentication foundation. `benilla-session/src/lib.rs`
  has a synthetic-only fixed manifest/epoch and all-member input/state voting;
  disconnect/timeouts abort rather than shrink the roster. It has no live join,
  catch-up, reconnect resume, world merge or atomic distributed save guarantee.
  Owner-authenticated exploration wiring was not verified in the inspected app.
- **Dependencies:** stable owner/character/world IDs; content/rules/build
  compatibility; deterministic RNG, time, iteration and numerical policy;
  authenticated command origin; snapshot/journal validation; agreed membership
  epochs and explicit partition/offline behavior. Party leadership is not
  simulation authority, and a PSK is neither individual identity nor anti-cheat.
- **Required tests:** solo-to-multiplayer admission at a committed boundary;
  late join/catch-up, voluntary leave and owner reconnect; peer loss/partition,
  competing membership proposals, replayed/stale commands, duplicate owner,
  conflicting snapshots and hash mismatch; platform replay equality. Test the
  full graph of failure transitions, not only two localhost peers exchanging chat.
- **Next entrypoints:** generalize the original `Session` manifest and commit
  boundary only after the world snapshot is complete. Specify whether peer loss
  pauses/ends an epoch or creates an explicit fork; never silently merge divergent
  reward histories. Hostless replication does not guarantee both progress and
  one shared history through arbitrary network partitions. CMaNGOS logic cannot
  answer that architectural decision.

## Port order and acceptance gates

1. **Define the target profile:** selected native content/mechanics, exact source
   revisions, schema/DBC dependencies and every deterministic deviation. Record
   each unsupported mechanic; no implicit fixed-value fallback
2. **Build deterministic single-player semantics:** creation/stats, spatial
   contract, melee/death, native spell subset, inventory/loot and native quest
   lifecycle. Add AI/script dependencies required by the selected content
3. **Prove the integrated loop:** create, travel, fight, cast, loot, accept and
   complete a native supported quest, die/resurrect, save/reopen. Use owned local
   assets/data for the actual client test; keep them out of source and artifacts
4. **Replicate proven state:** complete snapshots first, then membership and
   recovery, followed by groups/instances/transports. Validate solo and shared
   behavior from the same simulation rather than parallel unrelated runtimes
5. **Expand by audited coverage:** track support for every selected effect, aura,
   condition, AI action, script, item rule and quest feature. Cross-system areas
   such as pets, professions, vendors/trainers, reputation, PvP/battlegrounds,
   guilds/mail/trade and events still require explicit inventory before a
   full-world claim

For every port, retain the upstream notice and precise provenance; identify its
input domain and rounding/time/RNG policy; compare an independently derived
upstream oracle; test event/state ordering and negative cases; then perform real
client integration checks. A deterministic replay establishes consistency, not
correct game rules. A successful authored fixture establishes its own behavior,
not native content compatibility. No gameplay parity test or graphical smoke
test was run as part of this source-inventory document.

## Pinned upstream evidence

[core]: https://github.com/cmangos/mangos-classic/tree/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f
[world]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/World/World.cpp
[objects]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Globals/ObjectMgr.cpp
[dbc]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Server/DBCStores.cpp#L231
[map]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Maps/Map.cpp
[creature]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Creature.cpp
[player]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Player.cpp
[stats]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/StatSystem.cpp#L32-L567
[xp]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Tools/Formulas.h#L238-L407
[movement]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/MotionGenerators/MovementHandler.cpp
[motion]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/MotionGenerators/MotionMaster.cpp#L57-L580
[spline]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Movement/MoveSpline.cpp
[unit]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp
[threat]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Combat/ThreatManager.cpp#L331-L519
[spell]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/Spell.cpp
[effects]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/SpellEffects.cpp
[auras]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/SpellAuras.cpp
[spellmgr]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/SpellMgr.cpp
[item]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Item.cpp
[loot]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Loot/LootMgr.cpp
[questhandler]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Quests/QuestHandler.cpp
[questdef]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Quests/QuestDef.cpp#L171-L231
[creatureai]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/AI/BaseAI/CreatureAI.cpp#L55-L140
[unitai]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/AI/BaseAI/UnitAI.cpp
[eventai]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/AI/EventAI/CreatureEventAI.cpp
[scripts]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/AI/ScriptDevAI/ScriptDevAIMgr.cpp#L262-L411
[group]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Groups/Group.cpp#L97-L460
[social]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Social/SocialMgr.cpp
[mapmanager]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Maps/MapManager.cpp
[mapstate]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Maps/MapPersistentStateMgr.cpp
[transports]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Transports.cpp
[session]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Server/WorldSession.cpp#L350-L582
