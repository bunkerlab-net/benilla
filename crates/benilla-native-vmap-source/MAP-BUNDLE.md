# Selected collision model bundle

`PreparedDiscovery::assemble_map_bundle` joins complete map/tree framing with
its selected model products in memory. It returns no bundle for an empty source
placement stream. It retains a borrow of the prepared source capture and checks
that capture again after model conversion. `verify_unchanged` checks freshness
again when a caller needs it; the returned object is not a lasting filesystem
lease or gameplay admission token.

## Selection and names

The selected set contains the exact names referenced by first-wins map spawns,
including global records. It does not contain every model discovered in an
archive. Exact unsigned byte ordering and deduplication reproduce the source's
`std::set<std::string>` order. A single terminal NUL remains part of identity.
Empty names, embedded NULs and names longer than 500 bytes are rejected.

Raw lookup removes that optional terminal NUL. Effective source output naming
is recorded separately: `Foo.m2` becomes `Foo.m2.vmo`, while `Foo.m2\0` becomes
`Foo.m2`. The original writer and original consumer both truncate the latter
at `fopen`'s C-string boundary. The original map-product oracle verifies those
writer and lookup behaviors. This implementation does not call a filesystem API
with either name. It allows printable ASCII basenames only, rejects path
separators, colons and dot paths, and rejects duplicate effective output names.
A future publisher must provide a reviewed internal mapping and collision policy.
These metadata names are not validated runtime product identifiers.

## Bounds and failure behavior

- At most 32,768 unique names, borrowed during selection
- A bounded radix sort: at most 500 passes, cancellation every 1,024 scalar steps
- At most 32 MiB of owned name metadata
- At most 256 MiB of retained serialized model bytes
- At most 512 MiB of retained model vector capacity, checked after each conversion
- At most 131,072 model-writer cancellation polls across the selected set
- Each model retains the writer's independent 128 MiB input/output and element limits

The model byte total is checked after each bounded individual conversion. A
failed conversion can temporarily allocate that individual model's output in
addition to previously retained models: its output vector capacity is capped at
128 MiB. Reallocation may temporarily retain both old and new storage (a
conservative additional 256 MiB envelope), plus independently bounded BIH and
parser scratch.
The 512 MiB retained-capacity guard does not include that temporary writer scratch.
Standard container reservation and hash
operations are bounded but are not interruptible internally. All reservations
are fallible. Failure discards the whole bundle; no partial product is returned.
Source capture data, map products and their separate bounds remain additional
memory costs. No archive data is published by this API.

## Verification and current scope

The authored capture integration checks exact selected-name ordering, raw/model
associations, map byte identity, exact byte-budget success, budget failures and
final source-change rejection. Name-selection tests compare with an independent
ordered set and exercise the maximum envelope and cancellation positions.

The production geometry harness now consumes freshly assembled M2, WMO and
liquid model bytes, comparing them first with independent complete products
written by the pinned original CMaNGOS implementation. Its existing query
expectations remain independently generated. Those tests exercise real runtime
consumer/query code with controlled content. They do not start a normal game
session or establish that arbitrary installed assets can populate the world.

The runtime consumer still has a narrower model-name and exterior-map envelope.
Connecting safe name mapping, global-map loading, complete inventory validation
and normal session startup remains separate work. The bundle does not alter
those checks, export a complete map to disk, or bypass content validation.
