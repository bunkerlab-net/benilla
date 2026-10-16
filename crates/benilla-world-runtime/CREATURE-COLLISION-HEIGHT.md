# Initial creature model collision-height ownership

Pinned source: CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`,
Creature.cpp `InitEntry` / `Respawn`, Unit.cpp `GetCollisionHeight` and
ObjectDefines.h `DEFAULT_COLLISION_HEIGHT`. Formula and oracle details are in
[CREATURE-INIT.md](../benilla-sim/CREATURE-INIT.md).

`Profile::initialize_creature_collision_heights()` explicitly derives height from
the already retained [initial-model width source](CREATURE-COLLISION-WIDTH.md).
There is no second caller-supplied model projection, free scalar constructor or
Deserialize implementation for the height source/catalog. The width constructor
captures strict `CreatureCatalog::collision_height_row` field-15 bits alongside
field 14, verifying identical display/model identities and both DBC scale bit
patterns. Width source policy/binding v2 and repeated model/display consistency
include those raw height bits. Capturing height as `u32` never validates or
sanitizes it in width-only mode; malformed height does not change width's admitted
numeric domain. Width arithmetic policy remains unchanged.

The selected opaque `ModelSelectionReceipt` computes height with software binary32
round-to-nearest-even: effective initial `OBJECT_FIELD_SCALE_X` times raw model
height. Exact signed-zero product uses `2.03128f` (`0x4002007e`). Display/render
scales, width, combat reach and bounding radius never replace either operand.
The initial plan still requires positive effective object scale. The bounded
SpellFloat domain and exceptional arithmetic checks remain in force. The source
arithmetic preserves finite negative nonzero results; the runtime's existing
nonnegative GroundSettings height domain rejects them rather than repairing them.

The private height catalog retains each width-source binding and opaque computed
height. Its initializer atomically derives the NPC source-motion collision-height
scalar and validates the full profile before committing. A failed initialization
leaves the complete profile unchanged. Profile validation re-derives the retained
height and compares that motion scalar bit-for-bit. Replacing/removing the width
catalog cannot silently keep or refresh a mismatched height catalog. Repeating
height initialization for the same source is idempotent; an existing mismatched
catalog or independently edited bound motion scalar is rejected.

The runtime owns an optional generation-zero height witness per NPC and freezes
it onto every typed Route. `CreatureRouteRequest::require_collision_height`
validates the complete profile digest, owner generation, spawn/map/content/geometry,
height source and its accompanying width witness, selected model/object scale,
request owner position and exact request/profile collision-height scalars. A
serialized witness is untrusted transcript data and does not prove current
liveness. Replay compares the request built by the runtime; checkpoint restoration
reconstructs the expected source-bound ownership.

Respawn reruns source InitEntry/model selection. Until that later-life selection
is implemented, respawn clears both initial width and height ownership. Later
NPC generations cannot reuse or relabel generation-zero height. Height-independent
routes continue with explicit `None`.

Native format **30**, policy `benilla-native-pathfinder-constructor-dispatch-v30`, retains
optional profile height catalog, checkpoint `source_collision_height`, and typed
Route `collision_height`. Every serialized optional field must exist, using
explicit null for absence. Profile/state/record/checkpoint/melee digest domains
are v30; native29 and every earlier experimental format are rejected without
migration defaults. OriginalFixture v2 and character/profile transports are
unchanged. Native admission remains uninhabited.

This path owns initial NPC height only. Current forms/mounts/scales, cached player
water state, Chase vertical-gate provenance, full Route production and a
private artifact compiler are not supplied. Runtime/controller tests use authored
Plan inputs and MPQ/DBC bytes through the real loader; they establish the bounded
ownership mechanism, not authenticated source completeness or playable admission.

Player initial-model ownership is a separate optional path; see [Player collision height](PLAYER-COLLISION-HEIGHT.md). It does not enable cached water or full Route consumers.
