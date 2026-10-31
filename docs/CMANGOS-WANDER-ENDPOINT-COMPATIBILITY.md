# Selected Wander endpoint distance compatibility

The policy `cmangos-computepathtorandompoint-zero-distance-v1`, version 1, is a
maintainer-approved compatibility amendment to CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. It is not historical undefined-behavior
parity and is separate from the
[BuildPolyPath two-local cold-distance policy](CMANGOS-COLD-DISTANCE-COMPATIBILITY.md).

## Exact amendment

Only the local in `PathFinder::ComputePathToRandomPoint` changes:

```diff
-    float distanceToPoly;
+    float distanceToPoly = 0.0f;
```

Positive binary32 zero has bits `00000000`. On the first call, `clear()` leaves
74 zero slots with active length zero. Both existing active and tail scans can
therefore return without writing this distance, before the nearest-poly append
predicate reads it. Zero defines the existing `!*distance` disjunct; it does not
change the lookup helper, thresholds, filter, query, height result or append rule.

The complete original method, without its trailing newline, has SHA-256
`4a151a1cab7cf8cd9444d2c7edc09f3c5252fefaf5d537ad1b374775929840da`.
The same complete method with only the scalar initialization has SHA-256
`1f553a9cdb97d3441b9f3108fcf69d8c1a7df6101a41fb5e0b849cb07fe9265a`.
The independent observer retains the
[exact source intervals](../scripts/native_wander_source_oracle/source_intervals.json)
and [one-scalar patch](../scripts/native_wander_source_oracle/endpoint-zero.patch).

## Ordered supported path

The source clears active storage and points, sets NOPATH, straight=true and
force=false, updates its filter, selects its query, then draws angle followed by
linear radius. Sampling uses `(angle_draw * 2) * PI` and `radius_draw * maxRange`,
with separate binary32 products and coordinate additions; it is not area-uniform
sampling. The selected software trig profile is the existing bounded glibc 2.41
sinf/cosf implementation, checked against the source-observer vectors.

The loaded native prefix is created from the actual retained source1024 query and
previous opaque Wander terminal. It clears a private copy while preserving every
inactive and tail slot. Query selection is a separate operation after the owning
adapter's updateFilter. The prepared token can then be continued after the draws
without a second selection or clear. Malformed, unsupported or failed native
operations publish no accepted cache; the defined near-zero NOPATH branch below
is an ordinary completed calculation.

After the source minimum-distance test and HaveTile branch, the endpoint lookup
runs first, and can append a polygon beyond the first 74 slots. Real Detour
`getPolyHeight` adjusts endpoint Z before the exact `status == DT_SUCCESS` check.
The subsequent **unpatched** BuildPolyPath start/end lookups can now consult that
retained tail, which initializes their distance locals. The old BuildPolyPath
cold-distance repair is neither selected nor needed on this successful path.
Same-polygon selection executes the source two-point shortcut builder, installs
polygon[0]/active=1, and assigns NORMAL. All inactive and tail storage survives.

The source water/above-ground tail still runs even after successful BuildPolyPath.
The native terminal is only a dependency: the owner adapter must establish those
observations, constructor/filter/model provenance, actual dry segment coverage,
body sweep, ground support, LOS, transaction commit and replay. Filter 13 retains
the source owner's CanSwim capability on dry ground; it never admits wet traversal.

## Bounded refusal and evidence

`LoadedSourceWanderPrefix` and `LoadedSourceWanderTerminal` cannot be created
from serialized polygon references, a Chase result or equal-byte reloaded mesh.
They retain the actual query allocation. Work limits and deadlines remain
operational bounds, not source distance clamps or point limits. The maximum
endpoint lookup budget is 4024 primitives and the BuildPolyPath lookup budget is
8048; common defaults are three and six. One terminal budget unit covers the
single height query and finite two-point terminal, with deadline checks around
both. Each retained vector is limited to 4096 entries.

Supported calculations are dry, fixed non-dungeon, non-transport, same-polygon
NORMAL with explicit normalization disabled, plus the three source-defined
NOPATH branches: minimum distance, missing endpoint polygon and endpoint height.
The strict squared-distance `< 0.01f` branch preserves the cleared vector, active
zero, NOPATH and empty points, retaining the previous end (initially zero from
G3D Vector3 default construction). It performs no HaveTile, endpoint, height or
water/above-ground queries. It is a defined calculation, permitting ordinary
500 ms retry timing with both draws consumed; it is not an unsupported-operation
rollback. Two additional source NOPATH reasons are explicit: endpoint near/far
lookup returns no polygon, or its actual getPolyHeight returns a status other
than exact DT_SUCCESS. The former retains the cleared vector and sampled end;
the latter retains any endpoint append and the end Z written before the status
check. Both require the loaded dry water/above-ground suffix and can retry from
their actual opaque retained state. BuildPolyPath missing-start/end handling is
a different branch and remains refused; its extra source gates are not inferred.
Cross-polygon, far-from-polygon, wet/flying shortcut, dynamic scene,
expired or over-budget cases provide no Route. No replacement traversal,
raycast, smoothing, cold-path relabel or generic admission is introduced.

