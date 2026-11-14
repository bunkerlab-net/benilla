# Pinned producer policy

`prepare` retains `ProducerPolicy::CanonicalComponent`. `prepare_pinned` selects
`cmangos-8ec338a1-basename-fileexists-first-wins-v1` and requires an observed empty
output directory before and after its in-memory preparation. Neither function
publishes products or establishes gameplay admission. Existing limits remain
operational refusals, not complete-world inventory limits.

The pinned policy uses the exact, case-sensitive output basename as its cache
key. It never percent-decodes it or substitutes the canonical member path.
CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`,
`contrib/vmap_extractor/vmapextract/gameobject_extract.cpp:26-39` and
`vmapexport.cpp:91-99,160-167` check output files before looking up another member.
`model.cpp:49-73` emits no zero-hull file. A later emitting alias therefore wins,
and `model.cpp:169-184` opens the current output at placement time. Earlier map
rows seed that cache and generated IDs, `vmapexport.cpp:82-86,266-295`.

Pinned listfiles require a captured NUL. A final LF is insufficient because
`mpq_libmpq04.h:51-67` calls `strtok(NULL)` after every token. The typed refusal is
`vmap.listfile_undefined_suffix_read`. Canonical preparation keeps its separately
named logical-NUL rule. No undefined-byte equivalence is claimed.

Doodad names retain the original serialized length, including the NUL left by
`.mdx`/`.mdl` shortening, `model.cpp:241-253,299-300`. Model selection preserves
those raw bytes separately from the effective C-string output name,
`src/game/vmap/TileAssembler.cpp:298-329`. No geometry file is rewritten to make
an identity match. Existing explicitly named numerical projection profiles remain
component profiles, not proof of the target CPU/glibc 2.36 dispatch.

`source_occurrences` exposes original lookup ordinal, archive-scoped listfile
identity, probe count and encoded member identity. The pinned source digest also
binds archive index identities. The canonical digest remains unchanged.
`ProducerIdentity` streams whole-file hashes in exact
occurrence order, with distinct roles for archives, listfiles, members, tools,
libraries, numeric environment, arguments, initial output inventory, source-install
before/after receipts, config/offmesh bytes, generated IDs, BIH/model order,
transport models and products. Captured absence is distinct from an uncaptured
ordinal. Freshness checks reread those observations. These are diagnostic receipts,
not evidence that an arbitrary supplied receipt observed an earlier execution.

`producer_binding` adds exact raw product order/bytes and generated-ID records.
`AssembledSourceMap::product_identity` binds exact tree/BIH, tile and model output
bytes, raw names and C-string names in their actual order. These bind the data
presented to F6.03 without granting source residency or whole-prefix acceptance.
Completeness still requires an approved producer recording with all reached
occurrences and absence outcomes. A matching role list is not closure evidence.

The 48 historical products remain unchanged and unsealed. A new execution cannot
repair their absent producer lineage. No acquisition or reference recording is
authorized by this module. Private preparation changes no persisted native format.
