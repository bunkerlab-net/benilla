# Exact-source bounded VMap height oracle

This authored oracle tests only the loaded, height-enabled static VMap query. It
does not establish composite TerrainInfo selection, liquid, body support, actual
installation geometry, source authenticity, or production-session authority.

Regenerate offline using the pinned CMaNGOS Classic source:

```sh
python3 crates/benilla-native-geometry-tests/tools/scopedheight_generate.py /path/to/mangos-classic
python3 crates/benilla-native-geometry-tests/tools/scopedheight_generate.py /path/to/mangos-classic --check
```

The exact pin is `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. The generator reuses
the full-file guards and original native geometry compilation described in the
[authored VMap fixture contract](fixtures/vmap_source/README.md), with two further
whole-file guards in [scopedheight_source_hashes.sha256](scopedheight_source_hashes.sha256).
`StaticMapTree::getHeight`, `VMapManager2::GetMapTree`,
`VMapManager2::getHeight`, and `IVMapManager::isHeightCalcEnabled` are extracted
verbatim from those checked sources. The minimal manager declaration replaces
unrelated ownership/loading machinery with the same map-pointer lookup storage
populated by the harness, and an enabled height flag. It does not replace the
height, conversion, lookup, ray, intersection, or serialization algorithms.

The source constructs and serializes the same five instances and four authored
models as the existing ray oracle. Every run compares all six generated geometry
products with the existing checked-in bytes and fails on any difference. No
existing fixture product is rewritten, and no native parser reads fixture data.
The Rust test loads those actual VMap bytes through its checked product decoder.

The additional [scopedheight.tsv](fixtures/vmap_source/scopedheight.tsv) contains
1,018 bit-exact height queries. Columns are the case name, world x/y/z, signed
search distance, source-converted internal x/y/z, and final manager height.
Binary32 values use eight hexadecimal digits. Cases cover identity and rotated
ramps, stacked floors, M2 inclusion, source world-coordinate rounding, vertical
wall misses, nearest hits in either direction, exact origin and search endpoints,
adjacent endpoint floats, signed zero, subnormal search distances, and 10,000-unit
searches that exceed the separate 4,096-unit segment API cap.

The Rust prerequisite accepts finite world coordinates within ±100,000 and
finite signed search distances within ±10,000. Each actual probe is checked;
finite generated probes outside either bound return `Unsupported`, rather than
an invented no-height result. This is deliberately narrower than all possible
TerrainInfo-generated ranges. Height rays use exact `(0,0,-1)` or `(0,0,1)`,
including downward treatment of negative zero, with `ignoreM2Model=false`.
No normalization, endpoint-derived approximation, or segment-length restriction
is used. Misses return the source `VMAP_INVALID_HEIGHT_VALUE` (-200,000).

A shared query budget retains the existing 32,768-step cap and one absolute
deadline across composed probes. Each probe and its completion check that same
deadline. Exhaustion and unresolved candidates remain errors. The separately
bound request/reply echoes the complete snapshot binding, ticket, pose bits and
signed search bits; these are consistency declarations, not live authority.

Original CMaNGOS and G3D notices are retained through the existing
[fixture licensing record](fixtures/vmap_source/README.md). The authored harness,
generator, tests and bounded Rust port are GPL-3.0-or-later. Regeneration was
verified on Linux x86-64 with C++17, no fast math, no contraction and
round-to-nearest; cross-platform and real-asset validation remain separate.
