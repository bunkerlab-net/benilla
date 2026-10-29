# Bounded ordinary creature initialization

This is an arithmetic/data-dependency port, **not native world admission**. It
turns validated typed local rows into selected creation/effective stats and
model geometry/speeds, initial mainhand equipment metadata and the bounded initial
addon sheath byte. It neither creates a native profile nor proves that the
caller captured every relevant dependency. No positions, factions' hostility,
classlevel values, models, rates, damage constants, AI decisions or game rows
are invented. The committed fixtures contain synthetic arithmetic cases only.

Policy: `cmangos-8ec338a1-ordinary-creature-init-softfloat-v4`.
The API is `creature_init::prepare(Input)` followed by `Plan::resolve` with
explicit semantic draws. `SpawnTimingInputs` separately resolves the later
respawn draw/corpse cap. `regenerate_out_of_combat` is a narrowly named live-state
projection; a caller must enforce its alive/out-of-combat/no-modifier domain.
These helpers do not mutate world state, consume an RNG or perform I/O.

`Plan::resolve_with_model_receipt` returns the same public arithmetic result plus
an immutable, Serialize-only `ModelSelectionReceipt`; existing `resolve` delegates
and discards the receipt. Its model-only projection retains raw template choices
and scale, the resolved spawn-template identity/model override, sorted reachable
display scales and gender mappings, ordered model/gender/level draw identities,
ranges and values, selected pre-/post-gender display and effective object scale.
It also retains the selected immutable initial SQL bounding radius/combat reach,
so a consumer can check its NPC AI profile without treating either as DBC width.
It is minted inside the validated plan, not reconstructed from the caller-mutable
`InitializedCreature.model`. It omits unrelated initialized stats and does not
authenticate the input capture or turn supplied draws into a live RNG transcript.

`Input`, every nested source/value type, `SelectionRolls` and the full
`InitializedCreature` output support serialization without adding deserializers.
Raw source serialization retains every supplied field and row in original order,
including unrelated rows, inactive legacy values, signed-zero float bits and
`Rows::Missing` versus `Rows::Complete([])`. It never serializes just the selected
model projection as a substitute for full source identity. Output serialization
is still a caller-mutable arithmetic result, not an immutable admission receipt.

`Input::validate_retention_bounds` enforces a local aggregate budget of 4,096
rows and nested aura IDs across classlevel, model, display, resolved-spawn,
equipment-template, item-metadata, spawn-addon, template-addon and external-feature
collections. It also bounds retained storage to 262,144 bytes (`size_of::<Input>()`
plus row count times row size for each collection, plus four bytes per aura ID
and UTF-8 name bytes in the required complete `string_id` catalog).
Outer row counts are checked first; only then are the bounded number of nested
vector lengths read. Aura values are never scanned by the budget check. This
local storage bound does not claim serialized wire size, source completeness or
valid row contents. `prepare` runs it before allocating keyed maps or evaluating
stats; consumers must also run it before cloning or serializing retained source.
The cap is a resource bound, not a native database rule or wider numerical domain.

A private owner can retain one complete `Input` per spawn source, bind its full
serialized content, and retain the selected rolls and initialized result for each
generation. Revalidation uses `prepare(&retained_input)` followed by
`resolve_with_model_receipt(retained_rolls)` and exact result comparison. This
avoids copying raw source into every generation receipt. Generation identity,
actual RNG advancement, initial create/resource application and reset/respawn
ownership remain the consumer's obligations; this module invents none of them.

The receipt's `collision_width(display_id, model_id, raw_width)` checks the selected
display and nonzero DBC model identity, then executes Unit.cpp:11986–11987 with
software binary32: effective object scale times raw CreatureModelData.CollisionWidth;
only a product equal to signed zero selects ObjectDefines.h's `0.5097f` default.
Negative nonzero products are preserved, never repaired into the default. Normal
SpellFloat bounds and arithmetic-exception refusals still apply, including
underflow/subnormal/out-of-range products. Model/render scale is not multiplied.
The immutable `InitialCollisionWidth` proves this arithmetic only. A runtime
consumer must obtain width and ModelID directly from its loaded CreatureCatalog,
compare the receipt's display-scale projection with that reader, bind selected
rows/content revision and owner generation, and retain that source binding in its
own private catalog/state/request. A supplied scalar or ID alone is not provenance.

