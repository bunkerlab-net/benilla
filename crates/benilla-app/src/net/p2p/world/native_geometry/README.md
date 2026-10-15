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
Only the two source LOS queries described below have an additional static provider.
NPC visibility/accessibility, spell range/impact visibility, melee and near-point
fact queries remain unavailable. Timeout or failure
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
supplied and is not yet proven to equal source `sinf`/`cosf` output. The result carries
raw Map height (including invalid sentinels) and source ground-only allowed Z.
Strict comparisons preserve original Z bits on equality and invalid height. The
source final NearPoint fallback can reuse previously adjusted Z and needs a later
sequentially bound implementation; it is not silently admitted as an initial probe.

This callable subquery deliberately has no movement/mailbox route and emits no
complete NearPoint fact. Candidate arithmetic, selector ordering, collision config,
LOS and fallback sequencing remain separate dependencies. A complete future
NearPoint provider must impose cumulative work and an absolute deadline across all
candidate queries; the current scalar product query bounds one height operation.
No native admission, full movement provider or playable-world claim follows.

[Source selection/empty-tree oracle](../../../../../../benilla-native-geometry-tests/tools/ground_height_README.md)
is separate from loaded authored-product integration tests. Scalar callbacks prove
arithmetic and call order; they are never evidence for terrain or collision geometry.
