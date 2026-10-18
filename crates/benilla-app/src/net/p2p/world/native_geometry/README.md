# Local native geometry boundary

This is a bounded, opt-in **client-local** provider. It is not CMaNGOS VMap/MMap
parity, replicated deterministic geometry, or proof of playable native integration.
Production session admission remains closed. The registered service is inert until
an audited session is installed; character selection and exploration do not install it.

## Independent participant and source admission

A session requires a private-field `NonRenderParticipants` proof bound to its exact
content digest and map. A compiler must audit all game-object/door/transport/dynamic
spawns and scripted/runtime creation paths, and the controller must prevent introducing
unsupported participants while the proof is installed. ADT/M2/WMO completeness cannot
prove that absence. The content schema still lacks the required complete audit, so this
proof has no production constructor. Authored tests construct a fixture only.

Static-map admission now has a real read-only producer:
`benilla_formats::NativeM2MapInventory::load(chain, directory)` scans every raw WDT-listed
ADT, referenced WMO root/group, and M2 dependency through the user's patch chain. `NativeStaticMapIndex::from_inventory`
builds bounded spatial buckets from exact transformed source hulls. Run both off-thread
before admission. Install the resulting source snapshot explicitly; never run a whole
map scan in a geometry query or silently install one for exploration. A missing tile,
model/group/doodad, malformed reference, conflicting placement ID or unsupported source aborts the scan.

Dry static WMO placements have an immutable closure over raw root tables, every declared
numbered group, and every MODD M2 dependency, including inactive sets. The raw WMO profile
is version 17 with complete framed tables and geometry, valid MODS/MODR references, finite
transforms, and no liquid source or whole-group liquid override. Missing groups, failed
props, truncated arrays and renderer-recovered omissions cannot produce a certificate.
Unowned MODD records, overlapping selected sets, nonzero MODF flags/unused scale, nonzero
MODD flags, and global-WMO maps remain rejected. This removes the blanket rejection of
maps merely containing a building; real starter-map coverage is not established.

The inventory is a fixed source snapshot. Native entry must bind an immutable asset
source epoch, invalidate the session/index on source replacement, and reverify any
newly loaded assets against it. Digests alone do not keep a mutable installation fixed.
That entry/lifecycle contract is not bypassed here.

## Positive geometry domain

Each certified segment requires all of the following:

- Current map, Map.dbc directory, streamer and strict raw terrain-only WDT agree
- The supporting ADT and eight guard neighbors are loaded/furnished with live terrain
  colliders, exact raw source hashes, and complete dry M2/WMO placement records
- Strict raw MDDF/MMID/MMDX, MODF/MWID/MWMO and mixed MCRF records resolve every placement before tolerant
  rendering can filter it; duplicates, dangling IDs, invalid names/transforms and
  unknown flags reject. Local liquid sources remain unsupported
- Each loaded M2 has a separate private-field collision certificate over its exact
  bytes. A genuinely empty hull is distinct from a missing model, parser failure,
  truncated arrays, partial triangles, invalid indices or nonfinite data
- Each WMO root/group closure reproduces both renderer walking and camera triangle streams.
  Selected MODD indices, M2 witnesses and composed transforms are checked individually;
  a prop marked spawned after a failed asset load still fails coverage
- Placement transforms, normalized asset paths and live hull data match the sources
  exactly. Welds retain namespaced ADT/WMO/prop membership in append order; walking,
  camera and individual MODD members cannot substitute for one another. Every required
  colliding member appears exactly once, and every live scaled vertex/index is checked.
  Pending/disabled/missing/replaced/bare colliders cannot serve as proof
- The full-map source inventory agrees with the current raw guard. Every exterior
  source hull whose XY bound could cross the swept corridor must appear in the actual
  loaded guard proof. A large unloaded model outside the 3×3 guard never becomes empty
- All target-tile chunks have canonical finite center-fan topology and distinct
  footprints. The outward-rounded radius-expanded XY rectangle lies strictly within
  one tile. Every cell with closed overlap must be dry, passable, hole-free and flat
  at the exact common feet height. Both authors of a chunk seam are checked, including
  edge/corner contact and stationary zero-radius queries. Nonflat cells and holes
  outside that rectangle cannot intrude through their proved XY footprints. Slopes,
  tile crossings, steps, drops and height tolerances remain unsupported
- Every intersecting physics collider is the supporting terrain or one of the proven
  static hulls. Exact client walking/camera layers are checked; unknown additional colliders reject
- Because triangle meshes are hollow, an endpoint body whose AABB intersects an
  individual source hull AABB is conservatively blocked. This may reject usable space
  near irregular hulls, but does not bless a body enclosed by a hollow mesh. Two-sided
  hull sweeps, body checks and elevated LOS then detect surface obstacles

The raw WDT, nine ADTs, full-map inventory digest, sorted loaded placement/model
identities, map/tile coordinates and versioned local policy determine the geometry
revision. The flat-cell/seam and conservative LOS policy domain is version 6, independent
of native request contract v4 and any snapshot schema version. Source, transform or
collider replacement cannot reuse an old revision as proof.
The local masks are deliberately conservative. Body sweeps use the client walking filter;
elevated LOS uses the union of walking and camera faces so camera-only DETAIL faces cannot
disappear between clear endpoints. These are not server VMap/MOD_M2/ignoreM2 or near-point parity.

## Player path and request transport

`PlayerMotion` carries a trusted source-resolved body and maximum speed for this
bounded movement state. It does not infer speed from a packet, combat reach or a
renderer avatar. Mount/swim/flight, jump/fall, movement auras and transports require
separate support and remain outside this policy.

`Query::PlayerMovement` is served against the v4 contract: immutable `frame_origin`,
previously accepted endpoints in arrival order, the candidate destination and one
`elapsed_ms` for the whole frame. Every hop spends distance, including backtracking
and returning to the origin. Request count does not renew time. Every segment,
including a stationary one, receives fresh coverage/body checks. Known obstacles or
an exceeded budget return `accepted: false`; unknown geometry fails unavailable.

Local distance bounds are rounded outward, cumulative addition cannot lose tiny hops,
and the budget division is corrected downward when its fused residual shows upward
rounding. The driver independently rechecks the same budget on accepted replies.
This is a conservative local movement policy, not CMaNGOS client anti-cheat parity.

