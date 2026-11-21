# Original-source one-candidate WMO doodad transforms

This offline authored oracle executes unmodified `Doodad::ExtractSet` from
CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, the unmodified
`WMOInstance` constructor, original filename helpers and original
`GenerateUniqueObjectId`. Complete original G3D `Quat.cpp`, `Matrix3.cpp`,
`Vector3.cpp` and `g3dmath.cpp` translation units supply the arithmetic.
No Rust transform supplies an expected result.

There are 85 isolated cases, each with exactly one member of the original
`unordered_set<uint16>` reference collection. 79 cases emit one child record;
six exercise source-defined non-emission. This does not establish ordering or
IDs for a multi-candidate collection, extraction-wide IDs, complete directory
production, assembly, native-install compatibility, or gameplay admission.

## Reproduce

```sh
python3 crates/benilla-native-geometry-tests/tools/wmo_doodad_transform_generate.py /path/to/mangos-classic
python3 crates/benilla-native-geometry-tests/tools/wmo_doodad_transform_generate.py /path/to/mangos-classic --check --sanitize
CMANGOS_SOURCE=/path/to/mangos-classic python3 -m unittest discover -s crates/benilla-native-geometry-tests/tools -p wmo_doodad_transform_test.py
```

`--check` compares the entire fixture filename set and all bytes in a temporary
directory without replacing committed expectations. The 430 files live under
[fixtures/wmo_doodad_transform_source](fixtures/wmo_doodad_transform_source).
The separate [source guard](wmo_doodad_transform_source_hashes.sha256) verifies
extractor source/declarations and transitive original G3D headers before any
compilation. [Provenance](fixtures/wmo_doodad_transform_source/provenance.tsv)
records every source hash, the extracted-method hash, compiler and libc profile.
[Source fragments](fixtures/wmo_doodad_transform_source/source-fragments.cpp.test-fixture)
retain the original GPL notice and each included function byte-for-byte.

Verified profile: Linux x86-64, GCC 14.2.0, glibc 2.41, little-endian IEEE binary32,
round-to-nearest, C++17, `-O0`, no fast math, no floating-point contraction and no
strict aliasing. NaN sign/payload bits and libm output bits are observations of
this profile, not cross-platform guarantees. Sanitized rebuilding uses
`-fsanitize=undefined,float-cast-overflow -fno-sanitize-recover=all`.

## Authored boundary

The harness substitutes only archive acquisition: original `MPQFile` declarations
read a closed in-memory map containing an authored 64-byte MODF, never an
installation or external archive. The constructor starts at byte four, matching
the call site after ADT's name-table index read. Temporary raw files contain the
eight-byte raw signature plus an authored signed 32-bit collision count, exactly
the data these two routines inspect. They are header probes, not complete raw
model products. The child short-header case exercises the original checked
`fread` result; no truncated parent file is supplied because its unchecked read
could use an uninitialized count.

The parent constructor and child extraction write separate output streams so
both source results can be inspected. This splitting changes no routine or
state. The original ID map is cleared once before each parent. An emitting
parent gets ID 1 and its child gets ID 2; a parent early return leaves the child
with ID 1. These are isolated fixture labels. A production comparison with an
independent allocator must explicitly replace/ignore bytes 18 through 21, the
ID field; it must not promote these labels to an extraction-wide ID contract.

The harness calls `ExtractSet` after every constructor result, as the original
ADT call site does. Destructible parents, absent parent raw files and parents
with zero collision vertices leave the read position unmodified. Successful
parents first replace simultaneous zero X/Z with `533.33333f * 32`, then apply
`fixCoords` `(z,x,y)`. Doodads therefore remain eligible even when the parent
itself does not emit, and consume the different retained parent position.

All source input float fields are finite. Zero/underflow-norm quaternions and
max-finite parent rotation intentionally expose defined IEEE exceptions in this
profile: their original emitted record has nonfinite transform fields. These
are rejection witnesses for a finite-output safety boundary, not approved
finite Rust outputs. The source's quaternion squared-norm overflow case, with
finite components, normalizes to zero and produces an identity matrix; it is
recorded separately with its overflow exception. No out-of-bounds reference,
unterminated/oversized path, nonfinite input, integer overflow, malformed MODF,
or unchecked failed parent read is executed. Such rejection belongs to the
Rust boundary's tests.

## Cases

- Successful, destructible, zero-collision and missing-raw parent states; zero
  origin, signed-zero origin and X-only/Z-only zero; rotated early returns
- Normalized, non-unit and sign-negated quaternions; positive-subnormal squared
  norm, zero/underflow norm, large finite norm and overflowing squared norm
- Noncommuting parent/quaternion composition, positive and negative parent
  rotations, large finite rotation and max-finite radians-overflow observation
- Both exact Euler gimbal branches, nonzero third-angle gimbal results,
  immediate representable 90-degree neighbors and 89.9/90.1-degree neighbors
- Fractional, negative, zero, signed-zero, min-subnormal and max-finite child
  scale; source scale is serialized directly and does not scale local position