Policies: `cmangos-8ec338a1-initial-model-selection-receipt-v1` and
`cmangos-8ec338a1-initial-collision-width-softfloat-v1`. Both are initial-only:
Creature.cpp:1919 calls ResetEntry(true), which reruns UpdateEntry/InitEntry and
model selection (636–654/506–508). Respawn, scale changes and native-model
substitution invalidate a live owner's use until separately modeled/rebound;
receipt cloning does not implement those callbacks or open native admission.

The same selection receipt exposes
`collision_height(display_id, model_id, raw_height)` under policy
`cmangos-8ec338a1-initial-collision-height-softfloat-v1`. It multiplies raw
CreatureModelData field 15 by retained positive effective initial object scale;
only an exact signed-zero product selects native `2.03128f` (`0x4002007e`).
The opaque `InitialCollisionHeight` retains the identity, scale, raw height and
result, with no deserializer or public constructor. Negative nonzero products
remain negative. Width, SQL bounding radius/reach and model/render scale are
independent. The source's mount branch is commented out and its active lookup
uses native display, not current display. [INITIAL-COLLISION-HEIGHT.md](INITIAL-COLLISION-HEIGHT.md)
records source hashes, oracle profile, numeric refusals and the stored-zero-scale
getter boundary. This remains initial arithmetic, not live form/scale support.

## Pinned source and license

CMaNGOS Classic, CMaNGOS Project contributors, revision
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, GPL-2.0-or-later.
Rust translation and adapters are GPL-3.0-or-later. Upstream attribution/license
texts remain under [licenses](licenses/AUTHORS.md) and the crate's
[provenance](PROVENANCE.md). The combined executable remains GPL-covered.

Source files, with links pinned to that revision:

- [Creature.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Creature.cpp):
  `InitEntry`, `UpdateEntry`, `SelectLevel`, `ChooseDisplayId`, `LoadEquipment`,
  `SetVirtualItem`, `hasWeapon`, rank-rate lookup,
  base walk/run speed setters, `LoadFromDB`, `RegenerateAll`, `RegenerateHealth`,
  `RegeneratePower`, `GetCreatureAddon`, `LoadCreatureAddon`
- [ObjectMgr.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Globals/ObjectMgr.cpp):
  `LoadCreatureClassLvlStats`, `GetCreatureClassLvlStats`,
  `GetCreatureModelRandomGender`, template/model loader validation,
  `LoadCreatureSpawnDataTemplates`, `LoadCreatures`, `LoadEquipmentTemplates`,
  raw item Class/SubClass/InventoryType/Sheath/DamageType validation
- [StatSystem.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/StatSystem.cpp):
  creature stat/armor/AP/damage/health updates, conditional disarmed-weapon
  modifier and shared mana/stamina formulas
- [Unit.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp):
  neutral constructor modifiers, create powers, total stats/resistances,
  model normalization, base movement speeds, speed update/minimum-rate clamp,
  resource clamping
- [Creature.h](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Creature.h),
  [CreatureDefines.h](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/CreatureDefines.h),
  [Unit.h](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.h):
  raw field types, regeneration flags/predicates, no-crushing flag and 5,000 ms
  NPC regeneration interval, virtual-item byte offsets and `CanUseEquippedWeapon`
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
  class 1/2/4/8, selected levels 1 through 20, physical mainhand, bounded initial
  mainhand equipment, no mounts, initial auras, UnitMods or custom creation hooks
- Actual template fields, classlevel SQL rows, model SQL rows and local
  `CreatureDisplayInfo` identity/scale bindings; all reachable levels and model
  alternatives are validated before any draw is requested
- `Rows::Missing` is different from a captured empty result. Missing classlevel,
  selected/other-gender model, DBC display, resolved spawn template, either addon
  query or external
  dependency evidence fails. Duplicate keyed rows fail. A rejected classlevel
  getter row does not silently become a legacy fallback or fabricated stats
