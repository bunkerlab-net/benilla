# First-wins map spawn indexing

`PlacementRecords::map_spawn_index` reproduces the container behavior of pinned
CMaNGOS `TileAssembler::readMapSpawns` for one selected map. It accepts only the
private, captured record product. It does not accept arbitrary file paths or
publish collision maps.

Unique spawns are sorted by unsigned generated ID. The first record encountered
for each ID wins, even if a later record has different placement fields. Every
tile reference remains present, including duplicates. Tile keys use the original
`x << 16 | y` expression. Equal tile keys retain encounter order, matching the
original C++ multimap. Consumers must resolve each tile reference through its
unique spawn rather than use a later duplicate's placement fields.

The index borrows the original stream and its source binding. Record views still
include their original map/tile headers. Those views are extractor records, not
assembled model spawns: M2 bounds have not been calculated, and global WMO bounds
have not received the assembler's coordinate adjustment.

## Bounds and cancellation

At most 1,048,576 records can be indexed. Three vectors reserve exactly that
record count or less. Two retain indices; the temporary third vector is dropped
before return. Each entry uses two u32 words and two usize offsets. Allocator
bookkeeping is additional; the input stream has its own separate byte limit.
Four stable byte-radix passes provide unsigned ordering without comparison-sort
allocations. Cancellation is checked on each parsed record and every 1,024
entries during sorting. A failed operation returns no partial index.

## Evidence and scope

The numeric test rows exercise changed-payload duplicates, repeated tile
references, global tile 65/65 and interleaved IDs. The independent Python probe
hash-checks the original TileAssembler.cpp and runs its unchanged readMapSpawns
function. Only ModelSpawn parsing is stubbed with two authored numeric words;
this verifies original container control flow, not binary parsing or geometry.
The Rust test uses the same expected container results. A separate authored MPQ
integration checks mixed parents, multiple children and cross-tile duplicates.

Remaining dependencies for a complete collision-map output are:

1. Calculate transformed M2 vertex bounds and the original global WMO adjustment
2. Convert retained raw models into source-faithful model/group geometry and BIHs
3. Build the map BIH in ascending unique-ID order and retain its exact primitive slots
4. Serialize global objects and every tile reference with the matching primitive index
5. Compare complete authored maps and collision queries against the original assembler
6. Publish a fully rechecked output atomically, then separately integrate gameplay admission

This index does not make a populated world or NPC movement available.