The off-thread `LocalAuthority` uses a bounded 16-job queue and a 500ms absolute
request deadline. The main-thread NonSend service never waits on a channel; at most
eight attempts run per rendered frame, with a 2ms soft execution slice. Explicit
clock expiry is a typed app-local `Deadline`, distinct from missing geometry or
primitive/memory-budget exhaustion. While the original job remains live, frame
expiry requeues its unchanged request, ticket and reply endpoint at the same
bounded FIFO tail, then ends that service invocation. A concurrent refill may
exhaust that slot; this returns deadline-unavailable rather than exceeding capacity.
Actual missing/unsupported/binding errors are terminal even if time also elapsed.
Repeatedly oversized work may still exhaust the original 500ms lifetime; retries
are bounded availability attempts, not guaranteed progress. Generic retry alone
does not make native work resumable; the selected Route and primary NearPoint
continuations retain only the explicitly completed immutable stages described below.
No provider witness or partial Route is reused as completed authority. Other generic
callbacks keep their prior successful-result semantics: the slice limits scheduling,
while the original job expiry and cancellation fence still govern delivery. Only an
explicit provider deadline failure is retried; elapsed time never reclassifies a result.
The concrete Route resolver consumes an attempt-local borrowed validation token
from the mailbox instead of repeating the same successful `Session::body` check.
Only core can construct the token, from the exact immutable session/request;
it cannot be cloned, serialized or retained by the higher-ranked callback. Raw
entry points still validate fully. This certifies neither geometry nor continued
dispatch liveness: Calculate and final reply validation keep all their checks.
Selected initialized Route jobs retain separate context, width, model selection,
height, body, dispatch, model and owner-state validations on the original Work.
A privately constructed frozen-profile/owner proof binds the full request and
live dispatch. It reuses only completed immutable checks, including target-model
validation at the original target-water branch. Raw APIs keep their full checks.
The dispatch subphase also uses that exact completed owner/profile proof instead
of revalidating initialization and the selected immutable profile/configuration.
It still performs constructor owner lookup and enabled selection, receipt dispatch
and live-allocation checks, request hash/position checks, outer callback binding,
and prior-cache checks in their original order. Missing initialization takes the
full validator; wrong outer NPC retains its full error-order fallback. Changed
profile contents or a deserialized/fresh dispatch cannot reuse the proof; equal
transcript bytes alone are insufficient.
WanderPrepare adopts that same job and advances constructor, gate and preparation
observations before job-bound reply validation. Equal-live concurrent jobs share
the retained preparation allocation without replacing one another's evidence.
Fresh LOS, water, terrain and collision observations are never promoted across an
overdue attempt. Cancellation, original lease expiry and final timely certification
still fence every delivered result. These are selected component progress rules,
not a hard real-time claim or broader production admission.

These error variants are crate-private and nonserialized; work limits are unchanged. Each player job
contains at most 64 segments. Structural limits are checked before request cloning
or enqueueing. One corridor caps loaded hull members at 4,096 and aggregate hull
vertices/triangles at 262,144 before transformed allocation. The full-map loader caps
unique model geometry and aggregate placement-transform work; spatial bucket storage
is capped separately. These are work/memory limits, not a measured frame-time claim.

Queries run after Avian's default FixedPostUpdate physics/tree update and transform
propagation. Each full request and monotonically increasing ticket are echoed and
checked. NPC generation, movement IDs, map/content/geometry bindings are retained.
The two source LOS queries and explicitly selected primary NearPoint path described
below have loaded-product providers. NPC accessibility, spell impact visibility,
melee and general near-point fallback queries remain unavailable. Timeout or failure
commits nothing; the controller must retain the exact tick/input for retry or pause.

## Private generated-product prerequisite

`products::InventoriedProducts` reads a bounded exact declared terrain/VMap/MMap file set
from private staging into immutable owned bytes. It validates canonical identities,
versions/ABI declarations, lengths, hashes, framing and direct VMap model-name edges.
Its pinned dry `GridMap` point-height reader has 6,272 source-oracle vectors. Caller
provenance declarations and expected tile coverage do not become audited proofs. The
optional VMap and dry MMap readers separately validate the inventory's opaque internals
before loading/querying them, as additional vetoes described below.
[The exact inventory boundary](products/PROVENANCE.md) remains distinct from these
readers. Terrain interpolation does not enable slopes or production entry.

## Next local extraction and navigation step

[The private exporter/consumer contract](LOCAL-EXTRACTION.md) specifies the pinned
CMaNGOS toolchain, real extractor command names/options, raw-member and output inventory,
patch-chain agreement, VMap/MMap/Detour ABI/configuration, non-render participant audit,
source-epoch lifecycle and fail-closed path consumption. It uses only the user's local
read-only Data and private output; no game assets or generated maps enter the repository.

Full navigation remains distinct: collision height/scale, source near-point target
selection, wounded speed/spline state, liquids, dynamic state and server masks must be
ported or supplied before their capability can be admitted. `PathFinder::calculate`
returning true or a successful generator exit is insufficient evidence.

## Verification

`benilla-native-geometry-tests` includes the actual app mailbox/player/NPC solver and
actual pure terrain/static-mesh/exterior-index proofs without Bevy. It covers cumulative
path overspend, reversal, all-segment checks, rounding, exact binding, queue lifecycle,
corrupt/incomplete hulls, large exterior intruders and displaced f32 tile boundaries.
`benilla-formats` authored fixtures cover strict raw ADT/M2/WMO parsing, mixed placement references, complete root/group/doodad
closure, explicit empty versus failed hulls, missing/changed dependencies and complete-map
source inventory. No original game assets are embedded.

The cell proof exercises both seam axes and four-way corners, mismatched seam heights,
hidden interior hills/dips, radius-only contacts, untouched holes, and an exhaustive
256-fan perturbation oracle around a four-chunk corner. Authored ADT bytes feed the
actual player solver across a chunk seam. These are whole-area checks, not point-height
sampling; genuine sloped movement is still rejected.

Full Bevy type/runtime checks, real installed Data, Windows behavior and live native play
remain separate validation gates. Headless parser/collider fixtures do not establish a
working native starter scene.

## Optional loaded static VMap veto

`InventoriedProducts::load_static_vmap` loads bounded source-generated map/model/triangle
hierarchies and transformed dry collision geometry. Its source-derived nearest-ray query
has exact C++ differential fixtures, malformed-input/reference validation and explicit
missing-primitive errors. [Scope, source grants and evidence](products/vmap/PROVENANCE.md).
The optional provider combines the raw and VMap revisions, checks exact request binding,
and adds a static-ray obstacle veto only after the existing positive corridor proof.
It inherits the mailbox's absolute frame/request deadline. Service replacement revokes
pending and buffered old-authority replies. Production source/participant admission,
Detour/PathFinder and sloped-body support remain closed.


## Optional loaded dry MMap veto

`InventoriedProducts::load_navigation` validates actual dry MMap/Detour sections and
references before native loading. Its exact pinned C++ nearest/corridor/funnel queries
preserve source binary32 semantics and echo map/content/epoch/inventory/policy binding.
`install_with_navigation` composes this after the existing raw corridor and static VMap
veto. Endpoints must not snap and the funnel must remain direct; a successful route
cannot supply any missing body, support, slope, participant or admission proof.
[Source, ownership, ABI, limits and differential evidence](../../../../../../benilla-native-nav/PROVENANCE.md)
include native node-exhaustion/truncation, alignment and allocation-failure tests.

## Bound static source LOS

