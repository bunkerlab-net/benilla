# Facing numeric source fixtures

These authored numeric inputs exercise the bounded current-pose facing service.
They contain no world/database rows or game-install assets. Expected bits are
computed only by the independently compiled source oracle, never Rust or host
`atan2`/`atan2f`.

## Source profile

- CMaNGOS Classic commit `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:
  `Object.cpp`'s two `GetAngleAt` overloads and `HasInArcAt`, and
  `MapManager.h`'s `NormalizeOrientation`, are extracted unchanged. `Common.h`'s
  `M_PI_F` definition is extracted unchanged as well.
- glibc 2.41 commit `74f59e9271cbb4071671e5a474e7d4f1622b186f`:
  `sysdeps/ieee754/flt-32/e_atan2f.c` is compiled unchanged. Its source incorporates
  CORE-MATH revision `7835c5d`; the upstream MIT notice remains in that input.
- `tools/facing_source_hashes.sha256` pins complete input-file bytes after CRLF-to-LF normalization only. No download
  or pin update occurs during generation. The script requires local source paths.

The C source uses a minimal compatibility header for bit casts, branch hints, and
an unused symbol-alias declaration. The C++ harness supplies object coordinates
and identity and binds `std::atan2(float,float)` to the pinned C function through
a preprocessor namespace alias. `std::fmod(float,float)` retains its source
meaning and appears only in the oracle. Object error logging is an unreachable
abort stub; self identity returns before that overload is invoked. The
CMaNGOS function signatures and bodies are never rewritten.

The compiler profile is C11/C++17 with `-O2 -fno-fast-math -ffp-contract=off
-frounding-math -fsanitize=undefined,float-cast-overflow
-fno-sanitize-recover=all`, IEEE binary32/binary64, `FLT_EVAL_METHOD == 0`, and
`FE_TONEAREST`. Only explicit source `fma` calls are fused. The generator checks
undefined symbols with `nm` and refuses a dependency on host `atan2`/`atan2f`.
This establishes the named source numeric profile, not all historical CMaNGOS
platforms or a complete CMaNGOS server build.

## Reproduce

From the repository root:

```sh
python3 crates/benilla-sim/tools/generate_facing_fixture.py \
  /path/to/mangos-classic-8ec338a1704e7dcb1c0213eb7ed58f9231ade40f \
  /path/to/glibc-2.41/sysdeps/ieee754/flt-32/e_atan2f.c --check
cargo test --locked -p benilla-sim --test facing
```

Omit `--check` only to regenerate reviewed fixture changes. `--find-rare`
independently searches deterministic positive, similarly-sized finite operands
for eight non-tiny fallback inputs; those input bits are also fixed in the
normal generator. A branch-hint compatibility wrapper records entry into the
source rare branch without changing its expressions. Regeneration rejects a
named fallback case if it no longer enters that branch.

`oracle.tsv` records a case name, operation, raw hexadecimal input words, expected
hexadecimal result, and rare-branch flag. Boolean arguments and results are u32
zero/one. Coverage includes signed zero, normal/subnormal extremes, all finite
atan2 exponent ranges, small/large and general ratios, ratio branch thresholds,
eight non-tiny fallback inputs and one-bit/sign neighbors, signed full turns,
inclusive arc borders and adjacent rays, coordinate bounds, and coincident
positions with distinct identity.

`sample-digests.tsv` covers 131,072 raw atan2 pairs and 8,192 each of normalization,
bearing, and arc inputs. Each operation restarts xorshift32 at `0xa17a2f41` with
shifts 13/17/5, modulo-bounds magnitudes using integer bit patterns, and hashes
all input and answer words in little-endian order. Python and Rust independently
implement that integer-only input generator; expected results come from C/C++.
The Rust tests also reject nonfinite and immediately out-of-domain inputs in
every field, including fields unused by the source's self-identity shortcut.
Those rejection tests establish the service boundary, not extra CMaNGOS rules.
