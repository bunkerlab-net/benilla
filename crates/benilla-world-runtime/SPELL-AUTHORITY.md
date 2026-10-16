# Bounded current-state spell authority

`Profile::initialize_spell_authority` selects an immutable, non-deserializable
catalog after player, interaction, combat and source-motion initialization. It
removes externally authored `SpellFacts` from the selected execution path. The
legacy unselected synthetic host deliberately keeps its old contract.

This is an intermediate source-compatible domain, not general spell parity or
certification of stock Fireball data. Only active spell ID 133, fire school,
MAGE family 3 with family flags 0 or bit 0, ordinary MAGIC damage, one direct
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

## Raw closure and ownership

Inputs retain all 173 numeric SpellEntry words **after spell_template overrides**,
server attributes and bonus coefficients, cast-time/duration/range/category/facing
rows, NPC template resistance and immunity masks, actual min/max levels and kind,
addon/spawn-addon auras and script/immunity evidence. Profile initialization
replays the original player initializer and compares its combat/player genesis.
Its raw initial spells and equipped item proc/set/random/script fields remain
bound to the catalog. Initial aura-interrupt flags reject: even passive numeric
stat auras could otherwise disappear on ACTION/ACTION_LATE while native stats
remained unchanged. Unknown source rows never mean empty rows.

All applicable spell-area, condition, script, proc, affect, bonus, threat and
elixir hook sets must be explicitly complete and empty. The catalog does not
accept an externally asserted resistance, immunity, CanCrit or allowed verdict.
NPC resistances and immunity masks must be raw zero; totems, critters, world-boss
rank/level overrides, spell lists, control and addon/script modifiers reject.
The only admitted attribute bit is CANT_CRIT; unknown bits and damage classes
reject before RNG. The positive ID boundary closes source hardcoded LOS, cast-time,
wand-school, rolling-Ignite, aura stack/remove and evade-retention exceptions.

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
critical and periodic dice retain the existing source component order; a miss
consumes only the hit die, and disabled crit consumes no critical die.

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
format 27, `benilla-native-owned-player-environment-v27`, with v27 profile/state/
record/checkpoint/action hash domains. Published formats 10 through 19 reject without an
implicit migration. Separate character/profile save formats are unchanged.
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
