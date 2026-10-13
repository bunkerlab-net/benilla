# Bounded ordinary creature initialization

This is an arithmetic/data-dependency port, **not native world admission**. It
turns validated typed local rows into selected creation/effective stats and
model geometry/speeds. It neither creates a native profile nor proves that the
caller captured every relevant dependency. No positions, factions' hostility,
classlevel values, models, rates, damage constants, AI decisions or game rows
are invented. The committed fixtures contain synthetic arithmetic cases only.

Policy: `cmangos-8ec338a1-ordinary-creature-init-softfloat-v1`.
The API is `creature_init::prepare(Input)` followed by `Plan::resolve` with
explicit semantic draws. `SpawnTimingInputs` separately resolves the later
respawn draw/corpse cap. `regenerate_out_of_combat` is a narrowly named live-state
projection; a caller must enforce its alive/out-of-combat/no-modifier domain.
These helpers do not mutate world state, consume an RNG or perform I/O.

## Pinned source and license

CMaNGOS Classic, CMaNGOS Project contributors, revision
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, GPL-2.0-or-later.
Rust translation and adapters are GPL-3.0-or-later. Upstream attribution/license
texts remain under [licenses](licenses/AUTHORS.md) and the crate's
[provenance](PROVENANCE.md). The combined executable remains GPL-covered.

Source files, with links pinned to that revision:

- [Creature.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Creature.cpp):
  `InitEntry`, `UpdateEntry`, `SelectLevel`, `ChooseDisplayId`, rank-rate lookup,
  base walk/run speed setters, `LoadFromDB`, `RegenerateAll`, `RegenerateHealth`,
  `RegeneratePower`, `GetCreatureAddon`, `LoadCreatureAddon`
- [ObjectMgr.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Globals/ObjectMgr.cpp):
  `LoadCreatureClassLvlStats`, `GetCreatureClassLvlStats`,
  `GetCreatureModelRandomGender`, template/model loader validation,
  `LoadCreatureSpawnDataTemplates`, `LoadCreatures`
- [StatSystem.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/StatSystem.cpp):
  creature stat/armor/AP/damage/health updates and shared mana/stamina formulas
- [Unit.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp):
  neutral constructor modifiers, create powers, total stats/resistances,
  model normalization, base movement speeds, speed update/minimum-rate clamp,
  resource clamping
- [Creature.h](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Creature.h),
  [CreatureDefines.h](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/CreatureDefines.h),
  [Unit.h](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.h):
  raw field types, regeneration flags/predicates, no-crushing flag and 5,000 ms
  NPC regeneration interval
- [Object.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Object.cpp):
  `IsUsingNewSpawningSystem` is true when a nonzero database GUID differs from
  the runtime GUID low

The fixture generator pins SHA-256 for every file it extracts. It accepts a
local directory containing those source files; it does not download anything or
read a WoW install. `Creature.h` is the unchanged UTF-8 file from the pinned
repository. No Classic-DB rows, DBC files, assets or full upstream source files
are distributed by this change.

## Exact supported input boundary

- Fresh, living, ordinary rank-zero, uncontrolled nonpet outdoor creatures,
  class 1/2/4/8, selected levels 1 through 20, physical mainhand, no equipment,
  mounts, initial auras, UnitMods or custom initialization hooks
- Actual template fields, classlevel SQL rows, model SQL rows and local
  `CreatureDisplayInfo` identity/scale bindings; all reachable levels and model
  alternatives are validated before any draw is requested
- `Rows::Missing` is different from a captured empty result. Missing classlevel,
  selected/other-gender model, DBC display, resolved spawn template or external
  dependency evidence fails. Duplicate keyed rows fail. A rejected classlevel
  getter row does not silently become a legacy fallback or fabricated stats
- One resolved `creature_spawn_data_template` row, including the actual Entry=0
  default selected by the native loader. Model/faction/NPC/unit flag overrides,
  current HP/mana and run-on-spawn are supported. All raw override sentinels are
  checked. Equipment is allowed only when its effective selection is zero
- Explicit normal health/damage rates and normal corpse-decay seconds. Zero HP
  rate follows the source minimum-one rule; zero resource/template multipliers
  retain the source's different comparison branches. There is no default rate
