# Bounded WMO liquid raw production

This GPL-3.0-or-later source port uses CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, `WMOGroup::open` and
`ConvertToVMAPGroupWmo` in `contrib/vmap_extractor/vmapextract/wmo.cpp`.
The complete-group and collision-mask contract in [WMO provenance](WMO-PROVENANCE.md)
still applies. No game-install assets are included.

## Defined source behavior

`extract_wmo_collision` retains its dry-only contract. The separate
`extract_wmo_collision_from_source` takes the witnessed root lookup member after source basename normalization;
acquisition supplies this from its immutable dependency capture, preserving directory
spelling rather than passing the original listfile spelling. Its spelling is
needed for the source's case-sensitive `Stratholme_raid` type override. A filename
argument alone is not proof of source identity or a gameplay lease.

Every physical group chunk resets liquid state. A final MLIQ emits LIQU;
a following MODR or other chunk clears it. Repeated MLIQ replaces earlier state.
Skipped groups emit no liquid. All such chunks are validated before filtering,
including reset or skipped chunks. Existing source chunk order is preserved.

The input contains a 30-byte header, eight bytes per vertex and one flag per tile.
Output has a 32-byte ABI header, four bytes per height and unchanged tile flags.
The advertised LIQU block size still counts eight bytes per vertex, as upstream;
the original assembler consumes dimensions instead of trusting that size.
Type selection preserves root flag precedence, unsigned wrapping increment,
first non-15 low-nibble fallback, group ocean flag, and exact filename override.
The final signed-short field keeps its low 16 bits; the original assembler's
signed promotion is covered by boundary fixtures.

## Named compatibility rule

`wmo-liquid-header-zero-padding-v1` writes zero only to header offsets 30–31.
Both pinned extractor and assembler declarations have size 32/alignment 4, with
the signed 16-bit type at offset 28. These final two bytes are ABI padding and
are not read as a field. The original extractor leaves them uninitialized.
A controlled two-byte source patch makes the authored oracle deterministic;
changing these bytes to a different pattern produces identical assembled output.
This explicitly versioned rule does not claim equality with undefined bytes.
All defined fields, advertised size and source ordering remain unchanged.

## Safety envelope and evidence

Both tile dimensions must be positive and each vertex dimension must equal its
tile dimension plus one. This prevents the original assembler's allocated grid
and copied height arrays from disagreeing. Signed source arithmetic, exact chunk
length, finite positions/heights, and at most 1,048,576 vertices and tile flags
are checked. Existing per-file, aggregate-input and raw-output caps remain.
Zero-dimension and malformed/reset/skipped input is rejected without a partial
product; it is never guessed or replaced with zero-height water.

The [guarded authored oracle](../benilla-native-geometry-tests/tools/wmo_liquid_generate.py)
executes the pinned extractor, assembler and manager with the explicit padding
patch. Fixture inputs and products are authored numerical geometry. The actual
Rust liquid consumer is tested against those manager results and raw writer
bytes, including type masks, cell rejection, bounds, nonfinite queries and
identity mismatches. Acquisition tests exercise real authored MPQs, active/reset
liquid counts and changed-input rejection.

This adds complete raw model dependencies. Final transformed doodad placement,
global IDs, complete map-wide BIHs and production geometry admission remain
separate prerequisites. It does not expand wet movement support or establish
real-install WMO acceptance. The independent listfile oracle remains unverified
and is not part of this evidence.
