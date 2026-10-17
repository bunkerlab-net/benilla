# Initial player model collision height

The optional `Profile::initialize_player_collision_height` reads the successful
starter initializer's private race/class/gender identity, selects a strict loaded
`ChrRaces` display through `CharCreateCatalog::native_player_display`, and reads
that display's actual `CreatureDisplayInfo` / `CreatureModelData` height row. No
public constructor accepts a caller-selected tuple, display/model ID or height.
`CharBaseInfo` eligibility is the existing bounded client rule, not proof that
server `playercreateinfo` or other native source rows are complete.

The source is CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`: `Player.cpp:17491–17526`
(`InitDisplayIds`), `Object.cpp` (`_InitValues`, `SetObjectScale`, `SetFloatValue`),
`Object.h:364–367` (`GetObjectScale`) and `Unit.cpp:11930–11957`
(`GetCollisionHeight`). See the [simulation arithmetic contract](../benilla-sim/PLAYER-MODEL.md)
for pinned exact-body oracle inputs, compiler profile and numeric exclusions.

## Source ownership

Fresh player object scale is the separately rounded product of display scale and
model scale. `SetFloatValue` preserves fresh stored +0 if the product is either
zero. The raw product remains retained by the arithmetic receipt. The getter
repairs either stored zero to one; native-model raw height is then multiplied by
that effective scale. Only an exactly zero height product becomes `2.03128f`.
The player path therefore differs from NPC template-owned object scale. Width,
bounding radius, combat reach, appearance/render fallback and another owner's
height cannot substitute for the loaded native-player height.

The opaque source retains identity, selected loaded raw operands and the exact
arithmetic receipt. Validation rederives the complete receipt and the motion
scalar. Initialization commits both atomically; repeated initialization cannot
repair edited identity/scalar or replace an existing source with different rows.
The runtime body domain additionally rejects negative nonzero scales/heights;
signed zeros remain supported. Missing/short/ambiguous rows and unsupported
numeric inputs remain unavailable and do not mint a default height.

## Supported lifetime and mutation audit

This is a fresh, unshapeshifted, source-initialized mage model, not a general
post-login or persisted-character receipt. The witness remains valid only across
operations implemented by this bounded executor:

- `starter_init` validates its closed initial spell/equipment path; executable
  passives are stat/resistance/skill auras 22, 29, 30, 80, 83, 98, 101, 137, 142
  and the fixed permanent general reputation aura156. Owned model sources admit
  156 only after independently revalidating its exact reputation-bearing source
  genesis, including missing/retyped/duplicated-holder rejection. The pinned
  `Unit::GetCollisionHeight` (Unit.cpp:11930–11957) reads object scale and native
  model height; `GetObjectScaleMod` (10278–10294) sums aura61, while aura156 has
  no immediate handler and is consumed only by reputation-gain arithmetic
- `MageEffect` execution is empty/direct damage/periodic damage only; unsupported
  effects are refused, including transform/shapeshift/scale effects
- First-entry passive reconstruction and live training require unchanged initial,
  applied and skill aura sets; item effects/procs and unsupported scripts fail
- The NPC-triggered player slow path admits only its closed decrease-speed aura
  33 source, with no linked/script/extra effects; it does not alter object scale
- Pose, inventory, equipment durability, progression/admin level/skill/stat
  callbacks and corpse-state death transitions have no native display/scale
  operation; release-spirit/ghost transition remains unsupported

External hook absence and complete captured rows remain explicit producer
assertions. This audit proves the represented executor's closed operations, not
native source authenticity or `_LoadAuras`/other full login callback coverage.
GM model/scale/gender reset, transforms, shapeshift, mounted-model behavior,
resurrection/reinitialization and arbitrary scripts are outside this lifetime.
There is no invented model epoch to disguise those missing operations.

## Motion, checkpoints and unavailable dependencies

State owns a private witness bound to actual world/player identity, the complete
profile, content, geometry, map and source. `source_target` uses the owned value
when present. A typed Route or NearPoint snapshots the actual full current player
target, including pose and movement flags, with `target_player_height`.
`require_target_collision_height(profile, world, player)` checks that target,
source, identity and height. Replay compares the exact frame-generated requests;
a serialized witness or hash is not authentication. Untargeted Home routes carry
explicit `None`, as do typed requests whose profiles have no player source.
Height-independent consumers may continue; height-dependent consumers receive
explicit unavailability. Legacy synthetic spatial queries are not source proof.

Native format **39**, policy `benilla-native-creature-equipment-v39`,
binds source, state and required-option request fields with v39 profile/state/
record/checkpoint/melee digest domains. Omitted optional schema fields fail;
explicit `None` remains distinct from ownership. Checkpoints rederive the owner
witness and reject rehashed source/scalar/identity changes. Retained spline Route
targets must preserve the source player unit ID, height, bounding radius and
combat reach, even without optional ownership. Historical target poses need not
match the player's later current pose. Exact transcript replay verifies those
historical requests. Native25 and older experimental saves are rejected without
migration; OriginalFixture v2 is unchanged.

No production provider or admission gate is enabled. Cached current-player water
state, full owner/target water and allowed-Z branches, full Route consumers and
the native raw-content compiler still require independent work. A complete
initial height source does not certify any of them.