- One resolved `creature_spawn_data_template` row, including the actual Entry=0
  default selected by the native loader. Model/faction/NPC/unit flag overrides,
  current HP/mana and run-on-spawn are supported. All raw override sentinels are
  checked. Raw equipment-template and item-metadata closure is retained even
  for overridden template references; missing is not inferred to mean empty
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
  flags is a separate obligation, not certified by this arithmetic result. Raw
  `ExtraFlags` is retained independently; forced dual wield (`0x00800000`) is
  rejected even when the selected offhand item is zero

## Initial equipment and static disarm

`Input.equipment_templates` retains ordered raw `creature_equip_template` rows
(`entry`, all three `equipentry` values). `item_templates` retains raw identity,
Class/SubClass, display ID, signed SQL Material, InventoryType, sheath, Delay and
all five Damage lines. Both collections require explicit Missing/Complete state,
even when no equipment is selected. Remaining item columns require disposition
by the capture owner; this metadata is not an inventory item-use profile.

All supplied rows are validated before any draw. Duplicate keys fail, raw item
values requiring native Class/SubClass/InventoryType/Sheath/DamageType repairs
fail, and all nonzero equipment item references must resolve. Native equipment
validation permits InventoryType 13,14,15,17,21,22,23,25,26,28 in any slot without
additional per-slot or class/inventory compatibility restrictions. Display ID
zero is permitted; these numeric fields require no invented ItemDisplayInfo
lookup. Material's SQL signed-tinyint range is -128 through 127. Native GetUInt32
then uint8 narrowing is preserved, including -1 becoming byte 0xff.

A nonzero raw template EquipmentTemplateId must resolve even if the spawn later
overrides it, because ObjectMgr validates that reference independently first.
Spawn EquipmentId=-1 inherits it; a positive ID selects the spawn equipment row;
zero skips LoadEquipment and leaves the fresh zeroed state unchanged. Template
zero uses the force-clear path. Equipment row entry=0 can be retained but is never
consulted by LoadEquipment(0). A selected nonzero all-empty row preserves its
nonzero equipment ID. A missing positive spawn ID is rejected rather than
emulating LoadEquipment's stale-state-preserving no-op. These fresh-constructor
equivalences do not implement later ResetEntry/equipment transitions.

`InitializedCreature.equipment` retains the selected ID and optional mainhand
item's identity, display ID and two packed info words. Class, SubClass, Material
and InventoryType are successive bytes of word zero; sheath is word one's low
byte and remaining fresh bytes are zero. Server `hasWeapon(BASE_ATTACK)` checks
packed Class==2, including zero-display and zero-item-damage weapons.
`unit_bytes_2` is fresh UpdateEntry's melee sheath and aura misc flag word,
`0x00001001`, with its low byte then replaced by a selected bounded addon sheath.
This is separate from item-specific sheath metadata.

Effective raw UnitFlags come from the resolved spawn override or template
fallback. A mainhand Class==2 weapon plus static DISARMED (`0x00200000`) selects
the exact final binary32 0.5 damage multiplier. AP, classlevel/template damage,
base weapon damage and creature attack interval retain their own source formulas;
item Damage and Delay never replace them. The earlier unhalved SelectLevel update
is validated separately, preventing disarm from masking an unsupported transient
numeric field. No public caller-supplied disarm boolean is added.

Selected offhand/ranged items, forced dual wield, loot/event/script equipment
writers and existing equipment state remain unsupported. Dynamic disarm aura
application/removal, forms, later swaps and respawn need separately owned lifecycle
semantics. This mainhand arithmetic does not implement NPC-as-defender disarm/parry.
The model receipt keeps its model-only v1 policy and names initializer v3 as its
dependency; full source/output identity includes the new raw rows and derived fields.

## Initial spawn/template addons

`Input.spawn_id` is the exact nonzero database GUID, in the native uint32 domain.
`spawn_addons` and `template_addons` are separate complete exact-key queries of
`creature_addon` and `creature_template_addon`. Each collection permits zero or one
row; duplicate or foreign keys fail. `Rows::Missing` fails even when the other
query has a row. The complete-empty contract does not itself prove a capture was
performed. The private owner must bind this spawn identity and capture evidence.