- Ignored parent MODF scale, large finite positions and signed-zero local position
- Both worldspawn coordinates versus either coordinate alone; invalid selected
  set, empty set range, missing set list, nonzero selected reference, absent or
  zero-collision child raw data and the guarded short child header
- `.mdx`, `.mdl` and `.m2` names, including original case/space helpers. The
  `.mdx`/`.mdl` rewrite retains the pre-rewrite recorded length and writes its
  trailing NUL; `.m2` writes no terminator
- `portable_mdx`, `portable_mdl`, `portable_m2` use exactly `Models\Solid.mdx`,
  `Models\Solid.mdl`, `Models\Solid.m2`, with raw basename `Solid.m2`, for bounded
  acquisition tests. Additional `portable_*` cases cover quaternion, rotation,
  scale and parent-disposition variants; `math_parent_z90` and
  `math_quat_z90_scaled` provide exact isolated quarter-turn examples.
  `math_gimbal_signed_zero` records a negative-zero matrix element selecting
  -180 rather than +180 degrees through the original `atan2` branch.
  Other mixed-case names establish original arithmetic and
  serialization; they do not assert admission by a stricter portable-name policy

The names `gimbal_positive_exact`/`gimbal_negative_exact` mean the quaternion's
Y and W components are exactly +/-1 and 1. Their normalized matrix element is
actually one ULP inside +/-1 (`3f7fffff`/`bf7fffff`); original `toEulerAnglesXYZ`
therefore takes its unique-angle branch. The parent-gimbal and nonzero-X gimbal
cases hit the actual +/-1 branch. [Matrices](fixtures/wmo_doodad_transform_source/matrices.tsv)
record this distinction rather than assuming it from mathematical real numbers.

## Fixture schemas

All tables have a `# `-prefixed header. Integers/booleans are decimal; every float
is its eight-digit binary32 bit pattern. Names/path bytes are hexadecimal.
Rows are authored case order, never a claim about source reference ordering.

[cases.tsv](fixtures/wmo_doodad_transform_source/cases.tsv), 33 columns:

1. case
2. parent_flags
3. parent_vertices
4. parent_exists
5. child_vertices
6. child_exists
7. child_short
8. map
9. tile_x
10. tile_y
11. client_id
12. selected_set
13. sets
14. set_start
15. set_count
16. reference
17. parent_scale (the original uint16 MODF field, ignored by these routines)
18–20. parent_px, parent_py, parent_pz (before the constructor)
21–23. parent_rx, parent_ry, parent_rz (degrees)
24–26. child_px, child_py, child_pz
27–30. quat_x, quat_y, quat_z, quat_w
31. child_scale
32. path_hex (without terminating NUL)
33. raw_basename (the declared authored temporary raw filename)

[results.tsv](fixtures/wmo_doodad_transform_source/results.tsv), 23 columns:
case, parent_emitted, parent_px, parent_py, parent_pz (after constructor),
child_emitted, isolated_child_id, flags, record_length, name_length, name_hex,
px, py, pz, rx, ry, rz, scale, finite_transform, invalid, divide_by_zero,
overflow, underflow. The last four are original extraction's captured IEEE
exception flags; inexact is deliberately omitted. Non-emitting rows use zero
metadata and `-` for absent name/float values.

`matrices.tsv`, 29 columns: case, parent matrix (nine row-major float bits),
quaternion matrix (nine), quaternion-times-parent matrix (nine), and the original
`toEulerAnglesXYZ` return value (`euler_unique`). These diagnostics are computed
by original G3D methods after capturing the extraction's exceptions.

Each case directory contains:

- `input.modf`: full 64 bytes: uint32 name index at 0, uint32 client ID at 4,
  position at 8, degrees rotation at 20, min bounds at 32, max bounds at 44,
  uint16 flags/set/name-set/scale at 56/58/60/62. The original constructor reads
  the 60 bytes after the name index; its unused fields are never inspected
- `input.modd`: 40 bytes: name offset 7 at 0, position at 4, quaternion XYZW at
  16, scale at 32, color `0x12345678` at 36
- `input.modn`: `unused\0` followed by the declared path and final NUL
- `parent.dir`: exact constructor bytes, or empty when no parent is emitted
- `child.dir`: exact single-candidate output, or empty. Fields: uint32
  map/tileX/tileY/flags at 0/4/8/12; uint16 name-set 0 at 16; uint32 isolated ID
  at 18; position at 22; reordered Euler degrees at 34; scale at 46; uint32
  retained name length at 50; exactly that many name bytes at 54

## Notices

Authored harness, generator and tests are GPL-3.0-or-later. Guarded CMaNGOS
fragments retain GPL-2.0-or-later notices; see the existing
[CMaNGOS notice](../../benilla-app/src/net/p2p/world/native_geometry/products/CMANGOS-NOTICE.txt).
G3D's Modified BSD notice remains in [vmap_G3D_LICENSE.cpp.txt](vmap_G3D_LICENSE.cpp.txt)
and the guarded original `dep/g3dlite/license.cpp`.
