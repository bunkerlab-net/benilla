# Exact-source loaded direct-corridor observer

This test-only observer builds the complete, unchanged original Detour translation
units from CMaNGOS revision `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. It calls
original `dtNavMeshQuery::findPath` on actually loaded, authored numeric ground
meshes. It neither links nor calls the production bridge or Rust implementation.
No game assets, world databases, fake loaded references, substituted query results,
or rewritten original algorithm bodies are involved.

## Source boundary and provenance

`MoveMap.cpp:356` (`MMapManager::loadMapInstance`) establishes the source map query
pool: `query->init(mmap->navMesh, 1024)` at line 369. The hash-verified complete
method is included as provenance, not compiled as a replacement owner/importer.
The direct source method is `DetourNavMeshQuery.cpp:973`; path extraction is
`getPathToNode` at line 1167. Zero-option sliced controls compile the same original
file, using `initSlicedFindPath:1216`, `updateSlicedFindPath:1280`, and
`finalizeSlicedFindPath:1497`.

`direct_corridor_source_hashes.sha256` binds 21 complete original files: every
compiled Detour source/header, the map-pool source, both PathFinder inputs, and both
original licenses. Hashing normalizes CRLF to LF and nothing else. The generator
rejects incomplete, duplicate, malformed, or unexpected manifest entries.
`direct_corridor_source_fragments.test-fixture` preserves the complete unchanged
methods, original copyright notices, and licenses. The test package is
GPL-3.0-or-later; original Detour retains its zlib-style license and notices.

The existing independently authored two-tile mesh is reused byte-for-byte and
bound by `direct_corridor_input_hashes.sha256`. Original `dtCreateNavMeshData`
builds the new products, which are then read back and loaded using original
`dtNavMesh::addTile`. Every returned corridor reference is checked against that
loaded mesh. All new products use `.test-fixture`, never `.mmap` or `.mmtile`.

Each observation uses original endpoints for cost evaluation, fresh direct and
sliced query instances, ground flags include=1/exclude=0, and unchanged default
unit area costs. Fresh original nearest-poly queries with five-unit extents are a
bounded endpoint preparation dependency; cold `PathFinder::getPolyByLocation` is
excluded. Same-polygon direct queries return before clearing the node pool, so
fresh query instances deliberately avoid history-dependent diagnostic counts.

This establishes the loaded direct-corridor dependency only. It does not establish
owner routing, cold lookup, smoothing, cached corridor reuse, optimization,
normalization, PLAYERBOTS, off-mesh traversal, owner/body/movement admission, an
MMap compiler/importer, or gameplay. In particular, the ground filter's same-parent
sliced skip is present in the source package but has no separately claimed loaded
behavioral witness here. Source closed-node reopens are retained by the production
port; this corpus does not claim a measured reopen event.

## Actual direct-versus-sliced witness

Four flat full-tile quads form a square. With native X/Z labels A=(0,0), B=(1,0),
C=(0,1), D=(1,1), and tile width L=533.3333, the authored original endpoints are
(-L+0.40L, 0, -L+0.90L) and (-L+1.95L, 0, -L+1.05L), in native XYZ order.
The fixture records their exact binary32 values in world XYZ order.

On the same loaded bytes, both algorithms use 1024 nodes and capacity 74, and both
return exact `DT_SUCCESS` with three references. Original direct returns A,B,D;
zero-option sliced returns A,C,D. Direct allocates five nodes, two with nonzero
crossSide state; sliced allocates four. The distinct route is therefore not a
capacity or pool-size artifact. Original direct keys neighbors by
`(polygonRef, side >> 1)` at external links; sliced always uses state zero.
The reverse control also observes three nonzero-state direct nodes.

## Corpus and format

There are 22 tab-delimited rows. The header names every field. Coordinates are
world XYZ comma-separated eight-digit hexadecimal binary32 words. Status words
are hexadecimal; polygon references and counts are decimal. Corridors are
comma-separated references. `nodes` is the selected query pool ceiling;
`direct_nodes` and `sliced_nodes` are actual original pool allocations.
`direct_state_nodes` counts allocated nodes whose state is nonzero.
`sliced_iterations` is the sum of real one-iteration updates. It is not a direct
expansion count; unchanged original direct does not expose that statistic.

Geometry loading order is part of reference identity:

- `existing`: existing `authored.mmap`, then `0003232.mmtile` at grid [32,32] and
  `0003231.mmtile` at [32,31]
- `grid`: new `params.test-fixture`, then `grid00.test-fixture` [32,32],
  `grid10.test-fixture` [32,31], `grid01.test-fixture` [31,32], and
  `grid11.test-fixture` [31,31]
- `long600` or `long1100`: new parameters and the corresponding
  `long600.test-fixture` or `long1100.test-fixture` at [32,32]

Controls include:

- Same polygon with original unsnapped vertical coordinates and zero pool nodes
- Two polygons, cross-tile slopes, reverse crossings, and disconnected partial
- Exact output capacities 2, 3, 74, 600, and 1024: direct succeeds; original sliced
  incorrectly adds `DT_BUFFER_TOO_SMALL` when its write count equals capacity
- Proper truncation at one, two, and 74 output slots
- A 600-polygon chain distinguishes the source 1024 pool from the old 512 pool:
  with 74 output slots, statuses are respectively `0x40000010` (buffer) and
  `0x40000070` (buffer, partial, out-of-nodes)
- A reachable target consuming exactly 1024 nodes succeeds, distinguishing full
  capacity from actual exhaustion
- A 1100-polygon chain really exhausts 1024 nodes. Capacity 74 produces
  `0x40000070`; capacity 1024 produces direct `0x40000060` (partial, out-of-nodes)
  while sliced additionally reports buffer truncation at its exact result length

Rows with 512 nodes are explicit historical-pool controls, not the new source
stage. The public source dependency has point/corridor limits 2..74. Private raw
bridge tests may compare capacities 1..1024. Capacity 1100 is an original-query
control outside that bridge's bounded output domain.

## Numerical and object-lifetime profiles

Required profile: GCC 14.2.0, `x86_64-linux-gnu`, C++20, little endian, IEEE-754
binary32, x86-64 baseline/no FMA, no fast math, no FP contraction, no excess
precision, round-to-nearest, and no flush-to-zero/DAZ. Missing or mismatched tools
fail rather than skip. A `-fno-inline` build and `nm` verify the original query and
mesh writer symbols and reject production bridge/source-port symbols.

Original tile serialization aligns sections to four bytes although `dtLink`
requires eight. The loader retains every serialized byte and shifts its allocated
base by four only when needed to align links. Other section alignments are checked
at compile time. Tiles are explicitly removed before their backing storage is
freed.

The default unchanged-original lane has the known C++20 tile-object raw-storage
lifetime caveat: upstream `dtMeshTile` declares a private copy constructor and no
default constructor, while its allocator treats raw storage as tile objects.
These are empirical exact-source observations under the stated compiler profile.
Clean UBSan does not detect or resolve this caveat. It is separate from serialized
link-section alignment.

The separately labelled `--repair-tile-lifetime` companion first verifies the same
original hashes and then inserts only `dtMeshTile() = default;` into a temporary
copy of the original header. It changes no field, layout, algorithm, original
method body, or serialized byte. Both lanes compare all 15 member offsets,
104-byte size, eight-byte alignment, standard layout and noncopyability against an
independently renamed original declaration. The repair lane additionally requires
trivial default construction and destruction. This closes that specific lifetime
issue only; neither lane claims a proof that all original C++20 behavior is defined.
Normal and UBSan builds in both lanes must reproduce every fixture byte identically.

## Reproduce

From the repository root, with the pinned checkout as SOURCE:

```sh
python3 crates/benilla-native-nav/tools/direct_corridor_generate.py SOURCE --check --receipt-dir RECEIPTS/normal
python3 crates/benilla-native-nav/tools/direct_corridor_generate.py SOURCE --check --sanitize --receipt-dir RECEIPTS/ubsan
python3 crates/benilla-native-nav/tools/direct_corridor_generate.py SOURCE --check --repair-tile-lifetime --receipt-dir RECEIPTS/lifetime-normal
python3 crates/benilla-native-nav/tools/direct_corridor_generate.py SOURCE --check --repair-tile-lifetime --sanitize --receipt-dir RECEIPTS/lifetime-ubsan
python3 crates/benilla-native-nav/tools/direct_corridor_test_generate.py SOURCE
```

Without `--check`, only the named source package and eight `.test-fixture` products
are rewritten. Check mode never repairs drift. Receipts record compiler/target,
exact commands, full harness, observed tile header and digest, symbols, manifest
digests and output hashes. UBSan instruments all original Detour translation units
and the harness with undefined/float-cast-overflow and no recovery.

The guard suite tests source/fixture drift, full normalized-LF provenance, missing
inputs, malformed manifests and observation rows, source locations, markers,
compiler/symbol failures, bridge exclusion, exact-constructor-only repair,
behavior controls, safe output names, and actual `core.autocrlf=true` Git checkout
hash preservation. The temporary checkout uses an index only and creates no
commit. `.gitattributes` pins oracle text to LF and generated binary products to
`-text`; the textual case file explicitly overrides the binary product pattern.
The generator and its guards do not run Cargo, an app, or gameplay.
