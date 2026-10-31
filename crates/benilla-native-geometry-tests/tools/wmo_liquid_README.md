# Authored WMO liquid extraction and original-consumer oracle

This offline fixture proves the bounded authored chain from native WMO `MLIQ`
bytes through the pinned CMaNGOS Classic extractor and complete raw assembler to
original liquid queries. Source is pinned to
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. No installation bytes, database rows,
renderer geometry, downloaded executable or full world-compiler output is used.
This is not installation compatibility, complete world production or gameplay
admission evidence.

## Reproduce

```sh
python3 crates/benilla-native-geometry-tests/tools/wmo_liquid_generate.py /path/to/mangos-classic
python3 crates/benilla-native-geometry-tests/tools/wmo_liquid_generate.py /path/to/mangos-classic --check --sanitize
CMANGOS_SOURCE=/path/to/mangos-classic python3 -m unittest discover -s crates/benilla-native-geometry-tests/tools -p wmo_liquid_test.py
```

`--check` regenerates in an isolated temporary directory and compares the complete
fixture filename set and every byte without rewriting checked fixtures.
`--sanitize` additionally traps undefined original floating-point-to-integer
conversions. Original parsers receive only structurally valid authored inputs;
malformed-input rejection belongs to the Rust safety-boundary tests.

The existing [VMap source guard](vmap_source_hashes.sha256) and
[dry WMO source guard](wmo_vmap_source_hashes.sha256) protect all reused original
implementations/declarations. The additional
[liquid source guard](wmo_liquid_source_hashes.sha256) protects the included
Classic liquid-mask declarations. Guards run before compilation. No existing dry
fixture or oracle implementation is modified. The generator reuses its guarded
source-selection utilities only.

The verified profile is Linux x86-64, GCC 14.2.0, little-endian IEEE binary32,
round-to-nearest, C++17, `-O0`, no fast math, no floating-point contraction and no
strict aliasing. Platform-specific ABI and signed narrowing results are tested
explicitly; no cross-platform claim is made.

## The named padding policy

`WMO_LIQUID_HEADER_ZERO_PADDING_V1` initializes only the two ABI tail-padding bytes
of `WMOLiquidHeader`. This is an explicit deterministic policy, not an assertion
that the unpatched extractor emitted deterministic padding.

[layout.tsv](fixtures/wmo_liquid_source/layout.tsv) is measured from both the
original extractor declaration and the assembler's separate original declaration:

- Size 32, alignment 4
- Four integer dimensions at offsets 0, 4, 8, 12
- Three binary32 coordinates at offsets 16, 20, 24
- Signed short type at offset 28, width 2
- No declared field occupies bytes 30–31

Original `WMOGroup::open` allocates with `new WMOLiquidHeader` and reads only
`0x1E` bytes. Original conversion overwrites `type` and writes the entire 32-byte
object. The two unread bytes therefore have no source-defined value.
[padding-v1.patch.test-fixture](fixtures/wmo_liquid_source/padding-v1.patch.test-fixture)
adds exactly two unsigned-byte assignments after allocation. It changes no field,
condition, size, chunk order or serialization algorithm. The generator applies
this patch only to its temporary source fragment, never to the supplied source
checkout. [provenance.tsv](fixtures/wmo_liquid_source/provenance.tsv) records the
original source hash, exact patch hash and patched-source hash.

The harness statically asserts layout before the two assignments can execute.
It also mutates the resulting raw Water header padding to `a5 5a`, runs the
unchanged original assembler again, and asserts a byte-identical `.vmo` output.
[padding.tsv](fixtures/wmo_liquid_source/padding.tsv) records this consumer-
independence result. Equality with these raw fixtures means equality under the
named padding policy; it must never be described as exact equality with the
unpatched extractor's unspecified padding bytes.

## Original behavior retained

- `WMORoot`, `WMOGroup`, complete `ExtractSingleWmo` orchestration and original
  filename helpers execute the guarded source, with only the named padding patch
- Original `TileAssembler.cpp`, `WorldModel.cpp`, `BIH.cpp`, `ModelInstance.cpp`
  and the same G3D translation units as the dry oracle are compiled completely
- The assembler translation unit is included unchanged with an appended read-only
  layout bridge. Its liquid reader is not replaced
- Acquisition uses the original `MPQFile` declarations with a closed map of
  authored input files. No decompression or installation lookup is simulated
- No doodads are authored. `ExtractSingleModel` throws if unexpectedly called
- Original map-location callbacks, `GetLocationInfo`, `GetLiquidMask`,
  `VMapManager2::GetLiquidLevel`, model-instance liquid queries and group/liquid
  interpolation supply every expected query result
