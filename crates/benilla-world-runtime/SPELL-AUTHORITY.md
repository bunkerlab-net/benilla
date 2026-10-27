# Bounded current-state spell authority

`Profile::initialize_spell_authority` selects an immutable, non-deserializable
catalog after player, interaction, combat and source-motion initialization. It
removes externally authored `SpellFacts` from the selected execution path. The
legacy unselected synthetic host deliberately keeps its old contract.

This is an intermediate source-compatible domain, not general spell parity or
certification of stock Fireball data. Only active spell ID 133, fire school,
MAGE family 3 with family flags 0, bit 0 or the retained `0x40000001` mask,
ordinary MAGIC damage, one direct
school-damage effect and at most one periodic-damage effect are admitted. All
active effects must use explicit enemy target A=6/B=0, chain count at most one,
and zero radius/mechanic/trigger/modifier fields. Instant entry rejects explicitly.
The raw rank-chain row for ID 133, when present, must describe its first rank;
`GetSpellRank` remains the current source skill value and is not clamped to one.
Current player levels 1–60 and ordinary NPC levels 1–63 are admitted only when the
target level is at most `max(caster level, 20)`. The unchanged extracted
`CalculateEffectiveMagicResistancePercent` body proves zero additional NPC
level resistance exactly in this domain. Higher-than-caster NPCs above 20,
world-boss target-level overrides and partial-resistance paths remain excluded.
Historical source-authority preparation/release/aura witnesses use the same
level-60 ceiling, including after reload.

### Interrupt flags and source-owned melee timers

SpellEntry word 21 is `InterruptFlags`, distinct from words 22/23's aura/channel
flags. The pinned `SpellDefines.h:351–357` defines movement as `0x01`, player
damage pushback as `0x02`, stun as `0x04`, combat as `0x08`, and player damage
cancellation as `0x10`. The selected host admits the low four bits independently
or together. `Spell::update` (`Spell.cpp:3537–3547`) tests movement on position
change; `Spell::Delayed` (`6812`) tests pushback. The pinned source labels
STUN unimplemented and its complete symbol scan finds no symbolic consumer;
actual incoming stun, silence and interrupt effects remain outside
the catalog's closed source domain. Damage-cancel and unknown bits still reject.

COMBAT executes `_handle_immediate_phase` (`Spell.cpp:3446–3453`) against owned
player BASE/OFF timers. Source cooldown, power consumption and cast-result/Go
precede the reset; immediate effect-value and critical draws follow it. The
existing atomic projectile component computes those draws before exposing its
successful launch, then this host resets the timers before publishing buffered
events. This deterministic reset commutes with those draws only in the proved
idle/no-script/no-effect-observer domain; it is not a literal call-order oracle.
The same `Unit::Update`
then decrements both counters by the update diff. Rejection, movement cancellation
and interruption before release do not reset them. The admitted domain is
mainhand-only, idle (no melee-attacking state or victim), form-zero, no disarm,
and attack-speed factor one. General melee attacks, offhand/ranged equipment,
haste and attack-speed modifiers remain excluded by this spell-only domain.
Combat membership alone is not a player melee-attacking state. The separately
selected [player mainhand policy](PLAYER-MELEE.md) supports active melee only
without concurrent spells/projectiles/auras. A spell after an explicit Stop
resets BASE while preserving the source OFF retry residue. Its shared latest-writer
proof replaces the spell-only idle countdown check; the source reset still has
no active melee observer and does not gain a commuting proof for concurrent combat.

Initialization derives the attack-time float fields from source base values and
retained equipped weapon Delay, with source uint32/binary32 conversion. Equipment
break callbacks restore the base field. A required latest reset witness binds
profile/world/player/cast/target identity, historical prepare/release bindings,
the exact derived ReleaseHit receipt, pre-release source-history tuple, executing
event queue/dispatch, source execution and release-time intervals. Exact structural
release-transition validation preserves pre-release pushback state; a still-live
cast must also agree with the retained launch. It survives projectile cleanup,
target death and respawn. Restore re-derives the source interval and exact current
countdown. Rehashing only the outer checkpoint cannot repair a conflicting
receipt/history/owner/epoch binding. This is source consistency validation, not
authentication of arbitrarily rewritten compacted history.

