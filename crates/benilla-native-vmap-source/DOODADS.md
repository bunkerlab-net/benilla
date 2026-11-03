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

Group framing follows the raw writer's fixed 68-byte `MOGP` header and flat
chunk traversal. Only final `MONR`, `MOTV`, and `MOCV` payloads may end short at
EOF. The first two have empty source branches at `wmo.cpp:232-237`; `MOCV`
reaches the generic no-read path. The seek stops traversal without reading the
payload at `wmo.cpp:268` and `mpq_libmpq.cpp:116-120`. Such a tail contributes
no references or collision data. Consumed payloads, including `MODR`, and all
root chunks remain fully bounded. Unknown or misaligned short tags refuse.

For each selected-map WMO placement, the public read-only candidate list intersects
that membership with exactly its selected MODS interval. An out-of-range set yields
no candidates. There is no automatic union with a default set. The parent WMO's
destructible or zero-collision disposition does not suppress selection: upstream
calls ExtractSet after those early-return constructors too.

## Portable and bounded profile

An owner-approved deviation resolves each doodad lookup to the captured M2
product whose basename matches after the source's `fixnamen` and `fixname2`
normalisation. This includes ASCII case changes and stem spaces changed to
underscores. The original captured member and emitted basename remain unchanged;
no normalised archive member or filesystem alias is opened.
Different canonical members sharing a normalised output name still fail with
`vmap.flat_name_collision`, including equal bytes, different bytes, and different
collision dispositions. Existing missing-input, identity, and source-replacement
refusals remain.

Member paths accept printable ASCII punctuation, including ampersands in directory
components. `MPQFile` passes names directly to `libmpq__file_number`, whose hashes
uppercase each byte without a punctuation whitelist:
`mpq_libmpq.cpp:69-89`, `dep/libmpq/libmpq/mpq.c:483-495`, and
`dep/libmpq/libmpq/common.c:53-56`. Backslashes remain the only directory separator.
Spaces remain archive-name bytes; only the existing `fixname2` call sites rewrite
basename spaces. The portable profile still rejects control/non-ASCII bytes and
`/ : * ? " < > |`, which cannot safely name extracted flat outputs on Windows.
The 511-byte limit, empty/dot/traversal-component restrictions, and leading or
trailing component-space/dot restrictions are unchanged.

A successfully opened M2 with zero `nBoundingTriangles` remains an explicit
no-hull product. `Model::open` returns false for that normal disposition at
`model.cpp:49-71`; it is not a missing member or unsupported name.

In `vmapextract/wmo.cpp:89-103`, both retained Paths and the local extraction path
precede the `fixnamen`/`fixname2` mutation of the MPQ buffer. In
`gameobject_extract.cpp:14-33`, `.MDX` becomes `.M2`, and `GetPlainName` preserves
the original basename's case. In `model.cpp:241-260`, ExtractSet instead applies
`fixnamen`, then `fixname2`, then the lowercase legacy-extension conversion.
For an authored `AUTHOREDXX01.MDX`, extraction therefore emits `AUTHOREDXX01.M2`,
but placement opens `Authoredxx01.m2`. A case-sensitive filesystem can skip that
doodad when `fopen` fails. ADT MMDX extraction can create the lookup spelling
later because `adtfile.cpp:150-156` normalizes before ExtractSingleModel.
Benilla differs because source collision membership otherwise depends on
filesystem case rules and extraction order.
For a basename with a stem space, extraction retains the space while lookup
uses an underscore. Ordinary Windows filesystems also distinguish those names.
Without another earlier flat-file producer, the pinned source skips that doodad
on Windows too. The same captured-member rule resolves both shapes and refuses
competing members instead of letting extraction order select the geometry.

Normalised aliases retain the exact captured product and use its emitted basename in
placement records and assembled models. Runtime never needs a filesystem alias:
`MapTree.cpp:310,399` passes `spawn.name` to `VMapManager2.cpp:310-321`, which reads
`basepath + filename + ".vmo"` without normalization.

Missing required model/group inputs fail the complete preparation atomically.
Malformed records, nonfinite values and overflowing set ranges are rejected rather
than reproducing unsafe source accesses or unsigned wrapping. Group MLIQ framing is ignored by the doodad-only parser after the prior raw
writer has validated every liquid chunk. MLIQ in a root remains invalid.

Metadata has a cumulative 64 MiB logical retained-record budget; selected doodad
candidates have a 1,048,576-record limit. Existing member, group, dependency-event,
raw-output and lookup limits also apply. Cancellation and final changed-input
verification cover the expanded dependency reads. No output is published on failure.
Public diagnostic summaries expose only counts/categories and policy labels; raw
asset paths/set-name bytes are excluded from their serializers.

The writer policy is `cmangos-vmaps05-encounter-ledger-normalized-doodad-v7`.
This revision captures dependencies and selected membership, not final placement.

## Remaining assembly obligations

Captured candidates follow the explicitly named [reference-container profile](DOODAD-REFERENCE-ORDER.md),
including its retained-group insertion and rehash history before later set/raw-file
filters. Upstream increments uint16 doodad ordinals only after those filters;
the [encounter ledger](ENCOUNTER-IDS.md) preserves wrapping and global generated
IDs. Assembler duplicate first-wins geometry remains unimplemented.

Final placement also needs the exact parent early-return/zero-origin/permutation
state, G3D quaternion/matrix/Euler arithmetic and float rounding. No final position,
rotation or map-wide collision authority is produced by dependency capture. The source
records its basename length before extension shortening, so eventual dir_bin output
must retain that trailing-NUL name serialization detail. Full map BIHs and the later
GameObjectDisplayInfo phase remain separate prerequisites.

Tests use authored MPQ/model/WMO bytes. Real-install WMO acquisition has not been
established. Independent original-source projection and container oracles cover
their named profiles, but do not prove complete multi-instance map assembly. The separate blocked
listfile-oracle work is neither retried nor used by this implementation.

Captured candidates also retain the original pre-extension basename length for
[source-shaped, explicitly profiled projection](DOODAD-PROJECTION.md). The projection binding
includes the reference-order and writer policies; it is not a complete map witness.
