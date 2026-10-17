# Source-owned dry execution for habitat 3

Native format 48 (`benilla-native-wander-leg-v48`) supports a bounded dry
execution domain for retained `InhabitType=3` (`GROUND|WATER`). Its predicates
remain `CanWalk=true`, `CanSwim=true`, `CanFly=false`; a dry observation never
rewrites the habitat or selects the ground-only query branch. Wet initialization,
wet movement, water polygons, flight, water-walk, unknown geometry and production
admission remain unavailable.

The pinned source is CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`: `Creature.cpp` InitEntry/UpdateEntry/
ResetEntry, `Creature.h` capability predicates, `GridMap.cpp` IsSwimmable and
GetWaterOrGroundLevel, `Unit.cpp` UpdateAllowedPositionZ, and `PathFinder.cpp`
createFilter/updateFilter. Existing CMaNGOS provenance and GPL notices apply.
No client assets or raw-content compiler are supplied by this component.

## Initialization ownership and source order

`CreatureCapabilitySource.initialization` explicitly selects `Ground` for habitat
1 or `DryWater { map, respawn_position, snapshot }` for habitat 3. DryWater requires
selected generation, model/width and height catalogs, matching retained respawn
coordinates and runtime spawn, RUN-only supported spawn flags, and the existing
control/script/addon exclusions. WALK_IN_WATER and initial UNIT_FLAG_SWIMMING
are excluded. The snapshot binds map, content, source epoch, full inventory,
product policy, effective configuration and the VMap-height setting.

The generation owner selects the model and conditional gender before building
`CreatureInitializationRequest`. The request binds the outer world/profile/
authority/geometry/tick/NpcId, capability/generation/model source identities,
respawn position, selected display/model, object scale, raw and effective height,
model/gender draws, counter prefix, clock and ticket. The selected height is
resolved from the same source model plan, before the level draw. Only after a
matching initialization observation succeeds does level selection continue.
Initial DB respawn-delay selection still follows initialization.

`IsSwimmable` passes selected collision height as its depth threshold. Its nested
all-liquid query uses the separate default classification height `2.03128f`
(`0x4002007e`), not the model threshold. Every nonzero source liquid status reaches
the strict binary32 `level - depth_level > collision_height` comparison. The
executable dry subset accepts only exact `NO_WATER` with no liquid payload;
shallow wet observations that would return false also remain excluded.

The callable application `SourceCreatureInitializationProvider` obtains that
observation from an owned complete-dry MapHeight inventory. Enabled effective
VMap height and separately proven empty dynamic participants remain required.
The provider retains exact request/snapshot identity and one absolute deadline.
Missing product coverage cannot impersonate the source's absent-grid branch or
manufacture a dry answer. Source-context validation rejects foreign-world and
future initialization receipts. Public transcript fields and hashes are consistency
bindings, not proof that a source capture was authentic.

The explicitly synthetic `NativeWorld::synthetic_component_test_with_initialization`
cold-entry method takes an authority. Earlier constructors cannot supply missing
habitat-3 observations. Respawn repeats the same source order for the new selected
model and generation through the ordinary frame authority. Failed observations,
later source work, or publication discard the staged state and RNG suffix.

## Persistent dry movement state

Each generation history requires an explicit optional initialization receipt.
Habitat-3 lives retain their exact dry facts; missing fields do not default to
absence. Restore replays the model/gender prefix, rebuilds the request before the
level draw, checks the saved observation and compares the whole initialized life.
The capability owner derives its witness from this generation-owned history and
retains a separate zero SWIMMING projection alongside the flight-capability bits.
There is no independent caller setter for the predicates or live swimming state.

Source InitEntry only ORs MOVEFLAG_SWIMMING (`0x200000`), and UpdateEntry propagates
it to UNIT_FLAG_SWIMMING (`0x8000`). A later false query does not clear an earlier
wet state. This domain can preserve zero only because every admitted historical
life is positively dry. Death, corpse removal, Home and respawn cannot introduce
an unowned clearing operation. Wet lifecycle continuity needs a separate owner.

The selected policies are creature capability v3, creature generation v4 and
`cmangos-8ec338a1-native-dry-initialization-v1`. The ordinary scalar initializer
remains `cmangos-8ec338a1-ordinary-creature-init-softfloat-v3`; model staging does
not change its arithmetic or draw semantics. Ordinary addon, Home and visitor
closures retain their exact generation-source composition.

## Swimming-capable allowed Z on dry products

The owned NearPoint witness selects an explicit swimming-capable branch. Its
minimum-water-depth threshold comes from the validated current-generation
initialization receipt, not the session's initial-model body height. The receipt
snapshot must match the same loaded inventory and configuration policy. It runs
`Map::GetHeight(..., swim=true)`, retaining the source 50-yard static search and
ordered dynamic-height selection, instead of the ground-only 10-yard search.
It then queries all liquids at `(candidate.x, candidate.y, groundZ)`, including
when Map height is an invalid sentinel. The nested default classification height
and selected collision-height minimum depth remain separate bound inputs.

On the admitted NO_WATER result, GetWaterOrGroundLevel returns groundZ. The source
strict invalid-height and clamp comparisons remain exact, including signed-zero
and equality preservation. The source scalar wet arithmetic has independent
controls, but the loaded provider publishes only complete-dry observations and
never a water-walk branch. Primary and empty-selector NearPoint providers retain
candidate order, reset probes, carried Z, fallback order and cumulative deadlines.
LOS and complete route/body/segment proofs remain independent requirements.

## Filter 13 without wet polygons

`cmangos-8ec338a1-ground-or-water-capable-dry-route-filter-v2` derives initial
include/exclude flags `1/0` for habitat 1 and `13/0` for habitat 3. The latter is
`NAV_GROUND | NAV_WATER | NAV_MAGMA_SLIME`, even at a physically dry position.
Each update queries the executing owner's map position, first all liquids (15),
then water/ocean (10), and requires exact NO_WATER from both. Home's computed
spline launch position cannot replace that owner pose. Dry updates preserve 13;
no getNavTerrain call or nonzero liquid observation is admitted.

The app, native navigation boundary and runtime cache validator carry the actual
filter through constructors, cold/warm queries, retained replans and receipts.
A filter-1 cache cannot satisfy a habitat-3 request; stale generation, source,
snapshot or mesh lineage also rejects. Standalone NearPoint, filter and smoothing
providers have distinct geometry revisions; their independently valid handles do
not establish a composed Route. The integrated same-polygon path requires its
existing `SamePolygonBinding`; the bounded complete-route composition remains
same-polygon. The current width/model receipt reconstructs the selected model and
requires the exact initialization world, generation creation tick, first counter
and model/gender counter prefix. An equal display or height alone cannot satisfy
that binding. The Chase numeric gate uses this current model's combat reach.

Live body height comes from the required current collision-height receipt,
checked against the generation initialization's source/model/scale/raw height.
The raw model-source identity and generation-owned width/height identity are
separate domains and are checked against their respective owners. The frozen
session roster still validates the initial profile; it cannot replace a different
selected model height. A selected habitat-3 radius differing from that roster
remains unsupported. After installing the selected height, the body is revalidated
with `0 < radius <= 4` and `0 < height <= 16`; a larger height is rejected rather
than clamped or admitted through the broader scalar height domain.

A source terminal or headless proposal is not a complete Route. Prefix-only
resolution retains `StrictProductionCorridorUnavailable`. The separate app-local
composition requires the concrete production `LocalCorridor`, its strict segment
proofs, the source VMap ray and the existing deadline. Runtime publication and
commit acknowledgement remain distinct from candidate calculation. The habitat-3
LocalCorridor fixture contains committed filter-13, later-frame rollback/retry and
retained-replan assertions over authored geometry. Those assertions address this
component boundary, not real-content admission.

Filter-13 leaf smoothing/cache support is not a broader complete smooth-route
producer. The loaded mesh still admits only existing validated ground polygons.
Filter 13 alone does not authorize water/magma/slime polygons, wet missing/far-poly
branches, arbitrary normalization or unavailable Home routes. Existing loaded-cache
reopen and production session guards remain.

## Bounded idle EventAI composition

A byte-backed initialization test joins a single authored idle habitat-3 owner,
its generation-bound sheath, ExtraFlags=2 Defensive reaction, ordinary visitors,
and selected AGGRO/TEXT EventAI. Sight notifications leave it idle; an actual
hostile projectile callback starts combat. The test keeps the player in melee
reach and rejects reached route/near-point requests rather than borrowing the
ground-only fixture provider. Its 10% chance branch, successive lives, model and
initialization receipts, RNG history, rollback, replay and compaction share the
same runtime transaction. Camera membership, LOS, models, spells and text remain
authored. The separate combat-entry assistance lifecycle described below is not
established by this component test. No native format or admission change is made.

## Evidence and remaining real-content dependencies

The [IsSwimmable source observer](../benilla-sim/tools/creature_swimming_source/README.md)
and [allowed-Z observer](../benilla-native-geometry-tests/tools/swimming_allowed_z_README.md)
execute bounded source fragments. The InitEntry ordering guard checks pinned
source placement; it does not execute complete InitEntry or its lifecycle.
Source scalar oracles, authored product-byte tests and runtime/controller fixtures
establish different boundaries. Scalar status/depth controls do not establish
geometry; authored bytes do not establish an installed map. This domain cannot
serve as complete admission of actual creature entry 6 or all of its spawns.

The reviewed Classic DB source at
`ec4f596146be6467ea93c57397858e329e2db852` has entry-6 habitat 3 and ordinary
sheath-only addon inputs. Its model-info row 10913 gives radius/combat reach and
gender, but no collision height. Template Scale=0 requires actual
CreatureDisplayInfo 10913 scale, then
its CreatureModelData identity, width and height. Those DBC bytes are not supplied
by the authored fixtures. Their dimensions remain explicitly authored.

The reviewed base database contains 31 direct map-0 spawns, with 24 random and
seven idle movement rows; this base count is not a complete final-update proof.
The template has MovementType=1 and EventAI, while DB spawns select their own
movement type; the bounded ordinary visitor host requires the resolved idle
default. Ordinary SpellList is zero and the separate CharmedSpellList is 605.
Neither an authored idle scaffold nor a charmed-only spell-list field establishes
ordinary runtime source closure. Event 601's 10% AGGRO text choice also needs
complete EventAI, broadcast text, locale and audience ownership. Real random
movement, final source capture, model data, geometry and production admission
remain separate dependencies.

The same base entry has ExtraFlags=2, selecting Defensive reaction, and
CallForHelp=5. A faithful idle subset therefore must not acquire an aggressive
sight attack merely to exercise EventAI. Hostile-spell entry can reach its AGGRO
callback, but complete combat-entry parity requires another source owner:
`Unit::SetInCombatState` disables help checks, queues their re-enable, then calls
`TriggerAggroLinkingEvent`. That path sets the assistance latch, snapshots nearby
receivers with the selected help radius, and queues the linking/assistance
callback. Ordinary AI-notify visitor membership occurs at a different time and
is not evidence for that receiver query or those Unit events. Even a one-creature
authored scene with no helper effects cannot establish their flag/queue lifecycle.
The separately selected [assistance owner](COMBAT-ASSISTANCE.md) composes that
flag/queue lifecycle with explicitly authored complete ordered candidate traversal
and runtime-derived semantic and range denials.
The base dry component alone does not supply it; nonempty dispatch and unknown
traversal remain unsupported.
The ordinary visitor catalog now admits multiple independent spawns of a shared
entry under existing resource limits. This removes a catalog limitation; real
map membership and client assets remain separate dependencies. Selected positive
peer help additionally requires the explicit assistance and predicate owners;
dry initialization alone does not supply them.

Native47 and earlier profile/state/record/checkpoint/melee/callback identities
reject without migration. RNG v2, original World v2, character files and the
native store envelope remain unchanged. Rehashed receipts do not recreate live
loaded authority or erase any of these remaining dependencies.