- All integers used in stat arithmetic/results are bounded to one million;
  signed resistances have the same magnitude bound. `SpellFloat` admits finite
  normal binary32 or signed zero with magnitude at most one million. Invalid,
  overflowed, subnormal and out-of-range float-to-unsigned arithmetic fails explicitly
- The native loader's zero mainhand attack-time repair to 2,000 ms is supported;
  larger-than-60,000 ms timers are outside this port. Zero bounding radii are
  rejected because the native loader can repair playable-race models through
  `ChrRaces`, a dependency this numeric boundary does not bind. Template scale <=0
  uses the first actual template display's DBC scale. No-template-display fallback
  scale, invalid display/gender repairs, other-gender self-reference and entry-1
  model fallback are deliberately unsupported
- The only admitted nonzero static setting is StaticFlags2 NO_CRUSHING_BLOWS.
  Other settings require another concrete branch. Captured type flags are
  retained for the health-regeneration interaction gate; other runtime use of
  flags is a separate obligation, not certified by this arithmetic result

The caller must validate the remaining captured template/spawn fields and
applicable sources before constructing a runnable native profile. A public
`InitializedCreature`, `Rows::Complete([])` or an empty error list is not an
admission token and cannot substitute for source completeness verification.

## Source-order trace

1. Loader preprocessing converts raw classlevel `BaseMana` by subtracting the
   lesser of the raw mana and the integer native intellect bonus. It subtracts
   up to agility*2 from raw `BaseArmor`, and up to max(agility-10,0) from raw
   ranged AP. The raw inputs must be supplied before these transformations;
   passing already-loaded values would double-subtract
2. Template model selection consumes `irand(0, sum(probability)-1)` over nonzero
   display IDs, including a single model and singleton range. A nonzero resolved
   spawn model overrides the template and consumes no model draw. The selected
   original model consumes `urand(0,1)` only when it has an other-gender ID;
   zero chooses the alternate. It does not recursively reroll alternate gender
3. `SelectLevel` consumes `urand(min,max)` only for unequal levels. Forced-level
   and preserve-existing-health paths are outside this constructor
4. Modern classlevel armor uses native `std::round` after the armor multiplier.
   Primary attributes remain binary32 creation values, then truncate to integer
   effective fields. Negative attribute multipliers use native zero creation
   attributes. HP/mana zero multipliers preserve source branches, rather than
   multiplying resources by zero
5. The supported legacy armor branch interpolates sorted template HP/mana
   endpoints at binary32 relative level and uses raw armor/AP/damage. It
   deliberately applies damage multiplier once in `SelectLevel` and again in
   physical damage, as the source does. Negative modern HP/mana sentinel branches
   and negative damage multipliers are rejected
6. The rank health rate truncates to uint32 before the minimum-one clamp and
   creature HP multiplier. Health includes the difference between native
   stamina bonuses for truncated effective stamina and fractional create
   stamina. Mana adds the full effective intellect bonus only when create mana
   is positive, after raw-loader subtraction; the power multiplier is set only
   for the active power type. Armor includes effective agility*2
7. Final mainhand fields use the existing `npc_melee::creature_damage_range`
   port. AP is truncated before AP/14 contributes to damage. The source visits
   strength before agility on its first update, then repeats UpdateAllStats in
   UpdateEntry. A transient out-of-range rogue AP cast or ranged AP cast
   is rejected even if a later/mainhand result would otherwise be representable
8. Source spawn HP/mana overrides clamp to maxima. Zero HP selects full health.
   Zero mana selects full mana only for a mana-class creature with the out-of-
   combat power-regeneration flag; nonregenerating mana zero stays zero. Signed
   nonphysical create resistances remain unchanged under neutral modifiers
9. `UpdateModelData` uses currentScale/nativeScale; these are equal at supported
   initial creation, so SQL bounding radius/reach are not multiplied by the
   template scale. Nonzero template walk/run rates override actual model rates;
   `Unit::SetSpeedRate` clamps each rate to at least binary32 0.01 before
   effective speeds become rate*2.5 and rate*7 yards/second. No mount/aura/wounded
   slowdown or movement-generator inference is made
10. Entry corpse delay uses the template value when nonzero, otherwise the
    supplied normal config. This is named `entry_corpse_delay_seconds` because
    `LoadFromDB` later ALWAYS consumes the respawn draw, including equal bounds.
    `SpawnTimingInputs` applies the native inverted-maximum loader repair and,
    for legacy identical IDs, caps corpse delay to respawn*9/10. It does not
    implement death persistence, timers, forced respawn, reroll/reset or hooks