An explicitly LOS-enabled `StaticLosPolicy` binds the runtime profile digest and
its source-motion player/NPC collision heights. Session bodies must agree exactly;
missing motion, disabled/unknown policy or changed heights do not yield LOS facts.
The bounded source-motion profile has one NPC and frozen display/scale semantics.
The profile and independent non-render-participant proof remain production gated.

The installed VMap provider separately handles source spell Prepare/ReleaseCheck
LOS and NPC Sight LOS. It raises each endpoint by that unit's own source collision
height, preserves direction and binary32 addition, and queries the loaded static
VMap with `ignore_m2=true`. A source hit yields Failed and a source miss yields
Passed only in this declared static domain; missing primitives, expired queries,
changed bindings and absent products yield unavailable. The navigation-equipped
service uses the same VMap LOS method, not a pathfinding result. No flat support,
walking-body sweep or renderer LOS mask substitutes for this source predicate.

This follows pinned CMaNGOS `Object.cpp:1379–1391`, `Map.cpp:2668–2671`,
`Unit.cpp:11930–11957`, `Spell.cpp:4850` and `VMapManager2.cpp:187–202`.
`World.cpp:817` makes LOS configuration significant; loading a VMap does not
silently enable it. Explicit new-world policy is distinct from importing an old
server's effective configuration.

Source `isInAccessablePlaceFor` instead reads the target's current water state and
the actor's swim/walk/fly capabilities (`Unit.cpp:4460`). Those dependencies remain
unsupported. Visibility/detection is also distinct. The new LOS path establishes
neither of them, complete native spatial authority, live source-epoch ownership,
real-data admission nor playable gameplay. Source replacement must still revoke
the session; uninhabited native and participant constructors are unchanged.

## Product-backed ground height subquery

`InventoriedProducts::load_map_height` composes the existing real GridMap/static
VMap height reader with pinned `Map::GetHeight`, using an independently supplied
`NonRenderParticipants` token to establish an empty dynamic collision tree.
No-hit in that proven empty tree is source negative infinity. Missing participant
proof, missing terrain, incomplete VMap primitives or expired budgets are unavailable.
The conservative complete dry-VMap domain and previous reader guards remain intact;
creature habitat does not establish physical dryness.

`SourceHeightConfiguration` explicitly declares the recorded effective
`vmap.enableHeight` state and binds map, content, source epoch and effective-config
digest to the immutable loaded inventory. Only enabled height is implemented.
This is consistency checking, not authenticated config parsing or evidence that a
running source process actually used that configuration. Loading VMaps never
implicitly enables height. Source-authenticated extraction/configuration and the
production participant constructor remain separate closed gates.

The ground provider selects an immutable validated Profile/Session and freezes one
complete `MotionQuery::NearPoint` authority request. The existing typed capability
witness is checked against that profile's source catalog and exact outer NPC
identity/generation. The runtime already constructs it from the executing staged
owner; the provider never consults a stale committed-world NPC. Its narrow public
validator proves source consistency under the closed dry lifecycle and retained
generation-initialization invariant, not transcript authenticity or liveness.

Each initial/reset candidate subquery echoes the complete original authority and
NearPoint context, exact candidate XYZ bits, ticket, and snapshot map/content/source
epoch/inventory/policy. Initial Z must equal the original target Z. Candidate XY is
supplied by the scalar query and alone is not proven to equal source `sinf`/`cosf` output. The result carries
raw Map height (including invalid sentinels) and an explicit `AllowedHeightObservation`
for the Ground or Swimming branch. Habitat 3 preserves `CanSwim=true`: the latter
uses the source 50-yard static search plus dynamic height, then an all-liquid
query at groundZ with the default classification height and selected model-depth
threshold kept distinct. That threshold comes from the current-generation
initialization receipt, not the session's initial-model body; its snapshot must
match the same loaded inventory/configuration. Only complete NO_WATER products
are supported. Strict
comparisons preserve original Z bits on equality and invalid height. The
source final NearPoint fallback can reuse previously adjusted Z and needs a later
sequentially bound implementation; it is not silently admitted as an initial probe.

This callable subquery deliberately has no movement/mailbox route and emits no
complete NearPoint fact. Selector ordering, collision config, LOS and fallback
sequencing remain separate dependencies. A complete future
NearPoint provider must impose cumulative work and an absolute deadline across all
candidate queries; the current scalar product query bounds one height operation.
No native admission, full movement provider or playable-world claim follows.

[Source selection/empty-tree oracle](../../../../../../benilla-native-geometry-tests/tools/ground_height_README.md)
is separate from loaded authored-product integration tests. Scalar callbacks prove
arithmetic and call order; they are never evidence for terrain or collision geometry.


## Source-ordered dry creature initialization

`SourceCreatureInitializationProvider` accepts a fully bound
`Query::CreatureInitialization` over the same immutable complete-dry MapHeight
snapshot. Its policy is
`cmangos-8ec338a1-creature-initialization-complete-dry-products-v1`.
The runtime builds the request after model/gender selection and before level
selection, binding the selected collision height, separate `2.03128f` liquid
classification default, retained respawn position, world/profile/generation,
source identities, counter prefix, clock, ticket and effective configuration.
The provider validates that exact request and obtains NO_WATER from loaded product
bytes under one deadline. It does not infer dryness from habitat or consume a
player's cached environment. Missing geometry, stale bindings and any wet status
remain unavailable. This callable component has no production entry installer;
see the [runtime contract](../../../../../../benilla-world-runtime/CREATURE-DRY-HABITAT.md).

## Source-derived initial candidate

`BoundGroundHeightQuery::derive_initial_candidate` derives the first candidate's
XY from its frozen request, then invokes the loaded-byte ground query above.
`CurrentTargetToOwner` uses the existing pinned software facing kernel on target
XY toward owner XY. `Absolute` uses the explicit request angle. Neither substitutes
an orientation for a bearing or adds bounding radii to the request distance.
Transient angle and position values remain raw binary32, including subnormals and
signed zero; no persistent `Real` conversion silently narrows that domain.

The single glibc-derived trig algorithm now has separately selected native and
software arithmetic adapters. VMap keeps its original native-unfused arithmetic
and degree conversion. The candidate selects software RNE trigonometry and
binary32 multiplication, then separately rounded addition, then the exact source
map clamp. This is a pinned source numeric profile, not a claim that every C++
platform's libm produces identical bits. Existing geometry and height arithmetic
are not thereby converted to software floating point.

The revised spatial policy binds the candidate, trig and facing policy identities
and pinned source hashes, even for an absolute-angle query. The reply echoes the
resolved angle, policy digest and complete ground request/result. Caller-supplied
XY is never accepted by this derived-candidate method. Both reachable angle kinds,
raw tiny bearings and actual loaded authored products are exercised by tests.

