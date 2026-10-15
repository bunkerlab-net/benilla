# Bounded initial near-point XY candidate

`near_point::candidate_bits(target_xy, distance, angle)` translates only the
initial `WorldObject::GetNearPoint2dAt` arithmetic, including source map-coordinate
clamping. It returns raw XY binary32 bits. It does not implement the later
collision candidate search, radii, height adjustment, LOS, dynamic-object
exclusion, complete `GetNearPointAt`, PathFinder, or gameplay admission.

## Source, license and profile identity

CMaNGOS Classic revision `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

- `src/game/Entities/Object.cpp:2297–2304`, `WorldObject::GetNearPoint2dAt`
- `src/game/Maps/GridDefines.h:36–57,169–175`, `SIZE_OF_GRIDS`, map-size
  definitions and `MaNGOS::NormalizeMapCoord`

LF-normalized full-file SHA-256 identities, in `near_point::SOURCE_SHA256`:

- Object.cpp: `410904520fe2847a855c396d9fe442065268fd4ff5bb510633c3aadb5c54665b`
- GridDefines.h: `5f5413657f84c5f5964f2a69d342874d8d1f7f5ed2bab969e1d78b0650141d82`

Source copyright CMaNGOS contributors, GPL-2.0-or-later. This bounded translation
and authored numeric boundary are GPL-3.0-or-later, 2026 Benilla fork contributors;
existing [upstream notices](licenses/COPYRIGHT.md) and [GPL text](licenses/GPL-3.0.txt)
apply. The shared sinf/cosf core retains the separate original FSF 2018–2025
LGPL-2.1-or-later grant, complete notice, license and five source hashes described
in [SOURCE-TRIG.md](SOURCE-TRIG.md). No client assets or world rows are included.

The selected policy is
`cmangos-8ec338a1-initial-near-point-glibc-2.41-soft-rne-v1`, together with
`source_trig::SOFT_POLICY` and its source identities. CMaNGOS's native C library
calls are not a cross-platform bit identity: this contract explicitly chooses
the pinned glibc algorithm and software arithmetic.

## Raw input and output boundary

- Each input target coordinate is finite and within inclusive ±32760
  (`0x46fff000`), matching the existing current-pose facing bound
- Distance accepts every finite nonnegative binary32 value, up to `0x7f7fffff`
- Angle accepts finite normalized radians in [0, binary32 TAU], inclusive
  `0x40c90fdb`. Negative nonzero angles and values above TAU reject
- Signed zeros and subnormals are accepted and preserved by the source operations
- Every field is validated before evaluation, even when distance is zero

The actual raw angle must come from the caller's explicit normalized angle or
source-profile current-pose bearing. A bearing can be genuinely subnormal even
when both poses contain ordinary normal coordinates. Do not infer it by
converting through runtime `Real`, canonicalize its signed zero, or substitute a
host atan2. This helper has no object identity, phase, map or request authority.

The input coordinate bound is intentionally wider than the source output clamp.
Input coordinates are never clamped before the arithmetic: for example, a target
x of -20000 and distance 20000 at zero radians yield x zero. An unchanged target
outside the clamp, with zero distance, is clamped only after addition.

## Source operation order

1. Obtain binary32 sine/cosine from the shared software RNE glibc profile
2. Round the binary32 `distance * cosine` product, then separately round
   `target_x + product`
3. Round the binary32 `distance * sine` product, then separately round
   `target_y + product`
4. Apply `NormalizeMapCoord` to each sum using strict greater-than/less-than
   comparisons against ±17066.166015625 (`0x46855455`)

The source limit is `(533.33333f * 64 / 2) - 0.5` and is exactly representable
at the retained binary32 value. The strict comparisons preserve zero signs.
Products and sums cannot fuse; no FMA, host trig, or host rounding state is used.
The candidate alone proves neither ground support nor a clear route. Any
consumer's loaded ground, participant, current phase and other proof obligations
remain separate and must still fail closed.

## Evidence

The [offline oracle contract](../benilla-native-geometry-tests/tools/near_point_candidate_README.md)
extracts the unchanged source near-point and map-clamp bodies from hash-checked
CMaNGOS files and uses the pinned original glibc sinf/cosf bodies. It supplies
258 raw radian and 3,012 candidate cases, a 737,002-radian sample digest, and
a 32,768-candidate sample digest, including signed zero, subnormal, clamp,
cancellation and non-fused rounding cases. Rust tests consume those fixtures
without an upstream checkout or compiler and compare exact bits, never epsilon.
Separate authored tests reject unsupported inputs and exercise the distinction
between the admitted input bound and source output clamp.

These bounded numeric fixtures establish no native-content readiness, complete
near-point search, real-map coverage, live gameplay or host-libm equivalence.