Only the latest reset is retained, with bounded event-queue and progression-prefix
work charged to checkpoint restoration. No reset-history list grows per cast.
Positive effective mana cost binds it to the independently retained latest
TakePower timestamp even after event/projectile cleanup; zero-cost COMBAT and
zero-minimum-projectile-delay COMBAT remain explicitly unsupported. The latter
avoids restoring a transient zero-delay release as a live cast; source speed24
is unaffected. Legacy non-COMBAT spells retain their separate behavior. Owned
commands and restore enforce at most one preparing player cast, so two player
releases cannot occur in the same Unit phase. Co-due notifications retain source
queue ordering; a newer successful release replaces the witness in that order.
Weapon-break/level callbacks change current attack times without retroactively
changing an already-running countdown or its release-time equipment recipe.

The immutable catalog policy is `cmangos-8ec338a1-native-spell-authority-v7`.
Native format 51 uses `benilla-native-home-wander-v51` across profile,
state, record, checkpoint, melee-action, melee-chain and visitor-callback hashes.
The melee-release receipt integrity domain is also explicitly v51.
Native50 and earlier checkpoints/journals and old selected catalog policies
reject, including current outer rehashes and stale historical profile contexts.
Original World v2 and separate character/profile save formats are unchanged.

The retained full-raw ID 133 fixture uses authored source-shaped values, including
the `0x0f` interrupt mask. It is not evidence of actual DBC equality or a complete
C++ lifecycle oracle. The source producer/compiler and native admission remain
closed.

## Raw closure and ownership

Inputs retain all 173 numeric SpellEntry words **after spell_template overrides**,
server attributes and bonus coefficients, cast-time/duration/range/category/facing
rows, NPC template resistance and immunity masks, actual min/max levels and kind,
addon/spawn-addon auras and script/immunity evidence. Profile initialization
replays the seed-independent preparation or completed deterministic initializer
and compares its combat/player genesis.
Its raw initial spells and equipped item proc/set/random/script fields remain
bound to the catalog. Actual initial live-holder aura-interrupt flags reject: even passive numeric
stat auras could otherwise disappear on ACTION/ACTION_LATE while native stats
remained unchanged. A known inactive spell does not create a holder dependency.
The constructor seals all retained initial raw rows; read validation also checks
holder amounts, attributes, effects and control metadata against them. Unknown
source rows never mean empty rows. Equipped on-use spells are conservatively
excluded along with proc/equip spells; no item-use execution is introduced.

All applicable condition, script, proc, affect, bonus, threat and elixir hook sets
must be explicitly complete and empty. Full-raw spell_area evidence is a distinct
complete global raw table, not a spell-ID-filtered empty result. Its scoped
absence contract is described below. The catalog does not
accept an externally asserted resistance, immunity, CanCrit or allowed verdict.
NPC resistances and immunity masks must be raw zero; totems, critters, world-boss
rank/level overrides, spell lists, control and addon/script modifiers reject.
The admitted attribute bits are CANT_CRIT and NOT_SHAPESHIFT; the latter
requires the source-owned form-zero domain. Unknown bits and damage classes
reject before RNG. The positive ID boundary closes source hardcoded LOS, cast-time,
wand-school, rolling-Ignite, aura stack/remove and evade-retention exceptions.

The complete raw domain also retains ProcChance 101 with proven zero effective
proc flags, item subclass mask `0xffffffff` only with equipped class -1, default
1.0 damage multipliers in empty effect slots, and the exact direct bonus
coefficient bits `0x3dfbe76d`. Their source branches are neutral only after the
catalog proves the initialized stat/resistance/skill/reputation holder set,
complete-empty modifier hooks and equipped proc/set/random/script sources. The
explicit queried spell-ID union covers the active identity, every ordered initial
cast (including nonholders), initial holders and
all retained rank-chain references; an empty query result alone cannot establish
that scope. Full-raw profiles exclude the incoming player-slow effect host.

