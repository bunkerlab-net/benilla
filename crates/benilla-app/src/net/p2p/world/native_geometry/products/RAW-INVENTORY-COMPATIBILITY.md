# Raw inventory consumed-source compatibility

The existing client inventory load retains a private bounded receipt for each
actual Archive handle/read: requested member, winning archive basename, consumed
length and SHA-256 of the full buffer. The client policy is explicit. No extra
members are read and no raw buffers or caller-authored manifest are exposed.
Private callback inventories do not possess this genuine-load provenance.

An immutable allocation identity stays with that exact inventory when it moves
into NativeStaticMapIndex. The old raw inventory digest is unchanged and does not
include winners. A compatibility observation retains the genuine provenance owner
and a separate digest. A separately loaded equal-content inventory is a different
owner; equality of old raw digests cannot authorize index replacement.

PreparedSourceHeight compares every retained dependency against its actual source
VMap receipts, preserving exact winning basename, full raw hash and length. Source
paths use bounded ASCII case/slash lookup keys, but distinct spellings that collide
inside the same source receipt set are rejected. This is hash-and-length equality,
not literal cross-consumer byte comparison. The same successful private inventory
parser/load minted every root/group/MODD receipt and built the WMO aggregates;
this bridge does not reconstruct those aggregate formulas from a detached manifest.

The exact 4096 raw ADT coordinates must partition into WDT-present inventory tiles
with matching source Present digests, and all other coordinates with explicit
source Absent outcomes. Extra Present or Dummy ADTs fail. Selected generated
terrain coordinates use the established raw X/Y swap. Shared terrain/collision
receipt compatibility and both source epochs remain bound by the actual source
height object. Extra prefix-map and global discovery history remains owned by its
source capture, rather than being reinterpreted as renderer dependencies.

Source unchanged checks bracket construction and repeat when checking the current
owner. Deadline and cancellation checks bound metadata traversal and surround the
existing source verification. These are sequential observations, not an atomic
filesystem snapshot, hostile-ABA guarantee or a live-install lease. No successful
clearance may be retained across a future physics frame or staged yield.

The caller must obtain current provenance from the actually installed static
index. The metadata API itself cannot establish installation or physics freshness.
MapCatalog and renderer assets currently lack consumed-winner receipts; no such
origin claim is made here. Current catalog directory agreement and raw asset
content checks remain separate. No Map.dbc reread, importer or renderer acquisition
is introduced.

This compatibility object cannot become a Route, cache or commit. Concrete raw
3×3 coverage, continuous flat/dry support, live collider membership, body sweep,
segment LOS, source-ordered route ownership and atomic acknowledgement remain
mandatory separate requirements. Unsupported source/parser cases stay refused.
