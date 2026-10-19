# Source-bound selected-tile navigation generation

The private generation path consumes genuine `PreparedTerrain` and
`AssembledSourceMap` values. It compares their shared consumed-source receipts,
validates every terrain product and the complete collision index, and collects the
selected tile through the pinned CMaNGOS `TerrainBuilder` rules. It passes bounded
mesh arrays to a separate `benilla-native-navigation` process. The returned files
must match the requested map parameters and pass the production Detour reader.
These products describe a historical extraction inventory. They do not create a
world session or authorize NPC movement.

## What defines the map

The original `MapBuilder::discoverTiles` scans successfully emitted `.map` and
`.vmtile` files. Accordingly, this path uses the exact terrain products supplied by
the genuine capture plus the complete assembled collision map's tile products.
WDT tile presence does not stand in for a successful extraction.

The union determines the original map origin and maximum tile count. Terrain
collection uses the selected tile followed by the original four cardinal strips;
each neighbor's presence or absence is relative to this immutable inventory. All
successful input bytes, source versions, ordered tile union, neighbor observations,
selected tile, source configuration and off-mesh file identity bind the result.
Adding another extraction product changes the inventory and may change the origin.
Previously generated tiles cannot silently be combined with that new inventory.

Terrain and collision use different archive winner policies. The compatibility
check compares the required shared Map/WDT/selected ADT receipts and all overlapping
history receipts. It retains both independent source versions. It does not claim
whole-install equality. Freshness is checked sequentially before and after work;
it is observational, not an atomic filesystem lease. Terrain's existing freshness
reread is synchronous and is not covered by the helper's process deadline.

## Source configuration and limits

The helper preserves the pinned Recast/Detour algorithms and source generation
parameters. The parent applies the pinned stock JSON defaults, map overrides and
unpadded concatenated tile-key override, including float-to-integer truncation.
The source's stock off-mesh file is bound. A selected tile with a configured
connection is explicitly unsupported until the protocol carries those arrays.
The source empty-output and exactly-625-polygon skip outcomes are preserved.

See the generator crate's protocol and provenance documents for input, output,
allocation and numerical limits. The parent also retains the terrain and collision
readers' existing limits. The selected terrain capture currently contains at most
nine successful terrain tiles. This is not a claim of complete continent extraction.

The helper's tracked native allocation peak excludes separately bounded protocol
buffers, native mesh copies and operating-system overhead. The parent imposes a
hard process deadline and drains bounded output. The trusted packaged helper does
not create children, read game files, access profiles or communicate over a network.
This runner is not a sandbox for arbitrary executables. Platform-specific pipe and
termination behavior is documented beside the process implementation.

## Independent evidence

`tools/native-navigation` compiles the pinned original CMaNGOS extractor,
TerrainBuilder, MapBuilder and Detour query code. Its inputs are authored numeric
ADT/WMO fixtures, not game assets. Tests compare source mesh arrays, generated
map/tile bytes and original query outcomes against those independent captures.
The positive ridge/WMO fixture contains ground and steep-ground polygons and
nontrivial detail geometry. A separate ridge-only fixture exercises the original
exactly-625-polygon skip even though its terrain is not flat.

Measurements from these authored fixtures are not measurements of a real install.
Real-install generation, large-map coverage, additional navigation classes and
live-session admission remain separate verification work.