This is only the initial candidate plus ground adjustment. It does not decide that
`DetectPosCollision` is disabled, accept the candidate under LOS/selector rules,
implement ordered fallback, or publish a `Fact::NearPoint`. `OwnerPathFinder`,
complete chase/replan routing and production startup remain separate work. Request
consistency is not live-state authentication: the eventual owner must evaluate and
accept the witness inside the current staged transaction and revoke replaced source
sessions. Authored test inputs establish no installed world data or admission.

## Bounded primary NearPoint acceptance

`SourceNearPointProvider` owns immutable loaded products and their already-decoded
VMap through the new Arc-owned height reader. Borrowed reader APIs remain available.
It accepts only explicit, mutually consistent effective declarations with
`DetectPosCollision`, `vmap.enableHeight`, and `vmap.enableLOS` all enabled. These
are source-config consistency contracts, not authenticated extraction or defaults.
The new composite spatial revision binds the retained config, height/VMap identity,
candidate arithmetic and pinned acceptance source hashes. Existing independent
non-render participant exclusion remains mandatory.

The provider executes the pinned source's first-return path:

1. Validate the full owned-capability NearPoint request and derive its initial XY
2. Apply actual loaded Map height and the selected ground or dry swimming-capable allowed-Z
3. Test strict `abs(initial_z - adjusted_z) < ((distance + owner_radius) + target_radius)`
4. Only if that passes, ray-test target eye to candidate feet, with `ignoreM2=false`
5. Return the exact source NearPoint reply only if LOS is clear and every XYZ bit
   round-trips through the existing persistent Point representation

The fresh source selector has empty used-area lists because this revision comments
out its only nearby-position visitor. Its original-angle check is therefore true;
no invented crowd exclusion is added. Invalid ground can leave Z unchanged and
still satisfy source acceptance, so a successful NearPoint fact is not a body-support
proof. Both the target-eye origin and unraised feet endpoint differ from the existing
spell/NPC-sight wrapper. Source M2 geometry is included in this caller's ray.

Height and LOS consume one cumulative work allowance and one absolute deadline.
An obstructed or vertically rejected first point requires unsupported fallback and
returns unavailable authority, never a negative movement fact or guessed destination.
Signed-zero/subnormal outputs unsupported by persistent Point also fail explicitly.
No mutable snapshot, per-query VMap reload, leaked lifetime or ownership bypass is used.

The callable owned provider is exercised through the actual native controller,
durable store, projection and encoded receipt replay. Those component tests use real
loaded authored bytes for NearPoint and explicitly authored authorities for other
queries. Existing production entry and participant constructors remain closed. This
explicit `install_with_primary_near_point` now retains the owned provider in the
main-thread geometry mailbox. Its private-field policy is constructed from a
validated profile and freezes the exact source-motion bodies, clock and identities.
The primary policy accepts the existing one-owner slice or an explicitly selected
complete two-owner visitor scene. It retains the whole validated profile and exact
body roster, rejects missing, extra or crossed bodies even for an unrequested peer,
and checks each searcher's source catalog and exact generation. Stable spawn IDs
or equal dimensions alone cannot replace the source capability witness. The
separate empty-selector fallback and StaticLos policies remain singleton-only.
Only matching NearPoint requests and exactly echoed NearPoint replies are admitted;
other motion queries remain unsupported. The primary mailbox path first completes
its immutable `Session::body` validation in a separate service attempt. A private,
Work-owned proof binds the exact session, request, body and original job lease.
Later attempts borrow that proof and pin the actual provider allocation; equal
serialized request fields cannot substitute another Work or provider. The ground
provider still performs every ordinary binding, participant, pose, branch, dimension
and initialized swimming-snapshot check, replacing only duplicate searcher
validation with consumption of that exact retained-profile proof. Ground and LOS
are recomputed for every final attempt. Exact reply checks run under the retained
proof, and a completed result past the slice is discarded and retried under the
same 500ms lease. No geometry result survives that retry. Raw callers, the
receiving LocalAuthority, and the separate empty-selector path retain full
validation. This is mailbox scheduling work, with no source-operation change. Queue cancellation, replacement, absolute
deadlines and per-frame budgets still apply. The installer receives already-loaded
products and proof; it is not called by character entry or exploration. This does
not complete fallback/selector search, supply OwnerPathFinder routes, or activate a
populated world. The original candidate-only API remains a separate partial witness.

[Exact first-path source oracle](../../../../../../benilla-native-geometry-tests/tools/near_point_primary_README.md)
observes source callback order and boundary arithmetic. Its scalar callbacks are not
terrain, LOS, world-content or participant evidence; loaded-product/controller tests
supply the separate bounded integration evidence.

## Separately selected empty-selector NearPoint fallback

`SourceEmptySelectorNearPointProvider` composes the same immutable loaded ground
and VMap readers with the pinned empty-list selector. It has a distinct geometry
revision and `EmptySelectorNearPointPolicy`; selecting both it and the existing
primary-only policy is invalid. `install_with_empty_selector_near_point` is an
explicit inert installer, not character-entry routing or production admission.
The primary-only provider and its unsupported-fallback result remain unchanged.

The supported branch has an owned habitat-1 or dry habitat-3 searcher capability
witness, collision detection enabled, VMap height/LOS enabled, and independent
non-render participant exclusion. Config values remain explicit source declarations,
not authenticated capture. No-searcher, wet movement, flying, water-walk, disabled
configuration, dynamic participants and used-area lists remain excluded. The
primary and empty-selector geometry policy domains are v2 and bind the distinct
ground or swimming-capable allowed-Z observations.

A private sequential capability derives the primary XY and every raw signed
fallback angle. Each ordinary candidate starts its height probe at target Z;
strict source vertical acceptance precedes target-eye-to-candidate-feet LOS with
M2 included. The first accepted point is returned. A selected point that cannot
round-trip through persistent Point fails immediately, rather than searching for
a different representable result. Missing coverage and malformed/binding failures
also abort; they are not failed candidates or natural source exhaustion.

After the actual empty iterator exhausts, the source restores first XY but keeps
the last successfully adjusted Z. The final height query uses that carried Z and
returns its result without another vertical or LOS test. This surprising source
return is preserved. It is not a clear-path, support or accessibility certificate,
and cannot replace OwnerPathFinder. Actual loaded stacked-floor tests distinguish
this final query from an incorrect reset to target Z.

All probes share one absolute deadline and cumulative geometry-work budget. The
628-offset maximum is a numeric bound, not a promise that every search fits the
service budget. Primary/free height and LOS tickets are unique pairs; the terminal
height query has its own ticket and no LOS ticket. A failed sequence stays failed.
The read-only test observer sees actual queries and cannot replace their answers;
test work-limit instrumentation only lowers the existing ceiling.

The [independent full-source oracle](../../../../../../benilla-native-geometry-tests/tools/near_point_empty_README.md)
records candidate, height and conditional LOS order using explicitly authored
scalar callbacks. Separate loaded-product tests
compare matching cases using the actual readers. Controller tests use actual
NearPoint mailbox facts but explicitly synthetic remaining authorities; headless
checks do not exercise the Bevy scheduler. No installed-world content, complete
pathfinding, runtime activation or startup admission is established here.

