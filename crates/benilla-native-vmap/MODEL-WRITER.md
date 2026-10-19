# Raw collision model assembly

`assemble_raw_model` converts one complete captured `VMAPs05` model to the pinned
CMaNGOS `VMAP_7.0` model representation in memory. It neither reads source files
nor publishes products. A caller still needs captured provenance, selected-map
model-name resolution, map/tile assembly and complete consumer validation.

## Source contract

The implementation follows `WorldModel_Raw::Read`, `TileAssembler::convertRawFile`,
`GroupModel::writeToFile` and `WorldModel::writeFile` at CMaNGOS Classic revision
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. Required licensing and notices are in
CMANGOS-NOTICE.txt and the repository license. BIH construction and its independent
source oracle are described in BIH-BUILDER.md.

- Raw group order, group flags, group IDs and authored bounds remain unchanged
- M2 zero group bounds remain zero even when its mesh has nonzero bounds
- Triangle order and winding are unchanged; indices widen from u16 to u32
- Mesh BIHs use triangle bounds in source order, with incoming equal-component
  bits preserved; group BIHs include all groups in physical order
- A zero-vertex group ends after VERT, retaining its group entry and bounds but
  omitting TRIM, MBIH and LIQU, including any liquid carried in the raw model
- A nonempty vertex array with zero triangles keeps an empty mesh BIH and liquid
- A zero-group model ends after WMOD, with no GMOD or GBIH
- WMOD advertises eight bytes but writes a four-byte root ID
- Raw LIQU advertises eight bytes per height while storing four; its header is
  32 bytes including ignored padding. Dimensions determine consumed bytes
- Final LIQU advertises four fewer bytes than it writes, omitting type from its
  size calculation. The signed raw i16 type sign-extends to the serialized u32

## Adapter envelope

Input and final output are each capped at 128 MiB. The model permits at most
4,096 groups and 1,048,576 aggregate branch entries, vertices, triangles, liquid
heights and liquid tiles. A shared 65,536-poll work budget covers parsing loops,
all BIH builders and their serialization. The BIH work loops poll at most every
1,024 work steps; the aggregate budget prevents many groups from each consuming
an independent maximum build budget. Underlying BIH limits also remain active.
Reservations are fallible and bounded. Exhaustion returns an error with no partial
output. Caller cancellation is checked before allocation, through parsing,
construction and serialization, and before returning the complete buffer.

The original raw loader ignores chunk lengths and trailing bytes. This adapter
requires exact captured chunk lengths and EOF, whole index triples, valid indices,
finite ordered bounds, finite vertices/heights/corners, supported liquid flags,
positive consistent liquid grids, and bounded counts. These explicit stricter
checks reject unsafe or non-captured inputs rather than reproducing unchecked
source memory behavior. Ignored raw summary counts, branch values and liquid
padding are not interpreted as authority or geometry.

## Evidence and remaining work

Tests compare complete bytes against 23 independently generated original model
products: two M2 variants, a WMO with a retained empty group and 20 liquid cases.
They include source signed-type and chunk-size quirks. Additional original
raw-loader/assembler edge fixtures cover empty worlds and groups. Malformed
inputs, truncation, shared budget boundaries and cancellation are tested separately.

Complete model bytes alone do not establish a source lease, gameplay admission,
complete map coverage or a populated world. Selecting exactly the model names
used by a map is still the caller's responsibility; discovery also retains unused
extractions, so blindly converting every discovered model would not match the
original assembler's selected output set.
