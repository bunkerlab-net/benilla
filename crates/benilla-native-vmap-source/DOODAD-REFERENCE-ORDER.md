# Bounded reference-container ordering proof

The pinned extractor inserts eligible MODR indices into a default
`std::unordered_set<uint16>`. Its iteration order affects doodad ordinals,
generated IDs, assembler primitive order and equal-hit query winners. A sorted
set is not an equivalent replacement.

The private proof module reproduces one explicit original implementation profile:
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

This checkpoint is test-only. Existing captured candidates remain diagnostic
membership, and no generated IDs, complete map products or admission are added.
The next integration must form the proven reference order from eligible group
MODR insertions before selected-set/raw-file exclusions. That typed order can
then drive the original ordinal/duplicate/assembler rules. Full-container tests
do not prove the later uint16 ordinal-wrap behavior of ExtractSet.

The reference-order oracle uses only authored keys and the installed standard
library. Licensing is retained in [GCC-RUNTIME-NOTICE.txt](GCC-RUNTIME-NOTICE.txt);
the combined source crate remains GPL-3.0-or-later.