11. Out-of-combat regeneration accumulates milliseconds and performs at most one
    5,000 ms tick per call. Eligible uncontrolled HP adds integer maxHP/3, mana
    adds trunc(binary32(maxMana)/3), both clamped to maxima. The elapsed remainder
    can still exceed one tick after a large diff, as in native source. Combat
    mana regeneration, spirit/DBC dependencies, controlled regeneration, auras,
    evasion lifecycle and death handling remain external

All simulation arithmetic is `rustc_apfloat::ieee::Single`, round-to-nearest-even
at each source binary32 operation, toward-zero integer conversion, and ties-away
integral rounding for native `std::round`. There is no host-float arithmetic in
the implementation. Tests use host floats only to construct explicit fixture
bit patterns; generated expected outputs come from independent C++ source.

## Required capture evidence and unsupported dependencies

A consumer must revalidate exact selected rows and completeness/binding evidence,
not infer absence from an absent field. The companion exporter owns that work:

- `creature_spawn_data(guid,id)` for each selected GUID; full selected
  `creature_spawn_data_template(Entry,NpcFlags,UnitFlags,Faction,ModelId,
  EquipmentId,CurHealth,CurMana,SpawnFlags,RelayId,StringId,Name)` rows including
  the actual default Entry=0. Preserve signed sentinel fields
- `game_event_creature(guid,event)`, `pool_creature(guid,pool_entry)`,
  `pool_creature_template(id,pool_entry)` with exact GUID/entry selectors and
  captured-empty evidence; applicable event/pool/conditional/group substitution
  requires additional semantics and is rejected here
- `creature_addon(guid,mount,stand_state,sheath_state,emote,moveflags,auras)` and
  `creature_template_addon(entry,...)`, preserving nullable auras. GUID addon
  takes precedence; template addon is fallback. This module rejects applicable
  addons, including ones requiring only a future cosmetic handling branch
- Actual effective AI/spell-list, linking/assistance, spawn-group/conditional,
  initial aura/UnitMod, map/zone/OutdoorPvP, relay/string and encounter-corpse
  dependencies. A complete local capture plus source-aware applicability checks
  can establish absence. Unknown and applicable unimplemented features are
  distinct from proven absence; neither can be silently replaced with neutral
  modifiers or a different AI
- Model counterpart closure and DBC display identities/scales; complete source
  rates; real map/position/terrain/movement inputs and reset/death/respawn hooks
  remain obligations of the caller/runtime rather than this numeric module

The `ExternalFeature` list is a typed unsupported-feature report, not a manual
trust flag or permanent native-support ban. Adding a supported branch requires
its actual data binding and executable source semantics. No cloud access to the
user's game data is required or implied.

## Verification

`tools/generate_creature_init_fixture.py SOURCE_DIRECTORY` checks source hashes,
extracts 26 unchanged native method bodies plus exact loader/respawn fragments
and the numeric prefix of `SetSpeedRate`, then compiles the independent C++
harness with C++17, FE_TONEAREST, no fast math,
no FP contraction, and undefined-behavior/float-cast sanitizers. Expected fixture
outputs never call Rust. Current cases:

- 1,057 classlevel/legacy stats cases across four native classes, levels 1–20,
  rate/multiplier boundaries, fractional attributes and loader clamps
- 12 weighted model/gender selection cases checking native draw consumption
- 1,225 template/model speed precedence cases using native setters, UpdateSpeed,
  SetSpeedRate numerical prefix and GetSpeed, including the minimum-rate boundary
- 1,536 regeneration gate/rounding/accumulator cases
- 256 respawn-range/ID/corpse-cap cases, including singleton draws

The Rust tests compare all 4,086 rows bit-for-bit and also cover missing/bad
inputs, override sentinels, model selection ranges, unsupported hooks/flags,
source loader bonuses, scale invariance, ambiguous zero-radius repair rejection,
signed resistance values, raw legacy AP bounds and transient AP cast rejection.
Negative fractions between -1 and 0 truncate to zero, which is defined and
included in the source fixture. Focused tests/clippy and fixture parity are narrower than
workspace gates or genuine SQL/DBC/gameplay testing. They do not establish native
entry, full AI/combat, app wiring, multiplayer determinism or live parity.
