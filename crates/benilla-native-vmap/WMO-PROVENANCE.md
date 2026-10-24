# Bounded dry WMO raw collision extraction

This adapter ports the raw geometry projection of `WMORoot::open`,
`WMORoot::ConvertToVMAPRootWmo`, `WMOGroup::open`, `WMOGroup::ShouldSkip`,
`WMOGroup::ConvertToVMAPGroupWmo` and the ordered group loop in
`ExtractSingleWmo` from
[CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f](https://github.com/cmangos/mangos-classic/tree/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/contrib/vmap_extractor/vmapextract).
The original `wmo.cpp`, `wmo.h` and `vmapexport.cpp` hashes are in
[SOURCE-SHA256SUMS](SOURCE-SHA256SUMS). The unchanged upstream
[notice](CMANGOS-NOTICE.txt), authorship and compatible later-license terms
are described in [the crate provenance](PROVENANCE.md).

## Source geometry contract

The source's default small-VMap mode retains a triangle when its material
flags contain collision `0x08`, or render `0x20` without detail `0x04`.
Selected triangles retain their order and winding. Only referenced vertices
are emitted, in ascending original vertex-index order. This differs from a
first-use remap and from the renderer's material filtering. No coordinate
conversion, welding, normal repair or replacement bounds are applied.

Every root-declared group must be supplied in its original index order,
including groups that will be skipped. The source opens each group before
checking unreachable `0x80`, antiportal `0x04000000`, then the exact
`antiportal` group name. A missing declared group invalidates the complete
result rather than producing a partial model. Raw group flags, IDs and bounds
remain source values. `GRP ` branch values come from each 24-byte `MOBA`
record's ninth little-endian `u16`.

The root output's word at offset 8 is the sum of the selected **triangle**
counts returned by group conversion, despite the original variable/comment
calling it a vertex count. Offset 12 is the number of groups left after source
skip predicates. Output is explicitly little-endian `VMAPs05` serialization.

The source treats the first 68 bytes of `MOGP` as its group header and walks
its nested chunks afterward. It also resets liquid output state at every
chunk. This dry adapter rejects any `MLIQ` chunk rather than guessing heights,
relying on later chunk resets, or inferring a liquid from an unrelated header
field. The separate source-member API now supports [validated liquid production](WMO-LIQUID.md)
with its own source-order, ABI-padding and consumer proof.

## Scope of the result

This interface produces one raw WMO geometry object from immutable bytes. It
does not reproduce the original root loader's `MODN` model-extraction side
effects or its `MODS`/`MODD`/`MODR` doodad placement/reference collection.
Ignoring those fields for this raw geometry projection does not establish a
complete WMO dependency or placement closure. In particular, the result is
not a complete building collision scene if its placed doodads are absent.

Archive patch winners, path normalization, source leases, per-map encounter
IDs, world-wide placement collection, map-tree assembly, MMap generation and
production gameplay admission remain separate prerequisites. A selected tile
cannot stand in for the map-wide placement BIH: omitted placements must not
become invented clear collision space. No game-install bytes are bundled.

## Deliberate safety envelope

The original extractor performs unchecked reads and allocations; the following
are adapter restrictions, not claims about upstream validation. A leading
`MVER` version-17 chunk, complete chunk framing, singleton consumed chunks,
a 64-byte `MOHD` and a
68-byte `MOGP` header are required. Nested `MOGP` extents must fit the input.
Header-only `MOGP` followed by sibling geometry preserves the source's flattened
walk. Material/index/vertex/batch arrays must contain whole records, triangle
and index counts must match, indices must be in range, vertices must be finite,
and root/group bounds must be finite and ordered. Skipped groups receive these
same structural checks before source skip predicates run.

The name predicate preserves flag short-circuiting. An unskipped negative name
offset is rejected instead of reproducing undefined pointer arithmetic. The
bounded name comparison reads only through the first mismatch or the
`antiportal` terminator. A read that would cross `MOGN` is rejected; an offset
at or beyond its end means no name match, as in the defined source branch.
Consumed duplicate chunks are
rejected rather than adopting source allocation leaks or last-write behavior.

Each file is limited to 64 MiB; all supplied bytes together to 256 MiB, counting
repeated slices repeatedly; groups to 1,024; vertices and triangles per group
to 65,536; MOBA records and chunks per file to 65,536; complete raw output to
128 MiB. Checked allocations and integer arithmetic bound input processing and
output size. These are resource ceilings, not a hard wall-clock guarantee.
Requiring leading `MVER` also avoids the upstream group parser's first-iteration
`strcmp` against `MOGP` before initializing its fifth character. The adapter
does not assign deterministic behavior to that undefined input ordering.
No partial output is returned when any group fails. Finite vertices outside
authored bounds are not repaired or rejected by this raw writer; the actual
VMap consumer independently enforces its containment contract.

## Independent source evidence

The [original-source oracle](../benilla-native-geometry-tests/tools/generate_wmo_vmap_fixture.py)
starts from authored version-17 root/group bytes and executes unchanged pinned
root/group methods and the complete `ExtractSingleWmo` loop. Its six groups
exercise every material byte (all 256 combinations), the three skip predicates,
a retained empty group, and a remap whose first-use order differs from ascending
original indices. The original extractor also proves that a missing group,
even one that would be skipped, deletes the partial output.

The Rust producer compares its complete 7,472-byte result to the independently
generated raw file: six declared groups, three retained groups, 192 triangles,
and root ID 4242. The oracle then runs the original assembler and reader on
these raw bytes. Query fixtures include 2,208 rays, 540 heights and 540 location
queries. These authored tests establish the named component contract; they do
not establish real-install WMO coverage or a complete map.
