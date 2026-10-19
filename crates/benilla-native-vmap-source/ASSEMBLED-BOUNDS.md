# Source placement bounds

`PreparedDiscovery::assembled_spawn_bounds` creates the selected record stream,
indexes first-wins spawns and calculates bounds in ascending generated-ID order.
It returns the original captured records and the corresponding bounds/flags.
This is preparation for the map BIH, not a complete collision map.

For M2 placements it scans every retained raw vertex, including vertices unused
by triangles. Each vertex is scaled, then rotated. The source's exact double
constant 3.1415926535898 converts degree angles to radians before the f32 Euler
call. This differs from both standard double PI and the earlier doodad writer's
float-only conversion. Rotation is Rz(rot.y) * (Ry(rot.x) * Rx(rot.z)); dot-product
order and unfused f32 operations are retained.

The first transformed vertex initializes both endpoints. Later equal components
keep the newest vertex's bit pattern, including signed zero, as G3D's std::min/max
operand order does. Spawn position is added only after the complete scan.
M2 receives HAS_BOUND. Ordinary WMO endpoints remain unchanged; global non-M2
WMO endpoints receive the original x/y shift and explicit +0 on z. No endpoint
normalization, triangle-based tightening or group-bound substitution is applied.

## Safety envelope

This adapter consumes privately captured one-group M2 raw products. The existing
extractor requires positive collision indices and valid in-range vertices, so
zero-vertex and arbitrary multi-group raw models are outside this API. The general
original assembler accepts empty groups and can produce a translated zero box;
that behavior is not needed for this narrower captured-product input.

Finite inputs and finite intermediate vertex arithmetic are required. Rotation
is limited to inclusive -360 through 360 degrees by the named source-trig adapter.
Finite zero/negative scale and harmless underflow are retained. These are explicit
stricter validation rules; the original C++ routine has no equivalent validation.
A global 16,777,216 vertex-visit budget bounds repeated large-model instances.
Cancellation runs before each spawn and every 256 vertices. Input records, index
work and allocations retain their separate limits. Capture is rechecked after
bounds calculation; failures return no partial result and write no files.

## Verification

Three original M2 assembler fixture bounds match bit-for-bit: identity, translated
half-scale signed-zero geometry, and a rotated/scaled instance. Two original WMO
fixtures verify ordinary endpoint preservation. A separate source-derived global
shift case checks the +0 z signed-zero behavior. These authored fixtures do not
establish arbitrary native C++ libm or real-installation map parity.

An authored MPQ integration carries a parent, multiple doodads, a separate M2 and
duplicate cross-tile IDs through capture, record generation, indexing and bounds.
It samples cancellation throughout the complete path and at every final callback. No fixtures contain game assets.

Next steps remain raw model/group BIHs, map BIH primitive ordering, global/tile
serialization, complete-map original assembler/query comparison and atomic output
publication. Gameplay admission and populated-world startup remain separate work.
