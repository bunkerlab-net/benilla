# Captured source map queries

The captured-map integration test starts with an authored MPQ archive and calls
`prepare` and `assemble_map_bundle`. It then validates the resulting products
with the existing inventory and static geometry query implementation. This
checks a source-capture-to-query path without constructing a synthetic source
lease or weakening the consumer's checks.

## Authored source closure

The fixture supplies the minimal LiquidType and Map tables, a selected-map WDT,
a conventional root-WMO listing, one WMO root and group, and a selected ADT with
one MODF placement. All content is authored test data. No game assets are
included.

The original pinned WMO extractor produces the raw collision model. Its original
placement constructor and ID allocator produce the corresponding parent record.
The original map assembler and public map manager then supply complete expected
products and query results. These outputs are captured independently of the
Rust implementation.

The placement uses ordinary tiled WMO behavior: source-allocated ID 1, scale 1,
a nonzero position, and enclosing transformed bounds. The logical model name
is the natural plain `Hull.wmo` name. The asymmetric extractor tile `(30, 31)`
corresponds to public-manager tile `(31, 30)`.

## Consumer boundary

The test passes each returned model's exact typed identity into the inventory,
using its digest-based filename. Generated tree, tile and model bytes must match
the original source products before consumption. The loader and dry-map checks
run unchanged, followed by the original binary32 height, LOS and hit-point
expectations. Positive-hit assertions prevent missing geometry from passing as
an all-miss result.

Successful preparation and queries do not publish files or modify the input
archive. Source freshness is checked before use and tested after changes.
Cancellation or failed preparation does not produce a usable collision map.

## Scope

This is an authored ordinary-WMO integration test. The earlier generated-map
plain/terminal-NUL cases remain separate, as do the broader capture, extraction,
mutation and cancellation tests. The captured case does not establish that all
real client data is supported, that a whole game map has complete geometry, or
that global maps and every model type are supported through this path.

A successful test does not activate world startup, NPCs, quests or game-session
admission. Connecting validated complete content to normal gameplay remains a
separate implementation and verification step.
