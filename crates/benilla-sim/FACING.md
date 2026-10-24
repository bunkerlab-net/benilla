# Source-profile current-pose bearing and arc arithmetic

This is a bounded numeric service, not a map/visibility/LOS provider, complete
combat implementation, content producer or gameplay admission. Runtime selection
and receipt ownership are documented in
[the runtime facing contract](../benilla-world-runtime/FACING-AUTHORITY.md).

## Source and licensing

CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

- `src/game/Entities/Object.cpp:1465–1473`: `WorldObject::GetAngleAt`
  subtracts XY coordinates in binary32, calls the `std::atan2(float,float)`
  overload, and adds binary32 TAU only if the result is negative
- `Object.cpp:1503–1527`: `HasInArcAt`/`HasInArc` preserve the self-identity
  shortcut, normalize arc and relative angle independently, fold angles greater
  than PI, and compare inclusive negative/positive half-arc borders
- `Object.cpp:1546–1559`: `IsFacingTargetsFront` first tests the target's
  arc, then the attacker's arc. Equal coordinates are not equal object identity
- `src/game/Maps/MapManager.h:121–132`: `NormalizeOrientation` uses float
  fmod. Positive whole turns produce zero, negative whole turns produce TAU
- `src/game/Entities/Unit.cpp:4428–4431`: `SetInFront` uses `GetAngle`;
  `Unit.cpp:11033–11075`: `UpdateSplinePosition` uses the old map pose for
  target-facing and the same float subtraction/atan2/TAU sequence for tangent
  fallback. Its explicit `<math.h>` include supplies the float overload for the
  unqualified call; the selected profile still pins the implementation
- `Unit.cpp:3059–3066`: player dodge requires **both** front arcs
- `src/game/Spells/Spell.cpp:6245–6248`: `CheckRange` facing asks the
  player's single `HasInArc(target, PI)` after distance checks

CMaNGOS code is GPL-2.0-or-later, Copyright CMaNGOS contributors. The Rust
translations, boundary and software-operation adapters are GPL-3.0-or-later,
2026 Benilla fork contributors. Existing [upstream notices](licenses/COPYRIGHT.md)
and [GPLv3 text](licenses/GPL-3.0.txt) apply. No game files or database rows are
included.

The selected transcendental implementation is
[GNU libc 2.41 e_atan2f.c](https://github.com/bminor/glibc/blob/74f59e9271cbb4071671e5a474e7d4f1622b186f/sysdeps/ieee754/flt-32/e_atan2f.c),
from CORE-MATH `src/binary32/atan2/atan2f.c` revision `7835c5d`.
That file is **MIT**, Copyright (c) 2022–2024 Alexei Sibidanov and Paul Zimmermann;
its complete [notice](src/facing/MIT-NOTICE.txt) is retained. `atan2.rs` preserves
that grant. It is not covered by the VMap sinf/cosf port's separate LGPL notice.
The combined crate remains GPL-covered.

Full-file hashes, normalized only from CRLF to LF, are retained in
[the oracle manifest](tools/facing_source_hashes.sha256) and in
`facing::SOURCE_SHA256`. The algorithm/profile identity is
`cmangos-8ec338a1-facing-glibc-2.41-soft-rne-v1`.

## Numerical profile and boundary

The source names C++/libm operations, not a cross-platform bit identity. This
service explicitly chooses the pinned glibc implementation and emulates every
binary32/binary64 operation with the existing `rustc_apfloat` dependency.
Round-to-nearest/ties-to-even, gradual subnormals, and exact source expression
order are mandatory. Only the source's explicit FMA calls fuse; software
`mul_add_r` performs their single rounding. No host atan2, fmod, trig, FMA,
floating-point environment, fast-math, implicit contraction or platform libm is
used by the implementation. Floating-point status flags/errno are not gameplay
outputs and are not reproduced. Nonfinite inputs reject before evaluation.

The raw bit helpers retain signed zero and subnormal inputs/intermediates:

- `atan2_bits(y,x)` accepts every finite binary32 pair, including signed zero
- `bearing_bits(origin,toward)` accepts native XY coordinates inclusive ±32760
- `normalize_bits` and `has_in_arc_bits` accept finite input orientation/arc
  magnitudes up to `0x41c90fdb`, binary32 four times TAU. Internal relative
  angles may exceed that input bound and still use the exact source reducer
- `has_in_arc_bits` validates all inputs before its source self shortcut;
  its `same_object` parameter is identity supplied by the caller, never inferred
  from coordinates. Current runtime player/NPC calls always use false

Source `ang >= 0` retains negative zero. Source normalization also retains
negative zero. The existing runtime `Real` contract separately canonicalizes
stored zero and rejects subnormal outputs. A genuine tiny bearing which cannot
fit that storage contract fails the staged transaction; it is never clamped.
This raw/source versus runtime-storage distinction is intentional and tested.
Unsupported coordinates/angles reject, rather than extrapolating or silently
normalizing arbitrary input into the admitted domain.

## Evidence and limits

The independent oracle extracts unchanged CMaNGOS bodies from hash-checked
files and compiles the complete unchanged pinned glibc function behind a
minimal compatibility header. `std::atan2(float,float)` alone is redirected to
that source function. The oracle has no dependency on the Rust implementation.
C++ is built with no fast math, contraction disabled, explicit RNE and
sanitizers; source FMA remains explicit. Tests compare exact result bits and
booleans, never epsilons. The 3,351 curated signed-zero, subnormal, axis, ratio-reduction,
PI/TAU and inclusive-border cases include eight verified rare double-double
fallback inputs and their neighbors. Another 155,648 deterministic sampled
inputs are checked through exact result digests; these are not
exhaustive proof of every binary32 pair or multiplayer/platform execution.

Regenerate/verify with
`python3 tools/generate_facing_fixture.py /path/to/mangos-classic /path/to/e_atan2f.c --check`
from this crate. The two source paths must match the pinned manifest. No upstream checkout or
network access is required by normal Rust tests. Runtime tests separately
exercise current source phase, selection, derived receipt replay, tampering,
rollback and incompatible native format refusal.
