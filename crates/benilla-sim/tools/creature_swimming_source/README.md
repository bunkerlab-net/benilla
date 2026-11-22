# Exact-source swimming initialization primitives

The retained `.inc` files are unchanged LF-normalized excerpts from
[CMaNGOS Classic revision 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f](https://github.com/cmangos/mangos-classic/tree/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f):

- `GridMap.cpp:797–803,1147–1161`: complete CanCheckLiquidLevel and IsSwimmable
- `GridMap.h:154,157`: original declarations and default arguments
- `GridMapDefines.h:56–83`: status, liquid masks and data structure
- `Creature.h:64,403–409`: WALK_IN_WATER and habitat constants
- `Object.h:701–737,761–765`: movement constants and exact flag accessors
- `Creature.cpp:438–442,575–577`: initialization swimming branch and unit projection
- `Unit.h:481`: UNIT_FLAG_SWIMMING constant

Original GPL-2.0-or-later notices remain in every excerpt from a file with a
notice. `GridMapDefines.h` has no original header notice to copy; its unchanged
excerpt remains covered by the upstream project license. Existing
[provenance](../../PROVENANCE.md) and the complete
[GPLv3 license](../../licenses/GPL-3.0.txt) apply to this combined crate. The
minimal host, generator, tests and Rust translation are GPL-3.0-or-later,
Copyright 2026 Benilla fork contributors. No client assets, database rows or
proprietary data are included.

The independently hash-pinned [manifest](source_manifest.json) includes complete
source hashes, excerpt body hashes and retained byte hashes. Only full upstream
checkout bytes are CRLF-normalized. `--upstream` also verifies each retained body
and notice against the pinned source and checks the exact model/gender/native
model/liquid/SelectLevel ordering in the complete Creature.cpp. This is a
source-order structural check, not execution of the full initializer. Complete
Creature.cpp, Unit.cpp and MoveSplineInit.cpp hashes pin the separate lifecycle
review; this small oracle does not execute death, respawn or spline callbacks.

`oracle.cpp` supplies only typed stores, injected authored liquid observations,
query instrumentation and the surrounding scopes needed by the unchanged source
bodies. It does not restate the source swimming, availability, initialization or
unit-projection expressions. The liquid query hook checks exact forwarded
coordinates, captures mask and classification-height bits, and supplies an
optional external data object. It is not an implementation of getLiquidStatus,
actual byte-derived geometry or a runtime provider.

The 2,432 `S` cases cover every status word 0..15, all four combinations of VMap
height-enabled/GridMap-present, internal/external result storage, strict depth
threshold equality and adjacent binary32 values, differing radius and default
classification height, signed zero, negative source radii, cancellation, rounded
subtraction and an exact subnormal intermediate. The 2,648 `I` cases execute the
unchanged InitEntry branch and UpdateEntry projection block across habitats 1..7,
ExtraFlags short-circuits, prior SWIMMING, unrelated movement bits and their
complements, independent unit flags, unavailable queries and nonzero statuses.
The real source methods permit broader inputs than the bounded runtime owner;
these probes do not grant runtime admission for a wet or unavailable result.

Fixture fields are space-separated decimal integers. Float values use raw
binary32 encodings:

- `S vmap grid status level_bits depth_bits radius_bits external_data`
- `I habitat extra prior_movement base_unit vmap grid status level_bits depth_bits radius_bits external_data`

Both append:

`result projected_unit availability_calls grid_calls liquid_calls mask classification_bits external_level_bits external_depth_bits`

`S.result` is bool. `I.result` is the final movement word. The external-data
sentinels remain unchanged when not supplied or when no positive liquid result
writes them. The generated cases cover source-valid status bit combinations;
Rust separately checks raw nonzero truth conversion without constructing an
invalid C++ enum value.

From the repository root:

```
python3 crates/benilla-sim/tools/generate_creature_swimming_fixture.py --check --sanitize --upstream /path/to/pinned/mangos-classic
python3 crates/benilla-sim/tools/test_creature_swimming_oracle.py
cargo test -p benilla-sim --test creature_swimming_oracle
```

Missing compilers, changed hashes, changed excerpts and fixture drift fail;
none silently skip. Omit `--check` only to regenerate the authored fixture.
The generator does not run a raw-content compiler or query a database.
