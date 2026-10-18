# WMO doodad dependency closure

Pinned source: CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`,
`WMORoot::open`, `ExtractSingleModel`, `ExtractSingleWmo` and
`Doodad::ExtractSet`. See [source hashes](SOURCE-SHA256SUMS) and
[the acquisition boundary](PROVENANCE.md).

Root MODN processing captures every model dependency in source chunk/string order,
including unreferenced names. Paths are copied before the source mutates the MPQ
buffer's basename, so extraction uses original spelling with only the four exact
`.mdx`/`.MDX`/`.mdl`/`.MDL` extension conversions. Empty NUL entries preserve their
byte offsets and perform no model read. No-hull models create no file and are read
again on a later attempt. Existing emitted files produce successful extraction
without another member read, under the acquisition library's collision restrictions.

Repeated MODN chunks replace the retained Paths while retaining every extraction
event and accumulating successful name offsets. MODS, MODD and MODR replace arrays.
MOHD doodad counts do not select these loops. The parser requires whole 32-byte
MODS, 40-byte MODD and two-byte MODR records; MODD name indices use the low 24 bits,
with the ignored high byte retained. Position, XYZW quaternion, scale and color
remain raw. Finite values and checked MODS ranges form an explicit safety envelope.

Every root-declared group is captured. The existing raw writer returns the exact
filename indices surviving its validated ShouldSkip pass; the acquisition layer
uses that result rather than implementing a second skip predicate. Retained groups
contribute MODR even when their converted collision triangle count is zero.
References outside the spawn array or without successful name offsets are ignored;
remaining references are deduplicated. Flat-file existence and nonzero raw-count
eligibility are resolved at each selected placement, after later global roots may
have populated the cache. Final name dereferences occur only after selected-set filtering and are checked
against the last retained Paths, including source-used interior suffix offsets.

For each selected-map WMO placement, the public read-only candidate list intersects
that membership with exactly its selected MODS interval. An out-of-range set yields
no candidates. There is no automatic union with a default set. The parent WMO's
destructible or zero-collision disposition does not suppress selection: upstream
calls ExtractSet after those early-return constructors too.

## Portable and bounded profile

Successful MODN extraction must emit a basename byte-equal to the later normalized
lookup basename. A mismatch fails explicitly, avoiding different Windows/Linux
filesystem case rules. Existing distinct-member flat-name collision refusals remain.
Missing required model/group inputs fail the complete preparation atomically.
Malformed records, nonfinite values and overflowing set ranges are rejected rather
than reproducing unsafe source accesses or unsigned wrapping. Liquids remain outside
the raw writer and parser profile.

Metadata has a cumulative 64 MiB logical retained-record budget; selected doodad
candidates have a 1,048,576-record limit. Existing member, group, dependency-event,
raw-output and lookup limits also apply. Cancellation and final changed-input
verification cover the expanded dependency reads. No output is published on failure.
Public diagnostic summaries expose only counts/categories and policy labels; raw
asset paths/set-name bytes are excluded from their serializers.

The writer policy is `cmangos-vmaps05-small-dry-doodad-dependencies-v2`.
This revision captures dependencies and selected membership, not final placement.

## Remaining assembly obligations

Candidate ordering is a deterministic presentation only. Upstream iterates a
`std::unordered_set<uint16>` and increments doodad IDs after raw-file eligibility;
its implementation-dependent iteration affects the global generated IDs. Sorting
for inspection does not certify that emission order.

Final placement also needs the exact parent early-return/zero-origin/permutation
state, G3D quaternion/matrix/Euler arithmetic and float rounding. No final position,
rotation, generated ID or map-wide collision authority is produced here. The source
records its basename length before extension shortening, so eventual dir_bin output
must retain that trailing-NUL name serialization detail. Full map BIHs and the later
GameObjectDisplayInfo phase remain separate prerequisites.

Tests use authored MPQ/model/WMO bytes. Real-install WMO acquisition and an independent
original-source doodad oracle have not been established. The separate blocked
listfile-oracle work is neither retried nor used by this implementation.
