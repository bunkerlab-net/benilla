# Bounded reference-container ordering

The pinned extractor inserts eligible MODR indices into a default
`std::unordered_set<uint16>`. Its iteration order affects doodad ordinals,
generated IDs, assembler primitive order and equal-hit query winners. A sorted
set is not an equivalent replacement.

The private ordering module reproduces one explicit original implementation profile:
GCC 14 / libstdc++ 20250315, identity integer hash, default maximum load factor,
fresh empty construction/swap and insertion only. It is not a generic C++
unordered container or a promise about other standard libraries. The compiler,
headers and linked library are pinned by the independent oracle's hashes.

The out-of-line prime-growth implementation is observed across the entire 65,536
key universe. Its cardinality transitions are retained as profile data, not
replaced by a guessed prime rule. Global-list and bucket-predecessor operations
follow the original headers. Duplicate rejection does not grow the table. A
same-name root replacement starts fresh, including capacity; it is not clear()
on an existing container.

The [authored oracle](../benilla-native-geometry-tests/tools/wmo_doodad_order_README.md)
covers 753 cases and 7,901 literal checkpoints: collisions, duplicate passes, growth boundaries,
small exhaustive permutations, deterministic shuffled histories and four full
key permutations. It checks bucket count at every distinct cardinality and
records bounded literal iteration checkpoints. The maximum observed bucket
count is 85,229. Peak live logical vector/bitmap payload is 779,424 bytes during the last
rehash (allocator overhead is separate), with an 8KiB membership bitmap and
key-indexed links. Fresh construction initializes 65,536 key links even for an
empty set. Across a complete domain the fixed schedule visits 82,648 old nodes
and initializes 167,878 bucket slots; work is linear in this bounded setup plus
input attempts, rehash work and final members. Cancellation poisons any partially mutated state;
no partial ordering is returned.

Captured WMO references now use this profile. Insertions occur in retained-group
and MODR record order, after spawn-index and successful-name eligibility checks.
Selected MODS ranges and later raw-file/collision exclusions are applied only
after enumeration. This distinction matters: excluded keys can change a rehash
and the relative order of the remaining keys. Authored MPQ integration tests
cover duplicate references, retained zero-triangle groups, early extraction
failures and late raw-file exclusions across a growth boundary.

The summary names `doodad_reference_order_policy`; the writer policy is
`cmangos-vmaps05-encounter-ledger-v5`. Projection binding version 3
includes both identifiers and the encounter-ID policy, along with the numerical policy, source pin and capture
epoch. A candidate index is still not its generated object ID. The separate
[encounter ledger](ENCOUNTER-IDS.md) assigns source IDs and preserves uint16
child wrap. Complete map products and assembler duplicate first-wins geometry
remain separate from the full-domain reference-container proof.

The reference-order oracle uses only authored keys and the installed standard
library. Licensing is retained in [GCC-RUNTIME-NOTICE.txt](GCC-RUNTIME-NOTICE.txt);
the combined source crate remains GPL-3.0-or-later.
