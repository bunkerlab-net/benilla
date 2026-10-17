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
