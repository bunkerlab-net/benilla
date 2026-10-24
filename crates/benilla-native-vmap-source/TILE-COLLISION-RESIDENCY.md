# Tile-resident collision snapshots

The private application `PreparedTileCollision` view accepts only a genuine assembled source map and an explicit list of terrain-grid tiles. It validates the complete map placement index, exact model dependency closure, shared placement identities, transforms, classifications, BIH structure and enclosing placement bounds before decoding selected geometry. Source ADT tile coordinates are reversed when represented as terrain-grid `TileId` coordinates.

Selected tiles share spawn slots and model geometry. A placement referenced by either selected tile is resident once. Empty selections and absent empty tiles are valid; duplicate selections are rejected. Changing residency constructs a new immutable view. Reconstructing the same selection from the same source yields the same binding; no mutable eviction API is exposed.

Queries retain the existing production decoder and its failure behavior. Reaching an unloaded slot returns `Missing`, never a clear ray or a height miss. A coarse BIH leaf can conservatively reach an unloaded slot even when that placement's own bounds would miss the query. A later refinement could use the already validated placement bounds to avoid that unnecessary refusal. This checkpoint does not implement that refinement.

The parallel-axis regression follows pinned CMaNGOS revision `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, `src/game/vmap/BIH.h:122–152`: root clipping runs only when `G3D::fuzzyNe(dir[i], 0.0f)`. Moving a vertical ray sideways therefore does not establish root rejection on that parallel axis. The regression separately tests a nonparallel z interval that ends before the root bounds. Returning `Missing` on the reached unloaded slot is Benilla’s existing conservative availability contract; it is not a claim that original CMaNGOS returns that error.

## Validation and memory limits

The complete source envelope is bounded to 4,096 tile products, 32,768 models, 512 MiB of serialized bytes and 64 MiB of tree/tile bytes. At most 1,048,576 placement references are scanned. The existing BIH element/depth limits remain. At most 64 explicit tiles may be selected. The resident decoder retains its existing 16 MiB serialized-input budget, placement/group/element limits and query-work budget. These are independent serialized and work limits, not a claim that process memory is 16 MiB.

All source bytes contribute to the binding. Unresident model geometry is hashed but deliberately not decoded or certified valid. Loading it later invokes the ordinary complete model, transform and enclosing-bound checks. The complete placement index is validated even when its geometry is not resident.

Cancellation is polled during source checks, row hashing, placement scans and metadata initialization. Existing envelope/BIH validation and the resident decoder are synchronous bounded operations; polling occurs around them. Metadata, the original assembled source bundle, raw capture and decoder allocations have separate bounded memory costs. This view does not eliminate the producer's full-map capture/assembly memory peak, and real continent sizing remains to be measured.

## Historical source identity

The binding commits the complete actual source products, source version, map and canonical tile selection in a distinct policy namespace. The view owns decoded resident geometry and borrows its immutable source bundle. Queries remain historical after files change. Explicit `verify_unchanged` detects replacement; any future live-use adapter must check freshness at its chosen admission boundary. A successful old query is not a guarantee about the current installation.

This view supplies collision queries only. It cannot convert into the complete terrain/navigation inventory and does not enable a normal game session, pathfinding or NPC simulation. Authored MPQ integration tests exercise capture, assembly, tile residency and the original query oracle; real installation and large-continent verification remain separate.
