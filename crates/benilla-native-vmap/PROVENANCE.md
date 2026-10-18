# Bounded raw collision extraction

The dry WMO raw geometry adapter has its own [source contract and scope](WMO-PROVENANCE.md).
The following sections describe the M2 adapter.

The M2 adapter ports only `Model::open`, `fixCoordSystem` and
`Model::ConvertToVMAPModel` from
[CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f](https://github.com/cmangos/mangos-classic/tree/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f).
The source files are `contrib/vmap_extractor/vmapextract/model.cpp`,
`modelheaders.h`, `vec3d.h` and the `szRawVMAPMagic` definition in
`vmapexport.cpp`. Their original hashes are recorded in
[SOURCE-SHA256SUMS](SOURCE-SHA256SUMS).

CMaNGOS's original code is GPL-2.0-or-later; its
[notice](CMANGOS-NOTICE.txt) is retained. Upstream authorship is recorded in
[AUTHORS.md](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/AUTHORS.md)
and the upstream git history. This Rust port and its validation and tests are
2026 Benilla fork contributors, GPL-3.0-or-later, exercising the compatible
later-license option. The [GPL-3.0 text](../../LICENSE-GPL-3.0) ships with the
repository. No game-install bytes, models or third-party world database rows
are bundled.

## Exact source behavior

`extract_m2_collision(&[u8])` is a pure immutable-byte interface. It returns an
explicit `NoCollision` for a zero `nBoundingTriangles`, or complete raw
`VMAPs05\0` bytes for a validated nonempty hull. The count is the number of
`uint16` indices, not the number of triangles. The packed 324-byte source
header locates indices at `0xec` and vertices at `0xf4`; each count is followed
by its absolute file offset.

The source's two coordinate passes are retained: `(x,y,z) -> (x,z,-y)` when
opening, then `(x,y,z) -> (x,-z,y)` when writing. Together they restore the
original finite vertex bits, including signed zero. The second and third
indices of every triangle are exchanged. Vertices remain in their original
order, including unreferenced vertices; triangles are not sorted, welded,
filtered or repaired.

Raw output has exactly one group and the source's zero root WMO ID, group
flags, group ID, group AABB and liquid flags. `GRP ` has one branch containing
the index count; `INDX` and `VERT` sizes and counts match the source writer.
No vertex-derived bounds are substituted for the serialized zero group box.
Every integer and binary32 value is encoded explicitly little-endian to match
the pinned source's little-endian ABI, without depending on host struct layout.

Normals, authored bounds, render geometry and unrelated arrays are not read
by these source methods and are not validated by this adapter. A zero index
count ignores all collision offsets/counts other than that count, including
stale or otherwise invalid vertex fields. This matches `Model::open`'s early
return, rather than the separate render/native collision witness's stricter
empty-hull and normals contract.

## Explicit safety envelope

The original extractor copies and dereferences input without these checks.
They are deliberate adapter restrictions, not claims about upstream validation:

- A complete packed header, `MD20`, and versions 256 or 257 are required,
  including for `NoCollision`. This profile matches the established strict
  collision witness in
  [benilla-formats](../benilla-formats/src/models/native_collision.rs), not
  the unchecked upstream `Model::open`; later versions are refused
- Complete raw input is capped at 64 MiB, collision vertices at 65,536 and
  indices at 196,608, yielding at most 1,179,744 output bytes
- Nonempty consumed arrays must start after the header and fit the input;
  multiplication, 32-bit source offset addition, host-size conversion and
  output-size arithmetic are checked. No alignment restriction is imposed
- Nonempty index arrays must contain whole triangles and every index must
  name an existing vertex. Every vertex coordinate must be finite, including
  unused vertices. Degenerate triangles and finite extreme values are retained
- Output uses one `try_reserve_exact` allocation, after validation, and has no
  intermediate index or vertex allocation. Allocation failure is an error

The limits bound input traversal and output allocation; they are not a claim
of a hard operating-system time or resident-memory limit. Safe rejection is
never converted into `NoCollision`. Overlapping consumed arrays or trailing
unconsumed data are not silently rewritten.

## Evidence and scope

The unit tests compare full outputs byte-for-byte against the independent
pinned C++ extractor fixtures
[authored version 256](../benilla-native-geometry-tests/tools/fixtures/m2_vmap_source/authored-m2.m2.test-fixture)
and
[signed-zero version 257](../benilla-native-geometry-tests/tools/fixtures/m2_vmap_source/signed-zero-m2.m2.test-fixture).
These are original numeric test inputs. The oracle executes the pinned source
methods with only the input-buffer seam replaced; see the separate
[fixture generator](../benilla-native-geometry-tests/tools/generate_m2_vmap_fixture.py).
The Rust writer never generates its own expected oracle outputs.

Additional tests cover every truncated input prefix, invalid profile, index
counts/ranges, nonfinite and unused vertices, signed zeros and subnormals,
source offset overflow, allocation-size arithmetic and both count ceilings.
The independent assembler and `.vmo` reader/query evidence lives in the
geometry test crate; this crate does not assemble or load those products.

There is no CLI, filesystem, archive acquisition, ADT/WMO discovery, instance
transform, map-tree assembly, MMap generation, source lease or gameplay
admission here. Matching this raw serialization is a bounded source
compatibility prerequisite, not evidence of complete production geometry or
playable-world coverage.
