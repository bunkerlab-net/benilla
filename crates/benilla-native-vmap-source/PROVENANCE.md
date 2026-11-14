# Source acquisition provenance

This GPL-3.0-or-later adaptation uses CMaNGOS Classic revision
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. Preserve
[the upstream notice](CMANGOS-NOTICE.txt) and [source hashes](SOURCE-SHA256SUMS).
No client-install assets are distributed. Regression MPQs are authored fixtures.

## Source-owned order

- `vmapexport.cpp` mounts terrain, model, texture, wmo and base archives, followed
  by existing numeric patch archives 1–99. `misc.MPQ` is formatted but not appended.
  `mpq_libmpq.cpp` searches reverse mount order; a first member of size at most one
  blocks fallback. Per-archive listfiles are read from that archive, not from the
  merged winner chain.
- `ReadLiquidTypeTableDBC` precedes `ExtractWmo`, which precedes physical Map.dbc
  iteration. The listfile pass selects the case-sensitive `.wmo` substring before
  normalization and root/group filtering. Global raw products remain private if
  any later prerequisite fails.
- A listed global WMO root can be absent or a winning zero/one-byte placeholder.
  `WMORoot::open` then fails, and `ExtractSingleWmo` returns true without creating
  a raw file at `vmapexport.cpp:190-194`. Acquisition retains the absence/dummy
  receipt and global attempt, emits no product, and does not cache an empty model.
  Repeated list entries retry the lookup. Any later MWMO reference still requires
  a captured product and otherwise refuses with `vmap.unresolved_wmo_reference`.
- `ParsMapFiles` gates on WDT availability, then attempts all 4096 ADTs with x outer
  and y inner. MAIN does not select the traversal. Earlier physical map records
  can seed the flat model cache and are consumed through the selected record.
- `adtfile.cpp` processes MMDX/MWMO/MDDF/MODF sequentially; direct name-array indices
  are used, not MMID/MWID indirection. MMDX extracts models; MWMO records names.
  Existing global WMO products are required for references.
- `wdtfile.cpp` has distinct basename normalization. Its nonempty MODF path includes
  global placements and doodads; this shape remains unsupported here.
- Raw client IDs and transforms are retained without asserting final assembled
  coordinates, global generated IDs, placement bounds, or map-clearance authority.

## Canonical component compatibility profile

The named `cmangos-vmap-listfile-logical-nul-v1` rule treats the exact captured
listfile end as a logical NUL. The original allocation has no extra terminator,
so otherwise `strtok` can read beyond it. This deterministic bounded rule preserves
LF token ordering, embedded-NUL termination, the source counter and its unconditional
last-character removal (including LF-only lines). It does not claim parity with
undefined reads beyond the original allocation. Exact captured bytes, rather than
an appended physical byte, remain in the source receipt.

Independent authored tests cover this rule. An attempted separate source-oracle
verification did not complete and is not included as evidence or distributed
fixtures. No original-source oracle verification is claimed for listfile parsing.

Other unsafe or ambiguous profiles are rejected rather than guessed: malformed
chunks/strings, unsupported headers, invalid indices/nonfinite geometry, unsafe
member names, empty ADT/WDT name-table entries, and unsupported WMO liquid/instance
profiles. Doodad extraction and selected-set
membership follow [their separate bounded contract](DOODADS.md).
Missing required models/groups are errors even where upstream would skip output.
An unavailable global root candidate is distinct from a missing declared group
or required M2. `vmapexport.cpp:225-230,260-263` deletes the whole partial WMO output
when a declared group cannot open. It does not emit an empty group.
`model.cpp:35-43` and `wmo.cpp:102-104` omit an unavailable M2 from valid doodad
names. They do not emit an empty M2. These required-dependency errors remain
atomic refusals in this bounded profile.
M2 and WMO products and references use the full ASCII-case-folded canonical
member path. Each model retains its own collision, even when another model
has the same basename. Placement and assembled-model names encode that path
reversibly, replacing `%` with `%25` and `\` with `%5c`. This keeps the flat-name
consumer contract without merging directory identities. The original extracted
basename remains separately available.
For M2, this is an owner-approved deviation from
`gameobject_extract.cpp:26-39` and `model.cpp:49-73`: an earlier basename output
can give a later no-collision model another member's hull. Benilla instead keeps
each model's own hull or genuine zero hull. The path key subsumes the earlier
zero-hull alias exception; there is no separate alias cache.
Name and metadata limits are unchanged. Ambiguous outputs still refuse, and
fresh-source replay rejects changed bytes at the same canonical path.

## Bounds and identity

The acquisition policy is
`cmangos-vmap-numeric-1-through-99-reverse-first-hit-v1`; the raw writer profile is
`benilla-vmaps05-canonical-model-path-v9`. Both and the listfile policy are
reported separately from the input digest. No summary or serialized digest can
mint a prepared object.

Limits include 104 archives, 4096 directory entries, 64 MiB aggregate archive
indices, 1 MiB header scan, 512-byte member names, 64 MiB per member, 32768 lookups,
and 32 GiB decoded read-work. Strict validation and payload reads each charge that
work budget; recheck has the same bound. Raw retained products are limited to
256 MiB, 32768 products/global root events, 262144 dependency events and 1048576
placements. Doodad logical metadata is bounded to 64 MiB and selected candidates
to 1048576 records. Existing raw-writer group/chunk/output limits also apply. Failure or
cancellation returns no partial preparation object.

The additive MPQ probing/bounded-open implementation is independently authored
from format and source behavior; no libmpq implementation text is copied into the
permissively licensed MPQ crate. Its source references are recorded for audit.
Rechecking proves observed equality of the ordered consumed inputs, not filesystem
transactionality, immunity to ABA changes, or a live source lease.

The source-bound WMO writer supports validated MLIQ and preserves its final-chunk
reset behavior. `liquid_header_policy` records the explicit unused-padding rule;
`wmo_liquid_groups` counts emitted active groups, excluding reset and skipped
groups. Every consumed group remains bound to the changed-input receipts.

## Pinned execution policy

The separately named [pinned producer policy](PINNED-PRODUCER.md) ports the source
basename and FileExists rules without changing canonical component callers. Its
listfile parser refuses uncaptured suffix reads instead of adding a terminator.
Exact product and dependency receipts remain preparation, not historical lineage
or gameplay admission.