Direct exhaustive reader tests use a longer deadline to inspect every source
probe. Controller tests separately use the unchanged 2 ms mailbox allowance;
neither establishes that worst-case search always fits a production frame.
Deadline or work exhaustion rejects the entire fact without publication.

## Route owner and initial dry filter prerequisite

`SourceRouteFilterProvider` consumes the runtime-owned `CreatureRouteRequest`
with its exact source capability witness and current map position. Motion dispatch
constructs that position from the executing creature state after any forced
relocation; Home's independently computed spline `launch_origin` is not the
position queried by `PathFinder::updateFilter`.

This separately bound prerequisite owns an actual decoded `DryLiquidSnapshot`.
It queries that snapshot at the owned position in source order for `IsInWater`
(all liquids), then `IsUnderwater` (water/ocean). Loaded terrain coverage establishes
`CanCheckLiquidLevel` through the source's `GetGrid` branch. Missing coverage is
an unavailable dependency, never a false water predicate. The supported snapshot
proves exact `NO_WATER`; even an above-water nonzero status cannot be treated as dry.
The owned capability and observations produce the exact non-playerbot source
filter: habitat 1 uses `NAV_GROUND` (1), while habitat 3 retains `CanSwim=true`
and uses `NAV_GROUND | NAV_WATER | NAV_MAGMA_SLIME` (13), both with exclusions 0.
`cmangos-8ec338a1-ground-or-water-capable-dry-route-filter-v2` and
`benilla-source-route-owner-dry-filter-dependency-v2` bind this distinction.
All context, pose, inventory, epoch and geometry revision bindings remain attached
to the internal result. Each loaded constructor, cold/warm query and retained cache
uses the actual filter. Dry updates preserve 13, and a stale filter-1 cache rejects
for habitat 3. Mesh validation still admits only ground polygons; no wet route
branch, nonzero liquid status or `getNavTerrain` result is admitted.

This result supplies only the initial filter dependency. It does not implement
persistent chase filters/corridor reuse, normal source smoothing, allowed-Z path
normalization, route endpoint adaptation or a complete route fact. There is no
mailbox route admission or character-entry installer for this prerequisite.
Controller tests deliberately stop at unavailable route production and verify
that no staged gameplay, store or projection mutation escapes that failure.
Synthetic successful-route replay tests are labelled separately.

The [full-method source oracle](../../../../../../benilla-native-geometry-tests/tools/route_filter_README.md)
checks initial filter flags, exact liquid masks and short-circuit ordering. Its
scalar callbacks do not establish loaded coverage; provider tests obtain those
observations from the independent product reader. This initial result also does
not replace `calculate`'s later filter update or persistent wet-to-dry history.

## Loaded ground source-smoothing prerequisite

`SourceRouteSmoothingProvider` composes the bound route-owner filter with a
navigation snapshot from the same exact inventory, map, content and source epoch.
Its explicit configuration selects a source-stage point limit and separate work
budgets. These declarations do not infer map/dungeon settings, source defaults or
an authentic world configuration. The original absolute deadline spans both liquid
observations, bounded corridor preparation and all smoothing primitives.

The new native stage keeps original endpoints instead of passing the existing
funnel reader's nearest-surface endpoints into source smoothing. It supplies the
actual decoded owner filter and loaded corridor to the pinned `findSmoothPath`
semantics. Raw completion, a source point-limit failure and a successful early stop
remain distinct; none is a complete route or segment certificate. Output precedes
source optimization and conditional Z normalization. `SourceNavigationProvider`
and its older direct-segment veto remain unchanged.

Fresh corridor preparation uses the pinned direct `findPath` search semantics with
its 1024-node pool, resumed one original expansion at a time under the shared work
budget and deadline. It preserves direct-search cross-side node keys, parent
handling, queue ordering and exact-capacity status behavior. The older conservative
reader still uses its separate 512-node sliced query. Source-stage nearest-poly
preparation remains an explicitly bounded dependency, not the owner's cache lookup.
The original C++ source's cold-cache uninitialized-distance read is not executed,
repaired silently or assigned guessed deterministic behavior by this stage.
Persistent Chase cache/reuse, Home lifecycle, configuration branches and complete
movement evidence still require separate integration.

Controller tests use a clearly test-only channel to construct and retain the
non-Send native mesh on its query-owner thread. They reach the actual runtime Route
request, obtain loaded liquid and smoothing dependencies, then deliberately leave
route production unavailable. They verify exact retry and no runtime/store/
projection publication. The production mailbox still rejects Route; these tests do
not activate that mailbox or populated-world entry.

## Runtime-owned width and ignore-pathfinding warm calculation

`SourceRouteStraightPathProvider` is a loaded calculation dependency for an existing
runtime-owned warm Chase request. Its session, profile, map/content, source epoch,
configuration, prior cache revision and full authority request are bound together.
The collision width comes from `CreatureRouteRequest::require_collision_width`:
the selected native model's loaded DBC width and initializer scale, retained by the
runtime for that owner generation. Bounding radius and rendering scale are not
substitutes. Missing width, including after respawn without fresh model selection,
remains unavailable.

Before either calculation, the exact source coordinate predicate validates the
original destination then start (finite XY within the source map bound, finite Z).
Each attempt next validates `CreatureRouteRequest::require_owner_ignore_pathfinding`
and retains its typed `OwnerIgnorePathfindingReceipt`. The runtime privately mints
this receipt from the current NPC capability generation, the explicit zero
`InteractionCatalog` seed and the bounded lifecycle closure. It proves only that
`UNIT_STAT_IGNORE_PATHFINDING` (`0x10000000`) is clear. It does not assert the full
UnitState mask, source `ExtraFlags`, `m_ignoreMMAP` or global/map configuration.
Missing evidence is unavailable; edited owner generation, profile, source or receipt
bindings fail closed before any tile or filter observation. Coordinate rejection
still precedes receipt consumption. The source's intervening `SetCurrentNavMesh`
configuration and nav/query checks remain open, so this is a named dependency,
not an authenticated complete calculate prefix.

The loaded `HaveTile` dependency then queries source `calcTileLoc`/`getTileAt`
layer zero at the original XYZ pair on the same native mesh: start first, and
only when present, destination. Exactly coordinate -1 is the source sentinel;
other negative coordinates still receive their actual loaded lookup. A missing
endpoint is an unavailable source shortcut branch, never an inferred NOPATH,
NORMAL, complete Route, or a nearby-polygon substitute. Source-valid coordinates
outside this native bridge's narrower numeric domain also remain unavailable.