Both row types retain all typed payload columns: mount, stand state, sheath state,
emote, move flags and an ordered vector of aura IDs, including duplicates or zero
IDs on rejected inputs. Raw parsing, nullable SQL aura text and query evidence
remain the capture owner's responsibility. The bounded primitive cannot clean up
an invalid aura string or treat an incomplete query as empty. `move_flags` is
retained and may have any uint32 value because the pinned `LoadCreatureAddon`
never reads it; accepting it grants no movement capability.

`GetCreatureAddon` first looks up the database GUID for an ordinary nonpet, then
falls back to the template entry only if no spawn row exists. A spawn row replaces
the whole template row; a zero sheath still overrides a nonzero template sheath.
`LoadCreatureAddon(false)`, called by `Create` after `UpdateEntry`, writes the
selected sheath to offset zero of `UNIT_FIELD_BYTES_2`, preserving all upper bytes
and the fresh `UNIT_BYTE2_FLAG_AURAS` flag. Neither query having a row preserves
the fresh melee sheath. Admitted sheath states are 0, 1 and 2. Addon selection and
application consume no randomness and do not affect equipment metadata or stats.

Nonzero mount, stand state or emote, any nonempty aura vector and sheath states
outside 0..2 are rejected before selection. Both source rows are checked, including
an overridden template, rather than silently dropping unsupported dependencies or
emulating native loader repairs. Pet GUID lookup, mounted/model changes, stand and
emote projection, spell execution, dynamic addon replacement and later external
writers remain unsupported. `ExternalFeature::Addon` continues to reject addon
writers outside these retained initial rows.

This initializer branch does not relax another source catalog's admission rules.
In particular, the independent creature-capability source still excludes applicable
addon tables; a consumer composing those catalogs must retain that separate guard.
This is arithmetic and full typed source retention, not native data admission,
live respawn/reload ownership, a data compiler or proof of a running creature.

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

- `creature_equip_template(entry,equipentry1,equipentry2,equipentry3)` closure for
  raw template and resolved spawn equipment references, plus matching raw
  `item_template` metadata with signed Material, Delay and all Damage lines.
  Complete-empty claims require actual bounded source-query evidence

- `creature_spawn_data(guid,id)` for each selected GUID; full selected
  `creature_spawn_data_template(Entry,NpcFlags,UnitFlags,Faction,ModelId,
  EquipmentId,CurHealth,CurMana,SpawnFlags,RelayId,StringId,Name)` rows including
  the actual default Entry=0. Preserve signed sentinel fields
- `game_event_creature(guid,event)`, `pool_creature(guid,pool_entry)`,
  `pool_creature_template(id,pool_entry)` with exact GUID/entry selectors and
  captured-empty evidence; applicable event/pool/conditional/group substitution
  requires additional semantics and is rejected here
- `creature_addon(guid,mount,stand_state,sheath_state,emote,moveflags,auras)` and
  `creature_template_addon(entry,...)`, preserving nullable SQL auras in the
  private capture. Bind exact `Input.spawn_id`, both complete typed query results
  and every raw addon payload field. Whole-row GUID precedence and sheath 0..2
  are supported; mount, stand, emote and aura effects remain rejected
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

`tools/generate_creature_init_fixture.py SOURCE_DIRECTORY [--check]` checks source hashes,
extracts 33 unchanged native method bodies plus the inline weapon-usability and sheath bodies,
exact equipment-selection/loader/respawn/fresh-addon fragments, addon enums/struct
and virtual-item declarations
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
- 192 initial equipment-selection/packing/zero-display/zero-item-damage/static-
  disarm cases, preserving native final physical damage and creature attack timing

- 448 addon cases from unchanged `GetCreatureAddon`, `LoadCreatureAddon` and
  `Object::SetByteValue`: 64 fresh initializer cases plus 384 arbitrary prior-word
  cases for upper-byte preservation. These cross absence and sheath 0/1/2 for both
  source rows, independently vary ignored move flags, retain spawn-zero precedence,
  distinguish database GUID from runtime GUID low, and assert zero RNG draws

