# Source encounter identities

The pinned extractor has one process-wide map from `(uint32 client ID, uint16
child ordinal)` to a generated uint32 ID. `GenerateUniqueObjectId` assigns
`distinct key count + 1` on the first encounter and returns the original value
on repeats. The ordering of its associative container is not observable at this
step; only encounter order matters. M2 and WMO parent records share ordinal zero.
The namespace is not reset between tiles or physical Map.dbc rows.

Capture replays this ledger across every preceding physical row and the selected
map. Prior-map placements contribute IDs without being retained in the selected
map output. Eligible parent records allocate first; their selected, ordered and
raw-collision-eligible children follow. A destructible or zero-collision WMO can
still contribute emitting children because the source invokes ExtractSet after
the parent constructor returns. Each ExtractSet invocation starts its own uint16
ordinal at zero, increments only for eligible children, and wraps after 65,535.
The 65,536th child therefore uses the parent key. This is preserved explicitly;
IDs are not widened or silently made unique.

`Placement::generated_id()` returns None for a non-emitting parent. Each
DoodadCandidate retains its source child ordinal and generated ID. These are
candidate identities, not complete assembled geometry. Repeated source IDs can
have differing transforms or models. The later assembler must preserve its
first-wins geometry and tile primitive mapping; the ledger does not deduplicate
or merge returned placements.

The explicit policy is `cmangos-client-u32-child-u16-first-encounter-v1`.
Projection binding version 3 includes this policy and writer version 5, together
with the reference-order policy, numerical policy, pinned source and full capture
epoch. Source epoch alone is not a policy identifier or a live filesystem lease.

The ledger admits at most 1,048,576 distinct keys and 4,194,304 encounter attempts
across the entire scanned prefix. A separate 16,777,216-step replay budget is
charged before eligibility filters for each placement and each child-reference
visit. Thus repeated non-emitting prefix inputs cannot evade the work bound.
Repeats count toward the attempt bound but not the
distinct-key bound. Allocation uses checked reservation; failures discard the
whole preparation. No prefix becomes a map product. Existing source read and
placement budgets also apply. Retained ledger entries are discarded when capture
finishes; the selected records and diagnostic counts remain.

Tests compare authored key sequences and all 65,536 child ordinals with the
[pinned original function](tools/test_encounter_ids.py). Capture tests cover
physical-row order, repeated parent and child IDs across tiles, destructible
parents, and the distinction between prior-map replay and selected output.
Filtered-prefix tests also prove pre-filter budget exhaustion and cancellation.
Original source SHA-256 is checked before compiling the isolated function; no
install data is used. Run the original-function comparison with CMANGOS_SOURCE
pointing at the pinned checkout. Full-world collision products, GameObject phases,
map BIHs and gameplay admission remain outside this ledger.
