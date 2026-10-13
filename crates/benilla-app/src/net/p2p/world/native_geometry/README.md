# Local native geometry boundary

This is a bounded, opt-in **client-local** geometry provider, not CMaNGOS VMap/MMap
parity and not deterministic replicated world geometry. Production native entry is
still closed. The registered service is inert without an explicitly installed local
session; the character browser and exploration path do not install one.

## Non-render collision participants

A session additionally requires a private-field `TerrainOnlyParticipants` proof bound
to its exact content digest and map. The compiler must audit a complete source manifest
of game-object/door/transport/dynamic collision spawns and every scripted/runtime
creation path; the controller must forbid introducing any such participants while
that session is active. Empty ADT references or empty rendered entities cannot prove
this absence. The current content schema does not expose that complete audit, so
**the token has no production constructor**. Only authored component tests construct
an explicit fixture token. Missing or mismatched content/map proof rejects session
admission and every request. This is a concrete production blocker, separate from
the functioning positive static-terrain solver.

## Positive domain

The real app adapter can certify a straight movement interval only when:

- CurrentMap, Map.dbc directory, streamer and loaded WDT agree; raw WDT framing,
  required records and terrain-only flags are proven before filtered global-WMO state
- The target ADT and all eight guard neighbors are loaded and furnished, with live
  static terrain colliders, no pending collider, no model or WMO references
- Raw source ADT records certify exactly 256 bounded 145-height chunks and no liquid,
  placement or dangling placement-reference records, before renderer filters can omit
  malformed/unresolved entries; unsupported MH2O liquid storage is rejected
- All target-tile chunks have canonical, finite 145-vertex center-fan topology and
  distinct canonical footprints; the supporting chunk is flat, dry, passable and
  hole-free, and the whole radius-expanded XY rectangle lies strictly inside it
- Both feet heights equal the supporting vertices exactly; slopes, steps, holes,
  drops and chunk seams have no tolerance-based fallback
- The supporting collider's current transformed triangle data matches the checked
  asset, and no other collider's AABB intersects the swept body volume
- A body-box sweep and elevated two-sided LOS ray find no obstruction after the
  complete positive source/ground proof

The topology proof covers every point in the swept rectangle, not just endpoint or
interval samples. The source SHA-256s of the exact WDT and nine ADT byte streams, their map
and tile coordinates and the local policy tag determine the geometry revision.
Every processing attempt reloads those handles, proves current geometry and compares
the computed revision to the request. An unload, map change, source replacement or
collider replacement cannot reuse a caller-supplied revision as proof.

Typical real starter terrain has trees/buildings, water, slopes and chunk crossings.
It therefore normally remains unsupported by this first domain. A positive synthetic
ADT fixture demonstrates the mechanism; it does not demonstrate native gameplay.
No game assets are embedded. The next extension needs successful M2/WMO collision
provenance, source-correct masks, multi-chunk continuous surfaces and liquid coverage.
The mailbox/MovementRequest interface is independent of those geometry providers.

## Ownership, limits and ordering

`LocalAuthority` belongs on the native driver thread. Its synchronous `resolve`
uses a bounded 16-job queue, a one-result channel and a 500ms deadline; it never runs
on the renderer thread. The main-thread `NonSend` service uses only nonblocking
receives/sends. Expired work is discarded and counted. At most eight jobs are
processed per rendered `FrameCount`, even across multiple fixed ticks, with a 2ms
soft service budget checked between jobs. One job is bounded to nine tile metadata
checks, one complete 256-chunk proof/collider comparison and fixed collision queries.
It can exceed the soft budget; no exact frame-time performance is claimed.

The system runs in `FixedPostUpdate` after `PhysicsSystems::Last` and `TransformSystems::Propagate`, following the
actual default Avian schedule: collider tree updates, simulation and writeback
finish before the query. `Update` terrain streaming runs later; newly attached or
removed terrain is examined in the next fixed-step snapshot. An ordering regression
is included for the app CI; full Bevy compile/runtime checks require that CI.

The full original AuthorityRequest and a monotonic mailbox ticket are echoed and
checked, including world, tick, profile, policy, NPC generation, movement IDs, map,
content and geometry revisions. Both service and driver run the existing simulation
movement validator on a result. Only already-source-resolved `Query::Movement` is
supported; NPC/spell/melee/player authority facts return explicit unavailable errors.
The solver uses local FPU geometry and rejects anything the software-binary32
simulation budget validator cannot certify. Known obstacles produce bound Blocked
outcomes; missing/unsupported geometry is never an unreachable-path success.

A timeout/channel failure commits nothing because NativeWorld stages the complete
frame. The future controller must retain the exact tick/input for an explicit retry
or pause, and drop/replace the service on logout. It must not acknowledge or checkpoint
an uncommitted tick. No queue result is accepted from a peer or serialized as an
unverified geometry assertion. Session profile/content/body fields require a trusted
compiler; their digests alone are not asset authenticity or source-mechanics proof.

## Pinned source and remaining inputs

Reviewed CMaNGOS Classic commit
[8ec338a1704e7dcb1c0213eb7ed58f9231ade40f](https://github.com/cmangos/mangos-classic/tree/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f).
The local copies were checked against the existing simulation provenance SHA-256s:
Object.cpp `41090452…`, Unit.cpp `03b67a4b…`, TargetedMovementGenerator.cpp
`eaa569e5…`, HomeMovementGenerator.cpp `d225dd76…`, PathFinder.cpp `0fc26842…`.

- `WorldObject::IsWithinLOSInMap` / `IsWithinLOS` raise source and destination by
  each object's collision height, then call map LOS. Benilla body/camera face masks
  are client masks, not proven server VMap masks. In this placement-free flat subset
  only the positive-height terrain segment applies; broader LOS remains unsupported
- `Unit::GetCollisionHeight` uses object scale × CreatureModelData.CollisionHeight,
  with the source zero-height fallback. Combat reach or bounding radius is not that
  height. The current native profile lacks this source-resolved field
- `GetNearPointAt` uses source target distance/angle, coordinate normalization,
  allowed-height adjustment, position-collision configuration and alternate-angle
  LOS selection. Chase dynamic target distance uses reaches, offsets and source
  factors. This adapter never replaces it with a player-center goal
- Chase dispatch's optional straight path requires distance <200, height difference
  <5, source LOS and dry endpoints before source path normalization. Missing mmaps
  can produce a source shortcut; that is not permission for unchecked client geometry
- Source wounded slowdown below 30% health (unless excluded) and spline state are
  not in MovementRequest. The runtime/compiler must provide or reject those states
  before this adapter can establish native speed semantics

Missing compiler/runtime inputs are therefore source collision height/width/scale,
source near-point target distance/angle/configuration and target collision height,
current wounded-speed eligibility/multiplier, and an audited geometry snapshot
revision. No NativeCapability or production-enable bypass is created here.

## Verification

`cargo test -p benilla-native-geometry-tests` includes the actual app mailbox/solver
source and actual pure terrain proof source without Bevy. Tests cover positive planar
movement, exact arrival, software budget validation, unknown/obstructed geometry,
request/result binding, backpressure, expiry/disconnect and per-render-frame budgets.
`cargo test -p benilla-formats --lib native_coverage` exercises complete authored raw ADTs,
actual parsing, raw source identity and filtered-placement/liquid failures.
`cargo test -p benilla-wdt --lib` covers complete raw map records, incomplete or
filtered global-WMO declarations, malformed framing and WDT source replacement.
The app ordering test and live loaded-tile resource path still require full app CI;
no user-local install or native play session has been tested here.
