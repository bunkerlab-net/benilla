# Source BIH construction

`SourceBih::build` constructs a fresh BIH from ordered finite primitive bounds.
`serialize` emits the original little-endian bounds, tree words and final object
permutation. It has no filesystem, map-publication or gameplay-admission authority.

The port follows CMaNGOS Classic revision
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, `src/game/vmap/BIH.h` and `BIH.cpp`.
The upstream implementation credits Christopher Kulla's Sunflow BIH work under
MIT/X11, copyright 2003–2007; the CMaNGOS implementation is GPL-2.0-or-later.
This derived module is distributed under GPL-3.0-or-later with the repository's
required license and notices. G3D Vector3, AABox and g3dmath define the numerical
and ordering contracts.

## Preserved behavior

- Original input order and in-place, unstable center partition
- Center ties stay left; right entries swap with the shrinking right endpoint
- Longest-axis ties prefer Z, then Y, then X
- Root-bound equal components select the incoming primitive's bits; partition
  accumulators retain the existing equal component
- Fuzzy stopping uses widened f32 epsilon and f64 comparison, exactly as G3D does
- BVH2 clip nodes, empty-side wrappers, depth increments and left-before-right
  recursion preserve their source order
- Empty children remain sentinel representations rather than allocated leaves
- Leaf size is 3 for meshes/maps and 1 for groups; callers select it explicitly
- Map tile references use ascending-ID input slots, not positions in the final
  BIH object permutation

A fresh empty build produces a zero box and three-word empty leaf. Rebuilding an
existing C++ BIH as empty can retain its previous box; the pure fresh-only API
intentionally has no such mutable-history operation.

## Explicit limits

Input is limited to 1,048,576 primitives, 12,582,912 tree words and 67,108,864
partition/work steps. Vector reservations are fallible and bounded. Source depth
handling is preserved, including the extra increment for an empty-space wrapper;
there is no invented shallower structural cutoff. Cancellation is checked during
input, work and serialization. Exhaustion returns an error, never a partial tree
or silently enlarged leaf. Serialization is bounded by the retained word/object
caps and checked byte-count arithmetic.

Nonfinite/inverted input boxes and nonfinite extent, midpoint or shrink arithmetic
are rejected. This is a stricter adapter envelope than the original C++ builder.
Signed zero, finite degeneracy, subnormals and harmless underflow are retained.

These producer limits do not replace the existing consumer's independent limits
of 262,144 words/objects and structural depth policy. A built BIH is not proof
that a collision map has passed consumer validation.

## Independent verification

The generator compiles unchanged pinned BIH.cpp and G3D implementation units.
Its only replacement header supplies fixed-width integer aliases. Authored bounds
are passed to the original template builder and original serializer; Rust never
generates expected output. `--check` verifies the checked-in numeric corpus.

The 199 cases cover signed zero, coincident and zero-width boxes, center/axis ties,
subnormal and fuzzy-adjacent values, extreme finite coordinates, random boxes and
separated clusters. The corpus exercises BVH2 nodes, both empty-child wrapper
orientations and leaves larger than the requested leaf size. It does not claim
exhaustive proof of every depth-64 corner or all possible finite float patterns.

Additional Rust tests rebuild sections from existing original complete products:
M2/signed-zero M2 meshes, a WMO with a retained empty group, a four-group stack,
and three maps including a five-spawn subdivided map. Mesh, group and map BIH
bytes match independently produced original bytes. Bounds-only input does not
replace complete raw-model conversion, VMO framing, tile publication or full
world-query validation.