The first calculation updates the retained source-owned dry filter from fresh loaded
liquid observations only after both tiles are present, and passes the immutable loaded cache lineage, original
endpoints and exact width bits into the native straight calculation. A returned
NOPATH or INCOMPLETE result can continue privately through a fresh loaded tile
pair, second filter update, fresh warm lookup and terminal smooth calculation.
Both successful dependencies retain the separately validated runtime-owned bit
receipt and opaque, native-instance-bound tile evidence. Retry repeats coordinate
validation and owner-bit validation before its fresh tile pair. These validations
consume no geometry ticket. All nine possible tickets are reserved before the first
observation; the initial pair, two filter observations and straight query precede the retry pair, two new
filter observations, warm lookup and point query. The continuation uses the
straight result's complete terminal backing storage, inactive tail and path type;
it restores the original requested goal rather than using the clipped straight
endpoint. A bridge error or unsupported source domain is not a source path result
and does not authorize a successful fallback. Neither attempt commits a runtime
cache revision or publishes a Route fact.

This calculation dependency does not itself choose whether Chase may attempt
straight movement. The separately selected loaded gate below supplies that choice.
Active allowed-Z normalization, transports, Home's fresh owner, cold-cache
initialization and complete movement evidence remain separate. The production
mailbox and native entry admission remain unchanged.

## Explicit dry Chase straight gate

`SourceRouteChaseGateProvider` composes the pinned
`TargetedMovementGenerator.cpp:502` selection with the existing warm straight
calculation. `ChaseGateConfiguration` requires explicit enabled source LOS and
normalization-disabled straight configuration; there is no default-enabled setting.
This policy and the actual loaded VMap policy are hashed into a distinct revision.
The selection and inventory hashes establish consistency, not certification of a
producer's recorded effective configuration or a live source-epoch lease.

Only `ChaseInitial` and `ChaseReplan` are accepted. The complete session, selected
player environment, world/player identity, immutable profile, source owner generation,
clock, current pose, route endpoints, full prior cache and configuration are frozen.
The runtime's current-generation collision-width witness validates the retained
model-selection receipt, including NPC combat reach. Owned NPC/player height
witnesses supply the two eye heights; no free reach or height scalar is accepted.
The model receipt and witnesses remain in the returned dependency evidence.

The exact binary32 distance test is strict squared distance below
`(200 + owner combat reach)^2`, followed by strict absolute Z difference below 5.
Only after those pass does a real loaded static VMap ray run from the current owner
plus its own height to the requested route goal plus the target's own height. The
ray includes M2 (`ignore_m2=false`), unlike the existing spell/sight route. Independent
non-render participant exclusion and the existing complete static-domain limits
remain required; renderer rays and walking corridors cannot supply this answer.

A clear ray is followed by the actual `DryLiquidSnapshot` query at the owner,
recording source mask 15 and default liquid collision-height bits. Its actual
full-inventory NO_WATER proof is not a caller boolean. General source NPC water
means any nonzero terrain status, including ABOVE_WATER and WATER_WALK. Only then
is `require_target_water` consumed: the target's current owned player-environment
cache, which is never resampled at the target's position or at the route goal.
Every rejection stops the remaining observations. The dependency returns typed
`NormalRouteUnavailable(reason)` without inventing NOPATH or a normal route.

Only a passed gate invokes warm straight calculation. LOS and owner-water use the
first two tickets, followed by the nine-ticket calculate/retry range;
overflow is rejected before observations. The gate's water result is not reused by
`updateFilter`: fresh mask-15 then mask-10 owner observations still precede each
calculate, including retry. Original filter bits, path cache storage, source goal
and NOPATH-or-INCOMPLETE retry rules remain unchanged; SHORT alone never retries.
The original opt-in product inventory is preserved by the separately named complete
`DryRouteProjection`. The retained lineage must be minted on the very same loaded
navigation handle later consumed by the provider; reloading identical inventory
bytes does not transfer the native mesh capability. Wet navigation, cold-cache repair, transports, other route
kinds, normalization, Route publication and native admission remain unavailable.
The full calculate prefix remains incomplete: global pathfinding enable,
map-disabled configuration, nav/query ownership and dungeon context are
not authenticated by these dependencies. The bounded owned IGNORE_PATHFINDING bit
is independently authenticated on each attempted calculation. Loaded `HaveTile` observations establish
only their named predicate on the retained mesh. A nearby polygon can exist when
an endpoint tile is absent. There is no production admission or cold-distance repair.

## Loaded player-environment observations

`SourcePlayerEnvironmentProvider` retains an actual immutable `TerrainLiquidSnapshot`
and compares its full snapshot binding and decoded LiquidType catalog with the
explicitly selected runtime owner. `load_terrain_wet_liquid_owned` retains an
`Arc<VmapLiquidProducts>` for both liquid and area readers; queries need no live
staging files or leaked allocation, and dropping the reader releases its ownership. Public binding hashes are equality checks, never
source authenticity. `PlayerEnvironmentPolicy` requires the actual source-initialized
`PlayerCollisionHeightWitness`, exact world/player/profile/map/content/geometry
bindings, and the session's matching player body. No body height is inferred from
combat reach, bounding radius, or a rendering model.

The separately selected mailbox route preserves the complete typed phase query and
original absolute deadline through the loaded query and reply validation. Missing
products, unloaded neighboring tiles, source replacement and deadline expiry are
unavailable, never manufactured `NoWater`. Installing this route does not select
an environment source for another profile or open native session admission.

The reader can observe actual wet states. Runtime environment reduction separately
rejects unsupported threat, spell, hazard and mirror-callback transitions before a
frame commits. An observation is not permission to execute those callbacks or a
completed Chase water predicate for an unowned target.

## Actual constructor-dispatch query ownership

`products::source_query` loads an independent `SourceQueryProjection` over the
same actual-byte dry-liquid/VMap inventory. It uses the retained selected
configuration's real map/source-instance slot and the distinct native
`SourceMapQuery` 1024-node constructor. The existing `NavigationSnapshot`,
`DryRouteProjection`, bounded512 and warm-seed policies remain unchanged. The
new snapshot policy binds source configuration, source slot, inventory, dry proof,
source/Detour pins and actual native capacity. A separate loaded object with
identical snapshot bytes is still a different native allocation.

`SourcePathfinderQueryProvider` freezes the final profile/world/query policy,
after the query has been loaded. At an actual runtime Route dispatch it requires
the private live `PathfinderDispatchReceipt`, complete selected configuration,
matching outer world/profile/policy/tick/NpcId and exact inner dispatch binding.
The receipt names the first actual Chase dispatch after forced relocation and
before the straight gate, independently of any retained polygon cache. Nearby
initial Chase does not imply construction. Home uses a distinct first-use
stack-local constructor and a bounded dry same-polygon continuation; see
[the Home contract](../../../../../../benilla-world-runtime/HOME.md). A live receipt may outlast its callback;
it is not a timestamp attestation of native loading. The eventual composer must
bind this provider before driving the runtime and consume the event inside that
actual callback. Captured requests exercise rejection/identity controls in tests
and do not independently establish that ordering.