Native tests exercise real loaded geometry, endpoint-before-BuildPolyPath lookup
order, height adjustment, filter 1 and 13, repeated clear with retained tail,
foreign allocation rejection and failure/retry preservation. The independent
[source oracle](../scripts/native_wander_source_oracle/run_oracle.py) executes the
patched complete random-point method and original reached same-polygon source
branch against actual authored Detour geometry. It separately records external
owner callbacks, launch and timing inputs rather than claiming those as loaded
production geometry evidence.

A separately labeled observer lane loads the unchanged production-admitted
precision fixture. Native differential tests compare its NORMAL, endpoint-ref0
and endpoint-height-failure rows against every retained reference, active length,
point, endpoint and Detour operation in the source transcript. The original
one-quad observer uses a 64-unit tile and remains separate: its bytes are not
rewritten or admitted through the production loader's fixed-grid guard.

## Bounded native reconstruction primitive

The native-only reconstruction primitive is narrower than saved-owner admission.
For one immutable dry same-polygon Wander constructor, every Prepare clears the
active length before lookup. Lookup then ignores the first 74 backing slots and
searches the ordered retained tail. In this domain slots 1–73 remain zero,
NORMAL writes slot zero, and tail mutation only appends. A candidate operation
subsequence retains every append-producing calculation, the last NORMAL
calculation, the last non-MinimumDistance calculation, and the final calculation,
in their original order with duplicates removed. The extra final writers retain
slot zero across NOPATH and the old endpoint across MinimumDistance.

With the existing 4,096-reference resource ceiling and 74 initial slots, at most
4,022 calculations can append. The three final writers give at most 4,025 retained
calculations, independent of intervening no-append ticks or calculations. Every
retained operation still performs actual native Prepare, query selection and
calculation. The typed reopen caller supplies a fresh allocation; saved polygon
references are comparison
outputs, never reconstruction inputs. Source compatibility policy, geometry,
query capacity, filter and normalization configuration must remain identical.
This bound does not extend to other path branches or mutable configurations.

`SourceWanderWitness` captures successful native operations using the exact opaque
predecessor identity, not equal coordinates or equal cache contents. A staged
clone allows capture to become accepted only with the owner's commit ACK. Its
inert input recipe contains no polygon references and is validated for bounded
size, strict original ordinal order, checked count arithmetic and the canonical
union of actual appenders and last writers. Re-execution on the supplied actual
query begins with fresh constructor storage and preserves Prepare, selection and
calculation order. The opaque certificate retains that selected query, actual
terminal and resumed witness; a fresh attempt challenge prevents transferring a
certificate between unrelated reopen attempts. It is native semantic evidence,
not world, owner, inventory or composite geometry authority.

Semantic retained-state equivalence is a different claim from original transcript
equality. Elision may change a retained operation's recorded before-state;
ordinary retry observation comparisons remain unchanged. The original accepted
operation count and cache revision are separate from the number of retained
operations. Reconstruction does not replay omitted owner ticks, draw new gameplay
RNG, or recreate historical dispatch seals and ACKs.

## Selected typed component reopen boundary

The app's typed bridge is `cfg(test)` only. Its runtime pending-world boundary is
available only to the explicit `synthetic-native-tests` feature/test domain;
neither admits authentic content or enables native startup. The selected scope
is one current NORMAL or MinimumDistance NOPATH, dry same-polygon Wander owner
with immutable constructor, query configuration and filter. Earlier retained
MinimumDistance, EndpointMissing and EndpointHeight calculations are allowed;
current EndpointMissing/EndpointHeight, Chase, Home and suspended owners remain
outside this restore boundary. Generic cached-world restore remains closed.

The app capture retains accepted native input ordinals together with the original
constructor and pre-draw preparation/request metadata and exact angle/radius draws.
These serialized requests are inert expectations, not live dispatch permission.
Reopen requires fresh unpublished loaded providers, matching immutable inventory
and configuration, and source-ordered revalidation of constructor/createFilter,
clear/updateFilter, query selection, retained draw values, reached endpoint lookup
and height, the applicable dry suffix, and actual loaded corridor/body/ground/LOS
coverage where required by the branch. Historical request checks share the ordinary
initialized-body policy; the final retained request must equal the generator-owned
successful head and current saved spline in full for NORMAL, or the generator-owned
failed-route head for current MinimumDistance. Native format `NATIVE_FORMAT_VERSION` requires both
nullable head fields explicitly, and recipe version 3 distinguishes this owner
contract. A retained older spline
cannot stand in for the failed calculation. The narrow failed restore requires
the head to remain the current motion query at the same position. Draw validation uses the saved counter-addressable stream without
advancing gameplay RNG. Serialized references never seed the native query.