- Manager ownership is a bounded map binding and original products are loaded by
  original readers. Access changes allow binding and inspection only

Every parsed WMO chunk resets `LiquEx_size` and `liquflags`. `Reset` therefore
contains valid `MLIQ` followed by an empty `MOTV`, and produces a dry raw group.
Water has a wet group followed by a separate dry upper group, so liquid cursor
length and group boundaries are exercised together.

The raw `LIQU` length advertises `sizeof(header) + 8 * vertex_count + tile_count`,
but actual output contains only four bytes per height. These fixtures advertise
108 bytes and emit 72. Original assembly ignores that declared size and consumes
32 header bytes, nine four-byte heights and four tile flags. It immediately reads
the next Water group successfully. The fixtures retain this discrepancy. Original
assembled liquid serialization likewise retains its existing four-byte
`GetFileSize` omission for the type field; no advertised length is repaired.

## Authored cases and outputs

All 91 files are under [fixtures/wmo_liquid_source](fixtures/wmo_liquid_source).
[Cases](fixtures/wmo_liquid_source/cases.tsv) include explicit source member names,
input/output relative paths, flags/types and selected-group/triangle/liquid counts.
There are 20 authored roots, 21 source groups, 20 raw files and 20 assembled model
files, plus one map tree and tile containing 21 placements.

The wet groups have a two-by-two tile liquid, three-by-three heights, corner
`(2,3,99)` and heights `2,6,10,3,9,5,11,4,7`. Their tile bytes are
`0f,12,a1,83`, preserving both disabled low nibbles and high nibble bits. The
`Empty` case uses all `ff` flags. Source header type `0x6abc` is deliberately
replaced by original type inference; unknown per-vertex words remain in authored
inputs and are correctly absent from raw output. Every dimension satisfies
`xverts = xtiles + 1` and `yverts = ytiles + 1`.

Cases exercise root-flag versus legacy type precedence, first enabled tile scan,
all-disabled fallback, ocean group bit `0x80000`, exact case-sensitive
`Stratholme_raid` path matching, types 41 and 22, signed-short promotion for
32768/65535, narrowing 65536/65537, and legacy unsigned `UINT32_MAX + 1` wrapping
before fallback. Narrowed bits are preserved; original assembler promotion of
negative short types to uint32 is recorded explicitly.

Collision floors cover `[0,16]` by `[0,16]` at z=1. Water's second, dry floor is at
z=5 over `[4,8]` by `[4,8]`. Normal placements translate X by `40 * case_index`.
A second Water placement has translation `(0,100,4)`, rotation `(13,37,-9)` degrees
and scale 1.75, all transformed using original source arithmetic.

## Query schema and coverage

[liquid.tsv](fixtures/wmo_liquid_source/liquid.tsv) has the existing scoped-liquid
oracle's exact 23-column schema. Floats are eight hexadecimal binary32 digits;
IDs, masks and booleans are decimal. Caller sentinels are level 123.25, floor
-100000 and type `0x12345678`.

Columns are dataset, case, public world xyz, actual roundtripped internal xyz,
location hit, ground, spawn ID, root ID, group ID, group flags, ADT ID, requested
mask, liquid success, level, floor after call, type after call, direct instance
success, direct instance level, and whether that instance call was evaluated.

The 2,205 original manager calls contain 1,904 location hits and 342 successful
liquid results. Cases cover both triangle interpolation branches and diagonal,
disabled cells, high flag bits, inside-collision/outside-liquid points, upper
liquid boundaries, no floor, dry-over-wet selection, wrong mask, zero mask,
unknown/narrowed types, signed zero and transformed placements. Every sample runs
with masks 0, 1, 2, 4, 8, 15 and 16. The query coordinates are roundtripped through
the original public coordinate conversion before recording expectations.

[direct.tsv](fixtures/wmo_liquid_source/direct.tsv) adds 64 original group-liquid
samples with 48 successes, using the Water model read from the original assembled
product. Columns are model, case, local xyz, success, level and type. No direct
`WmoLiquid` construction supplies any fixture.

## Notices

Original GPL-2.0-or-later notices remain in the generated source fragment and the
existing [CMaNGOS notice](../../benilla-app/src/net/p2p/world/native_geometry/products/CMANGOS-NOTICE.txt).
The authored harness, generator and tests are GPL-3.0-or-later. G3D's Modified BSD
notice remains in [vmap_G3D_LICENSE.cpp.txt](vmap_G3D_LICENSE.cpp.txt); guarded BIH
source retains its Sunflow MIT/X11 attribution.
