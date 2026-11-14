# Exact source model identities

`SourceModelName` keeps the complete validated source name as its identity,
including an optional terminal NUL. Names are nonempty printable ASCII basenames,
with at most 500 total bytes. Embedded NULs, separators, colons and the basenames
`.` and `..` are rejected. Case, punctuation and terminal NULs are not normalized.

`safe_basename` produces `source-<64 lowercase hexadecimal digits>.vmo`. SHA-256
covers the domain `benilla-source-model-name-v1\0`, the complete byte length as
little-endian u32, and the complete source name. Only this digest-based basename
is suitable for an inventory-managed output path. Raw and effective source names
remain metadata; they must never be passed to filesystem APIs.

## Inventory and runtime integration

The production geometry inventory has an explicit `VmapSourceModel` identity
variant. Legacy `VmapModel` filenames retain their existing validation and lookup.
An inventory cannot mix these naming profiles. This stricter boundary avoids
ambiguity between a legacy filename and an exact source name for the same spawn.

Source-name inventories match tile references and declared dependencies by the
full typed identity. Runtime model indices use those same identities. Plain and
terminal-NUL names remain distinct even when they came from the same raw model.
Case-insensitive safe filename collisions and identical effective source outputs
are rejected before reads. Missing, extra and unreferenced dependencies retain
the existing rejection behavior. Geometry, size, work and exterior-map checks
remain in force.

The inventory revision has a source-identity domain marker and binds complete
source-name bytes for every product and dependency edge. Wholly legacy inventory
revisions retain their previous encoding. Source-aware inventories therefore have
new content identities and cannot be substituted for a legacy inventory revision.
The product specification is currently an internal Rust structure, not a newly
accepted network message or a serialized configuration format. No peer handshake
or save format is changed here.

This mapping permits source-compatible names without weakening filesystem path
checks. It does not itself publish products, load global-map records, or admit a
normal game session. Captured map bundles still need explicit inventory assembly,
complete-map validation and a reviewed publication path.
