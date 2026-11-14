# Captured dry WMO source oracle

SPDX-License-Identifier: GPL-3.0-or-later

These are entirely authored numeric inputs and unchanged original CMaNGOS
outputs, with no game-install assets. The guarded source pin is `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`.
Regenerate using `python3 crates/benilla-native-geometry-tests/tools/generate_wmo_vmap_fixture.py /path/to/cmangos-source`;
add `--check` to re-execute and compare every byte. The existing six-group oracle
is generated and checked separately without changing its fixture bytes.

- `hull.root.test-fixture`: authored version-17 root, one group, root ID 5875,
  no doodads or liquid, bounds [-1,-2,-3] through [4,5,6]
- `hull_000.group.test-fixture`: authored group ID 23, flags 0x2000, vertices
  [-1,-2,-3], [4,0,6], [0,5,1], indices [0,1,2], one collision MOPY entry,
  one MOBA record with word 8 equal to 1, matching bounds
- `hull.modf.test-fixture`: full 64-byte authored MODF including name index 0,
  client ID 0x89abcdef, position [16100,4,16600], zero rotation, bounds
  [16098,1,16599] through [16105,10,16604], flags/doodad-set 0, name-set 11,
  scale field 1024; the original ADT caller's four-byte name-index read is
  represented by seeking to offset 4 before the original WMOInstance constructor
- `hull.raw.vmap`: actual original ExtractSingleWmo output; offset 8 is its
  collision triangle count 1, not an authored replacement or vertex count 3
- `dir_bin`: actual original WMOInstance output for map 0, tile 30,31;
  GenerateUniqueObjectId starts with an empty allocator and allocates ID 1;
  original internal position [16600,16100,4], scale 1 and plain `Hull.wmo`
- `source-fragments.cpp.test-fixture`: exact hash-guarded original methods
  compiled by the harness, including the placement constructor and allocator
- `provenance.tsv`: pin, execution profile, original-source and captured-byte hashes

The only input substitution is the closed authored MPQFile acquisition map.
The no-doodad guard fails if model extraction is unexpectedly attempted. No
source behavior, ID allocator, model filename, placement, or raw output is patched.
`generate_map_products.py` first verifies this capture by re-executing the original
source, then supplies its raw and directory bytes to unchanged original
TileAssembler and public VMapManager2 queries. Its `captured_wmo` row in
`map-products.tsv` contains all three assembled products; the corresponding 66
`map-queries.tsv` rows cover loading, coordinates, heights, LOS and object hits.
The public load uses tile 31,30 because the original tile filename helper
transposes the dir_bin tile coordinates. Only the natural plain model lookup is
used for this capture; the older shortened-name fixtures are independent.
