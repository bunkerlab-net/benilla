# Selected-map product framing

`PreparedDiscovery::map_products` builds bounded in-memory tree/tile bytes from
captured selected-map records and rechecks the captured source after all buffers
are complete. `AssembledSpawnBounds::map_products` is the pure framing operation;
it does not re-open source files. Both return no map product for an empty record
stream, matching the original assembler's absent map entry.

These APIs do not publish files, convert model files, validate consumer queries,
or grant gameplay admission. The retained preprocessing identity is diagnostic;
a final unchanged-source check is not a filesystem lock or lasting lease.

## Source format

The source is CMaNGOS Classic revision
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, TileAssembler::convertWorld2 and
ModelSpawn::writeToFile. The model-bound and BIH ports are described separately.

- Unique payloads keep the first complete record for each generated ID
- Unique records and map BIH input slots are ascending by generated ID
- Every tile reference remains, including duplicates and encounter order
- Spawn flags/bounds use the calculated source values; other first-wins bytes,
  including name length and trailing NUL, are unchanged
- Tree framing is magic, isTiled byte, NODE/BIH, then GOBJ and global spawn/slot
  pairs without a global count
- Global references are selected by packed tile key 65/65, independently of flags
- Tile framing is magic, reference count, then spawn/slot pairs
- Slots refer to original ascending-ID input positions, not BIH permutation order
- Tile omission follows first-wins WORLDSPAWN flags. A non-world first entry emits
  the whole group, even later world entries. All-world groups emit no tile
- Leading world entries followed by a non-world entry expose unsafe original
  iterator/count behavior. This adapter explicitly rejects that case
- A non-world group at 65/65 appears in both GOBJ and a tile
- Coordinate presentation preserves original unpacking: high 16 bits for X,
  low eight bits for Y. Captured terrain coordinates are within 0–65

Buffers, including retained unique spawn bytes and discarded tile headers, have a
shared 256 MiB logical byte budget. Geometric Vec growth is checked and bounded,
so byte-vector capacities are at most twice that budget. Existing one-million
record bounds apply to metadata, and output is capped at 4,097 tile buffers.
Allocations are fallible. Cancellation is checked during index construction,
every spawn/reference, BIH construction/serialization and final source recheck.
Failure discards the unpublished result.

## Model-name and consumer compatibility

`spawn_model_names` returns exact first-wins name bytes in ascending spawn-ID
order, with duplicates. The original selected-model set is their case-sensitive
lexicographic deduplication, not all discovery extractions. The separate original
gameobject model export is outside this selected-map API.

For a trailing-NUL name such as `Foo.m2\0`, original conversion appends `.vmo`
before calling fopen, so its effective output basename is `Foo.m2`; plain
`Foo.m2` instead writes `Foo.m2.vmo`. Original loading follows the same operation
and caches the two exact source strings separately. Never pass unchecked embedded
NULs or source-derived paths to a filesystem API.

The current Benilla consumer requires NUL-free `.vmo` product basenames and tiled
exterior maps. Consequently, source-compatible writer output is not automatically
consumer-compatible. A follow-on design should map exact source identity to a
validated bounded internal model ID, preserve effective alias/collision semantics,
and reject ambiguity. Global maps need an explicit typed validation path for
GOBJ state. Neither requires silently disabling current validation checks.

## Independent fixtures

`tools/generate_map_products.py` compiles unchanged pinned assembler and consumer
translation units. Twelve authored cases cover conflicting duplicate payloads,
nonleaf splits on each axis, overlapping bounds, globals, ordinary 65/65 records,
safe mixed flags, absent maps and plain/trailing-NUL names. Rust compares complete
tree, tile and selected model bytes against original output. The harness also
observes real original model lookup names and load success using fixed literals.
It never executes the original unsafe mixed-group case.

Separate regressions cover cancellation at every observed framing poll, aggregate
byte-budget overflow, captured-source changes between bounds completion and final
recheck, and the unsupported mixed-group rejection. These are format/source tests,
not full-world or gameplay tests.
