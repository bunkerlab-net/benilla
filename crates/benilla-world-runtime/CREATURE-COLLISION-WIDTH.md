# Creature model collision-width ownership

Pinned source: CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`,
Creature.cpp `InitEntry` / `Respawn`, Unit.cpp `GetCollisionWidth`, and the
CreatureModelData / CreatureDisplayInfo layouts. Source grants and arithmetic
oracle details live in [CREATURE-INIT.md](../benilla-sim/CREATURE-INIT.md).

`CreatureCollisionWidthSource::from_initial_model` requires an opaque
`ModelSelectionReceipt` minted by successful validated `creature_init::Plan`
resolution and the actual loaded `benilla_formats::CreatureCatalog`. It does not
accept the mutable `InitializedCreature.model`, an asserted width, a bounding
radius, or a render-scale product. Each reachable receipt display scale is checked
bit-for-bit against the loaded display row. The selected display resolves its own
CreatureModelData identity and raw field 14 through that catalog. The same strict
loaded row also retains raw field-15 height bits, without numeric height validation.

The private retained catalog binds the relevant template display choices, raw
scale and effective spawn-template model override, conditional draw ranges and
values in source order, original/other-gender/native selection, selected bounds,
loaded display/model row identities, separate DBC scale bits, raw width/height bits and
computed width. Width source policy/binding v3 includes raw height and generation selection evidence; the width
arithmetic policy remains unchanged. Its complete projection is included in the native profile digest.
Repeated display/model/template/spawn-template identities must agree. The selected
entry, display, bounds and effective spawn-template identity must also agree with
the same NPC's profile and capability source. These are typed capture and in-process
construction contracts, not authenticated database origin, producer completeness,
or proof that the supplied semantic draws came from the source RNG.

Width uses only the initializer's `OBJECT_FIELD_SCALE_X` multiplied by raw
CreatureModelData field 14. Exactly signed zero selects `0.5097f` (`0x3f027bb3`).
Model/render scale and bounding radius never substitute for either operand.
Missing/incomplete rows remain missing; negative and nonfinite widths fail closed.
The existing bounded SpellFloat domain additionally rejects subnormals, magnitudes
over one million, and exceptional arithmetic including underflow/overflow. This is
not unrestricted binary32 input coverage or an admission-policy repair.

The runtime owns an immutable width witness for the selected NPC life. Legacy
profiles without generation ownership retain their generation-zero-only contract.
NearPoint capability ownership is unchanged. Each typed Route freezes the optional
width alongside its existing owner and map pose; width-independent dependency work
continues with explicit `None`. A width-consuming provider must call
`CreatureRouteRequest::require_collision_width`, which validates source binding,
content, selected model, scale/product and owner generation against the retained
catalog. A serialized witness alone is untrusted transcript data and proves no
liveness. The runtime constructs the live request from its executing owner; replay
compares that exact request, and checkpoint restore checks the same catalog.

Source respawn calls InitEntry again and can draw another model/gender. With a
[creature generation owner](CREATURE-GENERATION.md), each life rederives width
from its fresh model receipt and retained loaded rows. Every reachable alternative
is checked before RNG. Compact creation-draw evidence binds source, world, profile
and life; profile-only validation proves source consistency, while runtime and
restore additionally compare the actual current owner. A stale or independently
valid different selection cannot replace it. Without a generation owner, respawn
still clears initial width and later-generation width queries remain unavailable.

Native format **49**, policy `benilla-native-home-wander-v49`, retains
`Profile.creature_collision_widths`, the NPC checkpoint `source_collision_width`,
and typed Route `collision_width`, with v49 profile/state/record/checkpoint/melee
digest domains. New optional serialized fields require explicit null when absent;
omission does not manufacture a default. Native48 and earlier experimental saves
and records are rejected rather than migrated. OriginalFixture v2 and exploration
character/profile transports are unchanged. Native admission remains uninhabited.

The synthetic-only `synthetic_creature_model_fixture` creates invented rows and
an authored temporary MPQ, loads them through the production Chain/DBC path, and
exercises constructor, lifecycle, checkpoint, replay and Route ownership. It is
not a private artifact compiler or a production content producer.

[Initial collision-height ownership](CREATURE-COLLISION-HEIGHT.md) is a separate
explicit opt-in derived from this exact retained source, with no second input
projection and no change to width-only numeric validation.
