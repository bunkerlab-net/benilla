# Selected default Wander movement

This component follows CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f` for ordinary random-moving creatures.
A random default generator is not an Idle generator or a manufactured Chase
command. Its source respawn center, radius, movement type, creature capabilities,
model dimensions, loaded geometry and random draws have separate owners.
Production native admission remains closed.

## Source order and lifetime

The source `RandomMovementGenerator.cpp:29–81` initializes the active state,
allocates a PathFinder, removes any supported previous target/melee state and
stops a prior spline. Initialization does not choose a random destination or
launch movement. The first ordinary generator Update reaches its initially
expired timer.

The current component records semantic PathFinder construction identity at
Initialize. First Prepare materializes constructor dependencies against the
current supported owner state and an already-loaded native query. It does not
retain initialization-time constructor filter observations or establish native
allocation timing. Unchanged authored dry inputs can yield equivalent state;
historical constructor callbacks are a separate unproved integration boundary.
The source constructor obtains the default query and creates/updates its filter;
SetCurrentNavMesh belongs to the later ComputePathToRandomPoint operation.

Unit spline progression precedes movement-generator Update. A running spline
does not consume the Wander pause timer. Once finalized, the source updates the
timer and may attempt a new location. The no-free-movement branch has its own
source timer/update behavior and must not be confused with a running spline.
Successful movement decrements the retained move count or draws a new count
(1–3) and pause (3000–10000 ms), in that order. A defined failed attempt keeps
its consumed angle/radius draws and source cache mutations, then sets the ordinary
500 ms retry delay without launching a new spline. A prior finalized successful
spline may remain as history; a cold failed attempt has none. Near-zero distance
returns before owner
ignore-pathfinding/tile/endpoint/tail checks. A loaded endpoint polygon miss or
height-status failure instead retains the source water/ground suffix. Missing
ownership, unsupported branches and resource exhaustion remain transaction
failures, never invented source NoPath results. Extra random-run flags and controlled-unit branches require their
own explicit supported source closure.

Wander is the default generator retained underneath temporary Chase and Home.
Interrupt clears motion and stops the spline. After Home expires, Reset sets
its timer to zero and calls Initialize, replacing the PathFinder while retaining
the move count. Respawn creates a new default owner with initial count one.
These operations cannot be collapsed into a generic Idle replacement.

## Endpoint selection and explicit compatibility

`PathFinder.cpp:1351–1440` clears active path length while retaining its vector,
selects straight-line/nonforced mode, updates the current unit filter and map
query, then consumes the angle and radius draws. The sample uses a linear radius,
not square-root area sampling or Detour's random-point API. The source's strict
squared-distance threshold precedes query/tile gates. A loaded endpoint lookup
then selects the polygon, reads its height, and invokes BuildPolyPath. Water and
above-ground checks remain in the source suffix even after successful routing.

The source declares an uninitialized `distanceToPoly` scalar before the random
endpoint lookup. The explicitly selected compatibility policy
`cmangos-computepathtorandompoint-zero-distance-v1` initializes only that scalar
to zero. This is a documented deterministic deviation at an undefined source
read; it is not bit-for-bit parity with undefined C++. Its oracle records the
pinned source hash, original interval and exact one-scalar patch separately.
The independent [source oracle](../../scripts/native_wander_source_oracle/README.md)
retains the original and patched source intervals. The detailed
[compatibility contract](../../docs/CMANGOS-WANDER-ENDPOINT-COMPATIBILITY.md)
records the selected operation and cache lineage. All other defined source
operations and their order remain unchanged.

This policy is distinct from `cmangos-buildpolypath-zero-distance-v1`. On the
selected successful route, the initial endpoint lookup appends its actual
polygon reference to retained storage. The subsequent BuildPolyPath lookups
therefore find a retained tail and define their own distance outputs. That
continuation does not need or apply the separate cold-distance repair. Passing
an already-mutated random endpoint owner to a constructor-empty Chase route
would lose this source behavior.

The selected same-polygon terminal is the real two-point NORMAL path from
`PathFinder.cpp:506–519`. Cold and retained Wander attempts preserve their own
constructor/cache lineage. Unsupported wet, cross-polygon or missing-owner
paths reject explicitly; they do not acquire shortcut or walking defaults.

## Geometry and admission boundary

Loaded support is not by itself body clearance. The concrete LocalCorridor
retains its raw coverage, exact collider membership, Avian body/hull checks and
LOS checks. A thin test host can execute that same implementation and the real
parsed-terrain fixture without compiling the entire application test unit.
An authored loaded scene is still not actual Northshire terrain, source cell
membership or complete imported content.

Recorded checkpoints preserve the semantic owner. Ordinary direct restore and
saved-session reopen still reject retained native query/cache allocations.
Receipt replay from cache-free genesis reconstructs and compares intermediate
checkpoints, then continues through their suffix; it does not rehydrate a saved
native allocation.

The required `WanderSnapshot.failed_route` field is either explicit null or a
`WanderFailedRouteHead` containing the exact latest failed `RouteRequest` and its
typed `WanderNoPathReason`. An actual validated NoPath calculation sets it;
success and owner reset clear it. Its request retains the prior cache,
constructor/dispatch identity, query and time, sampled angle/radius and exact
origin/raw endpoint. Structural restore validates the prior-to-current cache
transition and retry timer. An older successful spline cannot supply this head.
The serialized request and cache remain evidence, never loaded query authority.

The required `WanderSnapshot.normal_route` field is either explicit null or a
`WanderNormalRouteHead` containing the exact latest cache-backed successful
`RouteRequest` and its complete `CompleteRoute`. Cache-free legacy routes leave
it null. The checkpoint owns this evidence independently of the spline, including
after movement has finalized and while Wander is suspended below Chase/Home.
A failed attempt clears the NORMAL head; owner reset clears both heads and the
cache before assigning a fresh construction identity. A missing field is not
implicitly null, and a matching spline or reconstruction recipe cannot supply a
missing head. The head and current cache must describe the same accepted
calculation; the failed and NORMAL heads cannot both be present.

The synthetic-only `PendingWanderReopen` path stages one current owner with a
NORMAL successful route or a MinimumDistance NoPath head. Other owners must be
cache-free Idle. Chase, Home, suspended Wander and other NoPath reasons remain
closed. NORMAL accepts the current moving or finalized successful spline only
with its exact checkpoint-owned successful head.
MinimumDistance accepts no spline or an older finalized success and binds its
failed request to the current motion query and position. Its terminal semantics
are path type 8, active length zero and no points; from/raw endpoint come from
the failed request, while actual endpoint, storage and filter come from the
current cache. MinimumDistance preserves the preceding actual endpoint and
storage, including the constructor-empty values on a cold attempt.

Historical operations and angle/radius draw pairs must match the saved owner,
profile, constructor, query/time bounds and counter-addressed RNG stream. The
final operation must equal the entire checkpoint-owned head, so a same-output
recipe cannot relabel a query, draw or prior cache. Fresh native replay receives a
one-attempt challenge; release compares the resulting certificate, operation
count and every terminal semantic field. The original checkpoint remains
unchanged. The app coordinator separately checks the exact loaded products and
publishes the fresh provider and restored world together; failure publishes
neither. This bounded component route does not open generic saved-session or
production native admission.

The selected dry same-polygon HomeForceDestination continuation composes with
this default owner; its constructor, second interruption and reset contract are
documented in [HOME.md](HOME.md). Other Home routing branches remain unsupported.

The pinned base entry-6 rows include 24 random-moving and seven idle map-0
spawns. Successful bounded movement does not establish their final loader-order
capture, actual model 10913 dimensions, complete behavioral content or normal
solo startup. Native format 81 (`benilla-native-creature-string-identities-v81`) separates the
required failed and NORMAL owner/receipt shapes. The app reconstruction recipe
is version 3 and rejects the earlier owner schemas. The motion policy is
`cmangos-8ec338a1-ground-linear-home-wander-v7` and the Wander policy is
`cmangos-8ec338a1-default-wander-ground-walk-v3`; native50 and all prior
experimental formats reject without migration. Character/profile saves remain separate.