Initial-cast source policy v2 retains the complete bounded starter input and
replays its ordered recursive InitialCast occurrences, including nonholders.
Prepared initialization binds both creation and first-entry plans, their order
evidence and indexed numeric dependencies to the same owned combat preparation.
Its canonical source identity must equal retained future-training input; newly
executed future initial casts cannot disappear behind unchanged final stats.
A strict first holder domain permits source-audited 20598, 20599 and the explicitly
authored test ID 60100 with exact shapes. Diplomacy also needs its separately
bound reputation host.

The prepared nonholder subset retains Dodge81's pending amount request and
Detect3050's no-execution occurrence. Native construction realizes only the
selected first-world-entry plan through the unchanged RNG v2 sampler, then binds
its receipt into every checkpoint; seedless validation consumes no random words.
See [initial-cast genesis](src/native/README.md#initial-cast-genesis) for the epoch,
counter, restore and evidence limits. This synthetic mechanism does not establish
actual Human loaded order, client DBC rows or a complete creation/entry closure;
other unsupported initial casts are rejected rather than silently dropped.

Indexed cast-time and duration evidence distinguishes Unknown, observed Missing,
and Found tuples and binds exact raw indices. Shared indices must agree across
all initial and active spells. No convention about indices 0, 1, 16 or 35 supplies
a missing value. The bounded synchronous holder policy requires a proven zero
cast time and source-derived permanence, absent aura scripts, and no applicable
rank relationship. These supplied evidence inputs are not raw-capture certificates.

Complete applicable spell_group/spell_group_spell and spell_chain rows are retained
and shape-validated, including nonempty rows. They do not have to be absent for
ID 133. `Unit::AddSpellAuraHolder` removes the old same-ID/same-caster holder
before its cross-ID stacking loop. NPCs start without addon holders and only
ID 133 can create a holder here, so that later loop has no pair to compare.
Selection rejects a second raw/available active identity. Current execution and
restore validate every cast/holder binding against the same raw catalog;
unsupported learned identities cannot enter through a checkpoint. A broader
multi-spell host must implement the real pairwise group/rank predicates first.

The source producer is still missing. These explicit input structures describe
what it must capture, not proof that an actual database/DBC closure was read.
Synthetic fixtures use ID 133 with authored values and establish no content
admission. `NativeCapability` remains uninhabited and production startup closed.

## Scoped area-source precondition

Selected full-raw execution requires an owned source-location dependency before
any first/current-frame effect and before an accepted destination becomes the
player pose. The separate GetZoneAndAreaId provider retains actual terrain/VMap
products and complete AreaTable, WMOAreaTable and Map bytes. It binds the exact
map, content, source epoch, inventory, catalog, session, player, tick, ticket and
binary32 pose. It does not reuse liquid observations, which can skip area lookup.
The source selects zone from the resolved row's zone, or that row's ID when zone
is zero; missing row yields zero IDs, while a real row ID zero remains a lookup.
No parent-row lookup or invented map-to-zone assignment substitutes for this.

Every spell-area source copy retains the same complete ten-column raw table.
The host conservatively rejects any potentially applicable row selected by the
active/initial/rank/holder/aura dependency scope or the actual zone/area indexes.
Unrelated rows are permitted: the table need not be globally empty. The source
loader does not insert area-index-zero entries, although global rows can still
matter through spell or aura keys. The host does not guess condition, race,
gender, quest or aura eligibility to waive a potentially relevant row.

A retained location witness must match the current pose and source identity;
missing, stale or inconsistent state rejects. Failed movement does not install
a destination witness. Every new frame after restore obtains a fresh observation.
These are execution-scope preconditions, not invented UpdateZone/UpdateArea
timers or callbacks. Runtime facts validate catalog lookup consistency; only the
installed app provider obtains the flag from owned geometry. Serialized/rehashed
facts alone cannot authenticate a coordinated fabrication of geometry history.

Public source-table evidence and authored byte-fixture tests do not establish
actual Northshire terrain or client DBC equality. Ordinary native admission stays
closed, and actual client assets remain required for any such claim.

## Source phases and spatial dependencies

- Prepare and release CheckCast each use the current staged player/NPC state.
  Readiness precedes LOS; caster-to-target LOS precedes target flags and
  CanAttackSpell; CheckRange precedes its facing branch; power follows range.
  LOS, range result and facing are distinct typed dependencies, not one allowed
  flag. The selected host derives [range](SPELL-RANGE.md) from both current poses,
  raw range row and combat reaches. The optional [facing authority](FACING-AUTHORITY.md)
  derives facing from the current caster pose; LOS remains external. Query failure
  and source rejection preserve this ordering.
- Release HitResult occurs only after release CheckCast/CheckPower succeeds.
  Current level, current crit stats, raw CANT_CRIT and the ordinary NPC domain
  determine hit and critical chances. `zero_resistance_npc_miss` is reused only
  after those exclusions are proved. Target evade is explicitly unsupported at
  this branch, rather than encoded as an invented resist chance.
- Delayed impact rechecks current CanAttackSpell and then source target visibility.
  Relationship failure yields the existing immune outcome; failed visibility
  drops effects. A resisted release never asks impact visibility. It does not
  reroll hit/crit against later stats.
- Periodic ticks borrow the installed holder and its historical binding. They do
  not rerun cast range, hit, crit, relationship or visibility. Raw immunity
  exclusion and the existing evade-absorption path remain separate.

TARGET_UNIT_ENEMY(6) is CHAIN in SpellTargets.cpp. CheckTarget skips caster LOS
for that enumerator, and a singleton chain has no jump LOS. Thus this domain
needs no invented second target-selection LOS and has no admitted empty-target
release path. General empty-target resource-spending launch, packet projection,
instant casts, multi-target chains, reflection, binary/resistance modifiers,
other aura lifecycles and script/proc execution remain concrete later work.

SpellGo projection reads only the selected ReleaseHit receipt (not the unused
chances in ReleaseCheck), with cast/ability/target binding; legacy release receipts
are accepted only for unselected profiles.

The selected host returns before the legacy unconditional combat-stat critical
assignment. CANT_CRIT therefore cannot be overwritten. Hit, direct damage,
critical and periodic dice retain the source component order; a miss consumes
only the hit die, and disabled crit consumes no critical die. At an allowed,
nonimmune successful delayed impact on a living target, random periodic-effect
values are drawn before the hostile-hit/EnterCombat callback. The callback still
observes pre-impact target health and installed auras: damage and aura installation
remain afterward. A resisted, immune, disallowed or dead-target impact consumes
no periodic value draw; a constant periodic value likewise consumes none.

## History, replay and persistence

Spatial requests and internally derived semantic receipts bind the profile,
generation, ordered frame witness, both current poses/levels and phase history.
The currently executing cast is borrowed from its real frame owner. During a
player EventProcessor callback its queue is temporarily outside State, so the
receipt additionally includes `EventCallbackSnapshot`: the actual staged queue,
pending DueEvent and callback ordinal. Prepare/periodic phases use the queue
already in State. The snapshot is data only and cannot resume a transaction.
No reply is replayed as if it were current mutable state.

Restore validates each historical raw binding and mana cost, then re-derives
stored release hit/crit chances from the validated **release-time** level and
stat recipe, including progression training/derived-call history prefixes. Later
level changes or equipment breaks cannot replace those release facts. Without
progression history, selection requires all equipped items to be non-durable,
so the immutable genesis crit fallback is sound. Mutable gear without its
historical owner rejects explicitly. No synthetic past-stat assertion is used.

Derived receipt verification, missing spatial evidence and late failures roll
back the whole staged state, command sequence, scheduler, RNG and tick record.
The controller/client/store regression additionally faults durable checkpoint
publication after a successful source release: publication remains pending,
retry publishes once without rerunning dice, and reopen preserves continuation.

This changes the experimental **simulation checkpoint/record** contract to native
format 51, `benilla-native-home-wander-v51`, with v51 profile/state/
record/checkpoint/action/callback hash domains. Native50 and earlier reject without
an implicit migration. Separate character/profile save formats are unchanged.
Incompatible open preserves current and previous save files; it does not replace
them with a new world. Raw catalog changes also change the profile digest.

## Provenance and evidence

GPL-3.0-or-later repository ports of GPL-2.0-or-later CMaNGOS Classic,
Copyright CMaNGOS contributors, no warranty. Pinned source:
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`.

- `Spell.cpp`: CheckCast (4676 onward), CheckRange (6219), AddUnitTarget,
  FillTargetMap/CheckTarget (7048), DoSpellHitOnUnit (1362)
- `SpellTargets.cpp`: target 6 CHAIN table and singleton filtering
- `Unit.cpp`: MagicSpellHitResult/SpellHitResult (2872–2979),
  CanCrit (3448), CalculateSpellMissChance (3855), CalculateSpellCritChance (3828),
  CalculateSpellResistChance (4002), AddSpellAuraHolder (4690 onward),
  RemoveNoStackAurasDueToAuraHolder and RemoveAurasWithInterruptFlags
- `Relations.cpp`: CanAttackSpell (1065) and CanAttackInCombat
- `Creature.cpp`: raw resistance creation and level/rank semantics
- `SpellAuras.cpp`: periodic damage tick and holder construction/removal
- `SpellMgr.h/.cpp`: IsIgnoreLosSpell, evade/death helpers, cast time,
  damage-class/family/positivity predicates and spell-chain data
- `SpellStacking.cpp`: raw spell-group load and pairwise holder rules
- `src/shared/Util/Util.h`: chance_u truncation

The checked-in 350-row oracle extracts unchanged CalculateSpellMissChance,
CalculateSpellCritChance and chance_u bodies from hash-pinned source, with
explicit ordinary zero-modifier C++ stubs. This is a **bounded numeric oracle**,
not a full CanCrit/Spell lifecycle differential. It covers 25 level pairs, seven
crit float encodings and CANT_CRIT on/off, with no-fast-math, contraction disabled
and UB/float-cast sanitizers. Only CRLF-to-LF normalization is permitted for
source/fixture checkout portability. Regenerate with:

```
python3 crates/benilla-world-runtime/tools/generate_spell_authority_fixture.py /path/to/pinned/mangos-classic
```

Runtime regressions separately cover actual prepare/release/impact/periodic
ordering, real quest reward stat changes, unsupported/unknown raw closures,
hardcoded exception IDs, crit suppression, exact miss draw use, stale/forged
receipts and scheduler contexts, rehashed historical launch tampering, format-10
refusal, durable publication faults and checkpoint/replay/reopen continuation.

### Delayed-impact periodic value / hostile callback ordering

The original `Spell.cpp` at the pinned revision above (full-file SHA-256
`ebcabd3073f188541e6a1abcc82a8d0ee6d48d360dc5ab16d9e63b3e87d75201`)
calls `ExecuteEffects` at line 1451 before `CasterHitTargetWithSpell` at 1454,
and installs the created aura holder at 1456 onward. `HandleEffect` calculates
effect values at 4597–4600. The immediate-launch path at 1640–1652 already
executes its applicable damage effects before its critical roll; it is unchanged.

`native::engine::Frame::casts_one` therefore draws only the existing eligible
periodic effect values before its synchronous hostile callback. It does not
move target damage, aura extraction/installation, or general scheduler phases.
The `periodic_impact_roll_precedes_hostile_callback_without_early_health_or_aura_mutation`
regression starts a real scheduled traveling cast, then checks the callback's
complete state witness and pre-impact health/phase. Its gating matrix checks
exact draw records/counter, callback presence and aura installation for successful,
resisted, immune, disallowed, dead-target and constant-periodic cases. The original
ordering fails its witness check; this is a source-order regression rather than
a claim of a full C++ spell-lifecycle differential.

This source-order correction is a dependency of the format-34 integration and
must not be published alone under format 33. `TickRecord.replies` retains each
`AuthorityReply.request`; its `InteractionWitness` hashes the complete staged
state including RNG position. `ReplayAuthority` compares requests and derived
replies exactly. Moving the draw therefore changes persisted callback evidence
even when the final RNG position and serialized member shapes are unchanged.
The integration must reject format-33 records/checkpoints after recomputing outer
hashes; unchanged hash domains cannot be used as an implicit migration.