The provider keeps at most 4096 constructor-query entries. Keys include exact
first-dispatch binding and outer NPC generation. A first event reserves storage
and retains the already loaded query before claiming its shared one-use live
seal. There are no fallible operations after claim before registry insertion.
Cloned first events cannot create a second lease in an equal-byte replacement
provider. The same live event reuses its existing entry. A regenerated first
event after outer transaction rollback must claim its fresh seal, then reuses
only the same constructor identity, frozen configuration and actual query.
Retained events without their existing entry fail closed. Serialized receipts
have no live seal and hashes/after-cache/warm seeds cannot restore an entry.
No eviction, query replacement, unload or registry deserialization is supported.
The query and registry intentionally use owner-thread `Rc<RefCell<_>>` ownership.
They are not `Send`/`Sync` and require local/non-send service placement, rather than
an ordinary Bevy `Resource`. This boundary does not change concurrency policy or
install the registry into the engine service.

The returned `SourceConstructorQuery` is explicitly **query ownership only**.
It does not execute `createFilter`/constructor `updateFilter`, which precede the
Chase gate in the source, and it cannot supply a Route fact. Current query starts
unselected. A separately requested calculate-prefix check preserves original
runtime endpoints, validates destination then start, selects the retained default
and observes actual loaded layer-zero endpoint tiles. Invalid source coordinates
never transition selection; an already expired operational budget returns before
selection. Equal-byte query instances cannot exchange the loaded observations.

Rollback reuse is safe only for this immutable loaded-query dependency and its
idempotent current-query selection. Native HaveTile is read-only. Future
BuildPolyPath/cache/filter composition must stage and validate mutable source
state and commit it with the enclosing runtime frame; it cannot infer rollback
safety, fresh constructor-filter evidence or a completed calculation from this
registry. Enabled-at-construction, fixed outdoor map/source slot zero and
nontransport configuration are required. Disabled-then-enabled same-map default
query reads remain excluded, as do all query replacement/map-change paths.

## Source-ordered same-polygon proposal

`SourceSamePolygonRouteProvider` composes the actual source1024 registry, loaded
owner filter and shared Chase gate observations without a warm seed. Its typed
binding includes the raw Corridor revision, the actual shared liquid/navigation/
VMap inventory and epoch, selected MMap configuration, explicit false/false
NormalizePath configuration, operational budgets, and the approved
`cmangos-buildpolypath-zero-distance-v1` source policy.

The provider must be loaded and bound before runtime execution and invoked inside
the authority callback. First construction performs distinct `createFilter` water
and underwater observations before the gate. The gate uses the actual loaded VMap
ray, fresh owner water, and runtime-owned target water. Destination/start validation
and selected-query ownership then precede the owned ignore bit and actual HaveTile;
only afterward does a fresh calculate `updateFilter` run. Cold lookup starts from
74 zero slots and active length zero under the named two-local compatibility
amendment. Same-polygon completion preserves the searchable tail, restores active
length one and NORMAL, and keeps exact `[origin, goal]` points. Retained calculation
uses the exact private immutable prior on that same allocation with original
warm lookup semantics. No raycast, findPath, smoothing, normal fallback or CutPath
work is implied by this branch.

Candidates are proposals rather than accepted cache mutations. Each of at most
256 generator/generation entries retains at most four immutable candidates. Only
an exact authenticated runtime next-prior can select one; serialized references
cannot recreate it. Exact request retries repeat source observations and cold
constructor work but reuse the identical immutable candidate. No ancestor is
pruned on resolve or on observing a later prior, because another operation in the
same outer frame can still fail. Saturation is unavailable, never eviction.
A committed head counts toward the same total cap of four; after acknowledgement
it leaves room for at most three uncommitted candidates. Direct cached-checkpoint
rehydration remains unsupported. The separate
[bounded reconstruction recipe](../../../../../../benilla-world-store/CACHE-RECONSTRUCTION.md)
reruns the complete retained accepted history from a checked cache-free anchor
through fresh actual providers;
it cannot import these candidates or references.

The app geometry identity is
`benilla-owned-dry-same-polygon-chase-home-wander-commit-continuation-v6`, including the
outer-commit acknowledgement, total-cap-four and bounded original-job policies.
Earlier uncommitted-proposal and monolithic-commit identities are incompatible.
Commit-head delivery itself does not alter the native payload schema; the current
payload is format 50 for complete authored one/two-owner source visitors.
`install_with_same_polygon` returns a proposal authority and an explicit paired
commit sink. The selected mailbox refuses queries until the controller attaches
that sink at the actual cache-free world boundary. A fresh opaque attachment and
the existing mailbox cancellation fence prevent equal-byte installations from
transferring ownership.

Immediately after successful `step_batch` and input retirement, the controller
captures an app-local nonserializable notice from that exact record and final
current Chase caches. It does so before projection/checkpoint can clear the
journal. The notice binds each consecutive tick, previous record, before/after
state, ordered Route replies and current owner/construction/cache, including
explicit absence. Successful calculation, concrete certification and reply
transmission remain speculative. Only this notice can acknowledge their use.

On its NonSend owner thread the provider validates the complete exact certified
chain and final current cache before atomically retaining the actual final native
candidate and pruning obsolete speculation. Historical spline prior-cache DTOs
remain unchanged and cannot select the current head. Exact duplicate notices are
idempotent; stale, skipped, wrong-attachment or mismatched notices cannot prune.
The committed watermark rejects delayed calculations before candidate staging.
Constructor-registry churn and its existing lifetime bound are unchanged.

Control notices share the explicit service work allowance but do not need current
terrain coverage or `WorldLive`. The controller retains at most one pending
notice and blocks another tick behind it. A failed delivery has its own
`CommittedAwaitingRouteAck` pause and retries only that exact notice, never the
committed inputs. Projection and checkpoint failures preserve the captured notice
and committed world, while required projection/outbox work retains priority.
Shutdown may save and release the session; it does not rehydrate loaded queries.

The engine-free harness executes this composition from actual runtime callbacks
and observes unchanged world checkpoints when the callback refuses the final
Route. Its remaining non-Route world prerequisites are explicitly authored.
The prefix-only `resolve` remains unavailable with
`StrictProductionCorridorUnavailable`. Source NORMAL and parsed planar support
alone cannot supply coverage, swept body, elevated LOS, continuous support and
absence of liquid.

The app-only `source_route_same_polygon_local` resolver accepts the concrete
production `LocalCorridor`, never an arbitrary `Corridor` implementation. It checks
the actual raw revision and exact terminal endpoints, obtains every strict raw
coverage/body/elevated-LOS/continuous-dry-support check, then runs a separate loaded
source VMap ray with the owner's height at both ends (including M2). Only after all
these checks and the original deadline does it construct an OwnerPathFinder
CompleteRoute with the exact prior/after cache and all five segment predicates.

