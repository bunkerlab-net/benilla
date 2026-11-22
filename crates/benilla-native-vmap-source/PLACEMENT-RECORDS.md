# Selected-map placement records

`PreparedDiscovery::placement_records` produces the selected map's source-shaped
`dir_bin` records in memory. It requires an explicit doodad numerical profile.
Each emitting parent precedes its eligible children; omitted parents do not omit
otherwise eligible children. Candidate IDs come from the global prefix ledger,
so earlier maps and duplicate raw identities retain their source effects.

M2 records preserve position permutation `[z,x,y]`, raw rotations, uint16 scale
divided by binary32 1024, zero nameSet and the final normalized basename length.
WMO records preserve the source zero-origin substitution, position and bounds
permutations, raw rotations, scale one and MODF nameSet. Doodad records retain the
existing profiled G3D values and pre-extension-shortening name length, including
the source's trailing NUL. Flags and the 18-byte pre-ID header follow the pinned
writers. Repeated IDs and records are preserved for the assembler's later
first-wins and tile-reference handling.

The output has a 128 MiB logical byte limit, checked additions and fallible
reservation. Explicit growth requests and accepted vector capacity are capped at
128 MiB; allocator bookkeeping and temporary projection buffers are additional.
No claim is made that 128 MiB bounds the whole process. Cancellation is
checked between parent/child records, during the final source recheck and while
hashing output in 64 KiB chunks. Any failure drops the partial buffer. The API
never writes into the install or publishes an output directory. Returned bytes
contain model basenames; the Debug representation exposes only counts.

The binding includes `cmangos-selected-dir-records-v1`, the source revision,
acquisition/listfile/liquid/writer/encounter/reference/numerical policies, captured
source epoch and complete output bytes. It is a preprocessing identity, not a
live source lease. A filesystem change after the final recheck remains possible;
a later publisher must establish its own publication boundary.

Tests byte-compare complete parent+child output against 17 original CMaNGOS
fixture cases, including omitted parents, zero origins, composed rotations and
zero/negative scales. The authored MPQ uses the original fixture's parent name,
so expected records are not patched. Separate M2 field tests cover coordinate
and signed-zero preservation, maximum uint16 scale and duplicate IDs. They do
not independently execute the original ModelInstance writer. Budget overflow,
cancellation, changed-input rejection and absence of filesystem writes are also
checked.

This stream is input for a future assembler. Complete raw-model conversion,
transformed M2 bounds, map and model BIHs, duplicate first-wins geometry, tile
primitive slots, queries against original multi-instance output and atomic
filesystem publication remain required. It does not make the world populated or
establish collision-clear space, navigation, a live lease or gameplay admission.