The pending runtime boundary requires the replayed terminal to match the saved
current Wander semantic state completely: full backing storage including
inactive references, active length, filter, path type, endpoints, points and query
configuration. It also binds the original operation count/revision and exact
checkpoint. Runtime tick, timers, RNG counter, spline and path identities remain
those of that checkpoint. Only after native, runtime and loaded-geometry checks
succeed may the app publish the world, reconstructed provider ownership and commit
boundary together. The actual selected retained query is transferred rather than
replaced with a newly unselected lease.

The separate Wander store persists a current checkpoint plus opaque recipe bytes
under an exclusive writer and checked predecessor generation. Opening it returns
inert data, never a restored world or callback-granted permission. Its combined
checkpoint/recipe payload ceiling is 8 MiB; the native limits remain 4,096 backing
references and 4,025 retained operations. These are operational resource ceilings,
not source movement semantics or a full-history journal. If optional capture exceeds
its supported lineage or byte bounds, the accepted gameplay operation remains
committed but that generation cannot be saved through this bridge. A failed save
does not truncate its witness or replace the previously durable pair.

The executed integration test reached 2,201 alive ticks with four fresh mid-leg
reopens at native revisions 2, 15, 32 and 48, followed by matching arrival and
next-leg continuation against an uninterrupted baseline. It retained at most two
operations in this authored entry-6-shaped dry filter-13 scene. All five reopen
component tests passed at the preceding NORMAL-only checkpoint, including wrong
geometry, then-unsupported current NOPATH, failed
reconstruction/rollback retry and mismatched durable checkpoint rejection. The
complete concrete corridor suite passed 534 tests. Store publication faults are
verified separately at the opaque-pair store layer, rather than injected into the
concrete loaded reopen test. Native equivalence tests separately cover fixed dry
filters 1 and 13. These receipts do not establish authentic gameplay/startup support.

### Current failed-route head and pause acceptance

The checkpoint owns the exact validated failed request and typed source reason,
including prior-cache input and constructor identity. This is persistence evidence,
not a new source field or authority. Success and generator reset clear it; an
ordinary failed calculation preserves it through the source 500 ms retry pause.
Simulation also preserves it under suspended Wander and later facing queries;
those broader shapes are not admitted by this selected reopen bridge.

MinimumDistance returns before source start/end writes, endpoint lookup and spline
launch. Reconstruction therefore checks the empty point list, zero active length,
full retained backing, filter and retained actual endpoint without substituting
the older successful spline. Fresh replay must reproduce the terminal natively.
Natural finalized NORMAL pauses use the existing same-owner path and retain their
exact countdown. New acceptance tests cover cold and retained MinimumDistance,
fresh reopen through retry, finalized pause, forged head and recipe rejection.
All nine concrete reopen tests passed, including these four new cases and the
existing 2,201-alive-tick baseline. The predicted retained-failure seed 841 was
confirmed by actual loaded geometry. Runtime verification passed 839 unit tests
and 40 integration tests; the three Home mailbox regressions also passed. These
are component receipts, not authentic populated-world startup acceptance.

## Live suspended-owner retirement

The app commit boundary represents the active generator and its suspended default
separately. The selected model permits active Chase plus one suspended Wander;
this is not a general limit on the CMaNGOS movement stack. Each live constructor
retains its own query binding, committed candidate and optional reconstruction
evidence through an accepted outer ACK. Home retires Chase, but the suspended
Wander remains alive until arrival resets the default. Home's PathFinder is local
to its route calculation and is never promoted to a retained owner. Reset replaces
the old Wander finder, preserves count and clears its timer; death clears both.

An earlier Home component assertion expected zero retained constructors during
Home. That was an ownership-scope error: the suspended default remained alive in
the source and simulation. Continuation passed because later Reset replaced its
finder, but the premature app retirement could not support faithful persistence.
The corrected acceptance must retain that default through Home.

This live-ownership correction does not open Chase/suspended current-checkpoint
restore. NORMAL Wander retains its validated successful request and complete route
alongside its current cache, independently of the global spline. A later failed
calculation clears that successful head and retains its own failed head; Reset
clears both when replacing the finder. The checkpoint-owned head binds owner,
constructor, query/time, cache revision and complete calculation, but is serialized
evidence rather than loaded authority. Chase still has no separate challenged
native reconstruction certificate, and retaining this head does not open a dual-owner
restore path. Live ACK evidence is not a substitute for fresh reconstruction. The separate existing cache-free-anchor recipe may reconstruct
the pair by executing every accepted input through fresh providers and comparing
complete records. Its 2,048-record bound remains; it is not compact current-checkpoint
restoration. Three concrete tests passed: sustained Chase with retained replan
through Home/Reset/relaunch, lethal clear, and fresh full-history reconstruction
with failure atomicity and exact next-tick equality. Direct cached restore remains
rejected in both original and reconstructed dual-owner worlds.

When a finalized Wander spline survives entry into a Chase that needs no route,
snapshot validation resolves the spline's retained cache through its Wander
owner, rather than requiring an unrelated Chase cache. The spline must be
finalized and its exact route evidence must match that suspended owner. This
corrects ordinary saved-motion validation without opening loaded cached restore.