`install_with_same_polygon` installs an explicit bounded Session Route policy and
already-loaded source readers before runtime driving. The Service routes actual
jobs through this concrete resolver. Generic movement, other source branches and
native production admission do not gain authority from this selection. Existing
ticket/outer-inner echo, cancellation, deadlines, frame budget and participant
checks remain in force. Complete replies additionally validate exact two-point
endpoints, all segment predicates, owner generation, cache inventory/epoch/policy,
first/retained constructor phase and revision transition.

The same actual product owner supplies source NearPoint MapHeight and VMap, dry
liquid, source1024 and runtime player-environment dependencies. A private typed
composite binds their actual revisions/configuration under one geometry identity;
it cannot relabel a provider with an arbitrary matching hash. The headless harness
executes genuine NearPoint and source-order callbacks but cannot certify a Route.
App-only tests exercise the real physics resolver and installed Service/mailbox
first calculation, failed-frame retry with a fresh live dispatch, retained
replans beyond the total candidate cap, exact native candidate promotion, stale
acknowledgement/callback refusal, real corridor failure, and cap saturation without
eviction followed by an exact retry, and actual spell-death/cache-free respawn
pruning. Those app tests require full Bevy/system-library CI and are unverified in
the local environment without `libudev.pc`.

The authored runtime/transport tests still use explicitly synthetic unrelated
combat and other non-geometry callbacks; they do not admit a production session.
Native checkpoint serialization is distinct from loaded-cache rehydration;
direct cached restore remains closed. The separate cache-free-anchor reconstruction
component retains every accepted tick since its anchor. A durably saved, globally
cache-free ordinary lifecycle boundary can become a new anchor with an exact
previous-generation bridge. Active-owner history still has a 2048-tick/8-MiB cap;
this does not compact a live cache or guarantee a suitable retirement point. Controller tests exercise actual projection/checkpoint failures,
committed input retirement, record chaining after journal clearing, and the distinct
acknowledgement retry barrier using an explicit authored sink; they do not substitute
for the real physics/Service tests. This bounded lifecycle sustains same-polygon
replanning across acknowledged ticks, not arbitrary per-tick chains, generator
churn, multi-polygon navigation, saved-query rehydration or production admission.

The two-owner authored composition initializes distinct owner radii, source model
widths/heights, speeds and orientations before loading the shared query providers.
An initially hostile A and raw-neutral Aggressive B retain the native34 visitor and
combat semantics: a deliberate supported hit makes B retaliate. Both source-owned
NearPoint callbacks use the actual loaded MapHeight/VMap readers, and app-only
fixtures require the installed Service and concrete LocalCorridor for both Routes.
Other visibility, membership, spell/combat and player-input callbacks remain
explicitly authored. In particular, nearby NPCs do not imply a new crowd-avoidance
branch; the pinned used-position visitor is disabled.

Candidates, constructor handles, current heads and original pending-job leases
stay owner/generation/construction-specific. Acknowledgement checks the complete
two-owner boundary before promoting or pruning either native candidate. Private
prepared-state assertions also keep each retained constructor and filter context
paired with its original request/live dispatch; these are internal invariants,
not publicly mutable capability fields. Cancellation and replacement never renew
the peer's absolute expiry. Death and cache-free respawn retire only the departed
owner's cache; cleared model witnesses still do not authorize fresh respawn routing.
Two-owner app fixtures cover acknowledged replans past the per-generator cap,
late-B failure/retry, crossed heads and generation-bound facts. Full app CI is
required to establish those fixtures' concrete physics/Service results.

The sustained authored trajectory keeps the selected combat host's required
backpedal policy and fanning behavior. It waits for actual spline completion,
then moves stopped-target pose samples along two rectangle axes chosen from the
current owner/target offset. The fixed component stays above the source backpedal
threshold; the far corner exceeds combined reach. Every resulting Route origin
and goal is checked against the fixture's first convex polygon. Non-Route combat,
bearing and Collider(None) observations remain explicitly authored fixtures.

A successful multi-Route same-generator batch is not claimed by this selected
Complete-only test path. `NativeWorld::step_batch` applies all commands before
source event/motion updates. `start_chase` reuses an existing Chase without a
calculation; a new Complete ChaseInitial launches its half-reach endpoint, so the
same-frame recheck cannot exceed combined reach. The one later `update_chase`
dispatch returns after launch. No live runtime caller sets `speed_changed` in this
bounded host. The acknowledgement validator still checks an ordered full chain,
but no manufactured candidate/notice chain substitutes for a reachable source
execution. The cache-free respawn test stops before a new constructor: it proves
obsolete-owner pruning, not constructor-registry churn support.

The distinct-giver quest variant reuses the exact source-selected native34 quest,
interaction, combat, facing, spell-133 and ordinary loot catalogs. Its app-only
fixture installs the real Service and LocalCorridor, binds the controller's actual
selected player into the environment and route policies, and pumps stock commands
through NativeController, NativeProjection and NativeStore. Natural hostile sight
must obtain A's first loaded native cache before accepting B's quest; no manually
installed cache, extra actor or alternative semantic authority supplies that step.
B stays a complete, independently clocked, combat-capable neutral Aggressive owner.

Only player Environment, source NearPoint and same-polygon Route observations are
loaded in this variant. The terrain, VMap, MMap and model rows themselves are
original authored fixtures. Complete cell membership, directional visibility,
combat accessibility/LOS, spell LOS/impact visibility, Collider(None), and player
pose acceptance remain expressly synthetic observations. The in-place player turn
is a stock movement command, not a loaded player movement proof. Semantic queries
that escape the selected source hosts are rejected by the spatial adapter.

The fixture checks actual candidate pointers and cache identities across controller
commit acknowledgements, late-B rollback at cache acquisition, acceptance, lethal credit, collection and
reward, and checkpoint faults at cache acquisition, lethal spell progression, item collection
and reward. Native heads remain unpromoted until the successful tick's required
save/publication barrier; checkpoint retry does not rerun commands or authority.
Direct saved-cache reopening and reopening a cache-free base whose journal adds an
actual loaded cache must both reject. A's death may clear its cache; this variant
never treats that endpoint as proof of loaded-cache rehydration. The headless
harness compiles the shared controller assertions and tests loaded input/proposal
composition only. The actual installed Service/physics tests require full app CI;
no production admission, installed private content, graphical playability or wider
navigation domain follows from these authored component tests.

The independently selected [source-location provider](SOURCE-LOCATION.md) reads exact
zone/area from owned terrain/VMap/DBC products. It does not infer location from liquid
status, prove spell-area closure or open gameplay admission.

The first-entry provider uses the same owned terrain/VMap/DBC products through a
separate exact-start-pose request, before the ticked location-query domain. It binds
the final profile, world, player, session and source snapshot, and returns a raw
flag with catalog-derived zone/area for the native genesis receipt. Its authored
component test retains a nonempty raw spell_area table; it does not select a
production session or supply missing client assets. Stored receipt validation is
content consistency, not authenticated historical terrain evidence.
