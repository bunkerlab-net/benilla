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
request deadline. The main-thread NonSend service never blocks; at most eight jobs
run per rendered frame, with a 2ms soft limit checked between jobs. Each player job
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
validator proves only source consistency under the current closed ground-only
lifecycle invariant, not transcript authenticity or liveness.

Each initial/reset candidate subquery echoes the complete original authority and
NearPoint context, exact candidate XYZ bits, ticket, and snapshot map/content/source
epoch/inventory/policy. Initial Z must equal the original target Z. Candidate XY is
supplied by the scalar query and alone is not proven to equal source `sinf`/`cosf` output. The result carries
raw Map height (including invalid sentinels) and source ground-only allowed Z.
Strict comparisons preserve original Z bits on equality and invalid height. The
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
2. Apply actual loaded Map height and the selected source ground-only allowed-Z
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
Only matching NearPoint requests and exactly echoed NearPoint replies are admitted;
other motion queries remain unsupported. Queue cancellation, replacement, absolute
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

The supported branch has a real ground-searcher capability witness, collision
detection enabled, VMap height/LOS enabled, and independent non-render participant
exclusion. Config values remain explicit source declarations, not authenticated
capture. No-searcher, swimming/flying, disabled-config, dynamic-participant and
used-area-list branches are not inferred or silently substituted.

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

## Route owner and initial ground filter prerequisite

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
The ground capability and observations produce the initial non-playerbot source
filter (`NAV_GROUND`, no exclusions). All context, pose, inventory, epoch and
geometry revision bindings remain attached to the internal result.

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

## Runtime-width-bound warm straight calculation

`SourceRouteStraightPathProvider` is a loaded calculation dependency for an existing
runtime-owned warm Chase request. Its session, profile, map/content, source epoch,
configuration, prior cache revision and full authority request are bound together.
The collision width comes from `CreatureRouteRequest::require_collision_width`:
the selected native model's loaded DBC width and initializer scale, retained by the
runtime for that owner generation. Bounding radius and rendering scale are not
substitutes. Missing width, including after respawn without fresh model selection,
remains unavailable.

The first calculation updates the retained dry-ground filter from fresh loaded
liquid observations and passes the immutable loaded cache lineage, original
endpoints and exact width bits into the native straight calculation. A returned
NOPATH or INCOMPLETE result can continue privately through a second filter update,
fresh warm lookup and terminal smooth calculation. The continuation uses the
straight result's complete terminal backing storage, inactive tail and path type;
it restores the original requested goal rather than using the clipped straight
endpoint. A bridge error or unsupported source domain is not a source path result
and does not authorize a successful fallback. Neither attempt commits a runtime
cache revision or publishes a Route fact.

This dependency does not choose whether Chase may attempt straight movement. That
source decision still needs its exact reach-expanded distance and vertical tests,
owner-eye line of sight including M2 geometry, owner water and target water, in
source order. Active allowed-Z normalization, transports, Home's fresh owner,
cold-cache initialization and complete movement evidence remain separate. The
production mailbox and native entry admission remain unchanged.
