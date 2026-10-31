# Exact loaded map classification

Pinned CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`
`DBCStructure.h::MapEntry`, `DBCfmt.h::MapEntryfmt`, and `DBCEnums.h` define
Map.dbc ID column 0 and type column 2 in a 42-word row. The source kinds are
common 0, instance 1, raid 2 and battleground 3. `IsBattleGround` is exactly
kind 3; `Instanceable` is exactly kinds 1, 2 or 3. A false instanceability result
alone is not a proof of non-BG for an unknown type. `IsContinent` instead tests
map ID 0/1; this owner does not confuse that predicate with common type.

## Loader and licensing boundary

The existing public `MapCatalog` loader retains optional exact full-width raw
rows from the same bytes it already reads. Structural coverage requires the
42-field/168-byte record layout and the complete declared record/string extent;
missing or malformed raw coverage is not a partial source table. Full values,
row order and duplicate IDs are preserved. Its existing UI map lookup behavior
is unchanged. This generic schema transport contains no new GPL classification
policy or native admission and does not touch the held private compiler path.

The new source-derived selector lives in the existing GPL simulation crate.
It checks complete raw coverage, refuses any duplicate map ID in the loaded
table, requires the selected ID, and refuses an out-of-domain selected type.
An unrelated unique unknown type is not used as evidence for the selected map.
It retains the selected full row in an opaque Serialize-only selection; there
is no public scalar/tuple proof constructor or Deserialize implementation.
Source attribution remains GPL-2.0-or-later, Rust translation GPL-3.0-or-later;
no crate or repository license is changed.

## Native integration and limits

The optional immutable native profile source binds that selection to the actual
profile map ID. Different reinitialization is refused atomically. Its selected
row participates in the profile digest, so checkpoints/replay use the exact
profile owner; no new mutable state or caller-provided `not_bg` boolean exists.
It is evidence for the selected classification only, not a whole-install hash,
geometry validity, native content capability or proof of all map callbacks.

When this owner exists, existing and newly configured ordinary player death
must match the source instanceability predicate and must not be BG. The checks
run before configuration mutation and during profile validation. Missing,
duplicate or unknown source data never defaults to common/non-BG. Profiles
without this optional owner retain their previous explicitly declared death
support contract, but that declaration is not promoted to raw map evidence.
Classifying a BG row does not implement BG gameplay or allow BG death.

This closes one input needed by a future ordinary corpse capture; it does not
capture, allocate, create, place or publish a corpse, execute ghost spells,
accept spirit release, or relax first-life restore. Native format `NATIVE_FORMAT_VERSION` identifies
the profile source shape and rejects earlier identities without migration.
Production admission and private source/compiler holds remain unchanged.

Evidence is separated: formats tests exercise actual authored MPQ/Chain loading;
simulation tests exercise source predicates and conservative selection; runtime
and controller tests exercise exact owner/configuration, real ticks and durable
checkpoint failure/retry/reopen. Authored rows are not a captured player install.