The first 4,278 fixture rows remain byte-identical, enforced by a generator prefix
hash. The complete 4,726-row fixture SHA-256 is
`1e43a09c982b51ee88b4ed58b85e4c0463feb3905d6ea66c1e9f4a4b547e43da`.
The Rust tests compare all 4,726 rows bit-for-bit and also cover missing/bad
inputs, override sentinels, model selection ranges, unsupported hooks/flags,
source loader bonuses, scale invariance, ambiguous zero-radius repair rejection,
complete ordered source/output serialization, aggregate pre-prepare row/byte bounds,
raw equipment closure, exact addon keys/completeness/whole-row precedence,
unsupported overridden addon rows, nested aura retention limits, ignored move-flag
identity, native allowed inventory types and loader-repair refusal,
forced dual wield/offhand/ranged refusal, static flags and unused item Damage/Delay,
missing-versus-empty inputs and omitted-versus-supplied semantic draws,
signed resistance values, raw legacy AP bounds and transient AP cast rejection.
Negative fractions between -1 and 0 truncate to zero, which is defined and
included in the source fixture. Focused tests/clippy and fixture parity are narrower than
workspace gates or genuine SQL/DBC/gameplay testing. They do not establish native
entry, full AI/combat, app wiring, multiplayer determinism or live parity.

`tools/generate_creature_collision_width_fixture.py UPSTREAM_ROOT [--check] [--sanitize]`
checks LF-normalized complete Unit.cpp and ObjectDefines.h hashes, extracts the
unchanged GetCollisionWidth method/default constant and runs an authored C++ host.
The numeric-only `.test-fixture` contains 2,295 signed-zero, negative, rounding,
scale and explicitly out-of-domain product cases. This oracle tests the width
method, not initialization, a DBC loader or lifecycle. The existing initialization
oracle remains the model-selection/scale source evidence. The generator uses
FE_TONEAREST, no fast math/contraction and optional undefined-behavior sanitizers;
its negative tests reject changed hashes, missing source/default and corrupted or
truncated fixtures, and verify unchanged extraction/LF handling.
The audited oracle profile is GCC 14.2.0/x86_64-linux-gnu, IEEE binary32,
FLT_EVAL_METHOD=0, FTZ/DAZ disabled with gradual-underflow controls. FTZ, DAZ and
non-nearest negative controls must refuse before invoking the source method.
Each run reports the full compiler command, version/target and generated-harness
and output hashes; this is narrower than cross-platform floating-point parity.

## Required template string identity

Raw `Template.string_id1` and `string_id2` and the complete cold
`Input.string_ids` query are retained before loader normalization. Missing is
not empty, including when both operands are zero. `ScriptMgr::LoadStringIds`
uses first-key insertion into its signed ID map; names and raw row order remain
source identity even though `ExistsStringId` only tests membership. Duplicate
names do not merge distinct IDs, and empty/Unicode names are retained. The
bounded port accepts positive `i32` catalog IDs and raw operands `0..=i32::MAX`;
negative/zero catalog IDs and high-bit raw operands are explicitly unsupported.
This is a host domain, not a claim that the source database forbids those rows.

`ObjectMgr.cpp:563–573` changes each unknown nonzero template string ID to zero.
The initializer emits the loaded pair as `effective_string_ids`, **not** a live
object string set. `Creature::UpdateEntry` applies each known nonzero value to
the existing object set without clearing it. Only the genuine generation-zero
constructor adapter starts with an empty set and deduplicates that pair. This
does not implement later entry changes, respawn string accumulation/removal,
event notification, map string indexes or world admission.

Pinned source additions: `ScriptMgr.cpp:1162–1200,3485–3487`
([source](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/DBScripts/ScriptMgr.cpp)),
SHA256 `6880432f6e48590d7da5e6d3745f581df001a73ffee6329f565a3874e6460f88`;
`ObjectMgr.cpp` SHA256
`dd27915ca363f81885d20783fc4f6c0392df20c4e3dfa031faa1a30df7a4b78b`.
The same upstream GPL attribution applies. Authored tests are not imported DB rows.
